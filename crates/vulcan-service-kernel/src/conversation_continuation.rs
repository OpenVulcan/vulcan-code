//! Authoritative continuation eligibility for terminated WebUI conversations.
//! 已终止 WebUI 会话的权威 Continue 资格判定。

use vulcan_host_tools::command_session_runtime::CommandSessionStatus;
use vulcan_service_api::ConversationContinuationSummary;
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetrySnapshotState, SessionId,
    SessionMessageKind, SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry,
    SessionState,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::{
    ServiceBridgeState, is_active_session_run_phase, pending_question_snapshots_from_agent_runtime,
    runtime_state_has_queued_pending_prompt_work, webui_session_kernel_has_active_turn,
};

/// Live blockers that are not fully represented by the persisted session runtime state.
/// 未完整保存在会话运行态中的实时 Continue 阻断条件。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SessionContinuationRuntimeBlockers {
    /// Whether a service-owned stream task still owns the session.
    /// 服务持有的流任务是否仍占用该会话。
    pub(crate) has_active_stream_task: bool,
    /// Whether the session kernel still owns an active turn.
    /// 会话内核是否仍持有活动回合。
    pub(crate) has_active_kernel_turn: bool,
    /// Whether an exact lifecycle or submission transaction currently owns turn admission.
    /// 精确生命周期或提交事务当前是否持有回合准入权。
    pub(crate) has_turn_admission_reservation: bool,
    /// Whether an accepted terminal report is still crossing its durable main-state boundary.
    /// 已接纳终态报告是否仍在跨越可靠主状态边界。
    pub(crate) has_terminal_persistence_owner: bool,
    /// Whether the persisted session summary still reports a running execution.
    /// 持久化会话摘要是否仍报告执行中。
    pub(crate) session_is_running: bool,
    /// Whether the shared Ask runtime contains a pending question for this session.
    /// 共享 Ask 运行时是否存在该会话的待回答问题。
    pub(crate) has_pending_question: bool,
    /// Whether a live command session owned by this conversation is still running.
    /// 该会话拥有的实时命令会话是否仍在运行。
    pub(crate) has_running_command_session: bool,
    /// Whether a child task in this parent tree is still running.
    /// 该父会话树中是否仍有子任务在运行。
    pub(crate) has_running_task: bool,
}

impl SessionContinuationRuntimeBlockers {
    /// Returns whether any live owner still prevents a terminal continuation.
    /// 返回是否仍有任一实时所有者阻止终态 Continue。
    fn any(self) -> bool {
        self.has_active_stream_task
            || self.has_active_kernel_turn
            || self.has_turn_admission_reservation
            || self.has_terminal_persistence_owner
            || self.session_is_running
            || self.has_pending_question
            || self.has_running_command_session
            || self.has_running_task
    }
}

/// Computes the claim-equivalent provider-visible state digest for display-time admission.
/// 为展示阶段准入计算与领取校验等价的 Provider 可见状态摘要。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the canonical conversation state inspected by the provider bridge.
/// `runtime_state` 持有由 Provider 桥接器检查的规范会话状态。
///
/// # Returns
/// 返回值
/// The lowercase SHA-256 digest and whether the current boundary ends in a tool result.
/// 返回小写 SHA-256 摘要，以及当前边界是否以工具结果结束。
fn continuation_canonical_dispatch_state(
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(String, bool), &'static str> {
    // canonicalSnapshot uses the same lossless bridge used by the authoritative claim validator.
    // canonicalSnapshot 使用与权威领取校验器相同的无损桥接器。
    let canonical_snapshot = vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
        &runtime_state.conversation_state,
    )
    .map_err(|_| "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED")?;
    // encoded contains exactly the fields accepted by transcript-only drift reconciliation.
    // encoded 精确包含仅转录漂移协调所接受的字段。
    let encoded = serde_json::to_vec(&(
        &canonical_snapshot.messages,
        &canonical_snapshot.continuation,
    ))
    .map_err(|_| "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED")?;
    // digest remains lowercase so it can be compared directly with the strict v4 reference.
    // digest 保持小写，可直接与严格 v4 引用比较。
    let digest = {
        use sha2::{Digest, Sha256};

        Sha256::digest(&encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    let ends_in_tool_result = matches!(
        canonical_snapshot
            .messages
            .last()
            .map(|message| &message.payload),
        Some(vulcan_core::types::ConversationPayload::ToolResult { .. })
    );
    Ok((digest, ends_in_tool_result))
}

/// Returns the latest visible canonical user, assistant, or error tail that controls prompt-free continuation.
/// 返回控制无提示词继续资格的最近一条可见规范用户、助手或错误尾部。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the canonical conversation record stream inspected at the terminal boundary.
/// `runtime_state` 持有在终态边界检查的规范会话记录流。
/// # Returns
/// 返回值
/// Returns the stable tail-kind key, or `None` when no canonical dialogue record exists.
/// 返回稳定的尾部类型键；不存在规范对话记录时返回 `None`。
pub(crate) fn canonical_conversation_tail_kind(
    runtime_state: &SessionRuntimeStateEntry,
) -> Option<&'static str> {
    runtime_state
        .conversation_state
        .records
        .iter()
        .rev()
        .find(|record| {
            record.visible
                && matches!(
                    record.record_kind,
                    SessionMessageRecordKind::Transcript
                        | SessionMessageRecordKind::AssistantOutput
                )
        })
        .map(|record| match record.kind {
            SessionMessageKind::Assistant => "assistant",
            SessionMessageKind::User => "user",
            SessionMessageKind::Error => "error",
        })
}

/// Returns whether one canonical tail kind supports prompt-free continuation.
/// 返回一个规范尾部类型是否支持无提示词继续。
///
/// # Parameters
/// 参数
/// `tail_kind` is the exact canonical or terminal-ledger tail label to classify.
/// `tail_kind` 是需要分类的精确规范尾部或终态账本尾部标签。
///
/// # Returns
/// 返回值
/// `true` only for user and error tails; assistant, empty, and synthetic labels fail closed.
/// 仅用户与错误尾部返回 `true`；助手、空值与合成标签均失败关闭。
pub(crate) fn is_prompt_free_continuation_tail_kind(tail_kind: &str) -> bool {
    matches!(tail_kind, "user" | "error")
}

/// Resolves one continuation opportunity from its persisted terminal boundary and live blockers.
/// 根据持久化终止边界与实时阻断条件解析一次 Continue 机会。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the canonical transcript and provider-call ledger.
/// `runtime_state` 持有规范转录与 Provider 调用账本。
/// `run_phase` is the current authoritative session phase.
/// `run_phase` 是当前权威会话阶段。
/// `blockers` contains live owners that cannot be reconstructed from the ledger alone.
/// `blockers` 包含无法仅从账本重建的实时所有者。
/// `expected` optionally requires one exact client-visible generation and ledger revision.
/// `expected` 可选要求客户端可见的精确代际与账本修订号。
///
/// # Returns
/// 返回值
/// Success only when the same terminal continuation can be displayed or atomically claimed.
/// 仅当同一终态 Continue 可被展示或原子领取时返回成功。
pub(crate) fn resolve_session_continuation_eligibility(
    runtime_state: &SessionRuntimeStateEntry,
    run_phase: SessionRunPhase,
    blockers: SessionContinuationRuntimeBlockers,
    expected: Option<(u64, u64)>,
) -> Result<(), &'static str> {
    if blockers.has_terminal_persistence_owner {
        return Err("ERR_CONVERSATION_CONTINUE_TERMINAL_PERSISTENCE_IN_FLIGHT");
    }
    if blockers.has_turn_admission_reservation {
        return Err("ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT");
    }
    if is_active_session_run_phase(run_phase) || blockers.any() {
        return Err("ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE");
    }
    if runtime_state_has_queued_pending_prompt_work(runtime_state)
        || runtime_state
            .active_precheck_turn_id
            .as_deref()
            .is_some_and(|turn_id| !turn_id.trim().is_empty())
    {
        return Err("ERR_CONVERSATION_CONTINUE_SESSION_BUSY");
    }
    let retry_snapshot = runtime_state
        .execution_retry_snapshot
        .as_ref()
        .ok_or("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING")?;
    if retry_snapshot.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
        return Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED");
    }
    if !retry_snapshot.replay_safety_proven() {
        return Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE");
    }
    if retry_snapshot.pending_action_label == "resume_preparation" {
        let preparation_route_selection = retry_snapshot
            .preparation_route_selection
            .as_ref()
            .ok_or("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE")?;
        if runtime_state.active_provider_id != preparation_route_selection.active_provider_id
            || runtime_state.active_connection_id
                != preparation_route_selection.active_connection_id
            || runtime_state.active_model_id != preparation_route_selection.active_model_id
            || runtime_state.provider_route_lease != retry_snapshot.provider_route
        {
            return Err("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED");
        }
    } else {
        let provider_route = retry_snapshot
            .provider_route
            .as_ref()
            .ok_or("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING")?;
        if runtime_state.provider_route_lease.as_ref() != Some(provider_route) {
            return Err("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED");
        }
    }
    if runtime_state
        .provider_call_ledger
        .active_context_generation_id
        != retry_snapshot.context_generation_id
    {
        return Err("ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED");
    }
    let (canonical_dispatch_sha256, ends_in_tool_result) =
        continuation_canonical_dispatch_state(runtime_state)?;
    if canonical_dispatch_sha256 != retry_snapshot.canonical_dispatch_sha256 {
        return Err("ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED");
    }
    if retry_snapshot.pending_action_label == "prepare_next_provider_request"
        && !ends_in_tool_result
    {
        return Err("ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING");
    }
    if retry_snapshot.state != ExecutionRetrySnapshotState::Frozen
        || retry_snapshot.claimed_attempt_id.is_some()
    {
        return Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN");
    }
    let ledger = &runtime_state.provider_call_ledger;
    let continuation = &ledger.continuation;
    if !continuation.eligible
        || continuation.claimed_at_ms.is_some()
        || continuation.invalidated_at_ms.is_some()
        || continuation
            .terminal_reason
            .as_deref()
            .is_none_or(str::is_empty)
        || continuation.terminal_at_ms.is_none()
        || continuation.expected_ledger_revision != ledger.revision
    {
        return Err("ERR_CONVERSATION_CONTINUE_STALE");
    }
    if let Some((expected_generation, expected_ledger_revision)) = expected
        && (continuation.generation != expected_generation
            || continuation.expected_ledger_revision != expected_ledger_revision)
    {
        return Err("ERR_CONVERSATION_CONTINUE_STALE");
    }
    if ledger.in_flight_call_count() != 0 {
        return Err("ERR_CONVERSATION_CONTINUE_CALL_IN_FLIGHT");
    }
    let tail_kind = canonical_conversation_tail_kind(runtime_state)
        .ok_or("ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_MISSING")?;
    if !is_prompt_free_continuation_tail_kind(tail_kind)
        || continuation.canonical_tail_kind.as_deref() != Some(tail_kind)
    {
        return Err("ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_INVALID");
    }
    Ok(())
}

/// Projects one stored continuation through the authoritative eligibility resolver.
/// 通过权威资格解析器投影一条已保存的 Continue 状态。
///
/// # Parameters
/// 参数
/// `runtime_state`, `run_phase`, and `blockers` are the same facts accepted by the resolver.
/// `runtime_state`、`run_phase` 与 `blockers` 是解析器接收的同一组事实。
///
/// # Returns
/// 返回值
/// A generation-preserving summary, or `None` when no terminal boundary has ever existed.
/// 返回保留代际的摘要；从未存在终止边界时返回 `None`。
pub(crate) fn conversation_continuation_summary(
    runtime_state: &SessionRuntimeStateEntry,
    run_phase: SessionRunPhase,
    blockers: SessionContinuationRuntimeBlockers,
) -> Option<ConversationContinuationSummary> {
    let continuation = &runtime_state.provider_call_ledger.continuation;
    // A normally completed assistant turn has no recovery boundary to expose.
    // 正常完成的助手回合不存在可恢复边界，不应向 WebUI 投影 Continue 卡片。
    if continuation.terminal_reason.as_deref() == Some("completed") {
        return None;
    }
    let eligibility =
        resolve_session_continuation_eligibility(runtime_state, run_phase, blockers, None);
    let blocked_reason = eligibility.as_ref().err().map(|error| {
        if *error == "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE" {
            return runtime_state
                .execution_retry_snapshot
                .as_ref()
                .and_then(|reference| reference.blocked_reason.clone())
                .unwrap_or_else(|| (*error).to_string());
        }
        (*error).to_string()
    });
    (!continuation.is_empty()).then(|| ConversationContinuationSummary {
        eligible: eligibility.is_ok(),
        generation: continuation.generation,
        expected_ledger_revision: continuation.expected_ledger_revision,
        reason: continuation.terminal_reason.clone(),
        blocked_reason,
    })
}

/// Collects all live continuation blockers from one lock-consistent service-state revision.
/// 从同一锁一致的服务状态修订中收集全部实时 Continue 阻断条件。
///
/// # Parameters
/// 参数
/// `state` is the locked service state, `workspace` its matching snapshot, and `session_id` the target conversation.
/// `state` 是已锁定服务状态，`workspace` 是与其匹配的快照，`session_id` 是目标会话。
///
/// # Returns
/// 返回值
/// The complete live blocker set, or an explicit Ask/task registry inspection error.
/// 返回完整实时阻断集合，或明确的 Ask/任务注册表检查错误。
pub(crate) fn session_continuation_runtime_blockers(
    state: &ServiceBridgeState,
    workspace: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<SessionContinuationRuntimeBlockers, String> {
    let session_id_text = session_id.to_string();
    let has_pending_question =
        pending_question_snapshots_from_agent_runtime(state.agent_runtime.as_ref())?
            .iter()
            .any(|question| question.session_id == session_id_text);
    let has_running_command_session = state.agent_runtime.as_ref().is_some_and(|runtime| {
        runtime
            .command_session_runtime
            .try_snapshot_sessions()
            .iter()
            .any(|snapshot| {
                snapshot.owner_session_id == session_id
                    && snapshot.status == CommandSessionStatus::Running
            })
    });
    let has_running_task = state
        .agent_runtime
        .as_ref()
        .map(|runtime| {
            runtime
                .task_runtime
                .open_subagent_registry_entries_for_parent_tree(session_id)
                .map(|entries| entries.iter().any(|entry| entry.status.is_running()))
        })
        .transpose()
        .map_err(|error| format!("failed to inspect continuation child tasks: {error}"))?
        .unwrap_or(false);
    Ok(SessionContinuationRuntimeBlockers {
        has_active_stream_task: state.active_stream_tasks.contains_key(&session_id),
        has_active_kernel_turn: webui_session_kernel_has_active_turn(state, session_id),
        has_turn_admission_reservation: state
            .session_turn_admission_reservations
            .contains_key(&session_id),
        has_terminal_persistence_owner: state
            .session_terminal_persistence_owners
            .contains_key(&session_id),
        session_is_running: workspace
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .is_some_and(|session| session.status == SessionState::Running),
        has_pending_question,
        has_running_command_session,
        has_running_task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_context::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    };
    use vulcan_workbench_model::{
        EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPreparationRouteSelection,
        ExecutionRetrySnapshotRef, SessionMessageEntry, SessionProviderRouteLeaseIdentity,
    };

    /// Builds one lightweight frozen retry reference for continuation eligibility tests.
    /// 为 Continue 资格测试构建一个轻量冻结重试引用。
    fn frozen_retry_snapshot_reference() -> ExecutionRetrySnapshotRef {
        ExecutionRetrySnapshotRef {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 1,
            state: ExecutionRetrySnapshotState::Frozen,
            content_sha256: "0".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("execution-1"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: Some(retry_route("route-a")),
            preparation_route_selection: Some(ExecutionRetryPreparationRouteSelection {
                active_provider_id: None,
                active_connection_id: None,
                active_model_id: None,
            }),
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: None,
            error_summary: Some(String::from("failed")),
            updated_at: String::from("1000"),
        }
    }

    /// Converts one materialized message entry into the canonical record shape used by tests.
    /// 将一个已物化消息条目转换为测试所用的规范记录结构。
    fn conversation_record(entry: SessionMessageEntry) -> SessionConversationRecord {
        SessionConversationRecord {
            id: entry.id,
            kind: entry.kind,
            record_kind: entry.record_kind,
            visible: entry.visible,
            title: entry.title,
            body: entry.body,
            metadata: entry.metadata,
            created_at: entry.created_at,
        }
    }

    /// Builds one eligible failed terminal boundary for resolver tests.
    /// 为解析器测试构建一个可 Continue 的失败终止边界。
    fn eligible_runtime_state() -> SessionRuntimeStateEntry {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.execution_retry_snapshot = Some(frozen_retry_snapshot_reference());
        runtime_state.provider_route_lease = Some(retry_route("route-a"));
        runtime_state
            .conversation_state
            .records
            .push(conversation_record(
                SessionMessageEntry::new(SessionMessageKind::Error, "Provider error", "failed")
                    .with_runtime_metadata("error-1", "1000"),
            ));
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("execution-1")),
                String::from("failed"),
                1000,
                String::from("error"),
                true,
            );
        let (canonical_dispatch_sha256, _) = continuation_canonical_dispatch_state(&runtime_state)
            .expect("eligible canonical dispatch state should serialize");
        runtime_state
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot")
            .canonical_dispatch_sha256 = canonical_dispatch_sha256;
        runtime_state
    }

    /// Builds one exact route identity for route-drift resolver tests.
    /// 为路由漂移解析器测试构建一个精确路由身份。
    fn retry_route(route_revision: &str) -> SessionProviderRouteLeaseIdentity {
        SessionProviderRouteLeaseIdentity {
            schema_version: 1,
            provider_id: String::from("provider-a"),
            connection_id: String::from("connection-a"),
            model_id: String::from("model-a"),
            protocol_profile_id: String::from("openai-responses.v1"),
            manifest_revision: 1,
            manifest_generation: 1,
            catalog_revision: String::from("catalog-a"),
            route_revision: route_revision.to_string(),
        }
    }

    /// Verifies prompt-free continuation accepts only exact user and error tail labels.
    /// 验证无提示词继续只接受精确的用户与错误尾部标签。
    #[test]
    fn continuation_tail_kind_requires_exact_user_or_error() {
        assert!(is_prompt_free_continuation_tail_kind("user"));
        assert!(is_prompt_free_continuation_tail_kind("error"));
        assert!(!is_prompt_free_continuation_tail_kind("assistant"));
        assert!(!is_prompt_free_continuation_tail_kind("empty"));
        assert!(!is_prompt_free_continuation_tail_kind(""));
    }

    /// Verifies an exact failed terminal boundary remains claimable without live owners.
    /// 验证没有实时所有者时精确的失败终止边界仍可被领取。
    #[test]
    fn continuation_resolver_accepts_exact_terminal_boundary() {
        let runtime_state = eligible_runtime_state();
        let continuation = &runtime_state.provider_call_ledger.continuation;

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                Some((
                    continuation.generation,
                    continuation.expected_ledger_revision,
                )),
            ),
            Ok(())
        );
    }

    /// Verifies a pending model choice and its display-only audit record do not invalidate Continue.
    /// 验证待生效模型选择及其仅展示审计记录不会使 Continue 失效。
    #[test]
    fn continuation_resolver_ignores_pending_model_display_record() {
        // runtimeState starts from an exact recoverable terminal boundary.
        // runtimeState 从一个精确可恢复的终止边界开始。
        let mut runtime_state = eligible_runtime_state();
        // continuation freezes the client-visible CAS pair before the display-only mutation.
        // continuation 在仅展示变更前冻结客户端可见的 CAS 对。
        let continuation = runtime_state.provider_call_ledger.continuation.clone();
        runtime_state.pending_model_selection =
            Some(vulcan_workbench_model::SessionPendingModelSelection {
                provider_id: String::from("command-code"),
                connection_id: String::from("command-code-account"),
                model_id: String::from("deepseek"),
            });
        // displayRecord models the i18n-backed timeline entry created by the model picker.
        // displayRecord 模拟模型选择器创建的 i18n 时间线记录。
        let mut display_record = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "",
            "provider_id=command-code\nconnection_id=command-code-account\nmodel_id=deepseek",
        )
        .with_runtime_metadata("model-switch-1", "1001");
        display_record.record_kind = SessionMessageRecordKind::DisplayNote;
        display_record.metadata = Some(serde_json::json!({
            "notice_kind": "model_switch",
            "activation": "next_submission",
        }));
        runtime_state
            .conversation_state
            .records
            .push(conversation_record(display_record));

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                Some((
                    continuation.generation,
                    continuation.expected_ledger_revision,
                )),
            ),
            Ok(())
        );
    }

    /// Verifies a changed frozen provider route is blocked before the Continue button can claim it.
    /// 验证冻结供应商路由变化会在 Continue 按钮领取前被阻断。
    #[test]
    fn continuation_resolver_rejects_frozen_provider_route_drift() {
        let mut runtime_state = eligible_runtime_state();
        let expected_route = retry_route("route-a");
        let current_route = retry_route("route-b");
        runtime_state
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot")
            .provider_route = Some(expected_route);
        runtime_state.provider_route_lease = Some(current_route);

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED")
        );
        assert_eq!(
            conversation_continuation_summary(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
            )
            .and_then(|summary| summary.blocked_reason),
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED"
            ))
        );
    }

    /// Verifies a missing live lease is rejected when the frozen reference names an exact route.
    /// 验证冻结引用指定精确路由时，缺失的实时租约会被拒绝。
    #[test]
    fn continuation_resolver_rejects_missing_live_provider_route() {
        // runtimeState starts from an otherwise exact claimable boundary.
        // runtimeState 从其他条件均精确可领取的边界开始。
        let mut runtime_state = eligible_runtime_state();
        runtime_state.provider_route_lease = None;

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED")
        );
    }

    /// Verifies pre-resolution model-selection drift is blocked before a preparation retry is clickable.
    /// 验证解析前模型选择漂移会在准备阶段重试可点击前被阻断。
    #[test]
    fn continuation_resolver_rejects_preparation_selection_drift() {
        // runtimeState changes only the current model selection after the retry boundary was frozen.
        // runtimeState 仅修改重试边界冻结后的当前模型选择。
        let mut runtime_state = eligible_runtime_state();
        runtime_state.active_model_id = Some(String::from("model-b"));

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED")
        );
    }

    /// Verifies context-generation drift is blocked by the display-time resolver.
    /// 验证上下文代际漂移会被展示阶段解析器阻断。
    #[test]
    fn continuation_resolver_rejects_context_generation_drift() {
        // runtimeState advances the live context generation without changing the frozen pointer.
        // runtimeState 在不修改冻结指针的情况下推进实时上下文代际。
        let mut runtime_state = eligible_runtime_state();
        runtime_state
            .provider_call_ledger
            .active_context_generation_id = Some(String::from("context-b"));

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED")
        );
    }

    /// Verifies provider-visible canonical drift is blocked before the Continue claim reads its body.
    /// 验证 Provider 可见规范状态漂移会在 Continue 领取读取正文前被阻断。
    #[test]
    fn continuation_resolver_rejects_canonical_dispatch_drift() {
        // runtimeState appends one provider-visible user message after the exact retry digest was frozen.
        // runtimeState 在精确重试摘要冻结后追加一条 Provider 可见用户消息。
        let mut runtime_state = eligible_runtime_state();
        runtime_state
            .conversation_state
            .messages
            .push(SessionConversationMessage {
                id: String::from("user-later"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("changed state"),
                    attachments: Vec::new(),
                },
            });

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED")
        );
    }

    /// Verifies a legacy reference without a route is blocked once the live session owns one.
    /// 验证旧引用缺少路由而实时会话已有路由时会被阻断。
    #[test]
    fn continuation_resolver_rejects_missing_route_against_live_route() {
        let mut runtime_state = eligible_runtime_state();
        runtime_state
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot")
            .provider_route = None;
        runtime_state.provider_route_lease = Some(retry_route("route-live"));

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED")
        );
    }

    /// Verifies an interrupted run may continue from its submitted user tail until a later submission invalidates it.
    /// 验证中断运行可从其已提交用户尾部继续，直至后续提交使其失效。
    #[test]
    fn continuation_resolver_accepts_terminal_user_tail_until_new_submission() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.execution_retry_snapshot = Some(frozen_retry_snapshot_reference());
        runtime_state.provider_route_lease = Some(retry_route("route-a"));
        runtime_state
            .conversation_state
            .records
            .push(conversation_record(
                SessionMessageEntry::new(SessionMessageKind::User, "User", "run this")
                    .with_runtime_metadata("user-1", "1000"),
            ));
        runtime_state
            .provider_call_ledger
            .mark_continuation_terminal(
                Some(String::from("execution-1")),
                String::from("interrupted_after_restart"),
                1000,
                String::from("user"),
                true,
            );
        // canonicalDispatchSha256 freezes the exact submitted user-tail state accepted by this fixture.
        // canonicalDispatchSha256 冻结当前夹具接受的精确已提交用户尾部状态。
        let (canonical_dispatch_sha256, _) = continuation_canonical_dispatch_state(&runtime_state)
            .expect("user-tail canonical dispatch state should serialize");
        runtime_state
            .execution_retry_snapshot
            .as_mut()
            .expect("user-tail fixture must retain a retry snapshot")
            .canonical_dispatch_sha256 = canonical_dispatch_sha256;
        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Idle,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Ok(())
        );

        assert!(
            runtime_state
                .provider_call_ledger
                .invalidate_continuation_for_submission(String::from("submission-2"), 1100)
        );
        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Idle,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_STALE")
        );
    }

    /// Verifies continuation projection rejects a missing or actively claimed retry sidecar.
    /// 验证 Continue 投影会拒绝缺失或正被领取的重试 sidecar。
    #[test]
    fn continuation_resolver_requires_frozen_unclaimed_retry_snapshot() {
        let mut missing_snapshot = eligible_runtime_state();
        missing_snapshot.execution_retry_snapshot = None;
        assert_eq!(
            resolve_session_continuation_eligibility(
                &missing_snapshot,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING")
        );

        let mut unsafe_snapshot = eligible_runtime_state();
        let retry_snapshot = unsafe_snapshot
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot");
        retry_snapshot.retryable = false;
        retry_snapshot.requires_manual_confirmation = true;
        retry_snapshot.blocked_reason = Some(String::from("tool_side_effect_in_flight"));
        assert_eq!(
            resolve_session_continuation_eligibility(
                &unsafe_snapshot,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE")
        );
        assert_eq!(
            conversation_continuation_summary(
                &unsafe_snapshot,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
            )
            .and_then(|summary| summary.blocked_reason),
            Some(String::from("tool_side_effect_in_flight"))
        );

        let mut unsupported_snapshot = eligible_runtime_state();
        let retry_reference = unsupported_snapshot
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot");
        retry_reference.pending_action_label = String::from("finalize_assistant");
        assert_eq!(
            resolve_session_continuation_eligibility(
                &unsupported_snapshot,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE")
        );

        let mut claimed_snapshot = eligible_runtime_state();
        let retry_snapshot = claimed_snapshot
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot");
        retry_snapshot.state = ExecutionRetrySnapshotState::Claimed;
        retry_snapshot.claimed_attempt_id = Some(String::from("attempt-2"));
        assert_eq!(
            resolve_session_continuation_eligibility(
                &claimed_snapshot,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN")
        );
        assert_eq!(
            conversation_continuation_summary(
                &claimed_snapshot,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
            )
            .map(|summary| summary.eligible),
            Some(false)
        );

        let mut active_with_stale_block_reason = eligible_runtime_state();
        active_with_stale_block_reason
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot")
            .blocked_reason = Some(String::from("tool_side_effect_in_flight"));
        let active_summary = conversation_continuation_summary(
            &active_with_stale_block_reason,
            SessionRunPhase::Requesting,
            SessionContinuationRuntimeBlockers {
                has_active_stream_task: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
        )
        .expect("stored continuation must remain visible for active-state diagnostics");
        assert_eq!(
            active_summary.blocked_reason,
            Some(String::from("ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE"))
        );
    }

    /// Verifies a previous retry schema is reported explicitly instead of being collapsed into generic unsafety.
    /// 验证旧版重试 Schema 会被明确报告，而不是折叠为笼统的不安全状态。
    #[test]
    fn continuation_resolver_reports_unsupported_retry_snapshot_version() {
        // runtimeState starts from an otherwise fully claimable terminal boundary.
        // runtimeState 从其他条件均可领取的终止边界开始。
        let mut runtime_state = eligible_runtime_state();
        // reference simulates the immediately previous strict contract without attempting migration.
        // reference 模拟紧邻的旧严格契约，且不尝试迁移。
        let reference = runtime_state
            .execution_retry_snapshot
            .as_mut()
            .expect("eligible fixture must retain a retry snapshot");
        reference.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION - 1;

        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED")
        );
        assert_eq!(
            conversation_continuation_summary(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
            )
            .and_then(|summary| summary.blocked_reason),
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED"
            ))
        );
    }

    /// Verifies a normally completed assistant turn does not project a blocked recovery card.
    /// 验证正常完成的助手回合不会投影阻断态恢复卡片。
    #[test]
    fn continuation_summary_suppresses_normal_completion() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        assert!(
            runtime_state
                .provider_call_ledger
                .mark_continuation_terminal(
                    None,
                    String::from("completed"),
                    1000,
                    String::from("assistant"),
                    false,
                )
        );

        assert!(
            conversation_continuation_summary(
                &runtime_state,
                SessionRunPhase::Ready,
                SessionContinuationRuntimeBlockers::default(),
            )
            .is_none()
        );
    }

    /// Verifies every live-owner class suppresses a stored continuation opportunity.
    /// 验证每一种实时所有者都会隐藏已保存的 Continue 机会。
    #[test]
    fn continuation_resolver_rejects_all_live_owner_classes() {
        let runtime_state = eligible_runtime_state();
        let blocker_cases = [
            (
                SessionContinuationRuntimeBlockers {
                    has_active_stream_task: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    has_active_kernel_turn: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    has_turn_admission_reservation: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    has_terminal_persistence_owner: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_TERMINAL_PERSISTENCE_IN_FLIGHT",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    session_is_running: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    has_pending_question: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    has_running_command_session: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
            ),
            (
                SessionContinuationRuntimeBlockers {
                    has_running_task: true,
                    ..SessionContinuationRuntimeBlockers::default()
                },
                "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
            ),
        ];

        for (blockers, expected_error) in blocker_cases {
            assert_eq!(
                resolve_session_continuation_eligibility(
                    &runtime_state,
                    SessionRunPhase::Failed,
                    blockers,
                    None,
                ),
                Err(expected_error)
            );
        }
    }

    /// Verifies the lock-consistent blocker collector observes real Stop reservations and terminal-persistence owners independently.
    /// 验证锁一致阻塞收集器会分别观察真实 Stop 预留与终态持久化所有者。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if either authoritative in-process owner is omitted.
    /// 此测试无返回值；任一权威进程内所有者被遗漏时测试失败。
    #[test]
    fn continuation_blocker_collector_observes_stop_and_terminal_persistence_owners() {
        // session_id identifies the exact regular conversation whose live ownership changes.
        // session_id 标识实时所有权发生变化的精确普通会话。
        let session_id = SessionId::new_v4();
        // state provides the authoritative reservation and persistence-owner tables.
        // state 提供权威预留表与持久化所有者表。
        let mut state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        // workspace is the same retained revision paired with each live blocker collection.
        // workspace 是与每次实时阻塞收集配对的同一保留修订。
        let workspace = state
            .workspace_snapshot
            .clone()
            .expect("test workspace should exist");
        // stop_reservation owns admission throughout one Stop transaction.
        // stop_reservation 在一次 Stop 事务期间持有准入权。
        let stop_reservation = crate::SessionTurnAdmissionReservation {
            reservation_id: uuid::Uuid::new_v4(),
            kind: crate::SessionTurnAdmissionReservationKind::RunStop,
        };
        state
            .session_turn_admission_reservations
            .insert(session_id, stop_reservation);
        let stop_blockers = session_continuation_runtime_blockers(&state, &workspace, session_id)
            .expect("Stop blocker collection should succeed");
        assert!(stop_blockers.has_turn_admission_reservation);
        assert!(!stop_blockers.has_terminal_persistence_owner);

        state
            .session_turn_admission_reservations
            .remove(&session_id);
        // terminal_owner spans exact active-owner removal through durable terminal persistence.
        // terminal_owner 跨越精确活动所有者移除直至可靠终态持久化。
        let terminal_owner = crate::SessionTerminalPersistenceOwner::RunStop(uuid::Uuid::new_v4());
        state
            .session_terminal_persistence_owners
            .insert(session_id, terminal_owner);
        let persistence_blockers =
            session_continuation_runtime_blockers(&state, &workspace, session_id)
                .expect("terminal-persistence blocker collection should succeed");
        assert!(!persistence_blockers.has_turn_admission_reservation);
        assert!(persistence_blockers.has_terminal_persistence_owner);
    }

    /// Verifies persisted precheck and queued-prompt work suppress terminal Continue admission.
    /// 验证已持久化的预检与排队提示词工作会阻止终态 Continue 准入。
    #[test]
    fn continuation_resolver_rejects_precheck_and_queued_prompt_work() {
        let mut precheck_state = eligible_runtime_state();
        precheck_state.active_precheck_turn_id = Some(String::from("precheck-1"));
        assert_eq!(
            resolve_session_continuation_eligibility(
                &precheck_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_SESSION_BUSY")
        );

        let mut queued_state = eligible_runtime_state();
        queued_state.ui_state.pending_prompt_queue.push(
            vulcan_workbench_model::PendingSessionPromptEntry {
                id: String::from("queued-1"),
                prompt: String::from("next user prompt"),
                hidden_model_guidance: None,
                preview: String::from("next user prompt"),
                attachments: Vec::new(),
                model_reference: None,
                runtime_options: Default::default(),
                source: vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                task_id: None,
                panel_entry_id: None,
                created_at: String::from("1001"),
            },
        );
        assert_eq!(
            resolve_session_continuation_eligibility(
                &queued_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_SESSION_BUSY")
        );

        let mut staged_state = eligible_runtime_state();
        staged_state.ui_state.pending_prompt_panel_queue.push(
            vulcan_workbench_model::PendingPromptPanelEntry {
                id: String::from("staged-1"),
                prompt: String::from("user-controlled draft"),
                hidden_model_guidance: None,
                preview: String::from("user-controlled draft"),
                attachments: Vec::new(),
                model_reference: None,
                runtime_options: Default::default(),
                state: vulcan_workbench_model::PendingPromptPanelEntryState::Ready,
                created_at: String::from("1002"),
            },
        );
        assert_eq!(
            resolve_session_continuation_eligibility(
                &staged_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Ok(())
        );
    }

    /// Verifies client revisions and the persisted canonical tail cannot drift independently.
    /// 验证客户端修订号与持久化规范尾部不能独立漂移。
    #[test]
    fn continuation_resolver_rejects_stale_revision_and_tail() {
        let mut runtime_state = eligible_runtime_state();
        let continuation = runtime_state.provider_call_ledger.continuation.clone();
        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                Some((
                    continuation.generation,
                    continuation.expected_ledger_revision + 1
                )),
            ),
            Err("ERR_CONVERSATION_CONTINUE_STALE")
        );

        // assistant_output models the actual final-output record kind emitted by successful Provider runs.
        // assistant_output 模拟 Provider 成功运行实际生成的最终输出记录类型。
        let mut assistant_output =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "Answer", "complete")
                .with_runtime_metadata("assistant-1", "1001");
        assistant_output.record_kind = SessionMessageRecordKind::AssistantOutput;
        runtime_state
            .conversation_state
            .records
            .push(conversation_record(assistant_output));
        assert_eq!(
            resolve_session_continuation_eligibility(
                &runtime_state,
                SessionRunPhase::Failed,
                SessionContinuationRuntimeBlockers::default(),
                None,
            ),
            Err("ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_INVALID")
        );
    }
}
