//! Terminal execution report memory application for WebUI sessions.
//! WebUI 会话终态执行报告内存应用逻辑。

use vulcan_agent_runtime::{
    ExecutionPromptPurpose, ExecutionRunReport,
    agent_runtime::conversation_hooks::{
        ConversationCompletionRequest, ConversationTurnCompletionStatus, VmmWritebackHookEffect,
    },
    conversation_protocol::SessionConversationState,
};
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_service_api::{ConversationRunOutcome, ServiceEventEmitter};
use vulcan_workbench_model::{SessionId, SessionRunPhase, SessionRuntimeStateEntry, SessionState};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::vmm_runtime_audit::{
    VmmRemovedLeaseAuditAction, build_vmm_lifecycle_events_for_state_diff,
};

use super::{
    AgentRuntimeBridge, ContextUsageChangedDelta, ServiceBridgeState, ServiceEvent,
    SharedServiceBridgeState, UpdateConversationRunPhaseDelta, WorkbenchDelta,
    WorkbenchDeltaEnvelope, active_public_ai_model_capabilities,
    active_session_summary_for_session_mut, apply_checkpoint_conversation_state,
    apply_terminal_execution_report_to_state, apply_vmm_writeback_effects_to_runtime_state,
    build_conversation_messages_patched_delta, build_vmm_hook_config_snapshot,
    context_usage_snapshot_from_runtime_state, conversation_messages_from_session,
    current_unix_millis_text, current_unix_millis_value, emit_service_event_with_trace,
    merge_runtime_state_records_into_display_messages, owning_project_id_for_session,
    persist_workspace_session_update, replace_conversation_messages_in_workbench_window,
    replace_task_display_messages_with_runtime_state_records, spawn_service_task,
    spawn_workspace_session_runtime_state_persist_task,
    spawn_workspace_session_transcript_persist_task, sync_hydrated_session_dialogue_ready_marker,
    task_display_prompt_anchor_from_workspace, webui_trace, webui_trace_if_slow,
    webui_turn_completion_gate_from_state, workbench_surface_matches_conversation,
    workspace_session_summary_for_session, workspace_snapshot_contains_regular_session,
};

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
/// `last_successful_provider_usage` records the latest provider usage when present.
/// `last_successful_provider_usage` 在存在时记录最近一次成功 provider 用量。
/// `provider_request_count` records the safe provider request count for continuation.
/// `provider_request_count` 记录续跑可用的安全 provider 请求次数。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after memory mutation and any required persistence tasks are scheduled.
/// 在完成内存变更并调度必要持久化任务后返回 `Ok(())`。
pub(super) fn apply_conversation_checkpoint(
    _app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: SessionConversationState,
    last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    provider_request_count: usize,
) -> Result<(), String> {
    let (is_main_session, workspace_snapshot) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let cached_task_prompt_anchor = cached_subagent_task_prompt_anchor(&guard, session_id);
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        {
            let now = current_unix_millis_text();
            // Only main-session checkpoints may persist through workspace-store; TaskSession checkpoints stay in memory here.
            // 只有主会话 checkpoint 可以通过 workspace-store 持久化；TaskSession checkpoint 在这里仅更新内存显示态。
            let is_main_session =
                workspace_session_summary_for_session(workspace, session_id).is_some();
            // runtime_state is a detached checkpoint draft so validation failure cannot insert or partially mutate workspace state.
            // runtime_state 是分离的检查点草稿，因此校验失败不会插入或部分修改工作区状态。
            let mut runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .cloned()
                .unwrap_or_else(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            apply_checkpoint_conversation_state(&mut runtime_state, conversation_state, &now)?;
            if let Some(usage) = last_successful_provider_usage {
                runtime_state.last_provider_usage = Some(usage.clone());
                runtime_state.last_successful_provider_usage = Some(usage);
            }
            if provider_request_count > 0 {
                runtime_state.last_safe_provider_request_count = Some(provider_request_count);
            }
            let runtime_state_snapshot = runtime_state.clone();
            workspace
                .session_runtime_state
                .insert(session_id, runtime_state);
            if is_main_session {
                merge_runtime_state_records_into_display_messages(
                    workspace,
                    session_id,
                    &runtime_state_snapshot,
                );
            } else {
                let task_prompt_anchor =
                    task_display_prompt_anchor_from_workspace(workspace, session_id)
                        .or(cached_task_prompt_anchor);
                replace_task_display_messages_with_runtime_state_records(
                    workspace,
                    session_id,
                    &runtime_state_snapshot,
                    task_prompt_anchor.as_deref(),
                );
            }
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            (is_main_session, workspace.clone())
        }
    };
    if is_main_session {
        spawn_workspace_session_transcript_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot.clone(),
            session_id,
            Some(String::from("conversation_checkpoint_transcript")),
        );
        spawn_workspace_session_runtime_state_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
            Some(String::from("conversation_checkpoint")),
        );
    }
    Ok(())
}

/// Result produced after applying one terminal report to retained memory.
/// 将一份终态报告应用到保留内存后产生的结果。
pub(super) struct TerminalReportMemoryApply {
    /// Updated workspace snapshot after applying the terminal report.
    /// 应用终态报告后的更新工作区快照。
    pub(super) workspace_snapshot: WorkspaceSnapshot,
    /// Completed-turn hook request that should run asynchronously.
    /// 需要异步执行的完成回合 hook 请求。
    pub(super) completion_request: Option<ConversationCompletionRequest>,
    /// Final conversation outcome when this report closes a regular user-facing turn.
    /// 当本报告闭合普通用户会话回合时的最终会话结果。
    pub(super) conversation_run_outcome: Option<ConversationRunOutcome>,
    /// Body-free VMM lifecycle events produced by terminal TTL consumption.
    /// 终态 TTL 消费产生的不含正文 VMM 生命周期事件。
    pub(super) vmm_lifecycle_events: Vec<vulcan_context::RuntimeVmmLifecycleEvent>,
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
    ///
    /// # Returns
    /// 返回值
    /// Returns exactly one action without permitting contradictory completion and termination.
    /// 返回唯一操作，不允许同时完成与终止的矛盾状态。
    fn from_terminal_facts(is_main_session: bool, is_success: bool, owns_hook_turn: bool) -> Self {
        if !is_main_session || !owns_hook_turn {
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
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
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
            .conversation_hook_runtime
            .lock()
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
/// `state` is the shared Tauri service bridge state that owns the in-memory workspace snapshot.
/// `state` 是持有内存工作区快照的共享 Tauri 服务桥状态。
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
    let workspace_snapshot = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
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
        workspace_snapshot,
        completion_request: None,
        conversation_run_outcome: None,
        vmm_lifecycle_events: Vec::new(),
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
        let cached_task_prompt_anchor = cached_subagent_task_prompt_anchor(&guard, session_id);
        let (is_success, runtime_state_snapshot, display_messages) = {
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
            let now = current_unix_millis_text();
            let task_prompt_anchor =
                task_display_prompt_anchor_from_workspace(workspace, session_id)
                    .or(cached_task_prompt_anchor);
            let (is_success, runtime_state_snapshot) = {
                // runtime_state is detached so terminal identity failure cannot insert or partially mutate TaskSession memory.
                // runtime_state 保持分离，因此终态身份失败不会插入或部分修改 TaskSession 内存。
                let mut runtime_state = workspace
                    .session_runtime_state
                    .get(&session_id)
                    .cloned()
                    .unwrap_or_else(|| SessionRuntimeStateEntry {
                        updated_at: now.clone(),
                        ..SessionRuntimeStateEntry::default()
                    });
                let apply_result = apply_terminal_execution_report_to_state(
                    &mut runtime_state,
                    session_id,
                    &now,
                    report,
                )?;
                // runtime_state_snapshot is the authoritative state after terminal report application for display projection.
                // runtime_state_snapshot 是终态报告应用后的权威状态，用于显示层投影。
                let runtime_state_snapshot = runtime_state.clone();
                workspace
                    .session_runtime_state
                    .insert(session_id, runtime_state);
                (apply_result.is_success, runtime_state_snapshot)
            };
            replace_task_display_messages_with_runtime_state_records(
                workspace,
                session_id,
                &runtime_state_snapshot,
                task_prompt_anchor.as_deref(),
            );
            workspace.session_run_phases.insert(
                session_id,
                if is_success {
                    SessionRunPhase::Ready
                } else {
                    SessionRunPhase::Failed
                },
            );
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            let display_messages = workspace
                .session_messages
                .get(&session_id)
                .cloned()
                .unwrap_or_default();
            (is_success, runtime_state_snapshot, display_messages)
        };
        if let Some(cache_entry) = guard.subagent_surface_detail_cache.get_mut(&session_id) {
            cache_entry.messages = display_messages;
            cache_entry.task_state.conversation_state =
                Some(runtime_state_snapshot.conversation_state.clone());
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

/// Returns the cached child-task prompt anchor retained by the subagent detail surface.
/// 返回子代理详情界面缓存中保留的子任务提示词锚点。
///
/// # Parameters
/// 参数
/// `state` is the retained service bridge state that may contain a subagent detail cache entry.
/// `state` 是可能包含子代理详情缓存条目的保留服务桥状态。
/// `session_id` selects the child TaskSession whose prompt anchor is needed.
/// `session_id` 选择需要提示词锚点的子 TaskSession。
///
/// # Returns
/// 返回值
/// A non-empty cached task description when available.
/// 当存在非空任务描述时返回缓存锚点。
fn cached_subagent_task_prompt_anchor(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> Option<String> {
    state
        .subagent_surface_detail_cache
        .get(&session_id)?
        .task_state
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
) -> Result<TerminalReportMemoryApply, String> {
    let session_id = report.session_id;
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
    // vmm_runtime_context is cloned before the retained workspace transaction begins.
    // vmm_runtime_context 在保留工作区事务开始前完成克隆。
    let (controller_runtime, vmm_controller_registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("Core VMM controller runtime is unavailable at terminal completion.")
            })?,
            guard.vmm_controller_registry.clone(),
        )
    };
    // vmm_controller is the sole terminal TTL state owner for this session.
    // vmm_controller 是该会话终态 TTL 状态的唯一所有者。
    let vmm_controller = vmm_controller_registry.get_or_open(
        &controller_runtime.data_root,
        session_id,
        &current_unix_millis_text(),
    )?;
    let prompt_purpose = report
        .preparation
        .as_ref()
        .map(|preparation| preparation.prompt_purpose)
        .unwrap_or(ExecutionPromptPurpose::UserTurn);
    let runs_completed_turn_hooks = prompt_purpose.runs_completed_turn_hooks();
    // Measures completion-gate inspection for active hooks and descendant subagents.
    // 衡量完成闸门对活跃 hook 与后代子代理的检查耗时。
    let completion_gate_started_at = current_unix_millis_value();
    let completion_gate = webui_turn_completion_gate_from_state(state, agent_runtime, session_id)?;
    webui_trace_if_slow(
        "execution_report_completion_gate_slow",
        completion_gate_started_at,
        format!("session={session_id}"),
    );
    let mut completion_request = None;
    // Measures in-memory workspace mutation for the terminal execution report.
    // 衡量终态执行报告写入内存工作区的耗时。
    let workspace_apply_started_at = current_unix_millis_value();
    let (workspace_snapshot, terminal_run_phase, terminal_status_label, vmm_lifecycle_events) = {
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
            let mut runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .cloned()
                .unwrap_or_else(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            // vmm_state_before_terminal is the exact controller audit baseline before TTL consumption.
            // vmm_state_before_terminal 是 TTL 消费前精确的控制器审计基线。
            let vmm_state_before_terminal = vmm_controller.snapshot()?;
            let apply_result = apply_terminal_execution_report_to_state(
                &mut runtime_state,
                session_id,
                &now,
                &report,
            )?;
            let conversation_state = runtime_state.conversation_state.clone();
            // hook_action encodes the only legal logical-turn transition for this terminal report.
            // hook_action 编码此终态报告唯一合法的逻辑回合转换。
            let hook_action = TerminalConversationHookAction::from_terminal_facts(
                is_main_session,
                apply_result.is_success,
                runs_completed_turn_hooks || completion_gate.has_active_hook_turn,
            );
            // completion_status freezes the inspected gate decision for hook commit and context consumption.
            // completion_status 固化已检查的闸门决策，供 Hook 提交与上下文消费共同使用。
            let completion_status = completion_gate.completion_status();
            // consumes_vmm_ttl identifies the single final boundary before either state machine commits ownership.
            // consumes_vmm_ttl 在任一状态机提交所有权前标识唯一终态边界。
            let consumes_vmm_ttl = terminal_action_consumes_vmm_ttl(hook_action, completion_status);
            // expected_vmm_turn_id identifies the controller command that must commit under the Hook ownership lock.
            // expected_vmm_turn_id 标识必须在 Hook 所有权锁内提交的控制器命令。
            let expected_vmm_turn_id = if consumes_vmm_ttl
                && (vmm_state_before_terminal.active_turn_id.is_some()
                    || vmm_state_before_terminal.last_consumed_turn_id.is_some())
            {
                // turn_id is the exact controller owner that must match the active Hook turn.
                // turn_id 是必须与活动 Hook 回合匹配的精确控制器所有者。
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
            let hook_commit = commit_terminal_conversation_hook(
                agent_runtime,
                TerminalConversationHookRequest {
                    session_id,
                    project_id,
                    assistant_content: report.assistant_output.as_deref().unwrap_or(
                        "The model completed the run but did not return visible text output.",
                    ),
                    provider_request_count: report.provider_request_count,
                    conversation_state: &conversation_state,
                    completion_status,
                    pending_subagent_count: completion_gate.pending_subagent_count,
                    action: hook_action,
                    expected_vmm_turn_id: expected_vmm_turn_id.as_deref(),
                },
                || {
                    let Some(turn_id) = expected_vmm_turn_id.as_deref() else {
                        return Ok(());
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
                session.status = if apply_result.is_success {
                    SessionState::Completed
                } else {
                    SessionState::Blocked
                };
                session.last_updated_label = String::from("刚刚");
            }
            if workspace.selected_session != session_id {
                runtime_state.ui_state.unread = true;
            }
            let runtime_state_snapshot = runtime_state.clone();
            workspace
                .session_runtime_state
                .insert(session_id, runtime_state);
            merge_runtime_state_records_into_display_messages(
                workspace,
                session_id,
                &runtime_state_snapshot,
            );
            workspace.session_run_phases.insert(
                session_id,
                if apply_result.is_success {
                    SessionRunPhase::Ready
                } else {
                    SessionRunPhase::Failed
                },
            );
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            if is_main_session {
                // Snapshot refresh is a derived runtime projection after the authoritative workspace commit.
                // 快照刷新是权威工作区提交后的派生运行时投影。
                if let Err(snapshot_refresh_error) = agent_runtime
                    .vmm_hook_runtime
                    .replace_snapshot(build_vmm_hook_config_snapshot(
                        &agent_runtime.provider_configuration,
                        workspace,
                    ))
                {
                    eprintln!(
                        "[webui-service] failed to refresh VMM hook runtime after terminal workspace commit for session {session_id}: {snapshot_refresh_error}"
                    );
                }
            }
            let terminal_run_phase = if apply_result.is_success {
                SessionRunPhase::Ready
            } else {
                SessionRunPhase::Failed
            };
            let terminal_status_label = if apply_result.is_success {
                SessionState::Completed.label().to_string()
            } else {
                SessionState::Blocked.label().to_string()
            };
            // vmm_lifecycle_events retain no provider-visible memory text.
            // vmm_lifecycle_events 不保留任何 Provider 可见记忆正文。
            // vmm_state_after_terminal binds every audit field to one exact controller revision.
            // vmm_state_after_terminal 将全部审计字段绑定到同一个精确控制器修订。
            let vmm_state_after_terminal = vmm_controller.snapshot()?;
            let vmm_lifecycle_events = build_vmm_lifecycle_events_for_state_diff(
                &vmm_state_before_terminal,
                &vmm_state_after_terminal,
                vmm_state_after_terminal.last_consumed_turn_id.as_deref(),
                VmmRemovedLeaseAuditAction::Expired,
            );
            (
                workspace.clone(),
                terminal_run_phase,
                terminal_status_label,
                vmm_lifecycle_events,
            )
        }
    };
    webui_trace_if_slow(
        "execution_report_workspace_apply_slow",
        workspace_apply_started_at,
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
        "execution_report_applied",
        format!(
            "session={session_id} is_main_session={} prompt_purpose={} runs_completed_turn_hooks={} completion_status={} pending_subagents={} has_active_hook_turn={} error={} provider_requests={} output_chars={}",
            workspace_session_summary_for_session(&workspace_snapshot, session_id).is_some(),
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
    let conversation_run_outcome =
        completion_gate
            .completion_status()
            .is_final()
            .then_some(if report.error.is_some() {
                ConversationRunOutcome::Failed
            } else {
                ConversationRunOutcome::Completed
            });
    Ok(TerminalReportMemoryApply {
        workspace_snapshot,
        completion_request,
        conversation_run_outcome,
        vmm_lifecycle_events,
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
    let events = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let surface_matches =
            workbench_surface_matches_conversation(&guard.workbench, &conversation_id);
        let before_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let messages = if surface_matches {
            workspace
                .session_messages
                .get(&session_id)
                .map(|entries| conversation_messages_from_session(&conversation_id, entries, false))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let public_model_capabilities = active_public_ai_model_capabilities(
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.public_ai_runtime.as_ref()),
        );
        let context_usage = if surface_matches {
            context_usage_snapshot_from_runtime_state(
                session_id,
                workspace.session_runtime_state.get(&session_id),
                &guard.live_context_budgets,
                public_model_capabilities.as_ref(),
            )
        } else {
            None
        };
        let mut events = Vec::new();
        let phase_event = terminal_report_workbench_delta_envelope(
            &mut guard,
            WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id: conversation_id.clone(),
                run_phase: run_phase.to_string(),
                status: Some(status_label.to_string()),
                last_updated_label: Some(String::from("now")),
            }),
        );
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = status_label.to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = current_unix_millis_text();
                break;
            }
        }
        events.push(phase_event);
        if surface_matches {
            if let Some(message_patch) = build_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                messages.as_slice(),
            ) {
                let message_event = terminal_report_workbench_delta_envelope(
                    &mut guard,
                    WorkbenchDelta::ConversationMessagesPatched(message_patch),
                );
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &conversation_id,
                    messages,
                );
                events.push(message_event);
            }
            let context_event = terminal_report_workbench_delta_envelope(
                &mut guard,
                WorkbenchDelta::ContextUsageChanged(ContextUsageChangedDelta {
                    context_usage: context_usage.clone(),
                }),
            );
            guard.workbench.context_usage = context_usage;
            events.push(context_event);
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
/// `controller_runtime` persists VMM outbox changes after the hook finishes.
/// `controller_runtime` 在 hook 完成后持久化 VMM outbox 变更。
/// `agent_runtime` provides VMM hook snapshot refresh state.
/// `agent_runtime` 提供 VMM hook 快照刷新状态。
/// `completion_request` is the prepared completed-turn hook dispatch request.
/// `completion_request` 是已准备的完成回合 hook 调度请求。
pub(super) fn spawn_conversation_completion_hook_task(
    _app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    completion_request: ConversationCompletionRequest,
) {
    spawn_service_task(async move {
        let outcome = completion_request.run().await;
        let session_id = outcome.session_id;
        if let Err(error) = outcome.report.as_ref() {
            eprintln!(
                "[webui-service] completed-turn hook failed for {}: {}",
                outcome.turn_id, error
            );
        }
        if outcome.vmm_writeback_effects.is_empty() {
            return;
        }
        // authoritative_status is the last successful PostAction lifecycle observation in FIFO order.
        // authoritative_status 是按 FIFO 顺序最后一次成功 PostAction 观察到的生命周期状态。
        let authoritative_status = outcome
            .vmm_writeback_effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                VmmWritebackHookEffect::SessionStatus { status } => Some(*status),
                _ => None,
            });
        if let Some(status) = authoritative_status {
            let vmm_controller_registry = match state.lock() {
                Ok(guard) => guard.vmm_controller_registry.clone(),
                Err(error) => {
                    eprintln!("failed to lock service state: {error}");
                    return;
                }
            };
            let now = current_unix_millis_text();
            let controller = match vmm_controller_registry.get_or_open(
                &controller_runtime.data_root,
                session_id,
                &now,
            ) {
                Ok(controller) => controller,
                Err(error) => {
                    eprintln!(
                        "[webui-service] failed to open VMM session controller after PostAction for {session_id}: {error}"
                    );
                    return;
                }
            };
            let command_id = format!(
                "postaction-session-status:{}:{}",
                outcome.turn_id,
                serde_json::to_string(&status).unwrap_or_else(|_| String::from("invalid"))
            );
            if let Err(error) = controller.execute_command(&command_id, &now, |candidate| {
                candidate
                    .apply_authoritative_session_status(&session_id.to_string(), status, &now)
                    .map(|_| ())
            }) {
                eprintln!(
                    "[webui-service] failed to persist authoritative VMM session status for {session_id}: {error}"
                );
                return;
            }
        }
        let workspace_snapshot = {
            let mut guard = match state.lock() {
                Ok(guard) => guard,
                Err(error) => {
                    eprintln!("failed to lock service state: {error}");
                    return;
                }
            };
            let workspace = match guard.workspace_snapshot.as_mut() {
                Some(workspace) => workspace,
                None => {
                    eprintln!(
                        "[webui-service] apply completed-turn hook effects failed: workspace snapshot is not ready"
                    );
                    return;
                }
            };
            let now = current_unix_millis_text();
            if let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) {
                apply_vmm_writeback_effects_to_runtime_state(
                    runtime_state,
                    outcome.vmm_writeback_effects,
                    &now,
                );
                runtime_state.updated_at = now;
            }
            // snapshotRefreshError must not discard already-applied Hook effects or skip persistence.
            // snapshotRefreshError 不得丢弃已应用的 Hook 副作用，也不得跳过持久化。
            if let Err(snapshot_refresh_error) =
                agent_runtime
                    .vmm_hook_runtime
                    .replace_snapshot(build_vmm_hook_config_snapshot(
                        &agent_runtime.provider_configuration,
                        workspace,
                    ))
            {
                eprintln!(
                    "[webui-service] failed to refresh VMM hook runtime after completed-turn effects for session {session_id}: {snapshot_refresh_error}"
                );
            }
            workspace.clone()
        };
        if let Err(error) = persist_workspace_session_update(
            Some(&state),
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            eprintln!("[webui-service] persist completed-turn hook effects failed: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies terminal hook ownership selects one exclusive action for every outcome.
    /// 验证终态 Hook 所有权为每种结果选择唯一互斥操作。
    #[test]
    fn terminal_conversation_hook_action_is_exclusive() {
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, true, true),
            TerminalConversationHookAction::Complete
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, false, true),
            TerminalConversationHookAction::Terminate
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(true, true, false),
            TerminalConversationHookAction::None
        );
        assert_eq!(
            TerminalConversationHookAction::from_terminal_facts(false, true, true),
            TerminalConversationHookAction::None
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
