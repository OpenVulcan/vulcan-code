use super::*;

impl SessionDocument {
    /// Adds one source-owned user grant and records its durable collaboration event.
    /// 增加一条来源拥有的用户授权，并记录持久协作事件。
    ///
    /// # Parameters / 参数
    ///
    /// `input` contains the host-admitted source, target, authority, and action set.
    /// `input` 包含宿主准入的来源、目标、权威主体和动作集合。
    ///
    /// # Returns / 返回值
    ///
    /// Returns the committed grant or the exact retry result.
    /// 返回已提交授权，或完全相同重试的幂等结果。
    pub fn add_collaboration_grant(
        &mut self,
        input: AddCollaborationGrantInput,
    ) -> Result<IdempotentMutation<CollaborationGrantReceipt>, DomainError> {
        if input.source_session_id.trim().is_empty()
            || input.source_session_id.trim() != input.source_session_id
            || input.source_session_id.len() > COLLABORATION_ID_MAX_BYTES
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.grant.source_session_id",
            });
        }
        if input.target_session_id.trim().is_empty()
            || input.target_session_id.trim() != input.target_session_id
            || input.target_session_id.len() > COLLABORATION_ID_MAX_BYTES
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.grant.target_session_id",
            });
        }
        if input.source_session_id == input.target_session_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.grant.distinct_sessions",
            });
        }
        if input.source_session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationUnauthorized {
                action: "grant_add",
                identity: input.source_session_id.clone(),
            });
        }
        if input.actions.is_empty() {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.grant.actions",
            });
        }
        validate_timestamp("collaboration.grant.created_at_ms", input.created_at_ms)?;

        let semantic = CollaborationOperationSemantic::AddGrant {
            grant_id: input.grant_id.clone(),
            authority: input.authority,
            source_session_id: input.source_session_id.clone(),
            target_session_id: input.target_session_id.clone(),
            actions: input.actions.clone(),
        };
        if let Some(existing) = self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Grant { grant_id } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let grant = self
                .collaboration_state()
                .grants
                .get(&grant_id)
                .cloned()
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "grant",
                    identity: grant_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(CollaborationGrantReceipt {
                grant,
                event_seq: existing.event_seq,
            }));
        }
        if self
            .collaboration_state()
            .grants
            .contains_key(&input.grant_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }

        // The mutation owns revision allocation so retries cannot race a pre-read revision.
        // 由变更内部分配修订号，避免预读修订号在并发重试中发生竞态。
        let created_revision = next_revision(self)?;
        let grant = CollaborationGrantRecord {
            grant_id: input.grant_id.clone(),
            authority: input.authority,
            source_session_id: input.source_session_id,
            target_session_id: input.target_session_id,
            actions: input.actions,
            created_revision,
            created_at_ms: input.created_at_ms,
            revoked_at_ms: None,
            revoked_revision: None,
        };
        let mut state = self.collaboration_state().clone();
        state
            .grants
            .insert(grant.grant_id.clone(), grant.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Grant {
                    grant_id: grant.grant_id.clone(),
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::GrantAdded,
            CollaborationAfterState::Grant {
                grant: grant.clone(),
            },
            input.created_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(CollaborationGrantReceipt {
            grant,
            event_seq,
        }))
    }

    /// Revokes one source-owned grant while retaining its audit record.
    /// 撤销一条来源拥有的授权，同时保留其审计记录。
    ///
    /// # Parameters / 参数
    ///
    /// `input` contains the grant identity, idempotency identity, and revocation timestamp.
    /// `input` 包含授权身份、幂等身份和撤销时间戳。
    ///
    /// # Returns / 返回值
    ///
    /// Returns the revoked grant or the exact retry result.
    /// 返回已撤销授权，或完全相同重试的幂等结果。
    pub fn remove_collaboration_grant(
        &mut self,
        input: RemoveCollaborationGrantInput,
    ) -> Result<IdempotentMutation<CollaborationGrantReceipt>, DomainError> {
        validate_timestamp("collaboration.grant.revoked_at_ms", input.revoked_at_ms)?;
        let semantic = CollaborationOperationSemantic::RemoveGrant {
            grant_id: input.grant_id.clone(),
        };
        if let Some(existing) = self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Grant { grant_id } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let grant = self
                .collaboration_state()
                .grants
                .get(&grant_id)
                .cloned()
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "grant",
                    identity: grant_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(CollaborationGrantReceipt {
                grant,
                event_seq: existing.event_seq,
            }));
        }

        let current = self
            .collaboration_state()
            .grants
            .get(&input.grant_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "grant",
                identity: input.grant_id.as_str().to_string(),
            })?;
        if current.source_session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationUnauthorized {
                action: "grant_revoke",
                identity: current.source_session_id.clone(),
            });
        }
        if current.revoked_at_ms.is_some() {
            // A second revoke is a no-op when an earlier revoke operation is present.
            // 已有撤销操作时，重复撤销是无操作幂等结果。
            if let Some(existing) = self
                .collaboration_state()
                .operations
                .values()
                .find(|operation| operation.semantic == semantic)
                .cloned()
            {
                return Ok(IdempotentMutation::Idempotent(CollaborationGrantReceipt {
                    grant: current,
                    event_seq: existing.event_seq,
                }));
            }
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.grant.revocation_operation",
            });
        }

        // The mutation owns the revocation revision for the same atomicity reason as creation.
        // 撤销修订号同样由变更内部分配，以保持原子性并避免预读竞态。
        let revoked_revision = next_revision(self)?;
        let mut grant = current;
        grant.revoked_at_ms = Some(input.revoked_at_ms);
        grant.revoked_revision = Some(revoked_revision);
        let mut state = self.collaboration_state().clone();
        state
            .grants
            .insert(grant.grant_id.clone(), grant.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Grant {
                    grant_id: grant.grant_id.clone(),
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::GrantRemoved,
            CollaborationAfterState::Grant {
                grant: grant.clone(),
            },
            input.revoked_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(CollaborationGrantReceipt {
            grant,
            event_seq,
        }))
    }
}
