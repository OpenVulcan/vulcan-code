//! Authoritative continuation eligibility for terminated WebUI conversations.
//! 已终止 WebUI 会话的权威 Continue 资格判定。

use vulcan_host_tools::command_session_runtime::CommandSessionStatus;
use vulcan_service_api::ConversationContinuationSummary;
use vulcan_workbench_model::{
    SessionId, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState,
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
            || self.session_is_running
            || self.has_pending_question
            || self.has_running_command_session
            || self.has_running_task
    }
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
    if tail_kind == "assistant" || continuation.canonical_tail_kind.as_deref() != Some(tail_kind) {
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
    (!continuation.is_empty()).then(|| ConversationContinuationSummary {
        eligible: resolve_session_continuation_eligibility(
            runtime_state,
            run_phase,
            blockers,
            None,
        )
        .is_ok(),
        generation: continuation.generation,
        expected_ledger_revision: continuation.expected_ledger_revision,
        reason: continuation.terminal_reason.clone(),
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
    use vulcan_context::SessionConversationRecord;
    use vulcan_workbench_model::SessionMessageEntry;

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
        runtime_state
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

    /// Verifies an interrupted run may continue from its submitted user tail until a later submission invalidates it.
    /// 验证中断运行可从其已提交用户尾部继续，直至后续提交使其失效。
    #[test]
    fn continuation_resolver_accepts_terminal_user_tail_until_new_submission() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
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

    /// Verifies every live-owner class suppresses a stored continuation opportunity.
    /// 验证每一种实时所有者都会隐藏已保存的 Continue 机会。
    #[test]
    fn continuation_resolver_rejects_all_live_owner_classes() {
        let runtime_state = eligible_runtime_state();
        let blocker_cases = [
            SessionContinuationRuntimeBlockers {
                has_active_stream_task: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
            SessionContinuationRuntimeBlockers {
                has_active_kernel_turn: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
            SessionContinuationRuntimeBlockers {
                session_is_running: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
            SessionContinuationRuntimeBlockers {
                has_pending_question: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
            SessionContinuationRuntimeBlockers {
                has_running_command_session: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
            SessionContinuationRuntimeBlockers {
                has_running_task: true,
                ..SessionContinuationRuntimeBlockers::default()
            },
        ];

        for blockers in blocker_cases {
            assert_eq!(
                resolve_session_continuation_eligibility(
                    &runtime_state,
                    SessionRunPhase::Failed,
                    blockers,
                    None,
                ),
                Err("ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE")
            );
        }
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
