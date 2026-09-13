//! Durable child-return facts, exact statistics pairing, and idempotent delivery receipts.
//! 可靠子任务返回事实、精确统计配对及幂等投递回执。

use super::persistence::DirtyFile;
use super::{
    ActiveTurnState, IdempotentMutation, SessionDocument, TurnOrigin,
    validate_exact_external_identity,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiCallStatus, AssistantSemanticStatus, ConversationEvent, EventBody, EventVisibility,
    SubagentReturnEvent, SubagentReturnStatsEvent, SubagentReturnStatus, TurnStatsEvent,
};
use crate::v1::ids::{Seq, SubagentTaskId, TurnId};
use crate::v1::stats::{StatBucket, TurnStatBuckets};
use std::collections::BTreeSet;

/// Exact durable child facts required to select one subagent return body.
/// 选择一份子代理回流正文所需的精确持久化子会话事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnBuildRequest {
    /// Final child Turn identity.
    /// 子会话最终回合身份。
    pub final_turn_id: TurnId,
    /// Stable external task identity.
    /// 稳定的外部任务身份。
    pub child_task_id: SubagentTaskId,
    /// Exact completed assistant-text seq returned to the parent.
    /// 精确回流给父会话的已完成 assistant 正文 seq。
    pub source_assistant_text_seq: Seq,
}

/// Opaque validated input for one atomic parent-side subagent return.
/// 一次父会话侧原子子代理回流的不透明已校验输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnInput {
    /// Exact child document identity, inspectable only inside the document domain.
    /// 精确子文档身份，仅允许文档领域内部检查。
    pub(super) child_session_id: String,
    /// Stable child task identity retained by validated construction.
    /// 由已校验构造保留的稳定子任务身份。
    child_task_id: SubagentTaskId,
    /// Exact validated assistant result, retained for internal drift checks.
    /// 精确已校验 assistant 结果，保留供内部漂移检查。
    pub(super) content: String,
    /// Final assistant fact selected by the child-return builder.
    /// 子任务返回构造器选定的最终 assistant 事实。
    source_assistant_text_seq: Seq,
    /// Inclusive first source fact covered by the child result.
    /// 子任务结果覆盖的首个源事实，包含边界。
    source_from_seq: Seq,
    /// Inclusive last source fact covered by the child result.
    /// 子任务结果覆盖的最后一个源事实，包含边界。
    source_to_seq: Seq,
    /// Exact child statistics fact paired with the returned content.
    /// 与返回正文配对的精确子任务统计事实。
    pub(super) child_turn_stats_seq: Seq,
    /// Validated aggregate checked by internal ownership and idempotency tests.
    /// 由内部归属与幂等测试核对的已校验聚合统计。
    pub(super) totals: StatBucket,
}

/// Durable receipt for one atomic subagent content-and-statistics return.
/// 一次原子子代理正文与统计回流的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnReceipt {
    /// Parent open Turn receiving the return.
    /// 接收回流的父开放回合。
    pub parent_turn_id: TurnId,
    /// Provider-visible subagent content seq.
    /// Provider 可见子代理正文 seq。
    pub return_seq: Seq,
    /// UiOnly inclusive statistics seq.
    /// UiOnly 包容性统计 seq。
    pub stats_seq: Seq,
    /// Exact child session identity.
    /// 精确子会话身份。
    pub child_session_id: String,
    /// Exact child task identity.
    /// 精确子代理任务身份。
    pub child_task_id: SubagentTaskId,
    /// Exact child terminal TurnStats seq.
    /// 精确子会话终态 TurnStats seq。
    pub child_turn_stats_seq: Seq,
    /// Exact child assistant-text seq used as the returned body.
    /// 被用作回流正文的精确子会话 assistant 文本 seq。
    pub source_assistant_text_seq: Seq,
}

/// Validates one complete statistics bucket.
/// 校验一个完整统计桶。
fn validate_stat_bucket(bucket: &StatBucket) -> Result<(), DomainError> {
    if bucket.unknown_usage_api_call_count > bucket.api_call_count {
        return Err(DomainError::InvalidStatistics {
            field: "unknown_usage_api_call_count",
        });
    }
    Ok(())
}

/// Validates direct/subagent/total consistency for one terminal Turn statistic.
/// 校验一条终态回合统计的 direct/subagent/total 一致性。
pub(super) fn validate_turn_stat_buckets(buckets: &TurnStatBuckets) -> Result<(), DomainError> {
    validate_stat_bucket(&buckets.direct)?;
    validate_stat_bucket(&buckets.subagent)?;
    validate_stat_bucket(&buckets.total)?;
    let expected = TurnStatBuckets::combine(buckets.direct.clone(), buckets.subagent.clone());
    if expected.total != buckets.total {
        return Err(DomainError::InvalidStatistics {
            field: "turn_stats.total",
        });
    }
    Ok(())
}

/// Validates the two adjacent events of one atomic final subagent return.
/// 校验一次原子最终子代理回流的两条相邻事件。
fn validate_subagent_return_pair(
    returned: &SubagentReturnEvent,
    stats: &SubagentReturnStatsEvent,
) -> Result<(), DomainError> {
    validate_exact_external_identity("child_session_id", &returned.child_session_id)?;
    validate_exact_external_identity("child_session_id", &stats.child_session_id)?;
    validate_stat_bucket(&stats.totals)?;
    if returned.child_session_id != stats.child_session_id
        || returned.child_task_id != stats.child_task_id
        || returned.child_turn_stats_seq != stats.child_turn_stats_seq
    {
        return Err(DomainError::InvalidStatistics {
            field: "subagent_return.identity",
        });
    }
    if returned.source_from_seq.value() > returned.source_to_seq.value()
        || returned.source_to_seq != returned.child_turn_stats_seq
        || returned.source_assistant_text_seq.value() < returned.source_from_seq.value()
        || returned.source_assistant_text_seq.value() > returned.source_to_seq.value()
    {
        return Err(DomainError::InvalidStatistics {
            field: "subagent_return.source_range",
        });
    }
    if stats.status != SubagentReturnStatus::Final {
        return Err(DomainError::InvalidStatistics {
            field: "subagent_return.status",
        });
    }
    Ok(())
}

/// Validates that every persisted final subagent return is one adjacent atomic pair.
/// 校验每个持久化最终子代理回流都是一组相邻原子事件。
pub(super) fn validate_subagent_return_history(
    events: &[ConversationEvent],
) -> Result<(), DomainError> {
    let mut identities = BTreeSet::new();
    for (index, event) in events.iter().enumerate() {
        match &event.event {
            EventBody::SubagentReturn(returned) => {
                let next = events
                    .get(index + 1)
                    .ok_or(DomainError::InvalidStatistics {
                        field: "subagent_return_stats_pair",
                    })?;
                let EventBody::SubagentReturnStats(stats) = &next.event else {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return_stats_pair",
                    });
                };
                if event.turn_id.is_none() || next.turn_id != event.turn_id {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.parent_turn",
                    });
                }
                if event.visibility != EventVisibility::Provider
                    || next.visibility != EventVisibility::UiOnly
                {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.visibility",
                    });
                }
                validate_subagent_return_pair(returned, stats)?;
                if !identities.insert((
                    returned.child_session_id.clone(),
                    returned.child_task_id.clone(),
                    returned.child_turn_stats_seq,
                )) {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.identity_unique",
                    });
                }
            }
            EventBody::SubagentReturnStats(_)
                if index > 0
                    && matches!(&events[index - 1].event, EventBody::SubagentReturn(_)) => {}
            EventBody::SubagentReturnStats(_) => {
                return Err(DomainError::InvalidStatistics {
                    field: "subagent_return_stats_pair",
                });
            }
            _ => {}
        }
    }
    Ok(())
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Returns the child frontier preceding this exact result, preserving idempotent retries.
    /// 返回本次精确结果之前的子会话回流前沿，保留幂等重试。
    /// Parameters identify the child and final text; returns the last previously covered source seq.
    /// 参数标识子会话与最终正文；返回此前已覆盖的最后源序号。
    pub fn subagent_return_frontier_before(
        &self,
        child_session_id: &str,
        final_text_seq: Seq,
    ) -> Option<Seq> {
        self.events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::SubagentReturn(returned)
                    if returned.child_session_id == child_session_id
                        && returned.source_to_seq.value() < final_text_seq.value() =>
                {
                    Some(returned.source_to_seq)
                }
                _ => None,
            })
            .max()
    }

    /// Builds a return covering every unreturned closed turn, including stopped predecessors.
    /// 构建覆盖全部尚未回流的闭合回合的结果，包含此前停止的回合。
    /// `after_seq` is the parent-owned coverage frontier; returns exact nonoverlapping totals.
    /// `after_seq` 为父会话持有的覆盖前沿；返回精确且不重叠的统计总量。
    pub fn build_subagent_return_input_since(
        &self,
        request: SubagentReturnBuildRequest,
        after_seq: Option<Seq>,
    ) -> Result<SubagentReturnInput, DomainError> {
        // Preserve exact final-body identity and durability validation from the canonical builder.
        // 保留规范构建器的精确最终正文身份与可靠性校验。
        let mut input = self.build_subagent_return_input(request)?;
        // The parent-owned frontier excludes every already returned source event.
        // 父会话持有的前沿排除全部已经回流的源事件。
        let from = after_seq.map_or(1, |seq| seq.value() + 1);
        if from < self.resident_start_seq || from > input.source_from_seq.value() {
            return Err(DomainError::InvalidStatistics {
                field: "child_unreturned_source_range",
            });
        }
        // Recompute each closed turn from its API facts, including legacy timing checkpoints.
        // 从 API 事实重算每个闭合回合，包含旧计时点。
        let mut totals = Vec::new();
        for event in self
            .events
            .iter()
            .filter(|event| event.seq.value() >= from && event.seq <= input.source_to_seq)
        {
            if let EventBody::TurnStats(stats) = &event.event {
                if stats.start_seq.value() < from {
                    return Err(DomainError::InvalidStatistics {
                        field: "child_unreturned_partial_turn",
                    });
                }
                let turn_id = event.turn_id.ok_or(DomainError::InvalidStatistics {
                    field: "child_unreturned_turn_id",
                })?;
                totals.push(self.aggregate_turn_buckets(turn_id, stats.start_seq)?.total);
            }
        }
        input.totals = StatBucket::from_subagent_returns(&totals);
        // These are turns of the same child, so merging them does not create extra child returns.
        // 这些是同一子会话的回合，合并它们不产生额外子代理回流次数。
        input.totals.subagent_return_count = totals
            .iter()
            .map(|bucket| bucket.subagent_return_count)
            .sum();
        input.source_from_seq = Seq::try_new(from)?;
        Ok(input)
    }

    /// Builds one exact parent-return input from a fully durable child document.
    /// 从一个完全持久化的子会话文档构建精确的父回流输入。
    pub fn build_subagent_return_input(
        &self,
        request: SubagentReturnBuildRequest,
    ) -> Result<SubagentReturnInput, DomainError> {
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        validate_exact_external_identity("child_session_id", &self.metadata.session_id)?;
        let terminal_stats: Vec<(&ConversationEvent, &TurnStatsEvent)> = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnStats(stats) if event.turn_id == Some(request.final_turn_id) => {
                    Some((event, stats))
                }
                _ => None,
            })
            .collect();
        if terminal_stats.len() != 1 {
            return Err(DomainError::InvalidStatistics {
                field: "child_turn_stats_unique",
            });
        }
        let (stats_event, stats) = terminal_stats[0];
        validate_turn_stat_buckets(&stats.buckets)?;
        if stats.end_seq != stats_event.seq
            || stats.start_seq.value() > stats.end_seq.value()
            || stats.start_seq.value() < self.resident_start_seq
        {
            return Err(DomainError::InvalidStatistics {
                field: "child_turn_stats_range",
            });
        }
        if request.source_assistant_text_seq.value() < stats.start_seq.value()
            || request.source_assistant_text_seq.value() > stats.end_seq.value()
        {
            return Err(DomainError::InvalidStatistics {
                field: "source_assistant_text_seq",
            });
        }
        if self.events.iter().any(|event| {
            (stats.start_seq.value()..=stats.end_seq.value()).contains(&event.seq.value())
                && event.turn_id != Some(request.final_turn_id)
        }) {
            return Err(DomainError::InvalidStatistics {
                field: "child_source_range_turn",
            });
        }
        let source_text = self
            .events
            .iter()
            .find(|event| event.seq == request.source_assistant_text_seq)
            .and_then(|event| match &event.event {
                EventBody::AssistantText(text)
                    if event.turn_id == Some(request.final_turn_id)
                        && text.status == AssistantSemanticStatus::Completed =>
                {
                    Some(text)
                }
                _ => None,
            })
            .ok_or(DomainError::InvalidStatistics {
                field: "source_assistant_text",
            })?;
        let latest_api_stats = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStats(api_stats)
                    if event.turn_id == Some(request.final_turn_id)
                        && event.seq.value() < stats_event.seq.value() =>
                {
                    Some((event.seq, api_stats))
                }
                _ => None,
            })
            .max_by_key(|(seq, _)| seq.value())
            .map(|(_, api_stats)| api_stats)
            .ok_or(DomainError::InvalidStatistics {
                field: "child_final_api_stats",
            })?;
        if latest_api_stats.status != ApiCallStatus::Completed
            || source_text.api_call_id != latest_api_stats.api_call_id
            || self.events.iter().any(|event| {
                matches!(
                    &event.event,
                    EventBody::ToolCall(tool)
                        if tool.api_call_id == latest_api_stats.api_call_id
                )
            })
        {
            return Err(DomainError::InvalidStatistics {
                field: "child_final_assistant",
            });
        }
        Ok(SubagentReturnInput {
            child_session_id: self.metadata.session_id.clone(),
            child_task_id: request.child_task_id,
            content: source_text.text.clone(),
            source_assistant_text_seq: request.source_assistant_text_seq,
            source_from_seq: stats.start_seq,
            source_to_seq: stats.end_seq,
            child_turn_stats_seq: stats_event.seq,
            totals: stats.buckets.total.clone(),
        })
    }

    /// Atomically appends one final subagent body and its complete inclusive statistics.
    /// 原子追加一份最终子代理正文及其完整包容性统计。
    /// Parameters: input contains exact durable child facts; now_ms is the parent observation time.
    /// 参数：input 包含精确可靠的子任务事实；now_ms 是父会话观测时间。
    /// Returns: the exact receipt, opening a return-owned successor only when no parent turn remains open.
    /// 返回值：精确回执；仅在父回合已结束时开启由回流拥有的后继。
    pub fn append_subagent_return_with_stats(
        &mut self,
        input: SubagentReturnInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<SubagentReturnReceipt>, DomainError> {
        if let Some(receipt) = self.existing_subagent_return(&input)? {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        if self.events.iter().any(|event| matches!(&event.event,
            EventBody::SubagentReturn(returned) if returned.child_session_id == input.child_session_id
                && returned.source_from_seq <= input.source_to_seq && input.source_from_seq <= returned.source_to_seq)) {
            return Err(DomainError::InvalidStatistics { field: "subagent_return.overlapping_source_range" });
        }
        // A child may outlive its parent turn; its return is the successor's own root semantic.
        // 子任务可能晚于父回合结束；回流自身就是后继回合的根语义。
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(2, u64::from(needs_turn))?;
        let returned = SubagentReturnEvent {
            child_session_id: input.child_session_id.clone(),
            child_task_id: input.child_task_id.clone(),
            content: input.content,
            source_assistant_text_seq: input.source_assistant_text_seq,
            source_from_seq: input.source_from_seq,
            source_to_seq: input.source_to_seq,
            child_turn_stats_seq: input.child_turn_stats_seq,
        };
        let stats = SubagentReturnStatsEvent {
            child_session_id: input.child_session_id.clone(),
            child_task_id: input.child_task_id.clone(),
            child_turn_stats_seq: input.child_turn_stats_seq,
            status: SubagentReturnStatus::Final,
            totals: input.totals,
        };
        validate_subagent_return_pair(&returned, &stats)?;
        let mut staged_allocator = self.id_allocator;
        // parent_turn_id is allocated in the same atomic transition as the return and statistics.
        // parent_turn_id 与回流及统计在同一个原子事务中分配。
        let parent_turn_id = if needs_turn {
            TurnId::try_new(staged_allocator.next_local_id()?)?
        } else {
            self.require_open_turn()?.turn_id
        };
        let mut staged_events = Vec::with_capacity(2);
        let return_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(parent_turn_id),
            EventVisibility::Provider,
            EventBody::SubagentReturn(returned),
            now_ms,
        )?;
        let stats_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(parent_turn_id),
            EventVisibility::UiOnly,
            EventBody::SubagentReturnStats(stats),
            now_ms,
        )?;
        let changed_event_seqs = vec![return_seq, stats_seq];
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        if needs_turn {
            self.active_turn = Some(ActiveTurnState {
                turn_id: parent_turn_id,
                origin: TurnOrigin::SubagentReturn,
                start_seq: return_seq,
                closed: false,
            });
            self.metadata.open_turn_start_seq = Some(return_seq);
        }
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(IdempotentMutation::Committed(SubagentReturnReceipt {
            parent_turn_id,
            return_seq,
            stats_seq,
            child_session_id: input.child_session_id,
            child_task_id: input.child_task_id,
            child_turn_stats_seq: input.child_turn_stats_seq,
            source_assistant_text_seq: input.source_assistant_text_seq,
        }))
    }

    /// Returns the latest durable subagent return not yet covered by a later physical API.
    /// 返回尚未被后续物理 API 覆盖的最新可靠子代理回流。
    pub fn pending_subagent_return_dispatch(
        &self,
    ) -> Result<Option<SubagentReturnReceipt>, DomainError> {
        let Some(active_turn) = self.active_turn.as_ref().filter(|turn| !turn.closed) else {
            return Ok(None);
        };
        let latest_api_started_seq = self
            .events
            .iter()
            .filter_map(|event| {
                (event.turn_id == Some(active_turn.turn_id)
                    && matches!(&event.event, EventBody::ApiStarted(_)))
                .then_some(event.seq)
            })
            .max_by_key(|seq| seq.value());
        let Some((return_index, return_event, returned)) = self
            .events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match &event.event {
                EventBody::SubagentReturn(returned)
                    if event.turn_id == Some(active_turn.turn_id)
                        && latest_api_started_seq
                            .is_none_or(|started_seq| event.seq.value() > started_seq.value()) =>
                {
                    Some((index, event, returned))
                }
                _ => None,
            })
            .next_back()
        else {
            return Ok(None);
        };
        let stats_event =
            self.events
                .get(return_index + 1)
                .ok_or(DomainError::InvalidStatistics {
                    field: "subagent_return_stats_pair",
                })?;
        let EventBody::SubagentReturnStats(stats) = &stats_event.event else {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            });
        };
        validate_subagent_return_pair(returned, stats)?;
        if return_event.turn_id != stats_event.turn_id
            || return_event.seq.value().checked_add(1) != Some(stats_event.seq.value())
        {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            });
        }
        Ok(Some(SubagentReturnReceipt {
            parent_turn_id: active_turn.turn_id,
            return_seq: return_event.seq,
            stats_seq: stats_event.seq,
            child_session_id: returned.child_session_id.clone(),
            child_task_id: returned.child_task_id.clone(),
            child_turn_stats_seq: returned.child_turn_stats_seq,
            source_assistant_text_seq: returned.source_assistant_text_seq,
        }))
    }

    /// Reconstructs one exact subagent return receipt without mutating the document.
    /// 在不修改文档的情况下重建一份精确子代理回流 receipt。
    fn existing_subagent_return(
        &self,
        input: &SubagentReturnInput,
    ) -> Result<Option<SubagentReturnReceipt>, DomainError> {
        validate_exact_external_identity("child_session_id", &input.child_session_id)?;
        validate_stat_bucket(&input.totals)?;
        let matching_returns: Vec<&ConversationEvent> = self
            .events
            .iter()
            .filter(|event| {
                matches!(
                    &event.event,
                    EventBody::SubagentReturn(returned)
                        if returned.child_session_id == input.child_session_id
                            && returned.child_task_id == input.child_task_id
                            && returned.child_turn_stats_seq == input.child_turn_stats_seq
                )
            })
            .collect();
        if matching_returns.is_empty() {
            return Ok(None);
        }
        if matching_returns.len() != 1 {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_identity_unique",
            });
        }
        let return_event = matching_returns[0];
        // Delivery identity belongs to a child completion, not to the currently open parent turn.
        // 投递身份属于一次子任务完成，而不属于当前开放的父回合。
        let parent_turn_id = return_event.turn_id.ok_or(DomainError::InvalidStatistics {
            field: "subagent_return.parent_turn",
        })?;
        let stats_event = self
            .events
            .iter()
            .find(|event| event.seq.value() == return_event.seq.value() + 1)
            .ok_or(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            })?;
        let EventBody::SubagentReturn(returned) = &return_event.event else {
            unreachable!("matching return filter guarantees the event kind")
        };
        let EventBody::SubagentReturnStats(stats) = &stats_event.event else {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_return_stats_pair",
            });
        };
        validate_subagent_return_pair(returned, stats)?;
        if returned.child_turn_stats_seq != input.child_turn_stats_seq
            || returned.source_assistant_text_seq != input.source_assistant_text_seq
            || returned.source_from_seq != input.source_from_seq
            || returned.source_to_seq != input.source_to_seq
            || returned.content != input.content
            || stats.totals != input.totals
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "subagent_return",
                field: "durable_facts",
            });
        }
        Ok(Some(SubagentReturnReceipt {
            parent_turn_id,
            return_seq: return_event.seq,
            stats_seq: stats_event.seq,
            child_session_id: returned.child_session_id.clone(),
            child_task_id: returned.child_task_id.clone(),
            child_turn_stats_seq: returned.child_turn_stats_seq,
            source_assistant_text_seq: returned.source_assistant_text_seq,
        }))
    }
}
