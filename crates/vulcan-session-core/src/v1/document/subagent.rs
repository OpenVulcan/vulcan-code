//! Durable content returns, legacy statistics pairing, and idempotent delivery receipts.
//! 可靠正文回流、旧版统计配对与幂等投递回执。

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
}

/// Durable receipt for one atomic subagent content return.
/// 一次原子子代理正文回流的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReturnReceipt {
    /// Parent open Turn receiving the return.
    /// 接收回流的父开放回合。
    pub parent_turn_id: TurnId,
    /// Provider-visible subagent content seq.
    /// Provider 可见子代理正文 seq。
    pub return_seq: Seq,
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
                validate_exact_external_identity("child_session_id", &returned.child_session_id)?;
                if event.visibility != EventVisibility::Provider {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.visibility",
                    });
                }
                if event.turn_id.is_none()
                    || returned.source_from_seq > returned.source_to_seq
                    || returned.source_to_seq != returned.child_turn_stats_seq
                    || returned.source_assistant_text_seq < returned.source_from_seq
                    || returned.source_assistant_text_seq > returned.source_to_seq
                {
                    return Err(DomainError::InvalidStatistics {
                        field: "subagent_return.source_range",
                    });
                }
                // Old returns still require the exact adjacent statistics pair.
                // 旧回流仍要求精确相邻统计事件对。
                if !returned.content_only {
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
                    if next.turn_id != event.turn_id || next.visibility != EventVisibility::UiOnly {
                        return Err(DomainError::InvalidStatistics {
                            field: "subagent_return.parent_turn",
                        });
                    }
                    validate_subagent_return_pair(returned, stats)?;
                }
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
                    && matches!(&events[index - 1].event, EventBody::SubagentReturn(returned) if !returned.content_only) =>
                {}
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
    /// `after_seq` is the parent-owned coverage frontier; returns an exact nonoverlapping source range.
    /// `after_seq` 为父会话持有的覆盖前沿；返回精确且不重叠的来源范围。
    pub fn build_subagent_return_input_since(
        &self,
        request: SubagentReturnBuildRequest,
        after_seq: Option<Seq>,
    ) -> Result<SubagentReturnInput, DomainError> {
        let mut input = self.build_subagent_return_input(request)?;
        let from = after_seq.map_or(1, |seq| seq.value() + 1);
        if from < self.resident_start_seq || from > input.source_from_seq.value() {
            return Err(DomainError::InvalidStatistics {
                field: "child_unreturned_source_range",
            });
        }
        // Preserve the source frontier without accumulating numeric usage.
        // 保留来源前沿，不累计数字用量。
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
        })
    }

    /// Atomically appends one final subagent body without numeric usage.
    /// 原子追加一份最终子代理正文，不附带数字用量。
    /// Parameters: input contains exact durable child facts; now_ms is the parent observation time.
    /// 参数：input 包含精确可靠的子任务事实；now_ms 是父会话观测时间。
    /// Returns: the exact receipt, opening a return-owned successor only when no parent turn remains open.
    /// 返回值：精确回执；仅在父回合已结束时开启由回流拥有的后继。
    pub fn append_subagent_return(
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
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(1, u64::from(needs_turn))?;
        let returned = SubagentReturnEvent {
            child_session_id: input.child_session_id.clone(),
            child_task_id: input.child_task_id.clone(),
            content: input.content,
            source_assistant_text_seq: input.source_assistant_text_seq,
            source_from_seq: input.source_from_seq,
            source_to_seq: input.source_to_seq,
            child_turn_stats_seq: input.child_turn_stats_seq,
            content_only: true,
        };
        let mut staged_allocator = self.id_allocator;
        let parent_turn_id = if needs_turn {
            TurnId::try_new(staged_allocator.next_local_id()?)?
        } else {
            self.require_open_turn()?.turn_id
        };
        let mut staged_events = Vec::with_capacity(1);
        let return_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(parent_turn_id),
            EventVisibility::Provider,
            EventBody::SubagentReturn(returned),
            now_ms,
        )?;
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
            &[return_seq],
        );
        Ok(IdempotentMutation::Committed(SubagentReturnReceipt {
            parent_turn_id,
            return_seq,
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
            .max();
        for event in self
            .events
            .iter()
            .rev()
            .filter(|event| event.turn_id == Some(active_turn.turn_id))
        {
            if let EventBody::SubagentReturn(returned) = &event.event {
                if latest_api_started_seq.is_some_and(|started| event.seq <= started) {
                    return Ok(None);
                }
                return Ok(Some(SubagentReturnReceipt {
                    parent_turn_id: active_turn.turn_id,
                    return_seq: event.seq,
                    child_session_id: returned.child_session_id.clone(),
                    child_task_id: returned.child_task_id.clone(),
                    child_turn_stats_seq: returned.child_turn_stats_seq,
                    source_assistant_text_seq: returned.source_assistant_text_seq,
                }));
            }
        }
        Ok(None)
    }

    /// Reconstructs one exact subagent return receipt without mutating the document.
    /// 在不修改文档的情况下重建一份精确子代理回流 receipt。
    fn existing_subagent_return(
        &self,
        input: &SubagentReturnInput,
    ) -> Result<Option<SubagentReturnReceipt>, DomainError> {
        validate_exact_external_identity("child_session_id", &input.child_session_id)?;
        let matching_returns: Vec<_> = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::SubagentReturn(returned)
                    if returned.child_session_id == input.child_session_id
                        && returned.child_task_id == input.child_task_id
                        && returned.child_turn_stats_seq == input.child_turn_stats_seq =>
                {
                    Some((event, returned))
                }
                _ => None,
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
        let (event, returned) = matching_returns[0];
        if returned.source_assistant_text_seq != input.source_assistant_text_seq
            || returned.source_from_seq != input.source_from_seq
            || returned.source_to_seq != input.source_to_seq
            || returned.content != input.content
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "subagent_return",
                field: "durable_facts",
            });
        }
        Ok(Some(SubagentReturnReceipt {
            parent_turn_id: event.turn_id.ok_or(DomainError::InvalidStatistics {
                field: "subagent_return.parent_turn",
            })?,
            return_seq: event.seq,
            child_session_id: returned.child_session_id.clone(),
            child_task_id: returned.child_task_id.clone(),
            child_turn_stats_seq: returned.child_turn_stats_seq,
            source_assistant_text_seq: returned.source_assistant_text_seq,
        }))
    }
}
