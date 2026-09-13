use super::*;

impl TaskRuntime {
    /// Stores one terminal child-task outcome in the process-local task projections.
    /// 将一条终态子任务结果保存到进程内任务投影。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the outcome; retained for the runtime call contract.
    /// - `current_session_id`：观察到该结果的控制器会话；为运行时调用契约保留。
    /// - `invocation`: Exact child-task invocation identity and configuration.
    /// - `invocation`：精确的子任务调用身份与配置。
    /// - `outcome`: Terminal execution outcome to project.
    /// - `outcome`：需要投影的终态执行结果。
    ///
    /// # Returns
    /// 返回值
    /// Success after the resident task and registry projections are updated.
    /// 在常驻任务与注册表投影更新后返回成功。
    pub(super) fn store_task_outcome(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        self.store_task_outcome_projection(current_session_id, invocation, outcome, true)
            .map(|_| ())
    }

    /// Stores one terminal outcome while deferring parent-mailbox publication until successor ownership is known.
    /// 保存一条终态结果，并将父级 mailbox 发布延迟到后继执行所有权确定之后。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the outcome.
    /// - `current_session_id`：观察到该结果的控制器会话。
    /// - `invocation`: Exact physical child execution that produced the outcome.
    /// - `invocation`：产出该结果的精确物理子执行。
    /// - `outcome`: Terminal execution outcome to persist without immediate mailbox publication.
    /// - `outcome`：需要持久化但不立即发布 mailbox 的终态执行结果。
    ///
    /// # Returns
    /// 返回值
    /// `true` when this execution still owned and committed the stable task record; `false` when it was stale.
    /// 当该执行仍拥有并提交了稳定任务记录时返回 `true`；执行已过期时返回 `false`。
    pub(super) fn store_task_outcome_without_parent_mailbox(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<bool, String> {
        self.store_task_outcome_projection(current_session_id, invocation, outcome, false)
    }

    /// Publishes a previously committed terminal outcome to the process-local parent mailbox.
    /// 将先前已经提交的终态结果发布到进程内父级 mailbox。
    ///
    /// # Parameters
    /// 参数
    /// - `invocation`: Exact child execution that owns the terminal event.
    /// - `invocation`：拥有该终态事件的精确子执行。
    /// - `outcome`: Committed terminal outcome used to build the mailbox payload.
    /// - `outcome`：用于构建 mailbox 载荷的已提交终态结果。
    pub(super) fn publish_task_outcome_to_parent_mailbox(
        &self,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) {
        if outcome.execution_status == TaskExecutionStatus::Completed {
            match self.durable_return_mailboxes.read() {
                Ok(returns)
                    if returns
                        .get(&invocation.task_id)
                        .is_some_and(|(execution, _)| execution == &invocation.execution_id) =>
                {
                    return;
                }
                Err(_) => {
                    eprintln!(
                        "[task-runtime] durable return registry locked; refusing premature completion feedback"
                    );
                    return;
                }
                _ => {}
            }
        }
        self.publish_subagent_mailbox_event(
            invocation,
            task_session_status_from_execution_status(outcome.execution_status),
            Some(outcome.final_output_text.as_str()),
            None,
            Some(outcome),
        );
    }

    /// Commits one terminal task projection with explicit parent-mailbox publication policy.
    /// 按显式父级 mailbox 发布策略提交一条终态任务投影。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the outcome.
    /// - `current_session_id`：观察到该结果的控制器会话。
    /// - `invocation`: Exact child-task invocation identity and configuration.
    /// - `invocation`：精确的子任务调用身份与配置。
    /// - `outcome`: Terminal execution outcome to project.
    /// - `outcome`：需要投影的终态执行结果。
    /// - `publish_parent_mailbox`: Whether this commit may immediately wake the parent waiter.
    /// - `publish_parent_mailbox`：本次提交是否可以立即唤醒父级等待者。
    ///
    /// # Returns
    /// 返回值
    /// Whether this exact execution committed the stable task projection.
    /// 返回该精确执行是否提交了稳定任务投影。
    fn store_task_outcome_projection(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
        publish_parent_mailbox: bool,
    ) -> Result<bool, String> {
        let transaction_lock = self.task_execution_state_lock(invocation.task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot store the terminal outcome.",
            )
        })?;
        let status = task_session_status_from_execution_status(outcome.execution_status);
        if !self.task_execution_can_commit_status(invocation, status)? {
            task_runtime_trace(
                "task_outcome_stale_execution_skipped",
                format!(
                    "task={} execution_id={}",
                    invocation.task_id, invocation.execution_id
                ),
            );
            return Ok(false);
        }

        let record = self.build_task_outcome_record(current_session_id, invocation, outcome)?;
        self.commit_task_session_record(record.clone())?;
        self.upsert_subagent_registry_entry(
            &record,
            status,
            task_record_registry_result_summary(&record, status),
        )?;
        if publish_parent_mailbox {
            self.publish_task_outcome_to_parent_mailbox(invocation, outcome);
        }
        Ok(true)
    }

    /// Stores one running child-task outcome in the process-local task projections.
    /// 将一条运行中的子任务结果保存到进程内任务投影。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the running outcome.
    /// - `current_session_id`：观察到运行中结果的控制器会话。
    /// - `invocation`: Exact child-task invocation identity and configuration.
    /// - `invocation`：精确的子任务调用身份与配置。
    /// - `outcome`: Running execution outcome to project.
    /// - `outcome`：需要投影的运行中执行结果。
    ///
    /// # Returns
    /// 返回值
    /// Success after the resident task and registry projections are updated.
    /// 在常驻任务与注册表投影更新后返回成功。
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
        self.commit_task_session_record(record.clone())?;
        self.upsert_subagent_registry_entry(
            &record,
            TaskSessionStatus::Running,
            task_record_registry_result_summary(&record, TaskSessionStatus::Running),
        )?;
        Ok(())
    }

    /// Mirrors one prepared resident task record into the process-local child-agent registry.
    /// 将一条已准备的常驻任务记录镜像到进程内子代理注册表。
    ///
    /// # Parameters
    /// 参数
    /// - `task_id`: Exact resident child-task identifier.
    /// - `task_id`：精确的常驻子任务标识。
    ///
    /// # Returns
    /// 返回值
    /// Success after the matching registry entry is projected.
    /// 在匹配注册表条目投影完成后返回成功。
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

    /// Builds the next process-local task record without mutating the resident registry.
    /// 构建下一条进程内任务记录，但不修改常驻注册表。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the outcome; retained for the runtime call contract.
    /// - `current_session_id`：观察到该结果的控制器会话；为运行时调用契约保留。
    /// - `invocation`: Exact child-task invocation identity and configuration.
    /// - `invocation`：精确的子任务调用身份与配置。
    /// - `outcome`: Execution outcome used to refresh the resident record.
    /// - `outcome`：用于刷新常驻记录的执行结果。
    ///
    /// # Returns
    /// 返回值
    /// The fully staged resident task record.
    /// 返回完整暂存的常驻任务记录。
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

    /// Commits one staged task record to the process-local resident registry.
    /// 将一条已暂存任务记录提交到进程内常驻注册表。
    ///
    /// # Parameters
    /// 参数
    /// - `record`: Fully staged task record to publish.
    /// - `record`：需要发布的完整暂存任务记录。
    ///
    /// # Returns
    /// 返回值
    /// Success after the exact task id owns the new record.
    /// 在精确任务标识拥有新记录后返回成功。
    fn commit_task_session_record(&self, record: TaskSessionRecord) -> Result<(), String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task session registry is locked and cannot commit the result.")
        })?;
        sessions.insert(record.task_id, record);
        Ok(())
    }

    /// Builds one failed resident task record without mutating runtime state.
    /// 构建一条失败的常驻任务记录，但不修改运行时状态。
    ///
    /// # Parameters
    /// 参数
    /// - `invocation`: Exact child-task invocation that failed.
    /// - `invocation`：发生失败的精确子任务调用。
    /// - `error`: Stable failure text projected to the process-local registry.
    /// - `error`：投影到进程内注册表的稳定失败文本。
    ///
    /// # Returns
    /// 返回值
    /// The fully staged failed task record.
    /// 返回完整暂存的失败任务记录。
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

    /// Projects one resident task record into the process-local child-agent registry.
    /// 将一条常驻任务记录投影到进程内子代理注册表。
    ///
    /// # Parameters
    /// 参数
    /// - `record`: Exact resident task record to project.
    /// - `record`：需要投影的精确常驻任务记录。
    /// - `status`: Runtime lifecycle status for the projection.
    /// - `status`：投影对应的运行时生命周期状态。
    /// - `result_summary`: Optional terminal summary derived from the resident record.
    /// - `result_summary`：从常驻记录派生的可选终态摘要。
    ///
    /// # Returns
    /// 返回值
    /// Success after the process-local registry owns the projection.
    /// 在进程内注册表拥有该投影后返回成功。
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
                // Preserve a process-local archive against late completion callbacks.
                // 保留进程内归档状态，防止晚到的完成回调将其覆盖。
                if existing.status == SubagentStatus::Archived {
                    entry.status = SubagentStatus::Archived;
                    entry.stop_reason = existing.stop_reason.clone();
                    entry.updated_at_ms = existing.updated_at_ms;
                }
                if entry.stop_reason.is_none() {
                    entry.stop_reason = existing.stop_reason.clone();
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

    /// Stores one failed child-task outcome in the process-local task projections.
    /// 将一条失败的子任务结果保存到进程内任务投影。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the failure; retained for the runtime call contract.
    /// - `current_session_id`：观察到失败的控制器会话；为运行时调用契约保留。
    /// - `invocation`: Exact child-task invocation identity and configuration.
    /// - `invocation`：精确的子任务调用身份与配置。
    /// - `error`: Stable failure text projected to the resident record and mailbox.
    /// - `error`：投影到常驻记录与邮箱的稳定失败文本。
    ///
    /// # Returns
    /// 返回值
    /// Success after the resident task and registry projections are updated.
    /// 在常驻任务与注册表投影更新后返回成功。
    pub(super) fn mark_task_failure(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<(), String> {
        self.mark_task_failure_projection(current_session_id, invocation, error, true)
            .map(|_| ())
    }

    /// Stores one failed outcome while deferring parent-mailbox publication until successor ownership is known.
    /// 保存一条失败结果，并将父级 mailbox 发布延迟到后继执行所有权确定之后。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session that observed the failure.
    /// - `current_session_id`：观察到失败的控制器会话。
    /// - `invocation`: Exact physical child execution that failed.
    /// - `invocation`：发生失败的精确物理子执行。
    /// - `error`: Stable failure text to persist without immediate mailbox publication.
    /// - `error`：需要持久化但不立即发布 mailbox 的稳定失败文本。
    ///
    /// # Returns
    /// 返回值
    /// `true` when this execution committed the failure projection; `false` when it was stale.
    /// 当该执行提交了失败投影时返回 `true`；执行已过期时返回 `false`。
    pub(super) fn mark_task_failure_without_parent_mailbox(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<bool, String> {
        self.mark_task_failure_projection(current_session_id, invocation, error, false)
    }

    /// Publishes a previously committed child failure to the process-local parent mailbox.
    /// 将先前已经提交的子代理失败发布到进程内父级 mailbox。
    ///
    /// # Parameters
    /// 参数
    /// - `invocation`: Exact child execution that owns the failure event.
    /// - `invocation`：拥有该失败事件的精确子执行。
    /// - `error`: Stable failure text used by the mailbox payload.
    /// - `error`：mailbox 载荷使用的稳定失败文本。
    pub(super) fn publish_task_failure_to_parent_mailbox(
        &self,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) {
        self.publish_subagent_mailbox_event(
            invocation,
            TaskSessionStatus::Failed,
            Some(error),
            Some(error),
            None,
        );
    }

    /// Commits one failed task projection with explicit parent-mailbox publication policy.
    /// 按显式父级 mailbox 发布策略提交一条失败任务投影。
    ///
    /// # Parameters
    /// 参数
    /// - `current_session_id`: Controller session retained by the task-runtime call contract.
    /// - `current_session_id`：任务运行时调用契约保留的控制器会话。
    /// - `invocation`: Exact child execution that failed.
    /// - `invocation`：发生失败的精确子执行。
    /// - `error`: Stable failure text projected into the task record.
    /// - `error`：投影到任务记录中的稳定失败文本。
    /// - `publish_parent_mailbox`: Whether this commit may immediately wake the parent waiter.
    /// - `publish_parent_mailbox`：本次提交是否可以立即唤醒父级等待者。
    ///
    /// # Returns
    /// 返回值
    /// Whether this exact execution committed the stable failure projection.
    /// 返回该精确执行是否提交了稳定失败投影。
    fn mark_task_failure_projection(
        &self,
        _current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
        publish_parent_mailbox: bool,
    ) -> Result<bool, String> {
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
            return Ok(false);
        }

        let record = self.build_failed_task_record(invocation, error)?;
        self.commit_task_session_record(record.clone())?;
        self.upsert_subagent_registry_entry(
            &record,
            TaskSessionStatus::Failed,
            task_record_registry_result_summary(&record, TaskSessionStatus::Failed),
        )?;
        if publish_parent_mailbox {
            self.publish_task_failure_to_parent_mailbox(invocation, error);
        }
        Ok(true)
    }

    /// Removes one still-created resident task after its backend failed before execution began.
    /// 在后端尚未开始执行即失败时，移除仍处于 created 状态的常驻任务。
    ///
    /// # Parameters
    /// 参数
    /// - `invocation`: Exact pre-start child-task invocation to discard.
    /// - `invocation`：需要丢弃的精确预启动子任务调用。
    ///
    /// # Returns
    /// 返回值
    /// Success when the matching process-local record is absent or removed atomically with its registry entry.
    /// 当匹配的进程内记录不存在，或与注册表条目一同原子移除时返回成功。
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

    /// Publishes one process-local child-agent lifecycle event to the parent mailbox.
    /// 将一条进程内子代理生命周期事件发布到父级邮箱。
    ///
    /// # Parameters
    /// 参数
    /// - `invocation`: Exact child-task invocation that owns the event.
    /// - `invocation`：拥有该事件的精确子任务调用。
    /// - `status`: Runtime lifecycle status projected to the mailbox.
    /// - `status`：投影到邮箱的运行时生命周期状态。
    /// - `result_text`: Optional terminal result text for process-local consumers.
    /// - `result_text`：供进程内消费者使用的可选终态结果文本。
    /// - `error_text`: Optional failure text for process-local consumers.
    /// - `error_text`：供进程内消费者使用的可选失败文本。
    /// - `outcome`: Optional execution outcome used by the event payload builder.
    /// - `outcome`：供事件载荷构建器使用的可选执行结果。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; mailbox publication failures are reported to stderr.
    /// 不返回值；邮箱发布失败会写入标准错误。
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

    /// Resolves the typed configuration used by one new child execution.
    /// 解析一条新子任务执行使用的强类型配置。
    ///
    /// # Parameters
    /// 参数
    /// - `parent_controller_config`: Effective parent controller configuration.
    /// - `parent_controller_config`：父控制器的有效配置。
    /// - `invocation_config`: Child invocation overrides selected by the caller.
    /// - `invocation_config`：调用方选择的子任务覆盖配置。
    ///
    /// # Returns
    /// 返回值
    /// The fully resolved child-session configuration or an explicit catalog error.
    /// 返回完整解析的子会话配置，或显式目录错误。
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

    /// Resolves the enabled child-agent profile and normalized invocation overrides.
    /// 解析已启用的子代理画像与规范化调用覆盖配置。
    ///
    /// # Parameters
    /// 参数
    /// - `invocation_config`: Child invocation overrides selected by the caller.
    /// - `invocation_config`：调用方选择的子任务覆盖配置。
    ///
    /// # Returns
    /// 返回值
    /// The optional typed profile and normalized invocation, or an explicit catalog error.
    /// 返回可选强类型画像与规范化调用，或显式目录错误。
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

/// Returns the compact process-local registry result summary for one child-task record.
/// 返回一条子任务记录用于进程内注册表投影的紧凑结果摘要。
///
/// # Parameters
/// 参数
/// - `record`: Exact resident child-task record.
/// - `record`：精确的常驻子任务记录。
/// - `status`: Runtime lifecycle status projected from the record.
/// - `status`：从记录投影出的运行时生命周期状态。
///
/// # Returns
/// 返回值
/// Non-empty terminal result text, otherwise no summary.
/// 返回非空终态结果文本，否则不返回摘要。
fn task_record_registry_result_summary(
    record: &TaskSessionRecord,
    status: TaskSessionStatus,
) -> Option<&str> {
    (!status.is_active() && !record.last_result_text.trim().is_empty())
        .then_some(record.last_result_text.as_str())
}
