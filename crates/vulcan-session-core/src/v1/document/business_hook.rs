//! Canonical capture and replay checks for main-session business Hook admission.
//! 主会话业务 Hook 准入的规范捕获与重放检查。

use super::*;
use crate::v1::business_hook::{BusinessHookBinding, BusinessHookMetadata};

#[cfg(test)]
mod tests;

impl SessionDocument {
    /// Proves first main-business preparation from the complete durable owner frontier.
    /// 从完整可靠所有者前沿证明首次主业务准备。
    /// Callers must additionally validate their exact source event; returns no execution grant by itself.
    /// 调用方仍须验证精确来源事件；本凭据单独不授予执行许可。
    pub fn main_business_initial_preparation(
        &self,
    ) -> Result<crate::v1::business_hook::InitialBusinessPreparation, DomainError> {
        if self.memory_revision() != self.durable_revision() {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision(),
                durable_revision: self.durable_revision(),
            });
        }
        self.validate_business_hook_binding()?;
        // Physical, auxiliary and explicit lifecycle owners exclude new business preparation.
        // 物理、辅助及明确生命周期所有者排除新业务准备。
        let turn = self.require_open_turn()?;
        if turn.origin == TurnOrigin::QuestionExecution
            || self.metadata.archived_at_ms.is_some()
            || self.metadata.child_execution_profile.is_some()
            || self.active_api().is_some()
            || self.collaboration_run_control()
                != crate::v1::collaboration::CollaborationRunControl::Running
            || self
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| {
                    execution.state
                        == crate::v1::collaboration::CollaborationQuestionExecutionState::Running
                })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "initial_business.owner",
            });
        }
        // Prior business effects require original identity recovery, never a new Hook seed.
        // 既有业务效果要求恢复原身份，绝不能创建新 Hook 种子。
        if self.metadata.business_hook.binding.as_ref().is_some_and(|binding| binding.turn_id == turn.turn_id)
            || self.business_input_dispatch_started(turn.turn_id, turn.start_seq)
            || self.collaboration_state().child_tasks.values().any(|task| task.parent_turn_id == turn.turn_id)
            || self.events().iter().any(|event| event.turn_id == Some(turn.turn_id)
                && matches!(&event.event, EventBody::ToolCall(tool) if matches!(tool.status,
                    ToolCallStatus::Prepared | ToolCallStatus::Running | ToolCallStatus::WaitingUser | ToolCallStatus::Interrupted)))
        {
            return Err(DomainError::CollaborationStateViolation { field: "initial_business.prior_execution" });
        }
        Ok(crate::v1::business_hook::InitialBusinessPreparation {
            session_id: self.metadata.session_id.clone(),
            segment_generation: self.segment_generation(),
            revision: self.durable_revision(),
            turn_id: turn.turn_id,
            start_seq: turn.start_seq,
            origin: turn.origin,
        })
    }

    /// Reports whether a business API started after `input_seq` in exactly `turn_id`.
    /// 返回精确 `turn_id` 内是否在 `input_seq` 之后启动过业务 API。
    /// Requires the caller's complete relevant history; this does not grant admission or prove completion.
    /// 要求调用方提供完整相关历史；此查询不授予准入，也不证明完成。
    pub fn business_input_dispatch_started(&self, turn_id: TurnId, input_seq: Seq) -> bool {
        // Auxiliary work must never acknowledge receipt of unfinished business input.
        // 辅助工作绝不能充当尚未处理业务输入的接收凭据。
        self.events().iter().any(|event| {
            event.turn_id == Some(turn_id)
                && event.seq > input_seq
                && matches!(&event.event, EventBody::ApiStarted(started)
                    if !matches!(started.purpose,
                        ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
        })
    }

    /// Returns original Hook state for an independently authorized business continuation.
    /// 为经过独立授权的业务续跑返回原 Hook 状态。
    /// Requires full durable history; stopped, auxiliary, archived and uncertain tool owners fail explicitly.
    /// 要求完整可靠历史；停止、辅助、归档及工具结果不明的所有者明确失败。
    pub fn main_business_recovery_state(&self) -> Result<BusinessHookMetadata, DomainError> {
        // Preserve the original journal after the common durable-owner checks.
        // 通过通用可靠归属检查后保留原日志。
        let state = self.business_hook_recovery_state()?;
        // Inspect tools only in the exact business turn proven by that journal.
        // 只检查已由该日志证明的精确业务回合内的工具。
        let turn = self.require_open_turn()?;
        if self.metadata.archived_at_ms.is_some() || self.metadata.child_execution_profile.is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "main_business_recovery.owner",
            });
        }
        // Replaying an interrupted tool can repeat external effects; Hook recovery is not execution consent.
        // 重放中断工具可能重复外部效果；Hook 恢复不能充当工具执行许可。
        if self.events().iter().any(|event| {
            event.turn_id == Some(turn.turn_id)
                && matches!(&event.event, EventBody::ToolCall(tool) if matches!(tool.status,
                    ToolCallStatus::Prepared | ToolCallStatus::Running | ToolCallStatus::WaitingUser
                        | ToolCallStatus::Interrupted))
        }) {
            return Err(DomainError::CollaborationStateViolation {
                field: "main_business_recovery.unconfirmed_tool_result",
            });
        }
        Ok(state)
    }

    /// Captures immutable `binding` against its original open turn without appending model-visible input.
    /// 对照原开放回合捕获不可变 `binding`，不追加模型可见输入。
    /// `now_ms` timestamps publication; exact retries return false and conflicts leave the document unchanged.
    /// `now_ms` 标记发布时间；精确重试返回 false，冲突保持文档不变。
    pub fn capture_business_hook_binding(
        &mut self,
        binding: BusinessHookBinding,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("business_hook.updated_at_ms", now_ms)?;
        // Validate the complete candidate before changing metadata or revision counters.
        // 修改元数据或修订计数之前校验完整候选。
        let candidate = BusinessHookMetadata {
            version: 1,
            binding: Some(binding.clone()),
            observed_inputs: self
                .metadata
                .business_hook
                .observed_inputs
                .iter()
                .filter(|entry| entry.turn_id == binding.turn_id)
                .cloned()
                .collect(),
        };
        candidate.validate(self.segment_generation())?;
        if binding.session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.session",
            });
        }
        if let Some(existing) = &self.metadata.business_hook.binding {
            if existing == &binding {
                return Ok(false);
            }
            if existing.turn_id == binding.turn_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "business_hook.binding_conflict",
                });
            }
        }
        // Only an admitted business boundary can publish new ownership, never an auxiliary request.
        // 只有已准入业务边界能够发布新归属，辅助请求不能发布。
        let turn = self.require_open_turn()?;
        if turn.turn_id != binding.turn_id
            || turn.start_seq != binding.start_seq
            || binding.observes_user != (turn.origin != TurnOrigin::AgentTask)
            || turn.origin == TurnOrigin::QuestionExecution
            || self.active_api().is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.business_turn",
            });
        }
        self.ensure_transition_capacity(0, 0)?;
        self.metadata.business_hook = candidate;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Validates persisted binding coordinates without requiring its business turn to remain open.
    /// 校验持久绑定坐标，不要求其业务回合仍开放。
    /// Full and tail loaders share the allocator bounds; recovery additionally proves the original start event.
    /// 完整与尾段加载器共用分配器边界；恢复还需证明原起始事件。
    pub(super) fn validate_business_hook_binding(&self) -> Result<(), DomainError> {
        self.metadata
            .business_hook
            .validate(self.segment_generation())?;
        if let Some(binding) = &self.metadata.business_hook.binding {
            // Restored ownership cannot promote an agent task into a user-memory lifecycle.
            // 恢复归属不能把代理任务提升为用户记忆生命周期。
            if self.active_turn().is_some_and(|turn| {
                turn.turn_id == binding.turn_id
                    && binding.observes_user != (turn.origin != TurnOrigin::AgentTask)
            }) {
                return Err(DomainError::CollaborationStateViolation {
                    field: "business_hook.observation_origin",
                });
            }
            if binding.session_id != self.metadata.session_id
                || binding.turn_id.value() >= self.id_frontier().next_local_id
                || binding.start_seq.value() >= self.id_frontier().next_seq
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "business_hook.frontier",
                });
            }
            if let Some(event) = self
                .events()
                .iter()
                .find(|event| event.seq == binding.start_seq)
                && event.turn_id != Some(binding.turn_id)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "business_hook.start_owner",
                });
            }
        }
        if self
            .metadata
            .business_hook
            .observed_inputs
            .iter()
            .any(|entry| {
                entry.input_id.value() >= self.id_frontier().next_local_id
                    || entry.turn_id.value() >= self.id_frontier().next_local_id
                    || entry.observed_revision > self.memory_revision()
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.timeline_frontier",
            });
        }
        Ok(())
    }

    /// Selects the original published Hook for an open business continuation without an active API.
    /// 为没有活动 API 的开放业务续跑选择原已发布 Hook。
    /// Returns immutable admission facts; source history and run control are rechecked before reconstruction.
    /// 返回不可变准入事实；重建前重新校验来源历史及运行控制。
    pub fn business_hook_recovery_binding(&self) -> Result<&BusinessHookBinding, DomainError> {
        use crate::v1::collaboration::{
            CollaborationExecutionPurpose, CollaborationQuestionExecutionState,
        };

        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        self.validate_business_hook_binding()?;
        // A binding cannot resume stopped business or substitute for an in-flight physical request.
        // 绑定不能恢复已停止业务，也不能替代正在执行的物理请求。
        let turn = self.require_open_turn()?;
        let binding = self.metadata.business_hook.binding.as_ref().ok_or(
            DomainError::CollaborationStateViolation {
                field: "business_hook.missing",
            },
        )?;
        if turn.turn_id != binding.turn_id
            || turn.start_seq != binding.start_seq
            || binding.observes_user != (turn.origin != TurnOrigin::AgentTask)
            || turn.origin == TurnOrigin::QuestionExecution
            || self.active_api().is_some()
            || self.collaboration_run_control()
                != crate::v1::collaboration::CollaborationRunControl::Running
            || self
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| {
                    // In-turn attempts live until this business turn closes, including after a reply.
                    // 回合内答疑尝试直到本业务回合闭合才结束，即使已经回复也一样。
                    // Only this exact non-standalone owner may share the original business Hook.
                    // 只有精确属于本回合的非独立所有者可以共享原业务 Hook。
                    execution.state == CollaborationQuestionExecutionState::Running
                        && (execution.purpose != CollaborationExecutionPurpose::InTurnQuestion
                            || execution.parent_turn_id != Some(turn.turn_id)
                            || execution.execution_turn_id.is_some())
                })
            || !self.events().iter().any(|event| {
                event.seq == binding.start_seq && event.turn_id == Some(binding.turn_id)
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.recovery_owner",
            });
        }
        Ok(binding)
    }

    /// Captures the validated original binding and ordered input observations from one durable revision.
    /// 从同一可靠修订捕获已验证原绑定和有序输入观察。
    /// Returns a detached recovery value; dispatch authority remains with the caller's lifecycle reservation.
    /// 返回分离恢复值；分发权威仍由调用方生命周期预留持有。
    pub fn business_hook_recovery_state(&self) -> Result<BusinessHookMetadata, DomainError> {
        // Reuse all original-turn and physical-execution checks before exposing recovery facts.
        // 暴露恢复事实之前复用全部原回合和物理执行检查。
        let binding = self.business_hook_recovery_binding()?;
        if self
            .metadata
            .business_hook
            .observed_inputs
            .iter()
            .any(|entry| entry.turn_id != binding.turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "business_hook.timeline_owner",
            });
        }
        Ok(self.metadata.business_hook.clone())
    }
}
