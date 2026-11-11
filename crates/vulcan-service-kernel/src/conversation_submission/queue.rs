//! Existing-session submission queue decision and commit.
//! 已有会话提交排队决策与提交。

use vulcan_agent_runtime::attachment_asset_index::{
    acquire_staged_attachment_ref, release_staged_attachment_ref, transfer_staged_attachment_ref,
};
use vulcan_core::types::ConversationAttachment;
use vulcan_session_core::v1::{EnqueueInputDisposition, EnqueueInputReceipt};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::*;
use crate::pending_prompts::{
    dispatch_queued_pending_prompt_hook, pending_prompt_attachment_staging_scope,
};
use crate::session_runtime_registry_v1::{SessionConditionalMutationReceipt, SessionHandle};
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
    /// Exact V1 handle loaded through the indexed session locator before queue routing.
    /// 在队列路由前通过索引会话定位器加载的精确 V1 句柄。
    pub(crate) v1_handle: Arc<SessionHandle>,
    /// Authoritative client input already resolved to ordered project resource ids.
    /// 已解析为有序项目资源 ID 的权威客户端输入。
    pub(crate) v1_submission: ClientInputSubmission,
    /// Exact existing session that owns the queue.
    /// 持有队列的精确已有会话。
    pub(crate) session_id: Uuid,
    /// Authoritative project workspace path resolved before queue admission.
    /// 在队列准入前解析的权威项目工作区路径。
    pub(crate) workspace_path: String,
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

/// Transfers a committed queue item's managed assets before releasing its composer owner.
/// 在释放输入框所有者之前转交已提交队列项的托管资产。
///
/// # Parameters
/// 参数
/// `workspace_path` selects the exact asset index, `session_id` and `client_action_id` identify
/// the durable queue owner, `currently_queued` distinguishes a queued input from a directly
/// scheduled turn, and `attachments` with `draft_scope` identify the original composer lease.
/// `workspace_path` 选择精确资产索引，`session_id` 与 `client_action_id` 标识可靠队列所有者，
/// `currently_queued` 区分排队输入和直接调度的回合，`attachments` 与 `draft_scope` 标识原输入框租约。
///
/// # Returns
/// 返回值
/// Success after the ownership handoff, or an explicit error that retains the composer owner if
/// the queue owner could not be acquired.
/// 所有权转交后返回成功；若队列所有者无法取得租约则返回明确错误并保留输入框所有者。
fn transfer_committed_queue_attachment_lease(
    workspace_path: &str,
    session_id: SessionId,
    client_action_id: &str,
    currently_queued: bool,
    attachments: &[ConversationAttachment],
    draft_scope: Option<&str>,
) -> Result<(), String> {
    let draft_scope = draft_scope.filter(|scope| !scope.trim().is_empty());
    if currently_queued {
        let queue_scope = pending_prompt_attachment_staging_scope(session_id, client_action_id)?;
        if let Some(draft_scope) = draft_scope {
            transfer_staged_attachment_ref(workspace_path, draft_scope, &queue_scope, attachments)?;
        } else {
            acquire_staged_attachment_ref(workspace_path, &queue_scope, attachments)?;
        }
    }
    if !currently_queued && let Some(draft_scope) = draft_scope {
        release_staged_attachment_ref(workspace_path, draft_scope, attachments)?;
    }
    Ok(())
}

/// Durably commits one existing-session queue input and verifies the exact stored payload.
/// 可靠提交一条已有会话队列输入，并核验精确落库载荷。
///
/// # Parameters
/// 参数
/// `handle` is the exact indexed V1 session owner, `submission` carries the browser action,
/// prompt, and ordered resource ids, and `now_ms` is the single admission timestamp.
/// `handle` 是精确索引定位的 V1 会话 owner；`submission` 携带浏览器动作、提示词与
/// 有序资源 ID；`now_ms` 是唯一准入时间戳。
///
/// # Returns
/// 返回值
/// Returns the complete durable Core receipt after validating the disposition-specific state.
/// A newly queued input must exist in metadata, while an idempotent historical admission may
/// already have a durable move/remove resolution and a superseding admission is already an event.
/// 在校验处置类型对应状态后返回完整可靠 Core 回执。新排队输入必须存在于 metadata；
/// 幂等历史准入可能已经存在可靠的移动或删除处置；取代 Goal 的准入则已经成为事件。
pub(crate) fn commit_existing_submission_v1_queue(
    handle: &Arc<SessionHandle>,
    submission: &ClientInputSubmission,
    now_ms: i64,
) -> Result<SessionConditionalMutationReceipt<EnqueueInputReceipt>, String> {
    let receipt = handle
        .enqueue_input(submission.clone(), now_ms)
        .map_err(|error| format!("failed to durably queue V1 submission: {error}"))?;
    let queued_input = handle
        .read(|document| {
            document
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .find(|input| input.input_id == receipt.value.input_id)
                .cloned()
        })
        .map_err(|error| format!("failed to verify durable V1 queue input: {error}"))?;
    match (&receipt.value.disposition, queued_input) {
        (EnqueueInputDisposition::Queued, Some(queued_input)) => {
            if queued_input.client_action_id != submission.client_action_id
                || queued_input.text != submission.text
                || queued_input.resource_ids != submission.resource_ids
                || queued_input.bound_request_config != submission.bound_request_config
            {
                return Err(format!(
                    "durable V1 queue input {} does not match the submitted client action",
                    receipt.value.input_id.value()
                ));
            }
        }
        (EnqueueInputDisposition::Queued, None) if receipt.changed => {
            return Err(format!(
                "new durable V1 queue receipt references missing input {}",
                receipt.value.input_id.value()
            ));
        }
        (EnqueueInputDisposition::Queued, None) => {
            // A no-change receipt may describe an admission that was later moved or removed.
            // 无变化回执可能描述一条后来已被移动或删除的历史准入。
        }
        (EnqueueInputDisposition::SupersededContinuationAndScheduled { .. }, Some(_)) => {
            return Err(format!(
                "scheduled V1 admission unexpectedly remains queued as input {}",
                receipt.value.input_id.value()
            ));
        }
        (EnqueueInputDisposition::SupersededContinuationAndScheduled { .. }, None) => {}
    }
    Ok(receipt)
}

/// Reads whether one exact durable admission is still present in the current V1 metadata queue.
/// 读取一条精确可靠准入当前是否仍存在于 V1 metadata 队列中。
///
/// # Parameters
/// 参数
/// `handle` is the exact session owner and `receipt` supplies both durable admission identities.
/// `handle` 是精确会话 owner；`receipt` 提供两个可靠准入身份。
///
/// # Returns
/// 返回值
/// Returns true only for the exact current metadata row; historical or scheduled admissions are false.
/// 仅精确匹配当前 metadata 行时返回 true；历史准入或已调度准入返回 false。
fn v1_admission_is_currently_queued(
    handle: &Arc<SessionHandle>,
    receipt: &EnqueueInputReceipt,
) -> Result<bool, String> {
    handle
        .read(|document| {
            document
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .any(|input| {
                    input.input_id == receipt.input_id
                        && input.client_action_id == receipt.client_action_id
                })
        })
        .map_err(|error| format!("failed to read current V1 queue visibility: {error}"))
}

/// Reads the current process-local acknowledgement projection without publishing a new delta.
/// 读取当前进程内确认投影，且不发布新的增量。
///
/// # Parameters
/// 参数
/// `state` owns the current WebUI projection and `session_id` selects its exact run phase.
/// `state` 持有当前 WebUI 投影；`session_id` 选择其精确运行阶段。
///
/// # Returns
/// 返回值
/// Returns the current workbench revision and run phase, or an explicit state-lock error.
/// 返回当前工作台修订号与运行阶段，或明确的状态锁错误。
pub(super) fn current_queued_submission_ack_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(u64, SessionRunPhase), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let run_phase = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_run_phases.get(&session_id).copied())
        .unwrap_or(SessionRunPhase::Idle);
    Ok((guard.workbench_generation, run_phase))
}

/// Builds one queued acknowledgement from the authoritative client action and current UI state.
/// 使用权威客户端动作与当前界面状态构建一条排队确认。
///
/// # Parameters
/// 参数
/// `client_submission_id` is the exact durable action id, `session_id` is the queue owner,
/// and `revision` plus `run_phase` describe the current process-local projection.
/// `client_submission_id` 是精确可靠动作 ID；`session_id` 是队列 owner；`revision` 与
/// `run_phase` 描述当前进程内投影。
///
/// # Returns
/// 返回值
/// Returns a queued RPC acknowledgement without creating any additional fact.
/// 返回不创建任何额外事实的排队 RPC 确认。
pub(super) fn submission_admission_ack(
    client_submission_id: String,
    session_id: SessionId,
    revision: u64,
    run_phase: SessionRunPhase,
    queued: bool,
    turn_id: Option<String>,
) -> SubmitConversationPromptAck {
    SubmitConversationPromptAck {
        client_submission_id,
        conversation_id: session_id.to_string(),
        accepted_message_id: None,
        turn_id,
        run_phase: run_phase.to_string(),
        // queued reflects the current V1 metadata state, not the historical admission route.
        // queued 反映当前 V1 metadata 状态，而不是历史准入路径。
        queued,
        queued_turn_dispatch: None,
        revision,
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
/// `v1_handle` is the exact durable session owner whose document is the sole queue occupancy
/// authority; an existing session without it can never be judged immediately admissible.
/// `v1_handle` 是精确可靠会话 owner，其文档是唯一的队列占用权威；缺少它的已有会话
/// 绝不能被判定为可即时提交。
///
/// # Returns
/// # 返回值
/// A strong immediate-or-queue routing decision, or an explicit state lock error.
/// 返回强类型即时或排队路由决策，或显式状态锁错误。
pub(crate) fn evaluate_existing_submission_queue(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    submission_session_id: Option<Uuid>,
    v1_handle: Option<&Arc<SessionHandle>>,
    submission_mode: &ConversationSubmissionMode,
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
    // durable_queue_started_at isolates the authoritative V1 queue occupancy read.
    // durable_queue_started_at 隔离权威 V1 队列占用读取部分。
    let durable_queue_started_at = current_unix_millis_value();
    // Queue occupancy and child obligations share one durable document revision. A missing
    // runtime mirror cannot unlock a new root while the open parent still owns child results.
    // 队列占用与子任务义务来自同一可靠文档版本；运行时镜像缺失不能放行仍拥有子结果的父回合替换。
    let (has_queued_inputs, has_unsettled_child_tasks) = match v1_handle {
        Some(handle) => handle
            .read(|document| {
                (
                    !document.metadata.input_state.queued_inputs.is_empty(),
                    document.active_turn().is_some_and(|turn| {
                        !turn.closed && document.has_unsettled_collaboration(Some(turn.turn_id))
                    }),
                )
            })
            .map_err(|error| format!("failed to read durable V1 submission ownership: {error}"))?,
        None => {
            return Err(format!(
                "existing session `{session_id}` submission requires its exact V1 document authority"
            ));
        }
    };
    webui_trace_if_slow(
        "submit_prompt_durable_queue_check_slow",
        durable_queue_started_at,
        format!(
            "session={session_id} has_queued_inputs={has_queued_inputs} has_unsettled_child_tasks={has_unsettled_child_tasks}"
        ),
    );
    // activity reads the stream owner and authoritative kernel turn from one revision.
    // activity 从同一版本读取活动流所有者与权威内核回合。
    let (has_active_stream_task, has_active_kernel_turn, has_waiting_scheduled_turn) = {
        // guard keeps both activity diagnostics on one authoritative service-state revision.
        // guard 使两个活动诊断保持在同一个权威服务状态版本上。
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.active_stream_tasks.contains_key(&session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            guard.waiting_queued_turns.contains_key(&session_id),
        )
    };
    webui_trace_if_slow(
        "submit_prompt_active_state_check_slow",
        active_state_started_at,
        format!("session={session_id}"),
    );
    // has_precheck_progress is the retained precheck ownership marker consumed by pending-lane rules.
    // has_precheck_progress 是待发送通道规则消费的保留 precheck 所有权标记。
    let has_precheck_progress = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_precheck_turn_id.as_deref())
        .is_some_and(|turn_id| !turn_id.trim().is_empty());
    // should_queue also preserves the current parent's exact child settlement ownership.
    // should_queue 同时保留当前父回合精确的子任务结算归属。
    // The parked accepted root keeps its place until it can take ownership of dispatch.
    // 已接收且等待派发的根回合保留原顺序，直至取得派发所有权。
    let should_queue = has_waiting_scheduled_turn
        || (submission_mode.appends_user_records() && has_unsettled_child_tasks)
        || session_has_blocking_system_prompt_work(
            workspace_snapshot,
            session_id,
            has_queued_inputs,
            has_active_stream_task,
            has_active_kernel_turn,
            has_precheck_progress,
        );
    webui_submit_trace(
        "submit_prompt_blocking_check",
        format!(
            "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_session_run={} has_unsettled_child_tasks={has_unsettled_child_tasks} elapsed_ms={}",
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
/// `state` owns only the process-local compatibility projection published after V1 commit.
/// `state` 仅持有在 V1 提交后发布的进程内兼容投影。
/// `agent_runtime` supplies the exact queued prompt runtime options.
/// `agent_runtime` 提供精确的排队提示词运行时选项。
/// `input` owns the exact V1 handle and submission plus compatibility display fields.
/// `input` 持有精确 V1 句柄与提交，以及兼容显示字段。
///
/// # Returns
/// # 返回值
/// Returns the queued acknowledgement after the V1 mutation is durable; compatibility delta
/// failures are logged without creating or revoking durable queue facts.
/// 在 V1 修改可靠落库后返回排队确认；兼容增量失败只记录日志，不创建或撤销可靠队列事实。
pub(crate) fn queue_existing_submission(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    input: ExistingSubmissionQueueInput,
) -> Result<SubmitConversationPromptAck, String> {
    // input fields are destructured once so the durable queue and compatibility mirror cannot
    // mix values from different submissions.
    // input 字段只解构一次，确保可靠队列与兼容镜像不会混用不同提交的值。
    let ExistingSubmissionQueueInput {
        v1_handle,
        v1_submission,
        session_id,
        workspace_path,
        hidden_model_guidance,
        attachments,
        attachment_draft_scope,
        reasoning_effort,
        model_selection,
        submit_total_started_at,
    } = input;
    let client_submission_id = v1_submission.client_action_id.as_str().to_string();
    let prompt = v1_submission.text.clone();
    let admitted_at_ms = i64::try_from(current_unix_millis_value())
        .map_err(|_| String::from("current queue admission time exceeds the V1 timestamp range"))?;

    // V1 metadata is the sole durable authority. No compatibility UI or Hook state is exposed
    // before this flush succeeds.
    // V1 metadata 是唯一可靠权威；在该 flush 成功前，不暴露任何兼容界面或 Hook 状态。
    let durable_receipt =
        commit_existing_submission_v1_queue(&v1_handle, &v1_submission, admitted_at_ms)?;
    let currently_queued = v1_admission_is_currently_queued(&v1_handle, &durable_receipt.value)?;
    let scheduled_turn_id = match &durable_receipt.value.disposition {
        EnqueueInputDisposition::Queued => None,
        EnqueueInputDisposition::SupersededContinuationAndScheduled {
            scheduled_turn_id, ..
        } => Some(scheduled_turn_id.value().to_string()),
    };
    if !durable_receipt.changed {
        // A duplicate admission can be the first opportunity to finish a lease handoff that
        // failed after the original V1 commit. Only a still-queued row may regain queue ownership.
        // 重复准入可能是原 V1 提交后租约转交失败的首次修复机会；仅仍在队列中的记录可重新取得队列所有权。
        if currently_queued
            && let Err(error) = transfer_committed_queue_attachment_lease(
                &workspace_path,
                session_id,
                client_submission_id.as_str(),
                true,
                &attachments,
                attachment_draft_scope.as_deref(),
            )
        {
            webui_trace(
                "queued_attachment_lease_handoff_retry_failed",
                format!("session={session_id} error={error}"),
            );
        }
        let (revision, run_phase) = current_queued_submission_ack_state(state, session_id)?;
        webui_submit_trace(
            "submit_prompt_admission_idempotent",
            format!(
                "session={session_id} client_submission_id={client_submission_id} v1_revision={} currently_queued={currently_queued}",
                durable_receipt.revision,
            ),
        );
        return Ok(submission_admission_ack(
            client_submission_id,
            session_id,
            revision,
            run_phase,
            currently_queued,
            scheduled_turn_id,
        ));
    }

    webui_submit_trace(
        "submit_prompt_admitted",
        format!(
            "session={session_id} reason=blocking_system_prompt_work v1_revision={}",
            durable_receipt.revision
        ),
    );
    // The durable V1 queue owns this handoff even when its process-local UI mirror later fails.
    // 即使进程内 UI 镜像随后失败，可靠 V1 队列仍持有这次所有权转交。
    if let Err(error) = transfer_committed_queue_attachment_lease(
        &workspace_path,
        session_id,
        client_submission_id.as_str(),
        currently_queued,
        &attachments,
        attachment_draft_scope.as_deref(),
    ) {
        webui_trace(
            "queued_attachment_lease_handoff_failed_after_v1_commit",
            format!("session={session_id} error={error}"),
        );
    }

    if !currently_queued {
        let scheduled_turn_id = scheduled_turn_id
            .ok_or_else(|| String::from("changed V1 admission is neither queued nor scheduled"))?;
        let (revision, run_phase) = current_queued_submission_ack_state(state, session_id)?;
        webui_submit_trace(
            "submit_prompt_goal_superseded",
            format!(
                "session={session_id} client_submission_id={client_submission_id} v1_revision={} turn_id={}",
                durable_receipt.revision, scheduled_turn_id
            ),
        );
        return Ok(submission_admission_ack(
            client_submission_id,
            session_id,
            revision,
            run_phase,
            false,
            Some(scheduled_turn_id),
        ));
    }

    // compatibilityMirror is process-local only. A failure here cannot revoke the already
    // durable V1 admission or create a second persistent authority.
    // compatibilityMirror 仅属于进程内兼容层；此处失败不能撤销已经可靠落库的 V1 准入，
    // 也不能创建第二个持久化权威。
    let queued_hook_event = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .and_then(|mut guard| {
            enqueue_webui_pending_prompt_from_submission(
                &mut guard,
                agent_runtime,
                session_id,
                &workspace_path,
                client_submission_id.as_str(),
                prompt,
                hidden_model_guidance,
                attachments,
                reasoning_effort.as_deref(),
                model_selection.as_ref(),
            )
        });

    let queued_hook_event = match queued_hook_event {
        Ok(event) => event,
        Err(error) => {
            webui_trace(
                "queued_submission_ui_mirror_failed_after_v1_commit",
                format!("session={session_id} error={error}"),
            );
            let (revision, run_phase) = current_queued_submission_ack_state(state, session_id)?;
            return Ok(submission_admission_ack(
                client_submission_id,
                session_id,
                revision,
                run_phase,
                true,
                None,
            ));
        }
    };
    if let Err(error) =
        dispatch_queued_pending_prompt_hook(&v1_handle, agent_runtime, queued_hook_event)
    {
        webui_trace(
            "queued_submission_hook_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    if let Err(error) = emit_pending_prompts_changed_delta(app, state, session_id) {
        webui_trace(
            "queued_submission_ui_delta_failed_after_v1_commit",
            format!("session={session_id} error={error}"),
        );
    }
    let (revision, run_phase) = match emit_queued_prompt_run_phase_delta(app, state, session_id) {
        Ok(state) => state,
        Err(error) => {
            webui_trace(
                "queued_submission_phase_delta_failed_after_v1_commit",
                format!("session={session_id} error={error}"),
            );
            current_queued_submission_ack_state(state, session_id)?
        }
    };
    webui_submit_trace(
        "submit_prompt_queued_returning",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    Ok(submission_admission_ack(
        client_submission_id,
        session_id,
        revision,
        run_phase,
        true,
        None,
    ))
}

#[cfg(test)]
mod attachment_lease_tests {
    use super::*;
    use vulcan_agent_runtime::{
        attachment_asset_index::load_attachment_asset_record_for_attachment,
        attachment_assets::{attachment_descriptor_from_mime_type, stage_attachment_bytes},
    };

    /// A committed queue keeps its asset when the composer owner is released.
    /// 已提交队列在输入框所有者释放后仍保留其资产。
    #[test]
    fn committed_queue_acquires_lease_before_releasing_composer() {
        let workspace = tempfile::tempdir().expect("isolated workspace");
        let workspace_path = workspace.path().to_string_lossy().into_owned();
        let descriptor =
            attachment_descriptor_from_mime_type("image/png").expect("supported image attachment");
        let attachment = stage_attachment_bytes(
            &workspace_path,
            &descriptor,
            "queued.png",
            None,
            &[1, 2, 3, 4],
        )
        .expect("stage managed attachment");
        let draft_scope = "composer-draft:conversation:queued-test";
        acquire_staged_attachment_ref(
            &workspace_path,
            draft_scope,
            std::slice::from_ref(&attachment),
        )
        .expect("composer owns staged asset");
        let session_id = SessionId::new_v4();
        transfer_committed_queue_attachment_lease(
            &workspace_path,
            session_id,
            "same-action",
            true,
            std::slice::from_ref(&attachment),
            Some(draft_scope),
        )
        .expect("queue takes ownership before composer release");
        let record = load_attachment_asset_record_for_attachment(&attachment)
            .expect("queue still owns the indexed asset");
        assert_eq!(
            record.staging_refs,
            vec![format!("pending-prompt:v2:{session_id}:same-action")]
        );
        assert!(std::path::Path::new(&attachment.local_path).is_file());
    }

    /// A rejected queue identity never releases the existing composer owner.
    /// 队列身份无效时绝不释放已有输入框所有者。
    #[test]
    fn failed_queue_acquisition_retains_composer_lease() {
        let workspace = tempfile::tempdir().expect("isolated workspace");
        let workspace_path = workspace.path().to_string_lossy().into_owned();
        let descriptor =
            attachment_descriptor_from_mime_type("image/png").expect("supported image attachment");
        let attachment = stage_attachment_bytes(
            &workspace_path,
            &descriptor,
            "retained.png",
            None,
            &[4, 3, 2, 1],
        )
        .expect("stage managed attachment");
        let draft_scope = "composer-draft:conversation:retained-test";
        acquire_staged_attachment_ref(
            &workspace_path,
            draft_scope,
            std::slice::from_ref(&attachment),
        )
        .expect("composer owns staged asset");
        assert!(
            transfer_committed_queue_attachment_lease(
                &workspace_path,
                SessionId::new_v4(),
                "invalid\naction",
                true,
                std::slice::from_ref(&attachment),
                Some(draft_scope),
            )
            .is_err()
        );
        let record = load_attachment_asset_record_for_attachment(&attachment)
            .expect("composer asset remains indexed");
        assert_eq!(record.staging_refs, vec![draft_scope]);
        assert!(std::path::Path::new(&attachment.local_path).is_file());
    }

    /// One missing asset rejects the whole transfer before changing another asset's owner.
    /// 一项缺失资产会在改变其他资产所有者之前拒绝整次转交。
    #[test]
    fn queue_transfer_validates_all_assets_before_publishing() {
        let workspace = tempfile::tempdir().expect("isolated workspace");
        let workspace_path = workspace.path().to_string_lossy().into_owned();
        let descriptor =
            attachment_descriptor_from_mime_type("image/png").expect("supported image attachment");
        let attachment = stage_attachment_bytes(
            &workspace_path,
            &descriptor,
            "valid.png",
            None,
            &[8, 7, 6, 5],
        )
        .expect("stage managed attachment");
        let draft_scope = "composer-draft:queue-atomic-reject";
        acquire_staged_attachment_ref(
            &workspace_path,
            draft_scope,
            std::slice::from_ref(&attachment),
        )
        .expect("composer owns staged asset");
        let mut missing = attachment.clone();
        missing.asset_id = Some(String::from("missing-index-record"));

        assert!(
            transfer_committed_queue_attachment_lease(
                &workspace_path,
                SessionId::new_v4(),
                "atomic-reject",
                true,
                &[attachment.clone(), missing],
                Some(draft_scope),
            )
            .is_err()
        );
        assert_eq!(
            load_attachment_asset_record_for_attachment(&attachment)
                .expect("original asset must remain indexed")
                .staging_refs,
            vec![String::from(draft_scope)]
        );
    }
}
