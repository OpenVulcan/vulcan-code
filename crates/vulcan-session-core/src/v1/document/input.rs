//! Client input identities, durable queue transitions, and matching input history validation.
//! 客户端输入身份、可靠队列转换及对应输入历史校验。

use super::persistence::DirtyFile;
use super::{
    ActiveTurnState, IdempotentMutation, SessionDocument, TurnOrigin, client_action_conflict,
    validate_timestamp,
};
use crate::v1::config::{
    DraftState, MetadataDocument, PendingInTurnInput, QueuedInput, QueuedInputIdentity,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ConversationEvent, EventBody, EventVisibility, ExecutionControlKind,
    QueuedInputResolutionEvent, QueuedInputResolutionKind, SystemContextOperation, TurnOutcome,
    UserInputEvent, UserInputSource,
};
use crate::v1::ids::{ClientActionId, PendingInputId, ResourceId, Seq, TurnId};
use std::collections::{BTreeMap, BTreeSet};

/// One client-authored input carrying its durable RPC idempotency identity.
/// 一条携带持久化 RPC 幂等身份的客户端输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInputSubmission {
    /// Stable external idempotency identity supplied by the client.
    /// 客户端提供的稳定外部幂等身份。
    pub client_action_id: ClientActionId,
    /// Exact user text without normalization.
    /// 未经归一化的精确用户正文。
    pub text: String,
    /// Ordered project resource identities.
    /// 有序项目资源身份。
    pub resource_ids: Vec<ResourceId>,
}

impl ClientInputSubmission {
    /// Builds one strongly typed client input submission.
    /// 构建一条强类型客户端输入提交。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` is the external RPC idempotency key; `text` and
    /// `resource_ids` are the exact model-visible input payload.
    /// `client_action_id` 是外部 RPC 幂等键；`text` 与 `resource_ids` 是精确的模型可见输入载荷。
    ///
    /// # Returns
    /// 返回值
    /// Returns a submission whose external identity is already validated.
    /// 返回外部身份已经过校验的提交。
    pub fn new(
        client_action_id: ClientActionId,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
    ) -> Self {
        Self {
            client_action_id,
            text: text.into(),
            resource_ids,
        }
    }
}

/// Durable disposition selected while accepting one future-turn input.
/// 接受一条未来回合输入时选定的持久化处置结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueInputDisposition {
    /// The input remains in the metadata FIFO queue.
    /// 输入继续保留在元数据 FIFO 队列中。
    Queued,
    /// The input atomically superseded an undispatched Goal continuation and opened a UserRoot Turn.
    /// 输入原子取代了尚未派发的 Goal 续跑，并开启一个 UserRoot 回合。
    SupersededGoalAndScheduled {
        /// Goal-continuation Turn closed by the user input.
        /// 被用户输入闭合的 Goal 续跑回合。
        stopped_turn_id: TurnId,
        /// Unique terminal TurnStats seq of the stopped Goal Turn.
        /// 被停止 Goal 回合的唯一终态 TurnStats seq。
        stopped_turn_stats_seq: Seq,
        /// Newly opened UserRoot Turn.
        /// 新开启的 UserRoot 回合。
        scheduled_turn_id: TurnId,
        /// Provider-visible seq of the scheduled user input.
        /// 已调度用户输入的 Provider 可见 seq。
        input_seq: Seq,
    },
}

/// Durable receipt of one idempotent enqueue request.
/// 一次幂等排队请求的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueInputReceipt {
    /// Exact external client action identity.
    /// 精确外部客户端动作身份。
    pub client_action_id: ClientActionId,
    /// Stable session-local pending-input identity.
    /// 稳定的会话本地待处理输入身份。
    pub input_id: PendingInputId,
    /// Authoritative queue or immediate-scheduling disposition.
    /// 权威排队或立即调度处置结果。
    pub disposition: EnqueueInputDisposition,
}

/// Durable receipt of one draft-state mutation.
/// 一次草稿状态修改的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftMutationReceipt {
    /// Exact draft state after the mutation.
    /// 修改后的精确草稿状态。
    pub draft: DraftState,
}

/// Durable receipt of restoring one queued input into the composer draft.
/// 把一条排队输入恢复到输入区草稿的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputDraftReceipt {
    /// Exact queue identity moved into the draft.
    /// 被移动到草稿的精确队列身份。
    pub identity: QueuedInputIdentity,
    /// Exact draft state published by the transition.
    /// 该转换发布的精确草稿状态。
    pub draft: DraftState,
}

/// Durable receipt of removing one queued input.
/// 移除一条排队输入的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputRemovalReceipt {
    /// Exact queue identity removed from metadata.
    /// 从元数据中移除的精确队列身份。
    pub identity: QueuedInputIdentity,
}

/// Durable receipt of guiding one queued input into the currently open logical Turn.
/// 把一条排队输入引导进当前开放逻辑回合的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputPendingTurnReceipt {
    /// Exact queue identity transferred without allocating a replacement input id.
    /// 无需分配替代输入 id 即完成转移的精确队列身份。
    pub identity: QueuedInputIdentity,
    /// Open logical Turn that now owns the pending input.
    /// 当前持有该过程内输入的开放逻辑回合。
    pub turn_id: TurnId,
    /// UiOnly ledger seq that durably records the queue resolution.
    /// 持久记录该队列处置的 UiOnly 账本 seq。
    pub resolution_seq: Seq,
}

/// Durable receipt of one complete FIFO queue reorder.
/// 一次完整 FIFO 队列重排的持久化 receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputOrderReceipt {
    /// Exact complete queue identity order after the mutation.
    /// 修改后的精确完整队列身份顺序。
    pub ordered: Vec<QueuedInputIdentity>,
}

/// Validates that one user input carries model-visible text or at least one resource.
/// 校验一条用户输入至少携带模型可见文本或一个资源。
pub(super) fn validate_user_input(
    text: String,
    resource_ids: &[ResourceId],
) -> Result<String, DomainError> {
    if text.trim().is_empty() && resource_ids.is_empty() {
        return Err(DomainError::EmptyUserInput);
    }
    Ok(text)
}

/// Returns the external client action id carried by one event, when any.
/// 返回一条事件携带的外部客户端动作 id（如有）。
pub(super) fn event_client_action_id(event: &ConversationEvent) -> Option<&ClientActionId> {
    match &event.event {
        EventBody::UserInput(input) => Some(&input.client_action_id),
        EventBody::ExecutionControl(control) => control.client_action_id.as_ref(),
        _ => None,
    }
}

/// Copies the exact durable identity of one queued metadata item.
/// 复制一条排队元数据条目的精确持久化身份。
fn queued_input_identity(queued: &QueuedInput) -> QueuedInputIdentity {
    QueuedInputIdentity {
        client_action_id: queued.client_action_id.clone(),
        input_id: queued.input_id,
    }
}

/// Merges one durable queue identity without accepting identity drift.
/// 合并一条持久化队列身份，且不接受身份漂移。
fn merge_queued_input_identity(
    resolved: &mut Option<QueuedInputIdentity>,
    candidate: QueuedInputIdentity,
) -> Result<(), DomainError> {
    if let Some(existing) = resolved {
        if *existing != candidate {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "queued_action_identity_drift",
            });
        }
        return Ok(());
    }
    *resolved = Some(candidate);
    Ok(())
}

/// Clears the staged draft only when it is the exact payload being admitted.
/// 仅当 staged 草稿与正在准入的载荷完全一致时才清空草稿。
pub(super) fn consume_matching_draft(
    metadata: &mut MetadataDocument,
    text: &str,
    resource_ids: &[ResourceId],
) -> bool {
    if metadata.input_state.draft.text != text
        || metadata.input_state.draft.resource_ids != resource_ids
    {
        return false;
    }
    metadata.input_state.draft = DraftState::default();
    true
}

/// Validates global client-action uniqueness and continuation pair structure.
/// 校验全局客户端动作唯一性与继续事件对结构。
pub(super) fn validate_client_action_history(
    metadata: &MetadataDocument,
    events: &[ConversationEvent],
) -> Result<(), DomainError> {
    let mut metadata_ids = BTreeSet::new();
    for client_action_id in metadata
        .input_state
        .pending_in_turn_inputs
        .iter()
        .map(|pending| &pending.client_action_id)
        .chain(
            metadata
                .input_state
                .queued_inputs
                .iter()
                .map(|queued| &queued.client_action_id),
        )
        .chain(
            metadata
                .input_state
                .draft
                .restored_from_queue
                .iter()
                .map(|identity| &identity.client_action_id),
        )
    {
        if !metadata_ids.insert(client_action_id.clone()) {
            return Err(DomainError::InvalidContinuationHistory {
                field: "duplicate_metadata_client_action_id",
            });
        }
    }

    let mut grouped: BTreeMap<ClientActionId, Vec<&ConversationEvent>> = BTreeMap::new();
    for event in events {
        if let EventBody::UserInput(input) = &event.event {
            validate_user_input(input.text.clone(), &input.resource_ids)?;
        }
        if let Some(client_action_id) = event_client_action_id(event) {
            if metadata_ids.contains(client_action_id) {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "client_action_in_metadata_and_stream",
                });
            }
            grouped
                .entry(client_action_id.clone())
                .or_default()
                .push(event);
        }
    }

    for action_events in grouped.values() {
        let mut input: Option<&ConversationEvent> = None;
        let mut requested: Option<&ConversationEvent> = None;
        let mut resumed: Option<&ConversationEvent> = None;
        for event in action_events {
            match &event.event {
                EventBody::UserInput(_) => {
                    if input.replace(event).is_some() {
                        return Err(DomainError::InvalidContinuationHistory {
                            field: "duplicate_stream_client_action_id",
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
                _ => {
                    return Err(DomainError::InvalidContinuationHistory {
                        field: "unexpected_client_action_event",
                    });
                }
            }
        }
        if requested.is_none() && resumed.is_none() {
            if input.is_none() {
                return Err(DomainError::InvalidContinuationHistory {
                    field: "empty_client_action",
                });
            }
            continue;
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
        if let Some(input) = input
            && (input.turn_id != Some(turn_id) || input.seq.value() >= requested.seq.value())
        {
            return Err(DomainError::InvalidContinuationHistory {
                field: "continue_input_order",
            });
        }
    }
    Ok(())
}

/// Validates durable queued-input terminal receipts against current metadata and stream identities.
/// 校验持久化排队输入终结 receipt 与当前元数据及事件流身份的一致性。
pub(super) fn validate_queued_input_resolution_history(
    metadata: &MetadataDocument,
    events: &[ConversationEvent],
) -> Result<(), DomainError> {
    let mut actions = BTreeSet::new();
    let mut input_ids = BTreeSet::new();
    for event in events {
        let EventBody::QueuedInputResolution(resolution) = &event.event else {
            continue;
        };
        validate_user_input(resolution.text.clone(), &resolution.resource_ids)?;
        if event.visibility != EventVisibility::UiOnly {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_event_shape",
            });
        }
        if !actions.insert(resolution.client_action_id.clone())
            || !input_ids.insert(resolution.input_id)
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "duplicate_resolution_identity",
            });
        }
        if metadata.input_state.queued_inputs.iter().any(|queued| {
            queued.client_action_id == resolution.client_action_id
                || queued.input_id == resolution.input_id
        }) {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_identity_still_pending",
            });
        }
        match resolution.resolution {
            QueuedInputResolutionKind::MovedToDraft | QueuedInputResolutionKind::Removed => {
                if event.turn_id.is_some() {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "terminal_resolution_turn_id",
                    });
                }
                if metadata
                    .input_state
                    .pending_in_turn_inputs
                    .iter()
                    .any(|pending| {
                        pending.client_action_id == resolution.client_action_id
                            || pending.input_id == resolution.input_id
                    })
                {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "resolution_identity_still_pending",
                    });
                }
                if events.iter().any(|candidate| {
                    matches!(
                        &candidate.event,
                        EventBody::UserInput(input)
                            if input.client_action_id == resolution.client_action_id
                                || input.input_id == resolution.input_id
                    )
                }) {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "resolution_identity_already_scheduled",
                    });
                }
                if let Some(draft_identity) =
                    metadata.input_state.draft.restored_from_queue.as_ref()
                    && (draft_identity.client_action_id == resolution.client_action_id
                        || draft_identity.input_id == resolution.input_id)
                {
                    let exact_moved_draft = resolution.resolution
                        == QueuedInputResolutionKind::MovedToDraft
                        && draft_identity.client_action_id == resolution.client_action_id
                        && draft_identity.input_id == resolution.input_id
                        && metadata.input_state.draft.text == resolution.text
                        && metadata.input_state.draft.resource_ids == resolution.resource_ids;
                    if !exact_moved_draft {
                        return Err(DomainError::InvalidQueuedInputOrder {
                            field: "resolution_draft_identity_conflict",
                        });
                    }
                }
            }
            QueuedInputResolutionKind::PromotedToPendingInTurn => {
                let turn_id = event.turn_id.ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "promoted_resolution_turn_id",
                })?;
                if metadata
                    .input_state
                    .draft
                    .restored_from_queue
                    .as_ref()
                    .is_some_and(|draft_identity| {
                        draft_identity.client_action_id == resolution.client_action_id
                            || draft_identity.input_id == resolution.input_id
                    })
                {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "promoted_resolution_draft_conflict",
                    });
                }
                let mut exact_pending = false;
                for pending in &metadata.input_state.pending_in_turn_inputs {
                    let action_matches = pending.client_action_id == resolution.client_action_id;
                    let input_matches = pending.input_id == resolution.input_id;
                    if action_matches && input_matches {
                        if pending.turn_id != turn_id
                            || pending.text != resolution.text
                            || pending.resource_ids != resolution.resource_ids
                            || exact_pending
                        {
                            return Err(DomainError::InvalidQueuedInputOrder {
                                field: "promoted_resolution_pending_conflict",
                            });
                        }
                        exact_pending = true;
                    } else if action_matches || input_matches {
                        return Err(DomainError::InvalidQueuedInputOrder {
                            field: "promoted_resolution_pending_conflict",
                        });
                    }
                }
                let mut exact_streamed = false;
                for candidate in events {
                    let EventBody::UserInput(input) = &candidate.event else {
                        continue;
                    };
                    let action_matches = input.client_action_id == resolution.client_action_id;
                    let input_matches = input.input_id == resolution.input_id;
                    if action_matches && input_matches {
                        if candidate.turn_id != Some(turn_id)
                            || input.input_source != UserInputSource::PendingInTurn
                            || input.text != resolution.text
                            || input.resource_ids != resolution.resource_ids
                            || exact_streamed
                        {
                            return Err(DomainError::InvalidQueuedInputOrder {
                                field: "promoted_resolution_stream_conflict",
                            });
                        }
                        exact_streamed = true;
                    } else if action_matches || input_matches {
                        return Err(DomainError::InvalidQueuedInputOrder {
                            field: "promoted_resolution_stream_conflict",
                        });
                    }
                }
                if exact_pending == exact_streamed {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "promoted_resolution_owner_count",
                    });
                }
            }
        }
        if resolution.resolution != QueuedInputResolutionKind::MovedToDraft
            && metadata
                .input_state
                .draft
                .restored_from_queue
                .as_ref()
                .is_some_and(|draft_identity| {
                    draft_identity.client_action_id == resolution.client_action_id
                        || draft_identity.input_id == resolution.input_id
                })
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_draft_identity_conflict",
            });
        }
        if resolution.resolution == QueuedInputResolutionKind::MovedToDraft
            && let Some(draft_identity) = metadata.input_state.draft.restored_from_queue.as_ref()
            && (draft_identity.client_action_id == resolution.client_action_id
                || draft_identity.input_id == resolution.input_id)
            && !(draft_identity.client_action_id == resolution.client_action_id
                && draft_identity.input_id == resolution.input_id
                && metadata.input_state.draft.text == resolution.text
                && metadata.input_state.draft.resource_ids == resolution.resource_ids)
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "resolution_draft_identity_conflict",
            });
        }
    }
    Ok(())
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Submits a root user prompt as a fresh turn.
    /// 以全新回合提交一条根用户提示词。
    pub fn submit_root_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<TurnId>, DomainError> {
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        if let Some(existing_turn_id) =
            self.existing_root_submission(&client_action_id, &text, &resource_ids)?
        {
            return Ok(IdempotentMutation::Idempotent(existing_turn_id));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        // A user can supersede a retained late return that has not started inference, including after Stop.
        // 用户可取代尚未启动推理的迟到回流，包括停止后保留的回流。
        let undispatched_return = self
            .active_turn
            .as_ref()
            .filter(|turn| {
                !turn.closed
                    && turn.origin == TurnOrigin::SubagentReturn
                    && self.active_api.is_none()
                    && !self.events.iter().any(|event| {
                        event.turn_id == Some(turn.turn_id)
                            && matches!(&event.event, EventBody::ApiStarted(_))
                    })
            })
            .cloned();
        if let Some(turn) = &self.active_turn
            && !turn.closed
            && undispatched_return.is_none()
        {
            return Err(DomainError::RootInputWhileTurnOpen {
                turn_id: turn.turn_id.value(),
            });
        }
        self.ensure_transition_capacity(if undispatched_return.is_some() { 3 } else { 1 }, 2)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::with_capacity(1);
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        if let Some(return_turn) = undispatched_return {
            // Preserve the return and its statistics as history before admitting the actual user root.
            // 准入真实用户根输入前，将回流及其统计完整保留为历史。
            let buckets =
                self.aggregate_turn_buckets(return_turn.turn_id, return_turn.start_seq)?;
            self.stage_turn_closure(
                &mut staged_allocator,
                &mut staged_events,
                return_turn.turn_id,
                return_turn.start_seq,
                TurnOutcome::Stopped,
                0,
                now_ms,
                buckets,
            )?;
        }
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                client_action_id,
                input_id,
                input_source: UserInputSource::Root,
                text,
                resource_ids,
            }),
            now_ms,
        )?;
        // changed_event_seqs also carries a superseded return boundary when one was closed above.
        // changed_event_seqs 同时携带上方被取代的回流闭合边界。
        let changed_event_seqs = staged_events
            .iter()
            .map(ConversationEvent::seq)
            .collect::<Vec<_>>();
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::UserRoot,
            start_seq: seq,
            closed: false,
        });
        self.metadata.open_turn_start_seq = Some(seq);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(IdempotentMutation::Committed(turn_id))
    }

    /// Queues one future-turn input into metadata without touching the stream.
    /// 把一条未来回合输入写入 metadata 队列，不触碰主事件流。
    pub fn enqueue_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<EnqueueInputReceipt>, DomainError> {
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        if let Some(existing_receipt) =
            self.existing_enqueue_receipt(&client_action_id, &text, &resource_ids)?
        {
            return Ok(IdempotentMutation::Idempotent(existing_receipt));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        validate_timestamp("updated_at_ms", now_ms)?;

        let undispatched_goal_turn = self.active_turn.as_ref().filter(|turn| {
            !turn.closed
                && turn.origin == TurnOrigin::GoalContinuation
                && self.active_api.is_none()
                && !self.events.iter().any(|event| {
                    event.turn_id == Some(turn.turn_id)
                        && matches!(event.event, EventBody::ApiStarted(_))
                })
        });
        if let Some(goal_turn) = undispatched_goal_turn.cloned() {
            if !self.metadata.input_state.queued_inputs.is_empty() {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "undispatched_goal_existing_queue",
                });
            }
            let buckets = self.aggregate_turn_buckets(goal_turn.turn_id, goal_turn.start_seq)?;
            self.ensure_transition_capacity(3, 2)?;
            let mut staged_allocator = self.id_allocator;
            let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
            let scheduled_turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let mut staged_events = Vec::with_capacity(3);
            let mut staged_metadata = self.metadata.clone();
            consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
            let stopped_turn_stats_seq = self.stage_turn_closure(
                &mut staged_allocator,
                &mut staged_events,
                goal_turn.turn_id,
                goal_turn.start_seq,
                TurnOutcome::Stopped,
                0,
                now_ms,
                buckets,
            )?;
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(scheduled_turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    client_action_id: client_action_id.clone(),
                    input_id,
                    input_source: UserInputSource::QueuedScheduled,
                    text,
                    resource_ids,
                }),
                now_ms,
            )?;
            let changed_event_seqs = staged_events
                .iter()
                .map(ConversationEvent::seq)
                .collect::<Vec<_>>();
            self.id_allocator = staged_allocator;
            self.metadata = staged_metadata;
            self.events.extend(staged_events);
            self.active_turn = Some(ActiveTurnState {
                turn_id: scheduled_turn_id,
                origin: TurnOrigin::UserRoot,
                start_seq: input_seq,
                closed: false,
            });
            self.metadata.open_turn_start_seq = Some(input_seq);
            self.bump_revision(
                &[DirtyFile::Metadata, DirtyFile::ActiveTail],
                now_ms,
                &changed_event_seqs,
            );
            return Ok(IdempotentMutation::Committed(EnqueueInputReceipt {
                client_action_id,
                input_id,
                disposition: EnqueueInputDisposition::SupersededGoalAndScheduled {
                    stopped_turn_id: goal_turn.turn_id,
                    stopped_turn_stats_seq,
                    scheduled_turn_id,
                    input_seq,
                },
            }));
        }

        self.ensure_transition_capacity(0, 1)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        staged_metadata.input_state.queued_inputs.push(QueuedInput {
            client_action_id: client_action_id.clone(),
            input_id,
            text,
            resource_ids,
            created_at_ms: now_ms,
        });
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(EnqueueInputReceipt {
            client_action_id,
            input_id,
            disposition: EnqueueInputDisposition::Queued,
        }))
    }

    /// Replaces the durable composer draft with exact text and ordered resources.
    /// 使用精确正文与有序资源替换持久化输入区草稿。
    ///
    /// # Parameters
    /// 参数
    /// `text` and `resource_ids` are preserved exactly; `now_ms` is the explicit mutation boundary.
    /// `text` 与 `resource_ids` 被原样保留；`now_ms` 是显式修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or already-current draft without manufacturing a revision for a no-op.
    /// 返回已提交或已经是当前值的草稿；no-op 不制造修订号。
    pub fn set_draft(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<DraftMutationReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let text = text.into();
        if self.metadata.input_state.draft.text == text
            && self.metadata.input_state.draft.resource_ids == resource_ids
        {
            return Ok(IdempotentMutation::Idempotent(DraftMutationReceipt {
                draft: self.metadata.input_state.draft.clone(),
            }));
        }
        let draft = DraftState {
            text,
            resource_ids,
            restored_from_queue: None,
        };
        self.metadata.input_state.draft = draft.clone();
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(DraftMutationReceipt {
            draft,
        }))
    }

    /// Clears the durable composer draft without touching queued inputs.
    /// 清空持久化输入区草稿，且不修改排队输入。
    ///
    /// # Parameters
    /// 参数
    /// `now_ms` is the explicit mutation boundary.
    /// `now_ms` 是显式修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or already-empty draft state.
    /// 返回已提交或已经为空的草稿状态。
    pub fn clear_draft(
        &mut self,
        now_ms: i64,
    ) -> Result<IdempotentMutation<DraftMutationReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let draft = DraftState::default();
        if self.metadata.input_state.draft == draft {
            return Ok(IdempotentMutation::Idempotent(DraftMutationReceipt {
                draft,
            }));
        }
        self.metadata.input_state.draft = draft.clone();
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(DraftMutationReceipt {
            draft,
        }))
    }

    /// Atomically moves one exact queued input into the durable composer draft.
    /// 原子地把一条精确排队输入移动到持久化输入区草稿。
    ///
    /// # Parameters
    /// 参数
    /// `identity` must match both the external action and local input id; `now_ms` is the mutation boundary.
    /// `identity` 必须同时匹配外部动作与本地输入 id；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact restored draft receipt, including an idempotent retry after reload.
    /// 返回精确恢复草稿 receipt，包括重载后的幂等重试。
    pub fn move_queued_input_to_draft(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputDraftReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(resolution) = self.existing_queued_input_resolution(&identity)? {
            if resolution.resolution != QueuedInputResolutionKind::MovedToDraft {
                return Err(client_action_conflict(&identity.client_action_id));
            }
            return Ok(IdempotentMutation::Idempotent(QueuedInputDraftReceipt {
                identity,
                draft: DraftState {
                    text: resolution.text.clone(),
                    resource_ids: resolution.resource_ids.clone(),
                    restored_from_queue: Some(QueuedInputIdentity {
                        client_action_id: resolution.client_action_id.clone(),
                        input_id: resolution.input_id,
                    }),
                },
            }));
        }
        let index = self.require_queued_input_index(&identity)?;
        let queued = self.metadata.input_state.queued_inputs[index].clone();
        let draft = DraftState {
            text: queued.text.clone(),
            resource_ids: queued.resource_ids.clone(),
            restored_from_queue: Some(identity.clone()),
        };
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let resolution_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            EventBody::QueuedInputResolution(QueuedInputResolutionEvent {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id,
                resolution: QueuedInputResolutionKind::MovedToDraft,
                text: queued.text,
                resource_ids: queued.resource_ids,
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.queued_inputs.remove(index);
        staged_metadata.input_state.draft = draft.clone();
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[resolution_seq],
        );
        Ok(IdempotentMutation::Committed(QueuedInputDraftReceipt {
            identity,
            draft,
        }))
    }

    /// Atomically removes one exact queued input from authoritative metadata.
    /// 从权威元数据中原子移除一条精确排队输入。
    ///
    /// # Parameters
    /// 参数
    /// `identity` must match both durable ids and `now_ms` is the mutation boundary.
    /// `identity` 必须同时匹配两个持久化 id；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or previously persisted removal receipt.
    /// 返回已提交或此前已持久化的移除 receipt。
    pub fn remove_queued_input(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputRemovalReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(resolution) = self.existing_queued_input_resolution(&identity)? {
            if resolution.resolution != QueuedInputResolutionKind::Removed {
                return Err(client_action_conflict(&identity.client_action_id));
            }
            return Ok(IdempotentMutation::Idempotent(QueuedInputRemovalReceipt {
                identity,
            }));
        }
        let index = self.require_queued_input_index(&identity)?;
        let queued = self.metadata.input_state.queued_inputs[index].clone();
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let resolution_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            EventBody::QueuedInputResolution(QueuedInputResolutionEvent {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id,
                resolution: QueuedInputResolutionKind::Removed,
                text: queued.text,
                resource_ids: queued.resource_ids,
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.queued_inputs.remove(index);
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[resolution_seq],
        );
        Ok(IdempotentMutation::Committed(QueuedInputRemovalReceipt {
            identity,
        }))
    }

    /// Atomically transfers one queued future input into the currently open logical Turn.
    /// 原子地把一条未来排队输入转移进当前开放逻辑回合。
    ///
    /// # Parameters
    /// 参数
    /// `identity` must match the durable queue item exactly; `now_ms` is the mutation boundary.
    /// `identity` 必须精确匹配持久化队列条目；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or previously persisted transfer receipt without allocating a new input id.
    /// 返回已提交或此前已持久化的转移 receipt，且不会分配新的输入 id。
    pub fn promote_queued_input_to_pending_turn(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputPendingTurnReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if let Some(resolution) = self.existing_queued_input_resolution(&identity)? {
            if resolution.resolution != QueuedInputResolutionKind::PromotedToPendingInTurn {
                return Err(client_action_conflict(&identity.client_action_id));
            }
            let resolution_event = self
                .events
                .iter()
                .find(|event| {
                    matches!(
                        &event.event,
                        EventBody::QueuedInputResolution(candidate)
                            if candidate.client_action_id == identity.client_action_id
                                && candidate.input_id == identity.input_id
                    )
                })
                .ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "promoted_resolution_event_missing",
                })?;
            let turn_id = resolution_event
                .turn_id
                .ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "promoted_resolution_turn_id",
                })?;
            return Ok(IdempotentMutation::Idempotent(
                QueuedInputPendingTurnReceipt {
                    identity,
                    turn_id,
                    resolution_seq: resolution_event.seq,
                },
            ));
        }
        let turn_id = self.require_open_turn()?.turn_id;
        let index = self.require_queued_input_index(&identity)?;
        let queued = self.metadata.input_state.queued_inputs[index].clone();
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let resolution_seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::UiOnly,
            EventBody::QueuedInputResolution(QueuedInputResolutionEvent {
                client_action_id: queued.client_action_id.clone(),
                input_id: queued.input_id,
                resolution: QueuedInputResolutionKind::PromotedToPendingInTurn,
                text: queued.text.clone(),
                resource_ids: queued.resource_ids.clone(),
            }),
            now_ms,
        )?;
        let mut staged_metadata = self.metadata.clone();
        staged_metadata.input_state.queued_inputs.remove(index);
        staged_metadata
            .input_state
            .pending_in_turn_inputs
            .push(PendingInTurnInput {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id,
                turn_id,
                text: queued.text,
                resource_ids: queued.resource_ids,
                created_at_ms: now_ms,
            });
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[resolution_seq],
        );
        Ok(IdempotentMutation::Committed(
            QueuedInputPendingTurnReceipt {
                identity,
                turn_id,
                resolution_seq,
            },
        ))
    }

    /// Resolves the exact durable queue identity owned by one client action.
    /// 解析一个客户端动作拥有的精确持久化队列身份。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` is the browser idempotency identity stored by queue admission.
    /// `client_action_id` 是排队准入保存的浏览器幂等身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the current or terminal queue identity, or `None` when the action never owned a
    /// queue item. Conflicting durable identities are rejected instead of guessed.
    /// 返回当前或已终结的队列身份；若该动作从未拥有队列条目则返回 `None`。持久化身份冲突会被
    /// 显式拒绝，而不是猜测。
    pub fn queued_input_identity_for_action(
        &self,
        client_action_id: &ClientActionId,
    ) -> Result<Option<QueuedInputIdentity>, DomainError> {
        let mut resolved = None;
        for queued in &self.metadata.input_state.queued_inputs {
            if queued.client_action_id == *client_action_id {
                merge_queued_input_identity(
                    &mut resolved,
                    QueuedInputIdentity {
                        client_action_id: queued.client_action_id.clone(),
                        input_id: queued.input_id,
                    },
                )?;
            }
        }
        for event in &self.events {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            if resolution.client_action_id == *client_action_id {
                merge_queued_input_identity(
                    &mut resolved,
                    QueuedInputIdentity {
                        client_action_id: resolution.client_action_id.clone(),
                        input_id: resolution.input_id,
                    },
                )?;
            }
        }
        if let Some(identity) = self
            .metadata
            .input_state
            .draft
            .restored_from_queue
            .as_ref()
            .filter(|identity| identity.client_action_id == *client_action_id)
        {
            merge_queued_input_identity(&mut resolved, identity.clone())?;
        }
        Ok(resolved)
    }

    /// Replaces the FIFO queue order using one exact complete identity list.
    /// 使用一份精确完整身份列表替换 FIFO 队列顺序。
    ///
    /// # Parameters
    /// 参数
    /// `ordered` must contain every current item exactly once; `now_ms` is the mutation boundary.
    /// `ordered` 必须恰好包含每个当前条目一次；`now_ms` 是修改边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns the committed or already-current complete order.
    /// 返回已提交或已经是当前值的完整顺序。
    pub fn reorder_queued_inputs(
        &mut self,
        ordered: Vec<QueuedInputIdentity>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<QueuedInputOrderReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if ordered.len() != self.metadata.input_state.queued_inputs.len() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "member_count",
            });
        }
        let current_order = self
            .metadata
            .input_state
            .queued_inputs
            .iter()
            .map(queued_input_identity)
            .collect::<Vec<_>>();
        if current_order == ordered {
            return Ok(IdempotentMutation::Idempotent(QueuedInputOrderReceipt {
                ordered,
            }));
        }
        let mut by_identity = self
            .metadata
            .input_state
            .queued_inputs
            .iter()
            .cloned()
            .map(|queued| (queued_input_identity(&queued), queued))
            .collect::<BTreeMap<_, _>>();
        if by_identity.len() != self.metadata.input_state.queued_inputs.len() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "duplicate_authoritative_identity",
            });
        }
        let mut reordered = Vec::with_capacity(ordered.len());
        let mut requested = BTreeSet::new();
        for identity in &ordered {
            if !requested.insert(identity.clone()) {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "duplicate_requested_identity",
                });
            }
            let queued =
                by_identity
                    .remove(identity)
                    .ok_or(DomainError::InvalidQueuedInputOrder {
                        field: "unknown_requested_identity",
                    })?;
            reordered.push(queued);
        }
        if !by_identity.is_empty() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "missing_requested_identity",
            });
        }
        self.metadata.input_state.queued_inputs = reordered;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(QueuedInputOrderReceipt {
            ordered,
        }))
    }

    /// Records one in-turn supplementary input bound to the open turn.
    /// 记录一条绑定开放回合的过程内补充输入。
    pub fn add_pending_in_turn_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<PendingInputId>, DomainError> {
        let ClientInputSubmission {
            client_action_id,
            text,
            resource_ids,
        } = submission;
        if let Some(existing_input_id) =
            self.existing_pending_submission(&client_action_id, &text, &resource_ids)?
        {
            return Ok(IdempotentMutation::Idempotent(existing_input_id));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        let turn_id = self.require_open_turn()?.turn_id;
        self.ensure_transition_capacity(0, 1)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        staged_metadata
            .input_state
            .pending_in_turn_inputs
            .push(PendingInTurnInput {
                client_action_id,
                input_id,
                turn_id,
                text,
                resource_ids,
                created_at_ms: now_ms,
            });
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(input_id))
    }

    /// Returns whether one input id already appeared in the main stream.
    /// 返回某个输入 id 是否已经出现在主事件流中。
    pub(super) fn input_id_already_streamed(&self, input_id: PendingInputId) -> bool {
        self.events.iter().any(|event| match &event.event {
            EventBody::UserInput(input) => input.input_id == input_id,
            _ => false,
        })
    }

    /// Returns an already persisted root submission when its action is an exact retry.
    /// 当一个动作是精确重试时返回已持久化的根输入提交。
    fn existing_root_submission(
        &self,
        client_action_id: &ClientActionId,
        text: &str,
        resource_ids: &[ResourceId],
    ) -> Result<Option<TurnId>, DomainError> {
        for event in &self.events {
            let EventBody::UserInput(input) = &event.event else {
                continue;
            };
            if input.client_action_id != *client_action_id {
                continue;
            }
            if input.input_source == UserInputSource::Root
                && input.text == text
                && input.resource_ids == resource_ids
            {
                return event
                    .turn_id
                    .map(Some)
                    .ok_or(DomainError::InvalidContinuationHistory {
                        field: "root_input.turn_id",
                    });
            }
            return Err(client_action_conflict(client_action_id));
        }
        Ok(None)
    }

    /// Returns an already queued submission when its action is an exact retry.
    /// 当一个动作是精确重试时返回已经排队的提交。
    fn existing_enqueue_receipt(
        &self,
        client_action_id: &ClientActionId,
        text: &str,
        resource_ids: &[ResourceId],
    ) -> Result<Option<EnqueueInputReceipt>, DomainError> {
        for queued in &self.metadata.input_state.queued_inputs {
            if queued.client_action_id != *client_action_id {
                continue;
            }
            if queued.text == text && queued.resource_ids == resource_ids {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: queued.input_id,
                    disposition: EnqueueInputDisposition::Queued,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        for event in &self.events {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            if resolution.client_action_id != *client_action_id {
                continue;
            }
            if resolution.text == text && resolution.resource_ids == resource_ids {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: resolution.input_id,
                    disposition: EnqueueInputDisposition::Queued,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        if let Some(identity) = self
            .metadata
            .input_state
            .draft
            .restored_from_queue
            .as_ref()
            .filter(|identity| identity.client_action_id == *client_action_id)
        {
            let draft = &self.metadata.input_state.draft;
            if draft.text == text && draft.resource_ids == resource_ids {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: identity.input_id,
                    disposition: EnqueueInputDisposition::Queued,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        for (index, event) in self.events.iter().enumerate() {
            let EventBody::UserInput(input) = &event.event else {
                continue;
            };
            if input.client_action_id != *client_action_id {
                continue;
            }
            if input.input_source == UserInputSource::QueuedScheduled
                && input.text == text
                && input.resource_ids == resource_ids
            {
                return Ok(Some(EnqueueInputReceipt {
                    client_action_id: client_action_id.clone(),
                    input_id: input.input_id,
                    disposition: self.enqueue_disposition_from_scheduled_event(index)?,
                }));
            }
            return Err(client_action_conflict(client_action_id));
        }
        Ok(None)
    }

    /// Reconstructs the original enqueue disposition from one exact scheduled input event.
    /// 从一条精确已调度输入事件重建最初的排队处置结果。
    fn enqueue_disposition_from_scheduled_event(
        &self,
        input_index: usize,
    ) -> Result<EnqueueInputDisposition, DomainError> {
        let Some(stats_index) = input_index.checked_sub(1) else {
            return Ok(EnqueueInputDisposition::Queued);
        };
        let Some(boundary_index) = input_index.checked_sub(2) else {
            return Ok(EnqueueInputDisposition::Queued);
        };
        let input_event = &self.events[input_index];
        let stats_event = &self.events[stats_index];
        let boundary_event = &self.events[boundary_index];
        let (
            EventBody::UserInput(_),
            EventBody::TurnStats(stats),
            EventBody::TurnBoundary(boundary),
        ) = (
            &input_event.event,
            &stats_event.event,
            &boundary_event.event,
        )
        else {
            return Ok(EnqueueInputDisposition::Queued);
        };
        let Some(stopped_turn_id) = stats_event.turn_id else {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "superseded_goal_stats_turn_id",
            });
        };
        if boundary_event.turn_id != Some(stopped_turn_id)
            || stats.outcome != TurnOutcome::Stopped
            || boundary.outcome != TurnOutcome::Stopped
            || !self.is_goal_continuation_turn(stopped_turn_id)
        {
            return Ok(EnqueueInputDisposition::Queued);
        }
        let scheduled_turn_id =
            input_event
                .turn_id
                .ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "scheduled_input_turn_id",
                })?;
        Ok(EnqueueInputDisposition::SupersededGoalAndScheduled {
            stopped_turn_id,
            stopped_turn_stats_seq: stats_event.seq,
            scheduled_turn_id,
            input_seq: input_event.seq,
        })
    }

    /// Returns whether one Turn was opened by a Goal-continuation context event.
    /// 返回一个回合是否由 Goal 续跑上下文事件开启。
    fn is_goal_continuation_turn(&self, turn_id: TurnId) -> bool {
        self.events.iter().find_map(|event| {
            if event.turn_id != Some(turn_id) {
                return None;
            }
            Some(matches!(
                &event.event,
                EventBody::SystemContext(context)
                    if context.context_key == "goal_continuation"
                        || (context.context_key == "goal_state"
                            && context.operation == SystemContextOperation::Upsert)
            ))
        }) == Some(true)
    }

    /// Resolves one exact queue identity or reports a structured missing/conflict error.
    /// 解析一个精确队列身份，或报告结构化缺失/冲突错误。
    fn require_queued_input_index(
        &self,
        identity: &QueuedInputIdentity,
    ) -> Result<usize, DomainError> {
        let mut partial_match = false;
        for (index, queued) in self.metadata.input_state.queued_inputs.iter().enumerate() {
            let queued_identity = queued_input_identity(queued);
            if queued_identity == *identity {
                return Ok(index);
            }
            if queued_identity.client_action_id == identity.client_action_id
                || queued_identity.input_id == identity.input_id
            {
                partial_match = true;
            }
        }
        if partial_match {
            return Err(client_action_conflict(&identity.client_action_id));
        }
        Err(DomainError::QueuedInputNotFound {
            input_id: identity.input_id.value(),
        })
    }

    /// Finds one terminal queued-input resolution and rejects partial identity drift.
    /// 查找一条排队输入终结处置，并拒绝部分身份漂移。
    fn existing_queued_input_resolution(
        &self,
        identity: &QueuedInputIdentity,
    ) -> Result<Option<&QueuedInputResolutionEvent>, DomainError> {
        let mut exact = None;
        for event in &self.events {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            let action_matches = resolution.client_action_id == identity.client_action_id;
            let input_matches = resolution.input_id == identity.input_id;
            if action_matches && input_matches {
                if exact.replace(resolution).is_some() {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "duplicate_resolution_identity",
                    });
                }
            } else if action_matches || input_matches {
                return Err(client_action_conflict(&identity.client_action_id));
            }
        }
        Ok(exact)
    }

    /// Returns an already pending or streamed in-turn submission for an exact retry.
    /// 对精确重试返回已经暂存或已流入回合的过程内提交。
    fn existing_pending_submission(
        &self,
        client_action_id: &ClientActionId,
        text: &str,
        resource_ids: &[ResourceId],
    ) -> Result<Option<PendingInputId>, DomainError> {
        for pending in &self.metadata.input_state.pending_in_turn_inputs {
            if pending.client_action_id != *client_action_id {
                continue;
            }
            if pending.text == text && pending.resource_ids == resource_ids {
                return Ok(Some(pending.input_id));
            }
            return Err(client_action_conflict(client_action_id));
        }
        let mut existing_input_id = None;
        for event in &self.events {
            if event_client_action_id(event) != Some(client_action_id) {
                continue;
            }
            if let EventBody::UserInput(input) = &event.event
                && input.input_source == UserInputSource::PendingInTurn
                && input.text == text
                && input.resource_ids == resource_ids
                && existing_input_id.replace(input.input_id).is_none()
            {
                continue;
            }
            return Err(client_action_conflict(client_action_id));
        }
        Ok(existing_input_id)
    }

    /// Requires one external client action identity to be globally unused.
    /// 要求一个外部客户端动作身份在整个会话中尚未使用。
    pub(super) fn require_client_action_unused(
        &self,
        client_action_id: &ClientActionId,
    ) -> Result<(), DomainError> {
        let used_by_metadata = self
            .metadata
            .input_state
            .pending_in_turn_inputs
            .iter()
            .any(|pending| pending.client_action_id == *client_action_id)
            || self
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .any(|queued| queued.client_action_id == *client_action_id)
            || self
                .metadata
                .input_state
                .draft
                .restored_from_queue
                .as_ref()
                .is_some_and(|identity| identity.client_action_id == *client_action_id);
        let used_by_event = self.events.iter().any(|event| {
            event_client_action_id(event).is_some_and(|value| value == client_action_id)
                || matches!(
                    &event.event,
                    EventBody::QueuedInputResolution(resolution)
                        if resolution.client_action_id == *client_action_id
                )
        });
        if used_by_metadata || used_by_event {
            return Err(client_action_conflict(client_action_id));
        }
        Ok(())
    }
}
