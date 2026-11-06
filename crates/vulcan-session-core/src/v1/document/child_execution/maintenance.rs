//! Exact durable business authority required before admitting child context maintenance.
//! 准入子上下文维护之前所需的精确可靠业务权威。

use super::*;
use crate::v1::collaboration::{CollaborationExecutionId, CollaborationRunControl};
use crate::v1::context_maintenance::{ContextMaintenanceExecution, ContextMaintenanceTerminal};

mod manual;

/// Immutable successful source captured from one fully published child document.
/// 从一个完整发布的子文档捕获的不可变成功来源。
/// Private fields prevent callers from manufacturing admission by copying arbitrary coordinates.
/// 私有字段防止调用方通过复制任意坐标制造准入。
#[derive(Debug, Clone, PartialEq)]
pub struct ChildContextMaintenanceSource {
    /// Original resolved child controls and immutable business ownership.
    /// 原已解析子控制与不可变业务归属。
    profile: ChildExecutionProfile,
    /// Fully validated original success proof, including the actual final assistant and terminal range.
    /// 完整校验的原成功证明，包含实际最终助手正文及终态范围。
    completed_business: SubagentReturnInput,
    /// Exact durable revision that must still own admission before a new maintenance record is written.
    /// 写入新维护记录前必须仍拥有准入的精确可靠修订。
    revision: u64,
    /// Context pointer against which the source was captured.
    /// 捕获来源时所依据的上下文指针。
    compaction_id: Option<CompactionId>,
}

impl ChildContextMaintenanceSource {
    /// Returns the original child's captured controls without granting a new business execution.
    /// 返回原子代理捕获控制，不授予新的业务执行。
    pub fn profile(&self) -> &ChildExecutionProfile {
        &self.profile
    }

    /// Returns the original fully validated success proof; maintenance cannot replace this result.
    /// 返回原完整校验的成功证明；维护不能替换此结果。
    pub fn completed_business(&self) -> &SubagentReturnInput {
        &self.completed_business
    }
}

impl SessionDocument {
    /// Validates maintenance journal ordering and its latest metadata recovery record without repairing either.
    /// 校验维护日志顺序及其最近元数据恢复记录，不修复任一来源。
    /// Returns explicit identity, ordering or closure errors; sealed starts are allowed only before the resident window.
    /// 身份、顺序或闭合错误明确返回；仅允许封存起始位于常驻窗口之前。
    pub(in crate::v1::document) fn validate_context_maintenance_history(
        &self,
    ) -> Result<(), DomainError> {
        // Track each exact auxiliary execution independently from reusable business/session identities.
        // 独立于可复用业务或会话身份跟踪每个精确辅助执行。
        let mut executions =
            BTreeMap::<CollaborationExecutionId, ContextMaintenanceExecution>::new();
        // The latest journal after-state must agree with the metadata snapshot.
        // 最近日志后状态必须与元数据快照一致。
        let mut latest = None;
        for event in self.events() {
            let EventBody::ContextMaintenance(record) = &event.event else {
                continue;
            };
            // History copied by an edit or fork keeps records of the generation that wrote them; a
            // record from a later generation is impossible.
            // 编辑或分叉复制的历史保留写入它们的代次记录；来自更晚代次的记录不可能存在。
            if record.segment_generation > self.segment_generation() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.record",
                });
            }
            record.validate(record.segment_generation)?;
            if record.source_revision >= self.metadata.document_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.source_revision",
                });
            }
            if event.visibility != EventVisibility::HostOnly
                || event.turn_id != Some(record.turn_id)
                || event.event_revision != crate::v1::event::EVENT_REVISION_INITIAL
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.event_owner",
                });
            }
            if let Some(terminal) = &record.terminal {
                if let Some(compaction_id) = terminal.compaction_id {
                    // Successful replay retains the exact committed node, even after a later compaction supersedes it.
                    // 即使后续压缩取代当前指针，成功重放仍保留精确已提交节点。
                    let node = self
                        .compactions
                        .iter()
                        .find(|node| node.compaction_id == compaction_id)
                        .ok_or(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.missing_committed_node",
                        })?;
                    if node.occurred_at_seq <= record.started_seq
                        || node.occurred_at_seq >= terminal.end_seq
                        || node.api_stats_seq <= record.started_seq
                        || node.api_stats_seq >= terminal.end_seq
                        || node.created_at_ms < record.started_at_ms
                        || node.created_at_ms > terminal.completed_at_ms
                    {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.committed_node_boundary",
                        });
                    }
                }
                if event.seq.value().checked_add(2) != Some(terminal.end_seq.value())
                    || event.created_at_ms != terminal.completed_at_ms
                    || event.updated_at_ms != terminal.completed_at_ms
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "context_maintenance.terminal_anchor",
                    });
                }
                if let Some(previous) = executions.get(&record.execution_id) {
                    // Terminal publication may only add terminal fields to its immutable start record.
                    // 终态发布只能向不可变起始记录增加终态字段。
                    let mut expected = previous.clone();
                    if expected.terminal.is_some() {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.duplicate_terminal",
                        });
                    }
                    expected.terminal = Some(terminal.clone());
                    if &expected != record {
                        return Err(DomainError::CollaborationStateViolation {
                            field: "context_maintenance.changed_start",
                        });
                    }
                } else if record.started_seq.value() >= self.resident_start_seq {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "context_maintenance.missing_start",
                    });
                }
                if !self.events().iter().any(|row| row.seq == terminal.end_seq && row.turn_id == Some(record.turn_id)
                    && matches!(&row.event, EventBody::TurnStats(stats) if stats.outcome == terminal.reason.outcome()))
                    || !self.events().iter().any(|row| row.seq.value() + 1 == terminal.end_seq.value() && row.turn_id == Some(record.turn_id)
                    && matches!(&row.event, EventBody::TurnBoundary(boundary) if boundary.purpose == TurnBoundaryPurpose::ContextMaintenance && boundary.reason == terminal.reason))
                {
                    return Err(DomainError::CollaborationStateViolation { field: "context_maintenance.missing_closure" });
                }
            } else if event.seq != record.started_seq
                || event.created_at_ms != record.started_at_ms
                || event.updated_at_ms != record.started_at_ms
                || executions.contains_key(&record.execution_id)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.start_anchor",
                });
            }
            executions.insert(record.execution_id.clone(), record.clone());
            // Only the current generation's journal backs the metadata snapshot; a new generation
            // starts without one.
            // 只有当前代次的日志对应元数据快照；新代次开始时没有快照。
            if record.segment_generation == self.segment_generation() {
                latest = Some(record);
            }
        }
        if let Some(record) = &self.metadata.context_maintenance {
            record.validate(self.segment_generation())?;
            if latest.is_some_and(|latest| latest != record)
                || (latest.is_none() && record.started_seq.value() >= self.resident_start_seq)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.metadata_journal",
                });
            }
            // A child keeps its exact captured profile; a manual root has neither child identity nor profile.
            // 子代理保留精确捕获配置；手动主会话既没有子执行身份，也没有子执行配置。
            let owner_matches = match (
                record.business_execution_id.as_ref(),
                self.metadata.child_execution_profile.as_ref(),
            ) {
                (Some(identity), Some(profile)) => {
                    profile.business_execution_id == identity.as_str()
                        && profile.business_turn_id == record.business_turn_id
                        && profile.segment_generation == record.segment_generation
                }
                (None, None) => true,
                _ => false,
            };
            if record.terminal.is_none()
                && (self.metadata.open_turn_start_seq != Some(record.started_seq) || !owner_matches)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.active_profile",
                });
            }
        } else if latest.is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "context_maintenance.metadata_missing",
            });
        }
        Ok(())
    }

    /// Opens a separate auxiliary turn from `source` and `execution_id`; returns an idempotent durable publication candidate.
    /// 根据 `source` 与 `execution_id` 开启独立辅助回合；返回幂等可靠发布候选。
    /// The caller must retain auxiliary exclusion and publish before dispatch; the original business remains closed.
    /// 调用者必须保持辅助互斥并在分发前发布；原业务保持关闭。
    pub fn begin_child_context_maintenance(
        &mut self,
        source: &ChildContextMaintenanceSource,
        execution_id: CollaborationExecutionId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<ContextMaintenanceExecution>, DomainError> {
        validate_timestamp("context_maintenance.started_at_ms", now_ms)?;
        if self.collaboration_run_control() != CollaborationRunControl::NaturallyIdle
            || self.metadata.archived_at_ms.is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "context_maintenance.stopped",
            });
        }
        if let Some(existing) = &self.metadata.context_maintenance
            && existing.execution_id == execution_id
        {
            if existing.source_revision != source.revision
                || existing
                    .business_execution_id
                    .as_ref()
                    .map(|id| id.as_str())
                    != Some(source.profile.business_execution_id.as_str())
                || self.metadata.child_execution_profile.as_ref() != Some(&source.profile)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.reused_identity",
                });
            }
            return Ok(IdempotentMutation::Idempotent(existing.clone()));
        }
        self.validate_child_context_maintenance_source(source)?;
        if self
            .metadata
            .context_maintenance
            .as_ref()
            .is_some_and(|record| record.terminal.is_none())
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "context_maintenance.already_running",
            });
        }
        self.ensure_transition_capacity(1, 1)?;
        // Allocate a genuinely new owner; no business or question turn is reused.
        // 分配真正的新所有者；不复用业务或答疑回合。
        let mut allocator = self.id_allocator;
        // The fresh local ID owns only maintenance physical requests.
        // 新本地 ID 仅拥有维护物理请求。
        let turn_id = TurnId::try_new(allocator.next_local_id()?)?;
        // Preserve the immutable publication frontier inside the start record.
        // 在起始记录中保留不可变发布前沿。
        let started_seq = Seq::try_new(allocator.frontier().1)?;
        // The journal after-state is also the metadata recovery authority.
        // 日志后状态同时是元数据恢复权威。
        let record = ContextMaintenanceExecution {
            execution_id,
            business_execution_id: Some(CollaborationExecutionId::try_new(
                &source.profile.business_execution_id,
            )?),
            business_turn_id: source.profile.business_turn_id,
            turn_id,
            segment_generation: self.segment_generation(),
            source_revision: source.revision,
            source_compaction_id: source.compaction_id,
            started_seq,
            started_at_ms: now_ms,
            terminal: None,
        };
        record.validate(self.segment_generation())?;
        // Stage all fallible event construction before changing the live document.
        // 在修改实时文档之前暂存所有可失败事件构造。
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

    /// Closes only `execution_id` with `reason` at `now_ms`; success requires its own committed compaction node.
    /// 在 `now_ms` 仅以 `reason` 闭合 `execution_id`；成功要求其自身已提交的压缩节点。
    /// Returns the exact auxiliary terminal sequence and never publishes business results or consumes TTL.
    /// 返回精确辅助终态序号，绝不发布业务结果或消耗 TTL。
    pub fn finish_context_maintenance(
        &mut self,
        execution_id: &CollaborationExecutionId,
        reason: TurnClosureReason,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        validate_timestamp("context_maintenance.completed_at_ms", now_ms)?;
        // Only the exact latest maintenance owner may publish this terminal.
        // 只有精确最近维护所有者可以发布此终态。
        let mut record = self
            .metadata
            .context_maintenance
            .clone()
            .filter(|record| &record.execution_id == execution_id)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "context_maintenance.owner",
            })?;
        if let Some(terminal) = &record.terminal {
            if terminal.reason != reason {
                return Err(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.terminal_conflict",
                });
            }
            return Ok(IdempotentMutation::Idempotent(terminal.end_seq));
        }
        // The open owner must be the independent maintenance turn, never its original business.
        // 开放所有者必须是独立维护回合，绝不能是原业务回合。
        let turn = self.require_open_turn()?.clone();
        if turn.turn_id != record.turn_id
            || turn.origin != TurnOrigin::ContextMaintenance
            || turn.start_seq != record.started_seq
            || self.active_api().is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "context_maintenance.terminal_busy_or_replaced",
            });
        }
        // Retain a permanent exact receipt instead of relying on the mutable current context pointer during reload.
        // 保留永久精确回执，避免重载时依赖可变当前上下文指针。
        let compaction_id = if reason == TurnClosureReason::Completed {
            // A successful Provider response alone is insufficient; the canonical context pointer must be committed.
            // 仅供应商响应成功不足以完成；必须提交规范上下文指针。
            let node = self
                .compactions
                .iter()
                .find(|node| Some(node.compaction_id) == self.runtime_lock.current_compaction_id)
                .filter(|node| Some(node.compaction_id) != record.source_compaction_id)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "context_maintenance.compaction_missing",
                })?;
            if !self.events().iter().any(|event| event.turn_id == Some(record.turn_id)
                && matches!(&event.event, EventBody::ApiStarted(api) if api.api_call_id == node.api_call_id && api.purpose == ApiPurposeKind::Compaction)) {
                return Err(DomainError::CollaborationStateViolation { field: "context_maintenance.foreign_compaction" });
            }
            Some(node.compaction_id)
        } else {
            None
        };
        self.ensure_transition_capacity(3, 0)?;
        // Terminal record, auxiliary boundary and unique statistics share one atomic publication.
        // 终态记录、辅助边界与唯一统计共享一次原子发布。
        let end_seq = Seq::try_new(self.id_allocator.frontier().1.checked_add(2).ok_or(
            DomainError::CollaborationStateViolation {
                field: "context_maintenance.sequence_overflow",
            },
        )?)?;
        record.terminal = Some(ContextMaintenanceTerminal {
            reason,
            compaction_id,
            end_seq,
            completed_at_ms: now_ms,
        });
        record.validate(self.segment_generation())?;
        // Aggregate only the auxiliary turn, leaving original business statistics untouched.
        // 仅聚合辅助回合，保持原业务统计不变。
        let buckets = self.aggregate_turn_buckets(record.turn_id, record.started_seq)?;
        // The copied allocator and staged events isolate failures from the live document.
        // 复制的分配器与暂存事件将失败与实时文档隔离。
        let mut allocator = self.id_allocator;
        // Exactly three events are expected because maintenance never expires business TTL.
        // 预期恰好三条事件，因为维护绝不使业务 TTL 到期。
        let mut events = Vec::new();
        self.stage_event(
            &mut allocator,
            &mut events,
            Some(record.turn_id),
            EventVisibility::HostOnly,
            EventBody::ContextMaintenance(record.clone()),
            now_ms,
        )?;
        if self.stage_turn_closure(
            &mut allocator,
            &mut events,
            record.turn_id,
            record.started_seq,
            reason,
            now_ms - record.started_at_ms,
            now_ms,
            buckets,
        )? != end_seq
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "context_maintenance.terminal_sequence",
            });
        }
        // Install the complete terminal only after every staged check succeeds.
        // 仅在所有暂存检查成功后安装完整终态。
        let seqs = events
            .iter()
            .map(ConversationEvent::seq)
            .collect::<Vec<_>>();
        self.id_allocator = allocator;
        self.events.extend(events);
        self.metadata.context_maintenance = Some(record);
        self.metadata.open_turn_start_seq = None;
        self.active_turn = Some(ActiveTurnState {
            closed: true,
            ..turn
        });
        self.bump_revision(&[DirtyFile::Metadata, DirtyFile::ActiveTail], now_ms, &seqs);
        Ok(IdempotentMutation::Committed(end_seq))
    }

    /// Captures maintenance authority for `execution_id` only after its exact success is fully durable.
    /// 仅在 `execution_id` 对应精确成功完整持久化后捕获维护权威。
    /// Returns None for an unfinished or non-successful source; busy, stopped, stale or malformed ownership is rejected.
    /// 未结束或非成功来源返回 None；占用、停止、过期或畸形归属明确拒绝。
    /// This read does not reopen the business turn, create an auxiliary execution, or authorize a Provider call.
    /// 本读取不重开业务回合、不创建辅助执行，也不授权 Provider 调用。
    pub fn capture_child_context_maintenance_source(
        &self,
        execution_id: &CollaborationExecutionId,
    ) -> Result<Option<ChildContextMaintenanceSource>, DomainError> {
        if self.memory_revision() != self.durable_revision() {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision(),
                durable_revision: self.durable_revision(),
            });
        }
        if self.metadata.archived_at_ms.is_some()
            || self.collaboration_run_control() != CollaborationRunControl::NaturallyIdle
            || self.active_api().is_some()
            || self.active_turn().is_some_and(|turn| !turn.closed)
            || !self.metadata.input_state.queued_inputs.is_empty()
            || !self.metadata.input_state.pending_in_turn_inputs.is_empty()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_maintenance.busy_or_stopped",
            });
        }
        // Profile ownership is independent from the most recently closed auxiliary turn.
        // 档案归属独立于最近闭合的辅助回合。
        let profile = self.metadata.child_execution_profile.as_ref().ok_or(
            DomainError::CollaborationStateViolation {
                field: "child_maintenance.profile_missing",
            },
        )?;
        profile.validate_owner(&self.metadata.session_id, self.segment_generation())?;
        if profile.business_execution_id != execution_id.as_str() {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_maintenance.original_execution",
            });
        }
        // Reuse the same source proof required for real parent return; a resident Completed flag is insufficient.
        // 复用真实父回流要求的同一来源证明；常驻 Completed 标志不足以授权。
        let Some(completed) =
            self.recover_completed_child_return_request(execution_id, profile.task_id.clone())?
        else {
            return Ok(None);
        };
        if completed.final_turn_id != profile.business_turn_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_maintenance.original_turn",
            });
        }
        // A later finished business task supersedes maintenance even if its new profile has not been captured.
        // 即使新档案尚未捕获，后来的已结束业务任务也会取代旧维护。
        if self
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
            != Some(profile.business_turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_maintenance.business_superseded",
            });
        }
        // The immutable proof is retained so later admission compares exact original content as well as coordinates.
        // 保留不可变证明，使后续准入同时比较精确原正文与坐标。
        let completed_business = self.build_subagent_return_input(completed)?;
        Ok(Some(ChildContextMaintenanceSource {
            profile: profile.clone(),
            completed_business,
            revision: self.durable_revision(),
            compaction_id: self.runtime_lock.current_compaction_id,
        }))
    }

    /// Rechecks `source` inside the canonical maintenance admission transaction before any mutation.
    /// 在规范维护准入事务内、任何修改之前重新检查 `source`。
    /// Returns success only while the exact durable revision, profile, success proof and context pointer remain current.
    /// 仅当精确可靠修订、档案、成功证明及上下文指针仍为当前权威时返回成功。
    pub fn validate_child_context_maintenance_source(
        &self,
        source: &ChildContextMaintenanceSource,
    ) -> Result<(), DomainError> {
        if self.durable_revision() != source.revision
            || self.runtime_lock.current_compaction_id != source.compaction_id
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_maintenance.source_changed",
            });
        }
        // Parsing retains the existing exact execution identifier contract; there is no alternative source lookup.
        // 解析保留既有精确执行标识契约；不存在替代来源查找。
        let execution_id =
            CollaborationExecutionId::try_new(&source.profile.business_execution_id)?;
        if self
            .capture_child_context_maintenance_source(&execution_id)?
            .as_ref()
            != Some(source)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_maintenance.source_changed",
            });
        }
        Ok(())
    }
}
