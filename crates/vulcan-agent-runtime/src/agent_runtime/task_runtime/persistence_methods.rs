use super::*;
use crate::conversation_protocol::append_record;
use vulcan_workbench_model::session_conversation_record_is_subagent_send_message_display_note;

impl TaskRuntime {
    /// Restores persisted child-agent registry entries for one parent tree without restarting work.
    /// 从持久化子任务元数据恢复某个父级树的子代理注册表条目，但不重启工作。
    pub fn restore_persisted_subagent_registry_for_parent_tree(
        &self,
        current_session_id: SessionId,
    ) -> Result<(), String> {
        let root_session_id = self.root_session_id_for_subagent_tree(current_session_id)?;
        let root_session_uuid = Uuid::from(root_session_id);
        if self
            .restored_task_session_registry_roots
            .read()
            .map_err(|_| String::from("The restored task-session registry marker is locked."))?
            .contains(&root_session_uuid)
        {
            return Ok(());
        }
        let Some(store) = self.task_session_store_for_root(Uuid::from(root_session_id))? else {
            self.restored_task_session_registry_roots
                .write()
                .map_err(|_| String::from("The restored task-session registry marker is locked."))?
                .insert(root_session_uuid);
            return Ok(());
        };
        loop {
            let metadata_to_lock = load_validated_open_subagent_metadata(&store, root_session_id)?;
            let mut task_ids = Vec::with_capacity(metadata_to_lock.len().saturating_add(1));
            task_ids.push(root_session_uuid);
            task_ids.extend(
                metadata_to_lock
                    .iter()
                    .map(|metadata| Uuid::from(metadata.task_session_id)),
            );
            let locked_task_ids = task_ids.iter().copied().collect::<BTreeSet<_>>();
            let transaction_locks = self.ordered_task_execution_state_locks(&task_ids)?;
            let mut transaction_guards = Vec::with_capacity(transaction_locks.len());
            for transaction_lock in &transaction_locks {
                transaction_guards.push(transaction_lock.lock().map_err(|_| {
                    String::from(
                        "The task execution transaction is locked and cannot restore persisted subagents.",
                    )
                })?);
            }
            let metadata_to_restore =
                load_validated_open_subagent_metadata(&store, root_session_id)?;
            let mut current_task_ids = BTreeSet::from([root_session_uuid]);
            current_task_ids.extend(
                metadata_to_restore
                    .iter()
                    .map(|metadata| Uuid::from(metadata.task_session_id)),
            );
            if current_task_ids != locked_task_ids {
                drop(transaction_guards);
                continue;
            }
            let restored_at = current_unix_millis_u64();
            let prepared_restores = metadata_to_restore
                .iter()
                .map(|metadata| {
                    self.materialize_persisted_subagent_restore(&store, metadata, restored_at)
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.commit_materialized_persisted_subagent_restores_with_task_locks_held(
                &store,
                &prepared_restores,
            )?;
            drop(transaction_guards);
            self.restored_task_session_registry_roots
                .write()
                .map_err(|_| String::from("The restored task-session registry marker is locked."))?
                .insert(root_session_uuid);
            return Ok(());
        }
    }

    /// Materializes one persisted subagent into complete registry, session, and metadata projections.
    /// 将一个持久化子代理完整物化为注册表、会话与元数据投影。
    ///
    /// # Parameters
    /// 参数
    /// - `store`: Exact segmented store that owns the persisted root tree.
    /// - `store`：拥有持久化根树的精确分段存储。
    /// - `metadata`: Non-archived metadata selected by validated root traversal.
    /// - `metadata`：经根遍历校验选中的未归档元数据。
    /// - `restored_at`: Shared timestamp for every runtime-lost lifecycle normalization.
    /// - `restored_at`：全部 runtime-lost 生命周期规范化共享的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A fully materialized projection, or a read/decoding error before any mutation.
    /// 完整物化投影，或任何变更发生前的读取或解码错误。
    pub(super) fn materialize_persisted_subagent_restore(
        &self,
        store: &SegmentedTaskSessionStore,
        metadata: &TaskSessionMetadata,
        restored_at: u64,
    ) -> Result<PreparedPersistedSubagentRestore, String> {
        let mut normalized_metadata = metadata.clone();
        let normalized_active_metadata = if metadata.status.is_active() {
            normalized_metadata.status = TaskSessionStatus::Stopped;
            normalized_metadata.stop_reason = Some(String::from("runtime_lost"));
            normalized_metadata.updated_at = restored_at;
            Some(normalized_metadata.clone())
        } else {
            None
        };
        let mut entry = subagent_registry_entry_identity_from_metadata(&normalized_metadata);
        entry.prompt_source = Some(subagent_prompt_source_for_task_config(
            normalized_metadata.subagent_profile_id.as_deref(),
        ));
        entry.last_task_message = store
            .load_latest_event_payload(normalized_metadata.task_session_id)
            .map_err(|error| error.message)?
            .and_then(|payload| subagent_last_task_message_from_event_payload(&payload));
        entry.result_summary = normalized_metadata.last_result_summary.clone();
        entry.segments = store
            .load_task_segment_summaries(normalized_metadata.task_session_id)
            .map_err(|error| error.message)?;
        let restored_prompt = entry
            .last_task_message
            .clone()
            .or_else(|| normalized_metadata.last_result_summary.clone())
            .unwrap_or_else(|| normalized_metadata.display_title.clone());
        let restored_context = store
            .load_context(normalized_metadata.task_session_id)
            .map_err(|error| error.message)?;
        let record = self.build_task_session_record_from_metadata(
            &normalized_metadata,
            restored_context.as_ref(),
            restored_prompt,
        )?;
        Ok(PreparedPersistedSubagentRestore {
            entry,
            record,
            original_metadata: metadata.clone(),
            normalized_active_metadata,
        })
    }

    /// Atomically commits fully materialized persisted subagent projections.
    /// 原子提交已完整物化的持久化子代理投影。
    ///
    /// # Parameters
    /// 参数
    /// - `store`: Exact segmented store used for normalized metadata writes and rollback.
    /// - `store`：用于规范化元数据写入与回滚的精确分段存储。
    /// - `prepared_restores`: Fallible reads and decoding already completed for the whole tree.
    /// - `prepared_restores`：整棵树已完成可失败读取与解码的准备投影。
    ///
    /// # Returns
    /// 返回值
    /// Success after persistence and both runtime projections commit, or an error with rollback detail.
    /// 持久化与两个运行时投影提交后返回成功，否则返回包含回滚详情的错误。
    #[cfg(test)]
    pub(super) fn commit_materialized_persisted_subagent_restores(
        &self,
        store: &SegmentedTaskSessionStore,
        prepared_restores: &[PreparedPersistedSubagentRestore],
    ) -> Result<(), String> {
        let mut task_ids = Vec::with_capacity(prepared_restores.len().saturating_mul(2));
        for prepared in prepared_restores {
            task_ids.push(Uuid::from(prepared.entry.session_id));
            task_ids.push(prepared.record.origin_session_id);
        }
        let transaction_locks = self.ordered_task_execution_state_locks(&task_ids)?;
        let mut transaction_guards = Vec::with_capacity(transaction_locks.len());
        for transaction_lock in &transaction_locks {
            transaction_guards.push(transaction_lock.lock().map_err(|_| {
                String::from(
                    "The task execution transaction is locked and cannot restore persisted subagents.",
                )
            })?);
        }
        self.commit_materialized_persisted_subagent_restores_with_task_locks_held(
            store,
            prepared_restores,
        )
    }

    /// Commits materialized restore projections while all root and task transaction locks are held.
    /// 在全部根与任务事务锁已持有时提交物化恢复投影。
    ///
    /// # Parameters
    /// 参数
    /// - `store`: Exact segmented store used for metadata persistence and rollback.
    /// - `store`：用于元数据持久化与回滚的精确分段存储。
    /// - `prepared_restores`: Lock-stable materialized projections for the complete open tree.
    /// - `prepared_restores`：完整开放树对应的锁稳定物化投影。
    ///
    /// # Returns
    /// 返回值
    /// Success after persistence and runtime clone-swap, or an error before runtime mutation.
    /// 持久化与运行时克隆交换后返回成功，否则在运行时变更前返回错误。
    fn commit_materialized_persisted_subagent_restores_with_task_locks_held(
        &self,
        store: &SegmentedTaskSessionStore,
        prepared_restores: &[PreparedPersistedSubagentRestore],
    ) -> Result<(), String> {
        let mut registry = self
            .subagent_registry
            .write()
            .map_err(|_| String::from("The subagent registry is locked and cannot be restored."))?;
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task-session registry is locked and cannot restore subagent records.")
        })?;
        let mut registry_preview = registry.clone();
        let mut sessions_preview = sessions.clone();
        let mut metadata_updates = Vec::new();
        for prepared in prepared_restores {
            let task_id = Uuid::from(prepared.entry.session_id);
            let existing_registry_entry = registry_preview
                .get_for_parent(prepared.entry.parent_session_id, &prepared.entry.agent_id)
                .cloned();
            let existing_record = sessions_preview.get(&task_id).cloned();
            match (existing_registry_entry, existing_record) {
                (Some(mut existing), Some(existing_record))
                    if existing.session_id == prepared.entry.session_id =>
                {
                    Self::validate_resident_task_record_identity(&existing_record, prepared)?;
                    let resident_status = subagent_status_from_task_session_status(
                        task_session_status_from_record_status(
                            existing_record.last_status.as_str(),
                        )?,
                    );
                    if existing.status != resident_status {
                        return Err(format!(
                            "Resident task session `{task_id}` status `{resident_status:?}` conflicts with registry status `{:?}`.",
                            existing.status
                        ));
                    }
                    merge_persisted_subagent_entry_without_status_overwrite(
                        &mut existing,
                        prepared.entry.clone(),
                    );
                    registry_preview
                        .upsert(existing)
                        .map_err(|error| error.to_string())?;
                    let resident_metadata = Self::resident_task_session_metadata_from_record(
                        &prepared.original_metadata,
                        &existing_record,
                    )?;
                    if resident_metadata != prepared.original_metadata {
                        metadata_updates
                            .push((prepared.original_metadata.clone(), resident_metadata));
                    }
                }
                (Some(existing), _) => {
                    registry_preview
                        .upsert(prepared.entry.clone())
                        .map_err(|error| error.to_string())?;
                    return Err(format!(
                        "Parent-scoped registry owner `{}` for session `{}` has no matching resident task record.",
                        existing.agent_id, existing.session_id
                    ));
                }
                (None, Some(existing_record)) => {
                    Self::validate_resident_task_record_identity(&existing_record, prepared)?;
                    let resident_entry = Self::resident_subagent_registry_entry_from_record(
                        &existing_record,
                        &prepared.entry,
                    )?;
                    registry_preview
                        .upsert(resident_entry)
                        .map_err(|error| error.to_string())?;
                    let resident_metadata = Self::resident_task_session_metadata_from_record(
                        &prepared.original_metadata,
                        &existing_record,
                    )?;
                    if resident_metadata != prepared.original_metadata {
                        metadata_updates
                            .push((prepared.original_metadata.clone(), resident_metadata));
                    }
                }
                (None, None) => {
                    registry_preview
                        .upsert(prepared.entry.clone())
                        .map_err(|error| error.to_string())?;
                    sessions_preview.insert(task_id, prepared.record.clone());
                    if let Some(normalized_metadata) = prepared.normalized_active_metadata.as_ref()
                    {
                        metadata_updates.push((
                            prepared.original_metadata.clone(),
                            normalized_metadata.clone(),
                        ));
                    }
                }
            }
        }
        let mut rollback_snapshots = Vec::with_capacity(metadata_updates.len());
        for (original_metadata, normalized_metadata) in metadata_updates {
            rollback_snapshots.push(original_metadata);
            if let Err(error) = store.save_task_session(normalized_metadata.clone()) {
                let primary_error = format!(
                    "Failed to persist normalized restored task session `{}`: {}",
                    normalized_metadata.task_session_id, error.message
                );
                return match restore_task_session_metadata_snapshots(store, &rollback_snapshots) {
                    Ok(()) => Err(format!(
                        "{primary_error} Original metadata snapshots were restored."
                    )),
                    Err(rollback_error) => Err(format!(
                        "{primary_error} Metadata rollback also failed: {rollback_error}"
                    )),
                };
            }
        }
        *registry = registry_preview;
        *sessions = sessions_preview;
        Ok(())
    }

    /// Validates stable identity shared by one resident and one persisted task-session record.
    /// 校验一条驻留任务会话记录与一条持久化任务会话记录共享的稳定身份。
    ///
    /// # Parameters
    /// 参数
    /// - `existing_record`: Resident record treated as the live lifecycle authority.
    /// - `existing_record`：作为实时生命周期权威的驻留记录。
    /// - `prepared`: Fully materialized persisted projection for the same stable session id.
    /// - `prepared`：同一稳定会话标识对应的完整物化持久化投影。
    ///
    /// # Returns
    /// 返回值
    /// Success only when task, public, root, parent, and depth identities match exactly.
    /// 仅当任务、公开标识、根、父级与深度身份完全一致时返回成功。
    fn validate_resident_task_record_identity(
        existing_record: &TaskSessionRecord,
        prepared: &PreparedPersistedSubagentRestore,
    ) -> Result<(), String> {
        let identity_matches = existing_record.task_id == prepared.record.task_id
            && existing_record.public_task_id == prepared.record.public_task_id
            && existing_record.origin_session_id == prepared.record.origin_session_id
            && existing_record.parent_task_id == prepared.record.parent_task_id
            && existing_record.depth == prepared.record.depth;
        if identity_matches {
            Ok(())
        } else {
            Err(format!(
                "Resident task session `{}` conflicts with persisted subagent `{}` identity.",
                existing_record.task_id, prepared.entry.agent_id
            ))
        }
    }

    /// Projects resident lifecycle authority back into one persisted metadata snapshot.
    /// 将驻留生命周期权威回投到一份持久化元数据快照。
    ///
    /// # Parameters
    /// 参数
    /// - `metadata`: Original persisted metadata retaining configuration and structural fields.
    /// - `metadata`：保留配置与结构字段的原始持久化元数据。
    /// - `record`: Identity-validated resident lifecycle authority.
    /// - `record`：已经身份校验的驻留生命周期权威。
    ///
    /// # Returns
    /// 返回值
    /// Metadata with status, timestamp, stop reason, and result summary aligned to the resident record.
    /// 状态、时间戳、停止原因与结果摘要已对齐驻留记录的元数据。
    fn resident_task_session_metadata_from_record(
        metadata: &TaskSessionMetadata,
        record: &TaskSessionRecord,
    ) -> Result<TaskSessionMetadata, String> {
        let status = task_session_status_from_record_status(record.last_status.as_str())?;
        let updated_at = record.updated_at.parse::<u64>().map_err(|error| {
            format!(
                "Resident task session `{}` has invalid updated_at `{}`: {error}",
                record.task_id, record.updated_at
            )
        })?;
        let mut resident_metadata = metadata.clone();
        resident_metadata.status = status;
        resident_metadata.updated_at = updated_at;
        resident_metadata.stop_reason = record.stop_reason.clone();
        resident_metadata.last_result_summary =
            task_record_registry_result_summary(record, status).map(str::to_string);
        Ok(resident_metadata)
    }

    /// Projects one live resident task record into a registry entry enriched by persisted display data.
    /// 将一条实时驻留任务记录投影为由持久化展示数据增强的注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// - `record`: Resident lifecycle and stable identity authority.
    /// - `record`：驻留生命周期与稳定身份权威。
    /// - `persisted_entry`: Materialized display fields from persistent storage.
    /// - `persisted_entry`：从持久化存储物化的展示字段。
    ///
    /// # Returns
    /// 返回值
    /// A registry entry whose lifecycle matches the resident record, or an explicit decoding error.
    /// 生命周期与驻留记录一致的注册表条目，或显式解码错误。
    fn resident_subagent_registry_entry_from_record(
        record: &TaskSessionRecord,
        persisted_entry: &SubagentRegistryEntry,
    ) -> Result<SubagentRegistryEntry, String> {
        let status = task_session_status_from_record_status(record.last_status.as_str())?;
        let created_at_ms = record.created_at.parse::<u64>().map_err(|error| {
            format!(
                "Resident task session `{}` has invalid created_at `{}`: {error}",
                record.task_id, record.created_at
            )
        })?;
        let updated_at_ms = record.updated_at.parse::<u64>().map_err(|error| {
            format!(
                "Resident task session `{}` has invalid updated_at `{}`: {error}",
                record.task_id, record.updated_at
            )
        })?;
        let parent_session_id = record.parent_task_id.unwrap_or(record.origin_session_id);
        let mut entry = SubagentRegistryEntry::new(
            record.public_task_id.clone(),
            SessionNodeId::from(record.task_id),
            SessionNodeId::from(parent_session_id),
            subagent_kind_for_task_record(record),
            created_at_ms,
        );
        entry.status = subagent_status_from_task_session_status(status);
        entry.stop_reason = record.stop_reason.clone();
        entry.updated_at_ms = updated_at_ms;
        entry.prompt_source = Some(subagent_prompt_source_for_task_record(record));
        if !record.last_prompt.trim().is_empty() {
            entry.last_task_message = Some(record.last_prompt.clone());
        }
        if let Some(result_summary) = task_record_registry_result_summary(record, status) {
            entry.result_summary = Some(result_summary.to_string());
        }
        let mut persisted_display_entry = persisted_entry.clone();
        if status.is_active() {
            persisted_display_entry.result_summary = None;
        }
        merge_persisted_subagent_entry_without_status_overwrite(
            &mut entry,
            persisted_display_entry,
        );
        Ok(entry)
    }

    /// Builds a lightweight in-memory TaskSession record for one open persisted subagent.
    /// 为一个未关闭的持久化子代理构建轻量内存 TaskSession 记录。
    ///
    /// # Parameters
    /// 参数
    /// - `metadata`: Normalized persisted identity and lifecycle metadata.
    /// - `metadata`：规范化后的持久化身份与生命周期元数据。
    /// - `context`: Optional persisted context document.
    /// - `context`：可选的持久化上下文文档。
    /// - `prompt`: Restored prompt used by the in-memory invocation projection.
    /// - `prompt`：内存调用投影使用的恢复提示词。
    ///
    /// # Returns
    /// 返回值
    /// One runtime record derived from the same normalized lifecycle projection.
    /// 从同一规范化生命周期投影派生的一条运行时记录。
    fn build_task_session_record_from_metadata(
        &self,
        metadata: &TaskSessionMetadata,
        context: Option<&SessionContextDocument>,
        prompt: String,
    ) -> Result<TaskSessionRecord, String> {
        if metadata.status == TaskSessionStatus::Archived {
            return Err(format!(
                "Archived task session `{}` cannot be materialized as an open runtime record.",
                metadata.task_session_id
            ));
        }
        task_session_record_from_persisted_metadata(
            metadata,
            context,
            prompt.clone(),
            TaskInvocationConfig::main(prompt),
            None,
            None,
        )
    }

    /// Persists one close-agent archive transaction and retains its exact rollback resources.
    /// 持久化一次 close-agent 归档事务，并保留其精确回滚资源。
    ///
    /// # Parameters
    /// 参数
    /// - `root_session_id`: Root session whose configured store owns the archive metadata.
    /// - `root_session_id`：其已配置存储拥有归档元数据的根会话。
    /// - `prepared_archive`: Prepared entries and task ids that define the exact transaction scope.
    /// - `prepared_archive`：定义精确事务范围的已准备条目与任务标识。
    ///
    /// # Returns
    /// 返回值
    /// Persistence commit retaining the exact store and rollback snapshots, or typed failure.
    /// 保留精确存储与回滚快照的持久化提交，或类型化失败。
    pub(super) fn persist_close_agent_archive_transaction(
        &self,
        root_session_id: SessionId,
        prepared_archive: &PreparedCloseAgentArchive,
    ) -> Result<CloseAgentArchivePersistenceCommit, CloseAgentArchiveFailure> {
        let Some(store) = self
            .task_session_store_for_root(root_session_id)
            .map_err(|error| {
                CloseAgentArchiveFailure::new(
                    CloseAgentArchiveFailurePhase::PersistArchive,
                    error,
                    CloseAgentArchiveRollbackOutcome::NotRequired,
                )
            })?
        else {
            return Ok(CloseAgentArchivePersistenceCommit {
                store: None,
                rollback_metadata_snapshots: Vec::new(),
            });
        };
        let mut original_metadata = Vec::new();
        let mut archived_metadata = Vec::new();
        for entry in &prepared_archive.entries {
            let Some(metadata) = store.load_task_session(entry.session_id).map_err(|error| {
                CloseAgentArchiveFailure::new(
                    CloseAgentArchiveFailurePhase::PersistArchive,
                    error.message,
                    CloseAgentArchiveRollbackOutcome::NotRequired,
                )
            })?
            else {
                continue;
            };
            let mut archived = metadata.clone();
            archived.status = TaskSessionStatus::Archived;
            archived.stop_reason = entry.stop_reason.clone();
            // Never move persisted lifecycle time backwards when task finalization raced ahead
            // of the archive projection prepared by close_agent.
            // 当任务收尾先于 close_agent 准备的归档投影时，绝不回退持久化生命周期时间。
            archived.updated_at = entry.updated_at_ms.max(metadata.updated_at);
            original_metadata.push(metadata);
            archived_metadata.push(archived);
        }
        for (metadata_index, metadata) in archived_metadata.into_iter().enumerate() {
            if let Err(save_error) = store.save_task_session(metadata) {
                let touched_metadata = &original_metadata[..=metadata_index];
                let rollback =
                    match restore_task_session_metadata_snapshots(&store, touched_metadata) {
                        Ok(()) => CloseAgentArchiveRollbackOutcome::Completed,
                        Err(error) => CloseAgentArchiveRollbackOutcome::Incomplete { error },
                    };
                return Err(CloseAgentArchiveFailure::new(
                    CloseAgentArchiveFailurePhase::PersistArchive,
                    format!(
                        "Failed to persist the close-agent transaction: {}",
                        save_error.message
                    ),
                    rollback,
                ));
            }
        }
        Ok(CloseAgentArchivePersistenceCommit {
            store: Some(store),
            rollback_metadata_snapshots: original_metadata,
        })
    }

    /// Persists stopped metadata for active tasks interrupted by application shutdown.
    /// 为应用关闭打断的活动任务持久化 stopped 元数据。
    pub(super) fn persist_app_shutdown_stopped_task_records(
        &self,
        task_ids: &[SessionId],
    ) -> Result<(), String> {
        if task_ids.is_empty() {
            return Ok(());
        }
        let records = {
            let sessions = self.sessions.read().map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot persist app-shutdown stops.",
                )
            })?;
            task_ids
                .iter()
                .filter_map(|task_id| sessions.get(task_id).cloned())
                .collect::<Vec<_>>()
        };
        let stopped_at = current_unix_millis_u64();
        for record in records {
            let Some(store) = self.task_session_store_for_root(record.origin_session_id)? else {
                continue;
            };
            let Some(mut metadata) = store
                .load_task_session(SessionNodeId::from(record.task_id))
                .map_err(|error| error.message)?
            else {
                continue;
            };
            if metadata.status != TaskSessionStatus::Archived {
                metadata.status = TaskSessionStatus::Stopped;
                metadata.stop_reason = Some(String::from("app_shutdown"));
                metadata.updated_at = stopped_at;
                metadata.last_result_summary = Some(String::from(STOPPED_TASK_RESULT_SUMMARY));
                store
                    .save_task_session(metadata)
                    .map_err(|error| error.message)?;
            }
        }
        Ok(())
    }

    /// Stores one newly completed task outcome back into the session registry.
    /// 将一条新完成的任务结果回写到会话注册表中。
    pub(super) fn store_task_outcome(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        let transaction_lock = self.task_execution_state_lock(invocation.task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot store the terminal outcome.",
            )
        })?;
        if !self.task_execution_can_commit_status(
            invocation,
            task_session_status_from_execution_status(outcome.execution_status),
        )? {
            task_runtime_trace(
                "task_outcome_stale_execution_skipped",
                format!(
                    "task={} execution_id={}",
                    invocation.task_id, invocation.execution_id
                ),
            );
            return Ok(());
        }
        let record = self.build_task_outcome_record(current_session_id, invocation, outcome)?;
        let persisted_status = self.persist_task_record_outcome(&record, invocation, outcome)?;
        let record = self.task_session_record_for_persisted_status(record, persisted_status)?;
        self.commit_task_session_record(record.clone())?;
        self.upsert_subagent_registry_entry(
            &record,
            persisted_status,
            task_record_registry_result_summary(&record, persisted_status),
        )?;
        self.publish_subagent_mailbox_event(
            invocation,
            task_session_status_from_execution_status(outcome.execution_status),
            Some(outcome.final_output_text.as_str()),
            None,
            Some(outcome),
        );
        Ok(())
    }

    /// Stores one running task outcome and persists the task creation/start state once.
    /// 保存运行中的任务结果，并且只持久化一次子任务创建/启动状态。
    pub(super) fn store_running_task_outcome_in_memory(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        debug_assert_eq!(outcome.execution_status, TaskExecutionStatus::Running);
        let transaction_lock = self.task_execution_state_lock(invocation.task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot store the running outcome.",
            )
        })?;
        if !self.task_execution_can_commit_status(invocation, TaskSessionStatus::Running)? {
            task_runtime_trace(
                "task_running_outcome_stale_execution_skipped",
                format!(
                    "task={} execution_id={}",
                    invocation.task_id, invocation.execution_id
                ),
            );
            return Ok(());
        }
        let record = self.build_task_outcome_record(current_session_id, invocation, outcome)?;
        let persisted_status = self.persist_task_record_state(
            &record,
            invocation,
            TaskSessionStatus::Running,
            None,
            None,
            Some(&outcome.snapshot),
            Some(&outcome.controller_config),
            Some(outcome),
        )?;
        let record = self.task_session_record_for_persisted_status(record, persisted_status)?;
        self.commit_task_session_record(record.clone())?;
        self.upsert_subagent_registry_entry(
            &record,
            persisted_status,
            task_record_registry_result_summary(&record, persisted_status),
        )?;
        Ok(())
    }

    /// Mirrors one prepared in-memory TaskSession record into the runtime registry before UI projection.
    /// 在 UI 投影前，将一条已准备好的内存 TaskSession 记录同步到运行时注册表。
    pub(super) fn upsert_prepared_task_registry_entry(
        &self,
        task_id: SessionId,
    ) -> Result<(), String> {
        let record = self
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The task session registry is locked and cannot mirror the prepared child agent.",
                )
            })?
            .get(&task_id)
            .cloned()
            .ok_or_else(|| {
                format!("Prepared child-agent task `{task_id}` is missing from the session registry.")
            })?;
        let status = task_session_status_from_record_status(record.last_status.as_str())?;
        self.upsert_subagent_registry_entry(
            &record,
            status,
            task_record_registry_result_summary(&record, status),
        )
    }

    /// Builds the next task-session record without mutating the in-memory session map.
    /// 构建下一条任务会话记录，但不修改内存会话表。
    fn build_task_outcome_record(
        &self,
        _current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<TaskSessionRecord, String> {
        let sessions = self.sessions.read().map_err(|_| {
            String::from("The task session registry is locked and cannot store the result.")
        })?;
        let now = current_unix_millis_text();
        let status = outcome.execution_status.as_str().to_string();
        let resume_controller_config = task_controller_config_for_resume_snapshot(
            &outcome.controller_config,
            &outcome.resolved_config,
        );
        let mut record = sessions
            .get(&invocation.task_id)
            .cloned()
            .unwrap_or_else(|| {
                build_task_session_record(TaskSessionRecordInit {
                    task_id: invocation.task_id,
                    public_task_id: invocation.public_task_id.clone(),
                    origin_session_id: invocation.root_session_id,
                    parent_task_id: invocation.parent_task_id,
                    depth: invocation.depth,
                    description: invocation.description.clone(),
                    subagent: invocation.subagent.clone(),
                    invocation_config: invocation.invocation_config.clone(),
                    display_title: invocation.display_title.clone(),
                    controller_config: Some(resume_controller_config.clone()),
                    snapshot: invocation.snapshot.clone(),
                    last_prompt: invocation.prompt.clone(),
                    last_status: String::from("created"),
                })
            });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = &record.created_at;
        record.controller_config = Some(resume_controller_config);
        record.snapshot = outcome.snapshot.clone();
        record.last_prompt = invocation.prompt.clone();
        record.last_result_text = outcome.final_output_text.clone();
        record.updated_at = now;
        record.last_status = status;
        record.execution_id = Some(invocation.execution_id.clone());
        record.display_title = invocation.display_title.clone();
        record.invocation_config = invocation.invocation_config.clone();
        if record.description.is_none() {
            record.description = invocation.description.clone();
        }
        if record.subagent.is_none() {
            record.subagent = invocation.subagent.clone();
        }
        Ok(record)
    }

    /// Commits one already-persisted task-session record into the in-memory session map.
    /// 将一条已持久化成功的任务会话记录提交到内存会话表。
    fn commit_task_session_record(&self, record: TaskSessionRecord) -> Result<(), String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task session registry is locked and cannot commit the result.")
        })?;
        sessions.insert(record.task_id, record);
        Ok(())
    }

    /// Aligns an in-memory task record with the status that persistence accepted as canonical.
    /// 将内存任务记录对齐到持久化层接受的规范状态。
    fn task_session_record_for_persisted_status(
        &self,
        mut record: TaskSessionRecord,
        persisted_status: TaskSessionStatus,
    ) -> Result<TaskSessionRecord, String> {
        let persisted_status_name = status_name_for_task_session_status(persisted_status);
        if record.last_status == persisted_status_name {
            return Ok(record);
        }
        if !persisted_status.is_active()
            && let Some(existing) = self
                .sessions
                .read()
                .map_err(|_| {
                    String::from(
                        "The task session registry is locked and cannot preserve terminal status.",
                    )
                })?
                .get(&record.task_id)
                .cloned()
                .filter(|existing| existing.last_status == persisted_status_name)
        {
            return Ok(existing);
        }
        record.last_status = persisted_status_name.to_string();
        Ok(record)
    }

    /// Builds the next failed task-session record without mutating the in-memory session map.
    /// 构建下一条失败任务会话记录，但不修改内存会话表。
    fn build_failed_task_record(
        &self,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<TaskSessionRecord, String> {
        let sessions = self.sessions.read().map_err(|_| {
            String::from("The task session registry is locked and cannot store the failure state.")
        })?;
        let mut record = sessions
            .get(&invocation.task_id)
            .cloned()
            .unwrap_or_else(|| {
                build_task_session_record(TaskSessionRecordInit {
                    task_id: invocation.task_id,
                    public_task_id: invocation.public_task_id.clone(),
                    origin_session_id: invocation.root_session_id,
                    parent_task_id: invocation.parent_task_id,
                    depth: invocation.depth,
                    description: invocation.description.clone(),
                    subagent: invocation.subagent.clone(),
                    invocation_config: invocation.invocation_config.clone(),
                    display_title: invocation.display_title.clone(),
                    controller_config: invocation.controller_config.clone(),
                    snapshot: invocation.snapshot.clone(),
                    last_prompt: invocation.prompt.clone(),
                    last_status: String::from("created"),
                })
            });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = &record.created_at;
        record.last_result_text = error.to_string();
        record.updated_at = current_unix_millis_text();
        record.last_status = String::from("failed");
        record.execution_id = Some(invocation.execution_id.clone());
        record.display_title = invocation.display_title.clone();
        record.invocation_config = invocation.invocation_config.clone();
        if record.controller_config.is_none() {
            record.controller_config = invocation.controller_config.clone();
        }
        Ok(record)
    }

    /// Upserts one Vulcan-native subagent registry entry from a task-session record.
    /// 根据任务会话记录更新或插入一条 Vulcan 原生子代理注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// `record` is the in-memory task-session record to mirror.
    /// `record` 是需要镜像的内存任务会话记录。
    /// `status` is the current task-session status.
    /// `status` 是当前任务会话状态。
    /// `result_summary` is an optional compact terminal or running result summary.
    /// `result_summary` 是可选的紧凑终态或运行结果摘要。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the registry entry is updated.
    /// 注册表条目更新成功时返回 `Ok(())`。
    pub(super) fn upsert_subagent_registry_entry(
        &self,
        record: &TaskSessionRecord,
        status: TaskSessionStatus,
        result_summary: Option<&str>,
    ) -> Result<(), String> {
        let parent_session_id = record.parent_task_id.unwrap_or(record.origin_session_id);
        let created_at_ms = record.created_at.parse::<u64>().unwrap_or_default();
        let updated_at_ms = record.updated_at.parse::<u64>().unwrap_or(created_at_ms);
        let mut entry = SubagentRegistryEntry::new(
            record.public_task_id.clone(),
            SessionNodeId::from(record.task_id),
            SessionNodeId::from(parent_session_id),
            subagent_kind_for_task_record(record),
            created_at_ms,
        );
        entry.status = subagent_status_from_task_session_status(status);
        entry.stop_reason = record.stop_reason.clone();
        entry.updated_at_ms = updated_at_ms;
        entry.prompt_source = Some(subagent_prompt_source_for_task_record(record));
        if !record.last_prompt.trim().is_empty() {
            entry.last_task_message = Some(record.last_prompt.clone());
        }
        if let Some(result_summary) = result_summary
            && !result_summary.trim().is_empty()
        {
            entry.result_summary = Some(result_summary.to_string());
        }
        let persisted_segments = self
            .load_task_segment_summaries_for_record(record)
            .unwrap_or_default();
        entry.segments = persisted_segments;
        let mut registry = self
            .subagent_registry
            .write()
            .map_err(|_| String::from("The subagent registry is locked and cannot be updated."))?;
        if let Some(existing) = registry
            .get_for_parent(entry.parent_session_id, &entry.agent_id)
            .cloned()
        {
            let same_task_session = existing.session_id == SessionNodeId::from(record.task_id);
            if same_task_session {
                // Only late updates for the same closed task preserve archived state.
                // 只有同一已关闭任务的晚到更新才保留归档状态。
                if existing.status == SubagentStatus::Archived {
                    entry.status = SubagentStatus::Archived;
                    entry.stop_reason = existing.stop_reason.clone();
                    entry.updated_at_ms = existing.updated_at_ms;
                }
                if entry.stop_reason.is_none() {
                    entry.stop_reason = existing.stop_reason.clone();
                }
                if entry.segments.is_empty() {
                    entry.segments = existing.segments;
                }
                if entry.result_summary.is_none() {
                    entry.result_summary = existing.result_summary;
                }
                if entry.last_task_message.is_none() {
                    entry.last_task_message = existing.last_task_message;
                }
            }
        }
        registry.upsert(entry).map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Stores one failed task outcome while preserving the latest snapshot for later resumption.
    /// 保存一条失败的任务结果，同时保留最新快照供后续续跑使用。
    pub(super) fn mark_task_failure(
        &self,
        _current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<(), String> {
        let transaction_lock = self.task_execution_state_lock(invocation.task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot store the failure outcome.",
            )
        })?;
        if !self.task_execution_can_commit_status(invocation, TaskSessionStatus::Failed)? {
            task_runtime_trace(
                "task_failure_stale_execution_skipped",
                format!(
                    "task={} execution_id={}",
                    invocation.task_id, invocation.execution_id
                ),
            );
            return Ok(());
        }
        let record = self.build_failed_task_record(invocation, error)?;
        let persisted_status = self.persist_task_record_failure(&record, invocation, error)?;
        let record = self.task_session_record_for_persisted_status(record, persisted_status)?;
        self.commit_task_session_record(record.clone())?;
        self.upsert_subagent_registry_entry(
            &record,
            persisted_status,
            task_record_registry_result_summary(&record, persisted_status),
        )?;
        self.publish_subagent_mailbox_event(
            invocation,
            TaskSessionStatus::Failed,
            Some(error),
            Some(error),
            None,
        );
        Ok(())
    }

    /// Discards one in-memory child-task placeholder that never reached the start event.
    /// 丢弃一条尚未到达启动事件的内存子任务占位记录。
    pub(super) fn discard_prestarted_task_record(
        &self,
        invocation: &TaskExecutionRequest,
    ) -> Result<(), String> {
        let transaction_lock = self.task_execution_state_lock(invocation.task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot discard the pre-start child agent.",
            )
        })?;
        let created_status = status_name_for_task_session_status(TaskSessionStatus::Created);
        let removed_record = {
            let mut sessions = self.sessions.write().map_err(|_| {
                String::from(
                    "The task session registry is locked and cannot discard the pre-start child agent.",
                )
            })?;
            let should_remove = sessions.get(&invocation.task_id).is_some_and(|record| {
                record.public_task_id == invocation.public_task_id
                    && record.origin_session_id == invocation.root_session_id
                    && record.parent_task_id == invocation.parent_task_id
                    && record.last_status == created_status
                    && record.execution_id.as_deref() == Some(invocation.execution_id.as_str())
            });
            if should_remove {
                sessions.remove(&invocation.task_id)
            } else {
                None
            }
        };
        if let Some(record) = removed_record {
            let parent_session_id =
                SessionNodeId::from(record.parent_task_id.unwrap_or(record.origin_session_id));
            let registry_removal = match self.subagent_registry.write() {
                Ok(mut registry) => registry.remove_exact_leaf(
                    parent_session_id,
                    &record.public_task_id,
                    SessionNodeId::from(record.task_id),
                    SubagentStatus::Created,
                ),
                Err(_) => {
                    self.sessions
                        .write()
                        .map_err(|_| {
                            String::from(
                                "The task session registry is locked and cannot roll back the pre-start discard after the registry lock failed.",
                            )
                        })?
                        .insert(record.task_id, record);
                    return Err(String::from(
                        "The subagent registry is locked and cannot discard the pre-start child agent.",
                    ));
                }
            };
            if let Err(error) = registry_removal {
                self.sessions
                    .write()
                    .map_err(|_| {
                        String::from(
                            "The task session registry is locked and cannot roll back the failed pre-start discard.",
                        )
                    })?
                    .insert(record.task_id, record);
                return Err(error.to_string());
            }
        }
        Ok(())
    }

    /// Publishes one session-boundary child-agent lifecycle event into the parent mailbox.
    /// 将一条 会话边界子代理生命周期事件发布到父级 mailbox。
    fn publish_subagent_mailbox_event(
        &self,
        invocation: &TaskExecutionRequest,
        status: TaskSessionStatus,
        result_text: Option<&str>,
        error_text: Option<&str>,
        outcome: Option<&TaskExecutionOutcome>,
    ) {
        let mut event = build_subagent_mailbox_event(
            invocation,
            status,
            current_unix_millis_u64(),
            result_text,
            error_text,
            outcome,
        );
        match self.persisted_task_turn_stats(invocation.task_id, invocation.execution_id.as_str()) {
            Ok(Some(turn_stats)) => {
                if let Some(payload) = event.payload.as_object_mut() {
                    payload.insert(String::from("turn_stats"), json!(turn_stats.stats));
                    payload.insert(
                        String::from("source_turn_stats_record_id"),
                        Value::from(turn_stats.record_id),
                    );
                } else {
                    eprintln!(
                        "[task-runtime] mailbox payload is not an object for task {} execution {}",
                        invocation.task_id, invocation.execution_id
                    );
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!(
                    "[task-runtime] failed to load mailbox Turn Stats for task {} execution {}: {error}",
                    invocation.task_id, invocation.execution_id
                );
            }
        }
        if let Err(error) = self.subagent_mailbox.push_event(event) {
            eprintln!(
                "[task-runtime] failed to publish subagent mailbox event for task {}: {error}",
                invocation.task_id
            );
        } else {
            self.notify_subagent_mailbox_activity();
        }
    }

    /// Persists one successful or running task state into the segmented TaskSession store when configured.
    /// 在已配置时将成功或运行中的任务状态持久化到分段 TaskSession 存储。
    fn persist_task_record_outcome(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<TaskSessionStatus, String> {
        let status = task_session_status_from_execution_status(outcome.execution_status);
        self.persist_task_record_state(
            record,
            invocation,
            status,
            Some(outcome.final_output_text.as_str()),
            None,
            Some(&outcome.snapshot),
            Some(&outcome.controller_config),
            Some(outcome),
        )
    }

    /// Persists one failed task state into the segmented TaskSession store when configured.
    /// 在已配置时将失败任务状态持久化到分段 TaskSession 存储。
    fn persist_task_record_failure(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<TaskSessionStatus, String> {
        self.persist_task_record_state(
            record,
            invocation,
            TaskSessionStatus::Failed,
            Some(error),
            Some(error),
            invocation
                .controller_config
                .as_ref()
                .map(|_| &invocation.snapshot),
            invocation.controller_config.as_ref(),
            None,
        )
    }

    /// Persists one task state snapshot and append-only lifecycle event.
    /// 持久化一份任务状态快照并追加生命周期事件。
    fn persist_task_record_state(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        status: TaskSessionStatus,
        result_text: Option<&str>,
        error_text: Option<&str>,
        snapshot: Option<&CoreConversationSnapshot>,
        controller_config: Option<&CoreControllerConfig>,
        outcome: Option<&TaskExecutionOutcome>,
    ) -> Result<TaskSessionStatus, String> {
        let now_ms = current_unix_millis_u64();
        let parent = parent_ref_for_task_record(record);
        let resolved_config = outcome
            .map(|outcome| outcome.resolved_config.clone())
            .unwrap_or_else(|| {
                self.resolve_task_session_config_for_record(record, controller_config)
            });
        let task_session_id = SessionNodeId::from(record.task_id);
        let root_session_id = SessionNodeId::from(record.origin_session_id);
        let mut persisted_status = status;
        if let Some(store) = self.task_session_store_for_root(record.origin_session_id)? {
            let mut metadata = match store
                .load_task_session(task_session_id)
                .map_err(|error| error.message)?
            {
                Some(metadata) => metadata,
                None => {
                    let mut metadata =
                        TaskSessionMetadata::new(vulcan_task_session::TaskSessionMetadataInit {
                            root_session_id,
                            task_session_id,
                            parent: parent.clone(),
                            public_task_id: record.public_task_id.clone(),
                            display_title: record.display_title.clone(),
                            requested_config: resolved_config.requested_config.clone(),
                            effective_config: resolved_config.effective_config.clone(),
                            depth: record.depth as u16,
                            timestamp_ms: now_ms,
                        });
                    metadata.description = record.description.clone();
                    metadata.agent_prompt_snapshot = record
                        .agent_prompt_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.text.clone());
                    store
                        .create_task_session(metadata.clone())
                        .map_err(|error| error.message)?;
                    metadata
                }
            };
            // The TaskSession index must exist before any segmented runtime document is loaded.
            // 在加载任何分段运行时文档之前，TaskSession 索引必须已经存在。
            let previous_runtime_checkpoint = store
                .load_runtime_checkpoint(task_session_id)
                .map_err(|error| error.message)?;
            let mut persisted_provider_call_ledger = previous_runtime_checkpoint
                .as_ref()
                .map(task_provider_call_ledger_from_runtime_checkpoint)
                .transpose()?
                .flatten();
            persisted_status = resolve_persisted_task_session_status(metadata.status, status);
            if persisted_status != status {
                return Ok(persisted_status);
            }
            metadata.display_title = record.display_title.clone();
            metadata.description = record.description.clone();
            metadata.subagent_profile_id =
                resolved_config.effective_config.subagent_profile_id.clone();
            metadata.inherit_parent_config = resolved_config.effective_config.inherit_parent_config;
            metadata.status = persisted_status;
            metadata.stop_reason = if persisted_status.is_active() {
                None
            } else {
                record.stop_reason.clone()
            };
            metadata.runtime_policy = resolved_config.runtime_policy;
            metadata.agent_prompt_snapshot = record
                .agent_prompt_snapshot
                .as_ref()
                .map(|snapshot| snapshot.text.clone());
            metadata.requested_config = resolved_config.requested_config.clone();
            metadata.effective_config = resolved_config.effective_config.clone();
            metadata.updated_at = now_ms;
            metadata.last_result_summary = result_text.and_then(task_result_summary);
            metadata.last_error = error_text.map(str::to_string);
            store
                .save_task_session(metadata.clone())
                .map_err(|error| error.message)?;

            let event_sequence = now_ms;
            store
                .append_event(SessionEventRecord::new(
                    format!(
                        "task-node-state-{}-{persisted_status:?}-{event_sequence}",
                        record.task_id
                    ),
                    task_session_id,
                    event_sequence,
                    task_session_event_kind_for_status(persisted_status),
                    now_ms,
                    task_state_event_payload(
                        record,
                        invocation,
                        result_text,
                        error_text,
                        persisted_status,
                        now_ms,
                        outcome,
                    ),
                ))
                .map_err(|error| error.message)?;

            let parent_sequence = now_ms;
            store
                .append_task_invocation_event(
                    parent,
                    TaskInvocationEvent {
                        event_id: format!(
                            "task-state-{}-{persisted_status:?}-{parent_sequence}",
                            record.task_id
                        ),
                        root_session_id,
                        parent: parent_ref_for_task_record(record),
                        task_session_id,
                        sequence: parent_sequence,
                        kind: task_invocation_event_kind_for_status(persisted_status),
                        status: persisted_status,
                        prompt: Some(invocation.prompt.clone()),
                        timestamp_ms: now_ms,
                        payload: task_state_event_payload(
                            record,
                            invocation,
                            result_text,
                            error_text,
                            persisted_status,
                            now_ms,
                            outcome,
                        ),
                    },
                )
                .map_err(|error| error.message)?;

            if let (Some(snapshot), Some(controller_config)) = (snapshot, controller_config) {
                let previous_context = store
                    .load_context(task_session_id)
                    .map_err(|error| error.message)?;
                let mut conversation_state = core_snapshot_to_session_conversation_state(snapshot)?;
                scope_task_context_display_records_to_prompt(
                    &mut conversation_state,
                    invocation.prompt.as_str(),
                );
                if !persisted_status.is_active() {
                    let initial_execution_started_at_ms = (!invocation.is_resumed)
                        .then(|| record.created_at.parse::<u64>().ok())
                        .flatten();
                    append_task_context_turn_stats_record(
                        &mut conversation_state,
                        record.task_id,
                        now_ms,
                        persisted_status,
                        result_text.or(error_text).unwrap_or_default(),
                        invocation.execution_id.as_str(),
                        outcome,
                        persisted_provider_call_ledger.as_mut(),
                        initial_execution_started_at_ms,
                    )?;
                }
                let conversation_state_value =
                    serde_json::to_value(&conversation_state).unwrap_or(Value::Null);
                store
                    .save_context(SessionContextDocument {
                        schema_version: 1,
                        node_id: task_session_id,
                        provider_id: Some(controller_config.provider_id.clone()),
                        model_id: Some(controller_config.model_id.clone()),
                        reasoning_effort: None,
                        system_prompt_layers: resolved_config
                            .effective_config
                            .system_prompt_layers
                            .clone(),
                        conversation_state: conversation_state_value,
                        compaction_anchor: previous_context
                            .as_ref()
                            .and_then(|context| context.compaction_anchor.clone()),
                        context_budget_fingerprint: previous_context
                            .as_ref()
                            .and_then(|context| context.context_budget_fingerprint.clone()),
                        tool_catalog_snapshot: Value::Null,
                        provider_request_preview: None,
                    })
                    .map_err(|error| error.message)?;
                let task_session_directory = store
                    .task_session_directory(task_session_id)
                    .map_err(|error| error.message)?;
                sync_segmented_session_display_from_conversation_state(
                    &task_session_directory,
                    &conversation_state,
                )?;
                sync_segmented_session_runtime_log_from_conversation_state(
                    &task_session_directory,
                    &conversation_state,
                    &now_ms.to_string(),
                )?;
                store
                    .save_task_session(metadata.clone())
                    .map_err(|error| error.message)?;
            }

            let mut runtime_checkpoint_payload = json!({
                "task_id": record.public_task_id,
                "display_title": record.display_title,
                "updated_at": now_ms,
            });
            if let Some(provider_call_ledger) = persisted_provider_call_ledger.as_ref() {
                runtime_checkpoint_payload["provider_call_ledger"] =
                    serde_json::to_value(provider_call_ledger).map_err(|error| {
                        format!("Failed to serialize the task Provider call ledger: {error}")
                    })?;
            }
            store
                .save_runtime_checkpoint(SessionRuntimeCheckpoint {
                    node_id: task_session_id,
                    status: status_name_for_task_session_status(persisted_status).to_string(),
                    last_sequence: event_sequence,
                    payload: runtime_checkpoint_payload,
                })
                .map_err(|error| error.message)?;
        }

        Ok(persisted_status)
    }

    /// Persists the complete physical-call ledger for one resident child task.
    /// 为一个常驻子任务持久化完整物理调用账本。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the exact TaskSession that owns the calls.
    /// `task_id` 标识拥有这些调用的精确 TaskSession。
    /// `provider_call_ledger` is the fully projected, revisioned ledger snapshot.
    /// `provider_call_ledger` 是完成投影并带修订号的账本快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns only after the TaskSession runtime checkpoint durably contains the ledger.
    /// 仅在 TaskSession 运行时检查点持久包含账本后返回。
    pub fn persist_task_provider_call_ledger(
        &self,
        task_id: SessionId,
        provider_call_ledger: &SessionProviderCallLedger,
    ) -> Result<(), String> {
        let transaction_lock = self.task_execution_state_lock(task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            format!("The task execution transaction is locked for task `{task_id}`.")
        })?;
        let record = self
            .sessions
            .read()
            .map_err(|_| String::from("The task-session registry is locked and cannot read."))?
            .get(&task_id)
            .cloned()
            .ok_or_else(|| {
                format!("TaskSession `{task_id}` does not exist for Usage persistence.")
            })?;
        let store = self
            .task_session_store_for_root(record.origin_session_id)?
            .ok_or_else(|| {
                format!(
                    "TaskSession `{task_id}` has no configured segmented store for Usage persistence."
                )
            })?;
        let task_session_id = SessionNodeId::from(task_id);
        let previous_checkpoint = store
            .load_runtime_checkpoint(task_session_id)
            .map_err(|error| error.message)?;
        let now_ms = current_unix_millis_u64();
        let mut checkpoint = previous_checkpoint.unwrap_or_else(|| SessionRuntimeCheckpoint {
            node_id: task_session_id,
            status: record.last_status.clone(),
            last_sequence: now_ms,
            payload: json!({
                "task_id": record.public_task_id,
                "display_title": record.display_title,
                "updated_at": now_ms,
            }),
        });
        if checkpoint.node_id != task_session_id {
            return Err(format!(
                "TaskSession `{task_id}` runtime checkpoint belongs to `{}`.",
                checkpoint.node_id
            ));
        }
        let payload = checkpoint.payload.as_object_mut().ok_or_else(|| {
            format!("TaskSession `{task_id}` runtime checkpoint payload is not an object.")
        })?;
        payload.insert(
            String::from("provider_call_ledger"),
            serde_json::to_value(provider_call_ledger).map_err(|error| {
                format!("Failed to serialize the task Provider call ledger: {error}")
            })?,
        );
        payload.insert(String::from("updated_at"), Value::from(now_ms));
        checkpoint.last_sequence = checkpoint.last_sequence.max(now_ms);
        store
            .save_runtime_checkpoint(checkpoint)
            .map_err(|error| error.message)
    }

    /// Loads the persisted physical-call ledger for one resident child task.
    /// 加载一个常驻子任务的持久化物理调用账本。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the exact TaskSession checkpoint owner.
    /// `task_id` 标识精确的 TaskSession 检查点所有者。
    ///
    /// # Returns
    /// 返回值
    /// Returns no ledger for a legacy or non-persistent task and rejects malformed stored data.
    /// 对旧版或非持久化任务返回空值，并拒绝损坏的已存储数据。
    pub(super) fn task_provider_call_ledger(
        &self,
        task_id: SessionId,
    ) -> Result<Option<SessionProviderCallLedger>, String> {
        let record = self
            .sessions
            .read()
            .map_err(|_| String::from("The task-session registry is locked and cannot read."))?
            .get(&task_id)
            .cloned();
        // A newly spawned TaskSession has no durable ledger until its first running snapshot is stored.
        // 新生成的 TaskSession 在首次运行快照持久化前没有持久账本。
        let Some(record) = record else {
            return Ok(None);
        };
        let Some(store) = self.task_session_store_for_root(record.origin_session_id)? else {
            return Ok(None);
        };
        let checkpoint = store
            .load_runtime_checkpoint(SessionNodeId::from(task_id))
            .map_err(|error| error.message)?;
        checkpoint
            .as_ref()
            .map(task_provider_call_ledger_from_runtime_checkpoint)
            .transpose()
            .map(Option::flatten)
    }

    /// Loads the ledger-materialized Turn Stats for one exact child execution.
    /// 加载一个精确子执行由账本物化得到的 Turn Stats。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the TaskSession context document.
    /// `task_id` 标识 TaskSession 上下文文档。
    /// `execution_id` selects the exact terminal materialization across resumes.
    /// `execution_id` 在多次续跑中选择精确的终态物化结果。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value only when the task has no segmented store or no matching legacy node.
    /// 仅当任务没有分段存储或没有匹配的旧版节点时返回空值。
    pub(super) fn persisted_task_turn_stats(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<Option<PersistedTaskTurnStats>, String> {
        let record = self
            .sessions
            .read()
            .map_err(|_| String::from("The task-session registry is locked and cannot read."))?
            .get(&task_id)
            .cloned()
            .ok_or_else(|| format!("TaskSession `{task_id}` does not exist for stats loading."))?;
        let Some(store) = self.task_session_store_for_root(record.origin_session_id)? else {
            return Ok(None);
        };
        let Some(context) = store
            .load_context(SessionNodeId::from(task_id))
            .map_err(|error| error.message)?
        else {
            return Ok(None);
        };
        let conversation_state =
            serde_json::from_value::<SessionConversationState>(context.conversation_state)
                .map_err(|error| format!("TaskSession `{task_id}` context is invalid: {error}"))?;
        let stats_record = conversation_state
            .records
            .iter()
            .rev()
            .filter(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
            .find(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("task_execution_id"))
                    .and_then(Value::as_str)
                    == Some(execution_id)
            })
            .map(|record| {
                let turn_stats = record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("turn_stats"))
                    .ok_or_else(|| {
                        format!(
                            "TaskSession `{task_id}` Turn Stats record `{}` has no typed payload.",
                            record.id
                        )
                    })?;
                let stats = serde_json::from_value::<SessionTurnStats>(turn_stats.clone())
                    .map_err(|error| {
                    format!(
                        "TaskSession `{task_id}` Turn Stats for execution `{execution_id}` are invalid: {error}"
                    )
                })?;
                Ok::<PersistedTaskTurnStats, String>(PersistedTaskTurnStats {
                    record_id: record.id.clone(),
                    stats,
                })
            })
            .transpose()?;
        Ok(stats_record)
    }

    /// Builds one segmented TaskSession store for the given root session when a host configured it.
    /// 当宿主已配置根目录时，为给定根会话构建一份分段 TaskSession 存储。
    pub(super) fn task_session_store_for_root(
        &self,
        root_session_id: SessionId,
    ) -> Result<Option<SegmentedTaskSessionStore>, String> {
        let root = self
            .task_session_store_roots
            .read()
            .map_err(|_| {
                String::from("The task-session store root registry is locked and cannot read.")
            })?
            .get(&root_session_id)
            .cloned();
        Ok(root.map(SegmentedTaskSessionStore::new))
    }

    /// Loads child task segment summaries from the segmented task metadata.
    /// 从分段子任务元数据加载子任务分段摘要。
    fn load_task_segment_summaries_for_record(
        &self,
        record: &TaskSessionRecord,
    ) -> Result<Vec<SessionSegmentSummary>, String> {
        let Some(store) = self.task_session_store_for_root(record.origin_session_id)? else {
            return Ok(Vec::new());
        };
        store
            .load_task_segment_summaries(SessionNodeId::from(record.task_id))
            .map_err(|error| error.message)
    }

    /// Finds one persisted TaskSession metadata record by direct parent and public task id.
    /// 按直接父级与公开任务标识查找一条持久化 TaskSession 元数据。
    pub(super) fn persisted_task_metadata_for_public_id(
        &self,
        root_session_id: SessionId,
        parent: ParentSessionRef,
        public_task_id: &str,
    ) -> Result<Option<TaskSessionMetadata>, String> {
        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Ok(None);
        };
        store
            .find_task_session_by_parent_and_public_id(parent, public_task_id)
            .map_err(|error| error.message)
    }

    /// Loads the persisted current context for one segmented TaskSession.
    /// 加载某个分段 TaskSession 的持久化当前上下文。
    pub(super) fn load_persisted_task_context(
        &self,
        root_session_id: SessionNodeId,
        task_session_id: SessionNodeId,
    ) -> Result<Option<SessionContextDocument>, String> {
        let root_session_id = Uuid::from(root_session_id);
        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Ok(None);
        };
        store
            .load_context(task_session_id)
            .map_err(|error| error.message)
    }

    /// Resolves one TaskSession configuration for a new child execution.
    /// 为一次新的子代理执行解析 TaskSession 配置。
    pub(super) fn resolve_task_session_config_for_execution(
        &self,
        parent_controller_config: &CoreControllerConfig,
        invocation_config: &TaskInvocationConfig,
    ) -> Result<TaskSessionResolvedConfig, String> {
        let parent_config = Some(core_controller_config_to_effective_config(
            parent_controller_config,
        ));
        let (profile, invocation) =
            self.resolve_catalog_profile_for_invocation(invocation_config)?;
        Ok(TaskSessionConfigResolver::resolve(
            TaskSessionConfigResolutionInput {
                global_default_config: SessionEffectiveConfig::default(),
                parent_config,
                profile,
                invocation,
            },
        ))
    }

    /// Resolves the persisted TaskSession configuration snapshot for one runtime record.
    /// 为一条运行时记录解析要持久化的 TaskSession 配置快照。
    fn resolve_task_session_config_for_record(
        &self,
        record: &TaskSessionRecord,
        controller_config: Option<&CoreControllerConfig>,
    ) -> TaskSessionResolvedConfig {
        let parent_config = controller_config.map(core_controller_config_to_effective_config);
        let (profile, invocation) = self
            .resolve_catalog_profile_for_invocation(&record.invocation_config)
            .unwrap_or_else(|_| (None, record.invocation_config.clone()));
        TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config,
            profile,
            invocation,
        })
    }

    /// Resolves the catalog profile and normalized invocation for one task call.
    /// 为单次任务调用解析目录配置与规范化调用配置。
    fn resolve_catalog_profile_for_invocation(
        &self,
        invocation_config: &TaskInvocationConfig,
    ) -> Result<
        (
            Option<vulcan_task_session::SubagentProfile>,
            TaskInvocationConfig,
        ),
        String,
    > {
        let mut invocation = invocation_config.clone();
        if let Some(agent_id) = invocation.subagent_profile_id.as_deref() {
            let definition = self.enabled_subagent_definition(agent_id)?;
            if invocation.subagent_mode.is_none() {
                invocation.subagent_mode = Some(definition.mode.as_str().to_string());
            }
            return Ok((Some(definition.to_profile()), invocation));
        }
        if invocation.subagent_mode.is_none() {
            invocation.subagent_mode = Some(SUBAGENT_MODE_DEFAULT.to_string());
        }
        Ok((None, invocation))
    }
}

impl CloseAgentArchivePersistenceCommit {
    /// Restores this commit's exact metadata snapshots through the store that created it.
    /// 通过创建本提交的存储恢复其精确元数据快照。
    ///
    /// # Returns
    /// 返回值
    /// Proven rollback outcome, including aggregated errors when restoration is incomplete.
    /// 已证实的回滚结果，恢复不完整时包含聚合错误。
    pub(super) fn rollback(self) -> CloseAgentArchiveRollbackOutcome {
        let Some(store) = self.store else {
            return CloseAgentArchiveRollbackOutcome::NotRequired;
        };
        if self.rollback_metadata_snapshots.is_empty() {
            return CloseAgentArchiveRollbackOutcome::NotRequired;
        }
        match restore_task_session_metadata_snapshots(&store, &self.rollback_metadata_snapshots) {
            Ok(()) => CloseAgentArchiveRollbackOutcome::Completed,
            Err(error) => CloseAgentArchiveRollbackOutcome::Incomplete { error },
        }
    }
}

/// Restores every supplied task metadata snapshot and reports all failed restores.
/// 恢复全部给定任务元数据快照，并报告所有恢复失败。
///
/// # Parameters
/// 参数
/// - `store`: Task-session store that owns the persisted projections.
/// - `store`：拥有持久化投影的任务会话存储。
/// - `metadata_snapshots`: Exact snapshots to restore in reverse write order.
/// - `metadata_snapshots`：按写入逆序恢复的精确快照。
///
/// # Returns
/// 返回值
/// Success when every snapshot is restored, otherwise all restore errors.
/// 全部快照恢复时返回成功，否则返回全部恢复错误。
fn restore_task_session_metadata_snapshots(
    store: &SegmentedTaskSessionStore,
    metadata_snapshots: &[TaskSessionMetadata],
) -> Result<(), String> {
    let mut restore_errors = Vec::new();
    for metadata in metadata_snapshots.iter().rev() {
        if let Err(error) = store.save_task_session(metadata.clone()) {
            restore_errors.push(format!(
                "task_session_id={}: {}",
                metadata.task_session_id, error.message
            ));
        }
    }
    if restore_errors.is_empty() {
        Ok(())
    } else {
        Err(restore_errors.join("; "))
    }
}

/// Loads, validates, and orders every open persisted subagent reachable from one root.
/// 加载、校验并排序从单个根可达的全部开放持久化子代理。
///
/// # Parameters
/// 参数
/// `store` owns the segmented task metadata for the requested root.
/// `store` 拥有所请求根的分段任务元数据。
/// `root_session_id` is the only main-session root accepted by this restore transaction.
/// `root_session_id` 是本次恢复事务唯一接受的主会话根。
///
/// # Returns
/// 返回值
/// Root-first open metadata for one unambiguous tree, or a deterministic read/integrity error.
/// 单一无歧义树的根优先开放元数据，或确定性的读取或完整性错误。
fn load_validated_open_subagent_metadata(
    store: &SegmentedTaskSessionStore,
    root_session_id: SessionNodeId,
) -> Result<Vec<TaskSessionMetadata>, String> {
    let mut metadata_by_parent = store
        .load_task_session_metadata_by_parent()
        .map_err(|error| error.message)?;
    validate_persisted_subagent_metadata_forest(&metadata_by_parent, root_session_id)?;
    let expected_restore_count = metadata_by_parent
        .values()
        .flat_map(|metadata| metadata.iter())
        .filter(|metadata| metadata.status != TaskSessionStatus::Archived)
        .count();
    let mut visited_parent_ids = BTreeSet::new();
    let mut parents = vec![ParentSessionRef::MainSession { root_session_id }];
    let mut metadata_to_restore = Vec::with_capacity(expected_restore_count);
    while let Some(parent) = parents.pop() {
        let parent_node_id = subagent_parent_ref_node_id(&parent);
        if !visited_parent_ids.insert(parent_node_id) {
            continue;
        }
        let Some(metadata_records) = metadata_by_parent.remove(&parent) else {
            continue;
        };
        for metadata in metadata_records {
            if metadata.status == TaskSessionStatus::Archived {
                continue;
            }
            parents.push(ParentSessionRef::TaskSession {
                root_session_id,
                parent_task_session_id: metadata.task_session_id,
            });
            metadata_to_restore.push(metadata);
        }
    }
    if metadata_to_restore.len() != expected_restore_count {
        return Err(format!(
            "Persisted subagent metadata contains {} non-archived entries outside the root `{root_session_id}` tree.",
            expected_restore_count.saturating_sub(metadata_to_restore.len())
        ));
    }
    Ok(metadata_to_restore)
}

/// Validates persisted open-agent identity and ancestry before any in-memory restore mutation.
/// 在任何内存恢复变更前校验持久化开放代理的身份与祖先关系。
///
/// # Parameters
/// 参数
/// `metadata_by_parent` contains every persisted task grouped by its encoded direct parent.
/// `metadata_by_parent` 包含按编码直接父级分组的全部持久化任务。
/// `root_session_id` is the only main-session root accepted by this restore transaction.
/// `root_session_id` 是本次恢复事务唯一接受的主会话根。
///
/// # Returns
/// 返回值
/// Success for one unambiguous rooted forest, or a deterministic integrity error.
/// 对单一无歧义根森林返回成功，否则返回确定性完整性错误。
fn validate_persisted_subagent_metadata_forest(
    metadata_by_parent: &BTreeMap<ParentSessionRef, Vec<TaskSessionMetadata>>,
    root_session_id: SessionNodeId,
) -> Result<(), String> {
    let mut metadata_by_session = BTreeMap::new();
    for metadata in metadata_by_parent
        .values()
        .flat_map(|metadata| metadata.iter())
    {
        if metadata.root_session_id != root_session_id {
            return Err(format!(
                "Persisted subagent session `{}` belongs to root `{}` instead of `{root_session_id}`.",
                metadata.task_session_id, metadata.root_session_id
            ));
        }
        if metadata_by_session
            .insert(metadata.task_session_id, metadata)
            .is_some()
        {
            return Err(format!(
                "Persisted subagent session `{}` appears more than once.",
                metadata.task_session_id
            ));
        }
    }
    let mut open_slots = BTreeMap::new();
    for metadata in metadata_by_session
        .values()
        .copied()
        .filter(|metadata| metadata.status != TaskSessionStatus::Archived)
    {
        match &metadata.parent {
            ParentSessionRef::MainSession {
                root_session_id: parent_root_session_id,
            } if *parent_root_session_id == root_session_id => {}
            ParentSessionRef::TaskSession {
                root_session_id: parent_root_session_id,
                parent_task_session_id,
            } if *parent_root_session_id == root_session_id => {
                let parent = metadata_by_session.get(parent_task_session_id).ok_or_else(|| {
                    format!(
                        "Persisted open subagent `{}` references missing parent session `{parent_task_session_id}`.",
                        metadata.task_session_id
                    )
                })?;
                if parent.status == TaskSessionStatus::Archived {
                    return Err(format!(
                        "Persisted open subagent `{}` belongs to archived parent session `{parent_task_session_id}`.",
                        metadata.task_session_id
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "Persisted open subagent `{}` has an invalid parent outside root `{root_session_id}`.",
                    metadata.task_session_id
                ));
            }
        }
        let slot = (metadata.parent.clone(), metadata.public_task_id.clone());
        if let Some(existing_session_id) = open_slots.insert(slot, metadata.task_session_id) {
            return Err(format!(
                "Persisted parent-scoped agent `{}` is open in both sessions `{existing_session_id}` and `{}`.",
                metadata.public_task_id, metadata.task_session_id
            ));
        }
    }
    Ok(())
}

/// Builds the identity and lifecycle projection used for registry restore preflight.
/// 构建注册表恢复预检使用的身份与生命周期投影。
///
/// # Parameters
/// 参数
/// `metadata` is one persisted task-session record.
/// `metadata` 是一条持久化任务会话记录。
///
/// # Returns
/// 返回值
/// A registry entry containing only authoritative identity and lifecycle facts.
/// 仅包含权威身份与生命周期事实的注册表条目。
fn subagent_registry_entry_identity_from_metadata(
    metadata: &TaskSessionMetadata,
) -> SubagentRegistryEntry {
    let mut entry = SubagentRegistryEntry::new(
        metadata.public_task_id.clone(),
        metadata.task_session_id,
        subagent_parent_session_id_from_metadata(metadata),
        subagent_kind_for_task_metadata(metadata),
        metadata.created_at,
    );
    entry.status = subagent_restore_status_from_metadata(metadata.status);
    entry.stop_reason = if metadata.status.is_active() {
        Some(String::from("runtime_lost"))
    } else {
        metadata.stop_reason.clone()
    };
    entry.updated_at_ms = metadata.updated_at;
    entry
}

/// Returns the node id represented by one persisted parent reference.
/// 返回一条持久化父级引用所代表的节点标识。
fn subagent_parent_ref_node_id(parent: &ParentSessionRef) -> SessionNodeId {
    match parent {
        ParentSessionRef::None => SessionNodeId::from(Uuid::nil()),
        ParentSessionRef::MainSession { root_session_id } => *root_session_id,
        ParentSessionRef::TaskSession {
            parent_task_session_id,
            ..
        } => *parent_task_session_id,
    }
}

/// Returns the direct parent session id encoded by persisted TaskSession metadata.
/// 返回持久化 TaskSession 元数据编码的直接父级会话标识。
fn subagent_parent_session_id_from_metadata(metadata: &TaskSessionMetadata) -> SessionNodeId {
    match metadata.parent {
        ParentSessionRef::MainSession { root_session_id } => root_session_id,
        ParentSessionRef::TaskSession {
            parent_task_session_id,
            ..
        } => parent_task_session_id,
        ParentSessionRef::None => metadata.root_session_id,
    }
}

/// Derives the subagent kind from persisted TaskSession configuration fields.
/// 根据持久化 TaskSession 配置字段派生子代理类型。
fn subagent_kind_for_task_metadata(metadata: &TaskSessionMetadata) -> SubagentKind {
    subagent_kind_for_task_config(
        metadata.subagent_profile_id.as_deref(),
        metadata.requested_config.system_prompt.as_deref(),
    )
}

/// Converts persisted status into a safe restored registry status without resuming execution.
/// 将持久化状态转换为不会恢复执行的安全注册表状态。
fn subagent_restore_status_from_metadata(status: TaskSessionStatus) -> SubagentStatus {
    if status.is_active() {
        return SubagentStatus::Stopped;
    }
    subagent_status_from_task_session_status(status)
}

/// Returns the compact registry result summary for a persisted task-session record.
/// 返回已持久化任务会话记录用于注册表投影的紧凑结果摘要。
fn task_record_registry_result_summary(
    record: &TaskSessionRecord,
    status: TaskSessionStatus,
) -> Option<&str> {
    (!status.is_active() && !record.last_result_text.trim().is_empty())
        .then_some(record.last_result_text.as_str())
}

/// Reads the latest task message from a persisted task event payload.
/// 从持久化任务事件载荷读取最近任务消息。
fn subagent_last_task_message_from_event_payload(payload: &Value) -> Option<String> {
    payload
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Merges persisted display fields without replacing live runtime state.
/// 合并持久化展示字段，但不替换实时运行状态。
fn merge_persisted_subagent_entry_without_status_overwrite(
    existing: &mut SubagentRegistryEntry,
    persisted: SubagentRegistryEntry,
) {
    if existing.prompt_source.is_none() {
        existing.prompt_source = persisted.prompt_source;
    }
    if existing.last_task_message.is_none() {
        existing.last_task_message = persisted.last_task_message;
    }
    if existing.result_summary.is_none() {
        existing.result_summary = persisted.result_summary;
    }
    if existing.segments.is_empty() {
        existing.segments = persisted.segments;
    }
}

/// Keeps inherited provider context while trimming task display records to the delegated prompt.
/// 保留继承的 Provider 上下文，同时将任务显示记录裁剪到被委派的提示词之后。
///
/// # Parameters
/// 参数
/// `conversation_state` is the task conversation state produced from the controller snapshot.
/// `conversation_state` 是从控制器快照转换出的任务会话状态。
/// `prompt` is the child task prompt that opens the task-owned display transcript.
/// `prompt` 是开启任务自有显示转录的子任务提示词。
fn scope_task_context_display_records_to_prompt(
    conversation_state: &mut SessionConversationState,
    prompt: &str,
) {
    let Some(start_index) =
        task_context_display_record_start_index(conversation_state.records.as_slice(), prompt)
    else {
        return;
    };
    if start_index > 0 {
        conversation_state.records = conversation_state.records.split_off(start_index);
    }
}

/// Resolves the first task-owned display record for one persisted child transcript.
/// 解析单份持久化子转录中的第一条任务自有显示记录。
///
/// # Parameters
/// 参数
/// `records` are display records converted from the child controller snapshot.
/// `records` 是从子控制器快照转换出的显示记录。
/// `prompt` is the exact delegated task prompt.
/// `prompt` 是精确的委派任务提示词。
///
/// # Returns
/// 返回值
/// The latest visible user record matching the prompt, or the latest visible user record as a fallback.
/// 返回与提示词匹配的最新可见用户记录；无法匹配时回退到最新可见用户记录。
fn task_context_display_record_start_index(
    records: &[SessionConversationRecord],
    prompt: &str,
) -> Option<usize> {
    let normalized_prompt = normalized_task_context_prompt(prompt)?;
    records
        .iter()
        .rposition(|record| {
            task_context_display_record_matches_prompt(record, normalized_prompt.as_str())
        })
        .or_else(|| {
            records
                .iter()
                .rposition(task_context_record_starts_visible_user_turn)
        })
}

/// Returns whether one display record is the task prompt anchor.
/// 判断单条显示记录是否为任务提示词锚点。
///
/// # Parameters
/// 参数
/// `record` is the candidate display record.
/// `record` 是候选显示记录。
/// `normalized_prompt` is the trimmed task prompt text.
/// `normalized_prompt` 是裁剪后的任务提示词文本。
///
/// # Returns
/// 返回值
/// `true` when the visible user record carries the same canonical prompt.
/// 当可见用户记录携带相同规范提示词时返回 `true`。
fn task_context_display_record_matches_prompt(
    record: &SessionConversationRecord,
    normalized_prompt: &str,
) -> bool {
    task_context_record_starts_visible_user_turn(record)
        && task_context_record_user_text(record)
            .and_then(normalized_task_context_prompt)
            .as_deref()
            == Some(normalized_prompt)
}

/// Returns whether one display record opens a user-authored task turn.
/// 判断单条显示记录是否开启用户编写的任务轮次。
///
/// # Parameters
/// 参数
/// `record` is the display record to inspect.
/// `record` 是需要检查的显示记录。
///
/// # Returns
/// 返回值
/// `true` when the record is a visible user transcript row.
/// 当记录是可见用户转录行时返回 `true`。
fn task_context_record_starts_visible_user_turn(record: &SessionConversationRecord) -> bool {
    if record.kind != SessionMessageKind::User
        || record.record_kind != SessionMessageRecordKind::Transcript
        || !record.visible
    {
        return false;
    }
    record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("starts_user_turn"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Reads the canonical user text from a task display record.
/// 从任务显示记录中读取规范用户文本。
///
/// # Parameters
/// 参数
/// `record` is the visible user transcript record.
/// `record` 是可见用户转录记录。
///
/// # Returns
/// 返回值
/// The canonical text when metadata or body provides one.
/// 当元数据或正文提供文本时返回规范文本。
fn task_context_record_user_text(record: &SessionConversationRecord) -> Option<&str> {
    record
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("canonical_user_text")
                .or_else(|| metadata.get("canonical_text"))
        })
        .and_then(Value::as_str)
        .or(Some(record.body.as_str()))
}

/// Normalizes one task prompt for exact display-boundary matching.
/// 归一化单条任务提示词，用于精确匹配显示边界。
///
/// # Parameters
/// 参数
/// `value` is the prompt-like text.
/// `value` 是类似提示词的文本。
///
/// # Returns
/// 返回值
/// A trimmed non-empty prompt string.
/// 返回裁剪后的非空提示词字符串。
fn normalized_task_context_prompt(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Reads the versioned Provider call ledger from one TaskSession runtime checkpoint.
/// 从一个 TaskSession 运行时检查点读取带版本的 Provider 调用账本。
///
/// # Parameters
/// 参数
/// `checkpoint` is the exact persisted runtime checkpoint for the child task.
/// `checkpoint` 是子任务对应的精确持久化运行时检查点。
///
/// # Returns
/// 返回值
/// Returns no ledger for legacy checkpoints and rejects malformed new ledger payloads.
/// 旧检查点返回无账本，新账本载荷损坏时显式拒绝。
fn task_provider_call_ledger_from_runtime_checkpoint(
    checkpoint: &SessionRuntimeCheckpoint,
) -> Result<Option<SessionProviderCallLedger>, String> {
    let Some(provider_call_ledger) = checkpoint.payload.get("provider_call_ledger") else {
        return Ok(None);
    };
    serde_json::from_value(provider_call_ledger.clone())
        .map(Some)
        .map_err(|error| {
            format!(
                "TaskSession `{}` Provider call ledger is invalid: {error}",
                checkpoint.node_id
            )
        })
}

/// Appends terminal turn statistics to the persisted child-agent context document.
/// 向持久化的子代理上下文文档追加终态轮次统计。
///
/// # Parameters
/// 参数
/// `conversation_state` is the child-agent conversation state being persisted.
/// `conversation_state` 是正在持久化的子代理会话状态。
/// `task_id` identifies the child task session that owns the generated records.
/// `task_id` 标识拥有生成记录的子任务会话。
/// `now_ms` is the persistence timestamp shared with the task state event.
/// `now_ms` 是与任务状态事件共用的持久化时间戳。
/// `status` is the terminal task status being saved.
/// `status` 是正在保存的终态任务状态。
/// `final_output_text` is the completed output or explicit terminal error text.
/// `final_output_text` 是完成输出或显式终态错误文本。
/// `execution_id` is the exact child execution used for idempotent materialization.
/// `execution_id` 是用于幂等物化的精确子执行标识。
/// `outcome` carries legacy aggregate counters when a complete outcome exists.
/// `outcome` 在完整结果存在时携带旧版聚合计数。
/// `provider_call_ledger` is the preferred physical-call authority for new sessions.
/// `provider_call_ledger` 是新会话首选的物理调用权威。
/// `initial_execution_started_at_ms` bounds first-run duration to task creation.
/// `initial_execution_started_at_ms` 将首次执行耗时限制在任务创建之后。
fn append_task_context_turn_stats_record(
    conversation_state: &mut SessionConversationState,
    task_id: SessionId,
    now_ms: u64,
    status: TaskSessionStatus,
    final_output_text: &str,
    execution_id: &str,
    outcome: Option<&TaskExecutionOutcome>,
    provider_call_ledger: Option<&mut SessionProviderCallLedger>,
    initial_execution_started_at_ms: Option<u64>,
) -> Result<(), String> {
    if conversation_state.records.iter().any(|record| {
        record.record_kind == SessionMessageRecordKind::TurnStats
            && record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("task_execution_id"))
                .and_then(Value::as_str)
                == Some(execution_id)
    }) || outcome
        .and_then(|outcome| outcome.final_assistant_message_id.as_deref())
        .is_some_and(|final_message_id| {
            conversation_state.records.iter().any(|record| {
                record.record_kind == SessionMessageRecordKind::TurnStats
                    && record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("task_final_assistant_message_id"))
                        .and_then(Value::as_str)
                        == Some(final_message_id)
            })
        })
    {
        return Ok(());
    }
    let now = now_ms.to_string();
    let summary_record_id =
        build_session_record_message_id(task_id, now.as_str(), conversation_state.records.len());
    append_record(
        conversation_state,
        SessionConversationRecord {
            id: summary_record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::SummaryOutput,
            visible: false,
            title: String::from("Summary"),
            body: final_output_text.to_string(),
            metadata: Some(json!({ "preview": final_output_text })),
            created_at: now.clone(),
        },
    )
    .map_err(|error| format!("Invalid task summary record identity: {error}"))?;
    let status = match status {
        TaskSessionStatus::Failed => SessionTurnStatsStatus::Failed,
        TaskSessionStatus::Stopped => SessionTurnStatsStatus::Stopped,
        _ => SessionTurnStatsStatus::Completed,
    };
    let stats_conversation_state = task_context_conversation_state_for_turn_stats(
        conversation_state,
        summary_record_id.as_str(),
    );
    let turn_stats = if let Some(provider_call_ledger) = provider_call_ledger {
        // The task terminal record must own the exact calls used by its rebuildable Turn Stats node.
        // 子任务终态记录必须绑定其可重建 Turn Stats 节点实际使用的精确调用。
        let _ = provider_call_ledger.assign_closing_record_id_to_latest_execution_for_owner(
            summary_record_id.clone(),
            Some("subagent"),
        );
        build_session_turn_stats_from_state_with_ledger_owner_and_segments(
            &stats_conversation_state,
            &*provider_call_ledger,
            summary_record_id.as_str(),
            status,
            SessionTurnStatsKind::UserTurn,
            "subagent",
            Vec::new(),
        )
    } else {
        let record_turn_stats = build_session_turn_stats_from_state_with_segments(
            &stats_conversation_state,
            summary_record_id.as_str(),
            status,
            outcome.map(|outcome| outcome.provider_turns_executed),
            SessionTurnStatsKind::UserTurn,
            Vec::new(),
        );
        let task_context_usage_segments = outcome
            .map(task_context_usage_segments_from_outcome)
            .unwrap_or_default();
        if record_turn_stats
            .as_ref()
            .is_some_and(task_turn_stats_has_token_usage)
            || task_context_usage_segments.is_empty()
        {
            record_turn_stats
        } else if let Some(record_turn_stats) = record_turn_stats {
            Some(merge_task_usage_segments_into_record_stats(
                record_turn_stats,
                task_context_usage_segments,
            ))
        } else {
            build_session_turn_stats_from_state_with_segments(
                &stats_conversation_state,
                summary_record_id.as_str(),
                status,
                None,
                SessionTurnStatsKind::UserTurn,
                task_context_usage_segments,
            )
        }
    };
    let Some(mut turn_stats) = turn_stats else {
        return Ok(());
    };
    clamp_task_turn_stats_duration_to_initial_execution_start(
        &mut turn_stats,
        now_ms,
        initial_execution_started_at_ms,
    );
    let stats_record_id =
        build_session_record_message_id(task_id, now.as_str(), conversation_state.records.len());
    append_record(
        conversation_state,
        SessionConversationRecord {
            id: stats_record_id,
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: format!("Turn Stats · {}", turn_stats.status.label()),
            body: turn_stats.detail_body(),
            metadata: Some(json!({
                "turn_stats": turn_stats,
                "turn_closing_record_id": summary_record_id,
                "task_execution_id": execution_id,
                "task_final_assistant_message_id": outcome
                    .and_then(|outcome| outcome.final_assistant_message_id.as_deref()),
            })),
            created_at: now,
        },
    )
    .map_err(|error| format!("Invalid task statistics record identity: {error}"))?;
    Ok(())
}

/// Clamps first-run child-agent duration so inherited display timestamps cannot extend it before task creation.
/// 钳制首次运行子智能体耗时，避免继承显示时间戳把耗时扩展到任务创建之前。
///
/// # Parameters
/// 参数
/// `stats` is the child-agent turn-statistics snapshot being finalized.
/// `stats` 是正在收尾的子智能体轮次统计快照。
/// `finished_at_ms` is the timestamp of the terminal statistics record.
/// `finished_at_ms` 是终态统计记录的时间戳。
/// `initial_execution_started_at_ms` is the task creation timestamp for non-resumed executions.
/// `initial_execution_started_at_ms` 是非续跑执行的任务创建时间戳。
fn clamp_task_turn_stats_duration_to_initial_execution_start(
    stats: &mut SessionTurnStats,
    finished_at_ms: u64,
    initial_execution_started_at_ms: Option<u64>,
) {
    let Some(initial_execution_started_at_ms) = initial_execution_started_at_ms else {
        refresh_task_turn_stats_output_speed(stats);
        return;
    };
    let Some(duration_ms) = stats.duration_ms else {
        refresh_task_turn_stats_output_speed(stats);
        return;
    };
    if finished_at_ms < initial_execution_started_at_ms {
        refresh_task_turn_stats_output_speed(stats);
        return;
    }
    let max_duration_ms = finished_at_ms.saturating_sub(initial_execution_started_at_ms);
    if duration_ms > max_duration_ms {
        stats.duration_ms = Some(max_duration_ms);
    }
    refresh_task_turn_stats_output_speed(stats);
}

/// Refreshes output speed after task-local duration or explicit usage changes.
/// 在任务本地耗时或显式用量变更后刷新输出速度。
///
/// # Parameters
/// 参数
/// `stats` is the turn-statistics snapshot whose derived speed is refreshed.
/// `stats` 是需要刷新派生速度的轮次统计快照。
fn refresh_task_turn_stats_output_speed(stats: &mut SessionTurnStats) {
    stats.output_tokens_per_second = match (stats.duration_ms, stats.output_tokens) {
        (Some(duration_ms), output_tokens) if duration_ms > 0 && output_tokens > 0 => {
            Some(output_tokens as f64 / (duration_ms as f64 / 1000.0))
        }
        _ => None,
    };
}

/// Builds a statistics-only view scoped to the current child-agent execution.
/// 构建仅用于统计的当前子智能体执行窗口视图。
///
/// # Parameters
/// 参数
/// `conversation_state` is the persisted child-agent conversation state.
/// `conversation_state` 是持久化的子智能体会话状态。
/// `closing_record_id` is the summary record that closes the current execution.
/// `closing_record_id` 是闭合当前执行的摘要记录。
///
/// # Returns
/// 返回值
/// A cloned state whose records start at the current execution trigger when one is identifiable.
/// 返回记录从可识别当前执行触发点开始的克隆状态。
fn task_context_conversation_state_for_turn_stats(
    conversation_state: &SessionConversationState,
    closing_record_id: &str,
) -> SessionConversationState {
    let Some(start_index) =
        task_context_current_execution_record_start_index(conversation_state, closing_record_id)
    else {
        return conversation_state.clone();
    };
    if start_index == 0 {
        return conversation_state.clone();
    }
    let mut scoped_state = conversation_state.clone();
    scoped_state.records = scoped_state.records.split_off(start_index);
    scoped_state
}

/// Resolves the display record that starts the current child-agent execution for statistics.
/// 解析用于统计的当前子智能体执行起始显示记录。
///
/// # Parameters
/// 参数
/// `conversation_state` contains task-owned display records plus the newly appended closing record.
/// `conversation_state` 包含任务自有显示记录以及刚追加的闭合记录。
/// `closing_record_id` identifies the current summary record.
/// `closing_record_id` 标识当前摘要记录。
///
/// # Returns
/// 返回值
/// The index of the first parent-to-child delivery note in the current execution batch.
/// 返回当前执行批次中第一条父到子投递记录的索引。
fn task_context_current_execution_record_start_index(
    conversation_state: &SessionConversationState,
    closing_record_id: &str,
) -> Option<usize> {
    let closing_index = conversation_state
        .records
        .iter()
        .position(|record| record.id == closing_record_id)?;
    let latest_delivery_index = conversation_state.records[..closing_index]
        .iter()
        .rposition(task_context_record_is_inter_agent_delivery_note)?;
    let search_start = conversation_state.records[..latest_delivery_index]
        .iter()
        .rposition(task_context_record_closes_delivery_stats_window)
        .map(|index| index + 1)
        .unwrap_or(0);
    conversation_state.records[search_start..=latest_delivery_index]
        .iter()
        .position(task_context_record_is_inter_agent_delivery_note)
        .map(|offset| search_start + offset)
}

/// Returns whether one record is a parent-to-child message delivery note.
/// 判断单条记录是否为父会话投递给子智能体的消息记录。
fn task_context_record_is_inter_agent_delivery_note(record: &SessionConversationRecord) -> bool {
    record.kind == SessionMessageKind::User
        && session_conversation_record_is_subagent_send_message_display_note(record)
}

/// Returns whether one record closes the previous child execution before a delivery note.
/// 判断单条记录是否在投递记录之前闭合上一段子智能体执行。
fn task_context_record_closes_delivery_stats_window(record: &SessionConversationRecord) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::AssistantOutput
            | SessionMessageRecordKind::SummaryOutput
            | SessionMessageRecordKind::TurnStats
            | SessionMessageRecordKind::TurnTerminated
    )
}

/// Returns whether one turn-statistics snapshot contains concrete token usage.
/// 返回单个轮次统计快照是否包含明确的 Token 用量。
///
/// # Parameters
/// 参数
/// `stats` is the turn-statistics snapshot being inspected.
/// `stats` 是正在检查的轮次统计快照。
///
/// # Returns
/// 返回值
/// `true` when any aggregated token field is non-zero.
/// 当任意聚合 Token 字段非零时返回 `true`。
fn task_turn_stats_has_token_usage(stats: &SessionTurnStats) -> bool {
    stats.input_tokens > 0
        || stats.output_tokens > 0
        || stats.reasoning_tokens > 0
        || stats.total_tokens > 0
        || stats.cached_input_tokens.is_some_and(|value| value > 0)
        || stats
            .cache_creation_input_tokens
            .is_some_and(|value| value > 0)
}

/// Converts parent-facing task usage segments into task-local turn-stat segments.
/// 将面向父会话的任务用量分段转换为子任务本地轮次统计分段。
///
/// # Parameters
/// 参数
/// `outcome` is the persisted task execution outcome containing parent-facing usage.
/// `outcome` 是包含面向父会话用量的持久化任务执行结果。
///
/// # Returns
/// 返回值
/// Task-local usage segments where child-agent work is represented as normal assistant work.
/// 返回子任务本地用量分段，其中智能体工作会表示为普通助手工作。
fn task_context_usage_segments_from_outcome(
    outcome: &TaskExecutionOutcome,
) -> Vec<SessionTurnUsageSegment> {
    outcome
        .usage_segments
        .iter()
        .cloned()
        .map(|mut segment| {
            if segment.kind == SessionTurnUsageSegmentKind::SubagentTask {
                segment.kind = SessionTurnUsageSegmentKind::AssistantResponse;
            }
            segment
        })
        .collect()
}

/// Merges explicit task usage into record-derived stats while preserving record-derived call counts.
/// 将显式任务用量合并进记录推导统计，同时保留记录推导出的调用次数。
///
/// # Parameters
/// 参数
/// `stats` is the record-derived statistics snapshot that may lack token counts.
/// `stats` 是可能缺少 Token 数的记录推导统计快照。
/// `usage_segments` are explicit usage segments reported by the completed child execution.
/// `usage_segments` 是子代理执行完成后报告的显式用量分段。
///
/// # Returns
/// 返回值
/// A merged statistics snapshot that fills token fields without inflating call counts.
/// 返回补齐 Token 字段且不会放大调用次数的合并统计快照。
fn merge_task_usage_segments_into_record_stats(
    mut stats: SessionTurnStats,
    mut usage_segments: Vec<SessionTurnUsageSegment>,
) -> SessionTurnStats {
    let record_has_provider_calls = stats.provider_call_count > 0;
    let record_has_tool_calls = stats.tool_call_count > 0;
    for segment in &mut usage_segments {
        if record_has_provider_calls {
            segment.provider_call_count = 0;
        }
        if record_has_tool_calls {
            segment.tool_call_count = 0;
        }
        merge_usage_segment_into_turn_stats(&mut stats, segment);
    }
    stats.usage_segments.extend(usage_segments);
    stats
}
