//! Client input identities, durable queue transitions, and matching input history validation.
//! 客户端输入身份、可靠队列转换及对应输入历史校验。

use super::persistence::DirtyFile;
use super::selection::active_request_from_pending;
use super::turns::{clear_expired_dynamic_context_metadata, expiring_ttl_contexts};
use super::{
    ActiveTurnState, IdempotentMutation, SessionDocument, TurnOrigin, client_action_conflict,
    validate_timestamp,
};
use crate::v1::config::{
    ActiveRequestConfig, DraftState, MetadataDocument, PendingInTurnInput, QueuedInput,
    QueuedInputIdentity,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiPurposeKind, ConversationEvent, EventBody, EventVisibility, ExecutionControlKind,
    QueuedInputResolutionEvent, QueuedInputResolutionKind, SystemContextOperation, TurnOutcome,
    UserInputEvent, UserInputSource,
};
use crate::v1::ids::{ClientActionId, PendingInputId, ResourceId, Seq, TurnId};
use crate::v1::user_prompt::initial_preparation_matches;
use std::collections::{BTreeMap, BTreeSet};

/// Validates exact ordered staged `attachment_ids` against `resource_ids`; absence never invents old asset lineage.
/// 根据`resource_ids`校验精确有序暂存`attachment_ids`；缺失绝不虚构旧资产来源。
/// Returns an explicit domain error for mismatched counts or invalid opaque identifiers.
/// 数量不匹配或不透明标识无效时返回明确领域错误。
fn validate_draft_attachment_ids(
    attachment_ids: &[Option<String>],
    resource_ids: &[ResourceId],
) -> Result<(), DomainError> {
    if (!attachment_ids.is_empty() && attachment_ids.len() != resource_ids.len())
        || attachment_ids.iter().flatten().any(|id| {
            id.is_empty()
                || id.trim() != id
                || id.len() > 1024
                || id.chars().any(|character| {
                    character.is_control() || matches!(character, '/' | '\\' | ':')
                })
        })
    {
        return Err(DomainError::InvalidQueuedInputOrder {
            field: "draft_staged_attachment_identity",
        });
    }
    Ok(())
}

/// One client-authored input carrying its durable RPC idempotency identity.
/// 一条携带持久化 RPC 幂等身份的客户端输入。
#[derive(Debug, Clone, PartialEq)]
pub struct ClientInputSubmission {
    /// Optional host-proven earlier preparation; omission creates fresh pending parameters only at new input admission.
    /// 可选宿主已证明更早准备；仅在新输入准入时将省略值创建为新的待准备参数。
    pub parameters: Option<crate::v1::UserPromptParameters>,
    /// Exact request configuration bound to this input before durable admission, when applicable.
    /// 适用时在可靠准入前绑定到本输入的精确请求配置。
    pub bound_request_config: Option<ActiveRequestConfig>,
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
            parameters: None,
            bound_request_config: None,
            client_action_id,
            text: text.into(),
            resource_ids,
        }
    }

    /// Attaches host-proven `parameters` to this exact ordered input; returns an error before admission if ownership differs.
    /// 将宿主已证明`parameters`附着到本精确有序输入；归属不符时在准入前返回错误。
    pub fn with_parameters(
        mut self,
        parameters: crate::v1::UserPromptParameters,
    ) -> Result<Self, DomainError> {
        parameters.validate_pending(&self.resource_ids)?;
        self.parameters = Some(parameters);
        Ok(self)
    }

    /// Binds the exact request configuration selected for this input's next execution boundary.
    /// 绑定为本输入下一执行边界选定的精确请求配置。
    ///
    /// # Parameters / 参数
    /// `request_config` is the already resolved session-owned provider request selection.
    /// `request_config` 是已经解析、由会话持有的 Provider 请求选择。
    ///
    /// # Returns / 返回值
    /// Returns this submission with the immutable request configuration attached.
    /// 返回附带不可变请求配置的本提交。
    pub fn with_bound_request_config(mut self, request_config: ActiveRequestConfig) -> Self {
        self.bound_request_config = Some(request_config);
        self
    }
}

/// Resolves explicitly optional `parameters` for a new input with ordered `resource_ids`; never used to hydrate old metadata.
/// 为有序`resource_ids`的新输入解析明确可选的`parameters`；绝不用于补水旧元数据。
pub(super) fn new_input_parameters(
    resource_ids: &[ResourceId],
    parameters: Option<crate::v1::UserPromptParameters>,
) -> Result<Option<crate::v1::UserPromptParameters>, DomainError> {
    if let Some(parameters) = parameters {
        parameters.validate_pending(resource_ids)?;
        Ok(Some(parameters))
    } else {
        Ok(crate::v1::UserPromptParameters::for_new_resources(
            resource_ids,
        ))
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
    if !metadata.input_state.draft.resource_sources.is_empty()
        && (metadata.input_state.draft.resource_sources.len()
            != metadata.input_state.draft.resource_ids.len()
            || metadata
                .input_state
                .draft
                .resource_sources
                .iter()
                .enumerate()
                .any(|(position, source)| {
                    source.is_some()
                        && metadata
                            .input_state
                            .draft
                            .staged_attachment_ids
                            .get(position)
                            .is_some_and(Option::is_some)
                }))
    {
        return Err(DomainError::InvalidQueuedInputOrder {
            field: "draft_resource_sources",
        });
    }
    validate_draft_attachment_ids(
        &metadata.input_state.draft.staged_attachment_ids,
        &metadata.input_state.draft.resource_ids,
    )?;
    // Validate persisted input parameters without synthesizing new intent for legacy absent data.
    // 校验持久输入参数，不为缺失旧数据合成新意图。
    if let Some(parameters) = &metadata.input_state.draft.parameters {
        parameters.validate_pending(&metadata.input_state.draft.resource_ids)?;
    }
    for pending in &metadata.input_state.pending_in_turn_inputs {
        if let Some(parameters) = &pending.parameters {
            parameters.validate_pending(&pending.resource_ids)?;
        }
    }
    for queued in &metadata.input_state.queued_inputs {
        if let Some(parameters) = &queued.parameters {
            parameters.validate_pending(&queued.resource_ids)?;
        }
        // A live queue owner may never also be scheduled for attachment release.
        // 仍在队列中的所有者绝不能同时进入附件释放待办。
        if metadata
            .input_state
            .pending_staging_release_actions
            .contains(&queued.client_action_id)
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "pending_staging_release_action_still_queued",
            });
        }
    }
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
            if let Some(parameters) = &input.parameters {
                parameters.validate(&input.resource_ids)?;
            }
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
        if let Some(parameters) = &resolution.parameters {
            parameters.validate_pending(&resolution.resource_ids)?;
        }
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
                        && metadata.input_state.draft.resource_ids == resolution.resource_ids
                        && initial_preparation_matches(
                            metadata.input_state.draft.parameters.as_ref(),
                            resolution.parameters.as_ref(),
                        );
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
                            || !initial_preparation_matches(
                                pending.parameters.as_ref(),
                                resolution.parameters.as_ref(),
                            )
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
                        // A completed-Turn handoff must prove exact membership in the next initial batch.
                        // 已完成回合交接必须证明其属于下一初始批次的精确成员。
                        let same_turn = candidate.turn_id == Some(turn_id)
                            && input.input_source == UserInputSource::PendingInTurn;
                        if !(same_turn
                            || super::input_handoff::input_was_handed_off(
                                events, candidate, turn_id,
                            ))
                            || input.text != resolution.text
                            || input.resource_ids != resolution.resource_ids
                            || !initial_preparation_matches(
                                input.parameters.as_ref(),
                                resolution.parameters.as_ref(),
                            )
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
                && metadata.input_state.draft.resource_ids == resolution.resource_ids
                && initial_preparation_matches(
                    metadata.input_state.draft.parameters.as_ref(),
                    resolution.parameters.as_ref(),
                ))
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
    /// Reads the exact accepted input for `client_action_id` across its declared queue, pending and event lifecycle.
    /// 沿已声明的队列、待处理及事件生命周期读取`client_action_id`对应的精确已接纳输入。
    /// Returns immutable input facts, including current prompt parameters; never reads the mutable composer draft or guesses the latest input.
    /// 返回不可变输入事实及当前提示词参数；绝不读取可变输入区草稿或猜测最近输入。
    pub fn client_input_for_action(
        &self,
        client_action_id: &ClientActionId,
    ) -> Result<Option<ClientInputSubmission>, DomainError> {
        // A promoted input can have a terminal queue receipt and one live owner with identical admission facts.
        // 已提升输入可以同时拥有终结队列回执及一个具有相同准入事实的活动所有者。
        let mut resolved: Option<(PendingInputId, ClientInputSubmission)> = None;
        // The declared lifecycle permits one live owner and at most one terminal queue receipt.
        // 已声明生命周期允许一个活动所有者及至多一个终结队列回执。
        let mut live_seen = false;
        // A second receipt is corruption, not another candidate source to silently prefer.
        // 第二个回执是损坏，而非另一个可静默优先的候选来源。
        let mut receipt_seen = false;
        // Merge only exact lifecycle records; conflicting identities or proofs are explicit invariant failures.
        // 仅合并精确生命周期记录；身份或证明冲突是显式不变量失败。
        let mut retain = |input_id: PendingInputId,
                          text: &str,
                          resource_ids: &[ResourceId],
                          parameters: Option<&crate::v1::UserPromptParameters>,
                          terminal_receipt: bool|
         -> Result<(), DomainError> {
            // State locations are explicit protocol variants, not guessed alternative field paths.
            // 状态位置是显式协议变体，而非猜测的替代字段路径。
            let seen = if terminal_receipt {
                &mut receipt_seen
            } else {
                &mut live_seen
            };
            if std::mem::replace(seen, true) {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "duplicate_client_input_source",
                });
            }
            if let Some(parameters) = parameters {
                parameters.validate(resource_ids)?;
            }
            if let Some((existing_id, existing)) = &resolved {
                if *existing_id != input_id
                    || existing.text != text
                    || existing.resource_ids != resource_ids
                    || !initial_preparation_matches(existing.parameters.as_ref(), parameters)
                {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "client_input_source_conflict",
                    });
                }
                // A stream owner's later frozen result adds real first-request facts without replacing its immutable admission proof.
                // 入流所有者后续冻结结果增加真实首次请求事实，但不替换不可变准入证明。
                if !parameters.is_some_and(|parameters| {
                    matches!(
                        parameters.media_delivery,
                        crate::v1::UserPromptMediaDelivery::Frozen { .. }
                            | crate::v1::UserPromptMediaDelivery::Preparing { .. }
                    )
                }) {
                    return Ok(());
                }
            }
            resolved = Some((
                input_id,
                ClientInputSubmission {
                    client_action_id: client_action_id.clone(),
                    text: text.to_owned(),
                    resource_ids: resource_ids.to_vec(),
                    parameters: parameters.cloned(),
                    bound_request_config: None,
                },
            ));
            Ok(())
        };
        for queued in &self.metadata.input_state.queued_inputs {
            if queued.client_action_id == *client_action_id {
                retain(
                    queued.input_id,
                    &queued.text,
                    &queued.resource_ids,
                    queued.parameters.as_ref(),
                    false,
                )?;
            }
        }
        for pending in &self.metadata.input_state.pending_in_turn_inputs {
            if pending.client_action_id == *client_action_id {
                retain(
                    pending.input_id,
                    &pending.text,
                    &pending.resource_ids,
                    pending.parameters.as_ref(),
                    false,
                )?;
            }
        }
        for event in &self.events {
            match &event.event {
                EventBody::QueuedInputResolution(resolution)
                    if resolution.client_action_id == *client_action_id =>
                {
                    retain(
                        resolution.input_id,
                        &resolution.text,
                        &resolution.resource_ids,
                        resolution.parameters.as_ref(),
                        true,
                    )?
                }
                EventBody::UserInput(input) if input.client_action_id == *client_action_id => {
                    retain(
                        input.input_id,
                        &input.text,
                        &input.resource_ids,
                        input.parameters.as_ref(),
                        false,
                    )?
                }
                _ => {}
            }
        }
        Ok(resolved.map(|(_, input)| input))
    }

    /// Resolves `source` in this exact document and verifies its position still names `resource_id`.
    /// 在本精确文档解析`source`，并验证其位置仍指向`resource_id`。
    /// Returns known preparation, explicit not-yet-prepared intent, or unknown history; contradictory resource references fail without substitution.
    /// 返回已知准备、明确尚未准备意图或未知历史；矛盾资源引用失败，不进行替代。
    pub fn user_prompt_resource_preparation(
        &self,
        source: &crate::v1::UserPromptResourceSource,
        resource_id: ResourceId,
    ) -> Result<crate::v1::UserPromptResourcePreparation, DomainError> {
        use crate::v1::{UserPromptMediaDelivery, UserPromptResourcePreparation};
        // Lookup is by immutable accepted input identity, never by repeated content or current composer text.
        // 按不可变已接纳输入身份查询，绝不按重复内容或当前输入区正文查询。
        let Some(input) = self.client_input_for_action(&source.client_action_id)? else {
            return Ok(UserPromptResourcePreparation::Unproven);
        };
        if input.resource_ids.get(source.resource_position) != Some(&resource_id) {
            return Err(crate::v1::user_prompt::invalid_prompt_media(
                "source_resource_occurrence",
            ));
        }
        // Old missing parameters remain unknown rather than gaining the new-intent constructor's semantics.
        // 旧缺失参数保持未知，不获得新意图构造器的语义。
        let Some(parameters) = input.parameters else {
            return Ok(UserPromptResourcePreparation::Unproven);
        };
        if parameters
            .unproven_resource_positions
            .binary_search(&source.resource_position)
            .is_ok()
        {
            return Ok(UserPromptResourcePreparation::Unproven);
        }
        match &parameters.media_delivery {
            UserPromptMediaDelivery::Frozen { origin } => {
                Ok(UserPromptResourcePreparation::Prepared(
                    origin.resources[source.resource_position].clone(),
                ))
            }
            UserPromptMediaDelivery::Pending | UserPromptMediaDelivery::Preparing { .. } => Ok(
                match parameters.original_resource(source.resource_position) {
                    Some(prepared) => UserPromptResourcePreparation::Prepared(prepared.clone()),
                    None => {
                        // A stale Pending event beyond its first API cannot gain new upload eligibility when copied.
                        // 越过首次API的陈旧Pending事件被复制时不能取得新上传资格。
                        let source_event = self.events.iter().find(|event| matches!(&event.event, EventBody::UserInput(input) if input.client_action_id == source.client_action_id));
                        if source_event.is_some_and(|event| !self.preparing_user_prompt_media_seqs().contains(&event.seq()))
                        || self.events.iter().any(|event| matches!(&event.event, EventBody::QueuedInputResolution(resolution) if resolution.client_action_id == source.client_action_id && resolution.resolution == QueuedInputResolutionKind::Removed)) {
                        UserPromptResourcePreparation::Unproven
                    } else {
                        UserPromptResourcePreparation::Pending
                    }
                    }
                },
            ),
        }
    }

    /// Submits a root user prompt as a fresh turn.
    /// 以全新回合提交一条根用户提示词。
    pub fn submit_root_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<TurnId>, DomainError> {
        let ClientInputSubmission {
            parameters,
            bound_request_config,
            client_action_id,
            text,
            resource_ids,
        } = submission;
        // New admission normalizes omission; persisted queue/history never uses this constructor.
        // 新准入规范化省略值；持久队列及历史绝不使用此构造器。
        let parameters = new_input_parameters(&resource_ids, parameters)?;
        if let Some(existing_turn_id) = self.existing_root_submission(
            &client_action_id,
            &text,
            &resource_ids,
            parameters.as_ref(),
        )? {
            return Ok(IdempotentMutation::Idempotent(existing_turn_id));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        // A user can supersede a retained late return that has not started inference, including after Stop.
        // 用户可取代尚未启动推理的迟到回流，包括停止后保留的回流。
        // Auxiliary requests do not consume the return, while any active physical request still excludes replacement.
        // 辅助请求不消费回流，但任何活动物理请求仍排除替代。
        let undispatched_return = self
            .active_turn
            .as_ref()
            .filter(|turn| {
                !turn.closed
                    && turn.origin == TurnOrigin::SubagentReturn
                    && self.active_api.is_none()
                    && !self.events.iter().any(|event| {
                        event.turn_id == Some(turn.turn_id)
                            && matches!(&event.event, EventBody::ApiStarted(started)
                                if !matches!(started.purpose,
                                    ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
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
        let request_config = bound_request_config.or_else(|| {
            self.metadata
                .config
                .pending_request
                .as_ref()
                .map(active_request_from_pending)
        });
        let expiring_contexts = undispatched_return
            .as_ref()
            .map(|turn| expiring_ttl_contexts(self, turn.turn_id))
            .transpose()?
            .unwrap_or_default();
        let ttl_expiration_count = u64::try_from(expiring_contexts.len()).map_err(|_| {
            DomainError::ImmutableFieldMutation {
                entity: "ttl_expiration",
                field: "event_count",
            }
        })?;
        self.ensure_transition_capacity(
            (if undispatched_return.is_some() { 3 } else { 1 })
                + u64::from(request_config.is_some())
                + ttl_expiration_count,
            2,
        )?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
        let event_count = (if undispatched_return.is_some() { 3 } else { 1 })
            + u64::from(request_config.is_some())
            + ttl_expiration_count;
        let mut staged_events = Vec::with_capacity(event_count as usize);
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
                crate::v1::event::TurnClosureReason::Stopped,
                0,
                now_ms,
                buckets,
            )?;
            clear_expired_dynamic_context_metadata(&mut staged_metadata, &expiring_contexts);
        }
        if let Some(request_config) = request_config {
            self.stage_bound_request_activation(
                &mut staged_metadata,
                &mut staged_allocator,
                &mut staged_events,
                request_config,
                Some(turn_id),
                now_ms,
            )?;
        }
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters,
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
            parameters,
            bound_request_config,
            client_action_id,
            text,
            resource_ids,
        } = submission;
        // Freeze admission proof before this input waits behind another turn.
        // 在本输入等待另一回合之前冻结准入证明。
        let parameters = new_input_parameters(&resource_ids, parameters)?;
        if let Some(existing_receipt) = self.existing_enqueue_receipt(
            &client_action_id,
            &text,
            &resource_ids,
            parameters.as_ref(),
            bound_request_config.as_ref(),
        )? {
            return Ok(IdempotentMutation::Idempotent(existing_receipt));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        validate_timestamp("updated_at_ms", now_ms)?;

        // User priority ends only when business starts; completed auxiliary work cannot consume that priority.
        // 用户优先权仅在业务启动后结束；已结束辅助工作不能消费该优先权。
        let undispatched_goal_turn = self.active_turn.as_ref().filter(|turn| {
            !turn.closed
                && turn.origin == TurnOrigin::GoalContinuation
                && self.active_api.is_none()
                && !self.events.iter().any(|event| {
                    event.turn_id == Some(turn.turn_id)
                        && matches!(&event.event, EventBody::ApiStarted(started)
                            if !matches!(started.purpose,
                                ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                })
        });
        if let Some(goal_turn) = undispatched_goal_turn.cloned() {
            if !self.metadata.input_state.queued_inputs.is_empty() {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "undispatched_goal_existing_queue",
                });
            }
            let buckets = self.aggregate_turn_buckets(goal_turn.turn_id, goal_turn.start_seq)?;
            let expiring_contexts = expiring_ttl_contexts(self, goal_turn.turn_id)?;
            let ttl_expiration_count = u64::try_from(expiring_contexts.len()).map_err(|_| {
                DomainError::ImmutableFieldMutation {
                    entity: "ttl_expiration",
                    field: "event_count",
                }
            })?;
            let request_config_event_count = u64::from(bound_request_config.is_some());
            let event_count = 3 + request_config_event_count + ttl_expiration_count;
            self.ensure_transition_capacity(event_count, 2)?;
            let mut staged_allocator = self.id_allocator;
            let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
            let scheduled_turn_id = TurnId::try_new(staged_allocator.next_local_id()?)?;
            let mut staged_events = Vec::with_capacity(event_count as usize);
            let mut staged_metadata = self.metadata.clone();
            consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
            let stopped_turn_stats_seq = self.stage_turn_closure(
                &mut staged_allocator,
                &mut staged_events,
                goal_turn.turn_id,
                goal_turn.start_seq,
                crate::v1::event::TurnClosureReason::Stopped,
                0,
                now_ms,
                buckets,
            )?;
            clear_expired_dynamic_context_metadata(&mut staged_metadata, &expiring_contexts);
            if let Some(request_config) = bound_request_config {
                self.stage_bound_request_activation(
                    &mut staged_metadata,
                    &mut staged_allocator,
                    &mut staged_events,
                    request_config,
                    Some(scheduled_turn_id),
                    now_ms,
                )?;
            }
            let input_seq = self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(scheduled_turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    parameters,
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
        // Persist the observed queue input together with admission, before any process-local Hook notification.
        // 将已观察队列输入与准入一起持久化，先于任何进程内 Hook 通知。
        // Canonical turn ownership exists before Hook preparation, closing that initial crash window too.
        // Hook 准备前已存在规范回合归属，同时关闭该初始崩溃窗口。
        if let Some(turn) = self
            .active_turn
            .as_ref()
            .filter(|turn| !turn.closed && turn.origin != TurnOrigin::QuestionExecution)
        {
            staged_metadata
                .business_hook
                .observed_inputs
                .retain(|entry| entry.turn_id == turn.turn_id);
            staged_metadata.business_hook.observed_inputs.push(
                crate::v1::business_hook::BusinessHookObservedInput {
                    turn_id: turn.turn_id,
                    client_action_id: client_action_id.clone(),
                    input_id,
                    observed_revision: self.next_mutation_revision()?,
                    kind: crate::v1::business_hook::BusinessHookInputKind::Queued,
                    content: text.clone(),
                },
            );
        }
        staged_metadata.input_state.queued_inputs.push(QueuedInput {
            parameters,
            bound_request_config,
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
        // Existing callers edit resource-only drafts; unchanged occurrences retain their already-known lineage.
        // 既有调用方编辑仅资源草稿；不变出现保留其已经知道的来源。
        let attachment_ids = if self.metadata.input_state.draft.resource_ids == resource_ids {
            self.metadata
                .input_state
                .draft
                .staged_attachment_ids
                .clone()
        } else {
            Vec::new()
        };
        self.set_draft_with_staged_attachments(text, resource_ids, attachment_ids, now_ms)
    }

    /// Replaces draft `text` and ordered `resource_ids` with host-verified per-occurrence `staged_attachment_ids` at `now_ms`.
    /// 在`now_ms`以宿主已验证逐出现`staged_attachment_ids`替换草稿`text`及有序`resource_ids`。
    /// Returns a durable mutation receipt; text-only edits retain preparation, while changed intentions clear it.
    /// 返回可靠修改回执；仅正文编辑保留准备，变化意图清除准备。
    pub fn set_draft_with_staged_attachments(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        staged_attachment_ids: Vec<Option<String>>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<DraftMutationReceipt>, DomainError> {
        // Existing callers retain source references only when they preserve the exact staged/resource occurrence sequence.
        // 既有调用方只有保留精确暂存及资源出现序列时，才保留来源引用。
        let sources = if self.metadata.input_state.draft.resource_ids == resource_ids
            && (self.metadata.input_state.draft.staged_attachment_ids == staged_attachment_ids
                || (self
                    .metadata
                    .input_state
                    .draft
                    .staged_attachment_ids
                    .is_empty()
                    && staged_attachment_ids.iter().all(Option::is_none)))
        {
            self.metadata.input_state.draft.resource_sources.clone()
        } else {
            Vec::new()
        };
        self.set_draft_with_resource_sources(
            text,
            resource_ids,
            staged_attachment_ids,
            sources,
            now_ms,
        )
    }

    /// Replaces draft `text` and ordered `resource_ids` using explicit staged identities and accepted-input `resource_sources` at `now_ms`.
    /// 在`now_ms`以显式暂存身份及已接纳输入`resource_sources`替换草稿`text`和有序`resource_ids`。
    /// Returns an idempotent durable mutation, preserving source proof through reorder without matching content or guessing old origins.
    /// 返回幂等可靠修改，通过重排保留来源证明，不匹配内容或猜测旧原身份。
    pub fn set_draft_with_resource_sources(
        &mut self,
        text: impl Into<String>,
        resource_ids: Vec<ResourceId>,
        mut staged_attachment_ids: Vec<Option<String>>,
        mut resource_sources: Vec<Option<crate::v1::UserPromptResourceSource>>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<DraftMutationReceipt>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_draft_attachment_ids(&staged_attachment_ids, &resource_ids)?;
        if !resource_sources.is_empty() && resource_sources.len() != resource_ids.len() {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "draft_resource_sources",
            });
        }
        if resource_sources.iter().all(Option::is_none) {
            resource_sources.clear();
        }
        if staged_attachment_ids.iter().all(Option::is_none) {
            staged_attachment_ids.clear();
        }
        // Identical content can be explicitly attached twice under different asset ids; do not collapse those intentions.
        // 相同内容可用不同资产ID显式附加两次；不能合并这些意图。
        let same_intentions = self.metadata.input_state.draft.resource_ids == resource_ids
            && self.metadata.input_state.draft.staged_attachment_ids == staged_attachment_ids
            && self.metadata.input_state.draft.resource_sources == resource_sources;
        // Resolve every explicit source before publishing any draft mutation, including equal-content reordered resources.
        // 发布任何草稿修改前解析全部显式来源，包括相同内容的重排资源。
        let mut preparations = Vec::with_capacity(resource_ids.len());
        for (position, resource_id) in resource_ids.iter().enumerate() {
            match (
                staged_attachment_ids.get(position).and_then(Option::as_ref),
                resource_sources.get(position).and_then(Option::as_ref),
            ) {
                (Some(_), Some(_)) => {
                    return Err(DomainError::InvalidQueuedInputOrder {
                        field: "draft_resource_source_conflict",
                    });
                }
                (Some(_), None) => {
                    preparations.push(crate::v1::UserPromptResourcePreparation::Pending)
                }
                (None, Some(source)) => {
                    preparations.push(self.user_prompt_resource_preparation(source, *resource_id)?)
                }
                (None, None) => {
                    preparations.push(crate::v1::UserPromptResourcePreparation::Unproven)
                }
            }
        }
        let text = text.into();
        if self.metadata.input_state.draft.text == text && same_intentions {
            return Ok(IdempotentMutation::Idempotent(DraftMutationReceipt {
                draft: self.metadata.input_state.draft.clone(),
            }));
        }
        let draft = DraftState {
            parameters: if same_intentions {
                self.metadata.input_state.draft.parameters.clone()
            } else {
                crate::v1::UserPromptParameters::for_resource_preparations(
                    &resource_ids,
                    preparations,
                )?
            },
            text,
            resource_ids,
            staged_attachment_ids,
            resource_sources,
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
                    resource_sources: resolution
                        .resource_ids
                        .iter()
                        .enumerate()
                        .map(|(resource_position, _)| {
                            Some(crate::v1::UserPromptResourceSource {
                                client_action_id: resolution.client_action_id.clone(),
                                resource_position,
                            })
                        })
                        .collect(),
                    staged_attachment_ids: Vec::new(),
                    parameters: resolution.parameters.clone(),
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
            resource_sources: queued
                .resource_ids
                .iter()
                .enumerate()
                .map(|(resource_position, _)| {
                    Some(crate::v1::UserPromptResourceSource {
                        client_action_id: queued.client_action_id.clone(),
                        resource_position,
                    })
                })
                .collect(),
            staged_attachment_ids: Vec::new(),
            parameters: queued.parameters.clone(),
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
                parameters: queued.parameters,
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
        staged_metadata
            .input_state
            .pending_staging_release_actions
            .insert(identity.client_action_id.clone());
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
                parameters: queued.parameters,
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
        staged_metadata
            .input_state
            .pending_staging_release_actions
            .insert(identity.client_action_id.clone());
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
        // An auxiliary turn cannot own a human task; leave its exact queue entry untouched.
        // 辅助回合不能拥有人类任务；保持精确队列项不变。
        let turn = self.require_open_turn()?;
        if turn.origin == TurnOrigin::QuestionExecution {
            return Err(DomainError::CollaborationStateViolation {
                field: "question_execution.business_inputs",
            });
        }
        // Only the admitted business turn receives this explicit queue promotion.
        // 只有已准入业务回合接收此次明确队列晋升。
        let turn_id = turn.turn_id;
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
                parameters: queued.parameters.clone(),
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
            .pending_staging_release_actions
            .insert(identity.client_action_id.clone());
        // A queue admitted during an earlier task is first observed by this task when explicitly promoted.
        // 更早任务期间准入的队列项，仅在明确提升时首次被本任务观察。
        staged_metadata
            .business_hook
            .observed_inputs
            .retain(|entry| entry.turn_id == turn_id);
        if let Some(observed) = staged_metadata
            .business_hook
            .observed_inputs
            .iter()
            .find(|entry| entry.client_action_id == queued.client_action_id)
        {
            if observed.input_id != queued.input_id || observed.content != queued.text {
                return Err(DomainError::CollaborationStateViolation {
                    field: "business_hook.promoted_identity",
                });
            }
        } else {
            staged_metadata.business_hook.observed_inputs.push(
                crate::v1::business_hook::BusinessHookObservedInput {
                    turn_id,
                    client_action_id: queued.client_action_id.clone(),
                    input_id: queued.input_id,
                    observed_revision: self.next_mutation_revision()?,
                    kind: crate::v1::business_hook::BusinessHookInputKind::Interrupt,
                    content: queued.text.clone(),
                },
            );
        }
        staged_metadata
            .input_state
            .pending_in_turn_inputs
            .push(PendingInTurnInput {
                parameters: queued.parameters,
                bound_request_config: queued.bound_request_config,
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

    /// Acknowledges one queue attachment release only after its asset-index transaction succeeds.
    /// 仅在资产索引事务成功后确认一条队列附件释放。
    ///
    /// # Parameters
    /// 参数
    /// `client_action_id` selects the exact persisted release intent; `now_ms` dates its removal.
    /// `client_action_id` 选择精确的持久释放意图，`now_ms` 记录移除时间。
    ///
    /// # Returns
    /// 返回值
    /// Whether the durable metadata changed, or a validation error.
    /// 返回可靠元数据是否变化，或校验错误。
    pub fn acknowledge_queued_staging_release(
        &mut self,
        client_action_id: &ClientActionId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<()>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if !self
            .metadata
            .input_state
            .pending_staging_release_actions
            .remove(client_action_id)
        {
            return Ok(IdempotentMutation::Idempotent(()));
        }
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(IdempotentMutation::Committed(()))
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
    /// `client_action_id` deduplicates delivery and `text` is the complete agent-message envelope.
    /// `client_action_id` 用于投递去重，`text` 为完整代理消息信封。
    /// Returns the source sequence; admission is allowed only between physical APIs.
    /// 返回源序号；仅允许在两次物理 API 之间准入。
    pub fn append_in_turn_guidance_before_api(
        &mut self,
        client_action_id: ClientActionId,
        text: String,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        for event in &self.events {
            if let EventBody::UserInput(input) = &event.event
                && input.client_action_id == client_action_id
            {
                if input.text == text && input.input_source == UserInputSource::AgentMessage {
                    return Ok(IdempotentMutation::Idempotent(event.seq));
                }
                return Err(client_action_conflict(&client_action_id));
            }
        }
        self.require_client_action_unused(&client_action_id)?;
        let turn_id = self.require_open_turn()?.turn_id;
        if let Some(api) = &self.active_api {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        let text = validate_user_input(text, &[])?;
        validate_timestamp("updated_at_ms", now_ms)?;
        self.ensure_transition_capacity(1, 1)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_events = Vec::new();
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            Some(turn_id),
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                client_action_id,
                input_id,
                input_source: UserInputSource::AgentMessage,
                text,
                resource_ids: vec![],
            }),
            now_ms,
        )?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[seq]);
        Ok(IdempotentMutation::Committed(seq))
    }

    /// Records one in-turn supplementary input bound to the open turn.
    /// 记录一条绑定开放回合的过程内补充输入。
    pub fn add_pending_in_turn_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<PendingInputId>, DomainError> {
        let ClientInputSubmission {
            parameters,
            bound_request_config,
            client_action_id,
            text,
            resource_ids,
        } = submission;
        // Supplementary input retains the caller's earlier preparation until its actual stream position is created.
        // 补充输入保留调用方更早准备，直到创建其真实入流位置。
        let parameters = new_input_parameters(&resource_ids, parameters)?;
        if let Some(existing_input_id) = self.existing_pending_submission(
            &client_action_id,
            &text,
            &resource_ids,
            parameters.as_ref(),
        )? {
            return Ok(IdempotentMutation::Idempotent(existing_input_id));
        }
        self.require_client_action_unused(&client_action_id)?;
        let text = validate_user_input(text, &resource_ids)?;
        // Direct callers must obey the same auxiliary boundary as queue promotion.
        // 直接调用者必须遵守与队列晋升相同的辅助边界。
        let turn = self.require_open_turn()?;
        if turn.origin == TurnOrigin::QuestionExecution {
            return Err(DomainError::CollaborationStateViolation {
                field: "question_execution.business_inputs",
            });
        }
        // Retain the exact original business turn for the pending supplement.
        // 为待处理补充输入保留精确原业务回合。
        let turn_id = turn.turn_id;
        self.ensure_transition_capacity(0, 1)?;
        let mut staged_allocator = self.id_allocator;
        let input_id = PendingInputId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_metadata = self.metadata.clone();
        consume_matching_draft(&mut staged_metadata, &text, &resource_ids);
        // Persist direct guidance with admission, before API terminal promotion or asynchronous preparation.
        // 将直接引导与准入一起持久化，先于 API 终态提升或异步准备。
        staged_metadata
            .business_hook
            .observed_inputs
            .retain(|entry| entry.turn_id == turn_id);
        staged_metadata.business_hook.observed_inputs.push(
            crate::v1::business_hook::BusinessHookObservedInput {
                turn_id,
                client_action_id: client_action_id.clone(),
                input_id,
                observed_revision: self.next_mutation_revision()?,
                kind: crate::v1::business_hook::BusinessHookInputKind::Interrupt,
                content: text.clone(),
            },
        );
        staged_metadata
            .input_state
            .pending_in_turn_inputs
            .push(PendingInTurnInput {
                parameters,
                bound_request_config,
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

    /// Streams in-turn inputs that arrived after the final physical API completed.
    /// 将最后一次物理 API 完成之后到达的过程内输入写入主事件流。
    ///
    /// A successful Turn must not close over such inputs: they would sit in metadata, unseen by the
    /// model. The caller keeps the Turn open and starts one continuation for the returned inputs,
    /// exactly as when they are promoted at an API terminal.
    /// 成功回合不得在这类输入未处理时关闭，否则它们只停留在元数据中而模型看不到。调用方保持
    /// 回合开放，并为返回的输入启动一次续跑，与在 API 终态晋升时完全一致。
    ///
    /// # Returns / 返回值
    /// The ordered seq values of the newly streamed inputs; empty when nothing was pending.
    /// 新写入输入的有序 seq；没有待处理输入时为空。
    pub fn promote_pending_in_turn_inputs_after_api(
        &mut self,
        now_ms: i64,
    ) -> Result<Vec<Seq>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let pending_inputs = self.metadata.input_state.pending_in_turn_inputs.clone();
        if pending_inputs.is_empty() {
            return Ok(Vec::new());
        }
        let turn = self.require_open_turn()?;
        let turn_id = turn.turn_id;
        if turn.origin == TurnOrigin::QuestionExecution {
            return Err(DomainError::CollaborationStateViolation {
                field: "question_execution.business_inputs",
            });
        }
        if let Some(api) = &self.active_api {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        if pending_inputs
            .iter()
            .any(|pending| pending.turn_id != turn_id)
        {
            return Err(DomainError::InvalidQueuedInputOrder {
                field: "pending_input_turn_mismatch",
            });
        }
        let bound_request_config = pending_inputs
            .iter()
            .rev()
            .find_map(|pending| pending.bound_request_config.clone());
        let promotable: Vec<PendingInTurnInput> = pending_inputs
            .into_iter()
            .filter(|pending| !self.input_id_already_streamed(pending.input_id))
            .collect();
        for pending in &promotable {
            validate_user_input(pending.text.clone(), &pending.resource_ids)?;
            if let Some(parameters) = &pending.parameters {
                parameters.validate_pending(&pending.resource_ids)?;
            }
        }
        let promotable_count = u64::try_from(promotable.len())
            .map_err(|_| DomainError::invalid_id("pending_input_count", u64::MAX))?;
        self.ensure_transition_capacity(
            promotable_count + u64::from(bound_request_config.is_some()),
            0,
        )?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::new();
        let mut staged_metadata = self.metadata.clone();
        if let Some(request_config) = bound_request_config {
            self.stage_bound_request_activation(
                &mut staged_metadata,
                &mut staged_allocator,
                &mut staged_events,
                request_config,
                Some(turn_id),
                now_ms,
            )?;
        }
        let mut promoted_input_seqs = Vec::with_capacity(promotable.len());
        for pending in promotable {
            promoted_input_seqs.push(self.stage_event(
                &mut staged_allocator,
                &mut staged_events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    parameters: pending.parameters,
                    client_action_id: pending.client_action_id,
                    input_id: pending.input_id,
                    input_source: UserInputSource::PendingInTurn,
                    text: pending.text,
                    resource_ids: pending.resource_ids,
                }),
                now_ms,
            )?);
        }
        staged_metadata.input_state.pending_in_turn_inputs.clear();
        let changed_event_seqs: Vec<Seq> =
            staged_events.iter().map(ConversationEvent::seq).collect();
        self.id_allocator = staged_allocator;
        self.metadata = staged_metadata;
        self.events.extend(staged_events);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed_event_seqs,
        );
        Ok(promoted_input_seqs)
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
        parameters: Option<&crate::v1::UserPromptParameters>,
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
                && initial_preparation_matches(input.parameters.as_ref(), parameters)
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
        parameters: Option<&crate::v1::UserPromptParameters>,
        bound_request_config: Option<&ActiveRequestConfig>,
    ) -> Result<Option<EnqueueInputReceipt>, DomainError> {
        for queued in &self.metadata.input_state.queued_inputs {
            if queued.client_action_id != *client_action_id {
                continue;
            }
            if queued.text == text
                && queued.resource_ids == resource_ids
                && initial_preparation_matches(queued.parameters.as_ref(), parameters)
                && queued.bound_request_config.as_ref() == bound_request_config
            {
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
            if resolution.text == text
                && resolution.resource_ids == resource_ids
                && initial_preparation_matches(resolution.parameters.as_ref(), parameters)
            {
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
            if draft.text == text
                && draft.resource_ids == resource_ids
                && initial_preparation_matches(draft.parameters.as_ref(), parameters)
            {
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
                && initial_preparation_matches(input.parameters.as_ref(), parameters)
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
        let input_event = &self.events[input_index];
        let stats_event = &self.events[stats_index];
        // Turn closure now places its ordered TTL removals between the boundary and statistics.
        // 回合闭合会在边界与统计之间写入有序 TTL 移除事件。
        let mut boundary_index = stats_index;
        loop {
            let Some(previous_index) = boundary_index.checked_sub(1) else {
                return Ok(EnqueueInputDisposition::Queued);
            };
            boundary_index = previous_index;
            let candidate = &self.events[boundary_index];
            if candidate.turn_id == stats_event.turn_id
                && matches!(&candidate.event, EventBody::SystemContext(context)
                    if context.operation == SystemContextOperation::Remove)
            {
                continue;
            }
            break;
        }
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
        parameters: Option<&crate::v1::UserPromptParameters>,
    ) -> Result<Option<PendingInputId>, DomainError> {
        for pending in &self.metadata.input_state.pending_in_turn_inputs {
            if pending.client_action_id != *client_action_id {
                continue;
            }
            if pending.text == text
                && pending.resource_ids == resource_ids
                && initial_preparation_matches(pending.parameters.as_ref(), parameters)
            {
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
                && matches!(
                    input.input_source,
                    UserInputSource::PendingInTurn | UserInputSource::QueuedScheduled
                )
                && input.text == text
                && input.resource_ids == resource_ids
                && initial_preparation_matches(input.parameters.as_ref(), parameters)
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
