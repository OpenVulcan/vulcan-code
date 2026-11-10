//! Manual summaries inside a retained business turn never close or resume that business.
//! 保留业务回合内的手动摘要绝不关闭或恢复该业务。

use super::*;
use crate::v1::context_maintenance::ManualContextCompactionBorrow;
use crate::v1::ids::ApiCallId;

impl SessionDocument {
    /// Validates the frozen source and returns an idle open-business proof, or None for closed history.
    /// 校验冻结来源，返回空闲开放业务的证明；已闭合历史返回 None。
    /// The caller must hold physical dispatch exclusion throughout the manual attempt.
    /// 调用者必须在整个手动尝试期间持有物理分发互斥。
    pub fn borrow_manual_context_compaction(
        &self,
        source: &ManualContextMaintenanceSource,
    ) -> Result<Option<ManualContextCompactionBorrow>, DomainError> {
        self.validate_manual_context_maintenance_source(source)?;
        self.validate_manual_context_maintenance_availability()?;
        let Some(turn) = self.active_turn().filter(|turn| !turn.closed) else {
            return Ok(None);
        };
        if matches!(
            turn.origin,
            TurnOrigin::ContextMaintenance | TurnOrigin::QuestionExecution
        ) || !self
            .completion_obligations(turn.turn_id)
            .open_child_task_ids
            .is_empty()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.competing_owner",
            });
        }
        Ok(Some(ManualContextCompactionBorrow {
            turn_id: turn.turn_id,
            start_seq: turn.start_seq,
            segment_generation: self.segment_generation(),
            source_revision: source.metadata.document_revision,
            after_seq: self.last_seq()?,
            source_compaction_id: self.runtime_lock.current_compaction_id,
            run_control: self.collaboration_run_control(),
        }))
    }

    /// Commits the exact borrowed summary after checking ownership in the same atomic mutation.
    /// 在同一原子修改中检查归属后提交精确借用摘要。
    pub fn record_borrowed_manual_compaction(
        &mut self,
        owner: ManualContextCompactionBorrow,
        input: CompactionCommitInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CompactionId>, DomainError> {
        self.validate_manual_compaction_borrow(owner)?;
        self.validate_manual_compaction_borrow_api(owner, input.api_call_id)?;
        if self.active_api().is_some()
            || (self.runtime_lock.current_compaction_id != owner.source_compaction_id
                && !self.compactions.iter().any(|node| {
                    Some(node.compaction_id) == self.runtime_lock.current_compaction_id
                        && node.api_call_id == input.api_call_id
                }))
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.borrowed_commit_owner",
            });
        }
        self.record_compaction(input, now_ms)
    }

    /// Confirms this borrow committed its own summary without creating a business terminal.
    /// 确认本次借用已提交自己的摘要，不创建业务终态。
    pub fn complete_borrowed_manual_compaction(
        &self,
        owner: ManualContextCompactionBorrow,
    ) -> Result<(), DomainError> {
        self.validate_manual_compaction_borrow(owner)?;
        let node = self
            .compactions
            .iter()
            .find(|node| {
                Some(node.compaction_id) == self.runtime_lock.current_compaction_id
                    && Some(node.compaction_id) != owner.source_compaction_id
                    && node.occurred_at_seq > owner.after_seq
            })
            .ok_or(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.borrowed_compaction_missing",
            })?;
        self.validate_manual_compaction_borrow_api(owner, node.api_call_id)?;
        if self.active_api().is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.borrowed_api_active",
            });
        }
        Ok(())
    }

    /// Fails only a sink-owned physical summary API; prior business policy and turn remain unchanged.
    /// 仅失败接收器拥有的物理摘要 API；原业务策略和回合保持不变。
    pub fn fail_manual_compaction_api(
        &mut self,
        api_call_id: ApiCallId,
        detail: &str,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let Some(api) = self.active_api() else {
            return Ok(false);
        };
        if api.api_call_id != api_call_id || api.purpose != ApiPurposeKind::Compaction {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.failure_api_owner",
            });
        }
        self.apply_atomic_transition(|document| {
            document.fail_active_api_for_execution(
                "context_compaction_failed",
                Some(detail),
                now_ms,
            )?;
            Ok(true)
        })
    }

    /// Records a manual failure after its physical API settled, without closing the borrowed business.
    /// 物理 API 结算后记录手动失败，不关闭被借用业务。
    pub fn fail_borrowed_manual_compaction(
        &mut self,
        owner: ManualContextCompactionBorrow,
        detail: &str,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        self.validate_manual_compaction_borrow(owner)?;
        if self.active_api().is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.borrowed_api_active",
            });
        }
        if self.events().iter().any(|event| {
            event.seq > owner.after_seq
                && event.turn_id == Some(owner.turn_id)
                && matches!(&event.event, EventBody::DisplayNote(note)
                if note.reason_code == "context_compaction_failed")
        }) {
            return Ok(false);
        }
        self.append_display_note(
            Some(owner.turn_id),
            crate::v1::event::DisplayNoticeKind::SystemError,
            crate::v1::event::NoticeSeverity::Error,
            "context_compaction_failed",
            Some(detail.to_owned()),
            None,
            None,
            None,
            now_ms,
        )?;
        Ok(true)
    }

    pub(in crate::v1::document) fn validate_manual_compaction_borrow(
        &self,
        owner: ManualContextCompactionBorrow,
    ) -> Result<(), DomainError> {
        if self.segment_generation() != owner.segment_generation
            || self.collaboration_run_control() != owner.run_control
            || self.metadata.archived_at_ms.is_some()
            || !self.active_turn().is_some_and(|turn| !turn.closed
                && turn.turn_id == owner.turn_id && turn.start_seq == owner.start_seq
                && !matches!(turn.origin, TurnOrigin::ContextMaintenance | TurnOrigin::QuestionExecution))
            || self.events().iter().any(|event| event.seq > owner.after_seq
                && matches!(&event.event, EventBody::ApiStarted(api)
                    if api.purpose != ApiPurposeKind::Compaction || event.turn_id != Some(owner.turn_id)))
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.borrowed_owner_changed",
            });
        }
        Ok(())
    }

    fn validate_manual_compaction_borrow_api(
        &self,
        owner: ManualContextCompactionBorrow,
        api_call_id: ApiCallId,
    ) -> Result<(), DomainError> {
        let latest = self.events().iter().rev().find_map(|event| {
            if event.seq <= owner.after_seq {
                return None;
            }
            match &event.event {
                EventBody::ApiStarted(api) => Some((event.turn_id, api)),
                _ => None,
            }
        });
        if !latest.is_some_and(|(turn, api)| {
            turn == Some(owner.turn_id)
                && api.purpose == ApiPurposeKind::Compaction
                && api.api_call_id == api_call_id
        }) {
            return Err(DomainError::CollaborationStateViolation {
                field: "manual_maintenance.borrowed_api_owner",
            });
        }
        Ok(())
    }
}
