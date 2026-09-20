//! Durable terminal settlement intent lifecycle for one exact session.
//! 单个精确会话的可靠终态结算意图生命周期。
//!
//! A persisted intent in `metadata.input_state.terminal_settlement` lets a
//! restarted process reconstruct the pending owner and retry the same turn
//! idempotently. Failure before this intent is durable is outside this document
//! contract and must be handled by the caller's pre-intent recovery path.
//! 已持久化在 `metadata.input_state.terminal_settlement` 中的意图允许重启进程
//! 重建待处理所有者，并幂等重试同一回合。意图持久化前的失败不属于本文档契约，
//! 必须由调用方的意图前恢复路径处理。

use super::*;
use crate::v1::config::{TerminalSettlementRecord, TerminalSettlementState};

impl SessionDocument {
    /// Returns the durable terminal settlement intent, if one is still open.
    /// 返回仍然开放的可靠终态结算意图（如有）。
    pub fn terminal_settlement_record(&self) -> Option<&TerminalSettlementRecord> {
        self.metadata.input_state.terminal_settlement.as_ref()
    }

    /// Writes or re-confirms one terminal settlement intent before the dangerous persistence phase.
    /// 在危险的持久化阶段之前写入或再次确认一条终态结算意图。
    ///
    /// An exactly repeated intent stays idempotent; a different settlement for
    /// the same session is rejected so a stale owner cannot silently supersede
    /// an unfinished settlement and reopen admission.
    /// 完全相同的意图重放保持幂等；同一会话上的不同结算会被拒绝，避免过期
    /// owner 静默取代未完成结算并重新开放准入。
    pub fn begin_terminal_settlement(
        &mut self,
        record: TerminalSettlementRecord,
        now_ms: i64,
    ) -> Result<IdempotentMutation<TerminalSettlementState>, DomainError> {
        validate_timestamp("settlement_time", now_ms)?;
        if record.session_id != self.metadata.session_id {
            return Err(DomainError::TerminalSettlementSessionMismatch {
                session_id: self.metadata.session_id.clone(),
            });
        }
        if let Some(existing) = self.metadata.input_state.terminal_settlement.as_ref() {
            if existing.settlement_id == record.settlement_id {
                return Ok(IdempotentMutation::Idempotent(existing.state));
            }
            return Err(DomainError::TerminalSettlementConflict {
                settlement_id: existing.settlement_id.clone(),
            });
        }
        self.metadata.input_state.terminal_settlement = Some(record.clone());
        self.touch_metadata_only_revision(now_ms);
        Ok(IdempotentMutation::Committed(record.state))
    }

    /// Marks one exact durable settlement as requiring a controlled retry.
    /// 把一条精确可靠结算标记为需要受控重试。
    pub fn mark_terminal_settlement_repair_required(
        &mut self,
        settlement_id: &str,
        now_ms: i64,
    ) -> Result<IdempotentMutation<()>, DomainError> {
        validate_timestamp("settlement_time", now_ms)?;
        let existing = self
            .metadata
            .input_state
            .terminal_settlement
            .as_mut()
            .ok_or_else(|| DomainError::TerminalSettlementMissing {
                settlement_id: settlement_id.to_owned(),
            })?;
        if existing.settlement_id != settlement_id {
            return Err(DomainError::TerminalSettlementMissing {
                settlement_id: settlement_id.to_owned(),
            });
        }
        if existing.state == TerminalSettlementState::RepairRequired {
            return Ok(IdempotentMutation::Idempotent(()));
        }
        existing.state = TerminalSettlementState::RepairRequired;
        existing.updated_at_ms = now_ms;
        self.touch_metadata_only_revision(now_ms);
        Ok(IdempotentMutation::Committed(()))
    }

    /// Resolves one exact settlement after its durable terminal fact is confirmed.
    /// 在可靠终态事实确认后结算一条精确结算。
    pub fn resolve_terminal_settlement(
        &mut self,
        settlement_id: &str,
        now_ms: i64,
    ) -> Result<IdempotentMutation<()>, DomainError> {
        validate_timestamp("settlement_time", now_ms)?;
        let Some(existing) = self.metadata.input_state.terminal_settlement.as_ref() else {
            return Ok(IdempotentMutation::Idempotent(()));
        };
        if existing.settlement_id != settlement_id {
            return Err(DomainError::TerminalSettlementMissing {
                settlement_id: settlement_id.to_owned(),
            });
        }
        self.metadata.input_state.terminal_settlement = None;
        self.touch_metadata_only_revision(now_ms);
        Ok(IdempotentMutation::Committed(()))
    }

    /// Returns whether one exact turn has a terminal fact in the resident document.
    /// 返回常驻文档中一条精确回合是否已经拥有终态事实。
    ///
    /// The terminal statistic event is the closure proof, but this document can be ahead of disk;
    /// callers must compare memory and durable revisions before treating the fact as persisted.
    /// 终态统计事件是闭合证明，但本文档可能领先于磁盘；调用方须比较内存与可靠修订后，
    /// 才能把该事实当作已经持久化。
    pub fn turn_has_terminal_fact(&self, turn_id: TurnId) -> bool {
        self.events.iter().any(|event| {
            event.turn_id == Some(turn_id) && matches!(event.event, EventBody::TurnStats(_))
        })
    }

    /// Publishes one metadata-only revision for the settlement intent transition.
    /// 为结算意图转换发布一个仅元数据的修订。
    fn touch_metadata_only_revision(&mut self, now_ms: i64) {
        self.metadata.updated_at_ms = self.metadata.updated_at_ms.max(now_ms);
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
    }
}
