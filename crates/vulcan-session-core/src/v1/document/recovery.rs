//! Narrow recovery of proven Goal ownership defects and pre-API terminal failures.
//! 对已证实的目标归属缺陷和 API 前终态失败进行窄范围恢复。

use super::*;

impl SessionDocument {
    /// Restores loaded parts, correcting only paired legacy goal edits in the mutable tail.
    /// 恢复已加载部件，仅纠正可写尾段内成对的旧目标编辑事件。
    /// Inputs are the same exact parts as `from_parts`; returns a strictly validated dirty candidate.
    /// 输入与 `from_parts` 相同；返回经过严格校验的待持久化候选。
    /// No event is deleted or resequenced; the caller must flush before publishing the owner.
    /// 不删除事件或重排序号；调用方必须在发布 owner 前完成 flush。
    pub fn from_parts_with_goal_owner_recovery(
        metadata: MetadataDocument,
        runtime_lock: RuntimeLock,
        compactions: Vec<CompactionNode>,
        segment_catalog: Vec<SegmentInfo>,
        mut events: Vec<ConversationEvent>,
    ) -> Result<Self, DomainError> {
        // Sealed history is immutable; automatic correction is limited to the published active tail.
        // 封口历史不可变；自动纠正仅限已发布的活动尾段。
        let tail_start = segment_catalog
            .last()
            .filter(|segment| segment.end_seq.is_none())
            .map(|segment| segment.start_seq);
        // closed tracks terminal facts, not guessed ownership from numeric IDs.
        // closed 跟踪终态事实，不从数字 ID 猜测归属。
        let mut closed = BTreeSet::new();
        // corrected is the exact event patch that the writer must publish atomically.
        // corrected 是写入器必须原子发布的精确事件补丁。
        let mut corrected = Vec::new();
        for index in 0..events.len() {
            let event = &events[index];
            if matches!(event.event, EventBody::TurnStats(_)) {
                if let Some(turn_id) = event.turn_id {
                    closed.insert(turn_id);
                }
                continue;
            }
            let Some(turn_id) = event.turn_id.filter(|turn| closed.contains(turn)) else {
                continue;
            };
            if !tail_start.is_some_and(|start| event.seq >= start) {
                continue;
            }
            // This exact pair came from update_goal_definition using a closed active_turn.
            // 此精确事件对来自 update_goal_definition 使用已结束的 active_turn。
            let is_goal_edit = matches!(&event.event, EventBody::SystemContext(context)
                if context.context_key == "goal_state" && context.scope == SystemContextScope::Session
                    && context.operation == SystemContextOperation::Upsert && context.ttl_turns.is_none()
                    && context.provider_role_hint == Some(SystemRoleHint::Developer))
                && event.visibility == EventVisibility::Provider;
            let paired_notice = events.get(index + 1).is_some_and(|next|
                next.turn_id == Some(turn_id) && next.created_at_ms == event.created_at_ms
                    && next.visibility == EventVisibility::UiOnly
                    && matches!(&next.event, EventBody::DisplayNote(note)
                        if note.notice_kind == DisplayNoticeKind::GoalStatus && note.reason_code == "goal_saved"
                            && note.severity == NoticeSeverity::Info && note.api_call_id.is_none()
                            && note.redacted_detail.is_none() && note.retry_count.is_none() && note.retry_limit.is_none()));
            if is_goal_edit && paired_notice {
                for changed in &mut events[index..index + 2] {
                    changed.turn_id = None;
                    changed.event_revision = changed
                        .event_revision
                        .checked_add(1)
                        .ok_or(DomainError::invalid_id("event_revision", u64::MAX))?;
                    corrected.push(changed.seq);
                }
            }
        }
        // Keep every original validator: any unrelated corruption rejects the entire candidate.
        // 保留全部原始校验：任何无关损坏都会拒绝整个候选。
        let mut document =
            Self::from_parts(metadata, runtime_lock, compactions, segment_catalog, events)?;
        if !corrected.is_empty() {
            // Corrupt revision frontiers must fail before staging a durable recovery, never overflow.
            // 损坏的修订边界必须在暂存可靠恢复前失败，绝不能溢出。
            document
                .memory_revision
                .checked_add(1)
                .ok_or(DomainError::invalid_id(
                    "document_revision",
                    document.memory_revision,
                ))?;
            document.ensure_transition_capacity(0, 0)?;
            document.bump_revision(
                &[DirtyFile::ActiveTail],
                document.metadata.updated_at_ms,
                &corrected,
            );
        }
        Ok(document)
    }

    /// Atomically records `reason_code` and closes an open Turn when no API owns execution.
    /// 没有 API 持有执行时，原子记录 `reason_code` 并闭合开放回合。
    /// `redacted_detail` is the producer's safe original failure, not a replacement diagnostic code.
    /// `redacted_detail` 是生产端已安全处理的原始失败，不是用于替换它的诊断编号。
    /// `now_ms` is the observed failure time; returns false on exact replay without new facts.
    /// `now_ms` 为观察到的失败时间；精确重放且无新事实时返回 false。
    pub fn fail_turn_without_active_api(
        &mut self,
        reason_code: &str,
        redacted_detail: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("failure_time", now_ms)?;
        // open_turn captures only the current execution owner, never its ended predecessor.
        // open_turn 仅捕获当前执行 owner，绝不捕获已经结束的前序回合。
        let open_turn = self
            .active_turn
            .as_ref()
            .filter(|turn| !turn.closed)
            .cloned();
        if let Some(api) = &self.active_api {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: api.turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        // An interrupted physical API already has a terminal fact and keeps its explicit Continue path.
        // 已中断的物理 API 已有终态事实，必须保留其显式 Continue 路径。
        if let Some(turn) = &open_turn
            && self.events.iter().rev().find_map(|event| {
                if event.turn_id != Some(turn.turn_id) {
                    return None;
                }
                match &event.event {
                    EventBody::ApiStats(stats) => Some(stats.end_reason),
                    _ => None,
                }
            }) == Some(Some(ApiEndReason::ProcessInterrupted))
        {
            return Ok(false);
        }
        // Include the just-closed owner for idempotency, but never append new facts to it.
        // 为幂等检查包含刚结束的 owner，但绝不向其追加新事实。
        let last_turn_id = self.active_turn.as_ref().map(|turn| turn.turn_id);
        // current_start bounds session-scoped error replay to the current logical lifetime.
        // current_start 将会话级错误重放限定在当前逻辑生命周期内。
        let current_start = self.active_turn.as_ref().map(|turn| turn.start_seq);
        // already_recorded prevents duplicate notices while still allowing a missing closure to be added.
        // already_recorded 防止重复提示，同时仍允许补齐缺失的闭合。
        let already_recorded = self.events.iter().rev().any(|event|
            (event.turn_id == last_turn_id || (event.turn_id.is_none() && current_start.is_none_or(|start| event.seq >= start)))
                && matches!(&event.event, EventBody::DisplayNote(note)
                if note.notice_kind == DisplayNoticeKind::SystemError && note.reason_code == reason_code));
        if already_recorded && open_turn.is_none() {
            return Ok(false);
        }
        self.memory_revision
            .checked_add(1)
            .ok_or(DomainError::invalid_id(
                "document_revision",
                self.memory_revision,
            ))?;
        // Reserve the whole error-plus-closure transaction before any state changes.
        // 在任何状态变化前为完整的错误加闭合事务预留容量。
        let event_count = u64::from(!already_recorded) + if open_turn.is_some() { 2 } else { 0 };
        self.ensure_transition_capacity(event_count, 0)?;
        // allocator and staged stay detached until every fallible validation has completed.
        // allocator 和 staged 在所有可失败校验完成前保持独立。
        let mut allocator = self.id_allocator;
        let mut staged = Vec::new();
        if !already_recorded {
            self.stage_event(
                &mut allocator,
                &mut staged,
                open_turn.as_ref().map(|turn| turn.turn_id),
                EventVisibility::UiOnly,
                EventBody::DisplayNote(DisplayNoteEvent {
                    notice_kind: DisplayNoticeKind::SystemError,
                    severity: NoticeSeverity::Error,
                    reason_code: reason_code.to_owned(),
                    redacted_detail: Some(redacted_detail.unwrap_or(reason_code).to_owned()),
                    api_call_id: None,
                    retry_count: None,
                    retry_limit: None,
                }),
                now_ms,
            )?;
        }
        if let Some(turn) = &open_turn {
            let started_at = self
                .events
                .iter()
                .find(|event| event.seq == turn.start_seq)
                .ok_or(DomainError::reference(
                    "turn_start_event",
                    turn.start_seq.value(),
                ))?
                .created_at_ms;
            let buckets = self.aggregate_turn_buckets(turn.turn_id, turn.start_seq)?;
            self.stage_turn_closure(
                &mut allocator,
                &mut staged,
                turn.turn_id,
                turn.start_seq,
                TurnOutcome::Stopped,
                now_ms.saturating_sub(started_at).max(0),
                now_ms,
                buckets,
            )?;
        }
        // Publish all changed event identities through one revision receipt.
        // 通过同一个修订回执发布全部变化事件身份。
        let changed: Vec<Seq> = staged.iter().map(ConversationEvent::seq).collect();
        self.id_allocator = allocator;
        self.events.extend(staged);
        if open_turn.is_some() {
            self.active_turn
                .as_mut()
                .expect("open turn was captured")
                .closed = true;
            self.metadata.open_turn_start_seq = None;
        }
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &changed);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// New events targeting ended Turns are rejected without allocating seq or mutating state.
    /// 指向结束回合的新事件被拒绝，不分配 seq 或修改状态。
    #[test]
    fn closed_turn_append_is_rejected_before_mutation() {
        let mut document = SessionDocument::new("guard", "project", "guard", 1);
        document.create_goal("work", None, "work", 1).unwrap();
        let ended = document.active_turn().unwrap().turn_id;
        document.close_turn(TurnOutcome::Completed, 1, 2).unwrap();
        let before = document.clone();
        assert!(matches!(
            document.append_display_note(
                Some(ended),
                DisplayNoticeKind::GoalStatus,
                NoticeSeverity::Info,
                "goal_saved",
                None,
                None,
                None,
                None,
                3
            ),
            Err(DomainError::TurnClosureMismatch { .. })
        ));
        assert_eq!(document, before);
        document
            .update_goal_definition("updated", None, "updated", 3)
            .unwrap();
        assert!(
            document
                .events()
                .iter()
                .rev()
                .take(2)
                .all(|event| event.turn_id.is_none())
        );
        document.rebuild_derived_state().unwrap();
    }

    /// Live pre-API failures close atomically and replay without reopening the Turn.
    /// 实时 API 前失败原子闭合，重放不会重新开启回合。
    #[test]
    fn pre_api_failure_closes_turn_once_and_accepts_new_input() {
        let mut document = SessionDocument::new("pre-api", "project", "failure", 1);
        document.create_goal("work", None, "work", 1).unwrap();
        document.block_goal("goal_blocked", 2).unwrap();
        let revision = document.memory_revision();
        assert!(
            document
                .fail_turn_without_active_api("stream_finished_without_v1_api_terminal", None, 3)
                .unwrap()
        );
        assert_eq!(document.memory_revision(), revision + 1);
        assert!(
            !document
                .fail_turn_without_active_api("stream_finished_without_v1_api_terminal", None, 4)
                .unwrap()
        );
        document.rebuild_derived_state().unwrap();
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("new-prompt").unwrap(),
                    "continue",
                    vec![],
                ),
                5,
            )
            .unwrap();
    }

    /// Error replay after an already closed Turn must use session scope and remain idempotent.
    /// 已闭合回合后的错误重放必须使用会话作用域，并保持幂等。
    #[test]
    fn failure_after_closed_turn_does_not_reopen_or_duplicate() {
        let mut document = SessionDocument::new("closed-failure", "project", "failure", 1);
        document.create_goal("work", None, "work", 1).unwrap();
        document.close_turn(TurnOutcome::Completed, 1, 2).unwrap();
        assert!(
            document
                .fail_turn_without_active_api("stream_failed", None, 3)
                .unwrap()
        );
        assert!(
            !document
                .fail_turn_without_active_api("stream_failed", None, 4)
                .unwrap()
        );
        assert!(document.events().last().unwrap().turn_id.is_none());
        document.rebuild_derived_state().unwrap();
    }

    /// An invalid revision frontier must return an error before failure settlement changes any fact.
    /// 非法修订边界必须在失败结算改变任何事实前返回错误。
    #[test]
    fn failure_revision_overflow_is_atomic() {
        let mut document = SessionDocument::new("overflow", "project", "failure", 1);
        document.create_goal("work", None, "work", 1).unwrap();
        document.memory_revision = u64::MAX;
        let before = document.clone();
        assert!(
            document
                .fail_turn_without_active_api("stream_failed", None, 2)
                .is_err()
        );
        assert_eq!(document, before);
    }

    /// A blocked goal's persisted pre-API failure must not survive restart as an open Turn.
    /// 受阻目标的持久化 API 前失败不能在重启后继续占用开放回合。
    #[test]
    fn blocked_pre_api_failure_is_recovered_without_resuming_goal() {
        let mut document = SessionDocument::new("legacy-pre-api", "project", "failure", 1);
        document.create_goal("work", None, "work", 1).unwrap();
        document.block_goal("goal_blocked", 2).unwrap();
        let turn = document.active_turn().unwrap().turn_id;
        document
            .append_display_note(
                Some(turn),
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Error,
                "stream_finished_without_v1_api_terminal",
                None,
                None,
                None,
                None,
                3,
            )
            .unwrap();
        assert_eq!(document.project_persisted_interruptions().unwrap(), 2);
        assert_eq!(document.project_persisted_interruptions().unwrap(), 0);
        assert!(document.active_turn().unwrap().closed);
        assert_eq!(document.metadata.mode.goal.state, GoalState::Blocked);
        document.rebuild_derived_state().unwrap();
    }
}
