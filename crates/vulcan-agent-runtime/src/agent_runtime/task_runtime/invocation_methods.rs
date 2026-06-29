use super::*;

impl TaskRuntime {
    /// Executes one spawn-agent-backed subagent invocation through the backend registered for the current controller session.
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

    /// Starts a new child-agent task for one spawn_agent invocation.
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

        Ok(build_spawn_agent_invocation_result(&invocation, &outcome))
    }

    /// Waits for one or more already spawned child agents without creating new subagent tasks.
    /// 等待一个或多个已启动的子代理，且不会创建新的任务会话。
    pub async fn invoke_wait_agent(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        // Tool-level tracing separates model wait latency from child-agent execution latency.
        // 工具级跟踪用于区分模型等待耗时和子代理自身执行耗时。
        let trace_started_at = current_unix_millis_u64();
        let timeout_ms = wait_agent_timeout_ms(&args)?.unwrap_or(WAIT_AGENT_DEFAULT_TIMEOUT_MS);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        let parent_thread_id = SessionNodeId::from(current_session_id);
        let mut mailbox_activity_rx = self.subscribe_subagent_mailbox_activity();
        let mut parent_input_activity_rx = self.subscribe_parent_input_activity();
        let mut waiting_logged = false;
        task_runtime_trace(
            "subagent_wait_agent_start",
            format!("parent={current_session_id} timeout_ms={timeout_ms}"),
        );
        loop {
            if self.take_parent_input_activity_for_parent(parent_thread_id)? {
                let result = build_wait_agent_steered_tool_result(current_session_id, timeout_ms);
                task_runtime_trace(
                    "subagent_wait_agent_steered",
                    format!(
                        "parent={current_session_id} source=queued_parent_input elapsed_ms={}",
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                return Ok(result);
            }
            if self.has_subagent_events_for_parent(parent_thread_id)? {
                let result =
                    build_wait_agent_activity_tool_result(current_session_id, timeout_ms, false);
                task_runtime_trace(
                    "subagent_wait_agent_return",
                    format!(
                        "parent={current_session_id} source=queued_mailbox elapsed_ms={}",
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                return Ok(result);
            }
            if tokio::time::Instant::now() >= deadline {
                let result =
                    build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true);
                task_runtime_trace(
                    "subagent_wait_agent_timeout",
                    format!(
                        "parent={current_session_id} elapsed_ms={}",
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
                        "parent={current_session_id} elapsed_ms={}",
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            tokio::select! {
                changed = mailbox_activity_rx.changed() => {
                    if changed.is_err() {
                        let result = build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true);
                        task_runtime_trace(
                            "subagent_wait_agent_timeout",
                            format!(
                                "parent={current_session_id} source=mailbox_closed elapsed_ms={}",
                                current_unix_millis_u64().saturating_sub(trace_started_at)
                            ),
                        );
                        return Ok(result);
                    }
                }
                changed = parent_input_activity_rx.changed() => {
                    if changed.is_ok() && self.take_parent_input_activity_for_parent(parent_thread_id)? {
                        let result = build_wait_agent_steered_tool_result(current_session_id, timeout_ms);
                        task_runtime_trace(
                            "subagent_wait_agent_steered",
                            format!(
                                "parent={current_session_id} elapsed_ms={}",
                                current_unix_millis_u64().saturating_sub(trace_started_at)
                            ),
                        );
                        return Ok(result);
                    }
                    if changed.is_err() {
                        let result = build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true);
                        task_runtime_trace(
                            "subagent_wait_agent_timeout",
                            format!(
                                "parent={current_session_id} source=parent_input_closed elapsed_ms={}",
                                current_unix_millis_u64().saturating_sub(trace_started_at)
                            ),
                        );
                        return Ok(result);
                    }
                }
                _ = tokio::time::sleep(remaining) => {
                    let result = build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true);
                    task_runtime_trace(
                        "subagent_wait_agent_timeout",
                        format!(
                            "parent={current_session_id} elapsed_ms={}",
                            current_unix_millis_u64().saturating_sub(trace_started_at)
                        ),
                    );
                    return Ok(result);
                }
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
            self.subagent_registry_entries_for_list_request(current_session_id, &args)?;
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
        Ok(result)
    }

    /// Reads registry entries for one list_agents request.
    /// 读取一次 list_agents 请求对应的注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent session id.
    /// `current_session_id` 是直接父会话标识。
    /// `args` carries the optional Vulcan path_prefix selector.
    /// `args` 携带可选的 Vulcan path_prefix 选择器。
    ///
    /// # Returns
    /// 返回值
    /// Matching registry entries cloned from the native registry snapshot.
    /// 从原生注册表快照克隆出的匹配条目。
    fn subagent_registry_entries_for_list_request(
        &self,
        current_session_id: SessionId,
        args: &Value,
    ) -> Result<Vec<SubagentRegistryEntry>, String> {
        let path_prefix = list_agents_path_prefix(args)?;
        let registry = self.subagent_registry_snapshot()?;
        let root_session_id = self.root_session_id_for_subagent_tree(current_session_id)?;
        let entries_by_session = registry
            .list_all()
            .into_iter()
            .map(|entry| (entry.session_id, entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut entries = entries_by_session
            .values()
            .filter_map(|entry| {
                let entry_path =
                    subagent_entry_tree_path(entry, &entries_by_session, root_session_id)?;
                let matches_prefix = path_prefix
                    .as_deref()
                    .is_none_or(|prefix| subagent_tree_path_matches_prefix(&entry_path, prefix));
                matches_prefix.then_some((entry_path, entry.clone()))
            })
            .collect::<Vec<_>>();
        entries.sort_by(|(left_path, left_entry), (right_path, right_entry)| {
            left_path
                .cmp(right_path)
                .then_with(|| left_entry.agent_id.cmp(&right_entry.agent_id))
        });
        Ok(entries.into_iter().map(|(_, entry)| entry).collect())
    }

    /// Resolves the root session id for one Vulcan subagent tree list request.
    /// 为一次 Vulcan 子代理树列表请求解析根会话标识。
    fn root_session_id_for_subagent_tree(
        &self,
        current_session_id: SessionId,
    ) -> Result<SessionNodeId, String> {
        Ok(self
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot resolve the root session.",
                )
            })?
            .get(&current_session_id)
            .map(|record| SessionNodeId::from(record.origin_session_id))
            .unwrap_or_else(|| SessionNodeId::from(current_session_id)))
    }

    /// Interrupts one known child agent selected by a Vulcan target.
    /// 按 Vulcan target 中断一个已知子代理。
    pub async fn invoke_interrupt_agent(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let target = interrupt_agent_target(&args)?;
        let normalized_target = normalize_public_task_identifier(&target)?;
        if is_root_agent_target(&target) {
            return Err(String::from("root is not a spawned agent."));
        }
        if self.current_session_matches_public_target(current_session_id, &normalized_target)? {
            return Err(String::from(
                "An agent cannot interrupt itself; return your result and let the parent interrupt you if needed.",
            ));
        }
        let trace_started_at = current_unix_millis_u64();
        task_runtime_trace(
            "subagent_interrupt_agent_start",
            format!("parent={current_session_id}"),
        );
        let parent_session_id = SessionNodeId::from(current_session_id);
        let entry = self
            .subagent_registry_snapshot()?
            .get(&normalized_target)
            .filter(|entry| entry.parent_session_id == parent_session_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Unknown child agent `{normalized_target}` for parent session `{current_session_id}`."
                )
            })?;
        let previous_status = entry.status;
        let stopped_task_ids = self.stop_task_tree(SessionId::from(entry.session_id))?;
        let result = VulcanToolInvocationResult::from_model_output(json!({
            "tool": INTERRUPT_AGENT_TOOL,
            "previous_status": previous_status,
        })).with_host_result(VulcanToolHostResult {
            kind: String::from("subagent_control"),
            payload: json!({
                "tool": INTERRUPT_AGENT_TOOL,
                "parent_session_id": current_session_id.to_string(),
                "matched_agents": [subagent_registry_entry_status_payload(&entry)],
                (STOPPED_SUBAGENT_TASK_IDS_KEY): stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "stopped_count": stopped_task_ids.len(),
            }),
        });
        task_runtime_trace(
            "subagent_interrupt_agent_return",
            format!(
                "parent={current_session_id} source=registry stopped_count={} elapsed_ms={}",
                stopped_task_ids.len(),
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
        Ok(result)
    }

    /// Returns whether the current session is itself the named child-agent target.
    /// 返回当前会话自身是否就是指定的子代理目标。
    fn current_session_matches_public_target(
        &self,
        current_session_id: SessionId,
        normalized_target: &str,
    ) -> Result<bool, String> {
        Ok(self
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot inspect the current agent.",
                )
            })?
            .get(&current_session_id)
            .is_some_and(|record| record.public_task_id == normalized_target))
    }
}
