//! Atomic Goal and Plan mode transitions with their typed inputs and receipts.
//! 目标与计划模式的原子转换及其强类型输入和回执。

use super::persistence::DirtyFile;
use super::{ActiveTurnState, SessionDocument, TurnOrigin, validate_system_context_content};
use crate::v1::config::{GoalConfig, GoalState, MAX_GOAL_OBJECTIVE_CHARS, MetadataDocument};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiPurposeKind, ConversationEvent, DisplayNoteEvent, DisplayNoticeKind, EventBody,
    EventVisibility, NoticeSeverity, SystemContextEvent, SystemContextOperation,
    SystemContextScope, SystemRoleHint, ToolCallStatus,
};
use crate::v1::ids::{IdAllocator, Seq, TurnId};

/// Result of one atomic Goal configuration transition.
/// 一次 Goal 配置原子转换的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalModeTransition {
    /// Provider-visible context seq emitted by the transition, when any.
    /// 转换产生的 Provider 可见上下文 seq（如有）。
    pub context_seq: Option<Seq>,
    /// Permanent UI notice seq emitted by the transition.
    /// 转换产生的永久 UI 通知 seq。
    pub notice_seq: Seq,
    /// Goal continuation turn created by the transition, when any.
    /// 转换创建的 Goal 续跑回合（如有）。
    pub continuation_turn_id: Option<TurnId>,
}

/// Validated input for one atomic Plan-mode transition.
/// 一次原子 Plan 模式转换的已校验输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanModeTransitionInput {
    /// Target Plan enablement.
    /// 目标 Plan 启用状态。
    enabled: bool,
    /// Exact provider-visible enter or exit semantic.
    /// 精确的 Provider 可见进入或退出语义。
    provider_context: String,
}

impl PlanModeTransitionInput {
    /// Builds one Plan transition input with mandatory non-empty semantic content.
    /// 使用必填的非空语义内容构建 Plan 转换输入。
    pub fn try_new(
        enabled: bool,
        provider_context: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            enabled,
            provider_context: validate_system_context_content(provider_context.into())?,
        })
    }
}

/// Validates and normalizes one Goal definition before any document mutation.
/// 在任何文档修改前校验并规范化一份 Goal 定义。
fn validate_goal_definition(
    objective: String,
    budget_tokens: Option<u64>,
    provider_context: String,
) -> Result<(String, String), DomainError> {
    let objective = objective.trim().to_string();
    if objective.is_empty() || objective.chars().count() > MAX_GOAL_OBJECTIVE_CHARS {
        return Err(DomainError::InvalidGoalObjective {
            max_chars: MAX_GOAL_OBJECTIVE_CHARS,
        });
    }
    if budget_tokens == Some(0) {
        return Err(DomainError::InvalidGoalBudget);
    }
    let provider_context = validate_system_context_content(provider_context)?;
    Ok((objective, provider_context))
}

/// Builds one strongly typed Goal system-context event body.
/// 构建一个强类型 Goal 系统上下文事件体。
fn goal_context_event(
    context_key: &str,
    operation: SystemContextOperation,
    content: String,
) -> EventBody {
    EventBody::SystemContext(SystemContextEvent {
        context_key: context_key.to_string(),
        operation,
        scope: SystemContextScope::Session,
        ttl_turns: None,
        provider_role_hint: Some(SystemRoleHint::Developer),
        content,
    })
}

/// Builds one permanent informational Goal timeline notice.
/// 构建一条永久的信息级 Goal 时间线通知。
pub(super) fn goal_notice_event(reason_code: &str) -> EventBody {
    EventBody::DisplayNote(DisplayNoteEvent {
        notice_kind: DisplayNoticeKind::GoalStatus,
        severity: NoticeSeverity::Info,
        reason_code: reason_code.to_string(),
        redacted_detail: None,
        api_call_id: None,
        retry_count: None,
        retry_limit: None,
    })
}

/// Returns the stable diagnostic name of one Goal state.
/// 返回一个 Goal 状态的稳定诊断名。
const fn goal_state_name(state: GoalState) -> &'static str {
    match state {
        GoalState::Active => "active",
        GoalState::Paused => "paused",
        GoalState::Completed => "completed",
        GoalState::Blocked => "blocked",
        GoalState::BudgetExhausted => "budget_exhausted",
    }
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Starts a Goal auto-continue turn from a persisted goal-continuation
    /// system semantic, asserting exactly-once creation.
    /// 从持久化的 goal-continuation 系统语义启动 Goal 自动续跑回合，并保证
    /// 只创建一次。
    pub fn start_goal_continuation_turn(
        &mut self,
        objective: impl Into<String>,
        now_ms: i64,
    ) -> Result<TurnId, DomainError> {
        if let Some(turn) = &self.active_turn
            && !turn.closed
        {
            return Err(DomainError::GoalContinuationAlreadyActive {
                turn_id: turn.turn_id.value(),
            });
        }
        self.ensure_transition_capacity(1, 1)?;
        let mut staged_allocator = self.id_allocator;
        let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::with_capacity(1);
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: "goal_continuation".to_string(),
                operation: SystemContextOperation::Upsert,
                scope: SystemContextScope::Session,
                ttl_turns: None,
                provider_role_hint: None,
                content: objective.into(),
            }),
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::GoalContinuation,
            start_seq: seq,
            closed: false,
        });
        self.metadata.open_turn_start_seq = Some(seq);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[seq],
        );
        Ok(turn_id)
    }

    /// Atomically creates a new active Goal and its first provider-visible semantic.
    /// 原子创建一个新的活动 Goal 及其首条 Provider 可见语义。
    ///
    /// # Parameters
    /// 参数
    /// `objective` and `budget_tokens` are the validated Goal definition, while
    /// `provider_context` is the exact developer semantic rendered for that definition.
    /// `objective` 与 `budget_tokens` 是要持久化的 Goal 定义，`provider_context`
    /// 是为该定义生成的精确 developer 语义。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed context, notice, and optional continuation-turn identities.
    /// 返回已提交的上下文、通知与可选续跑回合身份。
    pub fn create_goal(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.reject_dedicated_change("goal")?;
        let (objective, provider_context) =
            validate_goal_definition(objective.into(), budget_tokens, provider_context.into())?;
        if self.metadata.mode.plan.enabled {
            return Err(DomainError::ModeConflict {
                requested: "goal",
                active: "plan",
            });
        }
        if self.metadata.mode.goal.enabled {
            return Err(DomainError::GoalEnablementMismatch {
                expected_enabled: false,
                actual_enabled: true,
            });
        }
        self.require_goal_creation_boundary()?;
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;

        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let mut staged_active_turn = self.active_turn.clone();
        let (turn_id, continuation_turn_id, context_key) = if needs_turn {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            (Some(turn_id), Some(turn_id), "goal_state")
        } else {
            (
                self.active_turn.as_ref().map(|turn| turn.turn_id),
                None,
                "goal_state",
            )
        };
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                context_key,
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_saved"),
            now_ms,
        )?;
        if let Some(turn_id) = continuation_turn_id {
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq: context_seq,
                closed: false,
            });
        }

        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal = GoalConfig {
            enabled: true,
            objective,
            budget_tokens,
            origin_turn_id: turn_id,
            origin_seq: Some(context_seq),
            state: GoalState::Active,
            budget_cache: None,
            usage_baseline: None,
        };
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if continuation_turn_id.is_some() {
            staged_metadata.open_turn_start_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id,
        })
    }

    /// Atomically saves a Goal by creating, updating, or replacing the completed generation.
    /// 根据当前状态原子创建、更新 Goal，或替换已完成的 Goal 代际。
    ///
    /// # Parameters
    /// 参数
    /// `objective`, `budget_tokens`, and `provider_context` describe the complete next Goal
    /// definition. `now_ms` is the explicit semantic-boundary timestamp.
    /// `objective`、`budget_tokens` 与 `provider_context` 描述完整的下一 Goal 定义；
    /// `now_ms` 是明确传入的语义边界时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` only for an exact update no-op. A completed Goal is always replaced by
    /// a new active generation whose new `origin_seq` is its stable projected Goal identity.
    /// 仅当更新完全无变化时返回 `None`。已完成 Goal 始终替换为新的活动代际，新的
    /// `origin_seq` 即其稳定投影 Goal 身份。
    pub fn save_goal(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<Option<GoalModeTransition>, DomainError> {
        let objective = objective.into();
        let provider_context = provider_context.into();
        if !self.metadata.mode.goal.enabled {
            return self
                .create_goal(objective, budget_tokens, provider_context, now_ms)
                .map(Some);
        }
        if self.metadata.mode.goal.state != GoalState::Completed {
            return self.update_goal_definition(objective, budget_tokens, provider_context, now_ms);
        }
        let (objective, provider_context) =
            validate_goal_definition(objective, budget_tokens, provider_context)?;
        self.replace_completed_goal(objective, budget_tokens, provider_context, now_ms)
            .map(Some)
    }

    /// Replaces one completed Goal with a fully staged active generation.
    /// 使用完整暂存事务把一个已完成 Goal 替换为新的活动代际。
    fn replace_completed_goal(
        &mut self,
        objective: String,
        budget_tokens: Option<u64>,
        provider_context: String,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_goal_state(GoalState::Completed, GoalState::Active)?;
        if self.metadata.mode.plan.enabled {
            return Err(DomainError::ModeConflict {
                requested: "goal",
                active: "plan",
            });
        }
        self.require_goal_creation_boundary()?;
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;

        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let mut staged_active_turn = self.active_turn.clone();
        let (turn_id, continuation_turn_id, context_key) = if needs_turn {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            (Some(turn_id), Some(turn_id), "goal_state")
        } else {
            (
                self.active_turn.as_ref().map(|turn| turn.turn_id),
                None,
                "goal_state",
            )
        };
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                context_key,
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_saved"),
            now_ms,
        )?;
        if let Some(turn_id) = continuation_turn_id {
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq: context_seq,
                closed: false,
            });
        }

        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal = GoalConfig {
            enabled: true,
            objective,
            budget_tokens,
            origin_turn_id: turn_id,
            origin_seq: Some(context_seq),
            state: GoalState::Active,
            budget_cache: None,
            usage_baseline: None,
        };
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if continuation_turn_id.is_some() {
            staged_metadata.open_turn_start_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id,
        })
    }

    /// Atomically updates the objective and budget of one existing Goal.
    /// 原子更新一个已存在 Goal 的目标内容与预算。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` for an exact no-op; otherwise returns the committed transition.
    /// 完全无变化时返回 `None`，否则返回已提交转换。
    pub fn update_goal_definition(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<Option<GoalModeTransition>, DomainError> {
        let (objective, provider_context) =
            validate_goal_definition(objective.into(), budget_tokens, provider_context.into())?;
        self.require_goal_enabled(true)?;
        if self.metadata.mode.goal.state == GoalState::Completed {
            return Err(DomainError::GoalStateTransition {
                current: "completed",
                requested: "updated",
            });
        }
        if self.metadata.mode.goal.objective == objective
            && self.metadata.mode.goal.budget_tokens == budget_tokens
        {
            return Ok(None);
        }
        self.require_no_active_api_for_mode_transition()?;
        self.ensure_transition_capacity(2, 0)?;
        let turn_id = self
            .active_turn
            .as_ref()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_saved"),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.objective = objective;
        staged_metadata.mode.goal.budget_tokens = budget_tokens;
        staged_metadata.mode.goal.budget_cache = None;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        // Editing the same goal must preserve its lifetime and cumulative budget origin.
        // 编辑同一目标必须保留生命周期与累计预算起点。
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            self.active_turn.clone(),
            staged_events,
            now_ms,
        );
        Ok(Some(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        }))
    }

    /// Atomically completes the active Goal and withdraws any undispatched continuation turn.
    /// 原子完成活动 Goal，并撤销任何尚未派发的续跑回合。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the explicit semantic-boundary timestamp used by every fact in this
    /// transition.
    /// `now_ms` 是本次转换中所有事实共同使用的显式语义边界时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` when the Goal is already complete. Otherwise returns the committed
    /// provider-context and UI-notice identities. Every validation and allocation happens
    /// against staged state, so an error leaves the complete document unchanged.
    /// Goal 已完成时返回 `None`；否则返回已提交的 Provider 上下文与 UI 通知身份。
    /// 所有校验与分配都针对暂存状态执行，因此任何错误都会保持完整文档不变。
    pub fn complete_goal(
        &mut self,
        now_ms: i64,
    ) -> Result<Option<GoalModeTransition>, DomainError> {
        self.require_goal_enabled(true)?;
        if self.metadata.mode.goal.state == GoalState::Completed {
            return Ok(None);
        }

        let undispatched_continuation = self.active_turn.as_ref().filter(|turn| {
            !turn.closed
                && turn.origin == TurnOrigin::GoalContinuation
                // Even undispatched business cannot close while an auxiliary physical request owns it.
                // 即使业务尚未分发，辅助物理请求占用期间也不能关闭回合。
                && self.active_api.is_none()
                && !self.events.iter().any(|event| {
                    event.turn_id == Some(turn.turn_id)
                        && matches!(&event.event, EventBody::ApiStarted(started)
                            if !matches!(started.purpose,
                                ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                })
        });
        let closure_buckets = undispatched_continuation
            .map(|turn| self.aggregate_turn_buckets(turn.turn_id, turn.start_seq))
            .transpose()?;
        let expiring_contexts = undispatched_continuation
            .map(|turn| super::turns::expiring_ttl_contexts(self, turn.turn_id))
            .transpose()?
            .unwrap_or_default();
        let ttl_expiration_count = u64::try_from(expiring_contexts.len()).map_err(|_| {
            DomainError::ImmutableFieldMutation {
                entity: "ttl_expiration",
                field: "event_count",
            }
        })?;
        let appended_event_count = if undispatched_continuation.is_some() {
            4 + ttl_expiration_count
        } else {
            2
        };
        self.ensure_transition_capacity(appended_event_count, 0)?;

        let turn_id = self
            .active_turn
            .as_ref()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(appended_event_count as usize);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Remove,
                String::from(
                    "The active goal is complete; automatic continuation is no longer active.",
                ),
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_completed"),
            now_ms,
        )?;

        let mut staged_active_turn = self.active_turn.clone();
        let mut staged_metadata = self.metadata.clone();
        super::turns::clear_expired_dynamic_context_metadata(
            &mut staged_metadata,
            &expiring_contexts,
        );
        if let (Some(turn), Some(buckets)) = (undispatched_continuation, closure_buckets) {
            self.stage_turn_closure(
                &mut staged_allocator,
                &mut staged_events,
                turn.turn_id,
                turn.start_seq,
                crate::v1::event::TurnClosureReason::Stopped,
                0,
                now_ms,
                buckets,
            )?;
            let mut closed_turn = turn.clone();
            closed_turn.closed = true;
            staged_active_turn = Some(closed_turn);
            staged_metadata.open_turn_start_seq = None;
        }
        staged_metadata.mode.goal.state = GoalState::Completed;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(Some(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        }))
    }

    /// Atomically clears one existing Goal and records its explicit exit semantic.
    /// 原子清除一个已存在的 Goal，并记录其明确退出语义。
    pub fn clear_goal(
        &mut self,
        removal_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_no_active_api_for_mode_transition()?;
        let removal_context = validate_system_context_content(removal_context.into())?;
        self.ensure_transition_capacity(2, 0)?;
        // Goal state is session-scoped once the last turn has closed.
        // 最后回合关闭后，目标状态归属于会话，不得再向已闭合回合追加事件。
        let turn_id = self
            .active_turn
            .as_ref()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Remove,
                removal_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_cleared"),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal = GoalConfig::default();
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            self.active_turn.clone(),
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        })
    }

    /// Atomically pauses an active Goal and records its explicit exit semantic.
    /// 原子暂停一个活动 Goal，并记录其明确退出语义。
    pub fn pause_goal(
        &mut self,
        removal_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_goal_state(GoalState::Active, GoalState::Paused)?;
        self.require_no_active_api_for_mode_transition()?;
        let removal_context = validate_system_context_content(removal_context.into())?;
        self.ensure_transition_capacity(2, 0)?;
        // Goal state is session-scoped once the last turn has closed.
        // 最后回合关闭后，目标状态归属于会话，不得再向已闭合回合追加事件。
        let turn_id = self
            .active_turn
            .as_ref()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id);
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                "goal_state",
                SystemContextOperation::Remove,
                removal_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_paused"),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.state = GoalState::Paused;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            self.active_turn.clone(),
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id: None,
        })
    }

    /// Atomically resumes a paused or system-stopped Goal with a fresh context semantic.
    /// 使用新的上下文语义原子恢复已暂停或系统停止的 Goal。
    pub fn resume_goal(
        &mut self,
        objective: impl Into<String>,
        budget_tokens: Option<u64>,
        provider_context: impl Into<String>,
        now_ms: i64,
    ) -> Result<GoalModeTransition, DomainError> {
        let (objective, provider_context) =
            validate_goal_definition(objective.into(), budget_tokens, provider_context.into())?;
        self.require_goal_enabled(true)?;
        if self.metadata.mode.plan.enabled {
            return Err(DomainError::ModeConflict {
                requested: "goal",
                active: "plan",
            });
        }
        if matches!(
            self.metadata.mode.goal.state,
            GoalState::Active | GoalState::Completed
        ) {
            return Err(DomainError::GoalStateTransition {
                current: goal_state_name(self.metadata.mode.goal.state),
                requested: "active",
            });
        }
        self.require_no_active_api_for_mode_transition()?;
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(2);
        let mut staged_active_turn = self.active_turn.clone();
        let (turn_id, continuation_turn_id, context_key) = if needs_turn {
            let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            (Some(turn_id), Some(turn_id), "goal_state")
        } else {
            (
                self.active_turn.as_ref().map(|turn| turn.turn_id),
                None,
                "goal_state",
            )
        };
        let context_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            goal_context_event(
                context_key,
                SystemContextOperation::Upsert,
                provider_context,
            ),
            now_ms,
        )?;
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::UiOnly,
            goal_notice_event("goal_resumed"),
            now_ms,
        )?;
        if let Some(turn_id) = continuation_turn_id {
            staged_active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::GoalContinuation,
                start_seq: context_seq,
                closed: false,
            });
        }
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.objective = objective;
        staged_metadata.mode.goal.budget_tokens = budget_tokens;
        // Resumption is not creation: keep earlier work and accounting in the same goal.
        // 恢复不是创建：之前的工作与核算仍属于同一个目标。
        staged_metadata.mode.goal.state = GoalState::Active;
        staged_metadata.mode.goal.budget_cache = None;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        if continuation_turn_id.is_some() {
            staged_metadata.open_turn_start_seq = Some(context_seq);
        }
        self.commit_goal_transition(
            staged_metadata,
            staged_allocator,
            staged_active_turn,
            staged_events,
            now_ms,
        );
        Ok(GoalModeTransition {
            context_seq: Some(context_seq),
            notice_seq,
            continuation_turn_id,
        })
    }

    /// Atomically changes Plan mode and records the matching provider-visible semantic.
    /// 原子切换 Plan 模式，并记录与之匹配的 Provider 可见语义。
    pub fn set_plan_mode(
        &mut self,
        input: PlanModeTransitionInput,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        if input.enabled && self.metadata.mode.goal.enabled {
            return Err(DomainError::ModeConflict {
                requested: "plan",
                active: "goal",
            });
        }
        if self.metadata.mode.plan.enabled == input.enabled {
            return Ok(false);
        }
        self.reject_dedicated_change("plan_mode")?;
        self.require_no_active_api_for_mode_transition()?;
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            turn_id,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: String::from("plan_mode"),
                operation: if input.enabled {
                    SystemContextOperation::Upsert
                } else {
                    SystemContextOperation::Remove
                },
                scope: SystemContextScope::Session,
                ttl_turns: None,
                provider_role_hint: Some(SystemRoleHint::Developer),
                content: input.provider_context,
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.plan.enabled = input.enabled;
        staged_metadata.updated_at_ms = staged_metadata.updated_at_ms.max(now_ms);
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(true)
    }

    /// Updates the Goal terminal state and appends the structured display note
    /// (plan 7.5.6).
    /// 更新 Goal 终态并追加结构化 display-note(计划 7.5.6)。
    pub fn block_goal(
        &mut self,
        reason_code: impl Into<String>,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        self.require_goal_enabled(true)?;
        self.require_goal_state(GoalState::Active, GoalState::Blocked)?;
        let reason_code = reason_code.into();
        let reason_code = reason_code.trim();
        if reason_code.is_empty() {
            return Err(DomainError::InvalidGoalReasonCode);
        }
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let notice_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            goal_notice_event(reason_code),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.mode.goal.state = GoalState::Blocked;
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[notice_seq],
        );
        Ok(notice_seq)
    }

    /// Admits Goal creation while idle or inside its already-started native tool boundary.
    /// 在空闲状态或已经启动的原生创建工具边界中准入 Goal 创建。
    /// Parameters: none. Returns: success only for idle state or a running create_goal in the owning API.
    /// 参数：无。返回值：仅在空闲或所属 API 存在运行中的 create_goal 时成功。
    fn require_goal_creation_boundary(&self) -> Result<(), DomainError> {
        // The prepared/running V1 lifecycle is authority; a historical tool with this name cannot admit a write.
        // V1 准备/运行生命周期是权威；同名历史工具不能准入写入。
        if let Some(api) = &self.active_api
            && api.tool_seqs.iter().any(|seq| {
                self.events.iter().any(|event| {
                    event.seq == *seq
                        && matches!(&event.event, EventBody::ToolCall(tool)
                    if tool.api_call_id == api.api_call_id
                        && tool.tool_name == "create_goal"
                        && tool.status == ToolCallStatus::Running)
                })
            })
        {
            return Ok(());
        }
        self.require_no_active_api_for_mode_transition()
    }

    /// Requires no physical Provider API to be active before a mode transition.
    /// 要求模式转换前不存在活动的物理 Provider API。
    fn require_no_active_api_for_mode_transition(&self) -> Result<(), DomainError> {
        if let Some(api) = &self.active_api {
            return Err(DomainError::ModeTransitionWhileApiActive {
                api_call_id: api.api_call_id.value(),
            });
        }
        Ok(())
    }

    /// Requires the Goal enablement flag to equal one expected value.
    /// 要求 Goal 启用标记等于一个预期值。
    fn require_goal_enabled(&self, expected_enabled: bool) -> Result<(), DomainError> {
        let actual_enabled = self.metadata.mode.goal.enabled;
        if actual_enabled != expected_enabled {
            return Err(DomainError::GoalEnablementMismatch {
                expected_enabled,
                actual_enabled,
            });
        }
        Ok(())
    }

    /// Requires one exact Goal source state before a requested transition.
    /// 在请求转换前要求一个精确的 Goal 源状态。
    fn require_goal_state(
        &self,
        expected: GoalState,
        requested: GoalState,
    ) -> Result<(), DomainError> {
        let current = self.metadata.mode.goal.state;
        if current != expected {
            return Err(DomainError::GoalStateTransition {
                current: goal_state_name(current),
                requested: goal_state_name(requested),
            });
        }
        Ok(())
    }

    /// Publishes one fully staged Goal transition as a single document revision.
    /// 把一次已完整暂存的 Goal 转换发布为单一文档修订。
    fn commit_goal_transition(
        &mut self,
        metadata: MetadataDocument,
        id_allocator: IdAllocator,
        active_turn: Option<ActiveTurnState>,
        staged_events: Vec<ConversationEvent>,
        now_ms: i64,
    ) {
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.metadata = metadata;
        self.id_allocator = id_allocator;
        self.active_turn = active_turn;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
    }
}
