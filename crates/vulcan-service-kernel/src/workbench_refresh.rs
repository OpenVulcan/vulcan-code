//! workbench refresh helpers.
//! workbench refresh 辅助逻辑。

use super::*;

/// Refreshes the WebUI workbench after a focused workspace metadata update.
/// 在一次聚焦的工作区元数据更新后刷新 WebUI 工作台。
pub(crate) async fn refresh_workbench_snapshot(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        preserved_workspace_run_state,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        agent_runtime_for_status,
        live_listener_task_ids,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            preserved_workspace_run_state,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            guard.agent_runtime.clone(),
            guard.detached_subagent_task_listeners.clone(),
        )
    };

    let mut refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );
    let selected_session_id = refreshed_snapshot.selected_session;
    if selected_session_id != SessionId::nil() {
        hydrate_selected_session_runtime_if_needed(
            controller_runtime.clone(),
            &mut refreshed_snapshot,
            selected_session_id,
        )
        .await?;
        apply_persisted_task_mirror_statuses_to_workspace(
            &controller_runtime,
            &mut refreshed_snapshot,
            selected_session_id,
            |task_id| {
                webui_subagent_task_has_live_run(
                    agent_runtime_for_status
                        .as_ref()
                        .map(|runtime| &runtime.task_runtime),
                    live_listener_task_ids.contains(&task_id),
                    task_id,
                )
            },
        )?;
    }
    sync_selected_session_goal_summary_from_store(
        &state,
        controller_runtime.clone(),
        selected_session_id,
    )
    .await?;
    let (surface_messages, subagent_navigation, session_goal_summaries, hydrated_window_summaries) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &refreshed_snapshot,
        );
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &refreshed_snapshot),
            guard.session_goal_summaries.clone(),
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        agent_runtime_for_status
            .as_ref()
            .map(|agent_runtime| agent_runtime.command_session_runtime.as_ref()),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &refreshed_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question_snapshot_from_agent_runtime(agent_runtime_for_status.as_ref()),
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(refreshed_snapshot);
    }
    Ok(workbench)
}

/// Returns whether one session phase still represents active work.
/// 判断单个会话阶段是否仍代表活跃执行。
pub(crate) fn is_active_session_run_phase(phase: SessionRunPhase) -> bool {
    matches!(
        phase,
        SessionRunPhase::Preparing
            | SessionRunPhase::MemoryRecall
            | SessionRunPhase::Requesting
            | SessionRunPhase::Thinking
            | SessionRunPhase::ToolRunning
    )
}

/// Returns whether a runtime event closes the current streamed thinking segment.
/// 判断运行时事件是否会关闭当前流式思考片段。
pub(crate) fn is_provider_tool_boundary_runtime_event(event: &RuntimeEvent) -> bool {
    matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) && (event.title.starts_with("工具调用 · ") || event.title.starts_with("工具返回 · "))
}

/// Returns the current Unix timestamp as millisecond text for SQLite metadata rows.
/// 返回当前 Unix 毫秒时间戳文本，用于 SQLite 元数据行。
pub(crate) fn current_unix_millis_text() -> String {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
        .to_string()
}

/// Returns the current Unix timestamp as a millisecond integer for hot-path timing.
/// 返回当前 Unix 毫秒整数，用于热路径计时。
pub(crate) fn current_unix_millis_value() -> i128 {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
}

/// Returns a UTC timestamp suitable for human-readable diagnostic traces.
/// 返回适合人工阅读诊断日志的 UTC 时间戳。
pub(crate) fn current_webui_trace_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

/// Returns whether WebUI execution-chain diagnostic traces are enabled.
/// 返回 WebUI 执行链诊断日志是否已启用。
pub(crate) fn webui_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Emits one WebUI execution-chain diagnostic trace line when tracing is enabled.
/// 当诊断开关启用时输出一条 WebUI 执行链诊断日志。
pub(crate) fn webui_trace(stage: &str, detail: impl AsRef<str>) {
    if !webui_trace_enabled() {
        return;
    }
    eprintln!(
        "[webui-service][webui-trace] timestamp={} unix_ms={} stage={} {}",
        current_webui_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}

/// Emits one WebUI timing trace when a supposedly hot path exceeds the 20ms budget.
/// 当本应为热路径的步骤超过 20ms 预算时输出 WebUI 计时日志。
pub(crate) fn webui_trace_if_slow(stage: &str, started_at_ms: i128, detail: impl AsRef<str>) {
    let elapsed_ms = current_unix_millis_value().saturating_sub(started_at_ms);
    if elapsed_ms > 20 {
        webui_trace(
            stage,
            format!("elapsed_ms={elapsed_ms} {}", detail.as_ref()),
        );
    }
}

/// Builds compact RuntimeEvent metadata for WebUI trace logs.
/// 为 WebUI 跟踪日志构建紧凑的 RuntimeEvent 元数据。
pub(crate) fn runtime_event_trace_detail(event: &RuntimeEvent) -> String {
    let metadata = event.metadata.as_ref();
    let tool_call_id = metadata
        .and_then(|value| value.get("tool_call_id"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let tool_name = metadata
        .and_then(|value| value.get("tool_name"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let display_state = metadata
        .and_then(|value| value.get("display_state"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    format!(
        "tool_call_id={tool_call_id} tool_name={tool_name} display_state={display_state} title_chars={} detail_chars={}",
        event.title.chars().count(),
        event.detail.chars().count()
    )
}

/// Builds compact detached-task metadata for WebUI trace logs.
/// 为 WebUI 跟踪日志构建紧凑的分离任务元数据。
pub(crate) fn detached_task_event_trace_detail(event: &DetachedTaskRuntimeEvent) -> String {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
            event,
            ..
        }) => runtime_event_trace_detail(event),
        DetachedTaskRuntimeEvent::ParentRuntime { event, .. } => runtime_event_trace_detail(event),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingDelta {
            session_id,
            delta,
        }) => format!("session={session_id} delta_chars={}", delta.chars().count()),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::OutputDelta {
            session_id,
            delta,
        }) => format!("session={session_id} delta_chars={}", delta.chars().count()),
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderLoweringRepair { session_id, event },
        ) => format!(
            "session={session_id} provider={} model={} action={} affected_items={}",
            event.provider_id, event.model_id, event.action, event.affected_items
        ),
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ConversationCheckpoint {
                session_id,
                conversation_state,
                provider_request_count,
                ..
            },
        ) => format!(
            "session={session_id} records={} messages={} provider_requests={provider_request_count}",
            conversation_state.records.len(),
            conversation_state.messages.len()
        ),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(report)) => {
            format!(
                "provider_requests={} output_chars={} error={}",
                report.provider_request_count,
                report
                    .assistant_output
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count(),
                report.error.is_some()
            )
        }
        _ => String::from("detail=<none>"),
    }
}

/// Returns a compact label for one detached task event without dumping large payloads.
/// 返回单条分离任务事件的紧凑标签，避免输出大型载荷。
pub(crate) fn detached_task_event_label(event: &DetachedTaskRuntimeEvent) -> &'static str {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Prepared(_)) => {
            "child.prepared"
        }
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderRequestCheckpoint { .. },
        ) => "child.provider_request_checkpoint",
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderLoweringRepair { .. },
        ) => "child.provider_lowering_repair",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RequestReady { .. }) => {
            "child.request_ready"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::WireRequestReady {
            ..
        }) => "child.wire_request_ready",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RequestStarted {
            ..
        }) => "child.request_started",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingDelta {
            ..
        }) => "child.thinking_delta",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingCompleted {
            ..
        }) => "child.thinking_completed",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::OutputDelta { .. }) => {
            "child.output_delta"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
            event,
            ..
        }) => match event.kind {
            RuntimeEventKind::DelegateStarted => "child.runtime.delegate_started",
            RuntimeEventKind::DelegateFinished => "child.runtime.delegate_finished",
            RuntimeEventKind::ToolStarted => "child.runtime.tool_started",
            RuntimeEventKind::ToolFinished => "child.runtime.tool_finished",
            _ => "child.runtime.event",
        },
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ConversationCheckpoint { .. },
        ) => "child.checkpoint",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(_)) => {
            "child.finished"
        }
        DetachedTaskRuntimeEvent::ParentRuntime { event, .. } => match event.kind {
            RuntimeEventKind::DelegateStarted => "parent.delegate_started",
            RuntimeEventKind::DelegateFinished => "parent.delegate_finished",
            _ => "parent.runtime_event",
        },
    }
}

/// Converts runtime state into a WebUI workbench snapshot.
/// 将运行时状态转换为 WebUI 工作台快照。
pub(crate) fn workbench_from_runtime(
    controller: &ControllerRuntimeHandle,
    command_session_runtime: Option<&CommandSessionRuntime>,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    surface_messages: &[SessionMessageEntry],
    subagent_navigation: Option<SubagentNavigationSnapshot>,
    pending_question: Option<PendingQuestionSnapshot>,
    logs: &[String],
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
    session_goal_summaries: &BTreeMap<SessionId, SessionGoalSummary>,
    hydrated_conversation_windows: &[HydratedConversationWindowSummary],
) -> WorkbenchSnapshot {
    let selected_conversation_id = if workspace.selected_session == SessionId::nil() {
        None
    } else {
        Some(workspace.selected_session.to_string())
    };
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.as_str())
        .or(selected_conversation_id.as_deref());
    let mut messages = active_conversation_id
        .map(|conversation_id| {
            let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
                workspace,
                workspace.selected_session,
                subagent_navigation.as_ref(),
            );
            conversation_messages_from_session(
                conversation_id,
                surface_messages,
                allow_user_prompt_editing,
            )
        })
        .unwrap_or_default();
    append_hydrated_workspace_messages(&mut messages, workspace);
    let selected_runtime_state = workspace
        .session_runtime_state
        .get(&workspace.selected_session);
    let selected_ai_tool_configuration = selected_runtime_state
        .and_then(|runtime_state| runtime_state.ai_tool_configuration.as_ref())
        .unwrap_or(ai_tool_configuration);
    let ai_capability_model_capabilities =
        active_provider_configuration_model_capabilities(provider_configuration);
    let restorable_project_ids = workspace
        .task_projects
        .iter()
        .chain(workspace.archived_task_projects.iter())
        .map(|project| project.id.as_str())
        .collect::<BTreeSet<_>>();
    WorkbenchSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        modules: runtime_modules(
            controller,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
        ),
        projects: workspace
            .task_projects
            .iter()
            .map(|project| ProjectSummary {
                id: project.id.clone(),
                name: project.title.clone(),
                path: project.path.clone(),
                pinned: project.pinned,
                icon_pack: project.icon_pack.clone(),
                icon_key: project.icon_key.clone(),
            })
            .collect(),
        expanded_project_ids: workspace.expanded_task_projects.iter().cloned().collect(),
        conversations: workspace
            .sessions
            .iter()
            .filter(|session| session_segmented_metadata_exists(session))
            .map(|session| {
                let run_phase = workspace
                    .session_run_phases
                    .get(&session.id)
                    .copied()
                    .unwrap_or(SessionRunPhase::Idle);
                let effective_run_phase = run_phase;
                let is_running = session.status == SessionState::Running
                    || is_active_session_run_phase(run_phase);
                ConversationSummary {
                    id: session.id.to_string(),
                    title: session.title.clone(),
                    status: if is_running {
                        SessionState::Running.label()
                    } else {
                        session.status.label()
                    }
                    .to_string(),
                    run_phase: effective_run_phase.to_string(),
                    last_updated_label: session.last_updated_label.clone(),
                    last_updated_at: session.updated_at.clone(),
                    unread: workspace
                        .session_runtime_state
                        .get(&session.id)
                        .is_some_and(|runtime_state| runtime_state.ui_state.unread),
                    project_id: session.project_id.clone(),
                    fork_source: conversation_fork_source_summary_from_session(session),
                }
            })
            .collect(),
        archived_projects: workspace
            .archived_task_projects
            .iter()
            .map(|project| ProjectSummary {
                id: project.id.clone(),
                name: project.title.clone(),
                path: project.path.clone(),
                pinned: project.pinned,
                icon_pack: project.icon_pack.clone(),
                icon_key: project.icon_key.clone(),
            })
            .collect(),
        archived_conversations: workspace
            .archived_sessions
            .iter()
            .filter(|session| {
                session
                    .project_id
                    .as_deref()
                    .is_some_and(|project_id| restorable_project_ids.contains(project_id))
            })
            .filter(|session| session_segmented_metadata_exists(session))
            .map(|session| ConversationSummary {
                id: session.id.to_string(),
                title: session.title.clone(),
                status: String::from("Archived"),
                run_phase: SessionRunPhase::Idle.to_string(),
                last_updated_label: session.last_updated_label.clone(),
                last_updated_at: session.updated_at.clone(),
                unread: false,
                project_id: session.project_id.clone(),
                fork_source: conversation_fork_source_summary_from_session(session),
            })
            .collect(),
        selected_conversation_id,
        messages,
        hydrated_conversation_windows: hydrated_conversation_windows.to_vec(),
        conversation_hydration: None,
        subagent_navigation,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_luaskills(luaskills_catalog),
        lua_skills: lua_skills_catalog_summary(luaskills_catalog, provider_configuration),
        agent_skills: agent_skills_catalog_summary(provider_configuration),
        settings: settings(),
        providers: providers(provider_configuration),
        mcp_servers: mcp_servers(provider_configuration),
        reasoning_selection: reasoning_selection(provider_configuration),
        thinking_display_mode: workbench_thinking_display_mode(provider_configuration),
        git_commit: git_commit_settings(provider_configuration),
        host_tools: host_tools_settings(provider_configuration),
        ai_capabilities: ai_capabilities(
            selected_ai_tool_configuration,
            ai_capability_model_capabilities.as_ref(),
        ),
        default_ai_capabilities: ai_capabilities(
            ai_tool_configuration,
            ai_capability_model_capabilities.as_ref(),
        ),
        ai_capability_vendors: ai_capability_vendors(selected_ai_tool_configuration),
        ai_capability_vendor_catalog: ai_capability_vendor_catalog(),
        composer_attachment_capabilities: composer_attachment_capabilities(
            provider_configuration,
            selected_ai_tool_configuration,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: selected_runtime_state
            .map(pending_prompt_summaries_from_runtime_state)
            .unwrap_or_default(),
        pending_question,
        session_todolist: session_todolist_summary_from_runtime_state(selected_runtime_state),
        session_goal: session_goal_summaries
            .get(&workspace.selected_session)
            .cloned(),
        session_plan_mode: session_plan_mode_summary_from_runtime_state(selected_runtime_state),
        memories: memories(workspace),
        vmm_memory: vmm_memory_snapshot(workspace, provider_configuration, selected_runtime_state),
        command_sessions: command_sessions(controller, command_session_runtime),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("startup-{index}"), "info", "service", message)
            })
            .chain(
                mcp_catalog
                    .summary_rows
                    .iter()
                    .enumerate()
                    .map(|(index, message)| {
                        log_entry(&format!("mcp-{index}"), "info", "mcp", message)
                    }),
            )
            .collect(),
        context_injection: context_injection_snapshot_from_runtime_state(selected_runtime_state),
        context_usage: context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            selected_runtime_state,
            live_context_budgets,
            provider_configuration,
            ai_tool_configuration,
        ),
    }
}
