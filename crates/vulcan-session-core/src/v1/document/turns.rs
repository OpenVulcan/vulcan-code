//! Logical turn continuation, closure, statistics, and deterministic successor selection.
//! 逻辑回合继续、闭合、统计及确定性后继选择。

use super::input::{
    ClientInputSubmission, consume_matching_draft, event_client_action_id, validate_user_input,
};
use super::modes::goal_notice_event;
use super::persistence::DirtyFile;
use super::{
    ActiveTurnState, IdempotentMutation, SessionDocument, TurnOrigin, client_action_conflict,
    validate_system_context_content, validate_timestamp,
};
use crate::v1::config::{GoalBudgetCache, GoalState};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiCallStatus, ApiStatsEvent, AssistantSemanticStatus, ContinuationEligibility,
    ConversationEvent, EventBody, EventVisibility, ExecutionControlEvent, ExecutionControlKind,
    SubagentReturnStatus, SystemContextEvent, SystemContextOperation, SystemContextScope,
    SystemRoleHint, TurnBoundaryEvent, TurnOutcome, TurnStatsEvent, UserInputEvent,
    UserInputSource,
};
use crate::v1::goal::{GoalStopReason, decide_goal_continuation};
use crate::v1::ids::{ClientActionId, IdAllocator, PendingInputId, ResourceId, Seq, TurnId};
use crate::v1::stats::{StatBucket, TurnStatBuckets};
use std::collections::BTreeSet;

/// One explicit request to resume the current unfinished logical Turn.
/// 一次恢复当前未完成逻辑回合的显式请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationRequest {
    /// Client submission and optional supplementary user payload.
    /// 客户端提交身份与可选补充用户载荷。
    pub submission: ClientInputSubmission,
    /// Optional turn-scoped developer guidance admitted in the same durable mutation.
    /// 在同一可靠修改中准入的可选回合级 developer 引导。
    pub hidden_model_guidance: Option<String>,
}

impl ContinuationRequest {
    /// Builds one strongly typed continuation request.
    /// 构建一条强类型继续请求。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` is the exact external idempotency key; empty text
    /// plus no resources means a pure Continue without an added user input.
    /// `client_action_id` 是精确的外部幂等键；空正文且无资源表示不附加用户输入的纯继续。
    ///
    /// # Returns
    /// 返回值
    /// Returns the immutable continuation request.
    /// 返回不可变的继续请求。
    pub fn new(
        client_action_id: ClientActionId,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
    ) -> Self {
        Self {
            submission: ClientInputSubmission::new(client_action_id, text, resource_ids),
            hidden_model_guidance: None,
        }
    }

    /// Attaches one explicit turn-scoped developer guidance payload.
    /// 附加一条显式的回合级 developer 引导载荷。
    ///
    /// # Parameters
    /// 参数
    /// `guidance` is the exact dynamic system semantic to persist with Continue admission.
    /// `guidance` 是与 Continue 准入一同持久化的精确动态系统语义。
    ///
    /// # Returns
    /// 返回值
    /// Returns the updated immutable request value.
    /// 返回更新后的不可变请求值。
    pub fn with_hidden_model_guidance(mut self, guidance: Option<String>) -> Self {
        self.hidden_model_guidance = guidance;
        self
    }
}

/// Durable receipt reconstructed from one continuation action's events.
/// 从一次继续动作的事件中重建的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinueTurnReceipt {
    /// Open logical Turn resumed by this action.
    /// 本动作恢复的开放逻辑回合。
    pub turn_id: TurnId,
    /// Optional local input identity when the action supplied new content.
    /// 本动作提供新内容时的可选本地输入身份。
    pub input_id: Option<PendingInputId>,
    /// Optional Provider-visible user-input seq.
    /// 可选的 Provider 可见用户输入 seq。
    pub input_seq: Option<Seq>,
    /// Optional seq of the turn-scoped developer guidance admitted with this action.
    /// 随本动作准入的可选回合级 developer 引导 seq。
    pub guidance_seq: Option<Seq>,
    /// Host-only seq recording the Continue request.
    /// 记录继续请求的 Host-only seq。
    pub requested_seq: Seq,
    /// Host-only seq recording that generation resumed.
    /// 记录生成已恢复的 Host-only seq。
    pub resumed_seq: Seq,
}

/// The unique successor selected while atomically completing one Turn.
/// 原子完成一个回合时选定的唯一后继。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnSuccessor {
    /// No successor Turn was opened.
    /// 未开启后继回合。
    None,
    /// The FIFO head queued user input opened the next UserRoot Turn.
    /// FIFO 队首用户输入开启了下一个 UserRoot 回合。
    QueuedUser {
        /// Newly opened Turn identity.
        /// 新开启的回合身份。
        turn_id: TurnId,
        /// Reused queued input identity.
        /// 复用的排队输入身份。
        input_id: PendingInputId,
        /// Reused external client action identity.
        /// 复用的外部客户端动作身份。
        client_action_id: ClientActionId,
        /// Provider-visible seq of the scheduled input.
        /// 已调度输入的 Provider 可见 seq。
        input_seq: Seq,
    },
    /// An active Goal opened the next automatic continuation Turn.
    /// 活动 Goal 开启了下一个自动续跑回合。
    GoalContinuation {
        /// Newly opened Goal Turn identity.
        /// 新开启的 Goal 回合身份。
        turn_id: TurnId,
        /// Provider-visible goal-context seq.
        /// Provider 可见 Goal 上下文 seq。
        context_seq: Seq,
    },
}

/// Result of atomically completing one turn and selecting its unique successor.
/// 原子完成一个回合并选择唯一后继的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCompletionTransition {
    /// Final statistics seq of the completed turn.
    /// 已完成回合的最终统计 seq。
    pub completed_turn_final_seq: Seq,
    /// Unique successor selected by queued-user-first policy.
    /// 按排队用户优先策略选定的唯一后继。
    pub successor: TurnSuccessor,
}

/// Builds the stable semantic key owned by one Continue guidance action.
/// 构建由一次 Continue 引导动作拥有的稳定语义键。
fn continuation_guidance_key(client_action_id: &ClientActionId) -> String {
    format!("continuation_guidance/{}", client_action_id.as_str())
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Atomically records one idempotent Continue action on an unfinished Turn.
    /// 在一个未完成回合上原子记录一次幂等继续动作。
    ///
    /// # Parameters
    /// 参数
    /// `request` carries the external idempotency key and optional supplementary
    /// user payload. `now_ms` is the explicit semantic-boundary timestamp.
    /// `request` 携带外部幂等键与可选补充用户载荷；`now_ms` 是显式语义边界时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Returns a receipt reconstructible from durable events. An exact retry
    /// returns the original receipt without advancing the document revision.
    /// 返回可从持久化事件重建的 receipt；精确重试返回原 receipt 且不推进文档修订。
    pub fn continue_open_turn(
        &mut self,
        request: ContinuationRequest,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ContinueTurnReceipt>, DomainError> {
        if let Some(receipt) = self.existing_continuation_receipt(&request)? {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        validate_timestamp("continue_open_turn.now_ms", now_ms)?;
        let turn_id = self.require_open_turn()?.turn_id;
        self.require_client_action_unused(&request.submission.client_action_id)?;
        if self.active_api.is_some() {
            return Err(DomainError::ContinuationNotEligible {
                reason: "active_api",
            });
        }
        self.validate_continuation_eligibility(turn_id)?;

        let ContinuationRequest {
            submission,
            hidden_model_guidance,
        } = request;
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        let has_input = !text.is_empty() || !resource_ids.is_empty();
        if has_input {
            validate_user_input(text.clone(), &resource_ids)?;
        }
        let hidden_model_guidance = hidden_model_guidance
            .map(validate_system_context_content)
            .transpose()?;
        let event_count = 2 + u64::from(has_input) + u64::from(hidden_model_guidance.is_some());
        let local_id_count = u64::from(has_input);
        self.ensure_transition_capacity(event_count, local_id_count)?;

        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(event_count as usize);
        let mut staged_metadata = self.metadata.clone();
        let draft_consumed =
            has_input && consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        let (input_id, input_seq) = if has_input {
            let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: client_action_id.clone(),
                    input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text,
                    resource_ids,
                }),
                now_ms,
            )?;
            (Some(input_id), Some(input_seq))
        } else {
            (None, None)
        };
        let guidance_seq = if let Some(content) = hidden_model_guidance {
            Some(self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::SystemContext(SystemContextEvent {
                    context_key: continuation_guidance_key(&client_action_id),
                    operation: SystemContextOperation::Upsert,
                    scope: SystemContextScope::Turn,
                    ttl_turns: Some(1),
                    provider_role_hint: Some(SystemRoleHint::Developer),
                    content,
                }),
                now_ms,
            )?)
        } else {
            None
        };
        let requested_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::TurnContinueRequested,
                api_call_id: None,
                client_action_id: Some(client_action_id.clone()),
            }),
            now_ms,
        )?;
        let resumed_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::TurnResumed,
                api_call_id: None,
                client_action_id: Some(client_action_id),
            }),
            now_ms,
        )?;
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        let dirty_files: &[DirtyFile] = if draft_consumed {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        } else {
            &[DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, now_ms, &changed_event_seqs);
        Ok(IdempotentMutation::Committed(ContinueTurnReceipt {
            turn_id,
            input_id,
            input_seq,
            guidance_seq,
            requested_seq,
            resumed_seq,
        }))
    }

    /// Reconstructs an existing continuation receipt before current eligibility checks.
    /// 在当前资格检查之前重建一份既有继续 receipt。
    fn existing_continuation_receipt(
        &self,
        request: &ContinuationRequest,
    ) -> Result<Option<ContinueTurnReceipt>, DomainError> {
        let action_id = &request.submission.client_action_id;
        let guidance_key = continuation_guidance_key(action_id);
        let expected_guidance = request
            .hidden_model_guidance
            .clone()
            .map(validate_system_context_content)
            .transpose()?;
        let mut input: Option<(&ConversationEvent, &UserInputEvent)> = None;
        let mut guidance: Option<(&ConversationEvent, &SystemContextEvent)> = None;
        let mut requested: Option<&ConversationEvent> = None;
        let mut resumed: Option<&ConversationEvent> = None;
        for event in &self.events {
            if let EventBody::SystemContext(context) = &event.event
                && context.context_key == guidance_key
            {
                if guidance.replace((event, context)).is_some() {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "duplicate_continue_guidance",
                    });
                }
                continue;
            }
            if event_client_action_id(event) != Some(action_id) {
                continue;
            }
            match &event.event {
                EventBody::UserInput(value) => {
                    if input.replace((event, value)).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_user_input",
                        });
                    }
                }
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnContinueRequested =>
                {
                    if requested.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_continue_requested",
                        });
                    }
                }
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnResumed =>
                {
                    if resumed.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_turn_resumed",
                        });
                    }
                }
                _ => return Err(client_action_conflict(action_id)),
            }
        }
        if requested.is_none() && resumed.is_none() {
            return if input.is_some() {
                Err(client_action_conflict(action_id))
            } else {
                Ok(None)
            };
        }
        let requested = requested.ok_or(DomainError::InvalidContinuationHistory {
            field: "missing_continue_requested",
        })?;
        let resumed = resumed.ok_or(DomainError::InvalidContinuationHistory {
            field: "missing_turn_resumed",
        })?;
        let turn_id = requested
            .turn_id
            .ok_or(DomainError::InvalidContinuationHistory {
                field: "continue_requested.turn_id",
            })?;
        if resumed.turn_id != Some(turn_id) || requested.seq.value() >= resumed.seq.value() {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_pair_order",
            });
        }
        let requested_has_input =
            !request.submission.text.is_empty() || !request.submission.resource_ids.is_empty();
        let (input_id, input_seq) = match input {
            Some((event, value)) => {
                if !requested_has_input
                    || event.turn_id != Some(turn_id)
                    || event.seq.value() >= requested.seq.value()
                    || value.input_source != UserInputSource::PendingInTurn
                    || value.text != request.submission.text
                    || value.resource_ids != request.submission.resource_ids
                {
                    return Err(client_action_conflict(action_id));
                }
                (Some(value.input_id), Some(event.seq))
            }
            None if requested_has_input => return Err(client_action_conflict(action_id)),
            None => (None, None),
        };
        let guidance_seq = match (guidance, expected_guidance) {
            (Some((event, context)), Some(expected_content))
                if event.turn_id == Some(turn_id)
                    && event.seq.value() < requested.seq.value()
                    && context.operation == SystemContextOperation::Upsert
                    && context.scope == SystemContextScope::Turn
                    && context.ttl_turns == Some(1)
                    && context.provider_role_hint == Some(SystemRoleHint::Developer)
                    && context.content == expected_content =>
            {
                Some(event.seq)
            }
            (None, None) => None,
            _ => return Err(client_action_conflict(action_id)),
        };
        Ok(Some(ContinueTurnReceipt {
            turn_id,
            input_id,
            input_seq,
            guidance_seq,
            requested_seq: requested.seq,
            resumed_seq: resumed.seq,
        }))
    }

    /// Validates that the latest physical API terminal permits an explicit Continue.
    /// 校验最近一次物理 API 终态允许显式继续。
    fn validate_continuation_eligibility(&self, turn_id: TurnId) -> Result<(), DomainError> {
        let latest_started = self
            .events
            .iter()
            .rev()
            .find(|event| {
                event.turn_id == Some(turn_id) && matches!(event.event, EventBody::ApiStarted(_))
            })
            .ok_or(DomainError::ContinuationNotEligible {
                reason: "missing_api_started",
            })?;
        let EventBody::ApiStarted(started) = &latest_started.event else {
            unreachable!("latest ApiStarted selected above")
        };
        let mut terminal: Option<(&ConversationEvent, &ApiStatsEvent)> = None;
        for event in self
            .events
            .iter()
            .filter(|event| event.turn_id == Some(turn_id))
        {
            if let EventBody::ApiStats(stats) = &event.event
                && stats.api_call_id == started.api_call_id
                && terminal.replace((event, stats)).is_some()
            {
                return Err(DomainError::ContinuationNotEligible {
                    reason: "duplicate_api_terminal",
                });
            }
        }
        let (terminal_event, terminal) = terminal.ok_or(DomainError::ContinuationNotEligible {
            reason: "missing_api_terminal",
        })?;
        if terminal_event.seq.value() <= latest_started.seq.value() {
            return Err(DomainError::ContinuationNotEligible {
                reason: "api_terminal_order",
            });
        }
        if self.events.iter().any(|event| {
            event.turn_id == Some(turn_id)
                && event.seq.value() > terminal_event.seq.value()
                && matches!(
                    event.event,
                    EventBody::ApiStarted(_)
                        | EventBody::TurnBoundary(_)
                        | EventBody::TurnStats(_)
                        | EventBody::ExecutionControl(ExecutionControlEvent {
                            control_kind: ExecutionControlKind::TurnResumed,
                            ..
                        })
                )
        }) {
            return Err(DomainError::ContinuationNotEligible {
                reason: "terminal_boundary_superseded",
            });
        }
        if terminal.continuation_eligibility == ContinuationEligibility::Allowed {
            Ok(())
        } else {
            Err(DomainError::ContinuationNotEligible {
                reason: "terminal_policy_forbidden",
            })
        }
    }

    /// Closes the open turn with one boundary followed by the unique terminal
    /// statistic, both committed by the same flush.
    /// 用一个边界与唯一终态统计闭合开放回合，两者由同一次 flush 提交。
    pub fn close_turn(
        &mut self,
        outcome: TurnOutcome,
        duration_ms: i64,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        let turn = self.require_open_turn()?;
        if let Some(api) = self.active_api.as_ref() {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: turn.turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        let turn_id = turn.turn_id;
        let start_seq = turn.start_seq;
        self.ensure_transition_capacity(2, 0)?;
        let buckets = self.aggregate_turn_buckets(turn_id, start_seq)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let final_seq = self.stage_turn_closure(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            start_seq,
            outcome,
            duration_ms,
            now_ms,
            buckets,
        )?;
        let mut staged_active_turn = turn.clone();
        staged_active_turn.closed = true;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.open_turn_start_seq = None;
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.active_turn = Some(staged_active_turn);
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(final_seq)
    }

    /// Stages one complete boundary/statistics pair without publishing owner state.
    /// 暂存一组完整的边界/统计事件，不发布 owner 状态。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn stage_turn_closure(
        &self,
        staged_allocator: &mut IdAllocator,
        staged_events: &mut Vec<ConversationEvent>,
        turn_id: TurnId,
        start_seq: Seq,
        outcome: TurnOutcome,
        duration_ms: i64,
        now_ms: i64,
        buckets: TurnStatBuckets,
    ) -> Result<Seq, DomainError> {
        self.stage_event(
            staged_allocator,
            staged_events,
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::TurnBoundary(TurnBoundaryEvent { outcome }),
            now_ms,
        )?;
        // The boundary event just took the last seq; the turn_stats event will
        // take exactly the next one, so the covered end seq includes the
        // statistics event itself (plan 12.21.2 "覆盖的起止 seq").
        // 边界事件刚刚取得最后一个 seq；turn_stats 事件将精确取得下一个，因此
        // 覆盖结束 seq 包含统计事件自身(计划 12.21.2"覆盖的起止 seq")。
        let end_seq = Seq::try_new(staged_allocator.frontier().1)?;
        self.stage_event(
            staged_allocator,
            staged_events,
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::TurnStats(TurnStatsEvent {
                start_seq,
                end_seq,
                outcome,
                duration_ms: Some(duration_ms),
                buckets,
            }),
            now_ms,
        )?;
        Ok(end_seq)
    }

    /// Completes the expected open turn and atomically selects its sole queued, Goal, or absent successor.
    /// 完成指定开放回合，并原子选择唯一的排队输入、Goal 或无后继结果。
    ///
    /// `expected_turn_id` is the receipt-free exactly-once key: after a
    /// successful transition the current open turn is either absent or has a
    /// different id, so replaying the same transition cannot close or create a
    /// second turn. All seq/id capacity and ownership checks run before the
    /// first event is appended.
    /// `expected_turn_id` 是无 receipt 的 exactly-once 键：转换成功后，当前开放
    /// 回合要么不存在，要么具有不同 id，因此重放同一转换不能再次闭合或创建
    /// 回合。所有 seq/id 容量与归属校验都在追加首个事件前完成。
    pub fn complete_turn_and_select_successor(
        &mut self,
        expected_turn_id: TurnId,
        now_ms: i64,
    ) -> Result<TurnCompletionTransition, DomainError> {
        let turn = self.require_open_turn()?.clone();
        if turn.turn_id != expected_turn_id {
            return Err(DomainError::TurnIdentityMismatch {
                expected: expected_turn_id.value(),
                actual: turn.turn_id.value(),
            });
        }
        if let Some(api) = self.active_api.as_ref() {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: expected_turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        self.require_final_assistant(expected_turn_id)?;
        let turn_start_event = self
            .events
            .iter()
            .find(|event| event.seq == turn.start_seq)
            .ok_or(DomainError::ReferenceViolation {
                kind: "turn_start_event",
                value: turn.start_seq.value(),
            })?;
        if turn_start_event.turn_id != Some(expected_turn_id) {
            return Err(DomainError::TurnIdentityMismatch {
                expected: expected_turn_id.value(),
                actual: turn_start_event.turn_id.map_or(0, TurnId::value),
            });
        }
        // Logical Turn duration is derived from the authoritative first event so continuation,
        // process restart, and multiple physical APIs never reset the elapsed interval.
        // 逻辑回合耗时从权威首事件推导，使继续、进程重启和多个物理 API 永不重置耗时区间。
        let duration_ms = now_ms.saturating_sub(turn_start_event.created_at_ms).max(0);

        let queued_successor = self.metadata.input_state.queued_inputs.first().cloned();
        if let Some(queued) = queued_successor.as_ref() {
            validate_user_input(queued.text.clone(), &queued.resource_ids)?;
        }

        let goal_budget = self.aggregate_goal_budget(now_ms)?;
        let goal_stats = TurnStatsEvent {
            start_seq: self.metadata.mode.goal.origin_seq.unwrap_or(turn.start_seq),
            end_seq: self.last_seq()?,
            outcome: TurnOutcome::Completed,
            duration_ms: None,
            buckets: goal_budget
                .as_ref()
                .map(|(_, buckets)| buckets.clone())
                .unwrap_or_default(),
        };
        let decision = decide_goal_continuation(&self.metadata.mode.goal, &goal_stats);
        let next_goal_state = match decision.reason {
            GoalStopReason::BudgetExhausted { .. } => Some(GoalState::BudgetExhausted),
            GoalStopReason::UnknownUsage => Some(GoalState::Blocked),
            _ => None,
        };
        let goal_objective = (queued_successor.is_none() && decision.should_continue)
            .then(|| self.metadata.mode.goal.objective.clone());
        let has_successor = queued_successor.is_some() || goal_objective.is_some();
        let appended_event_count =
            if has_successor { 3 } else { 2 } + u64::from(next_goal_state.is_some());
        self.ensure_transition_capacity(appended_event_count, u64::from(has_successor))?;
        let buckets = self.aggregate_turn_buckets(expected_turn_id, turn.start_seq)?;
        // Stage the boundary, the unique turn statistics, and the optional successor before
        // publishing any owner state. This keeps final-assistant completion exactly-once and
        // failure-atomic even at allocator exhaustion.
        // 在发布任何 owner 状态前暂存边界、唯一回合统计及可选后继，使最终 assistant
        // 完成即使遇到分配器耗尽也保持 exactly-once 与失败原子性。
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(appended_event_count as usize);
        let completed_turn_final_seq = self.stage_turn_closure(
            &mut staged_allocator,
            &mut staged_events,
            expected_turn_id,
            turn.start_seq,
            TurnOutcome::Completed,
            duration_ms,
            now_ms,
            buckets,
        )?;
        let mut staged_metadata = self.metadata.clone();
        let mut completed_turn = turn;
        completed_turn.closed = true;
        let mut staged_active_turn = Some(completed_turn);
        if staged_metadata.mode.goal.enabled {
            staged_metadata.mode.goal.budget_cache =
                goal_budget.map(|(cache, _)| GoalBudgetCache {
                    covered_until_seq: completed_turn_final_seq,
                    ..cache
                });
            if let Some(state) = next_goal_state {
                staged_metadata.mode.goal.state = state;
                // Persist the stop boundary with the decision so working time can be replayed after restart.
                // 将停止边界与决策一起持久化，使重启后可重放工作时长。
                self.stage_event(
                    &mut staged_allocator,
                    &mut staged_events,
                    None,
                    EventVisibility::UiOnly,
                    goal_notice_event(if state == GoalState::BudgetExhausted {
                        "goal_budget_exhausted"
                    } else {
                        "goal_usage_unknown"
                    }),
                    now_ms,
                )?;
            }
        }
        let successor = if let Some(queued) = queued_successor {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: queued.client_action_id.clone(),
                    input_id: queued.input_id,
                    input_source: UserInputSource::QueuedScheduled,
                    text: queued.text,
                    resource_ids: queued.resource_ids,
                }),
                now_ms,
            )?;
            staged_metadata.input_state.queued_inputs.remove(0);
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::UserRoot,
                start_seq: input_seq,
                closed: false,
            });
            staged_metadata.open_turn_start_seq = Some(input_seq);
            TurnSuccessor::QueuedUser {
                turn_id,
                input_id: queued.input_id,
                client_action_id: queued.client_action_id,
                input_seq,
            }
        } else if let Some(objective) = goal_objective {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let start_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::SystemContext(SystemContextEvent {
                    context_key: "goal_state".to_string(),
                    operation: SystemContextOperation::Upsert,
                    scope: SystemContextScope::Session,
                    ttl_turns: None,
                    provider_role_hint: None,
                    content: objective,
                }),
                now_ms,
            )?;
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq,
                closed: false,
            });
            staged_metadata.open_turn_start_seq = Some(start_seq);
            TurnSuccessor::GoalContinuation {
                turn_id,
                context_seq: start_seq,
            }
        } else {
            staged_metadata.open_turn_start_seq = None;
            TurnSuccessor::None
        };
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.active_turn = staged_active_turn;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(TurnCompletionTransition {
            completed_turn_final_seq,
            successor,
        })
    }

    /// Requires the latest physical API of one turn to carry final assistant text.
    /// 要求一个回合的最新物理 API 携带最终 assistant 正文。
    fn require_final_assistant(&self, turn_id: TurnId) -> Result<(), DomainError> {
        let latest_stats = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStats(stats) if event.turn_id == Some(turn_id) => {
                    Some((event.seq, stats))
                }
                _ => None,
            })
            .max_by_key(|(seq, _)| seq.value())
            .map(|(_, stats)| stats)
            .ok_or(DomainError::TurnMissingFinalAssistant {
                turn_id: turn_id.value(),
            })?;
        let api_call_id = latest_stats.api_call_id;
        let has_final_text = latest_stats.status == ApiCallStatus::Completed
            && self.events.iter().any(|event| {
                matches!(
                    &event.event,
                    EventBody::AssistantText(text)
                        if text.api_call_id == api_call_id
                            && text.status == AssistantSemanticStatus::Completed
                )
            });
        let has_tool_calls = self.events.iter().any(|event| {
            matches!(
                &event.event,
                EventBody::ToolCall(tool) if tool.api_call_id == api_call_id
            )
        });
        if !has_final_text || has_tool_calls {
            return Err(DomainError::TurnMissingFinalAssistant {
                turn_id: turn_id.value(),
            });
        }
        Ok(())
    }

    /// Aggregates the direct/subagent buckets of one active turn from its resident suffix.
    /// 从活动回合的常驻后缀聚合 direct/subagent 桶。
    ///
    /// `start_seq` is the authoritative first event of `turn_id`. Restricting
    /// aggregation to that suffix prevents every completed turn from rescanning
    /// the immutable history while preserving exact filtering for session-scoped
    /// events that may be interleaved with the active turn.
    /// `start_seq` 是 `turn_id` 的权威首事件。把聚合限制到该后缀，可避免每个已完成
    /// 回合重复扫描不可变历史，同时仍精确过滤可能夹在活动回合中的会话级事件。
    pub(super) fn aggregate_turn_buckets(
        &self,
        turn_id: TurnId,
        start_seq: Seq,
    ) -> Result<TurnStatBuckets, DomainError> {
        let start_index = start_seq
            .value()
            .checked_sub(self.resident_start_seq)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(DomainError::ReferenceViolation {
                kind: "turn_start_event",
                value: start_seq.value(),
            })?;
        let turn_events =
            self.events
                .get(start_index..)
                .ok_or(DomainError::ReferenceViolation {
                    kind: "turn_start_event",
                    value: start_seq.value(),
                })?;
        let first_event = turn_events.first().ok_or(DomainError::ReferenceViolation {
            kind: "turn_start_event",
            value: start_seq.value(),
        })?;
        if first_event.seq != start_seq || first_event.turn_id != Some(turn_id) {
            return Err(DomainError::TurnIdentityMismatch {
                expected: turn_id.value(),
                actual: first_event.turn_id.map_or(0, TurnId::value),
            });
        }
        let mut direct_usages = Vec::new();
        let mut direct_api_count = 0u64;
        let mut direct_tool_count = 0u64;
        let mut subagent_totals = Vec::new();
        let mut counted_subagent_stats = BTreeSet::new();
        for event in turn_events
            .iter()
            .filter(|event| event.turn_id == Some(turn_id))
        {
            match &event.event {
                // Purpose describes why this document called the Provider, not ownership by another document.
                // purpose 描述当前文档调用 Provider 的原因，不代表该调用归属于其他文档。
                EventBody::ApiStats(api) => {
                    direct_api_count += 1;
                    direct_usages.push(&api.usage);
                }
                EventBody::ToolCall(_) => {
                    direct_tool_count += 1;
                }
                EventBody::SubagentReturnStats(return_stats)
                    if return_stats.status == SubagentReturnStatus::Final
                        && counted_subagent_stats.insert((
                            return_stats.child_session_id.clone(),
                            return_stats.child_turn_stats_seq,
                        )) =>
                {
                    subagent_totals.push(&return_stats.totals);
                }
                _ => {}
            }
        }
        let direct =
            StatBucket::from_api_usages(direct_usages, direct_api_count, direct_tool_count, 0);
        let subagent = StatBucket::from_subagent_returns(subagent_totals);
        Ok(TurnStatBuckets::combine(direct, subagent))
    }
}
