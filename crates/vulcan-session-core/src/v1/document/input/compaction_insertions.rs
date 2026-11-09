//! Exact user-input handoff after a successful manual context summary.
//! 手动上下文摘要成功后的精确用户输入交接。

use super::*;
use crate::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};
use crate::v1::document::ApiGroupCompletionReceipt;
use crate::v1::event::TurnClosureReason;
use crate::v1::ids::CompactionId;

/// Identifies the durable input that the host may dispatch after releasing the summary owner.
/// 标识宿主释放摘要所有者后可分发的可靠输入。
pub enum CompactionInsertionDispatch {
    /// A queued batch opened a fresh business turn after independent maintenance.
    /// 独立维护结束后，排队批次开启了新的业务回合。
    NewTurn {
        /// Identity of the new business turn.
        /// 新业务回合身份。
        turn_id: TurnId,
        /// Original queue input identity.
        /// 原始排队输入身份。
        input_id: PendingInputId,
        /// Original client action identity.
        /// 原始客户端动作身份。
        client_action_id: ClientActionId,
        /// Durable sequence of the first business input.
        /// 首条业务输入的可靠序列。
        input_seq: Seq,
    },
    /// The inserted batch belongs to the existing business turn borrowed for compaction.
    /// 插入批次归属于压缩借用的现有业务回合。
    InTurn(ApiGroupCompletionReceipt),
}

impl SessionDocument {
    /// Admits the still-queued `identities` after the exact committed `compaction_id` and `terminal`.
    /// 在精确已提交 `compaction_id` 和 `terminal` 后准入仍在队列中的 `identities`。
    /// `now_ms` timestamps the input handoff. Returns a dispatch receipt, or none for cancelled or stale work.
    /// `now_ms` 为交接记录时间。返回分发回执，取消或过期的工作返回空。
    pub fn admit_compaction_insertions(
        &mut self,
        compaction_id: CompactionId,
        mut terminal: ApiGroupCompletionReceipt,
        identities: &[QueuedInputIdentity],
        now_ms: i64,
    ) -> Result<Option<CompactionInsertionDispatch>, DomainError> {
        validate_timestamp("compaction_insertions.updated_at_ms", now_ms)?;
        if identities.is_empty()
            || self.metadata.archived_at_ms.is_some()
            || self.active_api().is_some()
            || self.runtime_lock.current_compaction_id != Some(compaction_id)
            || !matches!(
                self.collaboration_run_control(),
                CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle
            )
            || !self.compactions.iter().any(|node| {
                node.compaction_id == compaction_id && node.api_call_id == terminal.api_call_id
            })
        {
            return Ok(None);
        }
        // turn must still be the summary's exact predecessor or borrowed owner.
        // turn 必须仍是摘要的精确前驱或被借用所有者。
        let Some(turn) = self
            .active_turn()
            .filter(|turn| turn.turn_id == terminal.turn_id)
        else {
            return Ok(None);
        };
        // inputs follows insertion order and excludes messages removed or restored to the composer meanwhile.
        // inputs 遵循插入顺序，排除期间被删除或恢复到输入框的消息。
        let inputs = identities
            .iter()
            .filter(|identity| {
                self.metadata
                    .input_state
                    .queued_inputs
                    .iter()
                    .any(|queued| {
                        queued.input_id == identity.input_id
                            && queued.client_action_id == identity.client_action_id
                    })
            })
            .collect::<Vec<_>>();
        if inputs.is_empty() {
            return Ok(None);
        }
        if !turn.closed
            && !matches!(
                turn.origin,
                TurnOrigin::ContextMaintenance | TurnOrigin::QuestionExecution
            )
        {
            return self.apply_atomic_transition(|document| {
                for identity in inputs {
                    document.promote_queued_input_to_pending_turn(identity.clone(), now_ms)?;
                }
                terminal
                    .promoted_input_seqs
                    .extend(document.promote_pending_in_turn_inputs_after_api(now_ms)?);
                Ok(Some(CompactionInsertionDispatch::InTurn(terminal)))
            });
        }
        // The independent maintenance terminal proves the old turn was settled successfully before opening business.
        // 独立维护的终态证明旧回合已成功结算，之后才能开启业务。
        if !turn.closed
            || turn.origin != TurnOrigin::ContextMaintenance
            || !self
                .metadata
                .context_maintenance
                .as_ref()
                .is_some_and(|record| {
                    record.turn_id == turn.turn_id
                        && record.terminal.as_ref().is_some_and(|end| {
                            end.reason == TurnClosureReason::Completed
                                && end.compaction_id == Some(compaction_id)
                        })
                })
        {
            return Ok(None);
        }
        self.apply_atomic_transition(|document| {
            document
                .schedule_compaction_insertion_batch(&inputs, now_ms)
                .map(Some)
        })
    }

    /// Schedules an ordered nonempty `inputs` batch in a new business turn and stamps it at `now_ms`.
    /// 在新业务回合中调度有序非空 `inputs` 批次，并使用 `now_ms` 记录时间。
    /// Returns the existing queued-turn receipt and keeps every unselected queue entry in place.
    /// 返回既有排队回合回执，并保持所有未选中队列项的顺序。
    fn schedule_compaction_insertion_batch(
        &mut self,
        inputs: &[&QueuedInputIdentity],
        now_ms: i64,
    ) -> Result<CompactionInsertionDispatch, DomainError> {
        // queued freezes the exact payloads and their attachment ownership before any staging.
        // queued 在暂存前冻结精确载荷及其附件归属。
        let queued = inputs
            .iter()
            .map(|identity| {
                self.require_queued_input_index(identity)
                    .map(|index| self.metadata.input_state.queued_inputs[index].clone())
            })
            .collect::<Result<Vec<_>, _>>()?;
        // bound_config preserves the last explicitly bound selection in the batch.
        // bound_config 保留批次最后一个显式绑定的请求选择。
        let bound_config = queued
            .iter()
            .rev()
            .find_map(|item| item.bound_request_config.clone());
        self.ensure_transition_capacity(
            queued.len() as u64 + u64::from(bound_config.is_some()),
            1,
        )?;
        // allocator, events, and metadata stage one transaction before the turn becomes visible.
        // allocator、events 和 metadata 在回合可见前暂存同一事务。
        let mut allocator = self.id_allocator;
        let mut events = Vec::new();
        let mut metadata = self.metadata.clone();
        // turn_id belongs to the new business root, independent of the completed summary.
        // turn_id 属于新的业务根回合，独立于已完成的摘要。
        let turn_id = TurnId::try_new(allocator.next_local_id()?)?;
        if let Some(config) = bound_config {
            self.stage_bound_request_activation(
                &mut metadata,
                &mut allocator,
                &mut events,
                config,
                Some(turn_id),
                now_ms,
            )?;
        }
        // root records the first canonical input while the remainder forms the same initial batch.
        // root 记录首条规范输入，其余输入组成同一个初始批次。
        let mut root = None;
        for item in queued {
            validate_user_input(item.text.clone(), &item.resource_ids)?;
            // input_seq is the canonical event sequence reused by the ordinary queued dispatcher.
            // input_seq 是普通排队分发器复用的规范事件序列。
            let input_seq = self.stage_event(
                &mut allocator,
                &mut events,
                Some(turn_id),
                EventVisibility::Provider,
                EventBody::UserInput(UserInputEvent {
                    parameters: item.parameters,
                    client_action_id: item.client_action_id.clone(),
                    input_id: item.input_id,
                    input_source: if root.is_none() {
                        UserInputSource::QueuedScheduled
                    } else {
                        UserInputSource::PendingInTurn
                    },
                    text: item.text,
                    resource_ids: item.resource_ids,
                }),
                now_ms,
            )?;
            metadata
                .input_state
                .pending_staging_release_actions
                .insert(item.client_action_id.clone());
            root.get_or_insert((item.input_id, item.client_action_id, input_seq));
        }
        let (input_id, client_action_id, input_seq) =
            root.ok_or(DomainError::InvalidQueuedInputOrder {
                field: "compaction_insertions.empty_batch",
            })?;
        metadata.input_state.queued_inputs.retain(|item| {
            !inputs
                .iter()
                .any(|identity| identity.input_id == item.input_id)
        });
        metadata.open_turn_start_seq = Some(input_seq);
        // changed identifies the exact events published with the selected queue removals.
        // changed 标识随所选队列移除一同发布的精确事件。
        let changed = events
            .iter()
            .map(ConversationEvent::seq)
            .collect::<Vec<_>>();
        self.id_allocator = allocator;
        self.events.extend(events);
        self.metadata = metadata;
        self.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::UserRoot,
            start_seq: input_seq,
            closed: false,
        });
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed,
        );
        self.set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "compaction-insert-running:{}",
                turn_id.value()
            ))?,
            state: CollaborationRunControl::Running,
            changed_at_ms: now_ms,
        })?;
        Ok(CompactionInsertionDispatch::NewTurn {
            turn_id,
            input_id,
            client_action_id,
            input_seq,
        })
    }
}
