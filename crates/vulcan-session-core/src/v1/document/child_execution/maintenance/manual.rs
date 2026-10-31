//! Explicit root-session maintenance without reopening a business turn or inventing user input.
//! 显式主会话维护，不重开业务回合，也不制造用户输入。

use super::*;
use crate::v1::context_maintenance::ManualContextMaintenanceSource;

impl SessionDocument {
    /// Captures a durable source without admitting execution; callers must serialize it with projection capture.
    /// 捕获可靠来源但不准入执行；调用方必须将其与投影捕获串行化。
    /// Returns opaque metadata, context and event-version evidence, or rejects an unpublished source.
    /// 返回不透明元数据、上下文及事件版本证据，或拒绝未发布来源。
    pub fn capture_manual_context_maintenance_source(
        &self,
    ) -> Result<ManualContextMaintenanceSource, DomainError> {
        if self.memory_revision() != self.durable_revision() {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.source_unpublished",
            });
        }
        Ok(ManualContextMaintenanceSource {
            metadata: self.metadata.clone(),
            runtime_lock: self.runtime_lock.clone(),
            event_revisions: self
                .events()
                .iter()
                .map(|event| (event.seq, event.event_revision))
                .collect(),
        })
    }

    /// Opens a standalone compaction owner for the captured idle source, allowing only draft changes.
    /// 为捕获的空闲来源开启独立压缩所有者，仅允许草稿变化。
    /// `source` binds the prepared history and controls; `execution_id` identifies this attempt; `now_ms` timestamps admission.
    /// `source` 绑定已准备历史与控制；`execution_id` 标识本次尝试；`now_ms` 记录准入时间。
    /// Returns the durable start fact, or rejects stale, busy, dedicated or child-session state without mutation.
    /// 返回可靠起始事实，或无修改地拒绝过期、忙碌、专职或子会话状态。
    pub fn begin_manual_context_maintenance(
        &mut self,
        source: &ManualContextMaintenanceSource,
        execution_id: CollaborationExecutionId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ContextMaintenanceExecution>, DomainError> {
        validate_timestamp("context_maintenance.started_at_ms", now_ms)?;
        if let Some(existing) = &self.metadata.context_maintenance
            && existing.execution_id == execution_id
        {
            if existing.business_execution_id.is_some()
                || existing.source_revision != source.metadata.document_revision
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.reused_identity",
                });
            }
            return Ok(IdempotentMutation::Idempotent(existing.clone()));
        }
        // Slash-command activation and draft autosave run independently; neither changes the Provider history.
        // 斜杠命令激活与草稿自动保存独立执行；二者都不改变供应商历史。
        // Ignore only the draft and its general write bookkeeping; retain every other source field exactly.
        // 仅忽略草稿及其通用写入记账；其他来源字段全部保持精确。
        let mut admission_metadata = self.metadata.clone();
        admission_metadata.input_state.draft = source.metadata.input_state.draft.clone();
        admission_metadata.document_revision = source.metadata.document_revision;
        admission_metadata.updated_at_ms = source.metadata.updated_at_ms;
        if self.durable_revision() < source.metadata.document_revision
            || admission_metadata != source.metadata
            || self.runtime_lock != source.runtime_lock
            || !self
                .events()
                .iter()
                .map(|event| (event.seq, event.event_revision))
                .eq(source.event_revisions.iter().copied())
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.source_changed",
            });
        }
        if self.metadata.child_execution_profile.is_some()
            || self.metadata.dedicated.is_some()
            || self.metadata.archived_at_ms.is_some()
            || self.active_api().is_some()
            || self.active_turn().is_some_and(|turn| !turn.closed)
            || !self.metadata.input_state.queued_inputs.is_empty()
            || !self.metadata.input_state.pending_in_turn_inputs.is_empty()
            || self
                .metadata
                .context_maintenance
                .as_ref()
                .is_some_and(|record| record.terminal.is_none())
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.unavailable",
            });
        }
        // A failed or stopped business turn is still a valid source; its outcome must remain unchanged.
        // 失败或停止的业务回合仍是有效来源；其结果必须保持不变。
        let business_turn_id = self
            .events()
            .iter()
            .rev()
            .find_map(|event| match &event.event {
                EventBody::TurnBoundary(boundary)
                    if boundary.purpose == TurnBoundaryPurpose::Business =>
                {
                    event.turn_id
                }
                _ => None,
            })
            .ok_or(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.business_missing",
            })?;
        self.ensure_transition_capacity(1, 1)?;
        // Stage the exact auxiliary owner and its journal anchor before publishing any state.
        // 在发布任何状态前暂存精确辅助所有者及其日志锚点。
        let mut allocator = self.id_allocator;
        // The fresh turn owns compaction APIs only, independently from the historical business.
        // 新回合只拥有压缩 API，与历史业务独立。
        let turn_id = TurnId::try_new(allocator.next_local_id()?)?;
        // The start record is the recovery authority after a crash or cold navigation.
        // 起始记录是崩溃或冷导航后的恢复权威。
        let started_seq = Seq::try_new(allocator.frontier().1)?;
        // Root business has no child execution identity; preserve the real source turn instead.
        // 主业务没有子执行身份；保留真实来源回合。
        let record = ContextMaintenanceExecution {
            execution_id,
            business_execution_id: None,
            business_turn_id,
            turn_id,
            segment_generation: self.segment_generation(),
            source_revision: source.metadata.document_revision,
            source_compaction_id: self.runtime_lock.current_compaction_id,
            started_seq,
            started_at_ms: now_ms,
            terminal: None,
        };
        record.validate(self.segment_generation())?;
        // The staged event is host-only and cannot become a fabricated Provider user message.
        // 暂存事件仅供宿主使用，不能成为伪造的供应商用户消息。
        let mut events = Vec::new();
        self.stage_event(
            &mut allocator,
            &mut events,
            Some(turn_id),
            EventVisibility::HostOnly,
            EventBody::ContextMaintenance(record.clone()),
            now_ms,
        )?;
        self.id_allocator = allocator;
        self.events.extend(events);
        self.metadata.context_maintenance = Some(record.clone());
        self.metadata.open_turn_start_seq = Some(started_seq);
        self.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::ContextMaintenance,
            start_seq: started_seq,
            closed: false,
        });
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[started_seq],
        );
        Ok(IdempotentMutation::Committed(record))
    }
}
