//! Canonical capture and validation of the latest child business execution profile.
//! 最新子业务执行配置的规范捕获与验证。

use super::*;
use crate::v1::child_execution::ChildExecutionProfile;

mod maintenance;
pub use maintenance::ChildContextMaintenanceSource;

#[cfg(test)]
mod tests;

impl SessionDocument {
    /// Selects the captured profile for an exact durable business execution at an idle API boundary.
    /// 在空闲 API 边界选择精确可靠业务执行的已捕获配置。
    /// `execution_id` comes from the parent's reservation; returns the source-owned profile only.
    /// `execution_id` 来自父侧预留；仅返回来源自有配置。
    /// The host must separately retain parent ownership and revalidate before actual dispatch.
    /// 宿主必须另行保持父侧归属，并在实际分发前重新校验。
    pub fn child_business_recovery_profile(
        &self,
        execution_id: &crate::v1::collaboration::CollaborationExecutionId,
    ) -> Result<&ChildExecutionProfile, DomainError> {
        use crate::v1::collaboration::{
            CollaborationQuestionExecutionState, CollaborationRunControl,
        };
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        // A stopped source or an auxiliary owner cannot be silently converted into business work.
        // 已停止来源或辅助所有者不能被静默转换为业务工作。
        if self.collaboration_run_control() != CollaborationRunControl::Running
            || self.active_api().is_some()
            || self
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| execution.state == CollaborationQuestionExecutionState::Running)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_business_recovery.execution_busy_or_stopped",
            });
        }
        // Only the original open business turn can supply an executable captured profile.
        // 只有原开放业务回合能够提供可执行捕获配置。
        let turn = self.require_open_turn()?;
        let profile = self.metadata.child_execution_profile.as_ref().ok_or(
            DomainError::CollaborationStateViolation {
                field: "child_business_recovery.profile_missing",
            },
        )?;
        profile.validate_owner(&self.metadata.session_id, self.segment_generation())?;
        if turn.origin != TurnOrigin::UserRoot
            || turn.turn_id != profile.business_turn_id
            || execution_id.as_str() != profile.business_execution_id
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_business_recovery.original_execution",
            });
        }
        // Full durable history proves the original root; a tail-only hint is not dispatch authority.
        // 完整可靠历史证明原根输入；仅尾段线索不构成分发权威。
        let roots = self
            .events()
            .iter()
            .filter(|event| {
                matches!(&event.event, EventBody::UserInput(input)
                if input.input_source == crate::v1::event::UserInputSource::Root
                    && input.client_action_id.as_str() == execution_id.as_str())
            })
            .collect::<Vec<_>>();
        if roots.len() != 1
            || roots[0].seq != turn.start_seq
            || roots[0].turn_id != Some(turn.turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_business_recovery.original_root",
            });
        }
        // Interrupted host work may already have external effects; configuration recovery cannot authorize replay.
        // 中断宿主工作可能已产生外部效果；配置恢复不能授权重放。
        if self.events().iter().any(|event| {
            event.turn_id == Some(turn.turn_id)
                && matches!(&event.event, EventBody::ToolCall(tool) if matches!(tool.status,
                    ToolCallStatus::Prepared | ToolCallStatus::Running | ToolCallStatus::WaitingUser
                        | ToolCallStatus::Interrupted))
        }) {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_business_recovery.unconfirmed_tool_result",
            });
        }
        Ok(profile)
    }

    /// Captures `profile` at a closed API boundary of its original open business turn.
    /// 在原开放业务回合的闭合 API 边界捕获 `profile`。
    /// `now_ms` timestamps a new revision; exact retries return false without changing history.
    /// `now_ms` 标记新修订时间；精确重试返回 false 且不改变历史。
    /// Only the trusted child preparation path may supply this configuration.
    /// 只有可信子准备路径能够提供此配置。
    pub fn capture_child_execution_profile(
        &mut self,
        profile: ChildExecutionProfile,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        profile.validate_owner(&self.metadata.session_id, self.segment_generation())?;
        validate_timestamp("child_execution_profile.updated_at_ms", now_ms)?;
        let turn = self.require_open_turn()?;
        if turn.turn_id != profile.business_turn_id || turn.origin == TurnOrigin::QuestionExecution
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.business_turn",
            });
        }
        // A previous capture remains authoritative after its root input has been sealed.
        // 根输入封存后，先前捕获的执行绑定仍是权威依据。
        let captured_execution =
            self.metadata
                .child_execution_profile
                .as_ref()
                .is_some_and(|captured| {
                    captured.business_turn_id == profile.business_turn_id
                        && captured.business_execution_id == profile.business_execution_id
                        && captured.segment_generation == profile.segment_generation
                });
        // First capture must prove the execution ID against the actual child root input.
        // 首次捕获必须通过真实子根输入证明执行 ID。
        let root_execution = self.events().iter().any(|event| {
            event.seq == turn.start_seq
                && matches!(&event.event, EventBody::UserInput(input)
                    if input.client_action_id.as_str() == profile.business_execution_id)
        });
        if !captured_execution && !root_execution {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.business_execution",
            });
        }
        if self.active_api().is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.active_api",
            });
        }
        if self.metadata.child_execution_profile.as_ref() == Some(&profile) {
            return Ok(false);
        }
        // Re-preparation can refresh host registrations, but cannot reinterpret the original invocation.
        // 重新准备可以刷新宿主注册，但不能重新解释原调用。
        if captured_execution
            && self
                .metadata
                .child_execution_profile
                .as_ref()
                .is_some_and(|captured| {
                    captured.invocation_config != profile.invocation_config
                        || captured.resolved_config != profile.resolved_config
                })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.immutable_task_config",
            });
        }
        // Request preparation may refresh executable fields, but cannot rename or reparent the same original task.
        // 请求准备可以刷新可执行字段，但不能重命名或重新归属同一个原任务。
        if captured_execution
            && self
                .metadata
                .child_execution_profile
                .as_ref()
                .is_some_and(|captured| {
                    captured.runtime_identity != profile.runtime_identity
                        || captured.root_session_id != profile.root_session_id
                        || captured.parent_session_id != profile.parent_session_id
                })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.immutable_runtime_identity",
            });
        }
        self.ensure_transition_capacity(0, 0)?;
        self.metadata.child_execution_profile = Some(profile);
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Checks persisted profile identity without requiring its business task to still be running.
    /// 检查持久配置身份，不要求其业务任务仍运行。
    /// Returns an error for unsupported versions or a foreign/future business owner.
    /// 不支持的版本、外来业务所有者或未来业务身份返回错误。
    pub(super) fn validate_child_execution_profile(&self) -> Result<(), DomainError> {
        if let Some(profile) = &self.metadata.child_execution_profile {
            profile.validate_owner(&self.metadata.session_id, self.segment_generation())?;
            if profile.business_turn_id.value() >= self.id_frontier().next_local_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_execution_profile.business_frontier",
                });
            }
        }
        Ok(())
    }
}
