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
                if let Some(visible_error) =
                    handle_prestart_cancelled_backend_error(self, &invocation, &error)?
                {
                    return Err(visible_error);
                }
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
                if let Some(visible_error) =
                    handle_prestart_cancelled_backend_error(self, &invocation, &error)?
                {
                    return Err(visible_error);
                }
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
        let queued_message_counts = registry_entries
            .iter()
            .map(|entry| {
                self.inter_agent_message_count_for_task(SessionId::from(entry.session_id))
                    .map(|count| (entry.agent_id.clone(), count))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let result = build_subagent_registry_status_tool_result(
            "list_agents",
            current_session_id,
            registry_entries,
            &queued_message_counts,
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

    /// Returns WebUI-visible open child-agent registry entries using the same semantics as list_agents.
    /// 使用与 list_agents 相同的语义返回 WebUI 可见的未关闭子代理注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the selected root or direct parent session id for the requested tree.
    /// `current_session_id` 是请求树对应的选中根会话或直接父会话标识。
    ///
    /// # Returns
    /// 返回值
    /// Registry entries that are still open and visible to normal subagent orchestration.
    /// 仍处于未关闭状态且对常规子代理调度可见的注册表条目。
    pub fn open_subagent_registry_entries_for_parent_tree(
        &self,
        current_session_id: SessionId,
    ) -> Result<Vec<SubagentRegistryEntry>, String> {
        self.subagent_registry_entries_for_list_request(current_session_id, &json!({}))
    }

    /// Returns read-only subagent orchestration information for model planning.
    /// 返回供模型规划使用的只读子代理调度信息。
    pub async fn invoke_agent_orchestration_info(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let request = agent_orchestration_info_request(&args)?;
        let include_agents = request.includes("agents");
        let include_models = request.includes("models");
        let include_modes = request.includes("modes");
        let include_runtime = request.includes("runtime");
        let model_context = self.execution_context_for_session(current_session_id);
        let agents = if include_agents {
            self.agent_orchestration_info_agents(&request, model_context.as_ref())?
        } else {
            Vec::new()
        };
        let models = if include_models {
            model_context
                .as_ref()
                .map(|context| {
                    let focused_model = self
                        .agent_orchestration_info_agent_fixed_model_selector(
                            &request,
                            &context.config,
                        )
                        .ok()
                        .flatten();
                    agent_orchestration_info_models(&context.config, focused_model.as_ref())
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let session_model = model_context
            .as_ref()
            .and_then(|context| agent_orchestration_session_model(&context.config))
            .unwrap_or(Value::Null);
        let agent_fixed_model = model_context
            .as_ref()
            .and_then(|context| {
                self.agent_orchestration_info_agent_fixed_model(&request, &context.config)
                    .ok()
            })
            .flatten()
            .unwrap_or(Value::Null);
        let runtime = if include_runtime {
            self.agent_orchestration_info_runtime(current_session_id)?
        } else {
            Value::Null
        };
        let modes = if include_modes {
            vec![
                json!({
                    "id": "default",
                    "description": "Plain child agent. Does not inherit parent conversation turns.",
                }),
                json!({
                    "id": "mirror",
                    "description": "Mirror child agent. Inherits current conversation context and receives mirror constraints.",
                }),
                json!({
                    "id": "agent",
                    "description": "Catalog-backed agent. Requires an enabled agent_id returned by this tool.",
                }),
                json!({
                    "id": "custom",
                    "description": "One-off custom-prompt child agent. Requires system_prompt.",
                }),
            ]
        } else {
            Vec::new()
        };
        let payload = json!({
            "tool": AGENT_ORCHESTRATION_INFO_TOOL,
            "parent_session_id": current_session_id.to_string(),
            "agents": agents,
            "models": models,
            "session_model": session_model,
            "agent_fixed_model": agent_fixed_model,
            "modes": modes,
            "runtime": runtime,
        });
        Ok(
            VulcanToolInvocationResult::from_model_output(payload.clone()).with_host_result(
                VulcanToolHostResult {
                    kind: String::from("subagent_orchestration_info"),
                    payload,
                },
            ),
        )
    }

    /// Builds the agent list section for agent_orchestration_info.
    /// 构建 agent_orchestration_info 的智能体列表分区。
    fn agent_orchestration_info_agents(
        &self,
        request: &AgentOrchestrationInfoRequest,
        model_context: Option<&TaskExecutionRuntimeContext>,
    ) -> Result<Vec<Value>, String> {
        let query = request.query.as_deref().map(str::to_ascii_lowercase);
        let catalog = self.subagent_catalog_snapshot()?;
        let mut agents = catalog
            .definitions
            .into_iter()
            .filter(|definition| definition.enabled)
            .filter(|definition| {
                if let Some(agent_id) = request.agent_id.as_deref()
                    && definition.id != agent_id
                {
                    return false;
                }
                let Some(query) = query.as_deref() else {
                    return true;
                };
                agent_definition_matches_query(definition, query)
            })
            .map(|definition| {
                json!({
                    "agent_id": definition.id,
                    "name": definition.name,
                    "description": definition.description,
                    "category": definition.source.category_id,
                    "enabled": definition.enabled,
                    "source": subagent_definition_scope_key(definition.source.scope),
                    "model_selection": subagent_definition_model_selection(
                        &definition,
                        model_context.map(|context| &context.config),
                    ),
                })
            })
            .collect::<Vec<_>>();
        agents.sort_by(|left, right| {
            left["agent_id"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["agent_id"].as_str().unwrap_or_default())
        });
        Ok(agents)
    }

    /// Builds the requested agent fixed-model section for agent_orchestration_info.
    /// 构建 agent_orchestration_info 请求的智能体固定模型分区。
    fn agent_orchestration_info_agent_fixed_model(
        &self,
        request: &AgentOrchestrationInfoRequest,
        session_config: &ProviderConfigurationFile,
    ) -> Result<Option<Value>, String> {
        let Some(definition) = self.agent_orchestration_info_agent_definition(request)? else {
            return Ok(None);
        };
        Ok(Some(subagent_definition_model_selection(
            &definition,
            Some(session_config),
        )))
    }

    /// Resolves the focused agent fixed-model selector for agent_orchestration_info.
    /// 解析 agent_orchestration_info 聚焦智能体的固定模型选择器。
    fn agent_orchestration_info_agent_fixed_model_selector(
        &self,
        request: &AgentOrchestrationInfoRequest,
        session_config: &ProviderConfigurationFile,
    ) -> Result<Option<(String, String)>, String> {
        let Some(definition) = self.agent_orchestration_info_agent_definition(request)? else {
            return Ok(None);
        };
        Ok(provider_model_selector_from_snapshot(
            &definition.provider_model,
            session_config,
        ))
    }

    /// Resolves the focused agent definition for agent_orchestration_info diagnostics.
    /// 解析 agent_orchestration_info 诊断聚焦的智能体定义。
    fn agent_orchestration_info_agent_definition(
        &self,
        request: &AgentOrchestrationInfoRequest,
    ) -> Result<Option<SubagentDefinition>, String> {
        let Some(agent_id) = request.agent_id.as_deref() else {
            return Ok(None);
        };
        let catalog = self.subagent_catalog_snapshot()?;
        Ok(catalog
            .definitions
            .into_iter()
            .find(|definition| definition.id == agent_id && definition.enabled))
    }

    /// Builds the runtime summary section for agent_orchestration_info.
    /// 构建 agent_orchestration_info 的运行时摘要分区。
    fn agent_orchestration_info_runtime(
        &self,
        current_session_id: SessionId,
    ) -> Result<Value, String> {
        let parent_session_id = SessionNodeId::from(current_session_id);
        let registry = self.subagent_registry_snapshot()?;
        let open_agents = registry
            .list_for_parent(parent_session_id)
            .into_iter()
            .filter(|entry| entry.status != SubagentStatus::Archived)
            .collect::<Vec<_>>();
        let running_agent_count = open_agents
            .iter()
            .filter(|entry| entry.status.is_running())
            .count();
        let queued_message_count =
            self.queued_inter_agent_message_count_for_parent(current_session_id)?;
        Ok(json!({
            "open_agent_count": open_agents.len(),
            "running_agent_count": running_agent_count,
            "queued_message_count": queued_message_count,
        }))
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
        let include_closed = list_agents_include_closed(args)?;
        let registry = self.subagent_registry_snapshot()?;
        let root_session_id = self.root_session_id_for_subagent_tree(current_session_id)?;
        let resident_session_ids = self.resident_subagent_session_ids_for_list()?;
        let entries_by_session = registry
            .list_all()
            .into_iter()
            .map(|entry| (entry.session_id, entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut entries = entries_by_session
            .values()
            .filter(|entry| {
                let is_resident = resident_session_ids.contains(&entry.session_id);
                let is_archived = entry.status == SubagentStatus::Archived;
                let is_restored_stopped = entry.status == SubagentStatus::Stopped;
                is_resident && !is_archived
                    || include_closed && is_archived
                    || !is_resident && is_restored_stopped
            })
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

    /// Closes one known child agent selected by a Vulcan target.
    /// 按 Vulcan target 关闭一个已知子代理。
    pub async fn invoke_close_agent(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let target = close_agent_target(&args)?;
        let cascade = close_agent_cascade(&args)?;
        let normalized_target = normalize_public_task_identifier(&target)?;
        if is_root_agent_target(&target) {
            return Err(String::from("root is not a spawned agent."));
        }
        if self.current_session_matches_public_target(current_session_id, &normalized_target)? {
            return Err(String::from(
                "An agent cannot close itself; return your result and let the parent close you if needed.",
            ));
        }
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
        if entry.status == SubagentStatus::Archived {
            return Err(format!(
                "Child agent `{normalized_target}` is already closed and cannot be closed again."
            ));
        }
        let previous_status = entry.status;
        let root_session_id = self.root_session_id_for_subagent_tree(current_session_id)?;
        let stopped_task_ids = self.stop_task_tree(SessionId::from(entry.session_id))?;
        let archived_entries =
            self.archive_subagent_registry_entries(&normalized_target, entry.session_id, cascade)?;
        let archived_task_ids = archived_entries
            .iter()
            .map(|entry| SessionId::from(entry.session_id))
            .collect::<Vec<_>>();
        self.mark_archived_tasks_in_sessions(&archived_task_ids)?;
        self.persist_archived_subagent_registry_entries(
            Uuid::from(root_session_id),
            &archived_entries,
        )?;
        let cleared_message_count =
            self.clear_inter_agent_messages_for_tasks(&archived_task_ids)?;
        let result = VulcanToolInvocationResult::from_model_output(json!({
            "tool": CLOSE_AGENT_TOOL,
            "target": normalized_target,
            "previous_status": previous_status,
            "closed": true,
            "closed_agents": archived_entries.iter().map(|entry| entry.agent_id.clone()).collect::<Vec<_>>(),
            "closed_count": archived_entries.len(),
            "cleared_message_count": cleared_message_count,
        }))
        .with_host_result(VulcanToolHostResult {
            kind: String::from("subagent_control"),
            payload: json!({
                "tool": CLOSE_AGENT_TOOL,
                "parent_session_id": current_session_id.to_string(),
                "matched_agents": archived_entries.iter().map(|entry| {
                    subagent_registry_entry_host_status_payload(entry, 0)
                }).collect::<Vec<_>>(),
                "closed_agents": archived_entries.iter().map(|entry| entry.agent_id.clone()).collect::<Vec<_>>(),
                "closed_count": archived_entries.len(),
                (STOPPED_SUBAGENT_TASK_IDS_KEY): stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "stopped_count": stopped_task_ids.len(),
                "closed": true,
                "cascade": cascade,
                "cleared_message_count": cleared_message_count,
            }),
        });
        Ok(result)
    }

    /// Marks one subagent registry subtree as archived.
    /// 将一个子代理注册表子树标记为已归档。
    fn archive_subagent_registry_entries(
        &self,
        agent_id: &str,
        target_session_id: SessionNodeId,
        cascade: bool,
    ) -> Result<Vec<SubagentRegistryEntry>, String> {
        let mut registry = self.subagent_registry.write().map_err(|_| {
            String::from("The subagent registry is locked and cannot close agents.")
        })?;
        let agent_ids =
            subagent_registry_close_agent_ids(&registry, agent_id, target_session_id, cascade);
        let mut archived_entries = Vec::new();
        let archived_at = current_unix_millis_u64();
        for agent_id in agent_ids {
            let Some(entry) = registry.get_mut(&agent_id) else {
                continue;
            };
            if entry.status == SubagentStatus::Archived {
                continue;
            }
            entry.set_status(SubagentStatus::Archived, archived_at);
            entry.stop_reason = Some(String::from("close_agent"));
            archived_entries.push(entry.clone());
        }
        Ok(archived_entries)
    }

    /// Returns in-memory child sessions that are still resident for list_agents.
    /// 返回 `list_agents` 仍可见的内存常驻子会话集合。
    ///
    /// # Returns
    /// 返回值
    /// Child session node ids currently retained by the runtime session registry.
    /// 当前仍由运行时会话注册表保留的子会话节点标识。
    fn resident_subagent_session_ids_for_list(&self) -> Result<BTreeSet<SessionNodeId>, String> {
        Ok(self
            .sessions
            .read()
            .map_err(|_| {
                String::from("The task-session registry is locked and cannot list resident agents.")
            })?
            .keys()
            .copied()
            .map(SessionNodeId::from)
            .collect())
    }

    /// Resolves the root session id for one Vulcan subagent tree list request.
    /// 为一次 Vulcan 子代理树列表请求解析根会话标识。
    pub(super) fn root_session_id_for_subagent_tree(
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
        let queued_message_count =
            self.inter_agent_message_count_for_task(SessionId::from(entry.session_id))?;
        let stopped_task_ids = self.stop_task_tree(SessionId::from(entry.session_id))?;
        let result = VulcanToolInvocationResult::from_model_output(json!({
            "tool": INTERRUPT_AGENT_TOOL,
            "previous_status": previous_status,
        })).with_host_result(VulcanToolHostResult {
            kind: String::from("subagent_control"),
            payload: json!({
                "tool": INTERRUPT_AGENT_TOOL,
                "parent_session_id": current_session_id.to_string(),
                "matched_agents": [subagent_registry_entry_host_status_payload(&entry, queued_message_count)],
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

/// Converts a pre-start backend cancellation into a visible error after discarding the unstarted placeholder.
/// 丢弃未启动占位后，将启动前后端取消转换为对外可见错误。
fn handle_prestart_cancelled_backend_error(
    task_runtime: &TaskRuntime,
    invocation: &TaskExecutionRequest,
    error: &str,
) -> Result<Option<String>, String> {
    if !task_execution_error_is_prestart_cancelled(error) {
        return Ok(None);
    }
    let visible_error = task_execution_visible_error(error).to_string();
    task_runtime
        .discard_prestarted_task_record(invocation)
        .map_err(|cleanup_error| {
            format!(
                "{visible_error} The pre-start child-agent placeholder could not be discarded: {cleanup_error}"
            )
        })?;
    Ok(Some(visible_error))
}

/// Collects registry agent ids that should be closed for one close_agent request.
/// 收集一次 close_agent 请求应关闭的注册表智能体标识。
fn subagent_registry_close_agent_ids(
    registry: &SubagentRegistry,
    agent_id: &str,
    target_session_id: SessionNodeId,
    cascade: bool,
) -> Vec<String> {
    if !cascade {
        return vec![agent_id.to_string()];
    }
    let mut session_ids = BTreeSet::from([target_session_id]);
    loop {
        let previous_len = session_ids.len();
        for entry in registry.list_all() {
            if session_ids.contains(&entry.parent_session_id) {
                session_ids.insert(entry.session_id);
            }
        }
        if session_ids.len() == previous_len {
            break;
        }
    }
    registry
        .list_all()
        .into_iter()
        .filter(|entry| session_ids.contains(&entry.session_id))
        .map(|entry| entry.agent_id.clone())
        .collect()
}

/// Returns whether one subagent definition matches a lowercase query.
/// 返回单个子代理定义是否匹配小写查询词。
fn agent_definition_matches_query(definition: &SubagentDefinition, query: &str) -> bool {
    let category = definition.source.category_id.as_deref().unwrap_or_default();
    [
        definition.id.as_str(),
        definition.name.as_str(),
        definition.description.as_deref().unwrap_or_default(),
        category,
    ]
    .into_iter()
    .any(|value| value.to_ascii_lowercase().contains(query))
}

/// Converts one subagent definition scope into a stable display key.
/// 将单个子代理定义作用域转换为稳定展示键。
fn subagent_definition_scope_key(
    scope: vulcan_task_session::SubagentDefinitionScope,
) -> &'static str {
    match scope {
        vulcan_task_session::SubagentDefinitionScope::System => "system",
        vulcan_task_session::SubagentDefinitionScope::User => "user",
        vulcan_task_session::SubagentDefinitionScope::Package => "package",
    }
}

/// Builds model-selection summary for one subagent definition.
/// 构建单个子代理定义的模型选择摘要。
fn subagent_definition_model_selection(
    definition: &SubagentDefinition,
    session_config: Option<&ProviderConfigurationFile>,
) -> Value {
    let Some(session_config) = session_config else {
        return match provider_model_selector_from_snapshot(
            &definition.provider_model,
            &ProviderConfigurationFile::default(),
        ) {
            Some((provider_id, model_id)) => json!({
                "mode": "fixed",
                "provider_id": provider_id,
                "model_id": model_id,
                "selector": format!("{provider_id}/{model_id}"),
                "available": Value::Null,
                "unavailable_reason": Value::Null,
            }),
            None => json!({
                "mode": "session",
            }),
        };
    };
    let Some((provider_id, model_id)) =
        provider_model_selector_from_snapshot(&definition.provider_model, session_config)
    else {
        return json!({
            "mode": "session",
        });
    };
    let available = provider_model_selector_is_available(session_config, &provider_id, &model_id);
    json!({
        "mode": "fixed",
        "provider_id": provider_id,
        "model_id": model_id,
        "selector": format!("{provider_id}/{model_id}"),
        "available": available,
        "unavailable_reason": if available {
            Value::Null
        } else {
            Value::String(String::from("The configured fixed model is not in the current effective enabled model choices; spawn_agent will fall back to the session model when no explicit model is provided."))
        },
    })
}

/// Builds selectable model rows from the current provider configuration.
/// 从当前供应商配置构建可选模型行。
///
/// # Parameters
/// 参数
/// `session_config` is the current provider configuration snapshot.
/// `session_config` 是当前供应商配置快照。
///
/// # Returns
/// 返回值
/// Model rows whose selector can be passed to spawn_agent.model.
/// 可传给 spawn_agent.model 的模型行。
pub(super) fn agent_orchestration_info_models(
    session_config: &ProviderConfigurationFile,
    agent_fixed_model: Option<&(String, String)>,
) -> Vec<Value> {
    let mut models = Vec::new();
    let session_model = session_config
        .model
        .as_deref()
        .and_then(crate::provider_config::parse_active_model_reference)
        .map(|(provider_id, model_id)| (provider_id.to_string(), model_id.to_string()));
    let provider_ids = agent_orchestration_info_model_provider_ids(
        session_config,
        session_model.as_ref(),
        agent_fixed_model,
    );
    for provider_id in provider_ids {
        let provider_name = crate::provider_config::supported_provider_by_id(provider_id)
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| provider_id.to_string());
        for model in crate::provider_config::effective_model_choices(provider_id, session_config) {
            let selector_pair = (provider_id.to_string(), model.id.clone());
            models.push(json!({
                "selector": format!("{provider_id}/{}", model.id),
                "provider_id": provider_id,
                "provider_name": provider_name,
                "model_id": model.id,
                "model_name": model.name,
                "context_limit": model.context_limit,
                "supports_tool_call": model.tool_call,
                "supports_reasoning": model.reasoning,
                "is_session_model": session_model.as_ref() == Some(&selector_pair),
                "is_agent_fixed_model": agent_fixed_model == Some(&selector_pair),
                "available": true,
                "unavailable_reason": Value::Null,
            }));
        }
    }
    models.sort_by(|left, right| {
        left["selector"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["selector"].as_str().unwrap_or_default())
    });
    models
}

/// Builds the provider-id set that can contribute model selectors for orchestration planning.
/// 构建可为子代理调度规划贡献模型选择器的供应商标识集合。
///
/// # Parameters
/// 参数
/// `session_config` is the current provider configuration snapshot.
/// `session_config` 是当前供应商配置快照。
/// `session_model` is the active session model selector, when configured.
/// `session_model` 是当前会话模型选择器（如果已配置）。
/// `agent_fixed_model` is the focused agent fixed model selector, when configured.
/// `agent_fixed_model` 是聚焦智能体固定模型选择器（如果已配置）。
///
/// # Returns
/// 返回值
/// Sorted provider ids that should be scanned for effective model choices.
/// 应扫描有效模型选项的有序供应商标识。
fn agent_orchestration_info_model_provider_ids<'a>(
    session_config: &'a ProviderConfigurationFile,
    session_model: Option<&'a (String, String)>,
    agent_fixed_model: Option<&'a (String, String)>,
) -> BTreeSet<&'a str> {
    let mut provider_ids = session_config
        .provider
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if let Some((provider_id, _)) = session_model {
        provider_ids.insert(provider_id.as_str());
    }
    if let Some((provider_id, _)) = agent_fixed_model {
        provider_ids.insert(provider_id.as_str());
    }
    provider_ids
}

/// Builds the current session model row when it is available.
/// 在当前会话模型可用时构建模型行。
///
/// # Parameters
/// 参数
/// `session_config` is the current provider configuration snapshot.
/// `session_config` 是当前供应商配置快照。
///
/// # Returns
/// 返回值
/// A compact session model payload, or None when no active model is configured.
/// 紧凑的会话模型载荷；没有配置激活模型时返回 None。
fn agent_orchestration_session_model(session_config: &ProviderConfigurationFile) -> Option<Value> {
    let active_model = session_config.model.as_deref()?;
    let (provider_id, model_id) =
        crate::provider_config::parse_active_model_reference(active_model)?;
    let available = provider_model_selector_is_available(session_config, provider_id, model_id);
    Some(json!({
        "selector": format!("{provider_id}/{model_id}"),
        "provider_id": provider_id,
        "model_id": model_id,
        "available": available,
    }))
}
