//! Existing-session submission queue decision and commit.
//! 已有会话提交排队决策与提交。

use vulcan_agent_runtime::attachment_asset_index::release_staged_attachment_ref;
use vulcan_core::types::ConversationAttachment;
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::activation::merge_submission_message_entries;
use super::preflight::existing_submission_workspace_slice;
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
    /// Stable client-generated identifier returned by the queued acknowledgement.
    /// 由排队确认原样返回的客户端稳定生成标识。
    pub(crate) client_submission_id: String,
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
    /// Composer attachments retained by the pending entry with only an optional metadata-only
    /// reduction; direct modes are recomputed at dispatch.
    /// 待发送条目保留的输入区附件，仅允许包含可选的仅元数据降级；直接传输模式在分发时重算。
    pub(crate) attachments: Vec<ConversationAttachment>,
    /// Optional internal composer-draft lease retained until this queued prompt becomes a canonical message.
    /// 在排队提示词成为规范消息前保留的可选内部输入区草稿租约。
    pub(crate) attachment_draft_scope: Option<String>,
    /// Optional reasoning effort retained for later dispatch.
    /// 为后续分发保留的可选推理强度。
    pub(crate) reasoning_effort: Option<String>,
    /// Optional exact provider, model, and credential selection retained for later dispatch.
    /// 为后续分发保留的可选精确供应商、模型与凭据选择。
    pub(crate) model_selection: Option<SubmissionModelSelection>,
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
/// `mutation` applies the queue-specific change to a lock-consistent one-session slice.
/// `mutation` 将队列专属变更应用到与锁一致的单会话切片。
///
/// # Returns
/// 返回值
/// Returns the mutation result after atomically merging the exact session slice, or an explicit error.
/// 原子合并精确会话切片后返回变更结果，或返回显式错误。
pub(crate) fn mutate_latest_workspace_for_existing_session<T, F>(
    state: &SharedServiceBridgeState,
    fallback_workspace: Option<WorkspaceSnapshot>,
    session_id: Uuid,
    mutation: F,
) -> Result<T, String>
where
    F: FnOnce(&mut WorkspaceSnapshot) -> Result<T, String>,
{
    // guard prevents stream projection from advancing the exact retained session during slice capture, mutation, and commit.
    // guard 防止流式投影在切片捕获、变更与提交期间推进精确保留会话。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // session_slice copies only queue metadata for the exact retained session; canonical history is detached and restored before mutation.
    // session_slice 仅复制精确保留会话的队列元数据；规范历史会在变更前摘下并恢复。
    let mut session_slice = match guard.workspace_snapshot.as_mut() {
        Some(current_workspace) => {
            existing_queue_mutation_workspace_slice(current_workspace, session_id)?
        }
        None => fallback_workspace.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?,
    };
    // result is produced before publication so a failed mutation cannot leave a partial workspace commit.
    // result 在发布前生成，确保变更失败不会留下部分工作区提交。
    let result = mutation(&mut session_slice)?;
    match guard.workspace_snapshot.as_mut() {
        Some(retained_workspace) => merge_existing_session_slice_into_retained_workspace(
            retained_workspace,
            session_slice,
            session_id,
        )?,
        None => guard.workspace_snapshot = Some(session_slice),
    }
    Ok(result)
}

/// Builds a queue-mutation slice without cloning canonical conversation history.
/// 构建不复制规范会话历史的队列变更切片。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained authoritative workspace whose exact runtime history is borrowed temporarily.
/// `workspace_snapshot` 是权威保留工作区，其精确运行时历史会被临时借出。
/// `session_id` identifies the sole queue session copied into the mutation slice.
/// `session_id` 标识复制到变更切片中的唯一队列会话。
///
/// # Returns
/// 返回值
/// A detached metadata slice with an empty canonical conversation state, or an exact retained-state error.
/// 返回规范会话状态为空的分离元数据切片，或精确保留状态错误。
fn existing_queue_mutation_workspace_slice(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: Uuid,
) -> Result<WorkspaceSnapshot, String> {
    // Queue mutations in pending_prompts, pending_prompt_services, and pending_dispatch access only UI queue state,
    // timestamps, phase, summary, and newly admitted transcript records; none reads canonical conversation_state.
    // pending_prompts、pending_prompt_services 与 pending_dispatch 的队列变更只访问界面队列状态、
    // 时间戳、阶段、摘要与新接纳转录记录；均不读取规范 conversation_state。
    // conversation_state is borrowed by ownership so cloning the remaining runtime metadata stays bounded.
    // conversation_state 通过所有权临时借出，使其余运行态元数据的复制保持有界。
    let Some(conversation_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .map(|runtime_state| std::mem::take(&mut runtime_state.conversation_state))
    else {
        // Runtime state is schema-optional for legacy or not-yet-hydrated rows; metadata-only mutations preserve that absence.
        // 对旧版或尚未水合的记录而言运行态在 Schema 中可缺失；仅元数据变更会保留该缺失状态。
        return existing_submission_workspace_slice(workspace_snapshot, session_id);
    };
    // slice_result is captured before restoration so both success and error paths restore the exact canonical state.
    // slice_result 在恢复前捕获，使成功与错误路径都会恢复精确规范状态。
    let slice_result = existing_submission_workspace_slice(workspace_snapshot, session_id);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Queued conversation `{session_id}` lost its runtime state."))?;
    runtime_state.conversation_state = conversation_state;
    slice_result
}

/// Moves one exact queue-session slice into the retained workspace without touching unrelated state.
/// 把一个精确排队会话切片移入保留工作区，且不触碰无关状态。
///
/// # Parameters
/// 参数
/// `retained_workspace` is the latest authoritative full workspace.
/// `retained_workspace` 是最新权威完整工作区。
/// `session_slice` owns only the mutated target session and its exact owned maps.
/// `session_slice` 仅持有已变更目标会话及其精确所属映射。
/// `session_id` identifies the only session scope allowed to merge.
/// `session_id` 标识唯一允许合并的会话范围。
///
/// # Returns
/// 返回值
/// Returns success after ownership transfer, otherwise an explicit integrity error.
/// 所有权转移完成后返回成功，否则返回明确的完整性错误。
fn merge_existing_session_slice_into_retained_workspace(
    retained_workspace: &mut WorkspaceSnapshot,
    mut session_slice: WorkspaceSnapshot,
    session_id: Uuid,
) -> Result<(), String> {
    let session_position = session_slice
        .sessions
        .iter()
        .position(|session| session.id == session_id)
        .ok_or_else(|| format!("queued session slice is missing session `{session_id}`"))?;
    let session = session_slice.sessions.remove(session_position);
    let retained_session = retained_workspace
        .sessions
        .iter_mut()
        .find(|candidate| candidate.id == session_id)
        .ok_or_else(|| format!("queued session `{session_id}` left retained state"))?;
    *retained_session = session;
    if let Some(messages) = session_slice.session_messages.remove(&session_id) {
        merge_submission_message_entries(
            retained_workspace
                .session_messages
                .entry(session_id)
                .or_default(),
            messages,
        );
    }
    // Queue slices omit presentation history, so absence preserves retained compaction state.
    // 队列切片会省略展示历史，因此缺少条目时保留已有压缩状态。
    if let Some(compactions) = session_slice.session_compactions.remove(&session_id) {
        retained_workspace
            .session_compactions
            .insert(session_id, compactions);
    }
    if let Some(mut runtime_state) = session_slice.session_runtime_state.remove(&session_id) {
        // retained_conversation_state is moved directly into the incoming metadata row without allocation or cloning.
        // retained_conversation_state 无分配、无复制地直接移入传入元数据记录。
        let retained_conversation_state = retained_workspace
            .session_runtime_state
            .get_mut(&session_id)
            .map(|retained_runtime_state| {
                std::mem::take(&mut retained_runtime_state.conversation_state)
            })
            .ok_or_else(|| format!("queued session `{session_id}` lost retained runtime state"))?;
        runtime_state.conversation_state = retained_conversation_state;
        retained_workspace
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    replace_exact_session_map_entry(
        &mut retained_workspace.session_run_phases,
        &mut session_slice.session_run_phases,
        session_id,
    );
    if session_slice.hydrated_session_ids.contains(&session_id) {
        retained_workspace.hydrated_session_ids.insert(session_id);
    } else {
        retained_workspace.hydrated_session_ids.remove(&session_id);
    }
    Ok(())
}

/// Replaces or removes one exact session-keyed entry by moving it from a detached slice.
/// 通过从分离切片移动条目来替换或移除一个精确会话键条目。
///
/// # Parameters
/// 参数
/// `retained` is the authoritative full-workspace map.
/// `retained` 是权威完整工作区映射。
/// `session_slice` is the detached one-session map.
/// `session_slice` 是分离的单会话映射。
/// `session_id` is the only key allowed to change.
/// `session_id` 是唯一允许变化的键。
///
/// # Returns
/// 返回值
/// This function returns no value.
/// 此函数无返回值。
fn replace_exact_session_map_entry<T>(
    retained: &mut std::collections::BTreeMap<Uuid, T>,
    session_slice: &mut std::collections::BTreeMap<Uuid, T>,
    session_id: Uuid,
) {
    if let Some(value) = session_slice.remove(&session_id) {
        retained.insert(session_id, value);
    } else {
        retained.remove(&session_id);
    }
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
    // activity reads the stream owner and authoritative kernel turn from one revision.
    // activity 从同一版本读取活动流所有者与权威内核回合。
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
        has_active_stream_task,
        has_active_kernel_turn,
        has_precheck_progress,
    );
    webui_submit_trace(
        "submit_prompt_blocking_check",
        format!(
            "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_session_run={} has_running_subagents=false elapsed_ms={}",
            has_active_stream_task || has_active_kernel_turn,
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
        client_submission_id,
        workspace_snapshot,
        session_id,
        prompt,
        hidden_model_guidance,
        attachments,
        attachment_draft_scope,
        reasoning_effort,
        model_selection,
        submit_total_started_at,
    } = input;
    let attachments_for_draft_release = attachments.clone();
    let attachment_draft_scope_for_release = attachment_draft_scope.clone();
    webui_submit_trace(
        "submit_prompt_queued",
        format!("session={session_id} reason=blocking_system_prompt_work"),
    );
    // queue_commit mutates the latest shared snapshot so a stale preflight clone cannot erase streamed output.
    // queue_commit 修改最新共享快照，避免过期预检副本擦除流式输出。
    let queued_hook_event = mutate_latest_workspace_for_existing_session(
        state,
        Some(workspace_snapshot),
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
                model_selection.as_ref(),
            )?;
            Ok(queued_hook_event)
        },
    )?;
    if let Some(draft_scope) = attachment_draft_scope_for_release
        .as_deref()
        .filter(|scope| !scope.trim().is_empty())
    {
        // queueScope was acquired before the workspace row became authoritative, so releasing the composer owner cannot orphan assets.
        // queueScope 在工作区行成为权威状态前已经获取，因此释放输入区所有者不会使资产变成孤儿。
        if let Err(error) = release_staged_attachment_ref(
            queued_hook_event.workspace_path(),
            draft_scope,
            &attachments_for_draft_release,
        ) {
            webui_trace(
                "queued_composer_attachment_draft_release_failed",
                format!("session={session_id} error={error}"),
            );
        }
    }
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
    // Queue admission changes runtime metadata only; a full transcript persist could overtake the active ordered writer.
    // 队列接纳只修改运行态元数据；完整转录持久化可能抢先越过活动的顺序写入器。
    spawn_workspace_session_runtime_state_persist_task(
        state.clone(),
        controller_runtime.clone(),
        session_id,
        Some(String::from("queued_submit")),
    );
    webui_submit_trace(
        "submit_prompt_queued_returning",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    Ok(SubmitConversationPromptAck {
        client_submission_id,
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
