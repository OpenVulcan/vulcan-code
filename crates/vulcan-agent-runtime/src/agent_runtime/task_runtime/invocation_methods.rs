use super::*;

impl TaskRuntime {
    /// Executes one spawn-agent-backed TaskSession invocation through the backend registered for the current controller session.
    /// 通过为当前控制器会话注册的后端执行一次由 spawn_agent 承载的 TaskSession 调用。
    pub async fn invoke_task(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let backend = self
            .backend_for_session(current_session_id)
            .ok_or_else(|| {
                format!(
                    "The current session `{}` does not have a task backend installed, so the subtask cannot run.",
                    current_session_id
                )
            })?;

        let parsed_args = parse_task_tool_arguments(args)?;
        let resolved_identifier = resolve_task_session_id(self, current_session_id, &parsed_args)?;
        if let Some(cached_result) = self.cached_task_invocation_result_for_existing_run(
            current_session_id,
            &resolved_identifier,
            &parsed_args.prompt,
        )? {
            return Ok(cached_result);
        }
        if self.task_has_active_or_starting_run(resolved_identifier.task_id)? {
            return Ok(build_running_task_invocation_result(
                current_session_id,
                &resolved_identifier,
                parsed_args.prompt.as_str(),
            ));
        }
        let invocation = prepare_task_execution_request(
            self,
            current_session_id,
            resolved_identifier,
            parsed_args,
        )?;
        let outcome = match backend(invocation.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                self.mark_task_failure(current_session_id, &invocation, &error)?;
                return Err(error);
            }
        };

        if outcome.execution_status != TaskExecutionStatus::Running {
            self.store_task_outcome(current_session_id, &invocation, &outcome)?;
        }

        Ok(build_task_invocation_result(&invocation, &outcome))
    }

    /// Starts a new child-agent task session for one spawn_agent invocation.
    /// 为一次 spawn_agent 调用启动新的子代理任务会话。
    pub async fn invoke_spawn_agent(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let backend = self
            .backend_for_session(current_session_id)
            .ok_or_else(|| {
                format!(
                    "The current session `{}` does not have a task backend installed, so the subagent cannot run.",
                    current_session_id
                )
            })?;

        let parsed_args = parse_task_tool_arguments(args)?;
        let resolved_identifier =
            resolve_new_spawn_task_session_id(self, current_session_id, &parsed_args)?;
        let invocation = prepare_task_execution_request(
            self,
            current_session_id,
            resolved_identifier,
            parsed_args,
        )?;
        let outcome = match backend(invocation.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                self.mark_task_failure(current_session_id, &invocation, &error)?;
                return Err(error);
            }
        };

        if outcome.execution_status != TaskExecutionStatus::Running {
            self.store_task_outcome(current_session_id, &invocation, &outcome)?;
        }

        Ok(build_task_invocation_result(&invocation, &outcome))
    }

    /// Waits for one or more already spawned child agents without creating new task sessions.
    /// 等待一个或多个已启动的子代理，且不会创建新的任务会话。
    pub async fn invoke_wait_agent(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        // Tool-level tracing separates model wait latency from child-agent execution latency.
        // 工具级跟踪用于区分模型等待耗时和子代理自身执行耗时。
        let trace_started_at = current_unix_millis_u64();
        let timeout_ms = subagent_tool_timeout_ms(&args).unwrap_or(300_000);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        let mut waiting_logged = false;
        task_runtime_trace(
            "subagent_wait_agent_start",
            format!("parent={current_session_id} timeout_ms={timeout_ms}"),
        );
        loop {
            let records = self.child_agent_records_for_tool_request(current_session_id, &args)?;
            let active_task_ids = self.active_task_ids_from_records(&records)?;
            if active_task_ids.is_empty() || tokio::time::Instant::now() >= deadline {
                let registry_entries =
                    self.subagent_registry_entries_for_tool_request(current_session_id, &args)?;
                if !registry_entries.is_empty() {
                    let result = build_subagent_registry_status_tool_result(
                        "wait_agent",
                        current_session_id,
                        registry_entries,
                        timeout_ms,
                        active_task_ids.is_empty(),
                    );
                    task_runtime_trace(
                        "subagent_wait_agent_return",
                        format!(
                            "parent={current_session_id} source=registry active_count={} elapsed_ms={}",
                            active_task_ids.len(),
                            current_unix_millis_u64().saturating_sub(trace_started_at)
                        ),
                    );
                    return Ok(result);
                }
                let result = build_subagent_status_tool_result(
                    "wait_agent",
                    current_session_id,
                    records,
                    timeout_ms,
                    active_task_ids.is_empty(),
                );
                task_runtime_trace(
                    "subagent_wait_agent_return",
                    format!(
                        "parent={current_session_id} source=records active_count={} elapsed_ms={}",
                        active_task_ids.len(),
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                return Ok(result);
            }
            if !waiting_logged {
                waiting_logged = true;
                task_runtime_trace(
                    "subagent_wait_agent_waiting",
                    format!(
                        "parent={current_session_id} active_count={} elapsed_ms={}",
                        active_task_ids.len(),
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let mut streams = active_task_ids
                .iter()
                .map(|task_id| Box::pin(self.subscribe_detached_task_events(*task_id)))
                .collect::<Vec<_>>();
            let mut event_futures = FuturesUnordered::new();
            for stream in streams.iter_mut() {
                event_futures.push(stream.next());
            }
            tokio::select! {
                _ = event_futures.next() => {}
                _ = tokio::time::sleep(remaining) => {}
            }
        }
    }

    /// Lists known child agents for the current direct parent without touching execution state.
    /// 列出当前直接父级下已知的子代理，不改变执行状态。
    pub async fn invoke_list_agents(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let trace_started_at = current_unix_millis_u64();
        task_runtime_trace(
            "subagent_list_agents_start",
            format!("parent={current_session_id}"),
        );
        let registry_entries =
            self.subagent_registry_entries_for_tool_request(current_session_id, &args)?;
        if !registry_entries.is_empty() {
            let result = build_subagent_registry_status_tool_result(
                "list_agents",
                current_session_id,
                registry_entries,
                0,
                true,
            );
            task_runtime_trace(
                "subagent_list_agents_return",
                format!(
                    "parent={current_session_id} source=registry elapsed_ms={}",
                    current_unix_millis_u64().saturating_sub(trace_started_at)
                ),
            );
            return Ok(result);
        }
        let records = self.child_agent_records_for_tool_request(current_session_id, &args)?;
        let result =
            build_subagent_status_tool_result("list_agents", current_session_id, records, 0, true);
        task_runtime_trace(
            "subagent_list_agents_return",
            format!(
                "parent={current_session_id} source=records elapsed_ms={}",
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
        Ok(result)
    }

    /// Queues a message for one existing child agent without starting new child work.
    /// 为一个已存在的子代理暂存消息，且不启动新的子代理工作。
    pub async fn invoke_send_message(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let trace_started_at = current_unix_millis_u64();
        let message_args = parse_subagent_message_tool_arguments(args, "send_message")?;
        task_runtime_trace(
            "subagent_send_message_start",
            format!(
                "parent={current_session_id} target={} message_chars={}",
                message_args.target,
                message_args.message.chars().count()
            ),
        );
        let target_task_ids = self
            .child_task_ids_for_message_target(current_session_id, message_args.target.as_str())?;
        self.queue_child_messages(&target_task_ids, message_args.message.as_str())?;
        let result = build_subagent_message_tool_result(
            "send_message",
            current_session_id,
            message_args.target.as_str(),
            target_task_ids,
            false,
            true,
            None,
        );
        task_runtime_trace(
            "subagent_send_message_return",
            format!(
                "parent={current_session_id} target={} elapsed_ms={}",
                message_args.target,
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
        Ok(result)
    }

    /// Sends a follow-up task to one existing child agent and triggers a child continuation when possible.
    /// 向一个已存在子代理发送后续任务，并在可行时触发子代理继续执行。
    pub async fn invoke_followup_task(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let trace_started_at = current_unix_millis_u64();
        let message_args = parse_subagent_message_tool_arguments(args, "followup_task")?;
        task_runtime_trace(
            "subagent_followup_task_start",
            format!(
                "parent={current_session_id} target={} message_chars={}",
                message_args.target,
                message_args.message.chars().count()
            ),
        );
        let selector_args = json!({ "task_name": message_args.target.clone() });
        let target_records =
            self.child_agent_records_for_tool_request(current_session_id, &selector_args)?;
        let active_task_ids = self.active_task_ids_from_records(&target_records)?;
        if !active_task_ids.is_empty() {
            self.queue_child_messages(&active_task_ids, message_args.message.as_str())?;
            let result = build_subagent_message_tool_result(
                "followup_task",
                current_session_id,
                message_args.target.as_str(),
                active_task_ids,
                true,
                true,
                Some(String::from(
                    "The selected child agent is still running; the follow-up was queued for the next child-agent continuation.",
                )),
            );
            task_runtime_trace(
                "subagent_followup_task_return",
                format!(
                    "parent={current_session_id} target={} queued_for_active=true elapsed_ms={}",
                    message_args.target,
                    current_unix_millis_u64().saturating_sub(trace_started_at)
                ),
            );
            return Ok(result);
        }
        let target_task_ids =
            self.child_task_ids_for_message_target(current_session_id, &message_args.target)?;
        let queued_messages = self.drain_queued_child_messages(&target_task_ids)?;
        let prompt = followup_prompt_from_messages(queued_messages, message_args.message);
        let task_args = json!({
            "prompt": prompt,
            "task_id": message_args.target,
            "description": format!("Follow up {}", message_args.target),
        });
        let result = self.invoke_task(current_session_id, task_args).await;
        task_runtime_trace(
            "subagent_followup_task_return",
            format!(
                "parent={current_session_id} target={} ok={} elapsed_ms={}",
                message_args.target,
                result.is_ok(),
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
        result
    }

    /// Resolves child task-session ids for one message target.
    /// 为单个消息目标解析子任务会话标识列表。
    fn child_task_ids_for_message_target(
        &self,
        current_session_id: SessionId,
        target: &str,
    ) -> Result<Vec<SessionId>, String> {
        let selector_args = json!({ "task_name": target });
        let records =
            self.child_agent_records_for_tool_request(current_session_id, &selector_args)?;
        let mut task_ids = records
            .into_iter()
            .map(|record| record.task_id)
            .collect::<Vec<_>>();
        if task_ids.is_empty() {
            task_ids.extend(
                self.subagent_registry_entries_for_tool_request(
                    current_session_id,
                    &selector_args,
                )?
                .into_iter()
                .map(|entry| entry.session_id.as_uuid()),
            );
        }
        if task_ids.is_empty() {
            return Err(format!(
                "The target child agent `{target}` does not exist under the current parent session."
            ));
        }
        task_ids.sort();
        task_ids.dedup();
        Ok(task_ids)
    }

    /// Queues one message for every selected child task session.
    /// 为每个选中的子任务会话暂存一条消息。
    fn queue_child_messages(&self, task_ids: &[SessionId], message: &str) -> Result<(), String> {
        let mut queued_messages = self.queued_child_messages.write().map_err(|_| {
            String::from("The child-agent message queue is locked and cannot update.")
        })?;
        for task_id in task_ids {
            queued_messages
                .entry(*task_id)
                .or_default()
                .push(message.to_string());
        }
        Ok(())
    }

    /// Drains queued parent-authored messages for the selected child task sessions.
    /// 取出选中子任务会话中暂存的父级消息。
    fn drain_queued_child_messages(&self, task_ids: &[SessionId]) -> Result<Vec<String>, String> {
        let mut queued_messages = self.queued_child_messages.write().map_err(|_| {
            String::from("The child-agent message queue is locked and cannot read.")
        })?;
        let mut drained = Vec::new();
        for task_id in task_ids {
            if let Some(messages) = queued_messages.remove(task_id) {
                drained.extend(messages);
            }
        }
        Ok(drained)
    }

    /// Reads registry entries for one subagent tool request.
    /// 读取一次子代理工具请求对应的注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent session id.
    /// `current_session_id` 是直接父会话标识。
    /// `args` carries the optional child-agent selector.
    /// `args` 携带可选子代理选择器。
    ///
    /// # Returns
    /// 返回值
    /// Matching registry entries cloned from the native registry snapshot.
    /// 从原生注册表快照克隆出的匹配条目。
    fn subagent_registry_entries_for_tool_request(
        &self,
        current_session_id: SessionId,
        args: &Value,
    ) -> Result<Vec<SubagentRegistryEntry>, String> {
        let registry = self.subagent_registry_snapshot()?;
        let parent_session_id = SessionNodeId::from(current_session_id);
        if let Some(target) = subagent_tool_target(args) {
            let target = normalize_public_task_identifier(&target)?;
            return Ok(registry
                .get(&target)
                .filter(|entry| entry.parent_session_id == parent_session_id)
                .cloned()
                .into_iter()
                .collect());
        }
        Ok(registry
            .list_for_parent(parent_session_id)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Stops one running child agent selected by task name, public id, or task-session id.
    /// 停止一个按任务名、公开标识或任务会话标识选中的运行中子代理。
    pub async fn invoke_close_agent(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let trace_started_at = current_unix_millis_u64();
        task_runtime_trace(
            "subagent_close_agent_start",
            format!("parent={current_session_id}"),
        );
        let registry_entries =
            self.subagent_registry_entries_for_tool_request(current_session_id, &args)?;
        if !registry_entries.is_empty() {
            let mut stopped_task_ids = Vec::new();
            for entry in &registry_entries {
                stopped_task_ids.extend(self.stop_task_tree(SessionId::from(entry.session_id))?);
            }
            let result = VulcanToolInvocationResult::from_model_output(json!({
                "tool": "close_agent",
                "parent_session_id": current_session_id.to_string(),
                "matched_agents": registry_entries.iter().map(subagent_registry_entry_status_payload).collect::<Vec<_>>(),
                "stopped_task_session_ids": stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "stopped_count": stopped_task_ids.len(),
            })).with_host_result(VulcanToolHostResult {
                kind: String::from("subagent_control"),
                payload: json!({
                    "tool": "close_agent",
                    "parent_session_id": current_session_id.to_string(),
                    "stopped_task_session_ids": stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                }),
            });
            task_runtime_trace(
                "subagent_close_agent_return",
                format!(
                    "parent={current_session_id} source=registry stopped_count={} elapsed_ms={}",
                    stopped_task_ids.len(),
                    current_unix_millis_u64().saturating_sub(trace_started_at)
                ),
            );
            return Ok(result);
        }
        let records = self.child_agent_records_for_tool_request(current_session_id, &args)?;
        let mut stopped_task_ids = Vec::new();
        for record in &records {
            stopped_task_ids.extend(self.stop_task_tree(record.task_id)?);
        }
        let result = VulcanToolInvocationResult::from_model_output(json!({
            "tool": "close_agent",
            "parent_session_id": current_session_id.to_string(),
            "matched_agents": records.iter().map(subagent_record_status_payload).collect::<Vec<_>>(),
            "stopped_task_session_ids": stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "stopped_count": stopped_task_ids.len(),
        })).with_host_result(VulcanToolHostResult {
            kind: String::from("subagent_control"),
            payload: json!({
                "tool": "close_agent",
                "parent_session_id": current_session_id.to_string(),
                "stopped_task_session_ids": stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            }),
        });
        task_runtime_trace(
            "subagent_close_agent_return",
            format!(
                "parent={current_session_id} source=records stopped_count={} elapsed_ms={}",
                stopped_task_ids.len(),
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
        Ok(result)
    }
}
