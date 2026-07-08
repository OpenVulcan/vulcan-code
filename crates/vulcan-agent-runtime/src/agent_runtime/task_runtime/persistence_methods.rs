use super::*;

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
        let mut metadata_by_parent = store
            .load_task_session_metadata_by_parent()
            .map_err(|error| error.message)?;
        let mut visited_parent_ids = BTreeSet::new();
        let mut parents = vec![subagent_restore_start_parent_ref(
            root_session_id,
            SessionNodeId::from(current_session_id),
        )];
        while let Some(parent) = parents.pop() {
            let parent_node_id = subagent_parent_ref_node_id(&parent);
            if !visited_parent_ids.insert(parent_node_id) {
                continue;
            }
            let Some(metadata_records) = metadata_by_parent.remove(&parent) else {
                continue;
            };
            for metadata in metadata_records {
                parents.push(ParentSessionRef::TaskSession {
                    root_session_id,
                    parent_task_session_id: metadata.task_session_id,
                });
                if metadata.status == TaskSessionStatus::Archived {
                    continue;
                }
                self.upsert_subagent_registry_entry_from_metadata(&metadata)?;
            }
        }
        self.restored_task_session_registry_roots
            .write()
            .map_err(|_| String::from("The restored task-session registry marker is locked."))?
            .insert(root_session_uuid);
        Ok(())
    }

    /// Upserts one subagent registry entry directly from persisted TaskSession metadata.
    /// 直接根据持久化 TaskSession 元数据更新或插入一条子代理注册表条目。
    fn upsert_subagent_registry_entry_from_metadata(
        &self,
        metadata: &TaskSessionMetadata,
    ) -> Result<(), String> {
        let restored_status = subagent_restore_status_from_metadata(metadata.status);
        let store = self.task_session_store_for_root(Uuid::from(metadata.root_session_id))?;
        let mut entry = SubagentRegistryEntry::new(
            metadata.public_task_id.clone(),
            metadata.task_session_id,
            subagent_parent_session_id_from_metadata(metadata),
            subagent_kind_for_task_metadata(metadata),
            metadata.created_at,
        );
        entry.status = restored_status;
        entry.stop_reason = if metadata.status.is_active() {
            Some(String::from("runtime_lost"))
        } else {
            metadata.stop_reason.clone()
        };
        entry.updated_at_ms = metadata.updated_at;
        entry.prompt_source = Some(subagent_prompt_source_for_task_config(
            metadata.subagent_profile_id.as_deref(),
        ));
        entry.last_task_message = store
            .as_ref()
            .and_then(|store| {
                store
                    .load_latest_event_payload(metadata.task_session_id)
                    .ok()
                    .flatten()
            })
            .and_then(|payload| subagent_last_task_message_from_event_payload(&payload));
        entry.result_summary = metadata.last_result_summary.clone();
        entry.segments = store
            .as_ref()
            .map(|store| store.load_task_segment_summaries(metadata.task_session_id))
            .transpose()
            .map_err(|error| error.message)?
            .unwrap_or_default();
        let restored_prompt = entry
            .last_task_message
            .clone()
            .or_else(|| metadata.last_result_summary.clone())
            .unwrap_or_else(|| metadata.display_title.clone());
        let restored_context = store
            .as_ref()
            .map(|store| store.load_context(metadata.task_session_id))
            .transpose()
            .map_err(|error| error.message)?
            .flatten();
        self.restore_task_session_record_from_metadata(
            metadata,
            restored_context.as_ref(),
            restored_prompt,
        )?;
        let inserted_from_persistence = {
            let mut registry = self.subagent_registry.write().map_err(|_| {
                String::from("The subagent registry is locked and cannot be restored.")
            })?;
            if let Some(existing) = registry.get_mut(&entry.agent_id) {
                merge_persisted_subagent_entry_without_status_overwrite(existing, entry);
                false
            } else {
                registry.upsert(entry);
                true
            }
        };
        if inserted_from_persistence && metadata.status.is_active() {
            self.persist_restored_stopped_task_session_metadata(metadata)?;
        }
        Ok(())
    }

    /// Restores a lightweight in-memory TaskSession record for one open persisted subagent.
    /// 为一个未关闭的持久化子代理恢复轻量内存 TaskSession 记录。
    fn restore_task_session_record_from_metadata(
        &self,
        metadata: &TaskSessionMetadata,
        context: Option<&SessionContextDocument>,
        prompt: String,
    ) -> Result<(), String> {
        if metadata.status == TaskSessionStatus::Archived {
            return Ok(());
        }
        let task_id = Uuid::from(metadata.task_session_id);
        let mut record_metadata = metadata.clone();
        if record_metadata.status.is_active() {
            record_metadata.status = TaskSessionStatus::Stopped;
            record_metadata.stop_reason = Some(String::from("runtime_lost"));
        }
        let record = task_session_record_from_persisted_metadata(
            &record_metadata,
            context,
            prompt.clone(),
            TaskInvocationConfig::main(prompt),
            None,
            None,
        )?;
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task-session registry is locked and cannot restore subagent records.")
        })?;
        sessions.entry(task_id).or_insert(record);
        Ok(())
    }

    /// Persists the runtime-lost recovery status for an active metadata record.
    /// 为活动元数据记录持久化运行时丢失后的恢复停止状态。
    fn persist_restored_stopped_task_session_metadata(
        &self,
        metadata: &TaskSessionMetadata,
    ) -> Result<(), String> {
        let Some(store) = self.task_session_store_for_root(Uuid::from(metadata.root_session_id))?
        else {
            return Ok(());
        };
        let mut restored = metadata.clone();
        restored.status = TaskSessionStatus::Stopped;
        restored.stop_reason = Some(String::from("runtime_lost"));
        restored.updated_at = current_unix_millis_u64();
        store
            .save_task_session(restored)
            .map_err(|error| error.message)
    }

    /// Persists archived status for closed child-agent entries when TaskSession metadata exists.
    /// 当 TaskSession 元数据存在时，为已关闭子代理条目持久化归档状态。
    pub(super) fn persist_archived_subagent_registry_entries(
        &self,
        root_session_id: SessionId,
        entries: &[SubagentRegistryEntry],
    ) -> Result<(), String> {
        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Ok(());
        };
        let archived_at = current_unix_millis_u64();
        for entry in entries {
            let Some(mut metadata) = store
                .load_task_session(entry.session_id)
                .map_err(|error| error.message)?
            else {
                continue;
            };
            metadata.status = TaskSessionStatus::Archived;
            metadata.stop_reason = Some(String::from("close_agent"));
            metadata.updated_at = archived_at;
            store
                .save_task_session(metadata)
                .map_err(|error| error.message)?;
        }
        Ok(())
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
        if let Some(existing) = registry.get(&entry.agent_id).cloned() {
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
        registry.upsert(entry);
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
        let created_status = status_name_for_task_session_status(TaskSessionStatus::Created);
        let removed_public_task_id = {
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
            });
            if should_remove {
                sessions
                    .remove(&invocation.task_id)
                    .map(|record| record.public_task_id)
            } else {
                None
            }
        };
        if let Some(public_task_id) = removed_public_task_id {
            let mut registry = self.subagent_registry.write().map_err(|_| {
                String::from(
                    "The subagent registry is locked and cannot discard the pre-start child agent.",
                )
            })?;
            let should_remove = registry.get(&public_task_id).is_some_and(|entry| {
                entry.session_id == SessionNodeId::from(invocation.task_id)
                    && entry.status == SubagentStatus::Created
            });
            if should_remove {
                registry.remove(&public_task_id);
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
        let event = build_subagent_mailbox_event(
            invocation,
            status,
            current_unix_millis_u64(),
            result_text,
            error_text,
            outcome,
        );
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
                let mut conversation_state = core_snapshot_to_session_conversation_state(snapshot);
                scope_task_context_display_records_to_prompt(
                    &mut conversation_state,
                    invocation.prompt.as_str(),
                );
                if let Some(outcome) = outcome {
                    append_task_context_turn_stats_record(
                        &mut conversation_state,
                        record.task_id,
                        now_ms,
                        status,
                        outcome,
                    );
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

            store
                .save_runtime_checkpoint(SessionRuntimeCheckpoint {
                    node_id: task_session_id,
                    status: status_name_for_task_session_status(persisted_status).to_string(),
                    last_sequence: event_sequence,
                    payload: json!({
                        "task_id": record.public_task_id,
                        "display_title": record.display_title,
                        "updated_at": now_ms,
                    }),
                })
                .map_err(|error| error.message)?;
        }

        Ok(persisted_status)
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

/// Builds the first parent reference to scan when restoring persisted subagents.
/// 构建恢复持久化子代理时首先扫描的父级引用。
fn subagent_restore_start_parent_ref(
    root_session_id: SessionNodeId,
    current_session_id: SessionNodeId,
) -> ParentSessionRef {
    if current_session_id == root_session_id {
        ParentSessionRef::MainSession { root_session_id }
    } else {
        ParentSessionRef::TaskSession {
            root_session_id,
            parent_task_session_id: current_session_id,
        }
    }
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
    if existing.stop_reason.is_none() {
        existing.stop_reason = persisted.stop_reason;
    }
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
/// `outcome` carries provider counts, tool counts, usage segments, and final output.
/// `outcome` 携带 Provider 次数、工具次数、用量分段与最终输出。
fn append_task_context_turn_stats_record(
    conversation_state: &mut SessionConversationState,
    task_id: SessionId,
    now_ms: u64,
    status: TaskSessionStatus,
    outcome: &TaskExecutionOutcome,
) {
    if outcome
        .final_assistant_message_id
        .as_deref()
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
        return;
    }
    let now = now_ms.to_string();
    let summary_record_id =
        build_session_record_message_id(task_id, now.as_str(), conversation_state.records.len());
    conversation_state.records.push(SessionConversationRecord {
        id: summary_record_id.clone(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::SummaryOutput,
        visible: false,
        title: String::from("Summary"),
        body: outcome.final_output_text.clone(),
        metadata: Some(json!({ "preview": outcome.final_output_text.clone() })),
        created_at: now.clone(),
    });
    let status = match status {
        TaskSessionStatus::Failed => SessionTurnStatsStatus::Failed,
        TaskSessionStatus::Stopped => SessionTurnStatsStatus::Stopped,
        _ => SessionTurnStatsStatus::Completed,
    };
    let record_turn_stats = build_session_turn_stats_from_state_with_segments(
        conversation_state,
        summary_record_id.as_str(),
        status,
        Some(outcome.provider_turns_executed),
        SessionTurnStatsKind::UserTurn,
        Vec::new(),
    );
    let task_context_usage_segments = task_context_usage_segments_from_outcome(outcome);
    let turn_stats = if record_turn_stats
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
            conversation_state,
            summary_record_id.as_str(),
            status,
            None,
            SessionTurnStatsKind::UserTurn,
            task_context_usage_segments,
        )
    };
    let Some(turn_stats) = turn_stats else {
        return;
    };
    let stats_record_id =
        build_session_record_message_id(task_id, now.as_str(), conversation_state.records.len());
    conversation_state.records.push(SessionConversationRecord {
        id: stats_record_id,
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::TurnStats,
        visible: true,
        title: format!("Turn Stats · {}", turn_stats.status.label()),
        body: turn_stats.detail_body(),
        metadata: Some(json!({
            "turn_stats": turn_stats,
            "turn_closing_record_id": summary_record_id,
            "task_final_assistant_message_id": outcome.final_assistant_message_id,
        })),
        created_at: now,
    });
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
