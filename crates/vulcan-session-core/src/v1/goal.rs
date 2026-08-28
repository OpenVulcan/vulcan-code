//! V1 Goal auto-continue decision fixture.
//! V1 Goal 自动续跑决策 fixture。
//!
//! Goal continuation is decided at the terminal turn boundary, counts budget
//! tokens as `uncached_input + output` over the frozen merged scope, and treats
//! the main event stream itself as the exactly-once basis. The completed turn
//! statistic and any next Goal turn are published by one document mutation and
//! one flush boundary.
//! Goal 续跑在回合终态边界决策，按冻结归并口径把预算 token 计为
//! `uncached_input + output`，并让主事件流自身成为 exactly-once 依据。已完成
//! 回合统计与可选的下一个 Goal 回合由一次文档修改和一个 flush 边界共同发布。

use crate::v1::config::{GoalConfig, GoalState};
use crate::v1::document::SessionDocument;
use crate::v1::event::{EventBody, TurnStatsEvent};
use crate::v1::ids::Seq;
use crate::v1::stats::goal_budget_tokens;

/// Reason why Goal auto-continue should (or should not) schedule a new turn.
/// Goal 自动续跑应(或不应)调度新回合的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStopReason {
    /// The Goal is not enabled.
    /// Goal 未启用。
    NotEnabled,
    /// Continue with a fresh turn.
    /// 继续新回合。
    Continue,
    /// The objective is already complete.
    /// 目标已完成。
    ObjectiveComplete,
    /// The budget was exhausted (or exceeded) by the last turn.
    /// 上一回合已耗尽(或超出)预算。
    BudgetExhausted {
        /// Configured budget.
        /// 配置预算。
        budget: u64,
        /// Spent tokens over the frozen scope; `None` when unknown.
        /// 冻结口径下已花费 token；未知时为 `None`。
        spent: Option<u64>,
    },
    /// The Goal is paused by the user or system.
    /// Goal 被用户或系统暂停。
    Paused,
    /// The Goal is blocked and cannot continue yet.
    /// Goal 被阻塞，暂时无法继续。
    Blocked,
    /// The last turn produced unknown usage, so exhaustion cannot be asserted.
    /// 上一回合产生未知用量，无法断言预算耗尽。
    UnknownUsage,
}

/// One Goal auto-continue decision.
/// 一次 Goal 自动续跑决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalContinuationDecision {
    /// Whether a new goal-continuation turn should be scheduled.
    /// 是否应调度新的 goal-continuation 回合。
    pub should_continue: bool,
    /// Stable reason of the decision.
    /// 决策的稳定原因。
    pub reason: GoalStopReason,
}

/// Decides whether Goal auto-continue should schedule the next turn from the
/// terminal turn statistic.
/// 依据终态回合统计决定是否应调度下一个回合。
pub fn decide_goal_continuation(
    goal: &GoalConfig,
    last_turn_stats: &TurnStatsEvent,
) -> GoalContinuationDecision {
    if !goal.enabled {
        return GoalContinuationDecision {
            should_continue: false,
            reason: GoalStopReason::NotEnabled,
        };
    }
    match goal.state {
        GoalState::Completed => GoalContinuationDecision {
            should_continue: false,
            reason: GoalStopReason::ObjectiveComplete,
        },
        GoalState::Blocked => GoalContinuationDecision {
            should_continue: false,
            reason: GoalStopReason::Blocked,
        },
        GoalState::Paused => GoalContinuationDecision {
            should_continue: false,
            reason: GoalStopReason::Paused,
        },
        GoalState::BudgetExhausted => GoalContinuationDecision {
            should_continue: false,
            reason: GoalStopReason::BudgetExhausted {
                budget: goal.budget_tokens.unwrap_or(0),
                spent: spent_from_turn(last_turn_stats),
            },
        },
        GoalState::Active => match goal.budget_tokens {
            None => GoalContinuationDecision {
                should_continue: true,
                reason: GoalStopReason::Continue,
            },
            Some(budget) => {
                let spent = spent_from_turn(last_turn_stats);
                match spent {
                    Some(spent) if spent >= budget => GoalContinuationDecision {
                        should_continue: false,
                        reason: GoalStopReason::BudgetExhausted {
                            budget,
                            spent: Some(spent),
                        },
                    },
                    Some(_) => GoalContinuationDecision {
                        should_continue: true,
                        reason: GoalStopReason::Continue,
                    },
                    None => GoalContinuationDecision {
                        should_continue: false,
                        reason: GoalStopReason::UnknownUsage,
                    },
                }
            }
        },
    }
}

/// Computes the budget tokens spent by one terminal turn statistic.
/// 计算一个终态回合统计花费的预算 token。
fn spent_from_turn(stats: &TurnStatsEvent) -> Option<u64> {
    goal_budget_tokens(&stats.buckets.total)
}

/// Recomputes the spent goal budget from stream facts up to one seq.
/// 依据截至某个 seq 的流事实重算已花费 Goal 预算。
///
/// The `metadata.goal.budget_cache` is only an accelerator; the authoritative
/// value is always recomputed from `api_stats` facts when the cache is stale.
/// `metadata.goal.budget_cache` 只是加速器；权威值总是在缓存失配时从
/// `api_stats` 事实重算。
pub fn recompute_goal_budget(
    document: &SessionDocument,
    until_seq: Seq,
) -> Result<Option<u64>, crate::v1::errors::DomainError> {
    let until = until_seq.value();
    let mut uncached_input = 0u64;
    let mut output = 0u64;
    let mut unknown = false;
    for event in document
        .events()
        .iter()
        .filter(|event| event.seq().value() <= until)
    {
        if let EventBody::ApiStats(stats) = &event.event {
            match (stats.usage.uncached_input_tokens, stats.usage.output_tokens) {
                (Some(input), Some(out)) => {
                    uncached_input = uncached_input.saturating_add(input);
                    output = output.saturating_add(out);
                }
                _ => unknown = true,
            }
        }
    }
    if unknown {
        return Ok(None);
    }
    Ok(Some(uncached_input.saturating_add(output)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::stats::{StatBucket, TurnStatBuckets, UsageSnapshot};

    fn usage(input: u64, output: u64) -> UsageSnapshot {
        UsageSnapshot {
            input_tokens: Some(input),
            uncached_input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: Some(input + output),
            ..UsageSnapshot::default()
        }
    }

    fn stats_with_spent(input: u64, output: u64) -> TurnStatsEvent {
        let bucket = StatBucket::from_api_usages([&usage(input, output)], 1, 0, 0);
        TurnStatsEvent {
            start_seq: Seq::try_new(1).unwrap(),
            end_seq: Seq::try_new(5).unwrap(),
            outcome: crate::v1::event::TurnOutcome::Completed,
            duration_ms: Some(1),
            buckets: TurnStatBuckets::combine(bucket, StatBucket::default()),
        }
    }

    fn goal(enabled: bool, budget: Option<u64>, state: GoalState) -> GoalConfig {
        GoalConfig {
            enabled,
            objective: String::new(),
            budget_tokens: budget,
            origin_turn_id: None,
            origin_seq: None,
            state,
            budget_cache: None,
        }
    }

    /// An enabled Goal continues when the budget is not exhausted.
    /// 预算未耗尽时启用的 Goal 继续。
    #[test]
    fn enabled_goal_continues_under_budget() {
        let decision = decide_goal_continuation(
            &goal(true, Some(100), GoalState::Active),
            &stats_with_spent(10, 20),
        );
        assert!(decision.should_continue);
        assert_eq!(decision.reason, GoalStopReason::Continue);
    }

    /// The budget exhausts when uncached input plus output reaches it.
    /// 未缓存输入加输出达到预算时预算耗尽。
    #[test]
    fn budget_exhaustion_triggers_stop() {
        let decision = decide_goal_continuation(
            &goal(true, Some(30), GoalState::Active),
            &stats_with_spent(10, 20),
        );
        assert!(!decision.should_continue);
        assert_eq!(
            decision.reason,
            GoalStopReason::BudgetExhausted {
                budget: 30,
                spent: Some(30)
            }
        );
    }

    /// Unknown usage forbids asserting budget exhaustion.
    /// 未知用量不允许断言预算耗尽。
    #[test]
    fn unknown_usage_does_not_assert_exhaustion() {
        let unknown = StatBucket::from_api_usages([&UsageSnapshot::default()], 1, 0, 0);
        let stats = TurnStatsEvent {
            start_seq: Seq::try_new(1).unwrap(),
            end_seq: Seq::try_new(5).unwrap(),
            outcome: crate::v1::event::TurnOutcome::Completed,
            duration_ms: Some(1),
            buckets: TurnStatBuckets::combine(unknown, StatBucket::default()),
        };
        let decision = decide_goal_continuation(&goal(true, Some(5), GoalState::Active), &stats);
        assert!(!decision.should_continue);
        assert_eq!(decision.reason, GoalStopReason::UnknownUsage);
    }

    /// Completed, paused, or blocked goals never continue.
    /// 已完成、暂停或被阻塞的 Goal 绝不继续。
    #[test]
    fn terminal_states_never_continue() {
        for state in [GoalState::Completed, GoalState::Blocked, GoalState::Paused] {
            let decision =
                decide_goal_continuation(&goal(true, None, state), &stats_with_spent(1, 1));
            assert!(!decision.should_continue, "{state:?} must stop");
        }
    }

    /// A paused goal reports the paused reason, not budget exhaustion.
    /// 暂停的 Goal 报告暂停原因，而不是预算耗尽。
    #[test]
    fn paused_reports_paused_reason() {
        let decision = decide_goal_continuation(
            &goal(true, Some(100), GoalState::Paused),
            &stats_with_spent(1, 1),
        );
        assert_eq!(decision.reason, GoalStopReason::Paused);
    }

    /// Recomputing the budget from stream facts matches the frozen scope.
    /// 从流事实重算预算与冻结口径一致。
    #[test]
    fn recompute_budget_from_stream() {
        let mut document = SessionDocument::new("s", "p", "t", 1);
        document
            .submit_root_input_fixture("a", Vec::new(), 1)
            .unwrap();
        let api = document
            .start_api(
                crate::v1::event::ApiPurposeKind::MainGeneration,
                crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                0,
            )
            .unwrap();
        document
            .append_assistant_text(
                api,
                "x",
                crate::v1::event::AssistantSemanticStatus::Completed,
                1,
            )
            .unwrap();
        document
            .complete_api_group(
                api,
                crate::v1::document::ApiCompletion {
                    request: crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                    usage: usage(10, 20),
                    status: crate::v1::event::ApiCallStatus::Completed,
                    end_reason: Some(crate::v1::event::ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: crate::v1::event::ContinuationEligibility::Forbidden,
                    timing: crate::v1::event::ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: 2,
                },
            )
            .unwrap();
        let last_seq = document.events().last().unwrap().seq();
        let spent = recompute_goal_budget(&document, last_seq).unwrap();
        assert_eq!(spent, Some(30));
    }
}
