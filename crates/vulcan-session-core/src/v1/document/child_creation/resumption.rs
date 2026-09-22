//! Failure-atomic successor publication from an exact retained child history.
//! 基于精确保留子历史的失败原子后继发布。

use super::*;

impl ChildCreationIntent {
    /// Validates stored publication authority without inventing missing predecessor history.
    /// 校验持久发布权威，不虚构缺失的前序历史。
    /// Returns an explicit error for invalid settings, timestamps or unsupported revision coordinates.
    /// 设置、时间戳或修订坐标非法时返回明确错误。
    pub(super) fn validate_creation_source(&self) -> Result<(), DomainError> {
        validate_timestamp("child_creation.created_at_ms", self.created_at_ms)?;
        validate_exact_external_identity("child_creation.project_tag", &self.project_tag)?;
        self.profile.validate_owner(
            self.profile.task_id.as_str(),
            self.profile.segment_generation,
        )?;
        match self.origin {
            ChildCreationOrigin::Initial => {
                self.rebuild_initial_document()?;
            }
            ChildCreationOrigin::Resumption {
                base_document_revision,
            } => {
                if base_document_revision == 0 || base_document_revision == u64::MAX {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "child_creation.resumption_revision",
                    });
                }
            }
        }
        Ok(())
    }
}

impl SessionDocument {
    /// Replaces an existing child root and physical request with an exact successor atomically.
    /// 原子地用精确后继替换已有子根输入及物理请求。
    /// `submission` supplies the execution action and prompt; `now_ms` timestamps the single publication.
    /// `submission` 提供执行动作及提示；`now_ms` 为单次发布的时间戳。
    /// Returns the original turn on exact retry; any error leaves the preceding execution unchanged.
    /// 精确重试返回原回合；任何错误均保持前序执行不变。
    pub fn submit_resumed_child_root_input(
        &mut self,
        submission: ClientInputSubmission,
        now_ms: i64,
    ) -> Result<IdempotentMutation<TurnId>, DomainError> {
        let operation_id = CollaborationOperationId::try_new(format!(
            "root-input-running:{}:{}",
            self.metadata.session_id,
            submission.client_action_id.as_str()
        ))?;
        self.apply_atomic_transition(move |document| {
            // Replay uses domain payload validation and preserves the current control state.
            // 重放使用领域载荷验证，并保留当前控制状态。
            if document.events().iter().any(|event| {
                matches!(&event.event, EventBody::UserInput(input)
                    if input.client_action_id == submission.client_action_id)
            }) {
                return match document.submit_root_input(submission, now_ms)? {
                    IdempotentMutation::Idempotent(turn) => {
                        Ok(IdempotentMutation::Idempotent(turn))
                    }
                    IdempotentMutation::Committed(_) => Err(DomainError::ImmutableFieldMutation {
                        entity: "embedded_task_resume",
                        field: "existing_root_action",
                    }),
                };
            }
            // Timing and old execution ownership are read inside this same atomic draft.
            // 在同一原子草稿内读取计时及旧执行归属。
            if let Some(turn) = document.active_turn().filter(|turn| !turn.closed) {
                let started_at_ms = document
                    .events()
                    .iter()
                    .find(|event| event.seq == turn.start_seq)
                    .ok_or(DomainError::ReferenceViolation {
                        kind: "turn_start_event",
                        value: turn.start_seq.value(),
                    })?
                    .created_at_ms;
                if document.active_api().is_some() {
                    document.interrupt_active_api_for_process(now_ms)?;
                }
                document.close_turn(
                    TurnOutcome::Stopped,
                    now_ms.saturating_sub(started_at_ms).max(0),
                    now_ms,
                )?;
            }
            let mutation = document.submit_root_input(submission, now_ms)?;
            document.set_collaboration_run_control(SetRunControlInput {
                operation_id,
                state: CollaborationRunControl::Running,
                changed_at_ms: now_ms,
            })?;
            Ok(mutation)
        })
    }

    /// Prepares a successor using the real existing allocator while leaving this child unchanged.
    /// 使用真实已有分配器准备后继，同时保持本子文档不变。
    /// `preparation` is the frozen runtime configuration; `now_ms` is the original admission time.
    /// `preparation` 为冻结运行时配置；`now_ms` 为原准入时间。
    /// Returns the unpublished successor and parent-owned intent for this exact source revision.
    /// 返回未发布后继及此精确来源修订的父侧意图。
    pub fn prepare_child_resumption(
        &self,
        preparation: ChildExecutionPreparation,
        now_ms: i64,
    ) -> Result<(Self, ChildCreationIntent), DomainError> {
        let previous = self.metadata.child_execution_profile.as_ref().ok_or(
            DomainError::CollaborationStateViolation {
                field: "child_creation.resumption_profile",
            },
        )?;
        if self.metadata.archived_at_ms.is_some()
            || preparation.task_id.to_string() != self.metadata.session_id
            || previous.root_session_id != preparation.root_session_id.to_string()
            || previous.parent_session_id != preparation.parent_session_id.to_string()
            || previous.business_execution_id == preparation.execution_id
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_creation.resumption_owner",
            });
        }
        // The cloned production allocator supplies new coordinates without writing the original history.
        // 克隆的生产分配器提供新坐标，不写入原历史。
        let mut successor = self.clone();
        let profile = successor.apply_atomic_transition(|document| {
            let turn = document
                .submit_resumed_child_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new(&preparation.execution_id)?,
                        preparation.invocation_config.prompt.clone(),
                        vec![],
                    ),
                    now_ms,
                )?
                .into_value();
            let profile = preparation.bind_child_turn(document.segment_generation(), turn)?;
            document.capture_child_execution_profile(profile.clone(), now_ms)?;
            Ok(profile)
        })?;
        let intent = ChildCreationIntent {
            origin: ChildCreationOrigin::Resumption {
                base_document_revision: self.memory_revision(),
            },
            profile,
            project_tag: self.metadata.project_tag.clone(),
            created_at_ms: now_ms,
        };
        intent.validate_creation_source()?;
        Ok((successor, intent))
    }

    /// Applies the original successor source to its exact predecessor or confirms an exact published retry.
    /// 将原后继来源应用到其精确前序，或确认已发布的精确重试。
    /// The host must validate parent reservation and lifecycle gates before supplying `intent`.
    /// 宿主提供 `intent` 前必须校验父预留及生命周期锁。
    /// Returns whether one atomic revision was produced; conflicts preserve all existing history.
    /// 返回是否产生一个原子修订；冲突保留全部已有历史。
    pub fn publish_child_resumption(
        &mut self,
        intent: &ChildCreationIntent,
    ) -> Result<bool, DomainError> {
        intent.validate_creation_source()?;
        let ChildCreationOrigin::Resumption {
            base_document_revision,
        } = intent.origin
        else {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_creation.resumption_origin",
            });
        };
        intent
            .profile
            .validate_owner(&self.metadata.session_id, self.segment_generation())?;
        if self.metadata.project_tag != intent.project_tag || self.metadata.archived_at_ms.is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_creation.resumption_scope",
            });
        }
        if self.metadata.child_execution_profile.as_ref() == Some(&intent.profile) {
            if self.memory_revision() <= base_document_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_creation.resumption_publication_revision",
                });
            }
            return Ok(false);
        }
        if self.memory_revision() != base_document_revision {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_creation.resumption_source_changed",
            });
        }
        self.apply_atomic_transition(|document| {
            document.submit_resumed_child_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new(&intent.profile.business_execution_id)?,
                    intent.profile.invocation_config.prompt.clone(),
                    vec![],
                ),
                intent.created_at_ms,
            )?;
            document
                .capture_child_execution_profile(intent.profile.clone(), intent.created_at_ms)?;
            Ok(true)
        })
    }
}
