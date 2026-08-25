//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crate::{
    apply_output_style_snapshot, require_segmented_session_directory_for_retained_session,
    resolve_output_style_snapshot, segmented_session_directory_for_workspace_session,
};
use uuid::Uuid;
use vulcan_agent_runtime::attachment_asset_index::promote_staged_attachment_to_message_ref;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnStart,
    begin_shared_prompt_turn, restore_shared_prompt_turn_from_execution_retry,
};
use vulcan_service_api::{
    ComposerAttachmentSummary, ServiceEvent, ServiceEventEmitter, SubmitConversationPromptAck,
    UpdateConversationRunPhaseDelta, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionKnowledgeLibrarySelection,
    SessionKnowledgeLibrarySelectionMode, SessionPendingModelSelection,
    SessionProviderToolTargetSelection, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::{
    ControllerRuntimeHandle, ExistingSubmissionAdmissionAuthorization,
    PromptSessionMaterializationKind, ServiceBridgeState, SessionContinuationRuntimeBlockers,
    SharedServiceBridgeState, UserPromptEditSubmitRequest, active_kernel_session_ids,
    advance_session_goal_dispatch_version,
    apply_webui_prompt_begin_outer_steps_deferred_context_persist,
    await_session_transcript_writer_durable, begin_existing_prompt_submission_exclusion,
    build_conversation_messages_patched_delta, capture_preserved_workspace_run_state,
    capture_webui_prompt_preparation_generation, capture_webui_prompt_preparation_workspace,
    commit_prepared_submission_media_ledger, composer_attachment_draft_scope,
    composer_attachment_summary_to_core, composer_attachment_summary_to_provider_core,
    composer_attachment_workspace_path_from_snapshot, conversation_messages_from_session,
    current_unix_millis_text, current_unix_millis_value,
    emit_conversation_continuation_summary_changed, emit_materialized_session_message_delta,
    emit_pending_prompts_changed_delta, emit_service_event_with_trace,
    enqueue_webui_pending_prompt_from_submission, hydrate_retained_submission_session_if_needed,
    is_active_session_run_phase, load_workspace_for_submission,
    mark_webui_prompt_preparation_failed, mark_webui_prompt_snapshot_preparing,
    materialize_prompt_session, owning_project_id_for_session, parse_prompt_session_target,
    parse_session_id, persist_initial_submission_goal,
    persist_latest_retained_session_runtime_state_update,
    persist_latest_retained_workspace_session_update, persist_workspace_session_update,
    prepare_submission_media, preserve_active_session_run_state, provider_tool_capability_key,
    refresh_exact_vmm_hook_session_projection, replace_conversation_messages_in_workbench_window,
    resolve_composer_attachment_summary_for_workspace, resolve_session_continuation_eligibility,
    retain_slow_submission_diagnostic, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, rollback_persisted_session_before_user_prompt,
    runtime_parts_from_state, session_continuation_runtime_blockers,
    session_has_blocking_system_prompt_work, session_process_public_ai_selection,
    snapshot_webui_context_before_user_prompt, spawn_exact_session_transcript_entries_persist_task,
    spawn_service_task, spawn_workspace_session_persist_task,
    spawn_workspace_session_runtime_state_persist_task,
    start_reserved_webui_session_kernel_user_turn, start_webui_precheck_or_continue,
    start_webui_session_kernel_user_turn, sync_hydrated_session_dialogue_ready_marker,
    validate_composer_attachment_summary, webui_session_allows_user_prompt_editing,
    webui_session_kernel_has_active_turn, webui_shared_prompt_precheck_plan, webui_submit_trace,
    webui_trace_if_slow, workbench_with_live_pending_question, workspace_path_for_session,
    workspace_session_summary_for_session, workspace_user_prompt_message_allows_destructive_edit,
};
mod activation;
mod edit;
mod preflight;
mod queue;
mod runtime_options;

/// Stable blocked reason used when the digest-bound retry sidecar cannot be read or validated.
/// 摘要绑定的重试 sidecar 无法读取或校验时使用的稳定阻断原因。
pub(crate) const EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_REASON: &str =
    "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE";

/// Controlled diagnostic retained after an unreadable retry sidecar is invalidated.
/// 不可读取的重试 sidecar 被失效后保留的受控诊断信息。
pub(crate) const EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_SUMMARY: &str =
    "The retained execution retry sidecar is missing, unreadable, or failed integrity validation.";

/// Controlled diagnostic retained after a retry body no longer matches its authoritative session.
/// 重试正文不再匹配权威会话后保留的受控诊断信息。
const EXECUTION_RETRY_SNAPSHOT_CLAIM_REJECTED_SUMMARY: &str =
    "The retained execution retry snapshot no longer matches the authoritative conversation state.";

/// Stable error returned while the same Continue claim has not crossed its durable admission boundary.
/// 同一 Continue 领取尚未跨过持久准入边界时返回的稳定错误。
const CONTINUATION_CLAIM_IN_FLIGHT_REASON: &str = "ERR_CONVERSATION_CONTINUE_CLAIM_IN_FLIGHT";

/// Defers retirement of one consumed durable retry file until the replacement runtime state is durable.
/// 将一个已消费可靠重试文件的退役操作延迟到替代运行态可靠落盘之后。
#[derive(Debug)]
pub(crate) struct DeferredConsumedExecutionRetryCleanup {
    /// Exact segmented session directory containing the single fixed retry file.
    /// 包含唯一固定重试文件的精确分段会话目录。
    session_directory: PathBuf,
    /// Digest-bound reference that alone authorizes removal of the consumed file.
    /// 唯一有权删除已消费文件的摘要绑定引用。
    reference: vulcan_workbench_model::ExecutionRetrySnapshotRef,
}

impl DeferredConsumedExecutionRetryCleanup {
    /// Returns the durable generation that the replacement live checkpoint must supersede.
    /// 返回替代实时检查点必须超过的可靠代际。
    ///
    /// # Returns
    /// 返回值
    /// The exact generation carried by the consumed digest-bound reference.
    /// 返回已消费摘要绑定引用携带的精确代际。
    pub(crate) fn generation(&self) -> u64 {
        self.reference.generation
    }
}

/// Captures exact cleanup authority for the durable retry body consumed by a newly accepted turn.
/// 为新接纳回合所消费的可靠重试正文捕获精确清理权限。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the exact data root, `workspace_snapshot` owns the current reference,
/// and `session_id` selects the accepted session.
/// `controller_runtime` 持有精确数据根，`workspace_snapshot` 持有当前引用，`session_id`
/// 选择已接纳会话。
///
/// # Returns
/// 返回值
/// An exact deferred cleanup token only for a durable prior body, or `None` for a memory-only/absent checkpoint.
/// 仅对可靠旧正文返回精确延迟清理令牌；内存检查点或缺失检查点返回 `None`。
pub(crate) fn deferred_consumed_execution_retry_cleanup(
    controller_runtime: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<Option<DeferredConsumedExecutionRetryCleanup>, String> {
    // priorReference is copied before the detached accepted row replaces it in memory.
    // priorReference 在分离的已接纳记录于内存中替换旧值之前完成复制。
    let prior_reference = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone());
    // sessionDirectory comes from the exact retained locator and performs no directory discovery.
    // sessionDirectory 来自精确保留定位器，不执行目录发现。
    let session_directory = segmented_session_directory_for_workspace_session(
        &controller_runtime.data_root,
        workspace_snapshot,
        session_id,
    )
    .ok_or_else(|| format!("Session `{session_id}` has no exact retry storage locator."))?;
    // reference prefers the retained digest owner; when a crash occurred between fixed-file and
    // state publication, one direct fixed-path read recovers cleanup authority without scanning.
    // reference 优先使用保留的摘要所有者；当崩溃发生在固定文件与状态发布之间时，
    // 通过一次固定路径直读恢复清理权限且不执行扫描。
    let reference = match prior_reference.filter(|reference| reference.content_bytes > 0) {
        Some(reference) => Some(reference),
        None => vulcan_workspace_store::workspace_store::load_unreferenced_execution_retry_snapshot_if_present(
            &session_directory,
            session_id,
        )?,
    };
    let Some(reference) = reference else {
        return Ok(None);
    };
    Ok(Some(DeferredConsumedExecutionRetryCleanup {
        session_directory,
        reference,
    }))
}

/// Persists a newly accepted live retry reference and then retires the consumed durable body.
/// 持久化新接纳的实时重试引用，并在成功后退役已消费的可靠正文。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained runtime row containing the replacement reference.
/// `state` 持有包含替代引用的权威保留运行态记录。
/// `controller_runtime` resolves the exact session runtime-state and retry-file targets.
/// `controller_runtime` 解析精确的会话运行态与重试文件目标。
/// `session_id` selects the single session whose retry ownership is changing.
/// `session_id` 选择重试所有权正在变化的唯一会话。
/// `consumed_retry_cleanup` authorizes deletion only after replacement state persistence succeeds.
/// `consumed_retry_cleanup` 仅在替代状态持久化成功后授权删除旧正文。
/// `reason` labels the asynchronous persistence transaction for diagnostics.
/// `reason` 为异步持久化事务提供诊断标签。
pub(crate) fn spawn_accepted_retry_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    consumed_retry_cleanup: Option<DeferredConsumedExecutionRetryCleanup>,
    reason: &'static str,
) {
    let Some(consumed_retry_cleanup) = consumed_retry_cleanup else {
        spawn_workspace_session_runtime_state_persist_task(
            state,
            controller_runtime,
            session_id,
            Some(String::from(reason)),
        );
        return;
    };
    // The old file remains the valid crash boundary until the replacement memory-only reference is durable.
    // 在替代的仅内存引用可靠落盘前，旧文件继续作为有效崩溃边界。
    spawn_service_task(async move {
        match persist_latest_retained_session_runtime_state_update(
            &state,
            &controller_runtime,
            session_id,
        )
        .await
        {
            Ok(()) => {
                // Exact-reference deletion cannot remove a newer terminal retry publication.
                // 精确引用删除不会移除更新的终止重试发布。
                if let Err(error) =
                    vulcan_workspace_store::workspace_store::delete_execution_retry_snapshot(
                        &consumed_retry_cleanup.session_directory,
                        session_id,
                        &consumed_retry_cleanup.reference,
                    )
                {
                    eprintln!(
                        "[webui-service] consumed execution retry cleanup failed after runtime-state persistence ({reason}): {error}"
                    );
                }
            }
            Err(error) => eprintln!(
                "[webui-service] background runtime-state persist failed ({reason}): {error}"
            ),
        }
    });
}

/// Advances the session retry generation from both the live checkpoint and the durable terminal ledger.
/// 同时根据实时检查点与可靠终态账本推进会话重试代际。
///
/// # Parameters
/// 参数
/// `prior_retry_generation` is the optional generation of the currently retained retry reference.
/// `prior_retry_generation` 是当前保留重试引用的可选代际。
/// `consumed_retry_generation` is the optional exact fixed-file generation recovered after a state-publication crash window.
/// `consumed_retry_generation` 是状态发布崩溃窗口后恢复的可选精确固定文件代际。
/// `terminal_generation` is the monotonic Continue generation retained across successful cleanup.
/// `terminal_generation` 是跨成功清理保留的单调 Continue 代际。
///
/// # Returns
/// 返回值
/// One generation greater than both authorities, or an explicit overflow error.
/// 返回比两个权威值都大一的代际，或明确的溢出错误。
fn next_execution_retry_generation(
    prior_retry_generation: Option<u64>,
    consumed_retry_generation: Option<u64>,
    terminal_generation: u64,
) -> Result<u64, String> {
    prior_retry_generation
        .unwrap_or_default()
        .max(consumed_retry_generation.unwrap_or_default())
        .max(terminal_generation)
        .checked_add(1)
        .ok_or_else(|| String::from("Execution retry generation overflowed."))
}

/// Classifies whether a matching Continue claim is absent, still committing, or durably admitted.
/// 判定匹配的 Continue 领取是不存在、仍在提交，还是已经完成持久准入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingContinuationClaimDisposition {
    /// No claimed retry and ledger pair belongs to this exact client intent.
    /// 不存在属于该精确客户端意图的已领取重试与账本组合。
    NotOwned,
    /// The exact claim exists in memory but durable admission has not yet been proven.
    /// 精确领取已存在于内存中，但尚未证明已完成持久准入。
    AdmissionInFlight,
    /// The exact claim has crossed persistence and entered an active kernel phase.
    /// 精确领取已经跨过持久化并进入活动会话内核阶段。
    DurablyAdmitted,
}

/// Returns whether one runtime row and retry reference own the exact Continue claim identity.
/// 返回一条运行态记录与重试引用是否拥有精确的 Continue 领取身份。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the continuation ledger paired with `retry_reference`.
/// `runtime_state` 持有与 `retry_reference` 配对的 Continue 账本。
/// `expected_generation`, `expected_ledger_revision`, and `client_continuation_id` identify one physical claim.
/// `expected_generation`、`expected_ledger_revision` 与 `client_continuation_id` 标识一次物理领取。
///
/// # Returns
/// 返回值
/// `true` only when both durable records name the same non-invalidated claimed attempt.
/// 仅当两条持久记录指向同一个未失效的已领取尝试时返回 `true`。
fn continuation_claim_identity_matches(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
    retry_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_continuation_id: &str,
) -> bool {
    // continuation is the ledger half of the exact two-record claim identity.
    // continuation 是精确双记录领取身份中的账本一半。
    let continuation = &runtime_state.provider_call_ledger.continuation;
    // Snapshot generation counts immutable checkpoint replacements, while expectedGeneration belongs only to terminal continuation CAS.
    // 快照代际统计不可变 checkpoint 替换次数，而 expectedGeneration 仅属于终止 Continue 的 CAS。
    retry_reference.state == vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed
        && retry_reference.claimed_attempt_id.as_deref() == Some(client_continuation_id)
        && continuation.generation == expected_generation
        && continuation.expected_ledger_revision == expected_ledger_revision
        && continuation.claimed_at_ms.is_some()
        && continuation.claimed_execution_id.as_deref() == Some(client_continuation_id)
        && continuation.invalidated_at_ms.is_none()
}

/// Classifies one matching Continue retry without treating an in-memory claim as a durable ACK.
/// 对匹配的 Continue 重试进行分类，避免把仅存在于内存中的领取当作持久 ACK。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the continuation ledger paired with the lightweight retry reference.
/// `runtime_state` 持有与轻量重试引用配对的 Continue 账本。
/// `retry_reference` is the retained retry identity currently visible to the service.
/// `retry_reference` 是服务当前可见的保留重试身份。
/// `expected_generation`, `expected_ledger_revision`, and `client_continuation_id` identify the exact retry intent.
/// `expected_generation`、`expected_ledger_revision` 与 `client_continuation_id` 标识精确重试意图。
/// `run_phase` and `has_active_kernel_turn` prove that the first claimant crossed persistence before activation.
/// `run_phase` 与 `has_active_kernel_turn` 证明首次领取者已在激活前跨过持久化边界。
///
/// # Returns
/// 返回值
/// The ownership and durable-admission disposition for this exact retry intent.
/// 返回该精确重试意图的所有权与持久准入分类。
fn classify_existing_continuation_claim(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
    retry_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_continuation_id: &str,
    run_phase: SessionRunPhase,
    has_active_kernel_turn: bool,
) -> ExistingContinuationClaimDisposition {
    // exactClaimOwned requires both the sidecar reference and ledger to name the same client intent.
    // exactClaimOwned 要求 sidecar 引用与账本同时指向同一客户端意图。
    let exact_claim_owned = continuation_claim_identity_matches(
        runtime_state,
        retry_reference,
        expected_generation,
        expected_ledger_revision,
        client_continuation_id,
    );
    if !exact_claim_owned {
        return ExistingContinuationClaimDisposition::NotOwned;
    }
    if is_active_session_run_phase(run_phase) && has_active_kernel_turn {
        ExistingContinuationClaimDisposition::DurablyAdmitted
    } else {
        ExistingContinuationClaimDisposition::AdmissionInFlight
    }
}

#[cfg(test)]
mod continuation_duplicate_claim_tests {
    use super::*;

    /// Builds the claimed retry-reference half of one duplicate Continue identity.
    /// 构建重复 Continue 身份中已领取的重试引用一半。
    fn claimed_retry_reference() -> vulcan_workbench_model::ExecutionRetrySnapshotRef {
        vulcan_workbench_model::ExecutionRetrySnapshotRef {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 19,
            state: vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed,
            content_sha256: "0".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("execution-1"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: None,
            preparation_route_selection: Some(
                vulcan_workbench_model::ExecutionRetryPreparationRouteSelection {
                    active_provider_id: None,
                    active_connection_id: None,
                    active_model_id: None,
                },
            ),
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: Some(String::from("client-1")),
            error_summary: None,
            updated_at: String::from("1000"),
        }
    }

    /// Verifies an exact duplicate cannot ACK until persistence is followed by active kernel projection.
    /// 验证精确重复请求只有在持久化后投影出活动会话内核状态时才能返回 ACK。
    #[test]
    fn duplicate_continue_ack_requires_durable_active_admission() {
        // runtimeState supplies the ledger half of the exact claimed identity.
        // runtimeState 提供精确领取身份中的账本一半。
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();
        runtime_state.provider_call_ledger.continuation.generation = 7;
        runtime_state
            .provider_call_ledger
            .continuation
            .expected_ledger_revision = 11;
        runtime_state
            .provider_call_ledger
            .continuation
            .claimed_at_ms = Some(1000);
        runtime_state
            .provider_call_ledger
            .continuation
            .claimed_execution_id = Some(String::from("client-1"));
        // reference supplies the matching claimed sidecar identity while retaining its independent checkpoint generation.
        // reference 提供匹配的已领取 sidecar 身份，同时保留独立的 checkpoint 代际。
        let reference = claimed_retry_reference();

        assert_eq!(
            classify_existing_continuation_claim(
                &runtime_state,
                &reference,
                7,
                11,
                "client-1",
                SessionRunPhase::Failed,
                false,
            ),
            ExistingContinuationClaimDisposition::AdmissionInFlight
        );
        assert_eq!(
            classify_existing_continuation_claim(
                &runtime_state,
                &reference,
                7,
                11,
                "client-1",
                SessionRunPhase::Preparing,
                false,
            ),
            ExistingContinuationClaimDisposition::AdmissionInFlight
        );
        assert_eq!(
            classify_existing_continuation_claim(
                &runtime_state,
                &reference,
                7,
                11,
                "client-1",
                SessionRunPhase::Preparing,
                true,
            ),
            ExistingContinuationClaimDisposition::DurablyAdmitted
        );
        assert_eq!(
            classify_existing_continuation_claim(
                &runtime_state,
                &reference,
                7,
                11,
                "client-2",
                SessionRunPhase::Preparing,
                true,
            ),
            ExistingContinuationClaimDisposition::NotOwned
        );
    }
}

pub(crate) use activation::*;
pub(super) use edit::*;
pub(crate) use preflight::*;
pub(crate) use queue::*;
pub(crate) use runtime_options::*;

/// Defines how an existing session obtains dialogue context before prompt submission.
/// 定义已有会话在提交提示词前如何取得对话上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingSessionHydrationPolicy {
    /// Loads persisted dialogue when the retained runtime context is empty.
    /// 当保留运行态上下文为空时加载持久化对话。
    HydrateIfEmpty,
    /// Trusts the retained snapshot even when a destructive edit intentionally made it empty.
    /// 即使破坏性编辑有意将其清空，也信任保留快照。
    TrustRetainedSnapshot,
}

/// Selects whether a submission creates visible user input or continues retained provider state.
/// 选择本次提交是创建可见用户输入还是继续保留的 Provider 状态。
#[derive(Debug, Clone)]
pub(super) enum ConversationSubmissionMode {
    /// Normal composer submission authored by the user.
    /// 用户通过输入区发起的普通提交。
    UserPrompt,
    /// Recovery submission admitted by one exact terminal ledger generation, optionally carrying a user follow-up.
    /// 由精确终态账本代际准入的恢复提交，可选择携带一条用户补充消息。
    Continuation {
        /// Exact continuation generation projected to the caller.
        /// 投影给调用方的精确 Continue 代际。
        expected_generation: u64,
        /// Exact provider-call ledger revision projected to the caller.
        /// 投影给调用方的精确 Provider 调用账本修订号。
        expected_ledger_revision: u64,
        /// Exact digest-bound reference read before the sidecar body was loaded.
        /// 在加载 sidecar 正文前读取的精确摘要绑定引用。
        retry_reference: Box<vulcan_workbench_model::ExecutionRetrySnapshotRef>,
        /// Exact frozen execution snapshot loaded and digest-validated before admission.
        /// 准入前加载并完成摘要校验的精确冻结执行快照。
        retry_snapshot: Arc<vulcan_workbench_model::ExecutionRetrySnapshot>,
        /// Optional user-authored follow-up installed only after the original frozen body passes final CAS validation.
        /// 仅在原始冻结正文通过最终 CAS 校验后安装的可选用户补充消息。
        user_followup: Option<vulcan_workbench_model::ExecutionRetryUserFollowup>,
    },
}

impl ConversationSubmissionMode {
    /// Returns the prompt source recorded in the shared turn state.
    /// 返回共享回合状态中记录的提示词来源。
    fn prompt_source(&self) -> PendingSessionPromptSource {
        match self {
            Self::UserPrompt => PendingSessionPromptSource::UserFollowup,
            Self::Continuation { .. } => PendingSessionPromptSource::ExecutionRetry,
        }
    }

    /// Returns whether this mode should append user-authored conversation records.
    /// 返回本模式是否应追加用户编写的会话记录。
    fn appends_user_records(&self) -> bool {
        matches!(self, Self::UserPrompt)
    }

    /// Returns whether this submission resumes one retained interrupted execution.
    /// 返回本次提交是否恢复一项已保留的中断执行。
    fn is_continuation(&self) -> bool {
        matches!(self, Self::Continuation { .. })
    }

    /// Returns the exact continuation generation and ledger revision when applicable.
    /// 在适用时返回精确 Continue 代际与账本修订号。
    fn continuation_expectation(&self) -> Option<(u64, u64)> {
        match self {
            Self::UserPrompt => None,
            Self::Continuation {
                expected_generation,
                expected_ledger_revision,
                ..
            } => Some((*expected_generation, *expected_ledger_revision)),
        }
    }

    /// Returns the exact frozen retry snapshot carried by a manual continuation.
    /// 返回手动 Continue 携带的精确冻结重试快照。
    fn retry_snapshot(&self) -> Option<&Arc<vulcan_workbench_model::ExecutionRetrySnapshot>> {
        match self {
            Self::UserPrompt => None,
            Self::Continuation { retry_snapshot, .. } => Some(retry_snapshot),
        }
    }

    /// Returns the exact digest-bound retry reference carried by a manual continuation.
    /// 返回手动 Continue 携带的精确摘要绑定重试引用。
    ///
    /// # Returns
    /// 返回值
    /// The pre-load reference for a continuation, otherwise `None`.
    /// Continue 返回正文加载前引用，其他模式返回 `None`。
    fn retry_reference(&self) -> Option<&vulcan_workbench_model::ExecutionRetrySnapshotRef> {
        match self {
            Self::UserPrompt => None,
            Self::Continuation {
                retry_reference, ..
            } => Some(retry_reference),
        }
    }

    /// Returns the optional user follow-up admitted with this recovery claim.
    /// 返回随本次恢复领取准入的可选用户补充消息。
    ///
    /// # Returns
    /// 返回值
    /// The exact durable follow-up draft for a recovery submission, otherwise `None`.
    /// 恢复提交返回精确可靠的补充消息草稿，其他模式返回 `None`。
    fn user_followup(&self) -> Option<&vulcan_workbench_model::ExecutionRetryUserFollowup> {
        match self {
            Self::UserPrompt => None,
            Self::Continuation { user_followup, .. } => user_followup.as_ref(),
        }
    }
}

/// Revision-bound Continue token optionally attached to a composer submission on a problem turn.
/// 在问题回合中可选附着到输入区提交的带修订 Continue 令牌。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ConversationContinuationExpectation {
    /// Exact continuation generation projected with the selected problem turn.
    /// 随选中问题回合投影的精确 Continue 代际。
    pub(super) expected_generation: u64,
    /// Exact Provider-call ledger revision projected with the selected problem turn.
    /// 随选中问题回合投影的精确 Provider 调用账本修订号。
    pub(super) expected_ledger_revision: u64,
}

/// Validates one frozen retry snapshot against the exact current session state before CAS claim.
/// 在 CAS 领取前，根据精确当前会话状态校验一份冻结的重试快照。
///
/// # Parameters
/// 参数
/// `runtime_state` is the authoritative retained session state at admission.
/// `runtime_state` 是准入时的权威保留会话状态。
/// `session_id` is the exact owner selected by the Continue RPC.
/// `session_id` 是 Continue RPC 选定的精确所有者。
/// `snapshot` is the complete digest-validated recovery body selected for the claim.
/// `snapshot` 是为本次领取选定且已完成摘要校验的完整恢复正文。
///
/// # Returns
/// 返回值
/// Success only when route, context generation, canonical state, lifecycle, and pending action are all replay-safe.
/// 仅当路由、上下文代际、规范状态、生命周期与待执行动作全部可安全重放时返回成功。
pub(crate) fn validate_execution_retry_snapshot_for_claim(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
    session_id: SessionId,
    snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<(), String> {
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_OWNER_MISMATCH",
        ));
    }
    if snapshot.state != vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen
        || snapshot.claimed_attempt_id.is_some()
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN",
        ));
    }
    if snapshot
        .user_followup
        .as_ref()
        .is_some_and(|followup| !followup.is_structurally_valid())
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_FOLLOWUP_INVALID",
        ));
    }
    if !matches!(
        &snapshot.pending_action,
        vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::DispatchProviderRequest { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::PrepareNextProviderRequest
            | vulcan_workbench_model::ExecutionRetryPendingAction::FinalizeAssistant
    ) {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_ACTION_UNSUPPORTED",
        ));
    }
    match &snapshot.pending_action {
        vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation { input } => {
            if runtime_state.active_provider_id != input.active_provider_id
                || runtime_state.active_connection_id != input.active_connection_id
                || runtime_state.active_model_id != input.active_model_id
                || runtime_state.provider_route_lease != snapshot.provider_route
            {
                return Err(String::from(
                    "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED",
                ));
            }
        }
        vulcan_workbench_model::ExecutionRetryPendingAction::DispatchProviderRequest { .. }
        | vulcan_workbench_model::ExecutionRetryPendingAction::ExecuteToolBatch { .. }
        | vulcan_workbench_model::ExecutionRetryPendingAction::FinalizeAssistant => {
            let provider_route = snapshot
                .provider_route
                .as_ref()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING"))?;
            if runtime_state.provider_route_lease.as_ref() != Some(provider_route) {
                return Err(String::from(
                    "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED",
                ));
            }
            let canonical_snapshot =
                vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
                    &runtime_state.conversation_state,
                )?;
            match &snapshot.pending_action {
                vulcan_workbench_model::ExecutionRetryPendingAction::ExecuteToolBatch { tools }
                    if !matches!(
                        canonical_snapshot
                            .messages
                            .last()
                            .map(|message| &message.payload),
                        Some(vulcan_core::types::ConversationPayload::Assistant { tool_calls, .. })
                            if !tool_calls.is_empty() && tool_calls.len() == tools.len()
                    ) =>
                {
                    return Err(String::from(
                        "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
                    ));
                }
                vulcan_workbench_model::ExecutionRetryPendingAction::FinalizeAssistant
                    if !matches!(
                        canonical_snapshot
                            .messages
                            .last()
                            .map(|message| &message.payload),
                        Some(vulcan_core::types::ConversationPayload::Assistant { tool_calls, .. })
                            if tool_calls.is_empty()
                    ) =>
                {
                    return Err(String::from(
                        "ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING",
                    ));
                }
                _ => {}
            }
        }
        vulcan_workbench_model::ExecutionRetryPendingAction::PrepareNextProviderRequest => {
            let provider_route = snapshot
                .provider_route
                .as_ref()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING"))?;
            if runtime_state.provider_route_lease.as_ref() != Some(provider_route) {
                return Err(String::from(
                    "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED",
                ));
            }
            let canonical_snapshot =
                vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
                    &runtime_state.conversation_state,
                )?;
            if !vulcan_core::controller::conversation_snapshot_has_seeded_provider_input(
                &canonical_snapshot,
            ) {
                return Err(String::from(
                    "ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING",
                ));
            }
        }
    }
    if runtime_state
        .provider_call_ledger
        .active_context_generation_id
        != snapshot.context_generation_id
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED",
        ));
    }
    // canonicalSnapshot uses the same lossless bridge used by provider runtime bootstrap.
    // canonicalSnapshot 使用与 Provider 运行时 bootstrap 相同的无损桥接。
    let canonical_snapshot =
        vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
            &runtime_state.conversation_state,
        )?;
    let canonical_bytes = serde_json::to_vec(&(
        &canonical_snapshot.messages,
        &canonical_snapshot.continuation,
    ))
    .map_err(|error| format!("Failed to serialize current retry dispatch state: {error}"))?;
    if execution_retry_sha256_hex(&canonical_bytes) != snapshot.canonical_digest {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED",
        ));
    }
    Ok(())
}

/// Returns the stable blocked reason for a deterministic retry-body claim failure.
/// 返回确定性重试正文领取失败对应的稳定阻断原因。
///
/// # Parameters
/// 参数
/// `error` is the exact error returned by the retry-body validator or route refresh guard.
/// `error` 是重试正文校验器或路由刷新守卫返回的精确错误。
///
/// # Returns
/// 返回值
/// A stable blocked reason only for failures that permanently invalidate the same frozen reference.
/// 仅对会永久使同一冻结引用失效的失败返回稳定阻断原因。
pub(crate) fn execution_retry_claim_failure_blocked_reason(error: &str) -> Option<&'static str> {
    match error {
        "tool_side_effect_in_flight" => Some("tool_side_effect_in_flight"),
        "semantic_output_observed" => Some("semantic_output_observed"),
        "provider_dispatch_outcome_unknown" => Some("provider_dispatch_outcome_unknown"),
        "non_retryable" => Some("non_retryable"),
        "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_OWNER_MISMATCH" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_OWNER_MISMATCH")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_ACTION_UNSUPPORTED" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_ACTION_UNSUPPORTED")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING"
        | "ERR_CONVERSATION_CONTINUE_RETRY_PROVIDER_MISSING"
        | "ERR_CONVERSATION_CONTINUE_RETRY_CONNECTION_MISSING"
        | "ERR_CONVERSATION_CONTINUE_RETRY_MODEL_MISSING" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED")
        }
        "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED" => {
            Some("ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED")
        }
        _ => None,
    }
}

/// Invalidates one exact lightweight retry reference after its authoritative body becomes unclaimable.
/// 在权威正文变得不可领取后，使一条精确轻量重试引用失效。
///
/// # Parameters
/// 参数
/// `runtime_state` is the authoritative retained state to mutate.
/// `runtime_state` 是需要修改的权威保留状态。
/// `expected_reference` is the exact reference whose body was checked outside the service lock.
/// `expected_reference` 是在服务锁外完成正文检查的精确引用。
/// `blocked_reason` is the stable machine reason projected to the localized WebUI.
/// `blocked_reason` 是投影到本地化 WebUI 的稳定机器原因。
/// `error_summary` is the controlled non-secret diagnostic retained with the reference.
/// `error_summary` 是随引用保留的受控非敏感诊断。
/// `updated_at` is the service-generated Unix-millisecond timestamp text.
/// `updated_at` 是服务生成的 Unix 毫秒时间戳文本。
///
/// # Returns
/// 返回值
/// `true` only when the same reference was still current and was invalidated.
/// 仅当同一引用仍为当前引用且已经失效时返回 `true`。
pub(crate) fn invalidate_execution_retry_reference_after_claim_failure(
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    expected_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    blocked_reason: &str,
    error_summary: &str,
    updated_at: String,
) -> bool {
    let Some(reference) = runtime_state.execution_retry_snapshot.as_mut() else {
        return false;
    };
    if reference != expected_reference {
        return false;
    }
    reference.invalidate_for_safety_reason(
        blocked_reason.to_string(),
        error_summary.to_string(),
        updated_at.clone(),
    );
    runtime_state.transient_execution_retry_snapshot = None;
    // Ledger eligibility mirrors the invalidated reference so persistence and projection cannot diverge.
    // 账本资格镜像已失效引用，避免持久化与投影发生分歧。
    runtime_state.provider_call_ledger.continuation.eligible = false;
    runtime_state.updated_at = updated_at;
    true
}

/// Persists and projects invalidation of one exact retry reference after an unlocked body check fails.
/// 在无锁正文检查失败后，持久化并投影一条精确重试引用的失效状态。
///
/// # Parameters
/// 参数
/// `app` emits the corrected conversation summary.
/// `app` 发出修正后的会话摘要。
/// `state` owns the lock-consistent workspace and Workbench projection.
/// `state` 持有锁一致的工作区与 Workbench 投影。
/// `controller_runtime` owns the exact durable session store.
/// `controller_runtime` 持有精确的可靠会话存储。
/// `session_id` identifies the Continue target.
/// `session_id` 标识 Continue 目标。
/// `expected_reference` is the reference whose sidecar or body validation failed.
/// `expected_reference` 是 sidecar 或正文校验失败对应的引用。
/// `blocked_reason` is the stable localized projection reason.
/// `blocked_reason` 是稳定的本地化投影原因。
/// `error_summary` is the controlled retained diagnostic.
/// `error_summary` 是受控的保留诊断。
///
/// # Returns
/// 返回值
/// `true` after the same reference is invalidated, persisted, and projected; `false` after a concurrent replacement.
/// 同一引用完成失效、持久化与投影后返回 `true`；并发替换后返回 `false`。
async fn persist_execution_retry_claim_failure_invalidation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    session_id: SessionId,
    expected_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    blocked_reason: &str,
    error_summary: &str,
) -> Result<bool, String> {
    // invalidated records whether the unlocked check still targets the current retained reference.
    // invalidated 记录无锁检查是否仍指向当前保留引用。
    let invalidated = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime_state = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        invalidate_execution_retry_reference_after_claim_failure(
            runtime_state,
            expected_reference,
            blocked_reason,
            error_summary,
            current_unix_millis_text(),
        )
    };
    if !invalidated {
        return Ok(false);
    }
    // projectionResult closes the current UI token before any potentially slow durable write.
    // projectionResult 在可能较慢的可靠写入前关闭当前界面令牌。
    let projection_result = emit_conversation_continuation_summary_changed(app, state, session_id);
    // persistenceResult durably closes the token after the immediate in-memory projection is attempted.
    // persistenceResult 在尝试即时内存投影后持久关闭令牌。
    let persistence_result = persist_latest_retained_workspace_session_update(
        state,
        controller_runtime.clone(),
        session_id,
    )
    .await;
    match (persistence_result, projection_result) {
        (Ok(()), Ok(())) => Ok(true),
        (Err(persistence_error), Ok(())) => Err(format!(
            "Failed to persist execution retry invalidation: {persistence_error}"
        )),
        (Ok(()), Err(projection_error)) => Err(format!(
            "Failed to project execution retry invalidation: {projection_error}"
        )),
        (Err(persistence_error), Err(projection_error)) => Err(format!(
            "Failed to persist execution retry invalidation: {persistence_error}; projection failed: {projection_error}"
        )),
    }
}

/// Computes a lowercase SHA-256 digest for exact retry-claim comparison bytes.
/// 为精确重试领取比较字节计算小写 SHA-256 摘要。
fn execution_retry_sha256_hex(encoded: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Retains the accepted pre-provider preparation boundary in process memory before acknowledgement.
/// 在提交确认返回前把已接纳的 Provider 调用前准备边界保留在进程内存中。
///
/// # Parameters
/// 参数
/// - `workspace_snapshot`: Detached accepted workspace state that receives the lightweight reference.
/// - `workspace_snapshot`：接收轻量引用的分离已接纳工作区状态。
/// - `session_id`: Session that owns the accepted prompt turn.
/// - `session_id`：拥有已接纳提示词回合的会话。
/// - `physical_attempt_id`: Submission identity that owns the first physical attempt.
/// - `physical_attempt_id`：拥有首次物理尝试的提交身份。
/// - `start`: Exact started prompt-turn state captured before deferred preparation.
/// - `start`：延迟准备前捕获的精确已启动提示词回合状态。
/// - `consumed_retry_generation`: Optional orphan/durable generation that the new checkpoint must supersede.
/// - `consumed_retry_generation`：新检查点必须超过的可选孤立或可靠代际。
///
/// # Returns
/// 返回值
/// Success after the process-local body and lightweight runtime reference are installed without filesystem mutation.
/// 在不修改文件系统的情况下安装进程内正文与轻量运行态引用后返回成功。
pub(crate) fn retain_accepted_preparation_retry_checkpoint(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    physical_attempt_id: &str,
    start: &SharedPromptTurnStart,
    consumed_retry_generation: Option<u64>,
) -> Result<(), String> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("Accepted preparation retry runtime state is missing."))?;
    let provider_route = runtime_state.provider_route_lease.clone();
    let controller_snapshot = vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
        &runtime_state.conversation_state,
    )?;
    let canonical_bytes = serde_json::to_vec(&(
        &controller_snapshot.messages,
        &controller_snapshot.continuation,
    ))
    .map_err(|error| {
        format!("Failed to serialize accepted preparation retry Core state: {error}")
    })?;
    let generation = next_execution_retry_generation(
        runtime_state
            .execution_retry_snapshot
            .as_ref()
            .map(|reference| reference.generation),
        consumed_retry_generation,
        runtime_state.provider_call_ledger.continuation.generation,
    )?;
    let now = current_unix_millis_text();
    let snapshot = vulcan_workbench_model::ExecutionRetrySnapshot {
        schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        generation,
        state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
        logical_execution_id: start.preparation_turn_id.clone(),
        logical_turn_id: Some(start.preparation_turn_id.clone()),
        next_provider_attempt_index: 0,
        physical_attempt_id: physical_attempt_id.to_string(),
        claimed_attempt_id: None,
        provider_route,
        provider_service_entry_id: None,
        credential_profile_id: None,
        controller_config: None,
        controller_snapshot,
        pending_action: vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
            input: Box::new(start.execution_retry_preparation_input(runtime_state)?),
        },
        user_followup: None,
        error: None,
        canonical_digest: execution_retry_sha256_hex(&canonical_bytes),
        context_generation_id: runtime_state
            .provider_call_ledger
            .active_context_generation_id
            .clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let reference = vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
        session_id, &snapshot,
    )?;
    // The complete body remains process-local until a real terminal interruption is classified.
    // 完整正文保持在进程内，直到真实终止中断完成分类。
    runtime_state.transient_execution_retry_snapshot = Some(Arc::new(snapshot));
    runtime_state.execution_retry_snapshot = Some(reference);
    runtime_state.updated_at = now;
    Ok(())
}

#[cfg(test)]
mod accepted_retry_generation_tests {
    use super::next_execution_retry_generation;

    /// Verifies successful cleanup cannot reset a later retry checkpoint below the durable terminal generation.
    /// 验证成功清理不会让后续重试检查点回退到可靠终态代际以下。
    #[test]
    fn retry_generation_remains_monotonic_after_live_reference_cleanup() {
        assert_eq!(
            next_execution_retry_generation(None, None, 9)
                .expect("terminal generation should advance without a live reference"),
            10
        );
        assert_eq!(
            next_execution_retry_generation(Some(12), None, 9)
                .expect("newer live generation should remain authoritative"),
            13
        );
        assert_eq!(
            next_execution_retry_generation(Some(7), None, 11)
                .expect("newer terminal generation should remain authoritative"),
            12
        );
        assert!(
            next_execution_retry_generation(None, None, u64::MAX).is_err(),
            "exhausted generation must fail explicitly"
        );
    }

    /// Verifies an unreferenced fixed file still advances the next in-memory retry generation.
    /// 验证未被引用的固定文件仍会推进下一份内存重试代际。
    #[test]
    fn retry_generation_advances_past_an_unreferenced_interruption() {
        assert_eq!(
            next_execution_retry_generation(None, Some(14), 13)
                .expect("orphaned durable generation should remain an ordering authority"),
            15
        );
    }
}

/// Freezes the exact provider, model, and credential selected when a prompt is submitted.
/// 固定提示词提交时选择的精确供应商、模型与凭据。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SubmissionModelSelection {
    /// Stable provider identifier selected in the composer.
    /// 输入区中选择的稳定供应商标识。
    pub(crate) provider_id: String,
    /// Stable model identifier selected in the composer.
    /// 输入区中选择的稳定模型标识。
    pub(crate) model_id: String,
    /// Exact credential connection selected in the composer.
    /// 输入区中选择的精确凭据连接。
    pub(crate) connection_id: String,
}

/// Captures one tri-state VMM policy pair selected before a conversation exists.
/// 捕获会话创建前选择的一组 VMM 三态策略。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionVmmPolicy {
    /// Precheck-memory injection mode.
    /// 预检记忆注入模式。
    precheck_mode: String,
    /// Post-turn memory writeback mode.
    /// 回合后记忆回写模式。
    memory_writeback_mode: String,
}

/// Captures one explicit knowledge-library selection for the materialized session.
/// 捕获用于已物化会话的一份显式知识库选择。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionKnowledgeSelection {
    /// Selection mode, either inherit or explicit.
    /// 选择模式，仅可为继承或显式。
    mode: String,
    /// Exact authorized library identifiers.
    /// 精确且已授权的知识库标识列表。
    library_ids: Vec<String>,
    /// Optional explicit retrieval policy for knowledge tools.
    /// 知识库工具可选的显式检索策略。
    #[serde(default)]
    retrieval_mode: Option<String>,
}

/// Captures one initial user goal created before the first prompt is submitted.
/// 捕获首次提示词提交前创建的初始用户目标。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionInitialGoal {
    /// Human-authored goal objective.
    /// 用户编写的目标文本。
    objective: String,
    /// Optional positive token budget.
    /// 可选正数 Token 预算。
    token_budget: Option<i64>,
}

/// Freezes one complete provider tool snapshot for a new conversation.
/// 固定新会话的一份完整供应商工具快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmissionProviderToolSettings {
    /// Capabilities enabled only for the materialized conversation.
    /// 仅为物化会话启用的能力。
    enabled_capabilities:
        std::collections::BTreeSet<vulcan_provider_adapters::ProviderToolCapabilityKind>,
    /// Exact model and account retained independently from activation.
    /// 与启用状态独立保留的精确模型与账号。
    selections: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        vulcan_provider_adapters::ProviderToolTargetSelection,
    >,
    /// Native model-extension target sets selected per generative capability.
    /// 按生成类能力选择的原生模型扩展目标集合。
    extension_target_sets: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        std::collections::BTreeSet<vulcan_provider_adapters::ProviderToolTargetSelection>,
    >,
    /// Session-selected provider or model-replacement mode per media capability.
    /// 当前会话为每项媒体能力选择的供应商专用或模型代替模式。
    #[serde(default)]
    media_understanding_modes: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
}

/// Freezes every draft-only session setting required by the first provider turn.
/// 固定首个 Provider 回合所需的全部草稿会话设置。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionSessionSettings {
    /// Optional session output-style identifier captured before the first prompt boundary.
    /// 在首次提示词边界前捕获的可选会话输出风格标识。
    #[serde(default)]
    output_style_id: Option<String>,
    /// Optional explicit VMM policy pair; absence preserves inherited defaults.
    /// 可选显式 VMM 策略对；缺省时保留继承默认值。
    vmm_policy: Option<SubmissionVmmPolicy>,
    /// Optional complete provider-tool snapshot; absence preserves an explicit session override or inherits current global defaults.
    /// 可选完整供应商工具快照；缺省时保留会话显式覆盖，或继承当前全局默认值。
    provider_tool_settings: Option<SubmissionProviderToolSettings>,
    /// Optional knowledge selection; absence preserves project inheritance.
    /// 可选知识库选择；缺省时保留项目继承。
    knowledge_selection: Option<SubmissionKnowledgeSelection>,
    /// Optional initial goal created before the first turn starts.
    /// 首轮开始前创建的可选初始目标。
    initial_goal: Option<SubmissionInitialGoal>,
}

/// Reports whether an absent per-turn snapshot must preserve an explicit session override.
/// 判断缺失的本回合快照是否必须保留会话显式覆盖。
fn should_preserve_existing_provider_tool_settings(
    has_explicit_settings: bool,
    provider_tool_settings_initialized: bool,
    provider_tool_selections_overridden: bool,
) -> bool {
    !has_explicit_settings
        && provider_tool_settings_initialized
        && provider_tool_selections_overridden
}

/// Converts one submitted VMM tri-state mode into the persisted optional layer value.
/// 将一个已提交的 VMM 三态模式转换为持久化可选层取值。
fn submission_vmm_policy_value(mode: &str) -> Result<Option<bool>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "inherit" => Ok(None),
        "enabled" => Ok(Some(true)),
        "disabled" => Ok(Some(false)),
        other => Err(format!("不支持的 VMM 记忆策略模式：{other}")),
    }
}

/// Resolves output style only when the submission is crossing a materialization boundary.
/// 仅当提交跨越会话物化边界时解析输出风格。
fn submission_output_style_snapshot(
    settings: Option<&SubmissionSessionSettings>,
    global_output_style_selection: vulcan_agent_runtime::provider_config::PersistedPromptOutputStyleSelection,
    allow_output_style_activation: bool,
) -> Result<Option<vulcan_prompt_composer::PromptOutputStyleSnapshot>, String> {
    if !allow_output_style_activation {
        return Ok(None);
    }
    resolve_output_style_snapshot(
        submission_output_style_id(settings).as_deref(),
        global_output_style_selection,
    )
    .map(Some)
}

/// Normalizes the explicit style override captured by a new-session submission.
/// 规范化新会话提交中捕获的显式输出风格覆盖。
fn submission_output_style_id(settings: Option<&SubmissionSessionSettings>) -> Option<String> {
    settings
        .and_then(|settings| settings.output_style_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Validates and applies draft-only settings before the first precheck or provider request.
/// 在首次预检或 Provider 请求前校验并应用草稿专属设置。
async fn apply_submission_session_settings(
    state: &SharedServiceBridgeState,
    controller_runtime: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    execution_mode: Option<&str>,
    settings: Option<&SubmissionSessionSettings>,
    allow_output_style_activation: bool,
) -> Result<(), String> {
    if settings
        .and_then(|settings| settings.initial_goal.as_ref())
        .is_some()
        && execution_mode.is_some_and(|mode| mode.eq_ignore_ascii_case("plan"))
    {
        return Err(String::from(
            "目标模式与计划模式互斥，不能在同一首次提交中同时启用。",
        ));
    }
    let configuration = runtime.configuration_snapshot()?;
    let global_output_style_selection = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
        .provider_configuration
        .prompt
        .output_style;
    let output_style_snapshot = submission_output_style_snapshot(
        settings,
        global_output_style_selection,
        allow_output_style_activation,
    )?;
    let provider_tool_settings =
        settings.and_then(|settings| settings.provider_tool_settings.as_ref());
    let global_media_understanding_modes =
        crate::public_ai_settings::session_media_understanding_modes_from_configuration(
            &configuration,
        );
    let has_explicit_provider_tool_settings = provider_tool_settings.is_some();
    let preserve_existing_provider_tools = should_preserve_existing_provider_tool_settings(
        has_explicit_provider_tool_settings,
        workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .is_some_and(|runtime_state| runtime_state.provider_tool_settings_initialized),
        workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .is_some_and(|runtime_state| runtime_state.provider_tool_selections_overridden),
    );
    if !preserve_existing_provider_tools {
        let enabled_capabilities = provider_tool_settings
            .map(|settings| &settings.enabled_capabilities)
            .unwrap_or(&configuration.provider_tool_enabled_capabilities);
        let provider_tool_selections = provider_tool_settings
            .map(|settings| &settings.selections)
            .unwrap_or(&configuration.provider_tool_selections);
        let provider_extension_target_sets = provider_tool_settings
            .map(|settings| &settings.extension_target_sets)
            .unwrap_or(&configuration.provider_extension_target_sets);
        let media_understanding_modes = provider_tool_settings
            .map(|settings| &settings.media_understanding_modes)
            .unwrap_or(&global_media_understanding_modes);
        crate::public_ai_settings::validate_provider_tool_session_settings(
            runtime,
            &configuration,
            enabled_capabilities,
            provider_tool_selections,
            media_understanding_modes,
            provider_extension_target_sets,
        )?;
    }
    let knowledge_selection = if let Some(selection) =
        settings.and_then(|settings| settings.knowledge_selection.as_ref())
    {
        let mode = match selection.mode.as_str() {
            "inherit" => SessionKnowledgeLibrarySelectionMode::Inherit,
            "explicit" => SessionKnowledgeLibrarySelectionMode::Explicit,
            other => return Err(format!("未知知识库选择模式：{other}")),
        };
        let knowledge_runtime = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .knowledge_runtime
            .clone()
            .ok_or_else(|| {
                String::from("service is still starting; knowledge runtime is not ready")
            })?;
        let project_id = owning_project_id_for_session(workspace_snapshot, session_id)
            .ok_or_else(|| String::from("当前会话不属于可用项目。"))?;
        let authorized = knowledge_runtime
            .authorized_library_ids(&project_id, false)
            .await?
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(denied) = selection
            .library_ids
            .iter()
            .find(|library_id| !authorized.contains(*library_id))
        {
            return Err(format!("知识库未授权给当前项目：{denied}"));
        }
        if let Some(retrieval_mode) = selection.retrieval_mode.as_deref()
            && !matches!(
                retrieval_mode,
                "raw_bm25" | "display_bm25" | "display_hybrid"
            )
        {
            return Err(format!("未知知识库检索策略：{retrieval_mode}"));
        }
        Some(
            SessionKnowledgeLibrarySelection {
                mode,
                library_ids: selection.library_ids.clone(),
                retrieval_mode: selection.retrieval_mode.clone(),
            }
            .normalized(),
        )
    } else {
        None
    };
    let updated_at = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    if let Some(policy) = settings.and_then(|settings| settings.vmm_policy.as_ref()) {
        runtime_state.vmm.precheck_enabled = submission_vmm_policy_value(&policy.precheck_mode)?;
        runtime_state.vmm.memory_writeback_enabled =
            submission_vmm_policy_value(&policy.memory_writeback_mode)?;
    }
    if !preserve_existing_provider_tools {
        let enabled_capabilities = provider_tool_settings
            .map(|settings| &settings.enabled_capabilities)
            .unwrap_or(&configuration.provider_tool_enabled_capabilities);
        let provider_tool_selections = provider_tool_settings
            .map(|settings| &settings.selections)
            .unwrap_or(&configuration.provider_tool_selections);
        let provider_extension_target_sets = provider_tool_settings
            .map(|settings| &settings.extension_target_sets)
            .unwrap_or(&configuration.provider_extension_target_sets);
        let media_understanding_modes = provider_tool_settings
            .map(|settings| &settings.media_understanding_modes)
            .unwrap_or(&global_media_understanding_modes);
        runtime_state.provider_tool_selections = provider_tool_selections
            .iter()
            .map(|(capability, selection)| {
                (
                    provider_tool_capability_key(*capability),
                    SessionProviderToolTargetSelection {
                        connection_id: selection.connection_id.clone(),
                        target_id: selection.target_id.clone(),
                    },
                )
            })
            .collect();
        runtime_state.provider_tool_enabled_capabilities = enabled_capabilities
            .iter()
            .map(|capability| provider_tool_capability_key(*capability))
            .collect();
        runtime_state.media_understanding_modes = media_understanding_modes
            .iter()
            .map(|(capability, mode)| (provider_tool_capability_key(*capability), *mode))
            .collect();
        runtime_state.provider_extension_target_sets = provider_extension_target_sets
            .iter()
            .map(|(capability, selections)| {
                (
                    provider_tool_capability_key(*capability),
                    selections
                        .iter()
                        .map(|selection| SessionProviderToolTargetSelection {
                            connection_id: selection.connection_id.clone(),
                            target_id: selection.target_id.clone(),
                        })
                        .collect(),
                )
            })
            .collect();
        runtime_state.provider_tool_settings_initialized = true;
        runtime_state.provider_tool_selections_overridden = has_explicit_provider_tool_settings;
    }
    if let Some(selection) = knowledge_selection {
        runtime_state.knowledge_library_selection = selection;
    }
    if allow_output_style_activation {
        // An explicit draft choice must remain session-owned after creation; null/empty inherits global.
        // 新会话草稿的显式选择必须在创建后归属会话；空值继续继承全局。
        runtime_state.output_style.override_style_id = submission_output_style_id(settings);
    }
    if let Some(output_style_snapshot) = output_style_snapshot {
        apply_output_style_snapshot(
            runtime_state,
            &output_style_snapshot,
            "new-session",
            None,
            updated_at.clone(),
        );
    }
    runtime_state.updated_at = updated_at;
    if let Some(goal) = settings.and_then(|settings| settings.initial_goal.as_ref()) {
        persist_initial_submission_goal(
            controller_runtime,
            session_id,
            &goal.objective,
            goal.token_budget,
        )
        .await?;
        advance_session_goal_dispatch_version(state, session_id)?;
    }
    Ok(())
}

/// Reports whether a persisted restart lease targets the same route owner but an older execution revision.
/// 判断持久化的重启租约是否仍指向同一路由所有者、但执行修订已经过期。
///
/// # Parameters
/// 参数
/// `expected` is the non-secret lease identity restored with the session runtime state.
/// `expected` 是随会话运行态恢复的非敏感租约身份。
/// `actual` is the identity resolved from the currently installed provider manifest.
/// `actual` 是从当前已安装供应商清单解析出的身份。
///
/// # Returns
/// 返回值
/// Returns true only when schema, provider, connection, model, and protocol are identical while
/// the execution route revision changed.
/// 仅当 Schema、供应商、连接、模型与协议完全相同且执行路由修订发生变化时返回 true。
fn provider_route_lease_revision_changed_for_same_target(
    expected: &vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
    actual: &vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
) -> bool {
    expected.schema_version == actual.schema_version
        && expected.provider_id == actual.provider_id
        && expected.connection_id == actual.connection_id
        && expected.model_id == actual.model_id
        && expected.protocol_profile_id == actual.protocol_profile_id
        && expected.route_revision != actual.route_revision
}

/// Refreshes a frozen pre-dispatch retry after the exact provider Manifest route changed.
/// 在精确 Provider Manifest 路由变化后刷新尚未分发的冻结重试。
///
/// # Parameters
/// 参数
/// `runtime` resolves the currently installed immutable provider route.
/// `runtime` 解析当前已安装的不可变 Provider 路由。
/// `session_id` owns both the process-local and persisted route leases.
/// `session_id` 同时拥有进程内与持久化路由租约。
/// `runtime_state` is the detached submission state that may adopt the current route.
/// `runtime_state` 是可以采用当前路由的分离提交状态。
/// `retry_snapshot` is the exact frozen preparation checkpoint being claimed.
/// `retry_snapshot` 是正在领取的精确冻结准备 checkpoint。
///
/// # Returns
/// 返回值
/// Success after either preserving the frozen route or revoking both stale lease identities.
/// 保留冻结路由或撤销两处陈旧租约身份后返回成功。
fn refresh_resume_preparation_route_after_manifest_change(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    session_id: SessionId,
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    retry_snapshot: &mut vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<(), String> {
    let Some(error) = retry_snapshot.error.as_ref() else {
        return Ok(());
    };
    if error.message
        != vulcan_agent_runtime::agent_runtime::provider::PROVIDER_MANIFEST_ROUTE_CHANGED_MESSAGE
        || !error.retryable
        || error.dispatch_outcome == vulcan_ai_contract::DispatchOutcome::Dispatched
        || error.semantic_output_observed
        || error.side_effect_observed
    {
        return Ok(());
    }
    let vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation { input } =
        &retry_snapshot.pending_action
    else {
        return Ok(());
    };
    let expected_route = retry_snapshot
        .provider_route
        .as_ref()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING"))?;
    let provider_id = input
        .active_provider_id
        .as_deref()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_PROVIDER_MISSING"))?;
    let connection_id = input
        .active_connection_id
        .as_deref()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_CONNECTION_MISSING"))?;
    let model_id = input
        .active_model_id
        .as_deref()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_MODEL_MISSING"))?;
    if expected_route.provider_id != provider_id
        || expected_route.connection_id != connection_id
        || expected_route.model_id != model_id
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED",
        ));
    }
    let configuration = runtime.configuration_snapshot()?;
    let resolved_selection = runtime.resolve_connection_model_selection(
        &configuration,
        connection_id,
        provider_id,
        model_id,
    )?;
    let resolved_identity =
        vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
            &resolved_selection,
        );
    if !provider_route_lease_revision_changed_for_same_target(expected_route, &resolved_identity) {
        return Ok(());
    }

    // A ResumePreparation checkpoint proves no provider request exists to replay. Revoking the
    // stale identity makes the restored preparation lease the current same-protocol route.
    // ResumePreparation checkpoint 证明不存在需要重放的 Provider 请求。撤销陈旧身份后，
    // 恢复的准备阶段会租用当前同协议路由。
    runtime.clear_session_selection_leases(session_id.to_string().as_str())?;
    runtime_state.provider_route_lease = None;
    retry_snapshot.provider_route = None;
    Ok(())
}

/// Claims one frozen continuation before its retry precheck is restored into the working state.
/// 在 retry precheck 恢复到工作状态前领取一条冻结 Continue。
///
/// # Parameters
/// 参数
/// `runtime` owns the current provider route generation and process-local leases.
/// `runtime` 持有当前 Provider 路由代际与进程内租约。
/// `runtime_state` is the detached canonical state before retry-owned precheck mutation.
/// `runtime_state` 是 retry 自有 precheck 写入前的分离规范状态。
/// `run_phase` and `blockers` are the current authoritative admission facts.
/// `run_phase` 与 `blockers` 是当前权威准入事实。
/// `expected_generation` and `expected_ledger_revision` bind the visible Continue generation.
/// `expected_generation` 与 `expected_ledger_revision` 绑定可见 Continue 代际。
/// `client_submission_id` uniquely owns the new physical attempt.
/// `client_submission_id` 唯一拥有新的物理尝试。
/// `expected_retry_reference` is the exact digest-bound pointer observed before unlocked sidecar loading.
/// `expected_retry_reference` 是无锁加载 sidecar 正文前观察到的精确摘要绑定指针。
/// `retry_snapshot` is the exact frozen sidecar loaded by the Continue RPC.
/// `retry_snapshot` 是 Continue RPC 加载的精确冻结 sidecar。
/// `user_followup` is installed only after the original frozen body passes both validation rounds.
/// `user_followup` 仅在原始冻结正文通过两轮校验后安装。
///
/// # Returns
/// 返回值
/// Success after the ledger and sidecar draft are claimed without creating precheck self-busy state.
/// 在不创建 precheck 自阻塞状态的前提下领取账本与 sidecar 草稿后返回成功。
fn claim_continuation_before_precheck_restore(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    session_id: SessionId,
    run_phase: SessionRunPhase,
    blockers: SessionContinuationRuntimeBlockers,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_submission_id: &str,
    expected_retry_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    retry_snapshot: &mut vulcan_workbench_model::ExecutionRetrySnapshot,
    user_followup: Option<&vulcan_workbench_model::ExecutionRetryUserFollowup>,
) -> Result<(), String> {
    resolve_session_continuation_eligibility(
        runtime_state,
        run_phase,
        blockers,
        Some((expected_generation, expected_ledger_revision)),
    )
    .map_err(String::from)?;
    if runtime_state.execution_retry_snapshot.as_ref() != Some(expected_retry_reference) {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
    }
    // The frozen sidecar must be fully valid before route refresh may clear a process-local lease.
    // 在路由刷新可以清除进程内租约之前，必须先完整验证冻结 sidecar。
    validate_execution_retry_snapshot_for_claim(runtime_state, session_id, retry_snapshot)?;
    refresh_resume_preparation_route_after_manifest_change(
        runtime,
        session_id,
        runtime_state,
        retry_snapshot,
    )?;
    // Refresh changes both copies of the route identity, so the same strict guard proves their
    // post-refresh state before the continuation ledger is claimed.
    // 刷新会同时修改两份路由身份，因此在领取 Continue 账本前再次使用同一严格守卫证明
    // 刷新后状态一致。
    validate_execution_retry_snapshot_for_claim(runtime_state, session_id, retry_snapshot)?;
    if user_followup.is_some() && retry_snapshot.user_followup.is_some() {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_FOLLOWUP_ALREADY_RETAINED",
        ));
    }
    if user_followup.is_some_and(|followup| !followup.is_structurally_valid()) {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_FOLLOWUP_INVALID",
        ));
    }
    // claimedAtMs is the single durable claim timestamp shared by the continuation ledger and retry sidecar transition.
    // claimedAtMs 是 Continue 账本与重试 sidecar 状态迁移共同使用的唯一可靠领取时间戳。
    let claimed_at_ms = i64::try_from(current_unix_millis_value())
        .map_err(|_| String::from("ERR_CONVERSATION_CONTINUE_TIMESTAMP_OUT_OF_RANGE"))?;
    runtime_state.provider_call_ledger.claim_continuation(
        expected_generation,
        expected_ledger_revision,
        client_submission_id.to_string(),
        claimed_at_ms,
    )?;
    retry_snapshot.state = vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed;
    retry_snapshot.claimed_attempt_id = Some(client_submission_id.to_string());
    if let Some(user_followup) = user_followup {
        retry_snapshot.user_followup = Some(user_followup.clone());
    }
    retry_snapshot.updated_at = current_unix_millis_text();
    Ok(())
}

/// Resolves live Continue blockers while excluding only this submission's exact admission row.
/// 解析实时 Continue 阻断，并且只排除本次提交拥有的精确准入行。
///
/// # Parameters
/// 参数
/// `state` and `retained_workspace` are one lock-consistent authoritative revision.
/// `state` 与 `retained_workspace` 属于同一个锁一致的权威修订。
/// `session_id` identifies the existing Continue target.
/// `session_id` 标识已有 Continue 目标。
/// `turn_admission` owns the exact PromptSubmission reservation created by preflight.
/// `turn_admission` 持有预检创建的精确 PromptSubmission 预留。
///
/// # Returns
/// 返回值
/// All external live blockers with the exact self-owned admission row removed, or an explicit ownership error.
/// 返回排除精确自身准入行后的全部外部实时阻断，或明确的所有权错误。
pub(crate) fn continuation_submission_runtime_blockers(
    state: &ServiceBridgeState,
    retained_workspace: &WorkspaceSnapshot,
    session_id: SessionId,
    turn_admission: &SubmissionTurnAdmission,
) -> Result<SessionContinuationRuntimeBlockers, String> {
    if !turn_admission.owns_current_prompt_submission_reservation(state, session_id) {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT",
        ));
    }
    let mut blockers =
        session_continuation_runtime_blockers(state, retained_workspace, session_id)?;
    blockers.has_turn_admission_reservation = false;
    Ok(blockers)
}

/// Merges only the runtime fields owned by one Continue checkpoint into the latest live row.
/// 仅把一次 Continue checkpoint 拥有的运行态字段合并到最新实时记录。
///
/// # Parameters
/// 参数
/// `candidate` is the detached continuation-owned runtime mutation to commit.
/// `candidate` 是准备提交的分离 Continue 自有运行态变更。
/// `live` is the latest retained runtime row observed under the service-state lock.
/// `live` 是在服务状态锁下观察到的最新保留运行态记录。
///
/// # Returns
/// 返回值
/// This function returns no value after applying only the route-lease, claim-ledger, retry-reference, precheck-owner, and timestamp changes.
/// 此函数只应用路由租约、领取账本、重试引用、预检所有者与时间戳变更后不返回值。
fn apply_continuation_owned_runtime_fields(
    candidate: &vulcan_workbench_model::SessionRuntimeStateEntry,
    live: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
) {
    // A same-target manifest refresh may revoke the stale lease before restored preparation resolves the current route.
    // 同目标 Manifest 刷新可能在恢复准备阶段解析当前路由前撤销陈旧租约。
    live.provider_route_lease = candidate.provider_route_lease.clone();
    // These fields form the exact Continue claim, its process-local body, and its preparation owner.
    // 以下字段组成精确 Continue 领取、进程内正文及其准备阶段所有者。
    live.provider_call_ledger = candidate.provider_call_ledger.clone();
    live.execution_retry_snapshot = candidate.execution_retry_snapshot.clone();
    live.transient_execution_retry_snapshot = candidate.transient_execution_retry_snapshot.clone();
    live.active_precheck_turn_id = candidate.active_precheck_turn_id.clone();
    live.updated_at = candidate.updated_at.clone();
}

/// Atomically publishes one claimed Continue runtime row without replacing the retained workspace.
/// 原子发布一条已领取 Continue 运行态记录，而不替换保留工作区。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative workspace, blockers, and exact admission row.
/// `state` 持有权威工作区、阻断状态与精确准入行。
/// `workspace_snapshot` is the detached candidate containing the newly projected claimed retry reference.
/// `workspace_snapshot` 是包含新投影已领取重试引用的分离候选。
/// `session_id` and `turn_admission` bind the commit to one existing-session submission owner.
/// `session_id` 与 `turn_admission` 将提交绑定到一个已有会话提交所有者。
/// `expected_generation`, `expected_ledger_revision`, and `client_submission_id` identify the exact claim.
/// `expected_generation`、`expected_ledger_revision` 与 `client_submission_id` 标识精确领取。
/// `expected_retry_reference` and `frozen_retry_snapshot` are the exact pre-I/O claim token and body.
/// `expected_retry_reference` 与 `frozen_retry_snapshot` 是 I/O 前的精确领取令牌与正文。
///
/// # Returns
/// 返回值
/// Success only after the frozen state is revalidated and the target runtime row is committed under one lock.
/// 仅在同一锁内重新校验冻结状态并提交目标运行态记录后返回成功。
#[expect(
    clippy::too_many_arguments,
    reason = "The parameters are the complete immutable Continue CAS identity and its exact admission owner."
)]
pub(crate) fn commit_continuation_claim_checkpoint(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    turn_admission: &SubmissionTurnAdmission,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_submission_id: &str,
    expected_retry_reference: &vulcan_workbench_model::ExecutionRetrySnapshotRef,
    frozen_retry_snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<(), String> {
    // candidateRuntimeState contains the target row prepared by this Continue attempt.
    // candidateRuntimeState 包含本次 Continue 尝试准备的目标记录。
    let candidate_runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    // claimedReference is the digest-bound reference returned after publishing the claimed body.
    // claimedReference 是发布已领取正文后返回的摘要绑定引用。
    let claimed_reference = candidate_runtime_state
        .execution_retry_snapshot
        .as_ref()
        .cloned()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
    if claimed_reference.snapshot_id != expected_retry_reference.snapshot_id
        || claimed_reference.generation != expected_retry_reference.generation
        || claimed_reference.logical_execution_id != expected_retry_reference.logical_execution_id
        || !continuation_claim_identity_matches(
            candidate_runtime_state,
            &claimed_reference,
            expected_generation,
            expected_ledger_revision,
            client_submission_id,
        )
    {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
    }

    // guard keeps final frozen validation and target-row publication in one atomic state transaction.
    // guard 将最终冻结校验与目标记录发布保持在同一个原子状态事务中。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // retainedWorkspace is the only authoritative source accepted at the durable claim boundary.
    // retainedWorkspace 是可靠领取边界唯一接受的权威来源。
    let retained_workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
    // blockers excludes only this exact submission-owned reservation.
    // blockers 只排除本次提交精确拥有的预留。
    let blockers = continuation_submission_runtime_blockers(
        &guard,
        retained_workspace,
        session_id,
        turn_admission,
    )?;
    // liveRunPhase is read from the same revision as every other final claim fact.
    // liveRunPhase 与最终领取的其他事实读取自同一个版本。
    let live_run_phase = retained_workspace
        .session_run_phases
        .get(&session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Idle);
    // liveRuntimeState is borrowed for validation so canonical history is not copied before commit.
    // liveRuntimeState 以借用方式参与校验，避免在提交前复制规范历史。
    let live_runtime_state = retained_workspace
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    resolve_session_continuation_eligibility(
        live_runtime_state,
        live_run_phase,
        blockers,
        Some((expected_generation, expected_ledger_revision)),
    )
    .map_err(String::from)?;
    if live_runtime_state.execution_retry_snapshot.as_ref() != Some(expected_retry_reference) {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
    }
    validate_execution_retry_snapshot_for_claim(
        live_runtime_state,
        session_id,
        frozen_retry_snapshot,
    )?;
    // committedRuntimeState is the sole necessary detached copy after the retained row is patched in place.
    // committedRuntimeState 是保留记录原地更新后唯一必要的分离副本。
    let committed_runtime_state = {
        let live_runtime_state = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        apply_continuation_owned_runtime_fields(candidate_runtime_state, live_runtime_state);
        live_runtime_state.clone()
    };
    workspace_snapshot
        .session_runtime_state
        .insert(session_id, committed_runtime_state);
    Ok(())
}

/// Atomically promotes one durably claimed Continue into its exact active Preparing turn.
/// 原子地把一条已可靠领取的 Continue 推进到其精确活动 Preparing 回合。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative claimed row and active runtime kernel.
/// `state` 持有权威已领取记录与活动运行时 kernel。
/// `workspace_snapshot` contains the detached Preparing projection to merge into the target session only.
/// `workspace_snapshot` 包含只应合并到目标会话的分离 Preparing 投影。
/// `session_id`, `turn_admission`, and `preparation_turn_id` bind the same admission owner to the same kernel turn.
/// `session_id`、`turn_admission` 与 `preparation_turn_id` 将同一准入所有者绑定到同一 kernel 回合。
/// The generation, ledger revision, and client id identify the already committed claim pair.
/// 代际、账本修订与客户端标识用于识别已经提交的领取对。
///
/// # Returns
/// 返回值
/// Success after the target runtime, phase, execution-status summary, and new transcript records are merged into the latest workspace.
/// 把目标运行态、阶段、执行状态摘要与新增转录记录合并到最新工作区后返回成功。
#[expect(
    clippy::too_many_arguments,
    reason = "The parameters bind one claimed continuation to its exact admission and kernel owners."
)]
pub(crate) fn commit_continuation_preparing_checkpoint(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    turn_admission: &SubmissionTurnAdmission,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_submission_id: &str,
    preparation_turn_id: &str,
) -> Result<(), String> {
    // candidateRuntimeState is borrowed until the exact live row is patched, avoiding a pre-validation history copy.
    // candidateRuntimeState 会借用到精确实时记录完成更新，避免校验前复制历史。
    let candidate_runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    // claimedReference is the exact durable identity installed by the first checkpoint commit.
    // claimedReference 是第一次 checkpoint 提交安装的精确可靠身份。
    let claimed_reference = candidate_runtime_state
        .execution_retry_snapshot
        .as_ref()
        .cloned()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
    if !continuation_claim_identity_matches(
        candidate_runtime_state,
        &claimed_reference,
        expected_generation,
        expected_ledger_revision,
        client_submission_id,
    ) {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
    }
    // candidatePhase is the explicit phase produced after the exact kernel turn starts.
    // candidatePhase 是精确 kernel 回合启动后生成的显式阶段。
    let candidate_phase = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .copied()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    // candidateSummary contributes only execution-owned status fields, never stale identity metadata.
    // candidateSummary 只提供执行拥有的状态字段，绝不提供陈旧身份元数据。
    let candidate_summary = workspace_session_summary_for_session(workspace_snapshot, session_id)
        .cloned()
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;

    // guard binds claim ownership, kernel ownership, and the session-scoped merge atomically.
    // guard 原子绑定领取所有权、kernel 所有权与会话级合并。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if !turn_admission.owns_current_prompt_submission_reservation(&guard, session_id) {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT",
        ));
    }
    // liveTurnId proves that the runtime kernel accepted this exact restored preparation owner.
    // liveTurnId 证明运行时 kernel 接受了该精确恢复准备所有者。
    let live_turn_id = guard
        .session_runtime_kernels
        .get(&session_id)
        .and_then(|kernel| kernel.active_turn())
        .map(|turn| turn.turn_id.as_str());
    if live_turn_id != Some(preparation_turn_id) {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
    }
    // liveRuntimeState is borrowed for claim validation before the retained row is patched in place.
    // liveRuntimeState 在保留记录原地更新前以借用方式参与领取校验。
    let live_runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    if live_runtime_state.execution_retry_snapshot.as_ref() != Some(&claimed_reference)
        || !continuation_claim_identity_matches(
            live_runtime_state,
            &claimed_reference,
            expected_generation,
            expected_ledger_revision,
            client_submission_id,
        )
    {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
    }
    // committedRuntimeState is the sole detached copy needed by later preparation after the retained row is patched.
    // committedRuntimeState 是保留记录更新后续准备所需的唯一分离副本。
    let committed_runtime_state = {
        let live_runtime_state = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        apply_continuation_owned_runtime_fields(candidate_runtime_state, live_runtime_state);
        live_runtime_state.clone()
    };
    // retainedWorkspace receives only target execution fields and therefore preserves every concurrent unrelated session mutation.
    // retainedWorkspace 只接收目标执行字段，因此会保留所有并发无关会话变更。
    let retained_workspace = guard
        .workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
    let retained_summary_position = retained_workspace
        .sessions
        .iter()
        .position(|session| session.id == session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    // candidateMessages contains only the supplemental records created by this Continue transaction.
    // candidateMessages 只包含本次 Continue 事务新建的补充记录。
    let candidate_messages = workspace_snapshot
        .session_messages
        .remove(&session_id)
        .unwrap_or_default();
    retained_workspace
        .session_run_phases
        .insert(session_id, candidate_phase);
    retained_workspace.hydrated_session_ids.insert(session_id);
    if !candidate_messages.is_empty() {
        merge_submission_message_entries(
            retained_workspace
                .session_messages
                .entry(session_id)
                .or_default(),
            candidate_messages,
        );
    }
    // retainedSummary keeps current title, ownership, and timestamps while accepting only new execution status.
    // retainedSummary 保留当前标题、归属与时间戳，只接纳新的执行状态。
    let committed_summary = {
        let retained_summary = &mut retained_workspace.sessions[retained_summary_position];
        retained_summary.status = candidate_summary.status;
        retained_summary.focus = candidate_summary.focus;
        retained_summary.last_updated_label = candidate_summary.last_updated_label;
        retained_summary.clone()
    };
    workspace_snapshot
        .session_runtime_state
        .insert(session_id, committed_runtime_state);
    // candidateSummary is refreshed from the committed live identity before activation builds its ACK delta.
    // candidateSummary 在 activation 构建 ACK 增量前从已提交的实时身份刷新。
    let local_summary = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    *local_summary = committed_summary;
    Ok(())
}

/// Loads the current Provider configuration after validating one exact connection ownership tuple.
/// 校验一个精确连接归属三元组后加载当前 Provider 配置。
///
/// # Parameters
/// 参数
/// `runtime` owns the current immutable Provider generation and `selection` is the exact requested tuple.
/// `runtime` 持有当前不可变 Provider 代际，`selection` 是精确请求三元组。
///
/// # Returns
/// 返回值
/// The authoritative configuration when the connection exists and belongs to the requested Provider.
/// 当连接存在且属于请求的 Provider 时返回权威配置。
fn validated_submission_model_selection_configuration(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    selection: &SubmissionModelSelection,
) -> Result<vulcan_provider_adapters::PersistedAiConfiguration, String> {
    // configuration is captured once so connection ownership and later route resolution use one generation.
    // configuration 只捕获一次，使连接归属与后续路由解析使用同一代际。
    let configuration = runtime.configuration_snapshot()?;
    let configured_connection = configuration
        .connection_graph
        .connections
        .get(selection.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "Connection `{}` is not configured.",
                selection.connection_id
            )
        })?;
    if configured_connection.provider_id != selection.provider_id {
        return Err(format!(
            "Connection `{}` does not belong to provider `{}`.",
            selection.connection_id, selection.provider_id
        ));
    }
    Ok(configuration)
}

/// Resolves one enabled exact model tuple against the same effective catalog used by execution.
/// 根据执行使用的同一有效目录解析一个已启用的精确模型三元组。
///
/// # Parameters
/// 参数
/// `runtime` owns catalog resolution, `configuration` is one captured generation, and `selection` is exact.
/// `runtime` 持有目录解析，`configuration` 是一次捕获的代际，`selection` 是精确选择。
///
/// # Returns
/// 返回值
/// The fully resolved immutable route, or an explicit unavailable/disabled error without fallback.
/// 返回完整解析的不可变路由；若不可用或被禁用，则返回明确错误且不降级。
fn resolve_enabled_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    selection: &SubmissionModelSelection,
) -> Result<vulcan_provider_adapters::ResolvedPublicAiSelection, String> {
    if !runtime.model_is_enabled_for_connection(
        configuration,
        selection.connection_id.as_str(),
        selection.model_id.as_str(),
    )? {
        return Err(format!(
            "Provider `{}` model `{}` is disabled.",
            selection.provider_id, selection.model_id
        ));
    }
    // resolvedCatalogs reconstruct the same overlay-aware membership used by execution.
    // resolvedCatalogs 重建执行使用的同一份覆盖感知成员关系。
    let resolved_catalogs = runtime.resolved_model_catalog_snapshot(configuration)?;
    let source_catalogs = runtime.source_catalog_snapshot()?;
    let effective_catalogs = if resolved_catalogs.contains_key(selection.connection_id.as_str())
        || source_catalogs.contains_key(selection.connection_id.as_str())
    {
        resolved_catalogs
    } else {
        // Manual catalogs without immutable source tracking retain their process-local authority.
        // 没有不可变来源跟踪的手工目录继续使用其进程内权威。
        runtime.catalog_snapshot()?
    };
    let connection_exposes_model = effective_catalogs
        .get(selection.connection_id.as_str())
        .is_some_and(|catalog| {
            catalog
                .models
                .iter()
                .any(|model| model.id == selection.model_id)
        });
    if !connection_exposes_model {
        return Err(format!(
            "Connection `{}` does not expose model `{}`.",
            selection.connection_id, selection.model_id
        ));
    }
    runtime.resolve_connection_model_selection(
        configuration,
        selection.connection_id.as_str(),
        selection.provider_id.as_str(),
        selection.model_id.as_str(),
    )
}

/// Validates and applies one submission-scoped model selection to session runtime state.
/// 校验并将单次提交范围的模型选择写入会话运行态。
///
/// # Parameters
/// 参数
/// `runtime` owns the authoritative provider configuration and discovered catalogs.
/// `runtime` 持有权威供应商配置与已发现目录。
/// `workspace_snapshot` contains the materialized session runtime state to update.
/// `workspace_snapshot` 包含待更新的已物化会话运行态。
/// `session_id` identifies the exact submitted session.
/// `session_id` 标识本次提交对应的精确会话。
/// `selection` is absent only when the composer has no available model.
/// `selection` 仅在输入区没有可用模型时缺失。
///
/// # Returns
/// 返回值
/// Returns success after an exact configured selection is applied, or a validation error.
/// 在精确已配置选择写入后返回成功，否则返回校验错误。
pub(crate) fn apply_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    selection: Option<&SubmissionModelSelection>,
) -> Result<(), String> {
    // pendingSelection is the exact user choice accepted while the previous turn retained its
    // immutable route. Submission may promote only this tuple and must never silently replace it.
    // pendingSelection 是上一回合保持不可变路由时接受的用户精确选择。提交只能提升该三元组，绝不能静默替换。
    let pending_selection = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.pending_model_selection.clone());
    if pending_selection.is_some() && selection.is_none() {
        return Err(String::from(
            "The pending session model is unavailable. Refresh the model list and explicitly reselect a model before submitting.",
        ));
    }
    let Some(selection) = selection else {
        return Ok(());
    };
    if let Some(pending_selection) = pending_selection.as_ref()
        && (pending_selection.provider_id != selection.provider_id
            || pending_selection.connection_id != selection.connection_id
            || pending_selection.model_id != selection.model_id)
    {
        return Err(String::from(
            "The submitted model does not match the session's pending exact selection. Refresh the model list and reselect explicitly.",
        ));
    }
    let configuration = validated_submission_model_selection_configuration(runtime, selection)?;
    // sessionLeaseScope is the stable owner key shared by process-local and persisted leases.
    // sessionLeaseScope 是进程内租约与持久化租约共享的稳定所有者键。
    let session_lease_scope = session_id.to_string();
    // leasedSelection is evaluated before current-catalog membership because an established
    // session route is intentionally allowed to outlive a hot Manifest model removal.
    // leasedSelection 在当前目录成员关系之前求值，因为已建立会话路由被明确允许在热更新
    // Manifest 移除模型后继续存在。
    let leased_selection = session_process_public_ai_selection(
        runtime,
        session_id,
        workspace_snapshot.session_runtime_state.get(&session_id),
    )?
    .filter(|leased| {
        leased.route.provider.id == selection.provider_id
            && leased.route.connection.id == selection.connection_id
            && leased.model.id == selection.model_id
    });
    if leased_selection.is_some() {
        // Explicit provider/account access revocation still terminates the lease. Manifest-owned
        // catalog/default changes are not treated as user revocation.
        // 显式供应商或账号访问撤销仍会终止租约；Manifest 持有的目录或默认值变化不视为用户撤销。
        let explicitly_disabled = configuration
            .model_enabled_override(selection.provider_id.as_str(), selection.model_id.as_str())
            == Some(false)
            || !configuration.connection_model_is_enabled(
                selection.connection_id.as_str(),
                selection.model_id.as_str(),
            );
        if explicitly_disabled {
            return Err(format!(
                "Provider `{}` model `{}` is disabled.",
                selection.provider_id, selection.model_id
            ));
        }
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        runtime_state.pending_model_selection = None;
        runtime_state.updated_at = current_unix_millis_text();
        return Ok(());
    }
    // resolvedSelection freezes the exact currently installed route used to validate restart drift.
    // resolvedSelection 冻结用于校验重启漂移的当前已安装精确路由。
    let resolved_selection =
        resolve_enabled_submission_model_selection(runtime, &configuration, selection)?;
    // processRouteLeaseAbsent distinguishes restart restoration from an in-process hot replacement.
    // processRouteLeaseAbsent 区分重启恢复与进程内热替换。
    let process_route_lease_absent = runtime
        .session_selection_lease(session_lease_scope.as_str())?
        .is_none();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    // targetChanged distinguishes the composer's per-submit snapshot from an explicit model
    // change. The composer sends its current selection on every turn, so clearing an unchanged
    // target here would silently adopt a newly activated Manifest route on the next turn.
    // targetChanged 区分输入区逐次提交快照与显式模型变更。输入区每轮都会发送当前选择，
    // 因此若在目标未变化时清除租约，下一轮会静默采用新激活的 Manifest 路由。
    let target_changed = runtime_state.active_provider_id.as_deref()
        != Some(selection.provider_id.as_str())
        || runtime_state.active_connection_id.as_deref() != Some(selection.connection_id.as_str())
        || runtime_state.active_model_id.as_deref() != Some(selection.model_id.as_str());
    // resolvedRouteIdentity is comparable with the persisted non-secret identity without retaining credentials.
    // resolvedRouteIdentity 可与持久化的非敏感身份比较，且不会保留凭据。
    let resolved_route_identity =
        vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
            &resolved_selection,
        );
    // restartedRouteChanged permits an ordinary new submission after an application upgrade to
    // adopt the installed route only when its owner and protocol are unchanged. An established
    // process-local lease still wins, preserving in-flight and hot-reload route immutability.
    // restartedRouteChanged 仅允许应用升级后的普通新提交在所有者与协议未变化时采用已安装路由；
    // 已建立的进程内租约仍然优先，从而保持执行中请求与热更新路由不可变。
    let restarted_route_changed = !target_changed
        && process_route_lease_absent
        && runtime_state
            .provider_route_lease
            .as_ref()
            .is_some_and(|expected| {
                provider_route_lease_revision_changed_for_same_target(
                    expected,
                    &resolved_route_identity,
                )
            });
    if target_changed || restarted_route_changed {
        runtime.clear_session_selection_leases(session_lease_scope.as_str())?;
        runtime_state.provider_route_lease = None;
    }
    runtime_state.active_provider_id = Some(selection.provider_id.clone());
    runtime_state.active_connection_id = Some(selection.connection_id.clone());
    runtime_state.active_model_id = Some(selection.model_id.clone());
    runtime_state.pending_model_selection = None;
    runtime_state.updated_at = current_unix_millis_text();
    Ok(())
}

/// Validates a recovery submission's exact future model without mutating the frozen active route.
/// 校验恢复提交的精确未来模型，同时不修改已冻结的当前活动路由。
///
/// # Parameters
/// 参数
/// `runtime` owns the authoritative catalog and process-local route lease.
/// `runtime` 持有权威目录与进程内路由租约。
/// `workspace_snapshot` contains the retained active and pending session selections.
/// `workspace_snapshot` 包含保留的活动与待生效会话选择。
/// `session_id` identifies the interrupted conversation and `selection` is the composer snapshot.
/// `session_id` 标识被中断会话，`selection` 是输入区快照。
///
/// # Returns
/// 返回值
/// Success only when the submitted tuple exactly matches the already-authoritative future selection.
/// 仅当提交三元组精确匹配已有权威未来选择时返回成功。
fn validate_recovery_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    selection: Option<&SubmissionModelSelection>,
) -> Result<(), String> {
    let Some(selection) = selection else {
        return Ok(());
    };
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    let expected_provider_id = runtime_state
        .pending_model_selection
        .as_ref()
        .map(|pending| pending.provider_id.as_str())
        .or(runtime_state.active_provider_id.as_deref());
    let expected_connection_id = runtime_state
        .pending_model_selection
        .as_ref()
        .map(|pending| pending.connection_id.as_str())
        .or(runtime_state.active_connection_id.as_deref());
    let expected_model_id = runtime_state
        .pending_model_selection
        .as_ref()
        .map(|pending| pending.model_id.as_str())
        .or(runtime_state.active_model_id.as_deref());
    if expected_provider_id != Some(selection.provider_id.as_str())
        || expected_connection_id != Some(selection.connection_id.as_str())
        || expected_model_id != Some(selection.model_id.as_str())
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH",
        ));
    }

    let configuration = validated_submission_model_selection_configuration(runtime, selection)?;
    let leased_selection =
        session_process_public_ai_selection(runtime, session_id, Some(runtime_state))?.filter(
            |leased| {
                leased.route.provider.id == selection.provider_id
                    && leased.route.connection.id == selection.connection_id
                    && leased.model.id == selection.model_id
            },
        );
    if leased_selection.is_some() {
        let explicitly_disabled = configuration
            .model_enabled_override(selection.provider_id.as_str(), selection.model_id.as_str())
            == Some(false)
            || !configuration.connection_model_is_enabled(
                selection.connection_id.as_str(),
                selection.model_id.as_str(),
            );
        if explicitly_disabled {
            return Err(String::from(
                "ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH",
            ));
        }
        return Ok(());
    }
    resolve_enabled_submission_model_selection(runtime, &configuration, selection)
        .map(|_| ())
        .map_err(|_| String::from("ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH"))
}

/// Promotes the exact pending session selection at the boundary immediately before a real Provider request.
/// 在真实 Provider 请求之前的边界提升精确的会话待生效选择。
///
/// # Parameters
/// 参数
/// `runtime` owns the installed route catalog, `workspace_snapshot` owns the session authority,
/// and `session_id` selects the exact pending route.
/// `runtime` 持有已安装路由目录，`workspace_snapshot` 持有会话权威，`session_id` 选择精确待生效路由。
///
/// # Returns
/// 返回值
/// The exact promoted selection after validation, or an error that leaves the pending workspace authority unchanged.
/// 校验通过后返回精确的已提升选择；失败时保持工作区待生效权威不变。
/// Validates and returns one pending exact model selection without mutating retained state.
/// 在不修改保留状态的情况下校验并返回一个待生效精确模型选择。
///
/// # Parameters
/// 参数
/// `runtime` owns the installed provider and model configuration.
/// `runtime` 持有已安装供应商与模型配置。
/// `runtime_state` owns the exact pending session tuple to validate.
/// `runtime_state` 持有需要校验的精确待生效会话三元组。
///
/// # Returns
/// 返回值
/// The validated pending tuple, or an explicit missing or unavailable-route error.
/// 返回已校验的待生效三元组，或明确的缺失与路由不可用错误。
pub(crate) fn validated_pending_model_selection_for_provider_request(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<SessionPendingModelSelection, String> {
    let pending_selection = runtime_state
        .pending_model_selection
        .clone()
        .ok_or_else(|| {
            String::from("The session has no pending model selection at the Provider boundary.")
        })?;
    let selection = SubmissionModelSelection {
        provider_id: pending_selection.provider_id.clone(),
        model_id: pending_selection.model_id.clone(),
        connection_id: pending_selection.connection_id.clone(),
    };
    // Validation is pure and therefore cannot expose a candidate route before durable publication.
    // 校验是纯操作，因此不会在可靠发布前暴露候选路由。
    let configuration = validated_submission_model_selection_configuration(runtime, &selection)?;
    let _resolved_selection =
        resolve_enabled_submission_model_selection(runtime, &configuration, &selection)?;
    Ok(pending_selection)
}

#[cfg(test)]
pub(crate) fn apply_pending_model_selection_for_provider_request(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<SessionPendingModelSelection, String> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    let pending_selection =
        validated_pending_model_selection_for_provider_request(runtime, runtime_state)?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    // A pending picker record is created only for a changed future tuple; promotion therefore clears
    // the old persisted route identity while leaving process-local lease revocation to the caller's
    // durability transaction.
    // 只有未来三元组变化时才会创建待生效选择记录；因此提升会清除旧持久路由身份，
    // 而进程内租约撤销由调用方的可靠事务负责。
    runtime_state.active_provider_id = Some(pending_selection.provider_id.clone());
    runtime_state.active_connection_id = Some(pending_selection.connection_id.clone());
    runtime_state.active_model_id = Some(pending_selection.model_id.clone());
    runtime_state.pending_model_selection = None;
    runtime_state.provider_route_lease = None;
    runtime_state.updated_at = current_unix_millis_text();
    Ok(pending_selection)
}

#[cfg(test)]
mod submission_model_selection_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use vulcan_provider_adapters::{
        CredentialConnection, CredentialMode, PersistedAiConfiguration, PublicAiRuntime,
        UnavailableCredentialSecretResolver, parse_openrouter_model_catalog,
    };
    use vulcan_workbench_model::SessionSummary;

    use super::*;
    use crate::ensure_webui_session_runtime_state;

    /// Builds one configured runtime whose global active model differs from the submitted model.
    /// 构建一个全局激活模型与本次提交模型不同的已配置运行时。
    /// Adds one protocol variant for each protocol id to every model of one catalog.
    /// 给一个目录的每个模型按协议 ID 各添加一个协议变体。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_profile_ids` are the exact protocols the models bind to; every model
    /// receives a variant for each id so system-manifest bindings can prove their exact
    /// variant before execution (plan §R5.1).
    /// `protocol_profile_ids` 是模型绑定的精确协议；每个模型为每个 ID 获得一个变体，
    /// 使系统清单绑定能够在执行前证明其精确变体（方案 §R5.1）。
    fn with_protocol_variants(
        mut catalog: vulcan_provider_adapters::ConnectionModelCatalog,
        protocol_profile_ids: &[&str],
    ) -> vulcan_provider_adapters::ConnectionModelCatalog {
        for model in &mut catalog.models {
            for protocol_profile_id in protocol_profile_ids {
                model.protocol_variants.insert(
                    (*protocol_profile_id).to_string(),
                    vulcan_provider_adapters::ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: Default::default(),
                        discovery_signature: String::from("test-signature"),
                        catalog_generation: catalog.generation,
                    },
                );
            }
        }
        catalog
    }

    fn configured_runtime() -> PublicAiRuntime {
        let runtime =
            PublicAiRuntime::first_batch(std::sync::Arc::new(UnavailableCredentialSecretResolver))
                .expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let connection = CredentialConnection {
            id: String::from("openrouter-account"),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential-openrouter-account"),
            display_name: String::from("OpenRouter account"),
            secret_reference: String::from("secret-openrouter-account"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection,
            )
            .expect("connection should register");
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
        configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-terra"),
            connection_id: String::from("openrouter-account"),
        });
        runtime
            .stage_source_catalog_for_testing(with_protocol_variants(
                parse_openrouter_model_catalog(
                    "openrouter-account",
                    1,
                    r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Previous","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-sol","name":"Submitted","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
                )
                .expect("catalog should parse"),
                &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
            ))
            .expect("source catalog should stage");
        runtime
            .replace_configuration(configuration.clone())
            .expect("configuration should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("configured test generation should publish");
        runtime
    }

    /// Verifies a new session uses the model snapshot captured by submit instead of the previous global model.
    /// 验证新会话使用提交时捕获的模型快照，而不是上一次全局模型。
    #[test]
    fn submitted_model_selection_overrides_previous_global_selection() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "New conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        let selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("openrouter-account"),
        };

        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("submitted selection should apply");

        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("session runtime state should exist");
        assert_eq!(
            runtime_state.active_provider_id.as_deref(),
            Some("openrouter")
        );
        assert_eq!(
            runtime_state.active_connection_id.as_deref(),
            Some("openrouter-account")
        );
        assert_eq!(
            runtime_state.active_model_id.as_deref(),
            Some("openai/gpt-5.6-sol")
        );
        assert_eq!(
            runtime
                .configuration_snapshot()
                .expect("configuration should remain readable")
                .active
                .as_ref()
                .map(|active| active.model_id.as_str()),
            Some("openai/gpt-5.6-terra")
        );
    }

    /// Verifies only the exact pending tuple can be promoted for the next real Provider request.
    /// 验证只有精确的待生效三元组才能为下一次真实 Provider 请求提升。
    #[test]
    fn next_provider_request_promotes_only_the_exact_pending_model_selection() {
        // runtime owns both available model identities used by the stale and accepted submissions.
        // runtime 持有过期提交与获准提交使用的两个可用模型身份。
        let runtime = configured_runtime();
        // sessionId identifies the existing session whose active route must not drift before promotion.
        // sessionId 标识在提升前活动路由不得漂移的既有会话。
        let session_id = Uuid::new_v4();
        // workspaceSnapshot contains the materialized session runtime row under test.
        // workspaceSnapshot 包含本测试使用的已物化会话运行态行。
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "Existing conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        // pendingSelection is the exact picker choice accepted for the next Provider request.
        // pendingSelection 是选择器为下一次 Provider 请求接受的精确选择。
        let pending_selection = vulcan_workbench_model::SessionPendingModelSelection {
            provider_id: String::from("openrouter"),
            connection_id: String::from("openrouter-account"),
            model_id: String::from("openai/gpt-5.6-sol"),
        };
        {
            // runtimeState starts on Terra while Sol remains pending.
            // runtimeState 当前使用 Terra，而 Sol 保持待生效。
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
                .expect("session runtime state should exist");
            runtime_state.active_provider_id = Some(String::from("openrouter"));
            runtime_state.active_connection_id = Some(String::from("openrouter-account"));
            runtime_state.active_model_id = Some(String::from("openai/gpt-5.6-terra"));
            runtime_state.pending_model_selection = Some(pending_selection.clone());
        }
        // staleSelection models an old composer snapshot that still names the active model.
        // staleSelection 模拟仍指向活动模型的旧输入区快照。
        let stale_selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            connection_id: String::from("openrouter-account"),
            model_id: String::from("openai/gpt-5.6-terra"),
        };
        let stale_error = apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&stale_selection),
        )
        .expect_err("stale composer selection must not replace pending authority");
        assert!(stale_error.contains("pending exact selection"));
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|state| state.active_model_id.as_deref()),
            Some("openai/gpt-5.6-terra")
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|state| state.pending_model_selection.as_ref()),
            Some(&pending_selection)
        );

        // activeLease models the immutable route already owned by the physical Provider call.
        // activeLease 模拟当前物理 Provider 调用已经持有的不可变路由。
        let configuration = runtime
            .configuration_snapshot()
            .expect("test configuration should remain readable");
        let active_lease = runtime
            .resolve_connection_model_selection(
                &configuration,
                "openrouter-account",
                "openrouter",
                "openai/gpt-5.6-terra",
            )
            .expect("active test route should resolve");
        runtime
            .lease_session_selection(session_id.to_string(), active_lease)
            .expect("active test route should lease");
        // promotedCandidate is detached so Provider-boundary validation cannot mutate retained memory before persistence.
        // promotedCandidate 保持分离，使 Provider 边界校验不能在持久化前修改保留内存。
        let mut promoted_candidate = workspace_snapshot.clone();
        let promoted_selection = apply_pending_model_selection_for_provider_request(
            &runtime,
            &mut promoted_candidate,
            session_id,
        )
        .expect("exact pending selection should promote");
        assert_eq!(promoted_selection, pending_selection);
        // retainedOriginal and processLease prove staging changed neither authority before the caller commits durability.
        // retainedOriginal 与 processLease 证明调用方提交可靠性前，暂存不会修改任一权威状态。
        let retained_original = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("retained original runtime state should remain present");
        assert_eq!(
            retained_original.active_model_id.as_deref(),
            Some("openai/gpt-5.6-terra")
        );
        assert_eq!(
            retained_original.pending_model_selection.as_ref(),
            Some(&pending_selection)
        );
        assert_eq!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("process route lease should remain readable")
                .as_ref()
                .map(|selection| selection.model.id.as_str()),
            Some("openai/gpt-5.6-terra")
        );
        // promotedState proves the pending authority became active exactly once.
        // promotedState 证明待生效权威恰好提升为活动选择一次。
        let promoted_state = promoted_candidate
            .session_runtime_state
            .get(&session_id)
            .expect("promoted runtime state should remain present");
        assert_eq!(
            promoted_state.active_provider_id.as_deref(),
            Some("openrouter")
        );
        assert_eq!(
            promoted_state.active_connection_id.as_deref(),
            Some("openrouter-account")
        );
        assert_eq!(
            promoted_state.active_model_id.as_deref(),
            Some("openai/gpt-5.6-sol")
        );
        assert!(promoted_state.pending_model_selection.is_none());
    }

    /// Verifies a new session rebinds an invalid global account to the preferred account for the same model.
    /// 验证新会话会把失效的全局账号重绑到同一模型的首选账号。
    #[test]
    fn new_session_rebinds_invalid_global_account_for_same_model() {
        // runtime starts with one valid account exposing both test models.
        // runtime 从一个公开两个测试模型的有效账号开始。
        let runtime = configured_runtime();
        let mut configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        // staleConnection represents another grouped credential whose catalog no longer contains the active model.
        // staleConnection 表示同一分组中目录已不再包含活动模型的另一份凭据。
        let stale_connection = CredentialConnection {
            id: String::from("openrouter-stale-account"),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential-openrouter-stale-account"),
            display_name: String::from("OpenRouter stale account"),
            secret_reference: String::from("secret-openrouter-stale-account"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                stale_connection.clone(),
            )
            .expect("stale connection should register");
        configuration
            .connection_graph
            .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "openrouter-account")
            .expect("valid model account preference should persist");
        configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: stale_connection.id.clone(),
        });
        // staleCatalog deliberately omits the globally selected model while remaining a valid account catalog.
        // staleCatalog 有意省略全局所选模型，同时保持为有效账号目录。
        let stale_catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                stale_connection.id.as_str(),
                1,
                r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Stale account model","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("stale account catalog should parse"),
            &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
        );
        runtime
            .stage_source_catalog_for_testing(stale_catalog)
            .expect("stale account source catalog should stage");
        runtime
            .replace_configuration(configuration.clone())
            .expect("invalid global account fixture should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("account-scoped catalogs should publish");

        let selection = crate::public_ai_workbench_selection(&configuration, &runtime, None, None)
            .expect("new-session selection should resolve")
            .expect("same model should remain available");

        assert_eq!(selection.route.connection.provider_id, "openrouter");
        assert_eq!(selection.model.id, "openai/gpt-5.6-sol");
        assert_eq!(selection.route.connection.id, "openrouter-account");
    }

    /// Verifies an unchanged per-submit model snapshot preserves the session route lease.
    /// 验证逐次提交中未变化的模型快照会保留会话路由租约。
    #[test]
    fn repeated_submission_selection_preserves_lease_until_target_changes() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "Existing conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        let selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("openrouter-account"),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("initial selection should apply");

        // leasedSelection simulates the immutable route installed by the first prepared turn.
        // leasedSelection 模拟首个已准备回合安装的不可变路由。
        let configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        let leased_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .expect("selected route should resolve");
        let lease_identity =
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
                &leased_selection,
            );
        runtime
            .lease_session_selection(session_id.to_string(), leased_selection.clone())
            .expect("session route should lease");
        workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .expect("session runtime state should exist")
            .provider_route_lease = Some(lease_identity.clone());

        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("unchanged submission snapshot should apply");
        assert_eq!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("session lease should remain readable")
                .as_ref()
                .map(|selection| selection.route_revision.as_str()),
            Some(leased_selection.route_revision.as_str()),
            "an unchanged per-submit snapshot must not clear the process-local lease"
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.provider_route_lease.as_ref()),
            Some(&lease_identity),
            "an unchanged per-submit snapshot must not clear the persisted lease identity"
        );

        // replacementCatalog simulates a hot catalog generation that removes the leased model.
        // replacementCatalog 模拟移除已租用模型的热更新目录代际。
        let replacement_catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                "openrouter-account",
                2,
                r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Replacement","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("replacement catalog should parse"),
            &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
        );
        runtime
            .stage_source_catalog_for_testing(replacement_catalog)
            .expect("replacement source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("replacement generation should publish");
        let replacement_catalogs = runtime
            .catalog_snapshot()
            .expect("replacement catalogs should remain readable");
        assert!(
            replacement_catalogs
                .get("openrouter-account")
                .is_some_and(|catalog| catalog
                    .models
                    .iter()
                    .all(|model| model.id != selection.model_id)),
            "the current catalog must actually remove the leased model"
        );

        let selected_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
        let workbench_selection = crate::public_ai_workbench_selection(
            &configuration,
            &runtime,
            Some(session_id),
            selected_runtime_state,
        )
        .expect("lease-aware workbench selection should resolve")
        .expect("the process-local lease must remain the effective selection");
        assert_eq!(
            workbench_selection.route_revision, leased_selection.route_revision,
            "a hot catalog removal must not replace the selected session route"
        );
        let rows = crate::public_ai_provider_rows_for_selection(
            &configuration,
            &runtime,
            Some(&workbench_selection),
        )
        .expect("lease-aware provider rows should project");
        assert!(
            rows.iter()
                .any(|row| row.catalog_models.iter().any(|model| {
                    model.id == selection.model_id
                        && model.active
                        && !model.preferred_connection
                        && model.status.is_some()
                })),
            "the leased model must remain visible only as a selected, non-preferred session row"
        );
        let changed_sessions =
            crate::public_ai_settings::reconcile_missing_session_model_selections(
                &mut workspace_snapshot,
                &runtime,
                &configuration,
                &replacement_catalogs,
            )
            .expect("lease-aware session reconciliation should succeed");
        assert!(
            changed_sessions.is_empty(),
            "hot catalog removal must not reconcile an established session lease"
        );
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("the unchanged leased selection must survive current catalog removal");
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("session lease should remain readable")
                .is_some(),
            "per-submit validation must not clear a model removed only by hot catalog refresh"
        );

        // Clearing only the process snapshot simulates a restart while persisted lease identity
        // remains authoritative for drift detection.
        // 只清除进程快照用于模拟重启，而持久化租约身份仍对漂移检测保持权威。
        runtime
            .clear_session_selection_leases(session_id.to_string().as_str())
            .expect("process-local lease should clear for restart simulation");
        let changed_after_restart =
            crate::public_ai_settings::reconcile_missing_session_model_selections(
                &mut workspace_snapshot,
                &runtime,
                &configuration,
                &replacement_catalogs,
            )
            .expect("restart reconciliation should preserve persisted lease evidence");
        assert!(
            changed_after_restart.is_empty(),
            "restart reconciliation must not silently replace a drifted persisted lease"
        );
        let restarted_runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("session runtime state should remain present");
        assert_eq!(
            restarted_runtime_state.active_model_id.as_deref(),
            Some(selection.model_id.as_str()),
            "the persisted target must remain available to exact recovery validation"
        );
        assert_eq!(
            restarted_runtime_state.provider_route_lease.as_ref(),
            Some(&lease_identity),
            "the persisted lease identity must survive restart catalog reconciliation"
        );

        let replacement = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-terra"),
            connection_id: String::from("openrouter-account"),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&replacement),
        )
        .expect("changed submission target should apply");
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("session lease should remain readable")
                .is_none(),
            "an explicit target change must clear the process-local lease"
        );
        assert!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.provider_route_lease.as_ref())
                .is_none(),
            "an explicit target change must clear the persisted lease identity"
        );
    }

    /// Verifies an ordinary submission adopts a revised installed route after restart without weakening hot leases.
    /// 验证普通提交在重启后采用已修订的安装路由，同时不削弱进程内热租约。
    #[test]
    fn unchanged_submission_adopts_revised_route_after_restart() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "Restarted conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        let selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("openrouter-account"),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("initial selection should apply");
        let configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        let previous_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .expect("previous route should resolve");
        let previous_identity =
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
                &previous_selection,
            );
        workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .expect("session runtime state should exist")
            .provider_route_lease = Some(previous_identity.clone());

        // revisedCatalog keeps the exact model owner and protocol while changing an execution capability.
        // revisedCatalog 保持完全相同的模型所有者与协议，同时修改一项执行能力。
        let revised_catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                "openrouter-account",
                2,
                r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Previous","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-sol","name":"Submitted","context_length":16384,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("revised catalog should parse"),
            &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
        );
        runtime
            .stage_source_catalog_for_testing(revised_catalog)
            .expect("revised source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("revised generation should publish");
        let revised_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .expect("revised route should resolve");
        assert_ne!(
            previous_selection.route_revision, revised_selection.route_revision,
            "the fixture must change only the installed execution revision"
        );
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("process-local lease should remain readable")
                .is_none(),
            "restart simulation must not retain a process-local route snapshot"
        );

        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("ordinary submission should adopt the revised installed route");
        assert!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.provider_route_lease.as_ref())
                .is_none(),
            "the obsolete persisted identity must clear before execution preparation"
        );
        let blueprint =
            vulcan_agent_runtime::agent_runtime::provider::resolve_public_ai_provider_blueprint(
                &runtime,
                session_id.to_string().as_str(),
                None,
                vulcan_agent_runtime::provider_config::ReasoningEffortLevel::Off,
                Some(selection.provider_id.as_str()),
                Some(selection.model_id.as_str()),
                Some(selection.connection_id.as_str()),
            )
            .expect("execution preparation should lease the revised route without reselection");
        assert_eq!(
            blueprint.route_revision, revised_selection.route_revision,
            "the first post-restart ordinary submission must use the installed route revision"
        );
    }

    /// Verifies a pre-dispatch Manifest failure revokes both stale route identities before retry.
    /// 验证分发前的 Manifest 失败会在重试前撤销两处陈旧路由身份。
    #[test]
    fn resume_preparation_retry_adopts_revised_same_protocol_route() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        let previous_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "openrouter-account",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("previous route should resolve");
        let previous_identity =
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
                &previous_selection,
            );
        runtime
            .lease_session_selection(session_id.to_string(), previous_selection)
            .expect("previous route should lease");

        let revised_catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                "openrouter-account",
                2,
                r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Previous","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-sol","name":"Submitted","context_length":16384,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("revised catalog should parse"),
            &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
        );
        runtime
            .stage_source_catalog_for_testing(revised_catalog)
            .expect("revised source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("revised generation should publish");

        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("openrouter")),
            active_connection_id: Some(String::from("openrouter-account")),
            active_model_id: Some(String::from("openai/gpt-5.6-sol")),
            provider_route_lease: Some(previous_identity.clone()),
            ..Default::default()
        };
        let preparation_input = vulcan_workbench_model::ExecutionRetryPreparationInput {
            preparation_turn_id: String::from("turn-a"),
            session_title: String::from("session-a"),
            workspace_path: String::from("D:\\workspace"),
            active_provider_id: Some(String::from("openrouter")),
            active_connection_id: Some(String::from("openrouter-account")),
            active_model_id: Some(String::from("openai/gpt-5.6-sol")),
            original_source: PendingSessionPromptSource::UserFollowup,
            prompt: String::from("retry prompt"),
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: Vec::new(),
            execution_mode: String::from("build"),
            reasoning_effort: None,
            boundary_events: Vec::new(),
            canonical_user_message_id: None,
            precheck_payload: serde_json::json!({}),
            profile_payload: serde_json::json!({}),
        };
        let mut retry_snapshot = vulcan_workbench_model::ExecutionRetrySnapshot {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            generation: 1,
            state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
            logical_execution_id: String::from("logical-a"),
            logical_turn_id: Some(String::from("turn-a")),
            next_provider_attempt_index: 0,
            physical_attempt_id: String::from("attempt-a"),
            claimed_attempt_id: None,
            provider_route: Some(previous_identity),
            provider_service_entry_id: None,
            credential_profile_id: None,
            controller_config: None,
            controller_snapshot: vulcan_core::types::ConversationSnapshot::default(),
            pending_action:
                vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
                    input: Box::new(preparation_input),
                },
            user_followup: None,
            error: Some(vulcan_ai_contract::AiExecutionError {
                category: vulcan_ai_contract::AiErrorCategory::Cancelled,
                subcategory: Some(String::from("execution_interrupted")),
                stage: String::from("service_execution"),
                http_status: None,
                transport: None,
                websocket_close_code: None,
                upstream_code: None,
                upstream_type: None,
                upstream_parameter: None,
                upstream_message: None,
                request_id: None,
                provider_id: Some(String::from("openrouter")),
                entry_id: None,
                model_id: Some(String::from("openai/gpt-5.6-sol")),
                message: String::from(
                    "The provider manifest changed after this session activated; reselect the model to adopt the new route.",
                ),
                dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Unknown,
                semantic_output_observed: false,
                side_effect_observed: false,
                terminal: true,
                retryable: true,
                retry_after: None,
            }),
            canonical_digest: "a".repeat(64),
            context_generation_id: None,
            created_at: String::from("1000"),
            updated_at: String::from("1000"),
        };

        let mut dispatched_runtime_state = runtime_state.clone();
        let mut dispatched_snapshot = retry_snapshot.clone();
        dispatched_snapshot
            .error
            .as_mut()
            .expect("fixture error should exist")
            .dispatch_outcome = vulcan_ai_contract::DispatchOutcome::Dispatched;
        refresh_resume_preparation_route_after_manifest_change(
            &runtime,
            session_id,
            &mut dispatched_runtime_state,
            &mut dispatched_snapshot,
        )
        .expect("explicitly dispatched evidence should preserve the frozen route");
        assert!(dispatched_runtime_state.provider_route_lease.is_some());
        assert!(dispatched_snapshot.provider_route.is_some());

        refresh_resume_preparation_route_after_manifest_change(
            &runtime,
            session_id,
            &mut runtime_state,
            &mut retry_snapshot,
        )
        .expect("safe pre-dispatch retry should adopt the revised route");

        assert!(runtime_state.provider_route_lease.is_none());
        assert!(retry_snapshot.provider_route.is_none());
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("process lease should remain readable")
                .is_none()
        );
    }

    /// Verifies Continue claims the frozen preparation before restoring its own precheck marker.
    /// 验证 Continue 会在恢复自身 precheck 标记之前领取冻结准备态。
    #[test]
    fn continuation_claim_precedes_retry_precheck_restore() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("openrouter")),
            active_connection_id: Some(String::from("openrouter-account")),
            active_model_id: Some(String::from("openai/gpt-5.6-sol")),
            ..Default::default()
        };
        let error_entry = vulcan_workbench_model::SessionMessageEntry::new(
            vulcan_workbench_model::SessionMessageKind::Error,
            "Provider error",
            "failed",
        )
        .with_runtime_metadata("error-a", "1000");
        runtime_state
            .conversation_state
            .records
            .push(vulcan_context::SessionConversationRecord {
                id: error_entry.id,
                kind: error_entry.kind,
                record_kind: error_entry.record_kind,
                visible: error_entry.visible,
                title: error_entry.title,
                body: error_entry.body,
                metadata: error_entry.metadata,
                created_at: error_entry.created_at,
            });
        assert!(
            runtime_state
                .provider_call_ledger
                .mark_continuation_terminal(
                    Some(String::from("logical-a")),
                    String::from("preparation_failed"),
                    1000,
                    String::from("error"),
                    true,
                )
        );
        let continuation = runtime_state.provider_call_ledger.continuation.clone();
        let controller_snapshot = vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
            &runtime_state.conversation_state,
        )
        .expect("canonical retry state should project");
        let canonical_bytes = serde_json::to_vec(&(
            &controller_snapshot.messages,
            &controller_snapshot.continuation,
        ))
        .expect("canonical retry state should encode");
        let preparation_turn_id = String::from("precheck-a");
        let mut retry_snapshot = vulcan_workbench_model::ExecutionRetrySnapshot {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            generation: 1,
            state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
            logical_execution_id: String::from("logical-a"),
            logical_turn_id: Some(preparation_turn_id.clone()),
            next_provider_attempt_index: 0,
            physical_attempt_id: String::from("attempt-a"),
            claimed_attempt_id: None,
            provider_route: None,
            provider_service_entry_id: None,
            credential_profile_id: None,
            controller_config: None,
            controller_snapshot,
            pending_action:
                vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
                    input: Box::new(vulcan_workbench_model::ExecutionRetryPreparationInput {
                        preparation_turn_id: preparation_turn_id.clone(),
                        session_title: String::from("session-a"),
                        workspace_path: String::from("D:\\workspace"),
                        active_provider_id: Some(String::from("openrouter")),
                        active_connection_id: Some(String::from("openrouter-account")),
                        active_model_id: Some(String::from("openai/gpt-5.6-sol")),
                        original_source: PendingSessionPromptSource::UserFollowup,
                        prompt: String::from("retry prompt"),
                        hidden_model_guidance: None,
                        provider_visible_agent_messages: Vec::new(),
                        attachments: Vec::new(),
                        provider_attachments: Vec::new(),
                        execution_mode: String::from("build"),
                        reasoning_effort: None,
                        boundary_events: Vec::new(),
                        canonical_user_message_id: None,
                        precheck_payload: serde_json::json!({}),
                        profile_payload: serde_json::json!({}),
                    }),
                },
            user_followup: None,
            error: None,
            canonical_digest: execution_retry_sha256_hex(&canonical_bytes),
            context_generation_id: None,
            created_at: String::from("1000"),
            updated_at: String::from("1000"),
        };
        // retrySnapshotBytes models the immutable sidecar bytes that the production store hashes before exposing its reference.
        // retrySnapshotBytes 模拟生产存储在暴露引用前计算摘要的不可变 sidecar 字节。
        let retry_snapshot_bytes =
            serde_json::to_vec(&retry_snapshot).expect("retry snapshot should serialize");
        // retrySnapshotContentBytes preserves the exact non-zero serialized body length.
        // retrySnapshotContentBytes 保留精确且非零的序列化正文长度。
        let retry_snapshot_content_bytes = u64::try_from(retry_snapshot_bytes.len())
            .expect("retry snapshot fixture length should fit u64");
        // retrySnapshotContentSha256 is the content-addressed identity mirrored by the lightweight reference.
        // retrySnapshotContentSha256 是轻量引用镜像的内容寻址身份。
        let retry_snapshot_content_sha256 = execution_retry_sha256_hex(&retry_snapshot_bytes);
        // The resolver reads the lightweight reference and the explicit sidecar together.
        // 解析器会同时读取轻量引用与显式 sidecar。
        runtime_state.execution_retry_snapshot =
            Some(vulcan_workbench_model::ExecutionRetrySnapshotRef {
                schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
                snapshot_id: retry_snapshot.snapshot_id.clone(),
                generation: retry_snapshot.generation,
                state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
                content_sha256: retry_snapshot_content_sha256,
                content_bytes: retry_snapshot_content_bytes,
                logical_execution_id: retry_snapshot.logical_execution_id.clone(),
                pending_action_label: String::from("resume_preparation"),
                provider_route: retry_snapshot.provider_route.clone(),
                preparation_route_selection: Some(match &retry_snapshot.pending_action {
                    vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
                        input,
                    } => vulcan_workbench_model::ExecutionRetryPreparationRouteSelection {
                        active_provider_id: input.active_provider_id.clone(),
                        active_connection_id: input.active_connection_id.clone(),
                        active_model_id: input.active_model_id.clone(),
                    },
                    _ => panic!("fixture must retain a preparation action"),
                }),
                context_generation_id: retry_snapshot.context_generation_id.clone(),
                canonical_dispatch_sha256: retry_snapshot.canonical_digest.clone(),
                retryable: true,
                requires_manual_confirmation: false,
                blocked_reason: None,
                claimed_attempt_id: None,
                error_summary: None,
                updated_at: retry_snapshot.updated_at.clone(),
            });
        // expectedRetryReference preserves the exact lightweight pointer observed before the sidecar body was loaded.
        // expectedRetryReference 保留加载 sidecar 正文前观察到的精确轻量指针。
        let expected_retry_reference = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .expect("fixture retry reference should exist")
            .clone();
        // racedRuntimeState simulates another checkpoint replacing the lightweight reference after the body read.
        // racedRuntimeState 模拟正文读取后另一个 checkpoint 替换轻量引用。
        let mut raced_runtime_state = runtime_state.clone();
        raced_runtime_state
            .execution_retry_snapshot
            .as_mut()
            .expect("raced retry reference should exist")
            .content_sha256 = "f".repeat(64);
        // racedRetrySnapshot remains the stale body previously loaded by the caller.
        // racedRetrySnapshot 保持为调用方先前加载的陈旧正文。
        let mut raced_retry_snapshot = retry_snapshot.clone();
        let stale_error = claim_continuation_before_precheck_restore(
            &runtime,
            &mut raced_runtime_state,
            session_id,
            SessionRunPhase::Failed,
            SessionContinuationRuntimeBlockers::default(),
            continuation.generation,
            continuation.expected_ledger_revision,
            "continue-attempt-stale",
            &expected_retry_reference,
            &mut raced_retry_snapshot,
            None,
        )
        .expect_err("a replaced retry reference must reject the stale sidecar body");
        assert_eq!(stale_error, "ERR_CONVERSATION_CONTINUE_STALE");
        assert!(
            raced_runtime_state
                .provider_call_ledger
                .continuation
                .claimed_at_ms
                .is_none()
        );
        assert_eq!(
            raced_retry_snapshot.state,
            vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen
        );

        // userFollowup is installed only in the claimed sidecar after the original frozen body wins CAS.
        // userFollowup 仅在原始冻结正文赢得 CAS 后安装到已领取 sidecar。
        let user_followup = vulcan_workbench_model::ExecutionRetryUserFollowup {
            visible_prompt: String::from("supplemental instruction"),
            provider_prompt: String::from("supplemental instruction"),
            client_submission_id: String::from("continue-attempt-a"),
        };
        claim_continuation_before_precheck_restore(
            &runtime,
            &mut runtime_state,
            session_id,
            SessionRunPhase::Failed,
            SessionContinuationRuntimeBlockers::default(),
            continuation.generation,
            continuation.expected_ledger_revision,
            "continue-attempt-a",
            &expected_retry_reference,
            &mut retry_snapshot,
            Some(&user_followup),
        )
        .expect("Continue should claim before retry precheck restoration");
        runtime_state.active_precheck_turn_id = Some(preparation_turn_id);

        assert_eq!(
            retry_snapshot.state,
            vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed
        );
        assert_eq!(
            retry_snapshot.claimed_attempt_id.as_deref(),
            Some("continue-attempt-a")
        );
        assert_eq!(retry_snapshot.user_followup.as_ref(), Some(&user_followup));
        assert!(
            runtime_state
                .provider_call_ledger
                .continuation
                .claimed_at_ms
                .is_some()
        );
        assert_eq!(
            runtime_state.active_precheck_turn_id.as_deref(),
            Some("precheck-a")
        );
    }
}

/// Determines whether an existing submission must hydrate missing dialogue from persistence.
/// 判断已有会话提交是否必须从持久化层水合缺失对话。
///
/// # Parameters
/// 参数
/// `session_id` is present only for submissions targeting an existing session.
/// `session_id` 仅在提交目标为已有会话时存在。
/// `policy` describes whether the retained snapshot is authoritative when empty.
/// `policy` 描述保留快照为空时是否仍具权威性。
///
/// # Returns
/// 返回值
/// Returns true only when an existing ordinary submission requires hydration.
/// 仅当已有会话的普通提交需要水合时返回 true。
fn existing_submission_requires_hydration(
    session_id: Option<SessionId>,
    policy: ExistingSessionHydrationPolicy,
) -> bool {
    session_id.is_some() && policy == ExistingSessionHydrationPolicy::HydrateIfEmpty
}

/// Inserts one client submission identifier into a visible user record and its canonical runtime record.
/// 将一个客户端提交标识写入可见用户记录及其规范运行时记录。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the newly appended transcript and runtime records.
/// `workspace_snapshot` 持有刚追加的转录记录与运行时记录。
/// `session_id` selects the exact accepted conversation.
/// `session_id` 选择精确的已接纳会话。
/// `message_id` is the visible user record identifier returned by the shared turn reducer.
/// `message_id` 是共享回合 reducer 返回的可见用户记录标识。
/// `client_submission_id` is the stable browser-generated correlation identifier.
/// `client_submission_id` 是浏览器生成的稳定关联标识。
/// `submission_target` identifies a new or existing conversation submission.
/// `submission_target` 标识新会话或已有会话提交。
/// `accepted_at_ms` captures the service acceptance boundary for first-semantic latency.
/// `accepted_at_ms` 捕获用于首个语义输出延迟的服务接纳边界。
///
/// # Returns
/// 返回值
/// Success after both projections own the identifier, or an exact record-shape diagnostic.
/// 两个投影都拥有标识后返回成功，否则返回精确的记录结构诊断。
pub(crate) fn attach_client_submission_id_to_visible_user_record(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    message_id: Option<&str>,
    client_submission_id: &str,
    submission_target: &str,
    accepted_at_ms: i128,
) -> Result<(), String> {
    let Some(message_id) = message_id else {
        return Ok(());
    };
    // insert_identifier preserves the established object metadata contract and rejects a corrupted non-object value.
    // insert_identifier 保留既有对象元数据契约，并拒绝损坏的非对象值。
    let insert_identifier = |metadata: &mut Option<serde_json::Value>| -> Result<(), String> {
        if metadata.is_none() {
            *metadata = Some(serde_json::Value::Object(serde_json::Map::new()));
        }
        let object = metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| {
                format!("Visible user record `{message_id}` has non-object metadata.")
            })?;
        object.insert(
            String::from("client_submission_id"),
            serde_json::Value::String(client_submission_id.to_string()),
        );
        object.insert(
            String::from("client_submission_target"),
            serde_json::Value::String(submission_target.to_string()),
        );
        object.insert(
            String::from("client_submission_accepted_at_ms"),
            serde_json::Value::String(accepted_at_ms.to_string()),
        );
        Ok(())
    };
    let visible_message = workspace_snapshot
        .session_messages
        .get_mut(&session_id)
        .and_then(|messages| messages.iter_mut().find(|message| message.id == message_id))
        .ok_or_else(|| format!("Visible user record `{message_id}` was not projected."))?;
    insert_identifier(&mut visible_message.metadata)?;
    let runtime_record = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .and_then(|runtime_state| {
            runtime_state
                .conversation_state
                .records
                .iter_mut()
                .find(|record| record.id == message_id)
        })
        .ok_or_else(|| format!("Canonical user record `{message_id}` was not retained."))?;
    insert_identifier(&mut runtime_record.metadata)
}

/// User-authored composer fields carried by a recovery submission on a problem turn.
/// 问题回合恢复提交所携带的用户输入区字段。
#[derive(Debug, Clone)]
struct ContinuationUserSubmission {
    /// User-visible supplemental prompt.
    /// 用户可见的补充提示词。
    prompt: String,
    /// Optional model-only host guidance appended only to Provider-visible text.
    /// 仅追加到 Provider 可见文本的可选模型专用宿主引导。
    hidden_model_guidance: Option<String>,
    /// Optional execution mode selected for the recovered run.
    /// 为恢复执行选择的可选执行模式。
    execution_mode: Option<String>,
    /// Optional reasoning effort selected for the recovered run.
    /// 为恢复执行选择的可选推理强度。
    reasoning_effort: Option<String>,
    /// Exact composer model snapshot, validated without changing the frozen route.
    /// 不修改冻结路由即可校验的精确输入区模型快照。
    model_selection: Option<SubmissionModelSelection>,
    /// Composer attachments retained for explicit unsupported-input rejection.
    /// 为显式拒绝不支持输入而保留的输入区附件。
    attachments: Option<Vec<ComposerAttachmentSummary>>,
}

/// Builds one durable recovery follow-up with distinct visible and Provider prompt text.
/// 构建一条区分可见文本与 Provider 文本的可靠恢复补充消息。
///
/// # Parameters
/// 参数
/// `submission` owns the user-authored composer fields and `client_submission_id` owns admission identity.
/// `submission` 持有用户编写的输入区字段，`client_submission_id` 持有准入身份。
///
/// # Returns
/// 返回值
/// A structurally valid sidecar follow-up, or an exact request error.
/// 返回结构有效的 sidecar 补充消息，或精确请求错误。
fn execution_retry_user_followup_from_submission(
    submission: &ContinuationUserSubmission,
    client_submission_id: &str,
) -> Result<vulcan_workbench_model::ExecutionRetryUserFollowup, String> {
    // visiblePrompt is the exact trimmed text that will appear in canonical user history.
    // visiblePrompt 是将出现在规范用户历史中的精确去空白文本。
    let visible_prompt = submission.prompt.trim();
    if visible_prompt.is_empty() {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_FOLLOWUP_EMPTY"));
    }
    // providerPrompt may add host-only guidance without leaking it into the visible transcript.
    // providerPrompt 可以追加仅宿主可见的引导，且不会泄漏到可见转录。
    let provider_prompt = match submission
        .hidden_model_guidance
        .as_deref()
        .map(str::trim)
        .filter(|guidance| !guidance.is_empty())
    {
        Some(guidance) => format!(
            "{visible_prompt}\n\n<hidden_host_guidance>\n{guidance}\n</hidden_host_guidance>"
        ),
        None => visible_prompt.to_string(),
    };
    // followup is the durable pair consumed only after prior tool-call reconciliation.
    // followup 是仅在旧工具调用完成对账后消费的可靠文本对。
    let followup = vulcan_workbench_model::ExecutionRetryUserFollowup {
        visible_prompt: visible_prompt.to_string(),
        provider_prompt,
        client_submission_id: client_submission_id.to_string(),
    };
    if !followup.is_structurally_valid() {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_FOLLOWUP_INVALID",
        ));
    }
    Ok(followup)
}

/// Claims one frozen continuation and starts the exact retained execution without user input.
/// 领取一份冻结 Continue，并在不追加用户输入的情况下启动精确保留执行。
pub(super) async fn service_continue_conversation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_continuation_id: String,
) -> Result<SubmitConversationPromptAck, String> {
    service_continue_conversation_with_submission(
        app,
        state,
        conversation_id,
        expected_generation,
        expected_ledger_revision,
        client_continuation_id,
        None,
    )
    .await
}

/// Claims one frozen continuation and starts its retained execution with an optional durable user follow-up.
/// 领取一份冻结 Continue，并使用可选可靠用户补充消息启动其保留执行。
///
/// # Parameters
/// 参数
/// `app` emits the accepted continuation state and subsequent runtime deltas.
/// `app` 发出已接受的 Continue 状态与后续运行时增量。
/// `state` owns the retained workspace and canonical conversation state.
/// `state` 持有保留的工作区与规范会话状态。
/// `conversation_id` identifies the terminated WebUI conversation.
/// `conversation_id` 标识已终止的 WebUI 会话。
/// `expected_generation` is the exact continuation generation projected to the WebUI.
/// `expected_generation` 是投影给 WebUI 的精确 Continue 代际。
/// `expected_ledger_revision` is the exact provider-call ledger revision projected to the WebUI.
/// `expected_ledger_revision` 是投影给 WebUI 的精确 Provider 调用账本修订号。
/// `client_continuation_id` is the stable idempotency identity for one UI recovery intent.
/// `client_continuation_id` 是一次 UI 恢复意图使用的稳定幂等标识。
/// `user_submission` optionally supplies composer text and selections that must be reconciled into the recovery.
/// `user_submission` 可选提供必须协调进恢复流程的输入区文本与选择项。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after the retained provider state is admitted for continuation.
/// 保留的 Provider 状态获准继续后的轻量确认信息。
async fn service_continue_conversation_with_submission(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_continuation_id: String,
    user_submission: Option<ContinuationUserSubmission>,
) -> Result<SubmitConversationPromptAck, String> {
    let client_continuation_id = client_continuation_id.trim().to_string();
    if client_continuation_id.is_empty() {
        return Err(String::from("ERR_CONVERSATION_CONTINUE_CLIENT_ID_EMPTY"));
    }
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION"))?;
    // retryReference is read with eligibility so Continue cannot race a changed runtime snapshot.
    // retryReference 与资格信息在同一锁内读取，避免 Continue 与变化后的运行态快照竞争。
    let (controller_runtime, retry_reference, session_directory) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        let run_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        let retry_reference = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
        // existingClaimDisposition distinguishes a committed duplicate from the pre-persistence in-memory window.
        // existingClaimDisposition 区分已提交的重复领取与持久化前的纯内存窗口。
        let existing_claim_disposition = classify_existing_continuation_claim(
            runtime_state,
            retry_reference,
            expected_generation,
            expected_ledger_revision,
            client_continuation_id.as_str(),
            run_phase,
            webui_session_kernel_has_active_turn(&guard, session_id),
        );
        match existing_claim_disposition {
            ExistingContinuationClaimDisposition::DurablyAdmitted => {
                // revision is the already-published projection revision for the durable duplicate ACK.
                // revision 是持久重复 ACK 使用的已发布投影修订号。
                let revision = guard.workbench_generation;
                return Ok(SubmitConversationPromptAck {
                    client_submission_id: client_continuation_id,
                    conversation_id,
                    accepted_message_id: None,
                    turn_id: None,
                    run_phase: run_phase.to_string(),
                    queued: false,
                    revision,
                });
            }
            ExistingContinuationClaimDisposition::AdmissionInFlight => {
                return Err(String::from(CONTINUATION_CLAIM_IN_FLIGHT_REASON));
            }
            ExistingContinuationClaimDisposition::NotOwned => {}
        }
        let blockers = session_continuation_runtime_blockers(&guard, workspace, session_id)?;
        resolve_session_continuation_eligibility(
            runtime_state,
            run_phase,
            blockers,
            Some((expected_generation, expected_ledger_revision)),
        )
        .map_err(String::from)?;
        let retry_reference = retry_reference.clone();
        if retry_reference.state != vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen {
            return Err(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN",
            ));
        }
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        let session_directory = require_segmented_session_directory_for_retained_session(
            &guard,
            &controller_runtime.data_root,
            session_id,
        )?;
        (controller_runtime, retry_reference, session_directory)
    };
    // retrySnapshot is the exact sidecar payload; unreadable bodies durably invalidate the visible claim token.
    // retrySnapshot 是精确 sidecar 载荷；不可读取的正文会持久使可见领取令牌失效。
    let retry_snapshot =
        match vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
            &session_directory,
            session_id,
            &retry_reference,
        ) {
            Ok(snapshot) => snapshot,
            Err(load_error) => {
                eprintln!(
                    "[webui-service] execution retry sidecar validation failed for session {session_id}: {load_error}"
                );
                if let Err(invalidation_error) = persist_execution_retry_claim_failure_invalidation(
                    &app,
                    &state,
                    &controller_runtime,
                    session_id,
                    &retry_reference,
                    EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_REASON,
                    EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_SUMMARY,
                )
                .await
                {
                    eprintln!(
                        "[webui-service] unreadable execution retry invalidation failed for session {session_id}: {invalidation_error}"
                    );
                }
                return Err(String::from(
                    EXECUTION_RETRY_SNAPSHOT_BODY_UNREADABLE_REASON,
                ));
            }
        };
    // initialValidation rejects deterministic body drift before the shared submission pipeline reserves work.
    // initialValidation 在共享提交流水线预留工作前拒绝确定性的正文漂移。
    let initial_validation = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        if runtime_state.execution_retry_snapshot.as_ref() != Some(&retry_reference) {
            Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"))
        } else {
            validate_execution_retry_snapshot_for_claim(runtime_state, session_id, &retry_snapshot)
        }
    };
    if let Err(validation_error) = initial_validation {
        if let Some(blocked_reason) =
            execution_retry_claim_failure_blocked_reason(validation_error.as_str())
            && let Err(invalidation_error) = persist_execution_retry_claim_failure_invalidation(
                &app,
                &state,
                &controller_runtime,
                session_id,
                &retry_reference,
                blocked_reason,
                EXECUTION_RETRY_SNAPSHOT_CLAIM_REJECTED_SUMMARY,
            )
            .await
        {
            eprintln!(
                "[webui-service] rejected execution retry invalidation failed for session {session_id}: {invalidation_error}"
            );
        }
        return Err(validation_error);
    }

    // retryResult may still observe a CAS race, so deterministic body failures re-check the exact original reference.
    // retryResult 仍可能观察到 CAS 竞争，因此确定性正文失败会重新核对精确原始引用。
    let retry_result = service_retry_frozen_execution(
        app.clone(),
        state.clone(),
        conversation_id,
        expected_generation,
        expected_ledger_revision,
        client_continuation_id,
        retry_reference.clone(),
        retry_snapshot,
        user_submission,
    )
    .await;
    if let Err(retry_error) = retry_result.as_ref()
        && let Some(blocked_reason) =
            execution_retry_claim_failure_blocked_reason(retry_error.as_str())
        && let Err(invalidation_error) = persist_execution_retry_claim_failure_invalidation(
            &app,
            &state,
            &controller_runtime,
            session_id,
            &retry_reference,
            blocked_reason,
            EXECUTION_RETRY_SNAPSHOT_CLAIM_REJECTED_SUMMARY,
        )
        .await
    {
        eprintln!(
            "[webui-service] raced execution retry invalidation failed for session {session_id}: {invalidation_error}"
        );
    }
    retry_result
}

/// Starts a claimed frozen execution with an optional durable user follow-up.
/// 使用可选的可靠用户补充消息启动一份已领取冻结执行。
///
/// # Parameters
/// 参数
/// `app` emits accepted recovery state, while `state` owns the authoritative workspace.
/// `app` 发出已接受的恢复状态，`state` 持有权威工作区。
/// `conversation_id`, the expected ledger pair, and `client_continuation_id` bind the exact recovery claim.
/// `conversation_id`、预期账本二元组与 `client_continuation_id` 绑定精确恢复领取。
/// `retry_reference` and `retry_snapshot` carry the digest-bound frozen execution.
/// `retry_reference` 与 `retry_snapshot` 携带摘要绑定的冻结执行。
/// `user_submission` optionally carries the user's supplemental text and current runtime selections.
/// `user_submission` 可选携带用户补充文本与当前运行时选择。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after service-owned recovery admission, or an exact validation error.
/// 服务接管恢复准入后返回轻量确认，或返回精确校验错误。
async fn service_retry_frozen_execution(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    expected_generation: u64,
    expected_ledger_revision: u64,
    client_continuation_id: String,
    retry_reference: vulcan_workbench_model::ExecutionRetrySnapshotRef,
    retry_snapshot: vulcan_workbench_model::ExecutionRetrySnapshot,
    user_submission: Option<ContinuationUserSubmission>,
) -> Result<SubmitConversationPromptAck, String> {
    // userFollowup separates visible transcript text from Provider-only host guidance.
    // userFollowup 将可见转录文本与仅 Provider 可见的宿主引导分离。
    let user_followup = user_submission
        .as_ref()
        .map(|submission| {
            execution_retry_user_followup_from_submission(
                submission,
                client_continuation_id.as_str(),
            )
        })
        .transpose()?;
    // executionMode preserves the user's current execution-mode selection for the recovered run.
    // executionMode 为恢复执行保留用户当前选择的执行模式。
    let execution_mode = user_submission
        .as_ref()
        .and_then(|submission| submission.execution_mode.clone());
    // reasoningEffort preserves the user's current reasoning selection for the recovered run.
    // reasoningEffort 为恢复执行保留用户当前选择的推理强度。
    let reasoning_effort = user_submission
        .as_ref()
        .and_then(|submission| submission.reasoning_effort.clone());
    // modelSelection validates the future exact model without mutating the frozen active route.
    // modelSelection 校验未来精确模型，同时不修改已冻结的当前活动路由。
    let model_selection = user_submission
        .as_ref()
        .and_then(|submission| submission.model_selection.clone());
    // attachments are carried only so the existing explicit unsupported-input guard can reject them.
    // attachments 仅用于让既有显式不支持输入护栏拒绝附件。
    let attachments = user_submission
        .as_ref()
        .and_then(|submission| submission.attachments.clone());
    // execution_task transfers the accepted Continue transaction to service ownership before the RPC can be cancelled.
    // execution_task 在 RPC 可能被取消前，把已接纳的 Continue 事务转移给服务所有权。
    let execution_task = spawn_service_task(async move {
        service_start_conversation_execution(
            app,
            state,
            client_continuation_id,
            None,
            Some(conversation_id),
            None,
            None,
            None,
            execution_mode,
            reasoning_effort,
            model_selection,
            None,
            attachments,
            None,
            ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
            ConversationSubmissionMode::Continuation {
                expected_generation,
                expected_ledger_revision,
                retry_reference: Box::new(retry_reference),
                retry_snapshot: Arc::new(retry_snapshot),
                user_followup,
            },
        )
        .await
    });
    execution_task.await.map_err(|join_error| {
        format!("ERR_CONVERSATION_CONTINUE_EXECUTION_TASK_FAILED: {join_error}")
    })?
}

/// Routes one composer submission through ordinary admission or exact problem-turn recovery.
/// 将一条输入区提交路由到普通准入或精确的问题回合恢复流程。
///
/// # Parameters
/// 参数
/// `app` emits accepted workbench changes and `state` owns authoritative runtime state.
/// `app` 发出已接受的工作台变更，`state` 持有权威运行态。
/// `client_submission_id`, project/conversation ids, and `prompt` identify the authored submission.
/// `client_submission_id`、项目/会话标识与 `prompt` 标识用户提交。
/// `hidden_model_guidance`, execution options, model selection, session settings, and attachments carry the exact composer snapshot.
/// `hidden_model_guidance`、执行选项、模型选择、会话设置与附件携带精确输入区快照。
/// `continuation_expectation` binds the submission to the selected problem turn when recovery is required.
/// `continuation_expectation` 在需要恢复时将提交绑定到选中的问题回合。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after ordinary or recovery admission, or an exact validation error.
/// 普通或恢复准入后返回轻量确认，或返回精确校验错误。
#[allow(clippy::too_many_arguments)]
pub(super) async fn service_submit_conversation_prompt_rpc(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: String,
    prompt: String,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    session_settings: Option<SubmissionSessionSettings>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    continuation_expectation: Option<ConversationContinuationExpectation>,
) -> Result<SubmitConversationPromptAck, String> {
    if let Some(expectation) = continuation_expectation {
        // conversationId is mandatory because a continuation can never create a new conversation.
        // conversationId 是必需的，因为 Continue 绝不能创建新会话。
        let conversation_id = conversation_id
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION"))?;
        if session_settings.is_some() {
            return Err(String::from(
                "ERR_CONVERSATION_CONTINUE_SESSION_SETTINGS_NOT_ALLOWED",
            ));
        }
        return service_continue_conversation_with_submission(
            app,
            state,
            conversation_id,
            expectation.expected_generation,
            expectation.expected_ledger_revision,
            client_submission_id,
            Some(ContinuationUserSubmission {
                prompt,
                hidden_model_guidance,
                execution_mode,
                reasoning_effort,
                model_selection,
                attachments,
            }),
        )
        .await;
    }
    service_submit_conversation_prompt(
        app,
        state,
        client_submission_id,
        project_id,
        conversation_id,
        Some(draft_slot_key),
        prompt,
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        session_settings,
        attachments,
        None,
        ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ConversationSubmissionMode::UserPrompt,
    )
    .await
}

/// Starts an ordinary user-authored conversation submission through the shared execution admission path.
/// 通过共享执行准入路径启动普通用户编写的会话提交。
#[allow(clippy::too_many_arguments)]
pub(super) async fn service_submit_conversation_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: Option<String>,
    prompt: String,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    session_settings: Option<SubmissionSessionSettings>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    preauthorized_turn_admission: Option<ExistingSubmissionAdmissionAuthorization>,
    existing_session_hydration_policy: ExistingSessionHydrationPolicy,
    submission_mode: ConversationSubmissionMode,
) -> Result<SubmitConversationPromptAck, String> {
    service_start_conversation_execution(
        app,
        state,
        client_submission_id,
        project_id,
        conversation_id,
        draft_slot_key,
        Some(prompt),
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        session_settings,
        attachments,
        preauthorized_turn_admission,
        existing_session_hydration_policy,
        submission_mode,
    )
    .await
}

/// Converts a post-Hook-start submission failure into one terminal, persisted cleanup result.
/// 将 Hook 启动后的提交失败转换为一个终态化且已持久化的清理结果。
///
/// # Parameters
/// # 参数
/// `app`, `state`, `controller_runtime`, and `agent_runtime` provide the exact projection, ownership, persistence, and Hook boundaries.
/// `app`、`state`、`controller_runtime` 与 `agent_runtime` 提供精确投影、所有权、持久化与 Hook 边界。
/// `session_id` and `preparation_turn_id` identify the started logical prompt turn, while `failure` is the original controlled diagnostic.
/// `session_id` 与 `preparation_turn_id` 标识已启动的逻辑提示词回合，`failure` 是原始受控诊断。
///
/// # Returns
/// # 返回值
/// The original failure after successful cleanup, or a combined diagnostic when cleanup also fails.
/// 清理成功时返回原始失败；清理也失败时返回组合诊断。
async fn cleanup_submission_after_prompt_turn_start(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &super::ControllerRuntimeHandle,
    agent_runtime: &super::AgentRuntimeBridge,
    session_id: SessionId,
    preparation_turn_id: &str,
    failure: String,
) -> String {
    // cleanup_error is retained only when terminal projection or persistence could not finish.
    // cleanup_error 仅在终态投影或持久化无法完成时保留。
    let cleanup_error = mark_webui_prompt_preparation_failed(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        preparation_turn_id,
        &failure,
    )
    .await
    .err();
    cleanup_error.map_or(failure.clone(), |cleanup_error| {
        format!("{failure}; post-Hook-start cleanup failed: {cleanup_error}")
    })
}

/// Submits one ordinary user prompt or one revision-bound recovery through the shared conversation pipeline.
/// 通过共享会话流水线提交普通用户提示词或绑定修订号的恢复请求。
///
/// # Parameters
/// `app` emits accepted workbench updates and runtime events.
/// `app` 发出已接受的工作台更新与运行时事件。
/// `state` owns service handles and the retained workspace snapshot.
/// `state` 持有服务句柄与保留的工作区快照。
/// The remaining arguments preserve the existing prompt, session, model, attachment, admission, hydration, and submission-mode contracts.
/// 其余参数继续承载现有的提示词、会话、模型、附件、准入、恢复策略与提交模式契约。
///
/// # Returns
/// A lightweight acknowledgement after the request is queued or started.
/// 请求排队或启动后的轻量确认信息。
///
/// `existing_session_hydration_policy` controls whether an empty retained session is rehydrated.
/// `existing_session_hydration_policy` 控制是否重新水合空的保留会话。
///
/// # Returns
/// 返回值
/// Returns a lightweight acknowledgement after queuing or starting the submitted prompt.
/// 返回排队或启动提交提示词后的轻量确认信息。
///
/// `submission_mode` selects ordinary user input or a revision-bound recovery with an optional durable follow-up.
/// `submission_mode` 选择普通用户输入，或带可选持久补充消息且绑定修订号的恢复请求。
async fn service_start_conversation_execution(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: Option<String>,
    prompt: Option<String>,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    session_settings: Option<SubmissionSessionSettings>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    preauthorized_turn_admission: Option<ExistingSubmissionAdmissionAuthorization>,
    existing_session_hydration_policy: ExistingSessionHydrationPolicy,
    submission_mode: ConversationSubmissionMode,
) -> Result<SubmitConversationPromptAck, String> {
    // Measures the whole submit RPC body from service entry to final response.
    // 衡量提交 RPC 主体从服务入口到最终返回的整体耗时。
    let submit_total_started_at = current_unix_millis_value();
    let client_submission_id = client_submission_id.trim().to_string();
    if client_submission_id.is_empty() {
        return Err(String::from("clientSubmissionId must not be empty."));
    }
    // attachmentDraftScope is validated once and remains an internal-only owner identity through queueing or acceptance.
    // attachmentDraftScope 只校验一次，并在排队或接纳全程保持为仅内部使用的所有者身份。
    let attachment_draft_scope = draft_slot_key
        .as_deref()
        .map(composer_attachment_draft_scope)
        .transpose()?;
    let prompt = match (&submission_mode, prompt) {
        (ConversationSubmissionMode::UserPrompt, Some(prompt)) => prompt.trim().to_string(),
        (ConversationSubmissionMode::UserPrompt, None) => {
            return Err(String::from("A user submission requires prompt text."));
        }
        (ConversationSubmissionMode::Continuation { .. }, None) => String::new(),
        (ConversationSubmissionMode::Continuation { .. }, Some(_)) => {
            return Err(String::from(
                "An execution retry must not carry prompt text.",
            ));
        }
    };
    // claimedRetrySnapshot carries the exact sidecar through admission into deferred runtime preparation.
    // claimedRetrySnapshot 将精确 sidecar 从准入阶段传递到延迟运行时准备阶段。
    let mut claimed_retry_snapshot = submission_mode.retry_snapshot().cloned();
    // frozenRetrySnapshot preserves the immutable pre-claim body for the final locked CAS validation.
    // frozenRetrySnapshot 保留不可变的领取前正文，用于最终锁内 CAS 校验。
    let frozen_retry_snapshot = claimed_retry_snapshot.clone();
    // claimedRetryReference keeps the unlocked body read bound to the exact lightweight pointer through final CAS admission.
    // claimedRetryReference 将无锁正文读取持续绑定到最终 CAS 准入使用的精确轻量指针。
    let claimed_retry_reference = submission_mode.retry_reference().cloned();
    let prompt_source = submission_mode.prompt_source();
    let append_user_records = submission_mode.appends_user_records();
    webui_submit_trace(
        "submit_prompt_received",
        format!(
            "conversation_id={} project_id={} prompt_chars={} attachments={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            project_id.as_deref().unwrap_or("<none>"),
            prompt.chars().count(),
            attachments.as_ref().map(Vec::len).unwrap_or(0)
        ),
    );
    let attachment_summaries = attachments.unwrap_or_default();
    if prompt.is_empty() && attachment_summaries.is_empty() && !submission_mode.is_continuation() {
        return Err(String::from("请输入任务指令。"));
    }
    if submission_mode.is_continuation() && !attachment_summaries.is_empty() {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_ATTACHMENTS_NOT_ALLOWED",
        ));
    }

    // Measures runtime handle acquisition before workspace hydration begins.
    // 衡量工作区水合开始前获取运行时句柄的耗时。
    let runtime_parts_started_at = current_unix_millis_value();
    let (controller_runtime, agent_runtime, _luaskills_catalog, _mcp_catalog, _logs) =
        runtime_parts_from_state(&state)?;
    webui_submit_trace(
        "submit_prompt_runtime_parts_ready",
        format!(
            "conversation_id={} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(runtime_parts_started_at)
        ),
    );
    // submission_target fixes create-or-continue intent before admission, loading, or hydration starts.
    // submission_target 在准入、加载或水合开始前固定新建或继续意图。
    let submission_target = parse_prompt_session_target(conversation_id.as_deref())?;
    // submission_session_id is the existing identity used by retained-state and hydration lookups.
    // submission_session_id 是保留状态与水合查找使用的已有会话身份。
    let submission_session_id = submission_target.existing_session_id();
    if let Some(draft_slot_key) = draft_slot_key.as_deref() {
        let expected_draft_slot_key = match submission_session_id {
            Some(session_id) => format!("conversation:{session_id}"),
            None => format!(
                "new:{}",
                project_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("workspace")
            ),
        };
        if draft_slot_key.trim() != expected_draft_slot_key {
            return Err(String::from(
                "Composer attachment draft slot does not own the selected conversation target.",
            ));
        }
    }
    // Measures retained workspace reuse without cloning the full workbench snapshot on the hot path.
    // 衡量保留工作区复用，并避免在热路径克隆完整工作台快照。
    let retained_snapshot_started_at = current_unix_millis_value();
    // RetainedSubmissionPreflight keeps admission ownership aligned with the captured workspace revision.
    // RetainedSubmissionPreflight 让准入所有权与捕获的工作区版本保持一致。
    let RetainedSubmissionPreflight {
        turn_admission,
        project_admission,
        preserved_workspace_run_state,
        retained_dialogue_context_available,
        retained_workspace_snapshot,
    } = prepare_retained_submission_preflight(
        &state,
        project_id.as_deref(),
        submission_target,
        preauthorized_turn_admission,
    )?;
    webui_trace_if_slow(
        "submit_prompt_retained_snapshot_slow",
        retained_snapshot_started_at,
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} preserved_sessions={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            preserved_workspace_run_state
                .as_ref()
                .map(|state| state.session_count())
                .unwrap_or_default()
        ),
    );
    let workspace_load_started_at = current_unix_millis_value();
    let (mut workspace_snapshot, workspace_source) =
        if let Some(workspace_snapshot) = retained_workspace_snapshot {
            (
                workspace_snapshot,
                if submission_session_id.is_some() {
                    "retained-existing"
                } else {
                    "retained-new-slice"
                },
            )
        } else if submission_session_id.is_none() {
            return Err(String::from(
                "工作区仍在初始化，新会话不会回退加载历史工作区；请稍后重试。",
            ));
        } else {
            (
                load_workspace_for_submission(controller_runtime.clone(), submission_session_id)
                    .await?,
                "disk",
            )
        };
    if existing_submission_requires_hydration(
        submission_session_id,
        existing_session_hydration_policy,
    ) && let Some(session_id) = submission_session_id
    {
        hydrate_retained_submission_session_if_needed(
            controller_runtime.clone(),
            &mut workspace_snapshot,
            session_id,
        )
        .await?;
    }
    webui_submit_trace(
        "submit_prompt_workspace_ready",
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} source={workspace_source} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(workspace_load_started_at)
        ),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    // attachmentWorkspacePath binds every browser-provided asset id to the loaded authoritative project before any path can be read or persisted.
    // attachmentWorkspacePath 在读取或持久化任何路径前，把每个浏览器提供的资产标识绑定到已加载的权威项目。
    let attachment_workspace_path = composer_attachment_workspace_path_from_snapshot(
        &workspace_snapshot,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let authoritative_attachment_summaries = attachment_summaries
        .into_iter()
        .map(|summary| {
            resolve_composer_attachment_summary_for_workspace(&attachment_workspace_path, summary)
        })
        .collect::<Result<Vec<_>, _>>()?;
    // canonicalAttachments never retain request-local delivery decisions.
    // canonicalAttachments 永远不保留单次请求局部传输决策。
    let attachments = authoritative_attachment_summaries
        .iter()
        .cloned()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let attachments_for_draft_promotion = attachments.clone();
    // requestedProviderAttachments carries only a conservative metadata-only reduction; every
    // byte-exposing mode is recomputed from the exact route at the actual dispatch boundary.
    // requestedProviderAttachments 仅携带保守的仅元数据降级；每种暴露字节的模式都会在实际分发边界
    // 根据精确路线重新计算。
    let requested_provider_attachments = authoritative_attachment_summaries
        .into_iter()
        .map(composer_attachment_summary_to_provider_core)
        .collect::<Result<Vec<_>, _>>()?;
    if let ExistingSubmissionQueueDecision::Queue { session_id } =
        evaluate_existing_submission_queue(&state, &workspace_snapshot, submission_session_id)?
    {
        if submission_mode.is_continuation() {
            return Err(String::from("ERR_CONVERSATION_CONTINUE_SESSION_BUSY"));
        }
        return queue_existing_submission(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            ExistingSubmissionQueueInput {
                client_submission_id,
                workspace_snapshot,
                session_id,
                prompt,
                hidden_model_guidance,
                attachments: requested_provider_attachments,
                attachment_draft_scope,
                reasoning_effort,
                model_selection,
                submit_total_started_at,
            },
        );
    }
    // Media preparation starts only after queue admission so a queued prompt uploads at its real dispatch boundary.
    // 媒体准备仅在队列准入判断后开始，确保排队提示词在实际分发边界上传媒体。
    // submissionMediaRuntimeState carries the exact persisted lease identity from the loaded
    // workspace snapshot without re-reading mutable service state during upload.
    // submissionMediaRuntimeState 从已加载工作区快照携带精确持久化租约身份，上传期间不再读取
    // 可变服务状态。
    let submission_media_runtime_state = submission_session_id
        .and_then(|session_id| workspace_snapshot.session_runtime_state.get(&session_id));
    let prepared_submission_projection = prepare_submission_media(
        &app,
        &state,
        &agent_runtime,
        client_submission_id.as_str(),
        submission_session_id,
        submission_media_runtime_state,
        model_selection.as_ref(),
        requested_provider_attachments.as_slice(),
    )
    .await?;
    let provider_attachments = prepared_submission_projection.provider_attachments;
    let prepared_submission_media = prepared_submission_projection.prepared_media;
    let materialize_started_at = current_unix_millis_value();
    // materialization_project_id pins a new session to the exact project selected under preflight ownership.
    // materialization_project_id 将新会话固定到预检所有权下选择的精确项目。
    let materialization_project_id = if submission_session_id.is_none() {
        project_admission
            .exact_project_id()
            .or(project_id.as_deref())
    } else {
        project_id.as_deref()
    };
    let materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        materialization_project_id,
        submission_target,
        if prompt.is_empty() {
            "附件会话"
        } else {
            &prompt
        },
    )?;
    project_admission.validate_materialized_session(&materialized_session)?;
    webui_submit_trace(
        "submit_prompt_materialize_ready",
        format!(
            "session={} title_chars={} workspace_path_chars={} elapsed_ms={}",
            materialized_session.session_id,
            materialized_session.session_title.chars().count(),
            materialized_session.workspace_path.chars().count(),
            current_unix_millis_value().saturating_sub(materialize_started_at)
        ),
    );
    // kernel_reservation_id binds an existing materialization to the exact admission owner captured before loading.
    // kernel_reservation_id 将已有会话物化结果绑定到加载前捕获的精确准入所有者。
    let kernel_reservation_id = turn_admission.kernel_reservation_id(&materialized_session)?;
    prepare_materialized_submission_session(&mut workspace_snapshot, &materialized_session)?;
    if submission_mode.is_continuation() {
        validate_recovery_submission_model_selection(
            agent_runtime.public_ai_runtime.as_ref(),
            &workspace_snapshot,
            materialized_session.session_id,
            model_selection.as_ref(),
        )?;
    } else {
        apply_submission_model_selection(
            agent_runtime.public_ai_runtime.as_ref(),
            &mut workspace_snapshot,
            materialized_session.session_id,
            model_selection.as_ref(),
        )?;
    }
    if session_settings.is_some()
        && materialized_session.kind != PromptSessionMaterializationKind::Created
    {
        return Err(String::from("草稿会话设置只能随新会话的首次提交应用。"));
    }
    apply_submission_session_settings(
        &state,
        &controller_runtime,
        agent_runtime.public_ai_runtime.as_ref(),
        &mut workspace_snapshot,
        materialized_session.session_id,
        execution_mode.as_deref(),
        session_settings.as_ref(),
        materialized_session.kind == PromptSessionMaterializationKind::Created,
    )
    .await?;
    // session_id is copied from the authoritative materialization result after activation consumes its classification.
    // session_id 在激活阶段消费物化分类后，从权威物化结果中复制。
    let session_id = materialized_session.session_id;
    if matches!(submission_mode, ConversationSubmissionMode::UserPrompt)
        && let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
    {
        let _ = runtime_state
            .provider_call_ledger
            .invalidate_continuation_for_submission(
                client_submission_id.clone(),
                i64::try_from(current_unix_millis_value()).unwrap_or(i64::MAX),
            );
    }
    // session_title borrows the authoritative title through the final activation commit.
    // session_title 借用权威标题直到最终激活提交完成。
    let session_title = materialized_session.session_title.as_str();
    // workspace_path borrows the owning project path through precheck dispatch preparation.
    // workspace_path 借用所属项目路径直到 precheck 分发准备完成。
    let workspace_path = materialized_session.workspace_path.as_str();
    webui_submit_trace(
        "submit_prompt_starting_run",
        format!(
            "session={session_id} title_chars={}",
            session_title.chars().count()
        ),
    );
    let submit_run_prepare_started_at = current_unix_millis_value();
    let runtime_options = prepare_submission_runtime_options(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        SubmissionRuntimeOptionsInput {
            execution_mode,
            reasoning_effort,
        },
    )
    .await?;
    webui_submit_trace(
        "submit_prompt_runtime_options_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(submit_run_prepare_started_at)
        ),
    );
    if let Some((expected_generation, expected_ledger_revision)) =
        submission_mode.continuation_expectation()
    {
        // blockers are captured before restoring the retry-owned precheck so Continue cannot
        // classify its own active_precheck_turn_id as pre-existing queued work.
        // blockers 在恢复 retry 自有 precheck 前捕获，避免 Continue 把自己写入的
        // active_precheck_turn_id 误判成已有排队工作。
        // claimInput refreshes the detached candidate from one final authoritative retained-state revision.
        // claimInput 从最终一次权威保留态修订刷新分离候选。
        let (blockers, run_phase, authoritative_runtime_state) = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            let retained_workspace = guard
                .workspace_snapshot
                .as_ref()
                .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
            // The Continue submission itself owns one PromptSubmission reservation. Only that
            // exact id-qualified row may be excluded from the external admission blocker set.
            // Continue 提交自身会持有一条 PromptSubmission 预留。只有该条带精确标识的
            // 所有者行可以从外部准入阻断集合中排除。
            let blockers = continuation_submission_runtime_blockers(
                &guard,
                retained_workspace,
                session_id,
                &turn_admission,
            )?;
            let run_phase = retained_workspace
                .session_run_phases
                .get(&session_id)
                .copied()
                .unwrap_or(SessionRunPhase::Idle);
            let authoritative_runtime_state = retained_workspace
                .session_runtime_state
                .get(&session_id)
                .cloned()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
            (blockers, run_phase, authoritative_runtime_state)
        };
        // The continuation contract carries no model, session-setting, or reasoning mutation, so
        // replacing only its detached runtime row with the final retained row loses no request input.
        // Continue 契约不携带模型、会话设置或推理参数变更，因此仅用最终保留行替换其
        // 分离运行态不会丢失任何请求输入。
        workspace_snapshot
            .session_run_phases
            .insert(session_id, run_phase);
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, authoritative_runtime_state);
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        let retry_snapshot = claimed_retry_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
        let retry_reference = claimed_retry_reference
            .as_ref()
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
        claim_continuation_before_precheck_restore(
            agent_runtime.public_ai_runtime.as_ref(),
            runtime_state,
            session_id,
            run_phase,
            blockers,
            expected_generation,
            expected_ledger_revision,
            client_submission_id.as_str(),
            retry_reference,
            Arc::make_mut(retry_snapshot),
            submission_mode.user_followup(),
        )?;
    }
    let turn_begin_started_at = current_unix_millis_value();
    let resume_preparation_input =
        claimed_retry_snapshot
            .as_ref()
            .and_then(|snapshot| match &snapshot.pending_action {
                vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
                    input,
                } => Some(input.as_ref().clone()),
                _ => None,
            });
    let mut precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        if let Some(input) = resume_preparation_input {
            let restored_turn_id = input.preparation_turn_id.clone();
            // runtimeState is updated on the detached workspace before Hook restoration, so restoration has no fallible local mutation after it acquires ownership.
            // runtimeState 在 Hook 恢复前更新分离工作区，使恢复取得所有权后不再存在可失败的本地变更。
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
            runtime_state.active_precheck_turn_id = Some(restored_turn_id);
            restore_shared_prompt_turn_from_execution_retry(session_id, &mut hook_runtime, input)?
        } else {
            begin_shared_prompt_turn(
                &mut workspace_snapshot,
                &mut hook_runtime,
                SharedPromptTurnBeginInput {
                    session_id,
                    session_title,
                    workspace_path,
                    prompt_entry_id: format!(
                        "webui-prompt-{session_id}-{}",
                        current_unix_millis_text()
                    ),
                    prompt: &prompt,
                    hidden_model_guidance: hidden_model_guidance.as_deref(),
                    provider_visible_agent_messages: Vec::new(),
                    attachments,
                    provider_attachments: Some(provider_attachments),
                    append_canonical_user_message: append_user_records,
                    append_visible_user_prompt_record: append_user_records,
                    source: prompt_source,
                    execution_mode: runtime_options.execution_mode,
                    reasoning_effort: runtime_options.reasoning_effort,
                },
            )?
        }
    };
    if let Err(error) = commit_prepared_submission_media_ledger(
        &mut workspace_snapshot,
        session_id,
        precheck_start.canonical_user_message_id.as_deref(),
        prepared_submission_media,
    ) {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    let accepted_message_id = precheck_start.visible_user_prompt_record_id.clone();
    let submission_target_label = if submission_session_id.is_some() {
        "existing"
    } else {
        "new"
    };
    if let Err(error) = attach_client_submission_id_to_visible_user_record(
        &mut workspace_snapshot,
        session_id,
        accepted_message_id.as_deref(),
        &client_submission_id,
        submission_target_label,
        current_unix_millis_value(),
    ) {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    // visible_user_prompt_message is the single bounded record needed after ownership transfer commits the detached slice.
    // visible_user_prompt_message 是所有权转移提交分离切片后仍需使用的唯一有界记录。
    let visible_user_prompt_message = precheck_start
        .visible_user_prompt_record_id
        .as_deref()
        .and_then(|message_id| {
            workspace_snapshot
                .session_messages
                .get(&session_id)
                .and_then(|messages| messages.iter().find(|message| message.id == message_id))
        })
        .cloned();
    if submission_mode.continuation_expectation().is_some() {
        // sessionDirectory is fixed before claim publication for exact consumed-file cleanup.
        // sessionDirectory 在领取发布前固定，用于精确清理已消费文件。
        let session_directory = segmented_session_directory_for_workspace_session(
            &controller_runtime.data_root,
            &workspace_snapshot,
            session_id,
        );
        // checkpointCommitResult groups every fallible in-memory claim step behind one cleanup boundary.
        // checkpointCommitResult 把所有可失败的内存领取步骤归入同一清理边界。
        let checkpoint_commit_result = (|| -> Result<(), String> {
            session_directory.as_ref().ok_or_else(|| {
                format!("Session `{session_id}` has no exact retry storage locator.")
            })?;
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
            let retry_snapshot = claimed_retry_snapshot
                .as_mut()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
            let retry_reference =
                vulcan_workspace_store::workspace_store::project_execution_retry_snapshot(
                    session_id,
                    retry_snapshot.as_ref(),
                )?;
            runtime_state.transient_execution_retry_snapshot = Some(Arc::clone(retry_snapshot));
            runtime_state.execution_retry_snapshot = Some(retry_reference);
            // runtime updatedAt crosses the same in-memory claim boundary as the new reference.
            // 运行态 updatedAt 与新引用跨越同一个内存领取边界。
            runtime_state.updated_at = retry_snapshot.updated_at.clone();
            precheck_start.execution_retry_snapshot = claimed_retry_snapshot.clone();
            let (expected_generation, expected_ledger_revision) = submission_mode
                .continuation_expectation()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_STALE"))?;
            let expected_retry_reference = claimed_retry_reference
                .as_ref()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
            let frozen_retry_snapshot = frozen_retry_snapshot
                .as_ref()
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
            commit_continuation_claim_checkpoint(
                &state,
                &mut workspace_snapshot,
                session_id,
                &turn_admission,
                expected_generation,
                expected_ledger_revision,
                client_submission_id.as_str(),
                expected_retry_reference,
                frozen_retry_snapshot,
            )?;
            Ok(())
        })();
        if let Err(error) = checkpoint_commit_result {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                error,
            )
            .await);
        }
        if let Err(error) = persist_latest_retained_workspace_session_update(
            &state,
            controller_runtime.clone(),
            session_id,
        )
        .await
        {
            let cleanup_error = mark_webui_prompt_preparation_failed(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                &error,
            )
            .await
            .err();
            return Err(match cleanup_error {
                Some(cleanup_error) => format!("{error}; cleanup failed: {cleanup_error}"),
                None => error,
            });
        }
        let consumed_reference = claimed_retry_reference
            .as_ref()
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
        let session_directory = session_directory
            .as_ref()
            .ok_or_else(|| format!("Session `{session_id}` has no exact retry storage locator."))?;
        if let Err(error) = vulcan_workspace_store::workspace_store::delete_execution_retry_snapshot(
            session_directory,
            session_id,
            consumed_reference,
        ) {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                error,
            )
            .await);
        }
    }
    let turn_id = Some(precheck_start.preparation_turn_id.clone());
    let preparation_prompt_source = precheck_start.source;
    webui_submit_trace(
        "submit_prompt_turn_begin_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(turn_begin_started_at)
        ),
    );
    // Every accepted turn starts in Preparing; post-ACK precheck moves it to MemoryRecall only when required.
    // 每个已接纳回合都从 Preparing 开始；仅在需要时由 ACK 后预检推进到 MemoryRecall。
    let submit_visible_run_phase = SessionRunPhase::Preparing;
    let kernel_start_result = match state.lock() {
        Ok(mut guard) => {
            let kernel_turn_started_at = current_unix_millis_value();
            let result = if let Some(kernel_reservation_id) = kernel_reservation_id {
                start_reserved_webui_session_kernel_user_turn(
                    &mut guard,
                    session_id,
                    &precheck_start.preparation_turn_id,
                    kernel_reservation_id,
                )
            } else {
                start_webui_session_kernel_user_turn(
                    &mut guard,
                    session_id,
                    &precheck_start.preparation_turn_id,
                )
            };
            webui_submit_trace(
                "submit_prompt_kernel_turn_started",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(kernel_turn_started_at)
                ),
            );
            result
        }
        Err(error) => Err(format!("failed to lock service state: {error}")),
    };
    if let Err(error) = kernel_start_result {
        let cleanup_error = mark_webui_prompt_preparation_failed(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            &error,
        )
        .await
        .err();
        return Err(match cleanup_error {
            Some(cleanup_error) => format!("{error}; cleanup failed: {cleanup_error}"),
            None => error,
        });
    }
    mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);
    let begin_commit_started_at = current_unix_millis_value();
    if materialized_session.kind == PromptSessionMaterializationKind::Created {
        // New-session persistence establishes the segmented directory before optional retry sidecar publication.
        // 新会话持久化会在可选重试 sidecar 发布前建立分段目录。
        if let Err(error) = persist_workspace_session_update(
            None,
            controller_runtime.clone(),
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                error,
            )
            .await);
        }
    }
    // deferredConsumedRetryCleanup survives detached-state preparation but cannot run before the
    // replacement retained state is durably persisted.
    // deferredConsumedRetryCleanup 跨越分离状态准备阶段保留，但绝不能在替代保留状态可靠落盘前执行。
    let mut deferred_consumed_retry_cleanup = None;
    if !submission_mode.is_continuation() {
        match deferred_consumed_execution_retry_cleanup(
            &controller_runtime,
            &workspace_snapshot,
            session_id,
        ) {
            Ok(cleanup) => deferred_consumed_retry_cleanup = cleanup,
            Err(error) => eprintln!(
                "[webui-service] consumed execution retry cleanup could not be scheduled: {error}"
            ),
        }
        if let Err(error) = retain_accepted_preparation_retry_checkpoint(
            &mut workspace_snapshot,
            session_id,
            &client_submission_id,
            &precheck_start,
            deferred_consumed_retry_cleanup
                .as_ref()
                .map(|cleanup| cleanup.reference.generation),
        ) {
            // Retry-point retention is auxiliary for ordinary submissions and must never reject user input.
            // 重试点保留是常规提交的辅助能力，绝不能因此拒绝用户输入。
            eprintln!("[webui-service] accepted preparation retry checkpoint skipped: {error}");
            if let Some(runtime_state) = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
            {
                // A failed transient projection clears both halves so no body survives without its exact reference.
                // 临时投影失败时同时清除两部分，避免正文脱离精确引用继续存活。
                let _ = runtime_state.execution_retry_snapshot.take();
                runtime_state.transient_execution_retry_snapshot = None;
                runtime_state.updated_at = current_unix_millis_text();
            }
        }
    }
    let workspace_commit_result =
        if materialized_session.kind == PromptSessionMaterializationKind::Created {
            commit_created_submission_workspace_slice(
                &state,
                &workspace_snapshot,
                &materialized_session,
            )
        } else if let Some((expected_generation, expected_ledger_revision)) =
            submission_mode.continuation_expectation()
        {
            commit_continuation_preparing_checkpoint(
                &state,
                &mut workspace_snapshot,
                session_id,
                &turn_admission,
                expected_generation,
                expected_ledger_revision,
                client_submission_id.as_str(),
                &precheck_start.preparation_turn_id,
            )
        } else {
            commit_existing_submission_workspace_slice(
                &state,
                &mut workspace_snapshot,
                &materialized_session,
            )
        };
    if let Err(error) = workspace_commit_result {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    webui_submit_trace(
        "submit_prompt_begin_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(begin_commit_started_at),
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    if let Err(error) = commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    ) {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    if materialized_session.kind == PromptSessionMaterializationKind::Existing
        && let Some(visible_user_prompt_message) = visible_user_prompt_message.as_ref()
    {
        // transcriptAdmissionResult reserves the existing-session user row before any deferred runtime event can overtake it.
        // transcriptAdmissionResult 在任何延迟运行时事件抢占前，为已有会话用户记录预留持久化顺序。
        let transcript_admission_result = spawn_exact_session_transcript_entries_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            vec![visible_user_prompt_message.clone()],
            Some(String::from("accepted_canonical_user_message")),
        );
        // transcriptDurabilityResult joins synchronous admission and physical append completion into one pre-ACK boundary.
        // transcriptDurabilityResult 将同步准入与物理追加完成合并为一个 ACK 前边界。
        let transcript_durability_result = match transcript_admission_result {
            Ok(()) => await_session_transcript_writer_durable(session_id).await,
            Err(error) => Err(error),
        };
        if let Err(error) = transcript_durability_result {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                format!("Existing user prompt transcript durability failed: {error}"),
            )
            .await);
        }
    }
    if let Some(message) = visible_user_prompt_message.as_ref() {
        let message_id = message.id.as_str();
        // delta_started_at measures only the post-commit visible projection and event emission.
        // delta_started_at 仅衡量提交后的可见投影与事件发送。
        let delta_started_at = current_unix_millis_value();
        if let Err(error) = emit_materialized_session_message_delta(
            &app,
            &state,
            session_id,
            message,
            submit_visible_run_phase,
        ) {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                error,
            )
            .await);
        }
        webui_submit_trace(
            "submit_prompt_user_message_delta_ready",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms={} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(delta_started_at),
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
        tokio::task::yield_now().await;
        webui_submit_trace(
            "submit_prompt_user_message_delta_yielded",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
    }
    webui_submit_trace(
        "submit_prompt_visible_user_message_emitted",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    // A continuation claim is a synchronous durable admission boundary; ordinary user submissions retain the bounded asynchronous ACK path.
    // Continue 领取是同步持久化准入边界；普通用户提交继续使用有界异步 ACK 路径。
    if submission_mode.is_continuation() {
        if let Err(persist_error) = persist_latest_retained_workspace_session_update(
            &state,
            controller_runtime.clone(),
            session_id,
        )
        .await
        {
            let cleanup_error = mark_webui_prompt_preparation_failed(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                &persist_error,
            )
            .await
            .err();
            return Err(cleanup_error.map_or_else(
                || format!("Continue 领取持久化失败：{persist_error}"),
                |cleanup_error| {
                    format!(
                        "Continue 领取持久化失败：{persist_error}；失败清理也未完成：{cleanup_error}"
                    )
                },
            ));
        }
    } else if let Some(consumed_retry_cleanup) = deferred_consumed_retry_cleanup {
        // cleanupState and cleanupRuntime own a background durability transaction; the old file
        // remains a valid crash boundary until that transaction succeeds.
        // cleanupState 与 cleanupRuntime 持有后台可靠性事务；该事务成功前旧文件仍是有效崩溃边界。
        let cleanup_state = state.clone();
        let cleanup_runtime = controller_runtime.clone();
        spawn_service_task(async move {
            match persist_latest_retained_workspace_session_update(
                &cleanup_state,
                cleanup_runtime,
                session_id,
            )
            .await
            {
                Ok(()) => {
                    // Only a successful exact state persist may retire the consumed durable body.
                    // 只有精确状态持久化成功后才允许退役已消费的可靠正文。
                    if let Err(error) =
                        vulcan_workspace_store::workspace_store::delete_execution_retry_snapshot(
                            &consumed_retry_cleanup.session_directory,
                            session_id,
                            &consumed_retry_cleanup.reference,
                        )
                    {
                        eprintln!(
                            "[webui-service] consumed execution retry cleanup failed after state persistence: {error}"
                        );
                    }
                }
                Err(error) => eprintln!(
                    "[webui-service] background persist failed (submit_acceptance): {error}"
                ),
            }
        });
    } else {
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            Some(String::from("submit_acceptance")),
        );
    }
    let revision = match current_workbench_revision_from_state(&state) {
        Ok(revision) => revision,
        Err(error) => {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                error,
            )
            .await);
        }
    };
    let acknowledgement_elapsed_ms =
        current_unix_millis_value().saturating_sub(submit_total_started_at);
    if let Err(error) = retain_slow_submission_diagnostic(
        &state,
        &client_submission_id,
        submission_target_label,
        "acceptance",
        acknowledgement_elapsed_ms,
    ) {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    webui_submit_trace(
        "submit_prompt_returning",
        format!(
            "session={session_id} client_submission_id={client_submission_id} elapsed_ms_since_received={acknowledgement_elapsed_ms}"
        ),
    );
    let accepted_client_submission_id = client_submission_id.clone();
    if let Err(error) = promote_submission_attachment_draft_scope(
        workspace_path,
        session_id,
        precheck_start.canonical_user_message_id.as_deref(),
        attachments_for_draft_promotion.as_slice(),
        attachment_draft_scope.as_deref(),
    ) {
        // The accepted canonical message remains authoritative; retaining the draft lease is safer than duplicating the user turn.
        // 已接纳的规范消息保持权威；保留草稿租约比重复用户回合更安全。
        webui_submit_trace(
            "composer_attachment_draft_lease_promotion_failed",
            format!("session={session_id} error={error}"),
        );
    }
    let acknowledgement = SubmitConversationPromptAck {
        client_submission_id,
        conversation_id: session_id.to_string(),
        accepted_message_id,
        turn_id,
        run_phase: submit_visible_run_phase.to_string(),
        queued: false,
        revision,
    };
    // Deferred preparation begins only after the visible message and acknowledgement facts are committed.
    // 延迟准备仅在可见消息与确认事实提交后开始。
    let preparation_turn_id = precheck_start.preparation_turn_id.clone();
    let session_title = session_title.to_string();
    let workspace_path = workspace_path.to_string();
    let app_for_preparation = app.clone();
    let state_for_preparation = state.clone();
    let controller_runtime_for_preparation = controller_runtime.clone();
    let agent_runtime_for_preparation = agent_runtime.clone();
    spawn_service_task(async move {
        let preparation_started_at = current_unix_millis_value();
        let preparation_result = (|| -> Result<(), String> {
            // capturedPreparationOwner clones the workspace only after acknowledgement and binds every later write to the accepted kernel generation.
            // capturedPreparationOwner 仅在确认后克隆工作区，并把所有后续写入绑定到已接纳的 kernel 代际。
            let Some((mut workspace_snapshot, expected_preparation_generation)) =
                capture_webui_prompt_preparation_workspace(
                    &state_for_preparation,
                    session_id,
                    &preparation_turn_id,
                )?
            else {
                webui_submit_trace(
                    "submit_prompt_preparation_owner_closed",
                    format!("session={session_id} turn_id={preparation_turn_id}"),
                );
                return Ok(());
            };
            let context_snapshot_started_at = current_unix_millis_value();
            snapshot_webui_context_before_user_prompt(
                &controller_runtime_for_preparation,
                &workspace_snapshot,
                session_id,
                preparation_prompt_source,
            )?;
            webui_submit_trace(
                "submit_prompt_context_snapshot_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(context_snapshot_started_at)
                ),
            );
            let precheck_plan_started_at = current_unix_millis_value();
            let precheck_plan = if preparation_prompt_source.runs_user_prompt_precheck() {
                webui_shared_prompt_precheck_plan(
                    &agent_runtime_for_preparation,
                    &workspace_snapshot,
                    session_id,
                    &workspace_path,
                )?
            } else {
                SharedPromptPrecheckPlan {
                    has_precheck_hooks: false,
                    should_skip_precheck: true,
                    progress_labels: Vec::new(),
                }
            };
            webui_submit_trace(
                "submit_prompt_precheck_plan_ready",
                format!(
                    "session={session_id} has_hooks={} skip={} labels={} elapsed_ms={}",
                    precheck_plan.has_precheck_hooks,
                    precheck_plan.should_skip_precheck,
                    precheck_plan.progress_labels.len(),
                    current_unix_millis_value().saturating_sub(precheck_plan_started_at)
                ),
            );
            let outer_steps_started_at = current_unix_millis_value();
            apply_webui_prompt_begin_outer_steps_deferred_context_persist(
                &controller_runtime_for_preparation,
                &mut workspace_snapshot,
                session_id,
                &session_title,
            );
            webui_submit_trace(
                "submit_prompt_outer_steps_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(outer_steps_started_at)
                ),
            );
            let hook_snapshot_started_at = current_unix_millis_value();
            refresh_exact_vmm_hook_session_projection(
                &agent_runtime_for_preparation.vmm_hook_runtime,
                &workspace_snapshot,
                session_id,
            )?;
            webui_submit_trace(
                "submit_prompt_hook_snapshot_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
                ),
            );
            // currentPreparationSnapshot revalidates the immutable kernel owner and reads the latest retained UI domain without publishing the detached read-only draft.
            // currentPreparationSnapshot 重新校验不可变 kernel 所有者，并读取最新保留 UI 域，不发布分离的只读草稿。
            let Some(current_preparation_generation) = capture_webui_prompt_preparation_generation(
                &state_for_preparation,
                session_id,
                &preparation_turn_id,
            )?
            else {
                webui_submit_trace(
                    "submit_prompt_preparation_commit_skipped_stale_owner",
                    format!(
                        "session={session_id} generation={expected_preparation_generation} turn_id={preparation_turn_id}"
                    ),
                );
                return Ok(());
            };
            if current_preparation_generation != expected_preparation_generation {
                webui_submit_trace(
                    "submit_prompt_preparation_generation_replaced",
                    format!(
                        "session={session_id} expected_generation={expected_preparation_generation} current_generation={current_preparation_generation} turn_id={preparation_turn_id}"
                    ),
                );
                return Ok(());
            }
            spawn_workspace_session_persist_task(
                state_for_preparation.clone(),
                controller_runtime_for_preparation.clone(),
                session_id,
                Some(String::from("submit_begin")),
            );
            let precheck_started = start_webui_precheck_or_continue(
                &app_for_preparation,
                &state_for_preparation,
                &controller_runtime_for_preparation,
                &agent_runtime_for_preparation,
                precheck_start,
                precheck_plan,
            )?;
            webui_submit_trace(
                "submit_prompt_precheck_dispatch_returned",
                format!("session={session_id} started={precheck_started}"),
            );
            Ok(())
        })();
        let preparation_elapsed_ms =
            current_unix_millis_value().saturating_sub(preparation_started_at);
        if let Err(error) = retain_slow_submission_diagnostic(
            &state_for_preparation,
            &accepted_client_submission_id,
            submission_target_label,
            "post_ack_preparation",
            preparation_elapsed_ms,
        ) {
            eprintln!("[webui-service] failed to retain slow submission diagnostic: {error}");
        }
        if let Err(error) = preparation_result {
            eprintln!("[webui-service] accepted submission preparation failed: {error}");
            if let Err(cleanup_error) = mark_webui_prompt_preparation_failed(
                &app_for_preparation,
                &state_for_preparation,
                &controller_runtime_for_preparation,
                &agent_runtime_for_preparation,
                session_id,
                &preparation_turn_id,
                &error,
            )
            .await
            {
                eprintln!(
                    "[webui-service] accepted submission preparation cleanup failed: {cleanup_error}"
                );
            }
        }
    });
    Ok(acknowledgement)
}

/// Promotes one accepted composer draft lease into the canonical user-message owner.
/// 将一条已接纳输入区草稿租约提升为规范用户消息所有者。
///
/// # Parameters
/// 参数
/// `workspace_path` selects the project asset index and `session_id`/`message_id` identify the durable owner.
/// `workspace_path` 选择项目资产索引，`session_id` 与 `message_id` 标识持久所有者。
/// `attachments` contains canonical staged assets and `draft_scope` is their optional transient owner.
/// `attachments` 包含规范暂存资产，`draft_scope` 是其可选瞬时所有者。
///
/// # Returns
/// 返回值
/// Success after atomic promotion, or one explicit missing-owner/index error.
/// 原子提升成功后返回成功，或返回明确的所有者/索引缺失错误。
pub(super) fn promote_submission_attachment_draft_scope(
    workspace_path: &str,
    session_id: SessionId,
    message_id: Option<&str>,
    attachments: &[vulcan_core::types::ConversationAttachment],
    draft_scope: Option<&str>,
) -> Result<(), String> {
    let Some(draft_scope) = draft_scope.filter(|scope| !scope.trim().is_empty()) else {
        return Ok(());
    };
    if attachments.is_empty() {
        return Ok(());
    }
    let message_id = message_id.ok_or_else(|| {
        String::from("Accepted attachment draft lease has no canonical user message owner.")
    })?;
    promote_staged_attachment_to_message_ref(
        workspace_path,
        session_id.to_string().as_str(),
        message_id,
        attachments,
        &[draft_scope.to_string()],
    )
}

/// Reads the latest accepted workbench revision from shared service state.
/// 从共享服务状态读取最新已接受的工作台版本号。
fn current_workbench_revision_from_state(state: &SharedServiceBridgeState) -> Result<u64, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench_generation)
}

/// Emits the latest queued-prompt projection as an ordered workbench delta.
/// 将最新排队提示词投影作为有序工作台增量发出。
///
/// # Parameters
/// # 参数
/// - `app`: Event emitter that publishes the ordered projection.
/// - `app`：发布有序投影的事件发送器。
/// - `state`: Shared service state containing the authoritative workspace and revision.
/// - `state`：包含权威工作区与版本号的共享服务状态。
/// - `session_id`: Conversation whose current run phase must be projected.
/// - `session_id`：需要投影当前运行阶段的会话。
///
/// # Returns
/// # 返回值
/// The committed workbench revision and authoritative current run phase, or an exact state error.
/// 已提交的工作台版本与权威当前运行阶段，或准确的状态错误。
pub(crate) fn emit_queued_prompt_run_phase_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: Uuid,
) -> Result<(u64, SessionRunPhase), String> {
    let (event, run_phase) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // workspace is reread under the generation lock so later turn advancement always wins.
        // workspace 在版本锁内重新读取，使之后发生的回合推进始终胜出。
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("cannot project queued prompt without a retained workspace snapshot")
        })?;
        // run_phase is the current authoritative phase, never the value captured by queue commit.
        // run_phase 是当前权威阶段，绝不使用排队提交时捕获的旧值。
        let run_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        let conversation_id = session_id.to_string();
        let run_phase_text = run_phase.to_string();
        let mut status = None;
        let mut last_updated_label = None;
        if let Some(conversation) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation_id)
        {
            conversation.run_phase = run_phase_text.clone();
            status = Some(conversation.status.clone());
            last_updated_label = Some(conversation.last_updated_label.clone());
        }
        (
            WorkbenchDeltaEnvelope {
                generation: revision,
                delta: WorkbenchDelta::UpdateConversationRunPhase(
                    UpdateConversationRunPhaseDelta {
                        conversation_id,
                        run_phase: run_phase_text,
                        status,
                        last_updated_label,
                    },
                ),
            },
            run_phase,
        )
    };
    let generation = event.generation;
    emit_service_event_with_trace(
        app,
        "workbench_queued_prompt_run_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok((generation, run_phase))
}

#[cfg(test)]
mod submission_hydration_policy_tests {
    use super::*;

    /// Verifies a first-prompt edit treats its intentionally empty retained history as authoritative.
    /// 验证编辑第一条提示词时会将有意清空的保留历史视为权威状态。
    #[test]
    fn first_prompt_edit_does_not_rehydrate_the_removed_original_prompt() {
        let session_id = SessionId::from_u128(901);

        assert!(!existing_submission_requires_hydration(
            Some(session_id),
            ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ));
        assert!(existing_submission_requires_hydration(
            Some(session_id),
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ));
    }

    /// Verifies new-session submissions never enter the existing-session hydration path.
    /// 验证新会话提交永远不会进入已有会话水合路径。
    #[test]
    fn new_session_submission_never_requires_existing_session_hydration() {
        assert!(!existing_submission_requires_hydration(
            None,
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ));
    }
}

#[cfg(test)]
mod submission_session_settings_tests {
    use super::{
        SubmissionSessionSettings, should_preserve_existing_provider_tool_settings,
        submission_output_style_id, submission_output_style_snapshot,
    };
    use vulcan_agent_runtime::provider_config::PersistedPromptOutputStyleSelection;

    /// Verifies only an explicitly overridden session is preserved when a submission omits settings.
    /// 验证提交缺少设置时仅保留显式覆盖的会话。
    #[test]
    fn omitted_settings_preserve_explicit_session_only() {
        assert!(should_preserve_existing_provider_tool_settings(
            false, true, true
        ));
        assert!(!should_preserve_existing_provider_tool_settings(
            true, true, true
        ));
        assert!(!should_preserve_existing_provider_tool_settings(
            false, true, false
        ));
        assert!(!should_preserve_existing_provider_tool_settings(
            false, false, false
        ));
    }

    /// Verifies an ordinary existing-session submission cannot activate a newly selected style.
    /// 验证已有会话的普通提交不能激活新选择的输出风格。
    #[test]
    fn ordinary_submission_keeps_output_style_snapshot_unchanged() {
        let settings = SubmissionSessionSettings {
            output_style_id: Some(String::from("rigorous")),
            vmm_policy: None,
            provider_tool_settings: None,
            knowledge_selection: None,
            initial_goal: None,
        };
        assert!(
            submission_output_style_snapshot(
                Some(&settings),
                PersistedPromptOutputStyleSelection::Balanced,
                false,
            )
            .expect("ordinary submission should be accepted")
            .is_none()
        );
        assert_eq!(
            submission_output_style_snapshot(
                Some(&settings),
                PersistedPromptOutputStyleSelection::Balanced,
                true,
            )
            .expect("new-session boundary should resolve the style")
            .expect("new-session boundary should capture a style")
            .id,
            "rigorous"
        );
        assert_eq!(
            submission_output_style_id(Some(&settings)).as_deref(),
            Some("rigorous")
        );
    }
}
