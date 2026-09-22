//! Dedicated question-execution mutations and auxiliary-turn ownership.
//! 专用问题执行变更与辅助回合所有权。

use crate::v1::collaboration::*;
use crate::v1::document::{
    ActiveTurnState, DirtyFile, IdempotentMutation, SessionDocument, TurnOrigin,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{ConversationEvent, EventBody, EventVisibility};
use crate::v1::ids::{IdAllocator, Seq, TurnId};

impl SessionDocument {
    /// Fails the exact persisted auxiliary settlement without terminalizing its parent business task.
    /// 标记精确持久化辅助结算失败，不终结其父业务任务。
    /// `record` binds the execution and optional active API; detail is sanitized host diagnostic text.
    /// `record` 绑定执行及可选活动 API；detail 是宿主已脱敏诊断文本。
    /// Returns true for one atomic failure publication, false when the exact execution already ended.
    /// 一次原子失败发布返回 true，精确执行已结束时返回 false。
    pub fn fail_question_execution_for_terminal_settlement(
        &mut self,
        record: &crate::v1::config::TerminalSettlementRecord,
        detail: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        use crate::v1::config::TerminalSettlementScope;
        use crate::v1::event::{ApiPurposeKind, DisplayNoticeKind, NoticeSeverity};
        validate_timestamp("question_execution.failure_time", now_ms)?;
        // A stored intent is the authority for repair, including after a process restart.
        // 已保存意图是修复权威，进程重启后亦如此。
        let retained = self.terminal_settlement_record().ok_or_else(|| {
            DomainError::TerminalSettlementMissing {
                settlement_id: record.settlement_id.clone(),
            }
        })?;
        if retained.settlement_id != record.settlement_id
            || retained.turn_id != record.turn_id
            || retained.scope != record.scope
            || retained.owner_generation != record.owner_generation
            || retained.reason_code != record.reason_code
        {
            return Err(DomainError::TerminalSettlementConflict {
                settlement_id: retained.settlement_id.clone(),
            });
        }
        // Scope must carry an explicit question identity; there is no latest-question fallback.
        // 范围必须携带明确问题执行身份，不回退到最新问题。
        let TerminalSettlementScope::QuestionExecution {
            execution_id,
            api_call_id,
        } = &record.scope
        else {
            return Err(DomainError::CollaborationStateViolation {
                field: "terminal_settlement.question_scope",
            });
        };
        if self.terminal_settlement_has_terminal_fact(record)? {
            return Ok(false);
        }
        // Reject a replacement logical owner before touching any API, tool or document revision.
        // 修改任何 API、工具或文档修订前拒绝替代逻辑所有者。
        let turn = self.require_open_turn()?;
        if turn.turn_id != record.turn_id {
            return Err(DomainError::TurnIdentityMismatch {
                expected: record.turn_id.value(),
                actual: turn.turn_id.value(),
            });
        }
        if turn.origin == TurnOrigin::QuestionExecution
            && !self.metadata.input_state.pending_in_turn_inputs.is_empty()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "question_execution.business_inputs",
            });
        }
        // The running execution's immutable begin receipt separates successive attempts in one parent turn.
        // 运行中执行的不可变启动回执区分同一父回合内先后发生的尝试。
        let execution = self
            .collaboration_state()
            .question_executions
            .get(execution_id)
            .expect("settlement binding validated above");
        // State validation requires every execution operation to have its original receipt.
        // 状态校验要求每个执行操作都有其原始回执。
        let begin_seq = self
            .collaboration_state()
            .operations
            .get(&execution.operation_id)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "terminal_settlement.question_begin",
            })?
            .event_seq;
        // A request may have ended before the host can persist execution failure. Keep its original result.
        // 请求可能先于宿主持久化执行失败而结束；保留该请求原始结果。
        let interrupt_api = match (api_call_id, self.active_api()) {
            (None, None) => false,
            (Some(expected), Some(api))
                if *expected == api.api_call_id
                    && api.turn_id == record.turn_id
                    && api.purpose == ApiPurposeKind::QuestionExecution
                    && api.started_seq > begin_seq =>
            {
                true
            }
            (Some(expected), None) => {
                // Only the last physical request in this turn can satisfy the captured boundary.
                // 只有该回合最后一次物理请求能满足捕获的边界。
                let last_start = self.events().iter().rev().find(|event| {
                    event.turn_id == Some(record.turn_id)
                        && matches!(event.event, EventBody::ApiStarted(_))
                });
                // The immutable start and terminal must both prove the exact QA request and attempt.
                // 不可变起始与终态必须共同证明精确答疑请求及尝试。
                let terminal_proven = last_start.is_some_and(|start| {
                    start.seq > begin_seq
                        && matches!(&start.event, EventBody::ApiStarted(api)
                            if api.api_call_id == *expected && api.purpose == ApiPurposeKind::QuestionExecution)
                        && self.events().iter().any(|event| {
                            event.turn_id == Some(record.turn_id) && event.seq > start.seq
                                && matches!(&event.event, EventBody::ApiStats(api)
                                    if api.api_call_id == *expected && api.purpose == ApiPurposeKind::QuestionExecution)
                        })
                });
                if !terminal_proven {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "terminal_settlement.question_api",
                    });
                }
                false
            }
            _ => {
                return Err(DomainError::CollaborationStateViolation {
                    field: "terminal_settlement.question_api",
                });
            }
        };
        self.apply_atomic_transition(|staged| {
            if interrupt_api {
                staged.fail_active_api_for_execution(&record.reason_code, detail, now_ms)?;
            }
            staged.append_display_note(
                Some(record.turn_id),
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Error,
                &record.reason_code,
                detail.map(str::to_owned),
                *api_call_id,
                None,
                None,
                now_ms,
            )?;
            staged.complete_question_execution(CompleteQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "terminal-failure:{}",
                    record.settlement_id
                ))?,
                execution_id: execution_id.clone(),
                outcome: CollaborationQuestionExecutionOutcome::Failed,
                completed_at_ms: now_ms,
            })?;
            Ok(true)
        })
    }
}

/// Stages one typed collaboration event without publishing document state.
/// 暂存一条强类型协作事件但不发布文档状态。
fn stage_question_event(
    document: &SessionDocument,
    state: &CollaborationState,
    operation_id: CollaborationOperationId,
    event_kind: CollaborationEventKind,
    after_state: CollaborationAfterState,
    turn_id: Option<TurnId>,
    now_ms: i64,
) -> Result<(IdAllocator, Vec<ConversationEvent>, Seq), DomainError> {
    validate_timestamp("collaboration.updated_at_ms", now_ms)?;
    let event = CollaborationEvent {
        version: COLLABORATION_METADATA_VERSION,
        operation_id,
        event_kind,
        after_state,
    };
    event.validate()?;
    if !event.matches_state(state) {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.event_after_state",
        });
    }
    let mut staged_allocator = document.id_allocator;
    let mut staged_events = Vec::with_capacity(1);
    let event_seq = document.stage_event(
        &mut staged_allocator,
        &mut staged_events,
        turn_id,
        EventVisibility::HostOnly,
        EventBody::Collaboration(event),
        now_ms,
    )?;
    Ok((staged_allocator, staged_events, event_seq))
}

/// Returns an idempotent receipt for an already recorded execution operation.
/// 返回已记录执行操作的幂等回执。
fn existing_execution_receipt(
    existing: CollaborationOperationRecord,
) -> Result<IdempotentMutation<CollaborationQuestionExecutionReceipt>, DomainError> {
    let CollaborationOperationResult::QuestionExecution { execution } = existing.result else {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.operation.result",
        });
    };
    Ok(IdempotentMutation::Idempotent(
        CollaborationQuestionExecutionReceipt {
            execution,
            event_seq: existing.event_seq,
        },
    ))
}

/// Ensures that a settled execution is backed by target-side settlement state.
/// 确保已结算执行由目标侧终态结算状态支撑。
fn validate_settled_question(
    document: &SessionDocument,
    execution: &CollaborationQuestionExecutionRecord,
) -> Result<(), DomainError> {
    if execution.outcome != Some(CollaborationQuestionExecutionOutcome::Settled) {
        return Ok(());
    }
    let settled = document
        .collaboration_state()
        .inbound_deliveries
        .values()
        .find(|delivery| delivery.question_id == execution.question_id)
        .is_some_and(|delivery| delivery.settlement.is_some());
    if !settled {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.question_execution.settlement",
        });
    }
    Ok(())
}

/// Begins a dedicated question execution and, when idle, creates its real auxiliary turn.
/// 开始专用问题执行，并在空闲时创建真实辅助回合。
pub(super) fn begin_question_execution(
    document: &mut SessionDocument,
    input: BeginQuestionExecutionInput,
) -> Result<IdempotentMutation<CollaborationQuestionExecutionReceipt>, DomainError> {
    document.validate_target_participant(&input.participant)?;
    validate_timestamp(
        "collaboration.question_execution.started_at_ms",
        input.started_at_ms,
    )?;
    if input.purpose == CollaborationExecutionPurpose::NormalTask {
        return Err(DomainError::InvalidStatistics {
            field: "collaboration.question_execution.purpose",
        });
    }
    match input.purpose {
        CollaborationExecutionPurpose::InTurnQuestion if input.parent_turn_id.is_none() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.parent_turn_id",
            });
        }
        CollaborationExecutionPurpose::NaturallyIdleQuestion if input.parent_turn_id.is_some() => {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.parent_turn_id",
            });
        }
        _ => {}
    }
    // Exact replay precedes mutable admission checks: it must not wake or reject a stopped owner.
    // 精确重放先于可变准入检查：不得唤醒停止所有者，也不得拒绝已提交操作的回执。
    let semantic = CollaborationOperationSemantic::BeginQuestionExecution {
        execution_id: input.execution_id.clone(),
        question_id: input.question_id.clone(),
        participant: input.participant.clone(),
        purpose: input.purpose,
        parent_turn_id: input.parent_turn_id,
    };
    if let Some(existing) =
        document.existing_collaboration_operation(&input.operation_id, &semantic)?
    {
        return existing_execution_receipt(existing);
    }
    let inbound = document
        .collaboration_state()
        .inbound_deliveries
        .values()
        .find(|delivery| {
            delivery.question_id == input.question_id
                && delivery.target.same_execution(&input.participant)
                && matches!(
                    delivery.state,
                    CollaborationDeliveryState::Accepted | CollaborationDeliveryState::Projected
                )
        })
        .cloned()
        .ok_or_else(|| DomainError::CollaborationNotFound {
            kind: "accepted_question_delivery",
            identity: input.question_id.as_str().to_string(),
        })?;
    if inbound.execution_purpose != input.purpose || inbound.bound_turn_id != input.parent_turn_id {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.question_execution.binding",
        });
    }
    let run_control = document.collaboration_run_control();
    if matches!(
        run_control,
        CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
    ) || (input.purpose == CollaborationExecutionPurpose::InTurnQuestion
        && run_control == CollaborationRunControl::NaturallyIdle)
        || (input.purpose == CollaborationExecutionPurpose::NaturallyIdleQuestion
            && run_control != CollaborationRunControl::NaturallyIdle)
    {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.run_control.question_execution",
        });
    }
    if document
        .collaboration_state()
        .question_executions
        .contains_key(&input.execution_id)
    {
        return Err(DomainError::CollaborationConflict {
            operation_id: input.operation_id.as_str().to_string(),
        });
    }

    let parent_turn_id = input.parent_turn_id;
    let (execution_turn_id, event_turn_id, local_id_count) = match input.purpose {
        CollaborationExecutionPurpose::InTurnQuestion => {
            let turn_id = parent_turn_id.expect("in-turn question has a parent turn");
            let turn = document.active_turn().ok_or(DomainError::NoActiveTurn)?;
            if turn.closed || turn.turn_id != turn_id {
                return Err(DomainError::ReferenceViolation {
                    kind: "question_execution.parent_turn_id",
                    value: turn_id.value(),
                });
            }
            if turn.origin == TurnOrigin::QuestionExecution {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.question_execution.nested",
                });
            }
            (None, Some(turn_id), 0)
        }
        CollaborationExecutionPurpose::NaturallyIdleQuestion => {
            if document.active_turn().is_some_and(|turn| !turn.closed) {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.question_execution.naturally_idle_active_turn",
                });
            }
            let turn_id = TurnId::try_new(document.id_allocator.frontier().0)?;
            (Some(turn_id), Some(turn_id), 1)
        }
        CollaborationExecutionPurpose::NormalTask => unreachable!(),
    };
    if document.active_api().is_some() {
        return Err(DomainError::NoActiveApi);
    }
    document.ensure_transition_capacity(1, local_id_count)?;

    let execution = CollaborationQuestionExecutionRecord {
        execution_id: input.execution_id.clone(),
        operation_id: input.operation_id.clone(),
        question_id: input.question_id.clone(),
        participant: input.participant,
        purpose: input.purpose,
        parent_turn_id,
        execution_turn_id,
        state: CollaborationQuestionExecutionState::Running,
        started_at_ms: input.started_at_ms,
        completed_at_ms: None,
        outcome: None,
        queued_successor: None,
    };
    let mut state = document.collaboration_state().clone();
    state
        .question_executions
        .insert(input.execution_id.clone(), execution.clone());
    let event_seq = super::next_event_seq(document)?;
    state.operations.insert(
        input.operation_id.clone(),
        CollaborationOperationRecord {
            operation_id: input.operation_id.clone(),
            semantic,
            event_seq,
            result: CollaborationOperationResult::QuestionExecution {
                execution: execution.clone(),
            },
        },
    );
    state.validate()?;
    let (mut staged_allocator, staged_events, committed_event_seq) = stage_question_event(
        document,
        &state,
        input.operation_id,
        CollaborationEventKind::QuestionExecutionStarted,
        CollaborationAfterState::QuestionExecution {
            execution: execution.clone(),
        },
        event_turn_id,
        input.started_at_ms,
    )?;
    if committed_event_seq != event_seq {
        return Err(DomainError::SequenceViolation {
            expected: event_seq.value(),
            actual: committed_event_seq.value(),
        });
    }
    if let Some(turn_id) = execution_turn_id {
        let allocated_turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
        if allocated_turn_id != turn_id {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question_execution.turn_id_allocator",
            });
        }
    }
    let mut metadata = document.metadata.clone();
    metadata.collaboration.state = state;
    metadata.collaboration.last_event_seq = Some(committed_event_seq);
    if execution_turn_id.is_some() {
        metadata.open_turn_start_seq = Some(committed_event_seq);
    }
    metadata.collaboration.validate()?;
    document.id_allocator = staged_allocator;
    document.events.extend(staged_events);
    document.metadata = metadata;
    if let Some(turn_id) = execution_turn_id {
        document.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::QuestionExecution,
            start_seq: committed_event_seq,
            closed: false,
        });
    }
    document.bump_revision(
        &[DirtyFile::Metadata, DirtyFile::ActiveTail],
        input.started_at_ms,
        &[committed_event_seq],
    );
    Ok(IdempotentMutation::Committed(
        CollaborationQuestionExecutionReceipt {
            execution,
            event_seq: committed_event_seq,
        },
    ))
}

impl SessionDocument {
    /// Atomically schedules the FIFO user task after the exact completed standalone question execution.
    /// 在精确独立答疑执行完成后，原子调度 FIFO 用户任务。
    /// `execution_id` fixes the predecessor; `operation_id` deduplicates scheduling; `now_ms` dates the transition.
    /// `execution_id` 固定前驱；`operation_id` 对调度去重；`now_ms` 记录转换时间。
    /// Returns the immutable scheduling receipt, or none when no queued input exists. Never resumes a Goal.
    /// 返回不可变调度回执；无排队输入时返回空。绝不恢复 Goal。
    pub fn schedule_question_queued_successor(
        &mut self,
        execution_id: CollaborationExecutionId,
        operation_id: CollaborationOperationId,
        now_ms: i64,
    ) -> Result<Option<IdempotentMutation<CollaborationQuestionExecutionReceipt>>, DomainError>
    {
        self.apply_atomic_transition(move |document| {
            validate_timestamp("question_successor.scheduled_at_ms", now_ms)?;
            // Semantic identity fixes the predecessor independently of changing queue contents.
            // 语义身份独立于变化的队列内容固定前驱。
            let semantic = CollaborationOperationSemantic::ScheduleQuestionSuccessor {
                execution_id: execution_id.clone(),
            };
            if let Some(existing) =
                document.existing_collaboration_operation(&operation_id, &semantic)?
            {
                return existing_execution_receipt(existing).map(Some);
            }
            // The completed record is retained in metadata even after its event segment is sealed.
            // 即使事件段已封口，完成记录仍保留在元数据中。
            let mut execution = document
                .collaboration_state()
                .question_executions
                .get(&execution_id)
                .cloned()
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "question_execution",
                    identity: execution_id.as_str().into(),
                })?;
            if execution.state != CollaborationQuestionExecutionState::Completed
                || execution.purpose != CollaborationExecutionPurpose::NaturallyIdleQuestion
                || execution.queued_successor.is_some()
                || document.collaboration_run_control() != CollaborationRunControl::NaturallyIdle
                || document.active_api().is_some()
                || document.terminal_settlement_record().is_some()
                || !document
                    .metadata
                    .input_state
                    .pending_in_turn_inputs
                    .is_empty()
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "question_successor.admission",
                });
            }
            // A replacement turn cannot be overwritten, even when the old execution remains queryable.
            // 即使旧执行仍可查询，也不能覆盖替代回合。
            let prior = document.active_turn().ok_or(DomainError::NoActiveTurn)?;
            if !prior.closed
                || prior.origin != TurnOrigin::QuestionExecution
                || Some(prior.turn_id) != execution.execution_turn_id
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "question_successor.predecessor",
                });
            }
            // Only the exact FIFO head is consumed; an empty queue creates no receipt or revision.
            // 只消费精确 FIFO 队首；空队列不创建回执或修订。
            let Some(queued) = document.metadata.input_state.queued_inputs.first().cloned() else {
                return Ok(None);
            };
            super::super::input::validate_user_input(queued.text.clone(), &queued.resource_ids)?;
            document.ensure_transition_capacity(
                3 + u64::from(queued.bound_request_config.is_some()),
                1,
            )?;
            // Allocate a fresh business turn while preserving the accepted user input identity.
            // 分配新业务回合，同时保留已接纳用户输入身份。
            let mut allocator = document.id_allocator;
            // The input and its bound model configuration are staged in exact stream order.
            // 输入及其绑定模型配置按精确流顺序暂存。
            let mut events = Vec::new();
            // Queue removal and active-turn publication share the same metadata candidate.
            // 队列移除和活动回合发布共享同一元数据候选。
            let mut metadata = document.metadata.clone();
            // This is a genuine new user task, not a continuation of the auxiliary turn.
            // 这是真正的新用户任务，不是辅助回合的继续。
            let turn_id = TurnId::try_new(allocator.next_local_id()?)?;
            if let Some(config) = queued.bound_request_config.clone() {
                document.stage_bound_request_activation(
                    &mut metadata,
                    &mut allocator,
                    &mut events,
                    config,
                    Some(turn_id),
                    now_ms,
                )?;
            }
            // The durable input event is also the sole provider and dispatch payload authority.
            // 可靠输入事件也是供应商与分发载荷的唯一权威。
            let input_seq = document.stage_event(
                &mut allocator,
                &mut events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(crate::v1::event::UserInputEvent {
                    parameters: queued.parameters,
                    client_action_id: queued.client_action_id.clone(),
                    input_id: queued.input_id,
                    input_source: crate::v1::event::UserInputSource::QueuedScheduled,
                    text: queued.text,
                    resource_ids: queued.resource_ids,
                }),
                now_ms,
            )?;
            metadata.input_state.queued_inputs.remove(0);
            metadata.open_turn_start_seq = Some(input_seq);
            // All newly staged event ids are published by the outer atomic transaction once.
            // 外层原子事务一次发布所有新暂存事件身份。
            let changed = events
                .iter()
                .map(ConversationEvent::seq)
                .collect::<Vec<_>>();
            document.id_allocator = allocator;
            document.events.extend(events);
            document.metadata = metadata;
            document.active_turn = Some(ActiveTurnState {
                turn_id,
                origin: TurnOrigin::UserRoot,
                start_seq: input_seq,
                closed: false,
            });
            document.bump_revision(
                &[DirtyFile::Metadata, DirtyFile::ActiveTail],
                now_ms,
                &changed,
            );
            execution.operation_id = operation_id.clone();
            execution.queued_successor = Some(CollaborationQueuedUserSuccessor {
                turn_id,
                input_id: queued.input_id,
                client_action_id: queued.client_action_id,
                input_seq,
            });
            // Preserve an immutable scheduling result for retries after dispatch or process restart.
            // 为分发或进程重启后的重试保留不可变调度结果。
            let mut state = document.collaboration_state().clone();
            state
                .question_executions
                .insert(execution_id, execution.clone());
            // The operation receipt points to its own host event, not the preceding user input.
            // 操作回执指向自身宿主事件，而非前一条用户输入。
            let event_seq = super::next_event_seq(document)?;
            state.operations.insert(
                operation_id.clone(),
                CollaborationOperationRecord {
                    operation_id: operation_id.clone(),
                    semantic,
                    event_seq,
                    result: CollaborationOperationResult::QuestionExecution {
                        execution: execution.clone(),
                    },
                },
            );
            document.commit_collaboration_event(
                state,
                operation_id.clone(),
                CollaborationEventKind::QuestionSuccessorScheduled,
                CollaborationAfterState::QuestionExecution {
                    execution: execution.clone(),
                },
                now_ms,
            )?;
            document.set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "{}:running",
                    operation_id.as_str()
                ))?,
                state: CollaborationRunControl::Running,
                changed_at_ms: now_ms,
            })?;
            Ok(Some(IdempotentMutation::Committed(
                CollaborationQuestionExecutionReceipt {
                    execution,
                    event_seq,
                },
            )))
        })
    }
}

/// Completes a question execution and closes only its auxiliary turn.
/// 完成问题执行且只闭合其辅助回合。
pub(super) fn complete_question_execution(
    document: &mut SessionDocument,
    input: CompleteQuestionExecutionInput,
) -> Result<IdempotentMutation<CollaborationQuestionExecutionReceipt>, DomainError> {
    validate_timestamp(
        "collaboration.question_execution.completed_at_ms",
        input.completed_at_ms,
    )?;
    let semantic = CollaborationOperationSemantic::CompleteQuestionExecution {
        execution_id: input.execution_id.clone(),
        outcome: input.outcome,
    };
    if let Some(existing) =
        document.existing_collaboration_operation(&input.operation_id, &semantic)?
    {
        return existing_execution_receipt(existing);
    }
    let mut execution = document
        .collaboration_state()
        .question_executions
        .get(&input.execution_id)
        .cloned()
        .ok_or_else(|| DomainError::CollaborationNotFound {
            kind: "question_execution",
            identity: input.execution_id.as_str().to_string(),
        })?;
    if execution.state != CollaborationQuestionExecutionState::Running {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.question_execution.completion_state",
        });
    }
    execution.outcome = Some(input.outcome);
    validate_settled_question(document, &execution)?;
    let execution_turn_id = execution.execution_turn_id;
    let parent_turn_id = execution.parent_turn_id;
    let close_auxiliary_turn = execution_turn_id.is_some();
    let active_turn = document.active_turn().cloned();
    let active_turn_id = execution_turn_id.or(parent_turn_id);
    let Some(active_turn_id) = active_turn_id else {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.question_execution.turn_binding",
        });
    };
    let current_turn = active_turn.as_ref().ok_or(DomainError::NoActiveTurn)?;
    if current_turn.closed || current_turn.turn_id != active_turn_id {
        return Err(DomainError::ReferenceViolation {
            kind: "question_execution.turn_id",
            value: active_turn_id.value(),
        });
    }
    if close_auxiliary_turn && current_turn.origin != TurnOrigin::QuestionExecution {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.question_execution.turn_origin",
        });
    }
    if document.active_api().is_some() {
        return Err(DomainError::TurnCompletionWhileApiActive {
            turn_id: active_turn_id.value(),
            api_call_id: document
                .active_api()
                .expect("active API checked above")
                .api_call_id
                .value(),
        });
    }

    execution.operation_id = input.operation_id.clone();
    execution.state = CollaborationQuestionExecutionState::Completed;
    execution.completed_at_ms = Some(input.completed_at_ms);
    let mut state = document.collaboration_state().clone();
    state
        .question_executions
        .insert(input.execution_id.clone(), execution.clone());
    let event_seq = super::next_event_seq(document)?;
    state.operations.insert(
        input.operation_id.clone(),
        CollaborationOperationRecord {
            operation_id: input.operation_id.clone(),
            semantic,
            event_seq,
            result: CollaborationOperationResult::QuestionExecution {
                execution: execution.clone(),
            },
        },
    );
    state.validate()?;

    if close_auxiliary_turn {
        let expiring_contexts =
            super::super::turns::expiring_ttl_contexts(document, active_turn_id)?;
        let ttl_count = u64::try_from(expiring_contexts.len()).map_err(|_| {
            DomainError::ImmutableFieldMutation {
                entity: "ttl_expiration",
                field: "event_count",
            }
        })?;
        document.ensure_transition_capacity(3 + ttl_count, 0)?;
    } else {
        document.ensure_transition_capacity(1, 0)?;
    }
    let (staged_allocator, mut staged_events, committed_event_seq) = stage_question_event(
        document,
        &state,
        input.operation_id,
        CollaborationEventKind::QuestionExecutionCompleted,
        CollaborationAfterState::QuestionExecution {
            execution: execution.clone(),
        },
        Some(active_turn_id),
        input.completed_at_ms,
    )?;
    if committed_event_seq != event_seq {
        return Err(DomainError::SequenceViolation {
            expected: event_seq.value(),
            actual: committed_event_seq.value(),
        });
    }

    let mut metadata = document.metadata.clone();
    metadata.collaboration.state = state;
    metadata.collaboration.last_event_seq = Some(committed_event_seq);
    let mut staged_allocator = staged_allocator;
    let _terminal_event_seq = if close_auxiliary_turn {
        let start_seq = current_turn.start_seq;
        let start_event = document
            .events()
            .iter()
            .find(|event| event.seq == start_seq)
            .ok_or(DomainError::ReferenceViolation {
                kind: "question_execution.turn_start_event",
                value: start_seq.value(),
            })?;
        let duration_ms = input
            .completed_at_ms
            .saturating_sub(start_event.updated_at_ms)
            .max(0);
        let buckets = document.aggregate_turn_buckets(active_turn_id, start_seq)?;
        let terminal_event_seq = document.stage_turn_closure(
            &mut staged_allocator,
            &mut staged_events,
            active_turn_id,
            start_seq,
            match input.outcome {
                CollaborationQuestionExecutionOutcome::Settled
                | CollaborationQuestionExecutionOutcome::Unanswered => {
                    crate::v1::event::TurnClosureReason::Completed
                }
                CollaborationQuestionExecutionOutcome::Failed => {
                    crate::v1::event::TurnClosureReason::Failed
                }
            },
            duration_ms,
            input.completed_at_ms,
            buckets,
        )?;
        super::super::turns::clear_expired_dynamic_context_metadata(
            &mut metadata,
            &super::super::turns::expiring_ttl_contexts(document, active_turn_id)?,
        );
        metadata.open_turn_start_seq = None;
        Some(terminal_event_seq)
    } else {
        None
    };
    metadata.collaboration.validate()?;
    let changed_event_seqs = staged_events
        .iter()
        .map(ConversationEvent::seq)
        .collect::<Vec<_>>();
    document.id_allocator = staged_allocator;
    document.events.extend(staged_events);
    document.metadata = metadata;
    if close_auxiliary_turn {
        let mut closed_turn = current_turn.clone();
        closed_turn.closed = true;
        document.active_turn = Some(closed_turn);
    }
    document.bump_revision(
        &[DirtyFile::Metadata, DirtyFile::ActiveTail],
        input.completed_at_ms,
        &changed_event_seqs,
    );
    Ok(IdempotentMutation::Committed(
        CollaborationQuestionExecutionReceipt {
            execution,
            event_seq: committed_event_seq,
        },
    ))
}
