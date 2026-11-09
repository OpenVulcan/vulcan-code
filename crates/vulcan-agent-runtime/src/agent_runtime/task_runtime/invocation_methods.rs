use super::*;

impl TaskRuntime {
    /// Executes one spawn-agent-backed subagent invocation through the backend registered for the current controller session.
    /// 通过为当前控制器会话注册的后端执行一次由 spawn_agent 承载的 TaskSession 调用。
    pub async fn invoke_task(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        // backend is the exact handler registered for the current controller session.
        // backend 是为当前控制器会话注册的精确处理器。
        let backend = self
            .backend_for_session(current_session_id)?
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
        lifecycle: Option<collaboration_lifecycle::ChildTaskInvocation>,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        // backend is the exact handler registered for the current controller session.
        // backend 是为当前控制器会话注册的精确处理器。
        let backend = self
                .backend_for_session(current_session_id)?
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
        // Register the original trusted tool identity before the backend may publish its start event.
        // 在后端可能发布启动事件之前登记原始可信工具身份。
        let registration = if let Some(lifecycle) = lifecycle {
            if lifecycle.invocation.session_id != current_session_id.to_string() {
                return Err(String::from(
                    "spawn_agent lifecycle source does not match the parent session",
                ));
            }
            let operation_id =
                vulcan_session_core::v1::collaboration::CollaborationOperationId::try_new(
                    crate::tooling::collaboration::stable_collaboration_operation_id(
                        &lifecycle.invocation,
                        SPAWN_AGENT_TOOL,
                    ),
                )
                .map_err(|error| error.to_string())?;
            self.register_child_task_lifecycle_binding(ChildTaskLifecycleBinding {
                port: lifecycle.port,
                reservation: collaboration_lifecycle::ChildTaskReservationRequest {
                    invocation: lifecycle.invocation,
                    operation_id,
                    execution: collaboration_lifecycle::ChildTaskExecutionIdentity {
                        task_session_id: invocation.task_id,
                        execution_id: invocation.execution_id.clone(),
                        task_id: vulcan_session_core::v1::ids::SubagentTaskId::try_new(
                            invocation.task_id.to_string(),
                        )
                        .map_err(|error| error.to_string())?,
                    },
                    reserved_at_ms: current_unix_millis_u64().try_into().map_err(|_| {
                        String::from("child reservation timestamp exceeds the durable range")
                    })?,
                },
            })?;
            Some(collaboration_lifecycle::ChildBindingRegistrationGuard {
                runtime: self.clone(),
                execution_id: invocation.execution_id.clone(),
            })
        } else {
            None
        };
        let backend_result = backend(invocation.clone()).await;
        let backend_result = if registration.is_some()
            && self
                .take_child_task_lifecycle_binding(&invocation.execution_id)?
                .is_some()
        {
            // Preserve backend failures, including the marker used to discard pre-start placeholders.
            // 保留后端失败，包括用于丢弃启动前占位的标记。
            // Only a claimed-success mismatch needs a new ownership error.
            // 只有未认领却声称成功时才需要新的归属错误。
            match backend_result {
                Err(error) => Err(error),
                Ok(_) => Err(String::from(
                    "The task backend exited without claiming its child lifecycle binding.",
                )),
            }
        } else {
            backend_result
        };
        let outcome = match backend_result {
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
        let target_selectors = wait_agent_target_selectors(&args)?;
        let explicit_targets = !target_selectors.is_empty();
        let wait_target_session_ids =
            self.wait_agent_target_session_ids(current_session_id, &target_selectors)?;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        let parent_thread_id = SessionNodeId::from(current_session_id);
        let mut mailbox_activity_rx = self.subscribe_subagent_mailbox_activity();
        let mut parent_interruption_rx = self.subscribe_wait_agent_parent_interruption();
        let mut waiting_logged = false;
        let mut collected_events = Vec::new();
        let mut satisfied_target_session_ids = BTreeSet::new();
        task_runtime_trace(
            "subagent_wait_agent_start",
            format!(
                "parent={current_session_id} timeout_ms={timeout_ms} explicit_targets={explicit_targets} target_count={}",
                wait_target_session_ids.len()
            ),
        );
        loop {
            if self.take_wait_agent_parent_interruption_for_parent(parent_thread_id)? {
                let result = build_wait_agent_steered_tool_result(current_session_id, timeout_ms);
                task_runtime_trace(
                    "subagent_wait_agent_steered",
                    format!(
                        "parent={current_session_id} source=parent_interruption elapsed_ms={}",
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                return Ok(result);
            }
            let actionable_events =
                self.take_matching_subagent_events_for_parent(parent_thread_id, |event| {
                    wait_agent_mailbox_event_is_actionable(event.kind)
                        && (!explicit_targets
                            || wait_target_session_ids.contains(&event.child_thread_id))
                })?;
            if !actionable_events.is_empty() {
                for event in actionable_events {
                    satisfied_target_session_ids.insert(event.child_thread_id);
                    collected_events.push(event);
                }
            }
            // Every wait owns its target observation, even when another waiter claims the mailbox first.
            // 即使另一等待调用先认领 mailbox，每个等待仍拥有对其目标的状态观察责任。
            let cached_terminal_events = self.wait_agent_cached_terminal_events(
                current_session_id,
                &wait_target_session_ids,
                &satisfied_target_session_ids,
            )?;
            for event in cached_terminal_events {
                satisfied_target_session_ids.insert(event.child_thread_id);
                collected_events.push(event);
            }
            if !explicit_targets
                && collected_events.is_empty()
                && self
                    .wait_agent_running_target_session_ids(current_session_id)?
                    .is_empty()
            {
                let result =
                    build_wait_agent_no_active_agents_tool_result(current_session_id, timeout_ms);
                task_runtime_trace(
                    "subagent_wait_agent_return",
                    format!(
                        "parent={current_session_id} source=no_active_agents elapsed_ms={}",
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                return Ok(result);
            }
            let wait_complete = !collected_events.is_empty()
                && (!explicit_targets
                    || wait_target_session_ids.is_subset(&satisfied_target_session_ids));
            if wait_complete {
                let result = build_wait_agent_activity_tool_result(
                    current_session_id,
                    timeout_ms,
                    false,
                    collected_events.as_slice(),
                );
                task_runtime_trace(
                    "subagent_wait_agent_return",
                    format!(
                        "parent={current_session_id} source=queued_mailbox events={} elapsed_ms={}",
                        collected_events.len(),
                        current_unix_millis_u64().saturating_sub(trace_started_at)
                    ),
                );
                return Ok(result);
            }
            if tokio::time::Instant::now() >= deadline {
                let result = build_wait_agent_activity_tool_result(
                    current_session_id,
                    timeout_ms,
                    true,
                    collected_events.as_slice(),
                );
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
                        let result = build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true, collected_events.as_slice());
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
                changed = parent_interruption_rx.changed() => {
                    if changed.is_ok() && self.take_wait_agent_parent_interruption_for_parent(parent_thread_id)? {
                        let result = build_wait_agent_steered_tool_result(current_session_id, timeout_ms);
                        task_runtime_trace(
                            "subagent_wait_agent_steered",
                            format!(
                                "parent={current_session_id} source=parent_interruption elapsed_ms={}",
                                current_unix_millis_u64().saturating_sub(trace_started_at)
                            ),
                        );
                        return Ok(result);
                    }
                    if changed.is_err() {
                        let result = build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true, collected_events.as_slice());
                        task_runtime_trace(
                            "subagent_wait_agent_timeout",
                            format!(
                                "parent={current_session_id} source=parent_interruption_closed elapsed_ms={}",
                                current_unix_millis_u64().saturating_sub(trace_started_at)
                            ),
                        );
                        return Ok(result);
                    }
                }
                _ = tokio::time::sleep(remaining) => {
                    let result = build_wait_agent_activity_tool_result(current_session_id, timeout_ms, true, collected_events.as_slice());
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

    /// Resolves wait_agent targets to child session ids owned by the current parent.
    /// 将 wait_agent 目标解析为当前父级拥有的子会话标识。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent session that invoked wait_agent.
    /// `current_session_id` 是调用 wait_agent 的直接父会话。
    /// `target_selectors` are explicit model-provided agent ids, or empty for live-agent default waiting.
    /// `target_selectors` 是模型显式提供的智能体标识；为空时表示默认等待运行中的智能体。
    ///
    /// # Returns
    /// 返回值
    /// Session ids whose mailbox events may satisfy this wait call.
    /// 可满足本次等待调用的会话标识集合。
    fn wait_agent_target_session_ids(
        &self,
        current_session_id: SessionId,
        target_selectors: &[String],
    ) -> Result<BTreeSet<SessionNodeId>, String> {
        if target_selectors.is_empty() {
            return self.wait_agent_running_target_session_ids(current_session_id);
        }
        self.wait_agent_explicit_target_session_ids(current_session_id, target_selectors)
    }

    /// Resolves the default wait scope from open children and their active, starting, or queued execution ownership.
    /// 根据未关闭子代理及其活动、启动或排队执行所有权解析默认等待范围。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent session that invoked wait_agent.
    /// `current_session_id` 是调用 wait_agent 的直接父会话。
    ///
    /// # Returns
    /// 返回值
    /// Running child session ids that may produce the next relevant mailbox event.
    /// 可能产生下一条相关 mailbox 事件的运行中子会话标识。
    fn wait_agent_running_target_session_ids(
        &self,
        current_session_id: SessionId,
    ) -> Result<BTreeSet<SessionNodeId>, String> {
        let parent_session_id = SessionNodeId::from(current_session_id);
        // session_ids includes admitted successors whose previous registry outcome is still terminal.
        // session_ids 包含已准入但注册表仍保留上一执行终态的后继任务。
        let mut session_ids = BTreeSet::new();
        for entry in self
            .subagent_registry_snapshot()?
            .list_for_parent(parent_session_id)
        {
            if entry.status == SubagentStatus::Archived {
                continue;
            }
            // task_id addresses the same authoritative ownership and inbox used by cached-terminal checks.
            // task_id 定位与缓存终态检查相同的权威运行所有权及 inbox。
            let task_id = SessionId::from(entry.session_id);
            if entry.status.is_terminal() {
                let returns = self
                    .durable_return_mailboxes
                    .read()
                    .map_err(|_| "The durable return registry is locked.")?;
                if let Some((_, cached)) = returns.get(&task_id)
                    && cached.is_none()
                {
                    session_ids.insert(entry.session_id);
                }
            }
            if entry.status.is_running()
                || self.task_has_active_or_starting_run(task_id)?
                || self.inter_agent_message_count_for_task(task_id)? > 0
            {
                session_ids.insert(entry.session_id);
            }
        }
        Ok(session_ids)
    }

    /// Resolves explicit wait_agent selectors against open child agents.
    /// 根据未关闭子智能体解析显式 wait_agent 选择器。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent session that invoked wait_agent.
    /// `current_session_id` 是调用 wait_agent 的直接父会话。
    /// `target_selectors` are canonical child-agent ids.
    /// `target_selectors` 是规范化后的子智能体标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact child session ids requested by the model.
    /// 模型请求的精确子会话标识集合。
    fn wait_agent_explicit_target_session_ids(
        &self,
        current_session_id: SessionId,
        target_selectors: &[String],
    ) -> Result<BTreeSet<SessionNodeId>, String> {
        let parent_session_id = SessionNodeId::from(current_session_id);
        let registry = self.subagent_registry_snapshot()?;
        let mut session_ids = BTreeSet::new();
        for target in target_selectors {
            let entry = registry
                .get_for_parent(parent_session_id, target)
                .ok_or_else(|| {
                    format!(
                        "Unknown child agent `{target}` for parent session `{current_session_id}`."
                    )
                })?;
            if entry.status == SubagentStatus::Archived {
                return Err(format!(
                    "Child agent `{target}` is already closed and cannot be waited on."
                ));
            }
            session_ids.insert(entry.session_id);
        }
        Ok(session_ids)
    }

    /// Builds immediate terminal feedback for observed children whose mailbox event was already consumed.
    /// 为 mailbox 终态事件已被消费的目标子代理构建即时终态反馈。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the direct parent that owns the requested children.
    /// `current_session_id` 是拥有目标子代理的直接父会话。
    /// `target_session_ids` are the exact child sessions requested by this wait call.
    /// `target_session_ids` 是本次等待调用明确请求的子会话集合。
    /// `satisfied_session_ids` are targets already satisfied by newly claimed mailbox events.
    /// `satisfied_session_ids` 是已经由本次新认领 mailbox 事件满足的目标集合。
    ///
    /// # Returns
    /// 返回值
    /// Synthetic terminal events backed by authoritative registry state for still-unsatisfied targets.
    /// 返回由权威注册表状态支撑、用于尚未满足目标的合成终态事件。
    pub(super) fn wait_agent_cached_terminal_events(
        &self,
        current_session_id: SessionId,
        target_session_ids: &BTreeSet<SessionNodeId>,
        satisfied_session_ids: &BTreeSet<SessionNodeId>,
    ) -> Result<Vec<SubagentMailboxEvent>, String> {
        let parent_session_id = SessionNodeId::from(current_session_id);
        let mut cached_terminal_events = Vec::new();
        for entry in self
            .subagent_registry_snapshot()?
            .list_for_parent(parent_session_id)
        {
            if !target_session_ids.contains(&entry.session_id)
                || satisfied_session_ids.contains(&entry.session_id)
                || !entry.status.is_terminal()
            {
                continue;
            }
            let task_id = SessionId::from(entry.session_id);
            // A queued or starting successor owns the next terminal result, so the previous registry terminal cannot satisfy this wait.
            // 已排队或正在启动的后继执行拥有下一条终态结果，因此旧注册表终态不能满足本次等待。
            if self.inter_agent_message_count_for_task(task_id)? > 0
                || self.task_has_active_or_starting_run(task_id)?
            {
                continue;
            }
            // Every V1 terminal remains pending until its canonical parent result is durable.
            // 所有 V1 终态在规范父结果可靠落盘之前均保持待处理。
            if entry.status.is_terminal() {
                let returns = self
                    .durable_return_mailboxes
                    .read()
                    .map_err(|_| "The durable return registry is locked.")?;
                if let Some((_, cached)) = returns.get(&task_id) {
                    if let Some(event) = cached {
                        cached_terminal_events.push(event.clone());
                    }
                    continue;
                }
            }
            // terminal_kind and status_label preserve the exact registry terminal state in the wait contract.
            // terminal_kind 与 status_label 在等待契约中保留精确的注册表终态。
            let (terminal_kind, status_label) = match entry.status {
                SubagentStatus::Completed => (SubagentMailboxEventKind::Completed, "completed"),
                SubagentStatus::Failed => (SubagentMailboxEventKind::Failed, "failed"),
                SubagentStatus::Stopped => (SubagentMailboxEventKind::Stopped, "stopped"),
                _ => unreachable!("terminal registry filter admitted a non-terminal status"),
            };
            // result_text and error_text keep successful/stopped summaries separate from failures.
            // result_text 与 error_text 将成功/停止摘要和失败错误分开。
            let (result_text, error_text) = if terminal_kind == SubagentMailboxEventKind::Failed {
                (None, entry.result_summary.clone())
            } else {
                (entry.result_summary.clone(), None)
            };
            cached_terminal_events.push(SubagentMailboxEvent {
                parent_thread_id: parent_session_id,
                parent_turn_id: entry.parent_turn_id.clone(),
                child_thread_id: entry.session_id,
                kind: terminal_kind,
                timestamp_ms: entry.updated_at_ms,
                payload: json!({
                    "child_thread_id": entry.session_id.to_string(),
                    "public_id": entry.agent_id,
                    "parent_thread_id": parent_session_id.to_string(),
                    "status": status_label,
                    "task_status": status_label,
                    "display_title": entry.agent_id,
                    "result_text": result_text,
                    "error_text": error_text,
                    "cached_terminal_result": true,
                }),
            });
        }
        Ok(cached_terminal_events)
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
            .map(|(_, entry)| {
                self.inter_agent_message_count_for_task(SessionId::from(entry.session_id))
                    .map(|count| (entry.session_id, count))
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
        self.subagent_registry_entries_for_parent_tree(current_session_id, false)
    }

    /// Lists authoritative child-agent registry entries for one parent tree with explicit closed-entry admission.
    /// 按显式的已关闭条目准入规则列出一个父级树中的权威子代理注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// `current_session_id` is the root or nested parent whose complete child tree is requested.
    /// `current_session_id` 是请求完整子级树的根会话或嵌套父会话。
    /// `include_closed` admits archived entries only when the current UI coordination round still owns them.
    /// `include_closed` 仅在当前界面协调轮次仍持有已归档条目时准入这些条目。
    ///
    /// # Returns
    /// 返回值
    /// Authoritative registry entries matching the requested open-or-visible-closed scope.
    /// 返回匹配所请求的未关闭或本轮可见已关闭范围的权威注册表条目。
    pub fn subagent_registry_entries_for_parent_tree(
        &self,
        current_session_id: SessionId,
        include_closed: bool,
    ) -> Result<Vec<SubagentRegistryEntry>, String> {
        let arguments = if include_closed {
            json!({ "include_closed": true })
        } else {
            json!({})
        };
        Ok(self
            .subagent_registry_entries_for_list_request(current_session_id, &arguments)?
            .into_iter()
            .map(|(_, entry)| entry)
            .collect())
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
        // model_context is the exact registered runtime context used by every model-facing section.
        // model_context 是全部模型相关分区共同使用的精确已注册运行时上下文。
        let model_context = self.execution_context_for_session(current_session_id)?;
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
                    agent_orchestration_info_models(context, focused_model.as_ref())
                })
                .transpose()?
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let session_model = model_context
            .as_ref()
            .map(agent_orchestration_session_model)
            .transpose()?
            .flatten()
            .unwrap_or(Value::Null);
        let agent_fixed_model = model_context
            .as_ref()
            .map(|context| self.agent_orchestration_info_agent_fixed_model(&request, context))
            .transpose()?
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
                let model_selection =
                    subagent_definition_model_selection(&definition, model_context)?;
                Ok(json!({
                    "agent_id": definition.id,
                    "name": definition.name,
                    "description": definition.description,
                    "category": definition.source.category_id,
                    "enabled": definition.enabled,
                    "source": subagent_definition_scope_key(definition.source.scope),
                    "model_selection": model_selection,
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
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
        model_context: &TaskExecutionRuntimeContext,
    ) -> Result<Option<Value>, String> {
        let Some(definition) = self.agent_orchestration_info_agent_definition(request)? else {
            return Ok(None);
        };
        Ok(Some(subagent_definition_model_selection(
            &definition,
            Some(model_context),
        )?))
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
    ) -> Result<Vec<(String, SubagentRegistryEntry)>, String> {
        let path_prefix = list_agents_path_prefix(args)?;
        let include_closed = list_agents_include_closed(args)?;
        let registry = self.subagent_registry_snapshot()?;
        let root_session_id = self.root_session_id_for_subagent_tree(current_session_id)?;
        let resident_session_ids = self.resident_subagent_session_ids_for_list()?;
        let entries = registry
            .list_tree_paths_from_root(root_session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|(_, entry)| {
                let is_resident = resident_session_ids.contains(&entry.session_id);
                let is_archived = entry.status == SubagentStatus::Archived;
                let is_restored_stopped = entry.status == SubagentStatus::Stopped;
                is_resident && !is_archived
                    || include_closed && is_archived
                    || !is_resident && is_restored_stopped
            })
            .filter_map(|(entry_path, entry)| {
                let matches_prefix = path_prefix
                    .as_deref()
                    .is_none_or(|prefix| subagent_tree_path_matches_prefix(&entry_path, prefix));
                matches_prefix.then_some((entry_path, entry.clone()))
            })
            .collect::<Vec<_>>();
        Ok(entries)
    }

    /// Closes one known child agent selected by a Vulcan target.
    /// 按 Vulcan target 关闭一个已知子代理。
    pub async fn invoke_close_agent(
        &self,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        self.invoke_close_agent_with_pre_archive(current_session_id, args, |_| Ok(()))
    }

    /// Resolves immutable child identities before the host acquires their canonical lifecycle gates.
    /// 在宿主获取规范生命周期锁前解析不可变子会话身份。
    /// `current_session_id` owns the target in `args`; returns the selected archived projections without mutation.
    /// `current_session_id` 拥有 `args` 中的目标；返回选中的归档投影且不修改状态。
    pub fn close_agent_archive_scope(
        &self,
        current_session_id: SessionId,
        args: &Value,
    ) -> Result<Vec<SubagentRegistryEntry>, String> {
        let (_, entry) = self.resolve_close_agent_entry(current_session_id, args)?;
        Ok(self
            .prepare_close_agent_archive_entries(entry.session_id)?
            .entries)
    }

    /// Closes one child tree after the host durably seals each selected collaboration target.
    /// 宿主可靠封闭每个选中协作目标后，关闭一棵子代理树。
    /// `before_archive` receives the exact selected entries while their task locks exclude replacement.
    /// `before_archive` 在任务锁排除替换期间取得精确的选中条目。
    /// Returns the same close result as `invoke_close_agent`, or an error before runtime archival.
    /// 返回与 `invoke_close_agent` 相同的关闭结果，或在运行时归档之前返回错误。
    pub fn invoke_close_agent_with_pre_archive(
        &self,
        current_session_id: SessionId,
        args: Value,
        before_archive: impl FnOnce(&[SubagentRegistryEntry]) -> Result<(), String>,
    ) -> Result<VulcanToolInvocationResult, String> {
        let (normalized_target, entry) =
            self.resolve_close_agent_entry(current_session_id, &args)?;
        let previous_status = entry.status;
        let mut prepared_archive = self.prepare_close_agent_archive_entries(entry.session_id)?;
        let transaction_locks =
            self.ordered_task_execution_state_locks(&prepared_archive.task_ids)?;
        let mut transaction_guards = Vec::with_capacity(transaction_locks.len());
        for transaction_lock in &transaction_locks {
            transaction_guards.push(transaction_lock.lock().map_err(|_| {
                    String::from(
                        "The task execution transaction is locked and cannot close the selected agent tree.",
                    )
                })?);
        }
        // Stop ownership, runtime archival, and inbox clearing share the sorted task locks.
        // 停止所有权、运行时归档与 inbox 清理共享有序任务锁。
        // Auxiliary writers are owned by the host and must settle before any subtree member is archived.
        // 辅助写入器归宿主持有，必须在任何子树成员归档之前结算。
        for task_id in &prepared_archive.task_ids {
            self.ensure_no_auxiliary_execution_with_transaction_lock_held(*task_id)
                .map_err(|error| {
                    format!(
                        "Cannot close the agent tree before auxiliary execution settles: {error}"
                    )
                })?;
        }
        // Seal the canonical message target before runtime archival can make it unreachable.
        // 在运行时归档使规范消息目标不可达之前，先封闭其准入。
        before_archive(&prepared_archive.entries)?;
        let stopped_task_ids =
            self.reserve_and_abort_selected_task_runs(&prepared_archive.task_ids)?;
        self.mark_stopped_tasks_with_transaction_locks_held(&stopped_task_ids, "user_interrupt")?;
        prepared_archive.restamp_after_task_stop(current_unix_millis_u64());
        let transaction = CloseAgentTransaction {
            parent_session_id: current_session_id,
            target: normalized_target,
            previous_status,
            stopped_task_ids,
            prepared_archive,
        };
        let committed_archive = match self
            .commit_close_agent_runtime_archive_with_task_locks_held(&transaction.prepared_archive)
        {
            Ok(committed_archive) => committed_archive,
            Err(commit_error) => {
                let failure = CloseAgentArchiveFailure::new(
                    CloseAgentArchiveFailurePhase::CommitRuntimeArchive,
                    commit_error,
                );
                return Ok(build_close_agent_archive_failure_result(
                    &transaction,
                    failure,
                ));
            }
        };
        drop(transaction_guards);
        let result = VulcanToolInvocationResult::from_model_output(json!({
                "tool": CLOSE_AGENT_TOOL,
                "target": transaction.target.as_str(),
                "previous_status": transaction.previous_status,
                "closed": true,
                "closed_agents": committed_archive.entries.iter().map(|entry| entry.agent_id.clone()).collect::<Vec<_>>(),
                "closed_count": committed_archive.entries.len(),
                "cleared_message_count": committed_archive.cleared_message_count,
            }))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("subagent_control"),
                payload: json!({
                    "tool": CLOSE_AGENT_TOOL,
                    "parent_session_id": transaction.parent_session_id.to_string(),
                    "matched_agents": committed_archive.entries.iter().map(|entry| {
                        subagent_registry_entry_host_status_payload(entry, 0)
                    }).collect::<Vec<_>>(),
                    "closed_agents": committed_archive.entries.iter().map(|entry| entry.agent_id.clone()).collect::<Vec<_>>(),
                    "closed_count": committed_archive.entries.len(),
                    (STOPPED_SUBAGENT_TASK_IDS_KEY): transaction.stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "stopped_count": transaction.stopped_task_ids.len(),
                    "closed": true,
                    "cleared_message_count": committed_archive.cleared_message_count,
                }),
            });
        Ok(result)
    }

    /// Resolves the open child selected by the shared close tool argument contract.
    /// 按共享关闭工具参数契约解析选中的开放子会话。
    /// Returns its normalized public name and current entry, rejecting root, self and archived targets.
    /// 返回规范公开名称和当前条目，拒绝根、自身和已归档目标。
    fn resolve_close_agent_entry(
        &self,
        current_session_id: SessionId,
        args: &Value,
    ) -> Result<(String, SubagentRegistryEntry), String> {
        let target = close_agent_target(args)?;
        if is_root_agent_target(&target) {
            return Err(String::from("root is not a spawned agent."));
        }
        let normalized_target = normalize_public_task_identifier(&target)?;
        if self.current_session_matches_public_target(current_session_id, &normalized_target)? {
            return Err(String::from(
                "An agent cannot close itself; return your result and let the parent close you if needed.",
            ));
        }
        let parent_session_id = SessionNodeId::from(current_session_id);
        let entry = self
                .subagent_registry_snapshot()?
                .get_for_parent(parent_session_id, &normalized_target)
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
        Ok((normalized_target, entry))
    }

    /// Prepares immutable archived registry projections without mutating runtime state.
    /// 在不修改运行时状态的前提下准备不可变的归档注册表投影。
    ///
    /// # Parameters
    /// 参数
    /// - `target_session_id`: Stable session id of the selected child agent.
    /// - `target_session_id`：选中子代理的稳定会话标识。
    ///
    /// # Returns
    /// 返回值
    /// Complete archived projections that may be persisted before the runtime commit.
    /// 可在运行时提交前持久化的完整归档投影。
    pub(super) fn prepare_close_agent_archive_entries(
        &self,
        target_session_id: SessionNodeId,
    ) -> Result<PreparedCloseAgentArchive, String> {
        let registry = self.subagent_registry.read().map_err(|_| {
            String::from("The subagent registry is locked and cannot prepare agent closure.")
        })?;
        let mut archived_entries = Vec::new();
        let archived_at = current_unix_millis_u64();
        for entry in registry
            .list_subtree_from_session(target_session_id)
            .map_err(|error| error.to_string())?
        {
            if entry.status == SubagentStatus::Archived {
                continue;
            }
            let mut entry = entry.clone();
            entry.set_status(SubagentStatus::Archived, archived_at);
            entry.stop_reason = Some(String::from("close_agent"));
            archived_entries.push(entry);
        }
        Ok(PreparedCloseAgentArchive::new(archived_entries))
    }

    /// Commits one prepared close transaction to all runtime-owned projections.
    /// 将一个已准备的关闭事务提交到全部运行时自有投影。
    ///
    /// # Parameters
    /// 参数
    /// - `prepared_archive`: Authoritative entries and task ids prepared from one registry snapshot.
    /// - `prepared_archive`：从同一注册表快照准备的权威条目与任务标识。
    ///
    /// # Returns
    /// 返回值
    /// Latest committed registry projections and the number of removed queued messages.
    /// 最新的已提交注册表投影与被移除的排队消息数量。
    #[cfg(test)]
    pub(super) fn commit_close_agent_runtime_archive(
        &self,
        prepared_archive: &PreparedCloseAgentArchive,
    ) -> Result<CommittedCloseAgentRuntimeArchive, String> {
        let transaction_locks =
            self.ordered_task_execution_state_locks(&prepared_archive.task_ids)?;
        let mut transaction_guards = Vec::with_capacity(transaction_locks.len());
        for transaction_lock in &transaction_locks {
            transaction_guards.push(transaction_lock.lock().map_err(|_| {
                String::from(
                    "The task execution transaction is locked and cannot commit agent closure.",
                )
            })?);
        }
        self.commit_close_agent_runtime_archive_with_task_locks_held(prepared_archive)
    }

    /// Commits runtime archive projections while the caller owns every selected task lock.
    /// 在调用方持有全部选中任务锁时提交运行时归档投影。
    ///
    /// # Parameters
    /// 参数
    /// - `prepared_archive`: Stable archive scope already protected by ordered transaction guards.
    /// - `prepared_archive`：已由有序事务守卫保护的稳定归档范围。
    ///
    /// # Returns
    /// 返回值
    /// Latest committed registry projections and the number of removed queued messages.
    /// 最新的已提交注册表投影与被移除的排队消息数量。
    fn commit_close_agent_runtime_archive_with_task_locks_held(
        &self,
        prepared_archive: &PreparedCloseAgentArchive,
    ) -> Result<CommittedCloseAgentRuntimeArchive, String> {
        let mut registry = self.subagent_registry.write().map_err(|_| {
            String::from("The subagent registry is locked and cannot commit agent closure.")
        })?;
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task-session registry is locked and cannot commit agent closure.")
        })?;
        let mut inboxes = self.agent_inboxes.write().map_err(|_| {
            String::from(
                "The subagent agent-control inbox is locked and cannot commit agent closure.",
            )
        })?;
        let mut committed_entries = Vec::with_capacity(prepared_archive.entries.len());
        for prepared_entry in &prepared_archive.entries {
            let current_entry = registry
                .get_for_parent(prepared_entry.parent_session_id, &prepared_entry.agent_id)
                .filter(|current_entry| current_entry.session_id == prepared_entry.session_id)
                .ok_or_else(|| {
                    format!(
                        "The prepared close-agent entry `{}` no longer owns session `{}` under parent `{}`.",
                        prepared_entry.agent_id,
                        prepared_entry.session_id,
                        prepared_entry.parent_session_id
                    )
                })?;
            // Preserve task-finalization fields written after preparation and apply only the
            // lifecycle transition authorized by the prepared close transaction.
            // 保留准备后由任务收尾写入的字段，仅应用已准备关闭事务授权的生命周期转换。
            let mut committed_entry = current_entry.clone();
            committed_entry.set_status(
                SubagentStatus::Archived,
                prepared_entry
                    .updated_at_ms
                    .max(current_entry.updated_at_ms),
            );
            committed_entry.stop_reason = prepared_entry.stop_reason.clone();
            committed_entries.push(committed_entry);
        }
        registry
            .replace_existing_entries(&committed_entries)
            .map_err(|error| error.to_string())?;
        for archived_entry in &committed_entries {
            if let Some(record) = sessions.get_mut(&SessionId::from(archived_entry.session_id)) {
                record.last_status =
                    status_name_for_task_session_status(TaskSessionStatus::Archived).to_string();
                record.updated_at = archived_entry.updated_at_ms.to_string();
                record.stop_reason = archived_entry.stop_reason.clone();
            }
        }
        let mut cleared_message_count = 0;
        for task_id in &prepared_archive.task_ids {
            cleared_message_count += inboxes
                .remove(task_id)
                .map(|queue| queue.len())
                .unwrap_or(0);
            // A closed agent runs nothing, so its shell jobs end with it.
            // 已关闭的代理不再运行任何内容，因此其 shell 任务随之结束。
            vulcan_host_tools::shell_job::ShellJobRuntime::global().stop_session_jobs(
                &crate::tooling::native::canonical_shell_job_session_id(&task_id.to_string()),
                "The agent that started this job was closed.",
            );
        }
        Ok(CommittedCloseAgentRuntimeArchive {
            entries: committed_entries,
            cleared_message_count,
        })
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
        if is_root_agent_target(&target) {
            return Err(String::from("root is not a spawned agent."));
        }
        let normalized_target = normalize_public_task_identifier(&target)?;
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
            .get_for_parent(parent_session_id, &normalized_target)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Unknown child agent `{normalized_target}` for parent session `{current_session_id}`."
                )
            })?;
        let target_task_id = SessionId::from(entry.session_id);
        let transaction_lock = self.task_execution_state_lock(target_task_id)?;
        let transaction_guard = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot interrupt the selected agent.",
            )
        })?;
        let current_entry = self
            .subagent_registry
            .read()
            .map_err(|_| String::from("The subagent registry is locked."))?
            .get_for_parent(parent_session_id, &normalized_target)
            .cloned()
            .filter(|current_entry| current_entry.session_id == entry.session_id)
            .ok_or_else(|| {
                format!("Child agent `{normalized_target}` changed ownership before interruption.")
            })?;
        if current_entry.status == SubagentStatus::Archived {
            return Err(format!(
                "Child agent `{normalized_target}` is archived and cannot be interrupted."
            ));
        }
        let previous_status = current_entry.status;
        drop(transaction_guard);
        let queued_message_count = self.inter_agent_message_count_for_task(target_task_id)?;
        let stopped_task_ids = self.stop_task_tree(target_task_id)?;
        let result = VulcanToolInvocationResult::from_model_output(json!({
            "tool": INTERRUPT_AGENT_TOOL,
            "previous_status": previous_status,
        })).with_host_result(VulcanToolHostResult {
            kind: String::from("subagent_control"),
            payload: json!({
                "tool": INTERRUPT_AGENT_TOOL,
                "parent_session_id": current_session_id.to_string(),
                "matched_agents": [subagent_registry_entry_host_status_payload(&current_entry, queued_message_count)],
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

/// Builds one structured failed close-agent result after task stopping has already committed.
/// 在任务停止已经提交后构建一条结构化 close-agent 失败结果。
///
/// # Parameters
/// 参数
/// - `transaction`: Complete request, stop, and archive facts for the close transaction.
/// - `transaction`：关闭事务的完整请求、停止与归档事实。
/// - `failure`: Unified runtime archive phase error.
/// - `failure`：统一的运行时归档阶段错误。
///
/// # Returns
/// 返回值
/// Error-marked tool result carrying machine-readable close transaction facts.
/// 携带机器可读关闭事务事实的错误工具结果。
fn build_close_agent_archive_failure_result(
    transaction: &CloseAgentTransaction,
    failure: CloseAgentArchiveFailure,
) -> VulcanToolInvocationResult {
    let details = json!({
        "tool": CLOSE_AGENT_TOOL,
        "operation": "close",
        "target": transaction.target.as_str(),
        "parent_session_id": transaction.parent_session_id.to_string(),
        "previous_status": transaction.previous_status,
        "closed": false,
        "stopped": true,
        "archive_committed": false,
        "failure_phase": failure.phase.as_str(),
        "archive_task_ids": transaction.prepared_archive.task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "archive_task_count": transaction.prepared_archive.task_ids.len(),
        (STOPPED_SUBAGENT_TASK_IDS_KEY): transaction.stopped_task_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "stopped_count": transaction.stopped_task_ids.len(),
    });
    VulcanToolInvocationResult::from_tool_error_details(failure.error, details)
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
        vulcan_task_session::SubagentDefinitionScope::Plugin => "plugin",
    }
}

/// Builds model-selection summary for one subagent definition.
/// 构建单个子代理定义的模型选择摘要。
fn subagent_definition_model_selection(
    definition: &SubagentDefinition,
    model_context: Option<&TaskExecutionRuntimeContext>,
) -> Result<Value, String> {
    let Some(model_context) = model_context else {
        return Ok(
            match provider_model_selector_from_snapshot(
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
            },
        );
    };
    let Some((provider_id, model_id)) =
        provider_model_selector_from_snapshot(&definition.provider_model, &model_context.config)
    else {
        return Ok(json!({
            "mode": "session",
        }));
    };
    // Profile metadata must report the same policy admission as actual creation.
    // 定义元数据必须报告与实际创建相同的策略准入。
    let runtime = model_context
        .foundation
        .public_ai_runtime()
        .ok_or_else(|| String::from("The service has not installed the public AI runtime."))?;
    let configuration = runtime.configuration_snapshot()?;
    let calling_route = agent_orchestration_session_route(model_context, &configuration);
    let granted = subagent_selector_is_granted(
        model_context,
        calling_route.as_ref(),
        &provider_id,
        &model_id,
    );
    let available =
        granted && runtime.model_is_available(&configuration, &provider_id, &model_id)?;
    Ok(json!({
        "mode": "fixed",
        "provider_id": provider_id,
        "model_id": model_id,
        "selector": format!("{provider_id}/{model_id}"),
        "available": available,
        "unavailable_reason": if available {
            Value::Null
        } else if !granted {
            Value::String(String::from("This fixed model is not permitted by the subagent model policy. Choose a permitted explicit model or ask the user to update the policy."))
        } else {
            Value::String(String::from("The configured fixed model is unavailable. Without an explicit model, the calling model can be inherited only if permitted by the subagent policy. Choose an available permitted selector from the models list."))
        },
    }))
}

/// Builds the model rows the agent may pass to `spawn_agent.model`.
/// 构建代理可传给 `spawn_agent.model` 的模型行。
///
/// # Parameters
/// 参数
/// `model_context` carries the session route and allowlist; `agent_fixed_model` marks a focused
/// profile's fixed model.
/// `model_context` 携带会话路由与白名单；`agent_fixed_model` 标记聚焦配置的固定模型。
///
/// # Returns
/// 返回值
/// The session model first, then the allowlisted models in the user's order, each only while it is
/// an enabled conversation model on an enabled account, with the user's note.
/// 先是会话模型，再按用户顺序列出白名单模型；每个模型仅在为已启用账号上的已启用会话模型时列出，
/// 并附带用户备注。
pub(super) fn agent_orchestration_info_models(
    model_context: &TaskExecutionRuntimeContext,
    agent_fixed_model: Option<&(String, String)>,
) -> Result<Vec<Value>, String> {
    let runtime = model_context
        .foundation
        .public_ai_runtime()
        .ok_or_else(|| String::from("The service has not installed the public AI runtime."))?;
    let configuration = runtime.configuration_snapshot()?;
    let providers = runtime.effective_providers(&configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    // Only selectors that spawn_agent accepts are listed: enabled conversation models on enabled accounts.
    // 只列出 spawn_agent 接受的选择器：已启用账号上已启用的会话模型。
    let available_selectors = runtime.available_model_selectors(&configuration)?;
    let session_route = agent_orchestration_session_route(model_context, &configuration);
    let fixed_selector =
        agent_fixed_model.map(|(provider_id, model_id)| format!("{provider_id}/{model_id}"));
    // Only explicitly enabled inheritance adds the calling model to the granted rows.
    // 只有显式开启继承时才把调用模型加入授权行。
    let mut wanted: Vec<(String, String, String, bool)> = Vec::new();
    if model_context
        .config
        .subagent_model_policy(
            model_context
                .request_template
                .subagent_model_policy
                .as_ref(),
        )
        .0
        && let Some((provider_id, model_id, _)) = session_route
    {
        wanted.push((provider_id, model_id, String::new(), true));
    }
    for grant in effective_subagent_model_grants(model_context) {
        if !wanted.iter().any(|(provider_id, model_id, _, _)| {
            *provider_id == grant.provider_id && *model_id == grant.model_id
        }) {
            wanted.push((
                grant.provider_id.clone(),
                grant.model_id.clone(),
                grant.note.clone(),
                false,
            ));
        }
    }
    let mut rows = Vec::new();
    for (provider_id, model_id, note, is_session_model) in wanted {
        let selector = format!("{provider_id}/{model_id}");
        if !available_selectors.contains(&selector) {
            continue;
        }
        let mut model_name = None;
        let mut account_count = 0usize;
        for connection in configuration.connection_graph.connections.values() {
            if connection.provider_id != provider_id
                || !configuration.connection_is_enabled(connection.id.as_str())
                || configuration.connection_is_pending_validation(connection.id.as_str())
            {
                continue;
            }
            let Some(model) = catalogs
                .get(connection.id.as_str())
                .and_then(|catalog| catalog.models.iter().find(|model| model.id == model_id))
            else {
                continue;
            };
            account_count += 1;
            model_name.get_or_insert_with(|| model.display_name.clone());
        }
        let provider_name = providers
            .provider(provider_id.as_str())
            .map(|provider| provider.display_name.clone())
            .unwrap_or_else(|| provider_id.clone());
        let selection = runtime.resolve_model_selection(&configuration, &provider_id, &model_id)?;
        let reasoning_efforts = child_model_reasoning_efforts(&selection.model.capabilities);
        rows.push(json!({
            "provider_id": provider_id,
            "provider_name": provider_name,
            "model_name": model_name.unwrap_or_else(|| model_id.clone()),
            "model_id": model_id,
            "selector": selector,
            "note": note,
            "is_session_model": is_session_model,
            "is_agent_fixed_model": fixed_selector.as_deref() == Some(selector.as_str()),
            "account_count": account_count,
            "reasoning_efforts": reasoning_efforts,
            "default_reasoning_effort": reasoning_efforts.last(),
        }));
    }
    Ok(rows)
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
fn agent_orchestration_session_model(
    model_context: &TaskExecutionRuntimeContext,
) -> Result<Option<Value>, String> {
    let runtime = model_context
        .foundation
        .public_ai_runtime()
        .ok_or_else(|| String::from("The service has not installed the public AI runtime."))?;
    let configuration = runtime.configuration_snapshot()?;
    let Some((provider_id, model_id, connection_id)) =
        agent_orchestration_session_route(model_context, &configuration)
    else {
        return Ok(None);
    };
    // Excluded calling models must not leak back into the model-selection section.
    // 被排除的调用模型不得重新进入模型选择区块。
    if !subagent_selector_is_granted(
        model_context,
        Some(&(provider_id.clone(), model_id.clone(), connection_id.clone())),
        &provider_id,
        &model_id,
    ) {
        return Ok(None);
    }
    let available = runtime.model_is_available(&configuration, &provider_id, &model_id)?;
    Ok(Some(json!({
        "selector": format!("{provider_id}/{model_id}"),
        "provider_id": provider_id,
        "model_id": model_id,
        "connection_id": connection_id,
        "available": available,
    })))
}

/// Resolves the model the calling session actually runs, which inheriting children reuse.
/// 解析调用会话实际运行的模型，继承型子代理复用同一模型。
///
/// # Parameters
/// 参数
/// `model_context` carries the parent request template; `configuration` supplies the fallback.
/// `model_context` 携带父请求模板；`configuration` 提供回退值。
///
/// # Returns
/// 返回值
/// `(provider_id, model_id, connection_id)` from the request's frozen session selection, or the
/// global active model only when the request names none.
/// 来自请求冻结会话选择的 `(provider_id, model_id, connection_id)`；仅当请求未指定时回退到全局激活模型。
pub(super) fn agent_orchestration_session_route(
    model_context: &TaskExecutionRuntimeContext,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
) -> Option<(String, String, Option<String>)> {
    let request = &model_context.request_template;
    if let (Some(provider_id), Some(model_id)) = (
        request.provider_id_override.as_ref(),
        request.model_id_override.as_ref(),
    ) {
        return Some((
            provider_id.clone(),
            model_id.clone(),
            request.connection_id_override.clone(),
        ));
    }
    configuration.active.as_ref().map(|active| {
        (
            active.provider_id.clone(),
            active.model_id.clone(),
            Some(active.connection_id.clone()),
        )
    })
}
