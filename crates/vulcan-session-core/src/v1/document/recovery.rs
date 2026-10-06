//! Narrow recovery of proven Goal ownership defects and pre-API terminal failures.
//! 对已证实的目标归属缺陷和 API 前终态失败进行窄范围恢复。

use super::*;
use crate::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};

impl SessionDocument {
    /// Abandons the open turn after the host proves that no physical execution or preparation owns it.
    /// 宿主证明没有物理执行或准备拥有当前开放回合后，将其结算为中断。
    /// `now_ms` timestamps API cleanup and task results; returns the closure sequence atomically.
    /// `now_ms` 标记 API 清理和任务结果时间；原子返回闭合序号。
    /// Unlike a user stop, process loss must retain an abandoned outcome and never fabricate a reply.
    /// 与用户停止不同，进程丢失必须保留中断结果，绝不伪造回复。
    pub fn abandon_unowned_execution(&mut self, now_ms: i64) -> Result<Seq, DomainError> {
        let start_seq = self.require_open_turn()?.start_seq;
        let started_at_ms = self
            .events
            .iter()
            .find(|event| event.seq == start_seq)
            .ok_or_else(|| DomainError::reference("unowned_turn_start", start_seq.value()))?
            .created_at_ms;
        self.apply_atomic_transition(|document| {
            if document.active_api().is_some() {
                document.interrupt_active_api_state(
                    now_ms,
                    ApiEndReason::ProcessInterrupted,
                    false,
                )?;
            }
            document.close_turn(
                TurnOutcome::Abandoned,
                now_ms.saturating_sub(started_at_ms).max(0),
                now_ms,
            )
        })
    }

    /// Admits `submission` after the host proves execution is idle under exclusive admission.
    /// 宿主在独占准入下证明执行空闲后，接纳 `submission`。
    /// `now_ms` timestamps the new input; returns its receipt or leaves all old facts unchanged on error.
    /// `now_ms` 标记新输入时间；返回准入回执，失败则保留全部旧事实。
    /// An open journal is not a live worker. Preserve retries and stop only a superseded business turn.
    /// 开放日志不代表活跃工作器；保留重试，仅停止被新输入取代的业务回合。
    pub fn submit_root_input_after_execution_ended(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<TurnId>, DomainError> {
        // Stage validation, old closure and the new root together; invalid input cannot destroy Continue.
        // 将校验、旧回合闭合和新根一起暂存；无效输入不能破坏继续能力。
        self.apply_atomic_transition(move |document| {
            match document.submit_root_input(submission.clone(), now_ms) {
                Ok(receipt) => return Ok(receipt),
                Err(DomainError::RootInputWhileTurnOpen { .. }) => {}
                Err(error) => return Err(error),
            }
            // Auxiliary executions and undelivered children retain their exact settlement owners.
            // 辅助执行和未交付子任务保留各自精确结算所有者。
            let turn = document.require_open_turn()?.clone();
            if matches!(
                turn.origin,
                TurnOrigin::QuestionExecution | TurnOrigin::ContextMaintenance
            ) || !document
                .completion_obligations(turn.turn_id)
                .open_child_task_ids
                .is_empty()
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "root_input.unsettled_execution_owner",
                });
            }
            // Last durable observation excludes offline time from the stopped execution's duration.
            // 最后可靠观察时间避免将离线时间计入已停止执行时长。
            let observed_at_ms = document
                .events
                .iter()
                .filter(|event| event.turn_id == Some(turn.turn_id))
                .map(|event| event.updated_at_ms)
                .max()
                .unwrap_or(document.metadata.updated_at_ms);
            // Resolve the start by stable sequence instead of assuming a mutable event position.
            // 按稳定序号解析开始时间，不假设可变事件位置。
            let started_at_ms = document
                .events
                .iter()
                .find(|event| event.seq == turn.start_seq)
                .ok_or(DomainError::ReferenceViolation {
                    kind: "turn_start_event",
                    value: turn.start_seq.value(),
                })?
                .created_at_ms;
            if document.active_api().is_some() {
                // New user input supersedes even an undispatched Continue; cold loading alone does not.
                // 新用户输入也取代尚未派发的继续请求；单纯冷加载则不取代它。
                document.interrupt_active_api_state(
                    observed_at_ms,
                    ApiEndReason::ProcessInterrupted,
                    false,
                )?;
            }
            document.close_turn(
                TurnOutcome::Stopped,
                observed_at_ms.saturating_sub(started_at_ms).max(0),
                now_ms,
            )?;
            document.submit_root_input(submission, now_ms)
        })
    }

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

    /// Atomically fails an active execution boundary, forbids Continue, and closes its Turn.
    /// 原子标记活动执行边界失败、禁止继续，并闭合其所属回合。
    ///
    /// `reason_code` is the stable host failure identity, `redacted_detail` is the
    /// producer-sanitized original diagnostic, and `now_ms` is the exact observed
    /// boundary time. An active API is converted to a failed unknown/system terminal,
    /// all unfinished semantic and tool events are interrupted, one permanent display
    /// note is appended, and the open Turn is stopped. Exact replay returns `false`.
    /// `reason_code` 是稳定的宿主失败标识，`redacted_detail` 是生产端已脱敏的原始
    /// 诊断，`now_ms` 是精确的边界观测时间。活动 API 会转为失败的未知或系统终态，
    /// 所有未完成语义与工具事件会被中断，同时追加永久错误提示并停止开放回合。精确重放
    /// 返回 `false`。
    pub fn fail_execution_boundary_and_close_turn(
        &mut self,
        reason_code: &str,
        redacted_detail: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("failure_time", now_ms)?;
        // base_revision is the only revision visible before the isolated composite transition.
        // base_revision 是隔离复合转换开始前唯一对外可见的修订号。
        let base_revision = self.memory_revision;
        // candidate rolls the document back unless every fallible transition succeeds.
        // 除非所有可能失败的转换都成功，candidate 会回滚文档。
        let mut candidate = self.begin_atomic_mutation()?;
        // failure_event_start isolates facts appended by the failure-and-closure half of the transaction.
        // failure_event_start 隔离失败与关闭半段事务新追加的事实。
        let failure_event_start = candidate.events.len();
        // had_active_api records whether this transition must replace a generated API terminal.
        // had_active_api 记录当前转换是否必须替换生成的 API 终态。
        let had_active_api = candidate.active_api.is_some();
        // failed_api_identity retains the exact physical API and logical Turn for error correlation.
        // failed_api_identity 保留用于错误关联的精确物理 API 与逻辑回合身份。
        let failed_api_identity = if had_active_api {
            // Shared API cleanup does not decide whether a business or auxiliary turn closes.
            // 共享 API 清理不决定闭合业务回合还是辅助回合。
            let completion =
                candidate.fail_active_api_for_execution(reason_code, redacted_detail, now_ms)?;
            Some((completion.api_call_id, completion.turn_id))
        } else {
            None
        };
        // failure_changed reports whether the same atomic candidate also added or closed turn facts.
        // failure_changed 报告同一原子候选是否还新增或闭合了回合事实。
        let failure_changed =
            candidate.fail_turn_without_active_api(reason_code, redacted_detail, now_ms)?;
        if !had_active_api && !failure_changed {
            return Ok(false);
        }
        if let Some((api_call_id, turn_id)) = failed_api_identity {
            // failure_note is the unique newly appended SystemError correlated to the failed API.
            // failure_note 是与失败 API 关联的唯一新追加 SystemError。
            let missing_note =
                || DomainError::reference("execution_failure_display_note", api_call_id.value());
            let failure_index = candidate.events[failure_event_start..]
                .iter()
                .position(|event| {
                    event.turn_id == Some(turn_id)
                        && matches!(
                            &event.event,
                            EventBody::DisplayNote(note)
                                if note.notice_kind == DisplayNoticeKind::SystemError
                                    && note.reason_code == reason_code
                        )
                })
                .map(|offset| failure_event_start + offset)
                .ok_or_else(missing_note)?;
            let Some(ConversationEvent {
                event: EventBody::DisplayNote(failure_note),
                ..
            }) = candidate.events.get_mut(failure_index)
            else {
                return Err(missing_note());
            };
            failure_note.api_call_id = Some(api_call_id);
            candidate.validate_execution_failure_revision(base_revision)?;
        }
        candidate.commit();
        Ok(true)
    }

    /// Fails the current API and all unfinished semantics without closing its logical turn.
    /// 标记当前 API 及全部未完成语义失败，但不关闭其逻辑回合。
    /// The caller must prove exact execution ownership and commit the enclosing terminal transaction.
    /// 调用方必须证明精确执行所有权，并提交外层终态事务。
    /// Returns the exact API terminal receipt used for permanent failure-note correlation.
    /// 返回用于关联永久失败提示的精确 API 终态回执。
    pub(super) fn fail_active_api_for_execution(
        &mut self,
        reason_code: &str,
        redacted_detail: Option<&str>,
        now_ms: i64,
    ) -> Result<ApiGroupCompletionReceipt, DomainError> {
        // Interruption produces one staged terminal while preserving native tool structure.
        // 中断在保留原生工具结构的同时产生一个暂存终态。
        let (_, completion) =
            self.interrupt_active_api_state(now_ms, ApiEndReason::ProcessInterrupted, false)?;
        // Only the new terminal event can be rewritten before this transaction is published.
        // 只有当前新终态事件可在事务发布前被改写。
        let stats_index = self
            .events
            .iter()
            .position(|event| event.seq == completion.stats_seq)
            .ok_or_else(|| {
                DomainError::reference("api_stats_event", completion.stats_seq.value())
            })?;
        let stats_event = self.events.get_mut(stats_index).ok_or_else(|| {
            DomainError::reference("api_stats_event", completion.stats_seq.value())
        })?;
        // The exact receipt must refer to an API statistic, never an unrelated event.
        // 精确回执必须引用 API 统计，而非无关事件。
        let EventBody::ApiStats(stats) = &mut stats_event.event else {
            return Err(DomainError::reference(
                "api_stats_event",
                completion.stats_seq.value(),
            ));
        };
        stats.status = ApiCallStatus::Failed;
        stats.end_reason = Some(ApiEndReason::Unknown);
        stats.error = Some(RedactedApiError {
            category: ApiErrorCategory::Unknown,
            error_code: Some(reason_code.to_owned()),
            message: redacted_detail.unwrap_or(reason_code).to_owned(),
        });
        stats.continuation_eligibility = ContinuationEligibility::Forbidden;
        Ok(completion)
    }

    /// Verifies that all failure facts belong to the reserved atomic publication.
    /// 校验全部失败事实都属于预留的原子发布。
    /// `base_revision` is the revision before API cleanup; mismatched ownership returns an error.
    /// `base_revision` 是 API 清理前的修订；所有权不匹配时返回错误。
    fn validate_execution_failure_revision(&self, base_revision: u64) -> Result<(), DomainError> {
        // Failure receipts are created under one reserved revision, never renumbered afterward.
        // 失败回执在同一预留修订下创建，绝不在事后重新编号。
        let revision = self.staged_commit_revision.ok_or_else(|| {
            DomainError::reference("execution_failure_atomic_owner", self.memory_revision)
        })?;
        if self.memory_revision != revision
            || (base_revision != revision && base_revision.checked_add(1) != Some(revision))
        {
            return Err(DomainError::reference(
                "execution_failure_revision_span",
                self.memory_revision,
            ));
        }
        Ok(())
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
        // Cold loading discards closed active-turn caches; the latest terminal journal retains replay identity.
        // 冷加载丢弃已关闭活动回合缓存；最近终态日志保留重放身份。
        if self.active_turn().is_none()
            && self.active_api().is_none()
            && let Some(record) = self.metadata.context_maintenance.as_ref()
            && let Some(terminal) = record.terminal.as_ref()
            && self.events.iter().rev().find(|event| matches!(event.event, EventBody::TurnStats(_)))
                .is_some_and(|event| event.seq == terminal.end_seq && event.turn_id == Some(record.turn_id))
            && self.events.iter().any(|event| event.turn_id == Some(record.turn_id)
                && event.seq >= record.started_seq && event.seq < terminal.end_seq
                && matches!(&event.event, EventBody::DisplayNote(note)
                    if note.notice_kind == DisplayNoticeKind::SystemError && note.reason_code == reason_code))
        {
            return Ok(false);
        }
        if self
            .active_turn()
            .is_some_and(|turn| !turn.closed && turn.origin == TurnOrigin::ContextMaintenance)
        {
            // Keep the diagnostic and auxiliary closure in one publication, including API failure callers.
            // 将诊断与辅助闭合保持在同一次发布中，也支持 API 失败调用方。
            let mut staged = self.begin_atomic_mutation()?;
            // The open maintenance turn owns the diagnostic; the completed business must stay untouched.
            // 开放维护回合拥有诊断；已完成业务必须保持不变。
            let turn_id = staged.require_open_turn()?.turn_id;
            // Resolve the exact execution before staging any externally visible state.
            // 在暂存任何对外可见状态前解析精确执行。
            let execution_id = staged
                .metadata
                .context_maintenance
                .as_ref()
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.owner",
                })?
                .execution_id
                .clone();
            staged.append_display_note(
                Some(turn_id),
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Error,
                reason_code,
                Some(redacted_detail.unwrap_or(reason_code).to_owned()),
                None,
                None,
                None,
                now_ms,
            )?;
            staged.finish_child_context_maintenance(
                &execution_id,
                crate::v1::event::TurnClosureReason::Failed,
                now_ms,
            )?;
            staged.commit();
            return Ok(true);
        }
        // One reserved revision owns question attempt closure, business closure and idle control.
        // 同一预留修订拥有问题处理尝试闭合、业务闭合与空闲控制。
        let mut staged = self.begin_atomic_mutation()?;
        let business_turn_id = staged
            .active_turn()
            .filter(|turn| !turn.closed && turn.origin != TurnOrigin::QuestionExecution)
            .map(|turn| turn.turn_id);
        if let Some(turn_id) = business_turn_id {
            staged.finish_in_turn_question_executions(turn_id, now_ms)?;
        }
        if !staged.fail_turn_without_active_api_after_questions(
            reason_code,
            redacted_detail,
            now_ms,
        )? {
            // Interrupted requests retain their Continue path and every original question attempt.
            // 已中断请求保留 Continue 路径及全部原问题处理尝试。
            return Ok(false);
        }
        if let Some(turn_id) = business_turn_id
            && staged.collaboration_run_control() == CollaborationRunControl::Running
        {
            let operation_id = CollaborationOperationId::try_new(format!(
                "business-failure-idle:{}:{}",
                staged.segment_generation(),
                turn_id.value()
            ))?;
            staged.set_collaboration_run_control(SetRunControlInput {
                operation_id,
                state: CollaborationRunControl::NaturallyIdle,
                changed_at_ms: now_ms,
            })?;
        }
        if let Some(turn_id) = business_turn_id {
            staged.record_task_results_for_turn(turn_id, now_ms)?;
        }
        staged.commit();
        Ok(true)
    }

    /// Stages the failure note and exact closure after question attempts have been handled.
    /// 在问题处理尝试收尾后暂存失败提示与精确闭合。
    /// The arguments retain the public failure contract; false leaves the surrounding draft unpublished.
    /// 参数沿用公开失败契约；返回 false 时外层草稿不发布。
    fn fail_turn_without_active_api_after_questions(
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
        // Preserve the business interruption even after auxiliary work; an auxiliary interruption cannot retain business.
        // 辅助工作后仍保留业务中断；辅助中断不能保留业务回合。
        if let Some(turn) = &open_turn
            && self.events.iter().rev().find_map(|event| {
                if event.turn_id != Some(turn.turn_id) {
                    return None;
                }
                match &event.event {
                    EventBody::ApiStats(stats)
                        if turn.origin == TurnOrigin::QuestionExecution
                            || !matches!(
                                stats.purpose,
                                ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
                            ) =>
                    {
                        Some(stats.end_reason)
                    }
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
        let expiring_contexts = open_turn
            .as_ref()
            .map(|turn| super::turns::expiring_ttl_contexts(self, turn.turn_id))
            .transpose()?
            .unwrap_or_default();
        let ttl_expiration_count = u64::try_from(expiring_contexts.len()).map_err(|_| {
            DomainError::ImmutableFieldMutation {
                entity: "ttl_expiration",
                field: "event_count",
            }
        })?;
        self.next_mutation_revision()?;
        // Reserve the whole error-plus-closure transaction before any state changes.
        // 在任何状态变化前为完整的错误加闭合事务预留容量。
        let event_count = u64::from(!already_recorded)
            + if open_turn.is_some() { 2 } else { 0 }
            + ttl_expiration_count;
        self.ensure_transition_capacity(event_count, 0)?;
        // allocator and staged stay detached until every fallible validation has completed.
        // allocator 和 staged 在所有可失败校验完成前保持独立。
        let mut allocator = self.id_allocator;
        let mut staged = Vec::with_capacity(event_count as usize);
        let mut staged_metadata = self.metadata.clone();
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
                crate::v1::event::TurnClosureReason::Failed,
                now_ms.saturating_sub(started_at).max(0),
                now_ms,
                buckets,
            )?;
            super::turns::clear_expired_dynamic_context_metadata(
                &mut staged_metadata,
                &expiring_contexts,
            );
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
        }
        self.metadata = staged_metadata;
        if open_turn.is_some() {
            self.metadata.open_turn_start_seq = None;
        }
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &changed,
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::dynamic_context::DynamicContextInput;

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

    /// Pre-API failure closes a TTL generation once and clears its matching metadata entry.
    /// API 前失败只闭合一次 TTL 代际，并清理其匹配的 metadata 条目。
    #[test]
    fn pre_api_failure_persists_ttl_expiration_and_cleans_metadata() -> Result<(), DomainError> {
        let mut document = SessionDocument::new("pre-api-ttl", "project", "failure", 1);
        document.synchronize_dynamic_context(
            vec![DynamicContextInput::new(
                "failure-ttl",
                Some(String::from("expires on recovery")),
                SystemContextScope::Session,
                Some(1),
            )],
            1,
        )?;
        let turn_id = document.submit_root_input_fixture("work", Vec::new(), 2)?;
        assert!(document.fail_turn_without_active_api("stream_failed", None, 3)?);

        let expiration_events: Vec<_> = document
            .events()
            .iter()
            .filter(|event| {
                matches!(
                    &event.event,
                    EventBody::SystemContext(context)
                        if event.turn_id == Some(turn_id)
                            && context.context_key == "failure-ttl"
                            && context.operation == SystemContextOperation::Remove
                )
            })
            .collect();
        assert_eq!(expiration_events.len(), 1);
        assert!(matches!(
            &expiration_events[0].event,
            EventBody::SystemContext(context)
                if context.content.contains("expired after logical turn")
                    && context.ttl_turns.is_none()
        ));
        assert!(document.metadata.dynamic_context.is_empty());
        Ok(())
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
