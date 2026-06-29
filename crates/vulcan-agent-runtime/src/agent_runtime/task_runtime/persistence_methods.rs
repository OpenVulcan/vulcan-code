use super::*;

impl TaskRuntime {
    /// Stores one newly completed task outcome back into the session registry.
    /// 将一条新完成的任务结果回写到会话注册表中。
    pub(super) fn store_task_outcome(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        let record = self.upsert_task_outcome_record(current_session_id, invocation, outcome)?;
        self.publish_subagent_mailbox_event(
            invocation,
            task_session_status_from_execution_status(outcome.execution_status),
            Some(outcome.final_output_text.as_str()),
            None,
            Some(outcome),
        );
        self.persist_task_record_outcome(&record, invocation, outcome);
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
        let record = self.upsert_task_outcome_record(current_session_id, invocation, outcome)?;
        if let Err(error) = self.persist_task_record_state(
            &record,
            invocation,
            TaskSessionStatus::Running,
            None,
            None,
            Some(&outcome.snapshot),
            Some(&outcome.controller_config),
            Some(outcome),
        ) {
            eprintln!(
                "[task-session-store] persist running state failed for task {}: {error}",
                record.task_id
            );
        }
        Ok(())
    }

    /// Upserts one task outcome into the in-memory registry and returns the updated record.
    /// 将任务结果写入内存注册表，并返回更新后的记录。
    fn upsert_task_outcome_record(
        &self,
        _current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<TaskSessionRecord, String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task session registry is locked and cannot store the result.")
        })?;
        let now = current_unix_millis_text();
        let status = outcome.execution_status.as_str().to_string();
        let record = sessions.entry(invocation.task_id).or_insert_with(|| {
            build_task_session_record(
                invocation.task_id,
                invocation.public_task_id.clone(),
                invocation.root_session_id,
                invocation.parent_task_id,
                invocation.depth,
                invocation.description.clone(),
                invocation.subagent.clone(),
                invocation.invocation_config.clone(),
                invocation.display_title.clone(),
                invocation
                    .controller_config
                    .clone()
                    .or_else(|| Some(outcome.controller_config.clone())),
                invocation.snapshot.clone(),
                invocation.prompt.clone(),
                String::from("created"),
            )
        });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = &record.created_at;
        record.controller_config = Some(outcome.controller_config.clone());
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
        let task_status = task_session_status_from_execution_status(outcome.execution_status);
        let result_summary = (!task_status.is_active()
            && !record.last_result_text.trim().is_empty())
        .then_some(record.last_result_text.as_str());
        self.upsert_subagent_registry_entry(record, task_status, result_summary)?;
        Ok(record.clone())
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
        if let Some(prompt_source) = subagent_prompt_source_for_task_record(record) {
            entry.prompt_source = Some(prompt_source);
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
            if entry.segments.is_empty() {
                entry.segments = existing.segments;
            }
            if entry.result_summary.is_none() {
                entry.result_summary = existing.result_summary;
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
        let record = {
            let mut sessions = self.sessions.write().map_err(|_| {
                String::from(
                    "The task session registry is locked and cannot store the failure state.",
                )
            })?;
            let record = sessions.entry(invocation.task_id).or_insert_with(|| {
                build_task_session_record(
                    invocation.task_id,
                    invocation.public_task_id.clone(),
                    invocation.root_session_id,
                    invocation.parent_task_id,
                    invocation.depth,
                    invocation.description.clone(),
                    invocation.subagent.clone(),
                    invocation.invocation_config.clone(),
                    invocation.display_title.clone(),
                    invocation.controller_config.clone(),
                    invocation.snapshot.clone(),
                    invocation.prompt.clone(),
                    String::from("created"),
                )
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
            record.clone()
        };
        self.upsert_subagent_registry_entry(&record, TaskSessionStatus::Failed, Some(error))?;
        self.publish_subagent_mailbox_event(
            invocation,
            TaskSessionStatus::Failed,
            Some(error),
            Some(error),
            None,
        );
        self.persist_task_record_failure(&record, invocation, error);
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
    ) {
        let status = task_session_status_from_execution_status(outcome.execution_status);
        if let Err(error) = self.persist_task_record_state(
            record,
            invocation,
            status,
            Some(outcome.final_output_text.as_str()),
            None,
            Some(&outcome.snapshot),
            Some(&outcome.controller_config),
            Some(outcome),
        ) {
            eprintln!(
                "[task-session-store] persist outcome failed for task {}: {error}",
                record.task_id
            );
        }
    }

    /// Persists one failed task state into the segmented TaskSession store when configured.
    /// 在已配置时将失败任务状态持久化到分段 TaskSession 存储。
    fn persist_task_record_failure(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) {
        if let Err(persist_error) = self.persist_task_record_state(
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
        ) {
            eprintln!(
                "[task-session-store] persist failure failed for task {}: {persist_error}",
                record.task_id
            );
        }
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
    ) -> Result<(), String> {
        let Some(store) = self.task_session_store_for_root(record.origin_session_id)? else {
            return Ok(());
        };
        let now_ms = current_unix_millis_u64();
        let parent = parent_ref_for_task_record(record);
        let resolved_config = outcome
            .map(|outcome| outcome.resolved_config.clone())
            .unwrap_or_else(|| {
                self.resolve_task_session_config_for_record(record, controller_config)
            });
        let task_session_id = SessionNodeId::from(record.task_id);
        let root_session_id = SessionNodeId::from(record.origin_session_id);
        let mut metadata = match store
            .load_task_session(task_session_id)
            .map_err(|error| error.message)?
        {
            Some(metadata) => metadata,
            None => {
                let mut metadata = TaskSessionMetadata::new(
                    root_session_id,
                    task_session_id,
                    parent.clone(),
                    record.public_task_id.clone(),
                    record.display_title.clone(),
                    resolved_config.requested_config.clone(),
                    resolved_config.effective_config.clone(),
                    record.depth as u16,
                    now_ms,
                );
                metadata.description = record.description.clone();
                store
                    .create_task_session(metadata.clone())
                    .map_err(|error| error.message)?;
                metadata
            }
        };
        metadata.display_title = record.display_title.clone();
        metadata.description = record.description.clone();
        metadata.subagent_profile_id = None;
        metadata.inherit_parent_config = resolved_config.effective_config.inherit_parent_config;
        metadata.status = status;
        metadata.runtime_policy = resolved_config.runtime_policy;
        metadata.requested_config = resolved_config.requested_config.clone();
        metadata.effective_config = resolved_config.effective_config.clone();
        metadata.updated_at = now_ms;
        metadata.last_result_summary = result_text.and_then(task_result_summary);
        metadata.last_error = error_text.map(str::to_string);
        store
            .save_task_session(metadata)
            .map_err(|error| error.message)?;

        let event_sequence = now_ms;
        store
            .append_event(SessionEventRecord::new(
                format!(
                    "task-node-state-{}-{status:?}-{event_sequence}",
                    record.task_id
                ),
                task_session_id,
                event_sequence,
                task_session_event_kind_for_status(status),
                now_ms,
                task_state_event_payload(
                    record,
                    invocation,
                    result_text,
                    error_text,
                    status,
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
                    event_id: format!("task-state-{}-{status:?}-{parent_sequence}", record.task_id),
                    root_session_id,
                    parent: parent_ref_for_task_record(record),
                    task_session_id,
                    sequence: parent_sequence,
                    kind: task_invocation_event_kind_for_status(status),
                    status,
                    prompt: Some(invocation.prompt.clone()),
                    timestamp_ms: now_ms,
                    payload: task_state_event_payload(
                        record,
                        invocation,
                        result_text,
                        error_text,
                        status,
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
            store
                .save_context(SessionContextDocument {
                    schema_version: 1,
                    node_id: task_session_id,
                    provider_id: Some(controller_config.provider_id.clone()),
                    model_id: Some(controller_config.model_id.clone()),
                    reasoning_effort: None,
                    system_prompt_layers: vec![controller_config.system_prompt.clone()],
                    conversation_state: serde_json::to_value(
                        core_snapshot_to_session_conversation_state(snapshot),
                    )
                    .unwrap_or(Value::Null),
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
        }

        store
            .save_runtime_checkpoint(SessionRuntimeCheckpoint {
                node_id: task_session_id,
                status: status_name_for_task_session_status(status).to_string(),
                last_sequence: event_sequence,
                payload: json!({
                    "task_id": record.public_task_id,
                    "display_title": record.display_title,
                    "updated_at": now_ms,
                }),
            })
            .map_err(|error| error.message)?;

        let registry_result_summary =
            (!status.is_active()).then(|| result_text.filter(|text| !text.trim().is_empty()));
        self.upsert_subagent_registry_entry(record, status, registry_result_summary.flatten())?;

        Ok(())
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
        Ok(TaskSessionConfigResolver::resolve(
            TaskSessionConfigResolutionInput {
                global_default_config: SessionEffectiveConfig::default(),
                parent_config,
                profile: None,
                invocation: invocation_config.clone(),
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
        TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config,
            profile: None,
            invocation: record.invocation_config.clone(),
        })
    }
}
