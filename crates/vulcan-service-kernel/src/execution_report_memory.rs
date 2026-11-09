//! Terminal execution report memory application for WebUI sessions.
//! WebUI 会话终态执行报告内存应用逻辑。

use vulcan_agent_runtime::{
    ExecutionPromptPurpose, ExecutionRunReport,
    agent_runtime::conversation_hooks::{
        ConversationCompletionRequest, ConversationTurnCompletionStatus, VmmWritebackHookEffect,
    },
    conversation_protocol::SessionConversationState,
};
use vulcan_service_api::{
    ConversationRunOutcome, ConversationSummaryChangedDelta, ServiceEventEmitter,
};
use vulcan_workbench_model::{SessionId, SessionRunPhase, SessionRuntimeStateEntry, SessionState};

use super::{
    AgentRuntimeBridge, ContextUsageChangedDelta, ServiceBridgeState, ServiceEvent,
    SharedServiceBridgeState, UpdateConversationRunPhaseDelta, WorkbenchDelta,
    WorkbenchDeltaEnvelope, active_session_summary_for_session_mut,
    apply_vmm_writeback_effects_to_runtime_state, build_terminal_execution_report_state,
    context_usage_snapshot_from_runtime_state, conversation_attention_summary_for_session,
    current_unix_millis_text, current_unix_millis_value, emit_service_event_with_trace,
    owning_project_id_for_session, pending_question_snapshots_from_agent_runtime,
    refresh_exact_vmm_hook_session_projection, session_context_usage_facts_from_v1_registry,
    session_plan_mode_summary_from_runtime_state, session_public_ai_model_capabilities,
    spawn_service_task, sync_hydrated_session_dialogue_ready_marker, webui_trace,
    webui_trace_if_slow, webui_turn_completion_gate_from_state,
    workbench_surface_matches_conversation, workspace_session_summary_for_session,
    workspace_snapshot_contains_regular_session,
};
use crate::vmm_session_controller::open_registered_session_vmm_controller;
use crate::{HostRuntimeHandle, webui_vmm_runtime_readiness};
use vulcan_workspace_store::workspace_store::set_session_unread_v1;

/// Mirrors the in-memory unread flag of one main session into its durable session index row.
/// 将单个主会话的内存未读标记镜像写入其持久会话索引行。
///
/// # Parameters
/// 参数
/// `state` owns the retained runtime flag; `controller_runtime` owns the session index.
/// `state` 持有保留的运行态标记；`controller_runtime` 持有会话索引。
/// `session_id` identifies the indexed main session whose terminal report was just applied.
/// `session_id` 标识刚应用终态报告的已索引主会话。
///
/// # Returns
/// 返回值
/// Success once the index matches the latest in-memory flag, or the storage error.
/// 索引与最新内存标记一致后返回成功，否则返回存储错误。
pub(super) async fn persist_main_session_unread(
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    // A concurrent mark-read may change the flag during the write, so the latest value is written
    // until the persisted value equals the value observed after the write.
    // 并发的标记已读可能在写入期间改变标记，因此持续写入最新值，直到写入后观察到的值与已写入值一致。
    let mut persisted = None;
    loop {
        let current = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .is_some_and(|runtime_state| runtime_state.ui_state.unread);
        if persisted == Some(current) {
            return Ok(());
        }
        set_session_unread_v1(controller_runtime, &session_id.to_string(), current)
            .await
            .map_err(|error| format!("Failed to persist the session unread marker: {error}"))?;
        persisted = Some(current);
    }
}

/// Applies only the auxiliary checkpoint to existing session memory; V1 owns outcome and usage.
/// 仅将辅助检查点应用到既有会话内存；结果与用量由 V1 拥有。
/// `state` supplies the prepared adapter and `report` its exact session; errors leave the adapter unchanged.
/// `state` 提供已准备适配器，`report` 指定其精确会话；错误保持适配器不变。
/// Returns after atomic memory publication without phase changes, business hooks or completion notifications.
/// 原子发布内存后返回，不修改阶段、不执行业务 Hook、不发送完成通知。
pub(crate) fn apply_question_execution_report_to_memory(
    state: &SharedServiceBridgeState,
    report: &ExecutionRunReport,
) -> Result<(), String> {
    // The physical report session is the sole adapter key; no parent-session substitution is allowed.
    // 物理报告会话是唯一适配器键，不允许替换为父会话。
    let session_id = report.session_id;
    // Retain the service lock only for the local validated publication, never for durable I/O.
    // 仅在本地已验证发布期间持有服务锁，不跨可靠 I/O 持有。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // The adapter must have been installed by preparation before a provider stream can finish.
    // 供应商流结束前，准备阶段必须已安装适配器。
    let workspace = guard
        .workspace_snapshot
        .as_mut()
        .ok_or("The workspace adapter is unavailable.")?;
    // Read the latest state so concurrent non-execution UI changes are retained by the draft.
    // 读取最新状态，使草稿保留并发的非执行 UI 变化。
    let source = workspace
        .session_runtime_state
        .get(&session_id)
        .ok_or("The question execution has no prepared session adapter.")?;
    // Only the shared QA reducer may construct this projection; the ordinary terminal builder is excluded.
    // 只有共享答疑归约器可构建该投影，排除普通终态构建器。
    let projected =
        vulcan_agent_runtime::session_event_runtime::build_question_execution_report_state(
            source,
            session_id,
            &current_unix_millis_text(),
            report,
        )?;
    // Keep the child detail cache as a mirror of the same adapter, not another task result.
    // 子详情缓存保持为相同适配器的镜像，而非另一份任务结果。
    let conversation_state = projected.conversation_state.clone();
    workspace
        .session_runtime_state
        .insert(session_id, projected);
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    if let Some(entry) = guard.subagent_surface_detail_cache.get_mut(&session_id) {
        entry.task_state.conversation_state = Some(conversation_state);
        entry.last_access_ms = current_unix_millis_value();
    }
    Ok(())
}

/// Applies one streamed conversation checkpoint to retained workspace memory.
/// 将单个流式对话检查点应用到保留工作区内存。
///
/// # Parameters
/// 参数
/// `app` emits the rebuilt workbench snapshot after the in-memory mutation.
/// `app` 用于在内存变更后发送重建的工作台快照。
/// `state` owns the shared service bridge state and retained workspace snapshot.
/// `state` 持有共享服务桥状态与保留工作区快照。
/// `controller_runtime` persists main-session checkpoint state after mutation.
/// `controller_runtime` 在变更后持久化主会话检查点状态。
/// `session_id` identifies the session receiving the checkpoint.
/// `session_id` 标识接收检查点的会话。
/// `conversation_state` is the runtime conversation state captured by the provider stream.
/// `conversation_state` 是 provider 流捕获的运行时对话状态。
/// # Returns
/// 返回值
/// Returns `Ok(())` after memory mutation and any required persistence tasks are scheduled.
/// 将一份终态报告应用到保留内存后产生的结果。
pub(super) struct TerminalReportMemoryApply {
    /// Exact owning project id for a regular main-session terminal report.
    /// 普通主会话终态报告的精确所属项目标识。
    pub(super) project_id: Option<String>,
    /// Completed-turn hook request that should run asynchronously.
    /// 需要异步执行的完成回合 hook 请求。
    pub(super) completion_request: Option<ConversationCompletionRequest>,
    /// Final conversation outcome when this report closes a regular user-facing turn.
    /// 当本报告闭合普通用户会话回合时的最终会话结果。
    pub(super) conversation_run_outcome: Option<ConversationRunOutcome>,
    /// Whether the report belongs to an indexed main session whose unread flag must be persisted.
    /// 报告是否属于已索引主会话，其未读标记必须持久化。
    pub(super) persists_unread: bool,
}

/// Exact conversation-hook side effect committed for one terminal report.
/// 单个终态报告提交的精确会话 Hook 副作用。
enum TerminalConversationHookCommit {
    /// This report does not own a completed or terminated hook action.
    /// 此报告不持有完成或终止 Hook 操作。
    None,
    /// A completed-turn request was prepared with its real logical turn identity.
    /// 已使用真实逻辑回合标识准备完成回合请求。
    Completion {
        /// Stable logical turn identifier consumed by final context injection state.
        /// 由最终上下文注入状态消费的稳定逻辑回合标识。
        turn_id: String,
        /// Prepared asynchronous completed-turn hook request.
        /// 已准备的异步完成回合 Hook 请求。
        request: ConversationCompletionRequest,
    },
    /// The failed logical turn was removed and its termination hook was dispatched.
    /// 失败逻辑回合已移除且其终止 Hook 已分发。
    Terminated,
}

/// Exclusive terminal conversation-hook action selected from report ownership and outcome.
/// 根据报告所有权与结果选择的互斥终态会话 Hook 操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalConversationHookAction {
    /// The report owns no logical main-session hook transition.
    /// 报告不持有主会话逻辑 Hook 转换。
    None,
    /// A successful report prepares completed-turn dispatch.
    /// 成功报告准备完成回合分发。
    Complete,
    /// A failed report terminates the logical turn.
    /// 失败报告终止逻辑回合。
    Terminate,
}

impl TerminalConversationHookAction {
    /// Selects one exclusive action from authoritative terminal facts.
    /// 根据权威终态事实选择一个互斥操作。
    ///
    /// # Parameters
    /// 参数
    /// `is_main_session` proves ownership, `is_success` is the report result, and `owns_hook_turn` proves hook participation.
    /// `is_main_session` 证明所有权，`is_success` 是报告结果，`owns_hook_turn` 证明 Hook 参与。
    /// `durable_turn_completed` proves that Core published the logical Turn closure, not just an API terminal.
    /// `durable_turn_completed` 证明 Core 已发布逻辑回合闭合，而不仅是 API 终态。
    ///
    /// # Returns
    /// 返回值
    /// Returns exactly one action without permitting contradictory completion and termination.
    /// 返回唯一操作，不允许同时完成与终止的矛盾状态。
    fn from_terminal_facts(
        is_main_session: bool,
        is_success: bool,
        owns_hook_turn: bool,
        durable_turn_completed: bool,
    ) -> Self {
        if !is_main_session || !owns_hook_turn || (is_success && !durable_turn_completed) {
            Self::None
        } else if is_success {
            Self::Complete
        } else {
            Self::Terminate
        }
    }
}

/// Returns whether one terminal action owns the single VMM TTL consumption boundary.
/// 返回单个终态动作是否拥有唯一 VMM TTL 消费边界。
///
/// # Parameters
/// 参数
/// `action` is the exclusive terminal hook action selected from authoritative facts.
/// `action` 是根据权威事实选择的互斥终态 Hook 动作。
/// `completion_status` identifies whether descendant work still belongs to the same turn.
/// `completion_status` 标识后代工作是否仍属于同一回合。
///
/// # Returns
/// 返回值
/// `true` only for a successful, fully completed logical user turn.
/// 仅成功且完整结束的逻辑用户回合返回 `true`。
fn terminal_action_consumes_vmm_ttl(
    action: TerminalConversationHookAction,
    completion_status: ConversationTurnCompletionStatus,
) -> bool {
    action == TerminalConversationHookAction::Complete && completion_status.is_final()
}

/// Immutable facts required to commit one terminal conversation-hook action.
/// 提交单个终态会话 Hook 操作所需的不可变事实。
struct TerminalConversationHookRequest<'a> {
    /// Exact regular session receiving the terminal report.
    /// 接收终态报告的精确普通会话。
    session_id: SessionId,
    /// Authoritative owning project projected into hook payloads.
    /// 投影到 Hook 载荷中的权威所属项目。
    project_id: Option<String>,
    /// Visible assistant output supplied to the completion hook.
    /// 提供给完成 Hook 的可见助手输出。
    assistant_content: &'a str,
    /// Provider request count recorded by the finished run.
    /// 已结束运行记录的 Provider 请求次数。
    provider_request_count: usize,
    /// Final conversation state supplied to completed-turn hooks.
    /// 提供给完成回合 Hook 的最终会话状态。
    conversation_state: &'a SessionConversationState,
    /// Completion-gate status that decides whether the logical turn is final.
    /// 决定逻辑回合是否终结的完成闸门状态。
    completion_status: ConversationTurnCompletionStatus,
    /// Number of descendant subagents still blocking final completion.
    /// 仍阻止最终完成的后代子代理数量。
    pending_subagent_count: usize,
    /// Whether VMM remained fully configured and runtime-ready at terminal dispatch.
    /// 终态调度时 VMM 是否仍完成配置并处于运行就绪状态。
    vmm_runtime_ready: bool,
    /// Exclusive logical-turn action selected before hook runtime locking.
    /// 在锁定 Hook 运行时前选择的互斥逻辑回合操作。
    action: TerminalConversationHookAction,
    /// Optional exact VMM active-turn identity that must match before Hook ownership is consumed.
    /// 在消费 Hook 所有权前必须匹配的可选精确 VMM 活动回合身份。
    expected_vmm_turn_id: Option<&'a str>,
}

/// Commits the exact terminal conversation-hook action before workspace state becomes visible.
/// 在工作区状态可见前提交精确的终态会话 Hook 操作。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns the retained hook runtime and `request` contains authoritative terminal facts.
/// `agent_runtime` 持有保留 Hook 运行时，`request` 包含权威终态事实。
///
/// # Returns
/// 返回值
/// Returns the strongly typed committed action, or an explicit hook-runtime lock diagnostic.
/// 返回强类型已提交操作，或明确的 Hook 运行时锁诊断。
fn commit_terminal_conversation_hook<F>(
    agent_runtime: &AgentRuntimeBridge,
    request: TerminalConversationHookRequest<'_>,
    before_completion_commit: F,
) -> Result<TerminalConversationHookCommit, String>
where
    F: FnOnce() -> Result<(), String>,
{
    if request.action == TerminalConversationHookAction::Complete {
        // hook_runtime must commit logical-turn ownership before workspace terminal state is published.
        // hook_runtime 必须在工作区终态发布前提交逻辑回合所有权。
        let mut hook_runtime = agent_runtime
            .lock_conversation_hooks()
            .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
        // Missing process-local ownership requires canonical restoration before consuming any completion effect.
        // 缺失进程内所有权时，必须先规范恢复，之后才能消费任何完成效果。
        // A final User message may be agent guidance or dynamic context and cannot identify the original business root.
        // 最后一条 User 消息可能是代理引导或动态上下文，不能标识原业务根输入。
        if !hook_runtime.has_active_turn(request.session_id) {
            return Err(String::from(
                "Business completion requires restoration of its original conversation Hook owner.",
            ));
        }
        if let Some(expected_turn_id) = request.expected_vmm_turn_id {
            // actual_turn_id is copied before the mutating completion call so mismatch cannot remove Hook ownership.
            // actual_turn_id 在可变完成调用前复制，使身份不匹配无法移除 Hook 所有权。
            let actual_turn_id = hook_runtime
                .active_turn_id(request.session_id)
                .map(str::to_string);
            if actual_turn_id.as_deref() != Some(expected_turn_id) {
                return Err(format!(
                    "VMM active logical turn `{expected_turn_id}` does not match the retained conversation Hook turn {:?}.",
                    actual_turn_id
                ));
            }
        }
        // before_completion_commit durably commits controller state while Hook ownership is still locked and intact.
        // before_completion_commit 在 Hook 所有权仍被锁定且保持完整时持久提交控制器状态。
        before_completion_commit()?;
        // turn_id and completion_request are produced atomically from the same retained logical turn.
        // turn_id 与 completion_request 由同一个保留逻辑回合原子地产生。
        let (turn_id, completion_request) = hook_runtime
            .prepare_turn_completion_dispatch_with_status(
                request.session_id,
                request.project_id,
                request.assistant_content,
                request.provider_request_count,
                request.conversation_state,
                request.completion_status,
                request.pending_subagent_count,
                request.vmm_runtime_ready,
            );
        return Ok(TerminalConversationHookCommit::Completion {
            turn_id,
            request: completion_request,
        });
    }
    if request.action == TerminalConversationHookAction::Terminate {
        // hook_runtime termination is part of the same terminal ownership commit, never best-effort cleanup.
        // hook_runtime 终止属于同一终态所有权提交，绝不是尽力而为的清理。
        let mut hook_runtime = agent_runtime
            .lock_conversation_hooks()
            .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
        hook_runtime.terminate_turn(
            request.session_id,
            request.project_id,
            "execution_failed",
            request.provider_request_count,
        );
        return Ok(TerminalConversationHookCommit::Terminated);
    }
    Ok(TerminalConversationHookCommit::None)
}

/// Applies a TaskSession terminal report without rebuilding the full WebUI workbench.
/// 在不重建完整 WebUI workbench 的情况下应用 TaskSession 终态报告。
///
/// # Parameters
/// 参数
/// `state` is the shared Core service bridge state that owns the in-memory workspace snapshot.
/// `state` 是持有内存工作区快照的共享 Core 服务桥状态。
/// `report` is the completed child execution report.
/// `report` 是已完成的子执行报告。
///
/// # Returns
/// 返回值
/// Returns the updated workspace snapshot and no hook completion request.
/// 返回更新后的工作区快照，且不产生 hook 完成请求。
pub(super) fn apply_task_session_execution_report_to_memory(
    state: &SharedServiceBridgeState,
    report: ExecutionRunReport,
) -> Result<TerminalReportMemoryApply, String> {
    let (session_id, is_success) =
        apply_task_session_execution_report_to_memory_in_place(state, &report)?;
    webui_trace(
        "execution_report_applied_task_session_fast",
        format!(
            "session={session_id} error={} provider_requests={} output_chars={}",
            !is_success,
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok(TerminalReportMemoryApply {
        project_id: None,
        completion_request: None,
        conversation_run_outcome: None,
        persists_unread: false,
    })
}

/// Applies a TaskSession terminal report in place without rebuilding the full workspace snapshot.
/// 原地应用 TaskSession 终态报告，不重建完整工作区快照。
///
/// # Parameters
/// 参数
/// `state` is the shared service bridge state containing the retained workspace snapshot.
/// `state` 是包含保留工作区快照的共享服务桥状态。
/// `report` is the terminal execution report to merge into the TaskSession state.
/// `report` 是要合并进 TaskSession 状态的终态执行报告。
///
/// # Returns
/// 返回值
/// Returns the affected session id and whether the report represents a successful run.
/// 返回受影响的会话标识以及该报告是否代表成功运行。
pub(super) fn apply_task_session_execution_report_to_memory_in_place(
    state: &SharedServiceBridgeState,
    report: &ExecutionRunReport,
) -> Result<(SessionId, bool), String> {
    let session_id = report.session_id;
    let is_success = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let is_success = {
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
            let now = current_unix_millis_text();
            let (is_success, runtime_state) = {
                let default_runtime_state = SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                };
                let source_runtime_state = workspace
                    .session_runtime_state
                    .get(&session_id)
                    .unwrap_or(&default_runtime_state);
                // The builder owns the single atomic clone; the caller never clones canonical history again.
                // 构建器拥有唯一一次原子克隆；调用方不再重复克隆规范历史。
                let (runtime_state, apply_result) = build_terminal_execution_report_state(
                    source_runtime_state,
                    session_id,
                    &now,
                    report,
                )?;
                (apply_result.is_success, runtime_state)
            };
            workspace
                .session_runtime_state
                .insert(session_id, runtime_state);
            workspace.session_run_phases.insert(
                session_id,
                if is_success {
                    SessionRunPhase::Ready
                } else {
                    SessionRunPhase::Failed
                },
            );
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            is_success
        };
        let cached_conversation_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .map(|runtime_state| runtime_state.conversation_state.clone());
        if let Some(cache_entry) = guard.subagent_surface_detail_cache.get_mut(&session_id) {
            cache_entry.task_state.conversation_state = cached_conversation_state;
            cache_entry.last_access_ms = current_unix_millis_value();
        }
        is_success
    };
    webui_trace(
        "execution_report_applied_task_session_in_place",
        format!(
            "session={session_id} error={} provider_requests={} output_chars={}",
            !is_success,
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok((session_id, is_success))
}

/// Applies the terminal execution report to retained memory and emits the updated workbench view.
/// 将终态执行报告应用到保留内存中，并发出更新后的工作台视图。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit the refreshed workbench snapshot.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot mutated by the stream task.
/// `state` 持有由流任务修改的保留工作区快照。
/// `agent_runtime` provides conversation hooks and VMM synchronization state.
/// `agent_runtime` 提供会话 hook 与 VMM 同步状态。
/// `report` is the terminal execution report produced by the agent runtime.
/// `report` 是 Agent 运行时产出的终态执行报告。
/// `durable_turn_completed` comes from the exact V1 Turn-completion receipt and authorizes Hook completion.
/// `durable_turn_completed` 来自精确 V1 回合完成回执，并授权 Hook 完成。
///
/// # Returns
/// 返回值
/// The updated workspace snapshot after the terminal report has been applied in memory.
/// 返回终态报告已经应用到内存后的更新工作区快照。
pub(super) fn apply_execution_report_to_memory(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    report: ExecutionRunReport,
    durable_turn_completed: bool,
) -> Result<TerminalReportMemoryApply, String> {
    let session_id = report.session_id;
    webui_trace(
        "execution_report_apply_enter",
        format!("session={session_id}"),
    );
    let report_is_regular_session = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|workspace_snapshot| {
                workspace_snapshot_contains_regular_session(workspace_snapshot, session_id)
            })
    };
    if !report_is_regular_session {
        return apply_task_session_execution_report_to_memory(state, report);
    }
    // Agent-originated work retains internal ownership but cannot consume the user-memory lifecycle.
    // 代理发起工作保留内部归属，但不能消费用户记忆生命周期。
    let observes_user = agent_runtime
        .lock_conversation_hooks()
        .map_err(|error| format!("Failed to read business observer policy: {error}"))?
        .permits_user_observation(session_id);
    // Both immutable admission and current service readiness are required for terminal VMM work.
    // 终态 VMM 工作同时要求不可变准入事实与当前服务就绪。
    let vmm_runtime_ready = observes_user
        && report
            .preparation
            .as_ref()
            .is_some_and(|preparation| preparation.vmm_runtime_ready)
        && webui_vmm_runtime_readiness(state)?.service_ready;
    let vmm_controller = if vmm_runtime_ready {
        webui_trace(
            "execution_report_vmm_controller_open_start",
            format!("session={session_id}"),
        );
        let controller =
            open_registered_session_vmm_controller(state, session_id, &current_unix_millis_text())?;
        webui_trace(
            "execution_report_vmm_controller_open_finished",
            format!("session={session_id}"),
        );
        Some(controller)
    } else {
        None
    };
    let prompt_purpose = report
        .preparation
        .as_ref()
        .map(|preparation| preparation.prompt_purpose)
        .unwrap_or(ExecutionPromptPurpose::UserTurn);
    let runs_completed_turn_hooks = prompt_purpose.runs_completed_turn_hooks();
    // Measures completion-gate inspection for active hooks and descendant subagents.
    // 衡量完成闸门对活跃 hook 与后代子代理的检查耗时。
    let completion_gate_started_at = current_unix_millis_value();
    webui_trace(
        "execution_report_completion_gate_start",
        format!("session={session_id}"),
    );
    let completion_gate = webui_turn_completion_gate_from_state(state, agent_runtime, session_id)?;
    webui_trace(
        "execution_report_completion_gate_finished",
        format!("session={session_id}"),
    );
    webui_trace_if_slow(
        "execution_report_completion_gate_slow",
        completion_gate_started_at,
        format!("session={session_id}"),
    );
    let mut completion_request = None;
    // Measures in-memory workspace mutation for the terminal execution report.
    // 衡量终态执行报告写入内存工作区的耗时。
    let workspace_apply_started_at = current_unix_millis_value();
    webui_trace(
        "execution_report_workspace_apply_start",
        format!("session={session_id}"),
    );
    let (project_id, is_main_session, terminal_run_phase, terminal_status_label) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        {
            let now = current_unix_millis_text();
            // Main-session membership decides whether completion hooks and VMM side effects may run.
            // 主会话成员关系决定完成 hook 与 VMM 副作用是否允许执行。
            let is_main_session =
                workspace_session_summary_for_session(workspace, session_id).is_some();
            let project_id = owning_project_id_for_session(workspace, session_id);
            // runtime_state is detached so terminal identity failure cannot insert or partially mutate main-session memory.
            // runtime_state 保持分离，因此终态身份失败不会插入或部分修改主会话内存。
            let default_runtime_state = SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            };
            let source_runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .unwrap_or(&default_runtime_state);
            // VMM state is inspected only while the fully configured service remains ready.
            // 仅当完成配置的 VMM 服务仍就绪时才读取其状态。
            let vmm_state_before_terminal = match vmm_controller.as_ref() {
                Some(controller) => Some(controller.snapshot()?),
                None => None,
            };
            let (mut runtime_state, apply_result) = build_terminal_execution_report_state(
                source_runtime_state,
                session_id,
                &now,
                &report,
            )?;
            // hook_action encodes the only legal logical-turn transition for this terminal report.
            // hook_action 编码此终态报告唯一合法的逻辑回合转换。
            let hook_action = TerminalConversationHookAction::from_terminal_facts(
                is_main_session,
                apply_result.is_success,
                runs_completed_turn_hooks || completion_gate.has_active_hook_turn,
                durable_turn_completed,
            );
            // completion_status freezes the inspected gate decision for hook commit and context consumption.
            // completion_status 固化已检查的闸门决策，供 Hook 提交与上下文消费共同使用。
            let completion_status = completion_gate.completion_status();
            // Only the durable Turn receipt authorizes logical completion after a successful physical report.
            // 物理报告成功后，只有可靠回合回执才能授权逻辑完成。
            let logical_turn_completed =
                durable_turn_completed && apply_result.is_success && completion_status.is_final();
            // consumes_vmm_ttl identifies the single final boundary before either state machine commits ownership.
            // consumes_vmm_ttl 在任一状态机提交所有权前标识唯一终态边界。
            let consumes_vmm_ttl = vmm_runtime_ready
                && terminal_action_consumes_vmm_ttl(hook_action, completion_status);
            // expected_vmm_turn_id identifies the controller command that must commit under the Hook ownership lock.
            // expected_vmm_turn_id 标识必须在 Hook 所有权锁内提交的控制器命令。
            let expected_vmm_turn_id = if consumes_vmm_ttl
                && vmm_state_before_terminal.as_ref().is_some_and(|state| {
                    state.active_turn_id.is_some() || state.last_consumed_turn_id.is_some()
                }) {
                // turn_id is the exact controller owner that must match the active Hook turn.
                // turn_id 是必须与活动 Hook 回合匹配的精确控制器所有者。
                let vmm_state_before_terminal =
                    vmm_state_before_terminal.as_ref().ok_or_else(|| {
                        String::from("Durable VMM state is unavailable at terminal completion.")
                    })?;
                let turn_id = vmm_state_before_terminal
                    .active_turn_id
                    .clone()
                    .or_else(|| vmm_state_before_terminal.last_consumed_turn_id.clone())
                    .ok_or_else(|| {
                        String::from(
                            "Durable VMM state has no registered active logical turn at terminal completion.",
                        )
                    })?;
                Some(turn_id)
            } else {
                None
            };
            // hook_commit makes logical-turn ownership explicit before any workspace mutation is installed.
            // hook_commit 在安装任何工作区变更前显式提交逻辑回合所有权。
            webui_trace(
                "execution_report_terminal_hook_start",
                format!("session={session_id}"),
            );
            let hook_commit = commit_terminal_conversation_hook(
                agent_runtime,
                TerminalConversationHookRequest {
                    session_id,
                    project_id: project_id.clone(),
                    assistant_content: report.assistant_output.as_deref().unwrap_or(
                        "The model completed the run but did not return visible text output.",
                    ),
                    provider_request_count: report.provider_request_count,
                    conversation_state: &runtime_state.conversation_state,
                    completion_status,
                    pending_subagent_count: completion_gate.pending_subagent_count,
                    vmm_runtime_ready,
                    action: hook_action,
                    expected_vmm_turn_id: expected_vmm_turn_id.as_deref(),
                },
                || {
                    let Some(turn_id) = expected_vmm_turn_id.as_deref() else {
                        return Ok(());
                    };
                    let Some(vmm_controller) = vmm_controller.as_ref() else {
                        return Err(String::from(
                            "VMM controller is unavailable for a terminal turn that requires TTL consumption.",
                        ));
                    };
                    // command_id makes a terminal retry side-effect free after a durable controller commit.
                    // command_id 使控制器持久提交后的终态重试不产生副作用。
                    let command_id = format!("terminal_turn:{turn_id}");
                    let _ = vmm_controller.execute_command(&command_id, &now, |candidate| {
                        if candidate.last_consumed_turn_id.as_deref() != Some(turn_id) {
                            candidate.consume_turn(turn_id, &now)?;
                        }
                        Ok(())
                    })?;
                    Ok(())
                },
            )?;
            webui_trace(
                "execution_report_terminal_hook_finished",
                format!("session={session_id}"),
            );
            if let TerminalConversationHookCommit::Completion { turn_id, request } = hook_commit {
                completion_request = Some(request);
                if consumes_vmm_ttl {
                    runtime_state
                        .context_injection_state
                        .consume_turn(&turn_id, &now);
                    runtime_state.vmm_profile_sync.record_completed_turn();
                }
            }
            if let Some(session) =
                active_session_summary_for_session_mut(&mut workspace.sessions, session_id)
            {
                session.status = if !apply_result.is_success {
                    SessionState::Blocked
                } else if logical_turn_completed {
                    SessionState::Completed
                } else {
                    SessionState::Running
                };
                session.last_updated_label = String::from("just now");
            }
            if workspace.selected_session != session_id {
                runtime_state.ui_state.unread = true;
            }
            workspace
                .session_runtime_state
                .insert(session_id, runtime_state);
            workspace.session_run_phases.insert(
                session_id,
                if !apply_result.is_success {
                    SessionRunPhase::Failed
                } else if logical_turn_completed {
                    SessionRunPhase::Ready
                } else {
                    SessionRunPhase::Preparing
                },
            );
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            if is_main_session {
                // Snapshot refresh is a derived runtime projection after the authoritative workspace commit.
                // 快照刷新是权威工作区提交后的派生运行时投影。
                if let Err(snapshot_refresh_error) = refresh_exact_vmm_hook_session_projection(
                    &agent_runtime.vmm_hook_runtime,
                    workspace,
                    session_id,
                ) {
                    tracing::warn!(
                        "[webui-service] failed to refresh VMM hook runtime after terminal workspace commit for session {session_id}: {snapshot_refresh_error}"
                    );
                }
            }
            let terminal_run_phase = if !apply_result.is_success {
                SessionRunPhase::Failed
            } else if logical_turn_completed {
                SessionRunPhase::Ready
            } else {
                SessionRunPhase::Preparing
            };
            let terminal_status_label = if !apply_result.is_success {
                SessionState::Blocked.label().to_string()
            } else if logical_turn_completed {
                SessionState::Completed.label().to_string()
            } else {
                SessionState::Running.label().to_string()
            };
            (
                project_id,
                is_main_session,
                terminal_run_phase,
                terminal_status_label,
            )
        }
    };
    webui_trace_if_slow(
        "execution_report_workspace_apply_slow",
        workspace_apply_started_at,
        format!("session={session_id}"),
    );
    webui_trace(
        "execution_report_terminal_deltas_start",
        format!("session={session_id}"),
    );
    emit_terminal_execution_report_workbench_deltas(
        app,
        state,
        session_id,
        terminal_run_phase,
        terminal_status_label.as_str(),
    )?;
    webui_trace(
        "execution_report_terminal_deltas_finished",
        format!("session={session_id}"),
    );
    webui_trace(
        "execution_report_applied",
        format!(
            "session={session_id} is_main_session={} prompt_purpose={} runs_completed_turn_hooks={} completion_status={} pending_subagents={} has_active_hook_turn={} error={} provider_requests={} output_chars={}",
            is_main_session,
            prompt_purpose.diagnostics_label(),
            runs_completed_turn_hooks,
            completion_gate.completion_status().as_str(),
            completion_gate.pending_subagent_count,
            completion_gate.has_active_hook_turn,
            report.error.is_some(),
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    // conversation_run_outcome crosses the service boundary only for a genuinely final main-session turn.
    // conversation_run_outcome 仅在主会话回合真正结束时跨越服务边界。
    let conversation_run_outcome = (durable_turn_completed
        && completion_gate.completion_status().is_final())
    .then_some(if report.error.is_some() {
        ConversationRunOutcome::Failed
    } else {
        ConversationRunOutcome::Completed
    });
    Ok(TerminalReportMemoryApply {
        project_id,
        completion_request,
        conversation_run_outcome,
        // Only indexed main sessions own a durable unread row; child task sessions have none.
        // 只有已索引主会话拥有持久未读行；子任务会话没有。
        persists_unread: is_main_session,
    })
}

/// Emits terminal-report deltas without rebuilding the full workbench snapshot.
/// 在不重建完整工作台快照的情况下发出终态报告增量。
///
/// # Parameters
/// 参数
/// `app` emits delta events to connected WebUI clients.
/// `app` 将增量事件发给已连接的 WebUI 客户端。
/// `state` owns the retained workbench and workspace projections.
/// `state` 持有保留的工作台与工作区投影。
/// `session_id` identifies the terminal session.
/// `session_id` 标识进入终态的会话。
/// `run_phase` is the terminal run phase to expose.
/// `run_phase` 是需要展示的终态运行阶段。
/// `status_label` is the sidebar status label paired with the terminal phase.
/// `status_label` 是与终态阶段配套的侧栏状态标签。
///
/// # Returns
/// 返回值
/// `Ok(())` after all required deltas have been emitted.
/// 在发出所有必要增量后返回 `Ok(())`。
fn emit_terminal_execution_report_workbench_deltas(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    run_phase: SessionRunPhase,
    status_label: &str,
) -> Result<(), String> {
    let conversation_id = session_id.to_string();
    let v1_usage_facts = {
        let registry = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .session_runtime_registry
            .clone();
        session_context_usage_facts_from_v1_registry(registry.as_ref(), session_id)?
    };
    let events = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let surface_matches =
            workbench_surface_matches_conversation(&guard.workbench, &conversation_id);
        let public_model_capabilities = session_public_ai_model_capabilities(
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.public_ai_runtime.as_ref()),
            session_id,
            workspace.session_runtime_state.get(&session_id),
        );
        let context_usage = if surface_matches {
            context_usage_snapshot_from_runtime_state(
                session_id,
                workspace.session_runtime_state.get(&session_id),
                v1_usage_facts.as_ref(),
                &guard.live_context_budgets,
                public_model_capabilities.as_ref(),
                crate::context_compaction_window_limit_from_configuration(
                    workspace.session_runtime_state.get(&session_id),
                    guard
                        .agent_runtime
                        .as_ref()
                        .and_then(|runtime| runtime.public_ai_runtime.configuration_snapshot().ok())
                        .as_ref(),
                ),
            )
        } else {
            None
        };
        // unread is the runtime flag just committed by the terminal report; the summary must mirror it.
        // unread 是终态报告刚提交的运行态标记；摘要必须镜像它。
        let unread = workspace
            .session_runtime_state
            .get(&session_id)
            .is_some_and(|runtime_state| runtime_state.ui_state.unread);
        // Attention is rebuilt from the same live inputs as a full refresh, so the unread dot is not lost.
        // 注意力标记与完整刷新使用相同的实时输入重建，避免丢失未读圆点。
        let session_plan_mode = session_plan_mode_summary_from_runtime_state(
            workspace.session_runtime_state.get(&session_id),
        );
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
        let mut events = Vec::new();
        events.push(terminal_report_workbench_delta_envelope(
            &mut guard,
            WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id: conversation_id.clone(),
                run_phase: run_phase.to_string(),
                status: Some(status_label.to_string()),
                last_updated_label: Some(String::from("now")),
            }),
        ));
        let mut changed_conversation = None;
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = status_label.to_string();
                conversation.last_updated_label = String::from("just now");
                conversation.last_updated_at = current_unix_millis_text();
                conversation.unread = unread;
                conversation.attention = conversation_attention_summary_for_session(
                    session_id,
                    &pending_questions,
                    session_plan_mode.as_ref(),
                    unread,
                    Some(conversation.last_updated_at.clone()),
                );
                changed_conversation = Some(conversation.clone());
                break;
            }
        }
        if let Some(conversation) = changed_conversation {
            let selected_conversation_id = guard.workbench.selected_conversation_id.clone();
            events.push(terminal_report_workbench_delta_envelope(
                &mut guard,
                WorkbenchDelta::ConversationSummaryChanged(ConversationSummaryChangedDelta {
                    conversation,
                    selected_conversation_id,
                }),
            ));
        }
        if surface_matches {
            events.push(terminal_report_workbench_delta_envelope(
                &mut guard,
                WorkbenchDelta::ContextUsageChanged(ContextUsageChangedDelta {
                    context_usage: context_usage.clone(),
                }),
            ));
            guard.workbench.context_usage = context_usage;
        }
        events
    };
    for event in events {
        emit_service_event_with_trace(
            app,
            "workbench_terminal_report_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    Ok(())
}

/// Builds a terminal-report workbench delta envelope and advances the retained revision.
/// 构建终态报告工作台增量信封，并推进保留的工作台版本。
///
/// # Parameters
/// 参数
/// `guard` is the locked service state whose workbench revision should advance.
/// `guard` 是已加锁的服务状态，其工作台版本需要推进。
/// `delta` is the workbench delta emitted for the terminal report flow.
/// `delta` 是终态报告流程发出的工作台增量。
///
/// # Returns
/// 返回值
/// A delta envelope carrying the newly assigned generation.
/// 返回携带新分配 generation 的增量信封。
fn terminal_report_workbench_delta_envelope(
    guard: &mut ServiceBridgeState,
    delta: WorkbenchDelta,
) -> WorkbenchDeltaEnvelope {
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    let generation = guard.workbench_generation;
    guard.workbench.revision = generation;
    WorkbenchDeltaEnvelope { generation, delta }
}

/// Spawns the completed-turn hook without blocking queued prompt dispatch.
/// 启动完成回合 hook，避免阻塞排队提示词分发。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit any later VMM outbox updates.
/// `app` 是用于发出后续 VMM outbox 更新的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot that receives hook side effects.
/// `state` 持有接收 hook 副作用的保留工作区快照。
/// `agent_runtime` provides VMM hook snapshot refresh state.
/// `agent_runtime` 提供 VMM hook 快照刷新状态。
/// `completion_request` is the prepared completed-turn hook dispatch request.
/// `completion_request` 是已准备的完成回合 hook 调度请求。
pub(super) fn spawn_conversation_completion_hook_task(
    _app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    agent_runtime: AgentRuntimeBridge,
    mut completion_request: ConversationCompletionRequest,
) {
    spawn_service_task(async move {
        // Current readiness can only revoke the turn's previously admitted VMM permission.
        // 当前就绪状态只能撤销回合此前准入的 VMM 权限。
        let ready = match webui_vmm_runtime_readiness(&state) {
            Ok(readiness) => readiness.service_ready,
            Err(error) => {
                tracing::warn!("[webui-service] VMM completion readiness check failed: {error}");
                false
            }
        };
        completion_request.restrict_vmm_runtime_readiness(ready);
        let outcome = completion_request.run().await;
        let session_id = outcome.session_id;
        // Record hook dispatch by default without logging conversation content.
        // 默认记录 Hook 派发结果，不输出对话正文。
        match outcome.report.as_ref() {
            Ok(report) => tracing::info!(
                "[webui-service][vmm-writeback] stage=hook_result session={} turn={} service_ready={} payload_ready={:?} postaction_completed={} failures={} effects={}",
                session_id,
                outcome.turn_id,
                ready,
                report
                    .payload
                    .value
                    .get("vmm_runtime_ready")
                    .and_then(serde_json::Value::as_bool),
                report
                    .completed_hooks
                    .iter()
                    .any(|hook| hook == "builtin.vmm.post_action"),
                report.failures.len(),
                outcome.vmm_writeback_effects.len(),
            ),
            Err(error) => tracing::warn!(
                "[webui-service][vmm-writeback] stage=hook_result session={} turn={} service_ready={} dispatch_error={}",
                session_id,
                outcome.turn_id,
                ready,
                error,
            ),
        }
        // Continue-policy hook failures are carried in a successful dispatch report, not its Result error.
        // Continue 策略的 Hook 失败存于成功调度报告内，而非 Result 错误中。
        if let Ok(report) = outcome.report.as_ref() {
            for failure in &report.failures {
                tracing::warn!(
                    "[webui-service] completed-turn hook handler failed for {}: hook={} error={}",
                    outcome.turn_id,
                    failure.hook_id,
                    failure.message
                );
            }
        }
        if let Err(error) = outcome.report.as_ref() {
            tracing::warn!(
                "[webui-service] completed-turn hook failed for {}: {error}",
                outcome.turn_id
            );
        }
        if outcome.vmm_writeback_effects.is_empty() {
            return;
        }
        match webui_vmm_runtime_readiness(&state) {
            Ok(readiness) if readiness.service_ready => {}
            Ok(_) => return,
            Err(error) => {
                tracing::warn!("[webui-service] VMM writeback readiness check failed: {error}");
                return;
            }
        }
        // Open the existing durable controller before taking the outer service lock; publication is validated later.
        // 取得外层服务锁之前打开既有持久控制器；实际发布稍后再校验。
        let now = current_unix_millis_text();
        let controller = if outcome
            .vmm_writeback_effects
            .iter()
            .any(|effect| matches!(effect, VmmWritebackHookEffect::SessionStatus { .. }))
        {
            match open_registered_session_vmm_controller(&state, session_id, &now) {
                Ok(controller) => Some(controller),
                Err(error) => {
                    tracing::warn!(
                        "[webui-service] failed to open VMM session controller after PostAction for {session_id}: {error}"
                    );
                    return;
                }
            }
        } else {
            None
        };
        let mut guard = match state.lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("failed to lock service state: {error}");
                return;
            }
        };
        // Lock order is service state, hook admission, then session controller; configuration cannot interleave publication.
        // 锁顺序为服务状态、Hook 准入、会话控制器；配置变更不能穿插发布。
        let result = agent_runtime
            .vmm_hook_runtime
            .with_current_writeback_effects(
                session_id,
                outcome.vmm_writeback_effects,
                |effects| -> Result<(), String> {
                    if effects.is_empty() {
                        return Ok(());
                    }
                    let workspace = guard
                        .workspace_snapshot
                        .as_mut()
                        .ok_or_else(|| "workspace snapshot is not ready".to_owned())?;
                    if let Some((permit_id, status)) =
                        effects.iter().rev().find_map(|effect| match effect {
                            VmmWritebackHookEffect::SessionStatus { permit_id, status } => {
                                Some((*permit_id, *status))
                            }
                            _ => None,
                        })
                    {
                        // A new generation must not reuse a previous destination's status command receipt.
                        // 新代次不得复用此前目的地的状态命令回执。
                        let command_id = format!(
                            "postaction-session-status:{permit_id}:{}:{}",
                            outcome.turn_id,
                            serde_json::to_string(&status).map_err(|error| error.to_string())?
                        );
                        controller
                            .as_ref()
                            .ok_or_else(|| {
                                "VMM lifecycle effect has no opened session controller.".to_owned()
                            })?
                            .execute_command(&command_id, &now, |candidate| {
                                candidate
                                    .apply_authoritative_session_status(
                                        &session_id.to_string(),
                                        status,
                                        &now,
                                    )
                                    .map(|_| ())
                            })?;
                    }
                    if let Some(runtime_state) =
                        workspace.session_runtime_state.get_mut(&session_id)
                    {
                        apply_vmm_writeback_effects_to_runtime_state(runtime_state, effects, &now);
                        runtime_state.updated_at = now.clone();
                    }
                    Ok(())
                },
            )
            .and_then(|result| result);
        if let Err(error) = result {
            tracing::warn!(
                "[webui-service] failed to publish VMM writeback effects for {session_id}: {error}"
            );
            return;
        }
        // Release admission's read guard before refreshing the mirror under its write lock.
        // 在以写锁刷新镜像之前释放准入读守卫。
        if let Some(workspace) = guard.workspace_snapshot.as_ref()
            && let Err(error) = refresh_exact_vmm_hook_session_projection(
                &agent_runtime.vmm_hook_runtime,
                workspace,
                session_id,
            )
        {
            tracing::warn!(
                "[webui-service] failed to refresh VMM hook runtime after completed-turn effects for session {session_id}: {error}"
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records every service event so the emitted terminal delta can be inspected.
    /// 记录每个服务事件，以便检查发出的终态增量。
    #[derive(Default)]
    struct RecordedEvents {
        events: std::sync::Mutex<Vec<ServiceEvent>>,
    }

    impl vulcan_service_api::ServiceEventSink for RecordedEvents {
        /// Stores one emitted event; returns after the synchronous capture.
        /// 保存一个已发出的事件；同步捕获后返回。
        fn emit_service_event(&self, event: ServiceEvent) {
            self.events.lock().expect("recorded events").push(event);
        }
    }

    /// A background session that finished unread must publish `unread` and its attention in the summary delta.
    /// 后台完成且未读的会话必须在摘要增量中发布 `unread` 及其注意力标记。
    #[test]
    fn terminal_summary_delta_carries_the_committed_unread_flag() {
        let selected_id = uuid::Uuid::new_v4();
        let background_id = uuid::Uuid::new_v4();
        let mut service =
            crate::tests::test_support::test_service_state_with_parent_session(selected_id);
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.ui_state.unread = true;
        service
            .workspace_snapshot
            .as_mut()
            .expect("workspace")
            .session_runtime_state
            .insert(background_id, runtime_state);
        service.workbench.selected_conversation_id = Some(selected_id.to_string());
        // The retained summary still holds the stale value captured when the turn started.
        // 保留的摘要仍是回合开始时捕获的过期值。
        service
            .workbench
            .conversations
            .push(vulcan_service_api::ConversationSummary {
                id: background_id.to_string(),
                title: String::from("background"),
                status: String::from("Running"),
                run_phase: SessionRunPhase::Preparing.to_string(),
                last_updated_label: String::new(),
                last_updated_at: String::from("1"),
                created_at: String::from("1"),
                last_request_at: String::from("1"),
                manual_position: None,
                unread: false,
                attention: None,
                project_id: Some(String::from("project-a")),
                dedicated: false,
                assistant_name: None,
            });
        let state: SharedServiceBridgeState = std::sync::Arc::new(std::sync::Mutex::new(service));
        let recorded = std::sync::Arc::new(RecordedEvents::default());
        let app = ServiceEventEmitter::new(vec![recorded.clone()]);

        emit_terminal_execution_report_workbench_deltas(
            &app,
            &state,
            background_id,
            SessionRunPhase::Ready,
            SessionState::Completed.label(),
        )
        .expect("emit terminal deltas");

        let events = recorded.events.lock().expect("recorded events");
        let delta = events
            .iter()
            .find_map(|event| match event {
                ServiceEvent::WorkbenchDelta(envelope) => match &envelope.delta {
                    WorkbenchDelta::ConversationSummaryChanged(delta) => Some(delta),
                    _ => None,
                },
                _ => None,
            })
            .expect("conversation summary delta");
        assert!(delta.conversation.unread);
        assert_eq!(
            delta
                .conversation
                .attention
                .as_ref()
                .map(|attention| attention.kind.as_str()),
            Some("unread")
        );
        let guard = state.lock().expect("service state");
        let retained = guard
            .workbench
            .conversations
            .iter()
            .find(|conversation| conversation.id == background_id.to_string())
            .expect("retained summary");
        assert!(retained.unread);
    }

    /// Missing Hook ownership rejects completion before TTL persistence or synthesized turn allocation.
    /// 缺失 Hook 归属时，在 TTL 持久化或合成回合分配之前拒绝完成。
    /// Returns nothing; an actual registered root then proves normal completion still uses its original identity.
    /// 不返回值；随后实际登记根输入，证明正常完成仍使用原身份。
    #[tokio::test]
    async fn terminal_hook_requires_original_owner_before_effect_commit() {
        // The real runtime starts without a process-local Hook owner, matching a cold load.
        // 真实运行时起初没有进程内 Hook 所有者，与冷加载一致。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        // This physical session retains one stable identity throughout both checks.
        // 此物理会话在两次检查中保留同一稳定身份。
        let session_id = uuid::Uuid::new_v4();
        // No conversation text can substitute for the missing original ownership.
        // 任何会话文本都不能替代缺失的原归属。
        let conversation = SessionConversationState::default();
        // This counter represents the durable side effect executed only after ownership validation.
        // 此计数表示仅在归属校验后执行的可靠副作用。
        let committed = std::cell::Cell::new(0);
        // The exact request factory keeps all terminal facts identical across both attempts.
        // 精确请求工厂使两次尝试的全部终态事实保持相同。
        let request = || TerminalConversationHookRequest {
            session_id,
            project_id: None,
            assistant_content: "Done",
            provider_request_count: 1,
            conversation_state: &conversation,
            completion_status: ConversationTurnCompletionStatus::Completed,
            pending_subagent_count: 0,
            vmm_runtime_ready: false,
            action: TerminalConversationHookAction::Complete,
            expected_vmm_turn_id: None,
        };
        assert!(
            commit_terminal_conversation_hook(&runtime, request(), || {
                committed.set(committed.get() + 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(committed.get(), 0);
        assert!(
            !runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .has_active_turn(session_id)
        );
        // Actual root admission creates the one owner permitted to commit this terminal.
        // 实际根准入创建唯一获准提交此终态的所有者。
        let admitted = runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .prepare_user_prompt_hook_requests(
                session_id,
                "original-input",
                None,
                "D:/hook-owner-test".to_owned(),
                vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                vulcan_hooks::SystemPromptHookPhase::FirstSession,
                "Original task",
                &conversation,
                false,
            );
        // Preserve the captured root identity; completion must not allocate a replacement.
        // 保留捕获根身份；完成不能分配替代身份。
        let result = commit_terminal_conversation_hook(&runtime, request(), || {
            committed.set(committed.get() + 1);
            Ok(())
        })
        .unwrap();
        assert!(
            matches!(result, TerminalConversationHookCommit::Completion { turn_id, .. }
            if turn_id == admitted.turn_id)
        );
        assert_eq!(committed.get(), 1);
        assert!(
            !runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .has_active_turn(session_id)
        );
    }

    /// Verifies terminal hook ownership selects one exclusive action for every outcome.
    /// 验证终态 Hook 所有权为每种结果选择唯一互斥操作。
    #[test]
    fn terminal_conversation_hook_action_is_exclusive() {
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, true, true, true),
            TerminalConversationHookAction::Complete
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, false, true, false),
            TerminalConversationHookAction::Terminate
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, true, false, true),
            TerminalConversationHookAction::None
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(false, true, true, true),
            TerminalConversationHookAction::None
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, true, true, false),
            TerminalConversationHookAction::None,
            "a physical terminal must preserve the Hook of an unfinished logical Turn"
        );
    }

    /// Verifies failures, stopped ownership termination, and incomplete descendant work do not consume TTL.
    /// 验证失败、停止后的所有权终止以及未完成后代工作都不会消费 TTL。
    #[test]
    fn vmm_ttl_consumption_requires_successful_final_completion() {
        assert!(terminal_action_consumes_vmm_ttl(
            TerminalConversationHookAction::Complete,
            ConversationTurnCompletionStatus::Completed,
        ));
        assert!(!terminal_action_consumes_vmm_ttl(
            TerminalConversationHookAction::Complete,
            ConversationTurnCompletionStatus::AwaitingSubagents,
        ));
        assert!(!terminal_action_consumes_vmm_ttl(
            TerminalConversationHookAction::Terminate,
            ConversationTurnCompletionStatus::Completed,
        ));
        assert!(!terminal_action_consumes_vmm_ttl(
            TerminalConversationHookAction::None,
            ConversationTurnCompletionStatus::Completed,
        ));
    }
}
