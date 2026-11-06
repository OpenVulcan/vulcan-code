//! Host-initiated turn that wakes an idle session after its background shell jobs finished.
//! 后台 shell 任务结束后唤醒空闲会话的宿主发起回合。
//!
//! The finish notices themselves are process-local and reach the model as in-turn guidance before
//! the turn's first request; this module only owns the durable turn start and its settlement.
//! 结束通知本身只存在于进程内，并在本回合首次请求前作为回合内指引送达模型；本模块只负责
//! 可靠的回合起点及其结算。

use super::persistence::DirtyFile;
use super::{ActiveTurnState, SessionDocument, TurnOrigin, validate_system_context_content};
use crate::v1::collaboration::{
    CollaborationOperationId, CollaborationQuestionExecutionState, CollaborationRunControl,
    SetRunControlInput,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiPurposeKind, EventBody, EventVisibility, SystemContextEvent, SystemContextOperation,
    SystemContextScope, TurnOutcome,
};
use crate::v1::ids::{Seq, TurnId};

/// System-context key whose upsert opens a background shell-job continuation turn.
/// 其写入开启后台 shell 任务续跑回合的系统上下文键。
pub const SHELL_JOB_CONTINUATION_CONTEXT_KEY: &str = "shell_job_continuation";

/// Identity of one freshly opened shell-job continuation turn.
/// 一个新开启的 shell 任务续跑回合的身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellJobContinuationStart {
    /// The opened turn.
    /// 开启的回合。
    pub turn_id: TurnId,
    /// Provider-visible system-context seq that opened the turn.
    /// 开启该回合的 Provider 可见系统上下文 seq。
    pub context_seq: Seq,
}

/// Returns whether one event is the start authority of a shell-job continuation turn.
/// 返回一条事件是否为 shell 任务续跑回合的起点权威。
pub(crate) fn is_shell_job_continuation_start(event: &EventBody) -> bool {
    matches!(event, EventBody::SystemContext(context)
        if context.context_key == SHELL_JOB_CONTINUATION_CONTEXT_KEY
            && context.operation == SystemContextOperation::Upsert)
}

impl SessionDocument {
    /// Returns whether a shell-job continuation turn may open now.
    /// 返回当前是否允许开启 shell 任务续跑回合。
    ///
    /// Only an idle, unarchived root session whose user has not stopped it qualifies; queued user
    /// input, running auxiliary questions and child sessions keep their own owners.
    /// 仅限空闲、未归档且未被用户停止的根会话；排队用户输入、运行中的辅助问答与子会话
    /// 保留各自的所有者。
    pub fn shell_job_continuation_admissible(&self) -> bool {
        self.active_turn.as_ref().is_none_or(|turn| turn.closed)
            && self.active_api.is_none()
            && self.metadata.archived_at_ms.is_none()
            && self.metadata.child_execution_profile.is_none()
            && self.metadata.input_state.queued_inputs.is_empty()
            && self.metadata.input_state.pending_in_turn_inputs.is_empty()
            && matches!(
                self.collaboration_run_control(),
                CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle
            )
            && !self
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| execution.state == CollaborationQuestionExecutionState::Running)
    }

    /// Opens one shell-job continuation turn and grants it run permission in the same revision.
    /// 开启一个 shell 任务续跑回合，并在同一修订中授予运行许可。
    ///
    /// # Parameters
    /// 参数
    /// `content` is the model-facing start semantic; `now_ms` is the explicit mutation time.
    /// `content` 是面向模型的起点语义；`now_ms` 是明确的修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns the new turn identity. An open turn or an inadmissible session is an error, so
    /// replaying the call can never open a second turn.
    /// 返回新回合身份。存在开放回合或会话不允许时返回错误，因此重放调用不会开启第二个回合。
    pub fn start_shell_job_continuation_turn(
        &mut self,
        content: impl Into<String>,
        now_ms: i64,
    ) -> Result<ShellJobContinuationStart, DomainError> {
        if let Some(turn) = self.active_turn.as_ref().filter(|turn| !turn.closed) {
            return Err(DomainError::ShellJobContinuationTurnOpen {
                turn_id: turn.turn_id.value(),
            });
        }
        if !self.shell_job_continuation_admissible() {
            return Err(DomainError::CollaborationStateViolation {
                field: "shell_job_continuation.owner",
            });
        }
        let content = validate_system_context_content(content.into())?;
        self.apply_atomic_transition(move |document| {
            document.ensure_transition_capacity(1, 1)?;
            let mut staged_allocator = document.id_allocator;
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let mut staged_events = Vec::with_capacity(1);
            // A turn-scoped context expires with this turn, so later turns and compactions never resend it.
            // 回合作用域上下文随本回合失效，后续回合与压缩不会再次发送。
            let context_seq = document.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::SystemContext(SystemContextEvent {
                    context_key: SHELL_JOB_CONTINUATION_CONTEXT_KEY.to_string(),
                    operation: SystemContextOperation::Upsert,
                    scope: SystemContextScope::Turn,
                    ttl_turns: Some(1),
                    provider_role_hint: None,
                    content,
                }),
                now_ms,
            )?;
            document.id_allocator = staged_allocator;
            document.events.extend(staged_events);
            document.active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::ShellJobContinuation,
                start_seq: context_seq,
                closed: false,
            });
            document.metadata.open_turn_start_seq = Some(context_seq);
            document.bump_revision(
                &[DirtyFile::Metadata, DirtyFile::ActiveTail],
                now_ms,
                &[context_seq],
            );
            // Natural idle does not authorize business execution; the wake publishes it with the turn.
            // 自然空闲不授权业务执行；唤醒与回合一起发布运行许可。
            if document.collaboration_run_control() != CollaborationRunControl::Running {
                document.set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "shell-job-continuation:{}:{}:{}",
                        document.metadata.session_id,
                        document.segment_generation(),
                        context_seq.value()
                    ))?,
                    state: CollaborationRunControl::Running,
                    changed_at_ms: now_ms,
                })?;
            }
            Ok(ShellJobContinuationStart {
                turn_id,
                context_seq,
            })
        })
    }

    /// Returns whether `turn_id` is the open shell-job continuation turn that no business request has served yet.
    /// 返回 `turn_id` 是否为尚未由任何业务请求处理的开放 shell 任务续跑回合。
    pub fn shell_job_continuation_undispatched(&self, turn_id: TurnId) -> bool {
        self.active_turn.as_ref().is_some_and(|turn| {
            !turn.closed
                && turn.turn_id == turn_id
                && turn.origin == TurnOrigin::ShellJobContinuation
        }) && self.active_api.is_none()
            && !self.events.iter().any(|event| {
                event.turn_id == Some(turn_id)
                    && matches!(&event.event, EventBody::ApiStarted(started)
                        if !matches!(started.purpose,
                            ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
            })
    }

    /// Stops one shell-job continuation turn whose dispatch never started.
    /// 停止一个从未开始派发的 shell 任务续跑回合。
    ///
    /// # Returns
    /// 返回值
    /// Returns false without changes when the turn is closed, different, or already dispatched.
    /// 回合已闭合、不是同一回合或已经派发时返回 false 且不做修改。
    pub fn stop_undispatched_shell_job_continuation_turn(
        &mut self,
        turn_id: TurnId,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        if !self.shell_job_continuation_undispatched(turn_id) {
            return Ok(false);
        }
        self.close_turn(TurnOutcome::Stopped, 0, now_ms)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::document::ClientInputSubmission;
    use crate::v1::ids::ClientActionId;

    /// Builds an empty idle root session.
    /// 构建一个空闲的空根会话。
    fn idle_document() -> SessionDocument {
        SessionDocument::new("shell-job-wake", "project", "title", 1)
    }

    /// The turn opens once with the shell-job origin and run permission; a second call is refused.
    /// 回合以 shell 任务来源及运行许可开启一次；第二次调用被拒绝。
    #[test]
    fn opens_exactly_once_and_grants_run_permission() {
        let mut document = idle_document();
        assert!(document.shell_job_continuation_admissible());
        let start = document
            .start_shell_job_continuation_turn("jobs finished", 2)
            .expect("an idle session opens the continuation turn");
        let turn = document.active_turn().expect("the turn is open").clone();
        assert_eq!(turn.turn_id, start.turn_id);
        assert_eq!(turn.origin, TurnOrigin::ShellJobContinuation);
        assert_eq!(turn.start_seq, start.context_seq);
        assert_eq!(
            document.metadata.open_turn_start_seq,
            Some(start.context_seq)
        );
        assert_eq!(
            document.collaboration_run_control(),
            CollaborationRunControl::Running
        );
        assert!(document.shell_job_continuation_undispatched(start.turn_id));
        assert!(!document.shell_job_continuation_admissible());
        assert!(matches!(
            document.start_shell_job_continuation_turn("jobs finished", 3),
            Err(DomainError::ShellJobContinuationTurnOpen { turn_id }) if turn_id == start.turn_id.value()
        ));
    }

    /// A user turn that is still open keeps ownership.
    /// 仍开放的用户回合保留所有权。
    #[test]
    fn refuses_while_a_user_turn_is_open() {
        let mut document = idle_document();
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root").unwrap(),
                    "user prompt",
                    Vec::new(),
                ),
                2,
            )
            .unwrap();
        assert!(!document.shell_job_continuation_admissible());
        assert!(matches!(
            document.start_shell_job_continuation_turn("jobs finished", 3),
            Err(DomainError::ShellJobContinuationTurnOpen { .. })
        ));
    }

    /// An archived or user-stopped session is never woken.
    /// 已归档或被用户停止的会话不会被唤醒。
    #[test]
    fn refuses_archived_and_stopped_sessions() {
        let mut archived = idle_document();
        archived.metadata.archived_at_ms = Some(2);
        assert!(
            archived
                .start_shell_job_continuation_turn("jobs finished", 3)
                .is_err()
        );
        assert!(archived.active_turn().is_none());

        let mut stopped = idle_document();
        stopped
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("user-stop").unwrap(),
                state: CollaborationRunControl::MainOnly,
                changed_at_ms: 2,
            })
            .unwrap();
        assert!(!stopped.shell_job_continuation_admissible());
        assert!(
            stopped
                .start_shell_job_continuation_turn("jobs finished", 3)
                .is_err()
        );
    }

    /// Rebuilding derived state from the event stream recovers the open turn with its origin.
    /// 从事件流重建派生状态时恢复开放回合及其来源。
    #[test]
    fn rebuild_recovers_the_open_turn_origin() {
        let mut document = idle_document();
        let start = document
            .start_shell_job_continuation_turn("jobs finished", 2)
            .unwrap();
        let expected = document.active_turn().cloned();
        document.active_turn = None;
        document.rebuild_derived_state().unwrap();
        assert_eq!(document.active_turn().cloned(), expected);
        assert_eq!(
            document.active_turn().unwrap().origin,
            TurnOrigin::ShellJobContinuation
        );
        assert_eq!(document.active_turn().unwrap().turn_id, start.turn_id);
    }

    /// An undispatched turn stops once and its turn-scoped start context expires with it.
    /// 未派发回合只停止一次，其回合作用域起点上下文随之失效。
    #[test]
    fn undispatched_turn_stops_once_and_expires_its_context() {
        let mut document = idle_document();
        let start = document
            .start_shell_job_continuation_turn("jobs finished", 2)
            .unwrap();
        assert!(
            document
                .stop_undispatched_shell_job_continuation_turn(start.turn_id, 3)
                .unwrap()
        );
        assert!(document.active_turn().is_none_or(|turn| turn.closed));
        assert!(
            !document
                .stop_undispatched_shell_job_continuation_turn(start.turn_id, 4)
                .unwrap()
        );
        assert!(document.events().iter().any(|event| matches!(
            &event.event,
            EventBody::SystemContext(context)
                if context.context_key == SHELL_JOB_CONTINUATION_CONTEXT_KEY
                    && context.operation == SystemContextOperation::Remove
        )));
        // The closed session may be woken again by later jobs.
        // 已闭合会话可被之后的任务再次唤醒。
        assert!(document.shell_job_continuation_admissible());
    }

    /// A process restart closes an open continuation turn instead of leaving it waiting forever.
    /// 进程重启会闭合开放的续跑回合，而不是让其永远等待。
    #[test]
    fn restart_projection_closes_the_open_turn() {
        let mut document = idle_document();
        document
            .start_shell_job_continuation_turn("jobs finished", 2)
            .unwrap();
        assert!(document.project_persisted_interruptions().unwrap() > 0);
        assert!(document.active_turn().is_none_or(|turn| turn.closed));
        assert_eq!(document.project_persisted_interruptions().unwrap(), 0);
    }

    /// A queued user input supersedes the undispatched turn and opens its own turn.
    /// 排队用户输入取代未派发回合并开启自己的回合。
    #[test]
    fn user_input_supersedes_the_undispatched_turn() {
        let mut document = idle_document();
        let start = document
            .start_shell_job_continuation_turn("jobs finished", 2)
            .unwrap();
        let receipt = document
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("queued").unwrap(),
                    "user prompt",
                    Vec::new(),
                ),
                3,
            )
            .unwrap()
            .into_value();
        assert!(matches!(
            receipt.disposition,
            crate::v1::document::EnqueueInputDisposition::SupersededContinuationAndScheduled {
                stopped_turn_id,
                ..
            } if stopped_turn_id == start.turn_id
        ));
        assert_eq!(document.active_turn().unwrap().origin, TurnOrigin::UserRoot);
    }
}
