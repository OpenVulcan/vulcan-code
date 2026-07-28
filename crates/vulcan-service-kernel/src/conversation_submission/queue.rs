//! Existing-session submission queue decision and commit.
//! 已有会话提交排队决策与提交。

use vulcan_core::types::ConversationAttachment;
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::*;
use crate::pending_prompts::dispatch_queued_pending_prompt_hook;
use crate::{AgentRuntimeBridge, webui_trace};

/// Decision for routing one submission around blocking system prompt work.
/// 围绕阻塞系统提示词工作路由单次提交的决策。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExistingSubmissionQueueDecision {
    /// The submission may continue into immediate session materialization.
    /// 提交可以继续进入即时会话物化。
    StartImmediately,
    /// The exact existing session must receive the submission through its visible queue.
    /// 精确已有会话必须通过可见队列接收提交。
    Queue { session_id: Uuid },
}

/// Owned inputs consumed when one existing-session submission commits to the visible queue.
/// 单次已有会话提交写入可见队列时消费的有所有权输入。
pub(crate) struct ExistingSubmissionQueueInput {
    /// Hydrated workspace snapshot that owns the target queue.
    /// 持有目标队列的已水合工作区快照。
    pub(crate) workspace_snapshot: WorkspaceSnapshot,
    /// Exact existing session that owns the queue.
    /// 持有队列的精确已有会话。
    pub(crate) session_id: Uuid,
    /// User prompt body retained by the pending entry.
    /// 待发送条目保留的用户提示词正文。
    pub(crate) prompt: String,
    /// Optional hidden model guidance retained for later dispatch.
    /// 为后续分发保留的可选隐藏模型引导。
    pub(crate) hidden_model_guidance: Option<String>,
    /// Composer attachments retained by the pending entry.
    /// 待发送条目保留的输入区附件。
    pub(crate) attachments: Vec<ConversationAttachment>,
    /// Optional reasoning effort retained for later dispatch.
    /// 为后续分发保留的可选推理强度。
    pub(crate) reasoning_effort: Option<String>,
    /// Service-entry timestamp used by queued-return latency diagnostics.
    /// 排队返回延迟诊断使用的服务入口时间戳。
    pub(crate) submit_total_started_at: i128,
}

/// Commits one mutation against the latest retained workspace while the service-state lock is held.
/// 在持有服务状态锁期间，针对最新保留工作区提交一次变更。
///
/// # Parameters
/// 参数
/// `state` owns the latest retained workspace that may have advanced after submission preflight.
/// `state` 持有可能在提交预检后继续推进的最新保留工作区。
/// `fallback_workspace` is the hydrated submission snapshot used only when the retained state lacks the target.
/// `fallback_workspace` 是已水合提交快照，仅在保留状态缺少目标时使用。
/// `session_id` identifies the exact visible session whose queue is being changed.
/// `session_id` 标识待修改队列所属的精确可见会话。
/// `mutation` applies the queue-specific change to a lock-consistent workspace clone.
/// `mutation` 将队列专属变更应用到与锁一致的工作区副本。
///
/// # Returns
/// 返回值
/// Returns the mutation result after atomically replacing the retained workspace, or an explicit error.
/// 原子替换保留工作区后返回变更结果，或返回显式错误。
pub(crate) fn mutate_latest_workspace_for_existing_session<T, F>(
    state: &SharedServiceBridgeState,
    fallback_workspace: WorkspaceSnapshot,
    session_id: Uuid,
    mutation: F,
) -> Result<T, String>
where
    F: FnOnce(&mut WorkspaceSnapshot) -> Result<T, String>,
{
    // guard prevents stream projection from advancing the retained workspace during clone, mutation, and commit.
    // guard 防止流式投影在克隆、变更与提交期间推进保留工作区。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // committed_workspace starts from the latest retained revision whenever it already exists.
    // committed_workspace 在最新保留版本已经存在时始终以该版本为起点。
    let mut committed_workspace = match guard.workspace_snapshot.as_ref() {
        Some(current_workspace) => {
            let mut current_workspace = current_workspace.clone();
            merge_missing_existing_session_scope(
                &mut current_workspace,
                &fallback_workspace,
                session_id,
            )?;
            current_workspace
        }
        None => fallback_workspace,
    };
    // result is produced before publication so a failed mutation cannot leave a partial workspace commit.
    // result 在发布前生成，确保变更失败不会留下部分工作区提交。
    let result = mutation(&mut committed_workspace)?;
    guard.workspace_snapshot = Some(committed_workspace);
    Ok(result)
}

/// Reconciles only one visible session and its missing owned data into the latest workspace revision.
/// 仅将一个可见会话及其缺失的所属数据协调到最新工作区版本。
///
/// # Parameters
/// 参数
/// `current_workspace` is the latest retained workspace that must preserve unrelated concurrent changes.
/// `current_workspace` 是必须保留无关并发变更的最新保留工作区。
/// `fallback_workspace` owns the fully hydrated target session loaded during submission preflight.
/// `fallback_workspace` 持有提交预检期间加载的完整已水合目标会话。
/// `session_id` identifies the only session scope allowed to merge.
/// `session_id` 标识唯一允许合并的会话范围。
///
/// # Returns
/// 返回值
/// Returns success when the exact target scope is complete, otherwise an integrity error.
/// 精确目标范围完整时返回成功，否则返回完整性错误。
fn merge_missing_existing_session_scope(
    current_workspace: &mut WorkspaceSnapshot,
    fallback_workspace: &WorkspaceSnapshot,
    session_id: Uuid,
) -> Result<(), String> {
    // project_id comes from the latest summary when present and from the hydrated fallback only for a missing summary.
    // project_id 在最新摘要存在时取自最新摘要，仅在摘要缺失时取自已水合回退快照。
    let (project_id, session_was_missing) = if let Some(current_session) = current_workspace
        .sessions
        .iter()
        .find(|session| session.id == session_id)
    {
        (current_session.project_id.clone(), false)
    } else {
        // source_session is the unique visible target proven by the hydrated submission snapshot.
        // source_session 是已水合提交快照证明存在的唯一可见目标。
        let source_session = fallback_workspace
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .cloned()
            .ok_or_else(|| {
                format!("queued session `{session_id}` is missing from hydrated workspace")
            })?;
        // project_id retains the exact owner before the session summary moves into the current workspace.
        // project_id 在会话摘要移入当前工作区前保留其精确所属项目。
        let project_id = source_session.project_id.clone();
        current_workspace.sessions.push(source_session);
        (project_id, true)
    };
    if session_was_missing && let Some(project_id) = project_id {
        if let Some(current_project) = current_workspace
            .task_projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            if !current_project.session_ids.contains(&session_id) {
                current_project.session_ids.push(session_id);
            }
        } else {
            // source_project is required only when the latest workspace lacks the owning project itself.
            // source_project 仅在最新工作区连所属项目也缺失时才是必需数据。
            let source_project = fallback_workspace
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
                .cloned()
                .ok_or_else(|| {
                    format!("queued session `{session_id}` owns missing project `{project_id}`")
                })?;
            current_workspace.task_projects.push(source_project);
        }
    }
    if let Some(messages) = fallback_workspace.session_messages.get(&session_id) {
        current_workspace
            .session_messages
            .entry(session_id)
            .or_insert_with(|| messages.clone());
    }
    if let Some(compactions) = fallback_workspace.session_compactions.get(&session_id) {
        current_workspace
            .session_compactions
            .entry(session_id)
            .or_insert_with(|| compactions.clone());
    }
    if let Some(runtime_state) = fallback_workspace.session_runtime_state.get(&session_id) {
        current_workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| runtime_state.clone());
    }
    if let Some(run_phase) = fallback_workspace.session_run_phases.get(&session_id) {
        current_workspace
            .session_run_phases
            .entry(session_id)
            .or_insert(*run_phase);
    }
    if fallback_workspace
        .hydrated_session_ids
        .contains(&session_id)
    {
        current_workspace.hydrated_session_ids.insert(session_id);
    }
    Ok(())
}

/// Evaluates whether an existing-session submission must enter the visible pending queue.
/// 判断已有会话提交是否必须进入可见待发送队列。
///
/// # Parameters
/// # 参数
/// `state` owns the current active task registry and session kernel.
/// `state` 持有当前活动任务注册表与会话内核。
/// `workspace_snapshot` is the hydrated submission workspace used by pending-lane rules.
/// `workspace_snapshot` 是待发送通道规则使用的已水合提交工作区。
/// `submission_session_id` identifies the existing target, or `None` for a draft conversation.
/// `submission_session_id` 标识已有目标；草稿会话为 `None`。
///
/// # Returns
/// # 返回值
/// A strong immediate-or-queue routing decision, or an explicit state lock error.
/// 返回强类型即时或排队路由决策，或显式状态锁错误。
pub(crate) fn evaluate_existing_submission_queue(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    submission_session_id: Option<Uuid>,
) -> Result<ExistingSubmissionQueueDecision, String> {
    // session_id exists only for submissions targeting an already materialized conversation.
    // session_id 仅在提交目标为已物化会话时存在。
    let Some(session_id) = submission_session_id else {
        return Ok(ExistingSubmissionQueueDecision::StartImmediately);
    };
    // blocking_check_started_at measures the complete existing-session queue gate.
    // blocking_check_started_at 衡量完整的已有会话排队闸门。
    let blocking_check_started_at = current_unix_millis_value();
    // active_state_started_at isolates the shared-state lookup portion of the gate.
    // active_state_started_at 隔离闸门中的共享状态查询部分。
    let active_state_started_at = current_unix_millis_value();
    // activity reads the diagnostic stream handle and authoritative active kernel turn from one revision.
    // activity 从同一版本读取诊断用流句柄与权威活动内核回合。
    let (has_active_stream_task, has_active_kernel_turn) = {
        // guard keeps both activity diagnostics on one authoritative service-state revision.
        // guard 使两个活动诊断保持在同一个权威服务状态版本上。
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.active_stream_tasks.contains_key(&session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
        )
    };
    webui_trace_if_slow(
        "submit_prompt_active_state_check_slow",
        active_state_started_at,
        format!("session={session_id}"),
    );
    webui_submit_trace(
        "submit_prompt_subagent_running_check_skipped",
        format!("session={session_id} reason=independent_subagent_lane"),
    );
    // has_precheck_progress is the retained precheck ownership marker consumed by pending-lane rules.
    // has_precheck_progress 是待发送通道规则消费的保留 precheck 所有权标记。
    let has_precheck_progress = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_precheck_turn_id.as_deref())
        .is_some_and(|turn_id| !turn_id.trim().is_empty());
    // should_queue is the authoritative shared pending-lane decision for this existing session.
    // should_queue 是当前已有会话的权威共享待发送通道决策。
    let should_queue = session_has_blocking_system_prompt_work(
        workspace_snapshot,
        session_id,
        has_active_kernel_turn,
        has_precheck_progress,
    );
    webui_submit_trace(
        "submit_prompt_blocking_check",
        format!(
            "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_session_run={has_active_kernel_turn} has_running_subagents=false elapsed_ms={}",
            current_unix_millis_value().saturating_sub(blocking_check_started_at)
        ),
    );
    Ok(if should_queue {
        ExistingSubmissionQueueDecision::Queue { session_id }
    } else {
        ExistingSubmissionQueueDecision::StartImmediately
    })
}

/// Commits one existing-session submission to the visible queue and returns its acknowledgement.
/// 将单次已有会话提交写入可见队列并返回确认结果。
///
/// # Parameters
/// # 参数
/// `app` emits pending-queue and run-phase deltas in commit order.
/// `app` 按提交顺序发送待发送队列与运行阶段增量。
/// `state` owns the committed shared workspace snapshot.
/// `state` 持有已提交的共享工作区快照。
/// `controller_runtime` persists the latest post-delta workspace asynchronously.
/// `controller_runtime` 异步持久化增量后的最新工作区。
/// `agent_runtime` supplies the exact queued prompt runtime options.
/// `agent_runtime` 提供精确的排队提示词运行时选项。
/// `input` owns the hydrated workspace, exact session, pending entry fields, and latency origin.
/// `input` 持有已水合工作区、精确会话、待发送条目字段与延迟起点。
///
/// # Returns
/// # 返回值
/// The queued submission acknowledgement after both ordered deltas commit.
/// 返回两个有序增量均提交后的排队提交确认结果。
pub(crate) fn queue_existing_submission(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    input: ExistingSubmissionQueueInput,
) -> Result<SubmitConversationPromptAck, String> {
    // input fields are destructured once so the queue commit cannot mix values from different submissions.
    // input 字段只解构一次，确保队列提交不会混用不同提交的值。
    let ExistingSubmissionQueueInput {
        workspace_snapshot,
        session_id,
        prompt,
        hidden_model_guidance,
        attachments,
        reasoning_effort,
        submit_total_started_at,
    } = input;
    webui_submit_trace(
        "submit_prompt_queued",
        format!("session={session_id} reason=blocking_system_prompt_work"),
    );
    // queue_commit mutates the latest shared snapshot so a stale preflight clone cannot erase streamed output.
    // queue_commit 修改最新共享快照，避免过期预检副本擦除流式输出。
    let queued_hook_event = mutate_latest_workspace_for_existing_session(
        state,
        workspace_snapshot,
        session_id,
        |latest_workspace| {
            // workspace_path is resolved from the lock-consistent latest workspace revision.
            // workspace_path 从与锁一致的最新工作区版本解析。
            let workspace_path = workspace_path_for_session(latest_workspace, session_id);
            // queued_hook_event contains only committed queue facts for post-commit Hook dispatch.
            // queued_hook_event 仅包含供提交后 Hook 分发使用的已提交队列事实。
            let queued_hook_event = enqueue_webui_pending_prompt_from_submission(
                latest_workspace,
                agent_runtime,
                session_id,
                &workspace_path,
                prompt,
                hidden_model_guidance,
                attachments,
                reasoning_effort.as_deref(),
            )?;
            Ok(queued_hook_event)
        },
    )?;
    if let Err(error) = dispatch_queued_pending_prompt_hook(agent_runtime, queued_hook_event) {
        webui_trace(
            "queued_submission_hook_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    // _pending_revision confirms the queue projection committed before the ordered phase delta.
    // _pending_revision 确认队列投影先于有序阶段增量提交。
    let _pending_revision = emit_pending_prompts_changed_delta(app, state, session_id)?;
    // revision is the final ordered workbench revision returned to the submit caller.
    // revision 是返回提交调用方的最终有序工作台版本。
    let (revision, run_phase) = emit_queued_prompt_run_phase_delta(app, state, session_id)?;
    // state_for_persist owns the latest shared snapshot lookup performed after both deltas.
    // state_for_persist 持有两个增量之后执行的最新共享快照查询。
    let state_for_persist = state.clone();
    // controller_runtime_for_persist owns the background workspace persistence handle.
    // controller_runtime_for_persist 持有后台工作区持久化句柄。
    let controller_runtime_for_persist = controller_runtime.clone();
    spawn_service_task(async move {
        // snapshot reads the newest shared workspace only after both queue deltas have committed.
        // snapshot 仅在两个队列增量提交后读取最新共享工作区。
        let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[webui-service] queued submit persist snapshot failed: {error}");
                return;
            }
        };
        spawn_workspace_session_persist_task(
            state_for_persist,
            controller_runtime_for_persist,
            snapshot,
            session_id,
            Some(String::from("queued_submit")),
        );
    });
    webui_submit_trace(
        "submit_prompt_queued_returning",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    Ok(SubmitConversationPromptAck {
        conversation_id: session_id.to_string(),
        accepted_message_id: None,
        turn_id: None,
        run_phase: run_phase.to_string(),
        // queued records the submission route, even if execution starts before this ACK returns.
        // queued 记录本次提交经过排队路径，即使执行在 ACK 返回前已经开始也保持不变。
        queued: true,
        revision,
    })
}
