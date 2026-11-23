//! Starts fresh business turns using their exact durable queue identities.
//! 使用精确的可靠队列身份开启新的业务回合。

use super::*;
use crate::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};

impl SessionDocument {
    /// Finds an already scheduled root for the exact queue action, including after it has closed.
    /// 为精确排队动作查找已调度的根回合，包括已闭合的回合。
    /// Returns its turn ID if the corresponding queued root event is still available.
    /// 对应的排队根事件仍可读取时返回其回合 ID。
    pub fn scheduled_queued_turn_for_action(
        &self,
        client_action_id: &ClientActionId,
    ) -> Result<Option<TurnId>, DomainError> {
        self.events
            .iter()
            .find(|event| {
                matches!(
                    &event.event,
                    EventBody::UserInput(input)
                        if input.input_source == UserInputSource::QueuedScheduled
                            && input.client_action_id == *client_action_id
                )
            })
            .map(|event| {
                event.turn_id.ok_or(DomainError::InvalidQueuedInputOrder {
                    field: "scheduled_input_turn_id",
                })
            })
            .transpose()
    }

    /// Starts a fresh user turn from `identity` at `now_ms` after the prior turn has closed.
    /// 在前一回合闭合后，于 `now_ms` 用 `identity` 开启新的用户回合。
    /// Returns the new durable turn ID. An occupied, archived, or explicitly stopped session rejects admission.
    /// 返回新的持久回合 ID。会话被占用、归档或显式停止时拒绝准入。
    pub fn schedule_idle_queued_input(
        &mut self,
        identity: QueuedInputIdentity,
        now_ms: i64,
    ) -> Result<TurnId, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if self.active_turn().is_some_and(|turn| !turn.closed) || self.active_api().is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "pending_input.active_turn",
            });
        }
        if self.metadata.archived_at_ms.is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "pending_input.archived_session",
            });
        }
        if !matches!(
            self.collaboration_run_control(),
            CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle
        ) {
            return Err(DomainError::CollaborationStateViolation {
                field: "pending_input.stopped_turn",
            });
        }

        self.apply_atomic_transition(|document| {
            // The selected input and its run permission become durable in the same revision.
            // 选中的输入及其运行许可在同一个修订中持久化。
            let (turn_id, _, _) = document.schedule_queued_input_batch(&[&identity], now_ms)?;
            document.set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "idle-queued-input-running:{}",
                    turn_id.value()
                ))?,
                state: CollaborationRunControl::Running,
                changed_at_ms: now_ms,
            })?;
            Ok(turn_id)
        })
    }

    /// Schedules an ordered nonempty `inputs` batch in a new business turn and stamps it at `now_ms`.
    /// 在新业务回合中调度有序非空 `inputs` 批次，并使用 `now_ms` 记录时间。
    /// Returns the new turn, its original root identity, and the root sequence. Unselected queue entries stay in order.
    /// 返回新回合、原根输入身份和根序列，未选中的队列项保持原顺序。
    pub(super) fn schedule_queued_input_batch(
        &mut self,
        inputs: &[&QueuedInputIdentity],
        now_ms: i64,
    ) -> Result<(TurnId, QueuedInputIdentity, Seq), DomainError> {
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
        // turn_id belongs to the new business root, independent of the predecessor.
        // turn_id 属于新的业务根回合，独立于前一个回合。
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
        // The batch must have an authoritative root before transferring ownership.
        // 批次在转移归属前必须有权威根输入。
        let (input_id, client_action_id, input_seq) =
            root.ok_or(DomainError::InvalidQueuedInputOrder {
                field: "queued_input_schedule.empty_batch",
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
        Ok((
            turn_id,
            QueuedInputIdentity {
                input_id,
                client_action_id,
            },
            input_seq,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::config::ActiveRequestConfig;

    /// Checks that idle admission preserves the selected row's identity and payload across a failed turn.
    /// 检查失败回合后的空闲准入保留所选队列行的原始身份与内容。
    /// Takes no arguments, and verifies that explicit Stop rejects the same transition.
    /// 不接收参数，同时验证显式停止会拒绝同一转换。
    #[test]
    fn idle_queue_selection_preserves_bound_config_and_ordered_resources() {
        // Exercise Core independently from the provider and service.
        // 独立于供应商与服务来验证 Core。
        let mut document = SessionDocument::new("idle-queue", "project", "fixture", 1);
        // An open predecessor must block fresh admission.
        // 开放前驱必须阻止新回合准入。
        let turn = document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("original").unwrap(),
                    "Original request",
                    Vec::new(),
                ),
                2,
            )
            .unwrap();
        assert!(matches!(turn, IdempotentMutation::Committed(_)));
        // The three actions identify the order before selecting the middle row.
        // 三个动作标识选择中间条目前的队列顺序。
        let first = ClientActionId::try_new("queue-first").unwrap();
        let selected = ClientActionId::try_new("queue-selected").unwrap();
        let last = ClientActionId::try_new("queue-last").unwrap();
        // The resource IDs are intentionally out of numerical order.
        // 资源 ID 刻意使用非数字升序。
        let resources = vec![
            ResourceId::try_new(8).unwrap(),
            ResourceId::try_new(3).unwrap(),
        ];
        // The middle row owns an explicit model binding.
        // 中间条目拥有显式模型绑定。
        let config = ActiveRequestConfig {
            provider: Some("test-provider".into()),
            connection_id: Some("test-connection".into()),
            model: Some("test-model".into()),
            thinking_level: None,
            temperature: None,
        };
        document
            .enqueue_input(
                ClientInputSubmission::new(first.clone(), "First", Vec::new()),
                3,
            )
            .unwrap();
        document
            .enqueue_input(
                ClientInputSubmission::new(selected.clone(), "Selected", resources.clone())
                    .with_bound_request_config(config.clone()),
                4,
            )
            .unwrap();
        document
            .enqueue_input(
                ClientInputSubmission::new(last.clone(), "Last", Vec::new()),
                5,
            )
            .unwrap();
        // Use the actual admission identity assigned by the queue.
        // 使用队列分配的实际准入身份。
        let identity = document
            .queued_input_identity_for_action(&selected)
            .unwrap()
            .unwrap();
        assert!(matches!(
            document.schedule_idle_queued_input(identity.clone(), 6),
            Err(DomainError::CollaborationStateViolation {
                field: "pending_input.active_turn"
            })
        ));
        document
            .fail_execution_boundary_and_close_turn("fixture_failed", None, 7)
            .unwrap();
        // An explicitly stopped copy must decline the same action.
        // 显式停止的副本必须拒绝同一个动作。
        let mut stopped = document.clone();
        stopped
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("fixture-stop").unwrap(),
                state: CollaborationRunControl::Stopped,
                changed_at_ms: 8,
            })
            .unwrap();
        // Compare revisions to detect any partial mutation on rejection.
        // 对比修订以发现拒绝时的任何部分修改。
        let stopped_revision = stopped.memory_revision();
        assert!(matches!(
            stopped.schedule_idle_queued_input(identity.clone(), 9),
            Err(DomainError::CollaborationStateViolation {
                field: "pending_input.stopped_turn"
            })
        ));
        assert_eq!(stopped.memory_revision(), stopped_revision);
        assert_eq!(stopped.metadata.input_state.queued_inputs.len(), 3);

        // A failed predecessor releases ownership for one fresh turn.
        // 失败前驱释放归属，允许一个新回合。
        let turn_id = document
            .schedule_idle_queued_input(identity.clone(), 9)
            .unwrap();
        assert_eq!(document.metadata.config.active_request, config);
        assert_eq!(
            document.collaboration_run_control(),
            CollaborationRunControl::Running
        );
        assert_eq!(
            document
                .scheduled_queued_turn_for_action(&selected)
                .unwrap(),
            Some(turn_id)
        );
        assert!(document.events().iter().any(|event| {
            event.turn_id == Some(turn_id)
                && matches!(&event.event, EventBody::UserInput(input)
                    if input.input_source == UserInputSource::QueuedScheduled
                        && input.input_id == identity.input_id
                        && input.client_action_id == selected
                        && input.resource_ids == resources)
        }));
        assert_eq!(
            document
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .map(|item| &item.client_action_id)
                .collect::<Vec<_>>(),
            vec![&first, &last],
        );
        assert_eq!(
            document
                .metadata
                .input_state
                .pending_staging_release_actions,
            BTreeSet::from([selected]),
        );
    }
}
