//! Durable content returns, legacy statistics pairing, and idempotent delivery receipts.
//! 可靠正文回流、旧版统计配对与幂等投递回执。

use super::persistence::DirtyFile;
use super::{
    ActiveTurnState, IdempotentMutation, SessionDocument, TurnOrigin,
    validate_exact_external_identity,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiCallStatus, ApiPurposeKind, AssistantSemanticStatus, ConversationEvent, EventBody,
    EventVisibility, SubagentReturnEvent, SubagentReturnStatsEvent, SubagentReturnStatus,
    SystemContextScope, TurnBoundaryPurpose, TurnOutcome, TurnStatsEvent,
};
use crate::v1::ids::{ClientActionId, JS_SAFE_INTEGER_MAX, Seq, SubagentTaskId, TurnId};
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
    /// Durable physical child generation, not inferred from a parent receipt.
    /// 可靠物理子代次，不从父回执推断。
    pub(super) child_segment_generation: u64,
    /// Root action read from the exact final business turn's first event.
    /// 从精确最终业务回合首事件读取的根动作。
    pub(super) child_root_action_id: ClientActionId,
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
                    || !(1..=JS_SAFE_INTEGER_MAX).contains(&returned.child_segment_generation)
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
                    returned.child_segment_generation,
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
    /// Publishes an owned execution's exact return and successful terminal state in one revision.
    /// 在同一修订中发布所属执行的精确回流及成功终态。
    /// `execution_id` comes from the authenticated listener; `input` comes from the durable child.
    /// `execution_id` 来自已认证监听器；`input` 来自持久子文档。
    /// Returns an idempotent content receipt; any mismatch rolls back both transitions.
    /// 返回幂等正文回执；任何不匹配都会回滚两个转换。
    pub fn complete_child_task_with_return(
        &mut self,
        execution_id: crate::v1::collaboration::CollaborationExecutionId,
        input: SubagentReturnInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<SubagentReturnReceipt>, DomainError> {
        use crate::v1::collaboration::{
            CollaborationChildTaskState, CollaborationChildTaskTerminalOutcome,
            CollaborationOperationId, TerminalChildTaskInput,
        };
        // The reservation fixes the physical child, original parent turn and parent generation.
        // 预留固定物理子会话、原父回合及父代次。
        let task = self
            .collaboration_state()
            .child_tasks
            .get(&execution_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "child_task",
                identity: execution_id.as_str().to_owned(),
            })?;
        if task.task_id != input.child_task_id
            || task.child.session_id != input.child_session_id
            || task.child.segment_generation != input.child_segment_generation
            || execution_id.as_str() != input.child_root_action_id.as_str()
            || task.parent_segment_generation != self.metadata.segment_generation
            || task.state == CollaborationChildTaskState::Reserved
            || (task.state == CollaborationChildTaskState::Terminal
                && task.terminal_outcome != Some(CollaborationChildTaskTerminalOutcome::Completed))
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.return_binding",
            });
        }
        if task.state == CollaborationChildTaskState::Active
            && self
                .active_turn()
                .is_none_or(|turn| turn.closed || turn.turn_id != task.parent_turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.return_parent_turn",
            });
        }
        // Success is owned by the durable return publisher, never by physical model completion.
        // 成功归持久回流发布器所有，不能由物理模型完成直接宣告。
        let operation_id = CollaborationOperationId::try_new(format!(
            "{}:terminal:completed",
            task.reservation_operation_id.as_str()
        ))?;
        self.apply_atomic_transition(move |staged| {
            // Stage content first; a later terminal conflict must leave no partial publication.
            // 先暂存正文；后续终态冲突不得留下局部发布。
            let returned = staged.append_subagent_return(input, now_ms)?;
            if returned.clone().into_value().parent_turn_id != task.parent_turn_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_task.return_parent_turn",
                });
            }
            let terminal = staged.terminal_child_task(TerminalChildTaskInput {
                operation_id,
                execution_id,
                task_id: task.task_id,
                outcome: CollaborationChildTaskTerminalOutcome::Completed,
                terminal_at_ms: now_ms,
            })?;
            if returned.changed() != terminal.changed() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_task.return_terminal_pair",
                });
            }
            Ok(returned)
        })
    }

    /// Returns the child frontier preceding this exact result, preserving idempotent retries.
    /// 返回本次精确结果之前的子会话回流前沿，保留幂等重试。
    /// Parameters identify the child and final text; returns the last previously covered source seq.
    /// 参数标识子会话与最终正文；返回此前已覆盖的最后源序号。
    pub fn subagent_return_frontier_before(
        &self,
        child_session_id: &str,
        child_segment_generation: u64,
        final_text_seq: Seq,
    ) -> Option<Seq> {
        self.events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::SubagentReturn(returned)
                    if returned.child_session_id == child_session_id
                        && returned.child_segment_generation == child_segment_generation
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

    /// Locates a successful original execution after restart, without using the latest active turn.
    /// 重启后定位成功的原执行，不使用最新活动回合。
    /// `execution_id` identifies the root action and `child_task_id` identifies the reserved task.
    /// `execution_id` 标识根动作，`child_task_id` 标识已预留任务。
    /// Returns None for an unfinished or non-successful execution; malformed success remains an error.
    /// 未结束或非成功执行返回 None；畸形成功记录仍然报错。
    pub fn recover_completed_child_return_request(
        &self,
        execution_id: &crate::v1::collaboration::CollaborationExecutionId,
        child_task_id: SubagentTaskId,
    ) -> Result<Option<SubagentReturnBuildRequest>, DomainError> {
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        // Root action identity is unique across physical executions of a reusable child session.
        // 根动作身份在可复用子会话的物理执行之间唯一。
        let roots = self.events.iter().filter(|event| matches!(&event.event,
            EventBody::UserInput(input) if input.input_source == crate::v1::event::UserInputSource::Root
                && input.client_action_id.as_str() == execution_id.as_str())).collect::<Vec<_>>();
        if roots.len() != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_return.recovery_root",
            });
        }
        // This turn remains fixed even when a later auxiliary attempt or new task exists.
        // 即使后来存在辅助处理尝试或新任务，此回合也保持固定。
        let turn_id = roots[0]
            .turn_id
            .ok_or(DomainError::CollaborationStateViolation {
                field: "child_return.recovery_turn",
            })?;
        // A closed original source is required; absence does not authorize manufacturing completion.
        // 必须存在已闭合原来源；缺失不授权制造完成。
        let terminals = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnStats(stats) if event.turn_id == Some(turn_id) => {
                    Some((event, stats))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if terminals.is_empty() {
            return Ok(None);
        }
        if terminals.len() != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_return.recovery_terminal",
            });
        }
        if terminals[0].1.outcome != TurnOutcome::Completed {
            return Ok(None);
        }
        // Match the terminal API explicitly; arbitrary later assistant text is never a recovery receipt.
        // 显式匹配终态 API；任意后续助手文本都不能作为恢复回执。
        let terminal_api = self
            .events
            .iter()
            .rev()
            .find_map(|event| match &event.event {
                EventBody::ApiStats(stats)
                    if event.turn_id == Some(turn_id) && event.seq < terminals[0].0.seq =>
                {
                    Some(stats.api_call_id)
                }
                _ => None,
            })
            .ok_or(DomainError::CollaborationStateViolation {
                field: "child_return.recovery_api",
            })?;
        // Completed text blocks are immutable; ambiguity is corruption rather than a latest-text choice.
        // 已完成文本块不可变；歧义属于损坏，而非选择最新文本的理由。
        let texts = self.events.iter().filter(|event| event.turn_id == Some(turn_id)
            && event.seq < terminals[0].0.seq && matches!(&event.event,
                EventBody::AssistantText(text) if text.api_call_id == terminal_api && text.status == AssistantSemanticStatus::Completed))
            .collect::<Vec<_>>();
        if texts.len() != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_return.recovery_assistant",
            });
        }
        // The shared builder validates business purpose, completion status and absence of tool calls.
        // 共享构造器校验业务用途、完成状态及不存在工具调用。
        let request = SubagentReturnBuildRequest {
            final_turn_id: turn_id,
            child_task_id,
            source_assistant_text_seq: texts[0].seq,
        };
        self.build_subagent_return_input(request.clone())?;
        Ok(Some(request))
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
        // A surviving completed text block does not turn stopped or abandoned work into success.
        // 留存的已完成文本块不能将停止或放弃的任务变为成功。
        if stats.outcome != TurnOutcome::Completed {
            return Err(DomainError::InvalidStatistics {
                field: "child_final_outcome",
            });
        }
        // Auxiliary answer closure cannot masquerade as the child's business completion.
        // 辅助答疑闭合不能冒充子任务业务完成。
        let boundaries = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnBoundary(boundary)
                    if event.turn_id == Some(request.final_turn_id)
                        && event.seq < stats_event.seq =>
                {
                    Some(boundary)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if boundaries.len() != 1
            || boundaries[0].outcome != TurnOutcome::Completed
            || boundaries[0].reason != crate::v1::event::TurnClosureReason::Completed
            || boundaries[0].purpose != TurnBoundaryPurpose::Business
        {
            return Err(DomainError::InvalidStatistics {
                field: "child_final_business_boundary",
            });
        }
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
                // Session-wide context and host collaboration facts deliberately have no turn owner.
                // 会话级上下文及宿主协作事实按设计没有回合归属。
                // Keep exact visibility and scope checks; foreign turns remain forbidden.
                // 保留精确可见性与作用域校验；仍然禁止其他回合。
                && !(event.turn_id.is_none()
                    && match (&event.event, event.visibility) {
                        (EventBody::Collaboration(_), EventVisibility::HostOnly) => true,
                        (EventBody::SystemContext(context), EventVisibility::Provider) => matches!(
                            context.scope,
                            SystemContextScope::Global | SystemContextScope::Project | SystemContextScope::Session
                        ),
                        _ => false,
                    })
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
            || matches!(
                latest_api_stats.purpose,
                ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
            )
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
        // The source root supplies execution identity; neither the caller nor current turn may invent it.
        // 来源根提供执行身份；调用方及当前回合都不能制造该身份。
        let child_root_action_id = self
            .events
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(input)
                    if event.seq == stats.start_seq
                        && event.turn_id == Some(request.final_turn_id)
                        && input.input_source == crate::v1::event::UserInputSource::Root =>
                {
                    Some(input.client_action_id.clone())
                }
                _ => None,
            })
            .ok_or(DomainError::InvalidStatistics {
                field: "child_return.root_execution",
            })?;
        Ok(SubagentReturnInput {
            child_segment_generation: self.segment_generation(),
            child_root_action_id,
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
                    && returned.child_segment_generation == input.child_segment_generation
                    && returned.source_from_seq <= input.source_to_seq && input.source_from_seq <= returned.source_to_seq)) {
                return Err(DomainError::InvalidStatistics { field: "subagent_return.overlapping_source_range" });
            }
        let needs_turn = self.active_turn.as_ref().is_none_or(|turn| turn.closed);
        self.ensure_transition_capacity(1, u64::from(needs_turn))?;
        let returned = SubagentReturnEvent {
            child_segment_generation: input.child_segment_generation,
            child_root_action_id: input.child_root_action_id.clone(),
            parent_session_id: self.metadata.session_id.clone(),
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

    /// Checks whether a later business API has received the exact child return.
    /// 检查后续业务 API 是否已接收精确子代理回流。
    /// `receipt` fixes the original parent turn and return seq; auxiliary purposes never count.
    /// `receipt` 固定原父回合及回流序号；辅助用途绝不计入。
    /// Returns true only for a later business request in that same turn, including closed turns.
    /// 仅在同一回合存在后续业务请求时返回 true，也支持已关闭回合。
    pub fn subagent_return_dispatch_started(&self, receipt: &SubagentReturnReceipt) -> bool {
        self.events.iter().any(|event| {
            event.turn_id == Some(receipt.parent_turn_id)
                && event.seq > receipt.return_seq
                && matches!(&event.event, EventBody::ApiStarted(started)
                    if !matches!(started.purpose,
                        ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
        })
    }

    /// Returns the latest durable subagent return not yet covered by a later business API.
    /// 返回尚未被后续业务 API 覆盖的最新可靠子代理回流。
    /// Auxiliary questions and compaction never consume the original business result.
    /// 辅助答疑与压缩绝不能消费原业务结果。
    pub fn pending_subagent_return_dispatch(
        &self,
    ) -> Result<Option<SubagentReturnReceipt>, DomainError> {
        let Some(active_turn) = self.active_turn.as_ref().filter(|turn| !turn.closed) else {
            return Ok(None);
        };
        // Consumption belongs to business execution, not to every request using this history.
        // 消费归属于业务执行，而非所有使用该历史的请求。
        let latest_business_api_started_seq = self
            .events
            .iter()
            .filter_map(|event| {
                (event.turn_id == Some(active_turn.turn_id)
                    && matches!(&event.event, EventBody::ApiStarted(started)
                        if !matches!(started.purpose,
                            ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution)))
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
                if latest_business_api_started_seq.is_some_and(|started| event.seq <= started) {
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
                        && returned.child_segment_generation == input.child_segment_generation
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
        if returned.parent_session_id != self.metadata.session_id {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "subagent_return",
                field: "parent_session_id",
            });
        }
        if returned.source_assistant_text_seq != input.source_assistant_text_seq
            || returned.child_root_action_id != input.child_root_action_id
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
