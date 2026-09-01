//! Workspace persistence version guards.
//! 工作区持久化版本防护。

use super::*;

/// Builds an empty LuaSkills catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 LuaSkills 目录。
pub(crate) fn empty_luaskills_catalog() -> TauriLuaSkillsCatalog {
    TauriLuaSkillsCatalog {
        runtime_root: PathBuf::new(),
        managed_runtime_distribution_root: PathBuf::new(),
        managed_runtime_environment_root: PathBuf::new(),
        managed_runtime_report: Default::default(),
        managed_runtime_progress: None,
        system_lua_lib_dir: PathBuf::new(),
        skills: Vec::new(),
        entry_count: 0,
        runtime_settings: vulcan_luaskills_config::LuaSkillsRuntimeSettings::default(),
        runtime_settings_file: PathBuf::new(),
        restart_required: false,
    }
}

/// Builds an empty MCP catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 MCP 目录。
pub(crate) fn empty_mcp_catalog() -> TauriMcpRuntimeCatalog {
    TauriMcpRuntimeCatalog {
        configured_server_count: 0,
        enabled_server_count: 0,
        connected_server_count: 0,
        tool_count: 0,
        prompt_count: 0,
        resource_count: 0,
        summary_rows: Vec::new(),
        tools: Vec::new(),
    }
}

/// Returns the cached LuaSkills catalog without touching the LuaSkills engine on hot paths.
/// 在热路径上返回缓存的 LuaSkills 目录，避免触碰 LuaSkills 引擎。
pub(crate) fn cached_luaskills_catalog_from_state(
    state: &ServiceBridgeState,
) -> TauriLuaSkillsCatalog {
    state
        .cached_luaskills_catalog
        .clone()
        .unwrap_or_else(empty_luaskills_catalog)
}

/// Returns the cached MCP catalog without querying the MCP runtime on hot paths.
/// 在热路径上返回缓存的 MCP 目录，避免查询 MCP 运行时。
pub(crate) fn cached_mcp_catalog_from_state(state: &ServiceBridgeState) -> TauriMcpRuntimeCatalog {
    state
        .cached_mcp_catalog
        .clone()
        .unwrap_or_else(empty_mcp_catalog)
}

/// Parses a user-facing optional session identifier and ignores local draft identifiers.
/// 解析用户界面传入的可选会话标识，并忽略本地草稿标识。
pub(crate) fn parse_session_id(value: &str) -> Option<SessionId> {
    value.parse::<SessionId>().ok()
}

/// Converts the composer reasoning string into the shared provider-config enum.
/// 将输入区思考档位字符串转换为共享 provider-config 枚举。
pub(crate) fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffortLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(ReasoningEffortLevel::Off),
        "none" => Some(ReasoningEffortLevel::None),
        "adaptive" => Some(ReasoningEffortLevel::Adaptive),
        "minimal" => Some(ReasoningEffortLevel::Minimal),
        "low" => Some(ReasoningEffortLevel::Low),
        "medium" => Some(ReasoningEffortLevel::Medium),
        "high" => Some(ReasoningEffortLevel::High),
        "x-high" | "xhigh" => Some(ReasoningEffortLevel::XHigh),
        "max" => Some(ReasoningEffortLevel::Max),
        "ultra" => Some(ReasoningEffortLevel::Ultra),
        _ => None,
    }
}

/// Converts the WebUI thinking display string into the shared provider-config enum.
/// 将 WebUI 思考展示字符串转换为共享 provider-config 枚举。
pub(crate) fn parse_thinking_display_mode(value: &str) -> Option<WorkbenchThinkingDisplayMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "popup_only" => Some(WorkbenchThinkingDisplayMode::PopupOnly),
        "inline_output" => Some(WorkbenchThinkingDisplayMode::InlineOutput),
        "first_inline_with_popup" => Some(WorkbenchThinkingDisplayMode::FirstInlineWithPopup),
        "first_inline_only" => Some(WorkbenchThinkingDisplayMode::FirstInlineOnly),
        _ => None,
    }
}

/// Converts one shared thinking display enum into the WebUI wire value.
/// 将单个共享思考展示枚举转换为 WebUI 传输值。
pub(crate) fn thinking_display_mode_wire_value(mode: WorkbenchThinkingDisplayMode) -> &'static str {
    match mode {
        WorkbenchThinkingDisplayMode::PopupOnly => "popup_only",
        WorkbenchThinkingDisplayMode::InlineOutput => "inline_output",
        WorkbenchThinkingDisplayMode::FirstInlineWithPopup => "first_inline_with_popup",
        WorkbenchThinkingDisplayMode::FirstInlineOnly => "first_inline_only",
    }
}

/// Resolves the WebUI thinking display mode from the shared provider configuration.
/// 从共享 Provider 配置解析 WebUI 思考展示模式。
pub(crate) fn workbench_thinking_display_mode(
    provider_configuration: &ProviderConfigurationFile,
) -> String {
    thinking_display_mode_wire_value(
        provider_configuration
            .workbench_thinking_display_mode
            .unwrap_or_default(),
    )
    .to_string()
}

/// Resolves the effective reasoning effort for the active model using the shared selector view.
/// 使用共享选择器视图解析当前激活模型的生效思考档位。
pub(crate) fn active_reasoning_selection_effort(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningEffortLevel> {
    provider_configuration
        .workbench_reasoning_effort_explicit
        .then_some(provider_configuration.workbench_reasoning_effort)
        .flatten()
}

/// Builds the WebUI reasoning selector snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI 思考选择器快照。
pub(crate) fn reasoning_selection(
    provider_configuration: &ProviderConfigurationFile,
    capabilities: Option<&vulcan_ai_contract::ModelCapabilities>,
) -> Option<ReasoningSelectionSummary> {
    let capabilities = capabilities?;
    // Resolve the same host default used by the provider runtime before publishing selector state.
    // 在发布选择器状态前解析与供应商运行时相同的宿主默认策略。
    let mut effective_capabilities = capabilities.clone();
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut effective_capabilities);
    let capabilities = &effective_capabilities;
    // Explicit unsupported/fixed-off metadata is authoritative even when a stale catalog leaves effort entries behind.
    // 明确的不支持/固定关闭元数据具有最高优先级，即使旧目录残留了推理档位也必须保持关闭。
    if capabilities.reasoning == vulcan_ai_contract::CapabilitySupport::Unsupported
        || capabilities.reasoning_control == vulcan_ai_contract::ReasoningControlMode::FixedOff
    {
        return Some(ReasoningSelectionSummary {
            options: vec![ReasoningSelectionOptionSummary {
                value: String::from("fixed-off"),
                label: String::from("关闭"),
            }],
            selected_effort: String::from("fixed-off"),
            note: None,
        });
    }
    if capabilities.reasoning_efforts.is_empty() {
        let enabled = capabilities.reasoning.is_supported();
        return Some(ReasoningSelectionSummary {
            options: vec![ReasoningSelectionOptionSummary {
                value: String::from(if enabled { "fixed-on" } else { "fixed-off" }),
                label: String::from(if enabled { "开启" } else { "关闭" }),
            }],
            selected_effort: String::from(if enabled { "fixed-on" } else { "fixed-off" }),
            note: None,
        });
    }
    if capabilities.reasoning_efforts.len() == 1 {
        let effort = capabilities.reasoning_efforts[0];
        let level = reasoning_effort_level_from_public(effort);
        return Some(ReasoningSelectionSummary {
            options: vec![ReasoningSelectionOptionSummary {
                value: reasoning_effort_wire_value(level).to_string(),
                label: String::from(match effort {
                    vulcan_ai_contract::ReasoningEffort::None => "关闭",
                    vulcan_ai_contract::ReasoningEffort::Adaptive => "自动",
                    _ => "开启",
                }),
            }],
            selected_effort: reasoning_effort_wire_value(level).to_string(),
            note: None,
        });
    }
    // displayEfforts separates canonical UI ordering from the model-owned order that declares its default.
    // displayEfforts 将规范的界面排序与用于声明模型默认值的模型自有顺序分离。
    let mut display_efforts = capabilities.reasoning_efforts.clone();
    display_efforts.sort_by_key(|effort| match effort {
        vulcan_ai_contract::ReasoningEffort::None => 0,
        vulcan_ai_contract::ReasoningEffort::Adaptive => 1,
        vulcan_ai_contract::ReasoningEffort::Minimal => 2,
        vulcan_ai_contract::ReasoningEffort::Low => 3,
        vulcan_ai_contract::ReasoningEffort::Medium => 4,
        vulcan_ai_contract::ReasoningEffort::High => 5,
        vulcan_ai_contract::ReasoningEffort::XHigh => 6,
        vulcan_ai_contract::ReasoningEffort::Max => 7,
        vulcan_ai_contract::ReasoningEffort::Ultra => 8,
    });
    // options projects canonical display order without changing execution-time default resolution.
    // options 投影规范展示顺序，但不改变执行期默认档位解析。
    let options = display_efforts
        .iter()
        .copied()
        .map(|effort| {
            let level = reasoning_effort_level_from_public(effort);
            ReasoningSelectionOptionSummary {
                value: reasoning_effort_wire_value(level).to_string(),
                label: level.compact_label().to_string(),
            }
        })
        .collect::<Vec<_>>();
    let configured = provider_configuration
        .workbench_reasoning_effort_explicit
        .then_some(provider_configuration.workbench_reasoning_effort)
        .flatten();
    // effectiveEffort shares the exact execution-time resolver so the displayed default cannot diverge from dispatch.
    // effectiveEffort 复用精确的执行期解析器，确保展示默认值不会与分发行为分叉。
    let effective_effort =
        vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_reasoning_effort(
            capabilities,
            configured,
        );
    let selected_effort = reasoning_effort_wire_value(effective_effort).to_string();
    let note = configured
        .filter(|effort| reasoning_effort_wire_value(*effort) != selected_effort)
        .map(|effort| {
            format!(
                "当前模型不支持已保存的推理档位“{}”，已切换到该模型的启用档位“{}”。",
                effort.compact_label(),
                selected_effort
            )
        });
    Some(ReasoningSelectionSummary {
        options,
        selected_effort,
        note,
    })
}

/// Converts one public reasoning level into the shared workbench preference.
/// 将一个公共推理档位转换为共享工作台偏好。
fn reasoning_effort_level_from_public(
    effort: vulcan_ai_contract::ReasoningEffort,
) -> ReasoningEffortLevel {
    match effort {
        vulcan_ai_contract::ReasoningEffort::None => ReasoningEffortLevel::None,
        vulcan_ai_contract::ReasoningEffort::Adaptive => ReasoningEffortLevel::Adaptive,
        vulcan_ai_contract::ReasoningEffort::Minimal => ReasoningEffortLevel::Minimal,
        vulcan_ai_contract::ReasoningEffort::Low => ReasoningEffortLevel::Low,
        vulcan_ai_contract::ReasoningEffort::Medium => ReasoningEffortLevel::Medium,
        vulcan_ai_contract::ReasoningEffort::High => ReasoningEffortLevel::High,
        vulcan_ai_contract::ReasoningEffort::XHigh => ReasoningEffortLevel::XHigh,
        vulcan_ai_contract::ReasoningEffort::Max => ReasoningEffortLevel::Max,
        vulcan_ai_contract::ReasoningEffort::Ultra => ReasoningEffortLevel::Ultra,
    }
}

/// Returns the stable WebUI wire value for one workbench reasoning preference.
/// 返回一个工作台推理偏好的稳定 WebUI 传输值。
fn reasoning_effort_wire_value(effort: ReasoningEffortLevel) -> &'static str {
    match effort {
        ReasoningEffortLevel::Off => "off",
        ReasoningEffortLevel::None => "none",
        ReasoningEffortLevel::Adaptive => "adaptive",
        ReasoningEffortLevel::Minimal => "minimal",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
        ReasoningEffortLevel::Max => "max",
        ReasoningEffortLevel::Ultra => "ultra",
    }
}

/// Creates a readable generated session title from the submitted prompt.
/// 根据提交提示词创建可读的自动会话标题。
pub(crate) fn prompt_to_session_title(prompt: &str) -> String {
    let title = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("新会话");
    title.chars().take(48).collect()
}

/// Describes one cold-path full workbench snapshot refresh request.
/// 描述一次冷路径全量工作台快照刷新请求。
pub(crate) struct FullWorkbenchColdRefreshRequest<'a> {
    /// Emits the rebuilt workbench snapshot to WebUI subscribers.
    /// 负责向 WebUI 订阅者发送重建后的工作台快照。
    pub(crate) app: &'a ServiceEventEmitter,
    /// Owns the cached runtime state used while rebuilding the surface.
    /// 持有重建界面时需要读取的运行时缓存状态。
    pub(crate) state: &'a SharedServiceBridgeState,
    /// Provides controller-level persistence and filesystem services.
    /// 提供控制器级持久化与文件系统服务。
    pub(crate) controller_runtime: &'a ControllerRuntimeHandle,
    /// Provides the LuaSkills catalog projected into the workbench snapshot.
    /// 提供需要投影到工作台快照中的 LuaSkills 目录。
    pub(crate) luaskills_catalog: &'a TauriLuaSkillsCatalog,
    /// Provides MCP server and tool summaries for the workbench snapshot.
    /// 提供工作台快照需要的 MCP 服务与工具摘要。
    pub(crate) mcp_catalog: &'a TauriMcpRuntimeCatalog,
    /// Provides configured provider and model choices.
    /// 提供已配置的供应商与模型选项。
    pub(crate) provider_configuration: &'a ProviderConfigurationFile,
    /// Provides the process-local public AI runtime and its live model catalogs.
    /// 提供进程内公共 AI 运行时及其实时模型目录。
    pub(crate) public_ai_runtime: Option<&'a vulcan_provider_adapters::PublicAiRuntime>,
    /// Selects whether this refresh commits a caller-owned candidate or the latest retained workspace.
    /// 选择本次刷新提交调用方持有的候选工作区，还是锁内读取的最新保留工作区。
    pub(crate) workspace_source: FullWorkbenchColdRefreshWorkspaceSource,
    /// Carries service logs to project into the rebuilt workbench.
    /// 携带需要投影到重建工作台中的服务日志。
    pub(crate) logs: &'a [String],
    /// Selects whether subagent navigation should be rebuilt from state or preserved from cache.
    /// 选择子代理导航应从状态重建还是从缓存保留。
    pub(crate) subagent_navigation_mode: SubagentNavigationRefreshMode,
}

/// Selects the authoritative workspace source for one cold workbench refresh.
/// 为一次冷路径工作台刷新选择权威工作区来源。
#[derive(Debug, Clone)]
pub(crate) enum FullWorkbenchColdRefreshWorkspaceSource {
    /// Commits one heap-bound exact caller-owned mutation candidate without inflating the source enum.
    /// 提交一个堆上保存的调用方精确变更候选，避免放大来源枚举。
    Candidate(Box<WorkspaceSnapshot>),
    /// Captures the latest retained workspace after acquiring the shared state lock.
    /// 获取共享状态锁后捕获最新保留工作区。
    Retained,
}

/// Commits one cold-path workspace and workbench projection under the caller's state lock.
/// 在调用方持有的状态锁下提交一次冷路径工作区与工作台投影。
///
/// # Parameters
/// 参数
/// `state` is the exclusively locked retained state, `workspace_snapshot` is authoritative, and `workbench` is its matching projection.
/// `state` 是独占锁定的保留状态，`workspace_snapshot` 是权威工作区，`workbench` 是与其匹配的投影。
///
/// # Returns
/// 返回值
/// Returns the committed workbench with the newly allocated revision.
/// 返回带有新分配修订号的已提交工作台。
fn commit_cold_workbench_projection(
    state: &mut ServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
    mut workbench: WorkbenchSnapshot,
) -> WorkbenchSnapshot {
    crate::preserve_project_settings_revisions(&mut workbench, &state.workbench);
    state.workbench_generation = state.workbench_generation.wrapping_add(1);
    workbench.revision = state.workbench_generation;
    state.workspace_snapshot = Some(workspace_snapshot);
    state.workbench = workbench.clone();
    workbench
}

/// Resolves the exact workspace used by one cold refresh while the caller holds the shared state lock.
/// 在调用方持有共享状态锁时解析一次冷刷新的精确工作区。
///
/// # Parameters
/// 参数
/// `retained_workspace` is the latest shared workspace and `workspace_source` selects it or an exact mutation candidate.
/// `retained_workspace` 是最新共享工作区，`workspace_source` 选择该工作区或一个精确变更候选。
///
/// # Returns
/// 返回值
/// The authoritative owned workspace, or an explicit not-ready error for a missing retained source.
/// 返回拥有所有权的权威工作区；保留来源缺失时返回明确的未就绪错误。
fn resolve_full_workbench_cold_refresh_workspace(
    retained_workspace: Option<&WorkspaceSnapshot>,
    workspace_source: FullWorkbenchColdRefreshWorkspaceSource,
) -> Result<WorkspaceSnapshot, String> {
    match workspace_source {
        FullWorkbenchColdRefreshWorkspaceSource::Candidate(workspace_snapshot) => {
            Ok(*workspace_snapshot)
        }
        FullWorkbenchColdRefreshWorkspaceSource::Retained => {
            retained_workspace.cloned().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })
        }
    }
}

/// Stores and emits one rebuilt workbench snapshot after a cold-path workspace mutation.
/// 在冷路径工作区变更后存储并发送一份重建后的工作台快照。
///
/// # Parameters
/// 参数
/// `request` groups the runtime handles, workspace source, logs, and subagent navigation mode for this refresh.
/// `request` 聚合本次刷新需要的运行时句柄、工作区来源、日志与子代理导航模式。
///
/// # Returns
/// 返回值
/// The rebuilt workbench snapshot after it has been stored and emitted.
/// 返回已存储并发送后的重建工作台快照。
pub(crate) fn store_and_emit_full_workbench_snapshot_for_cold_refresh(
    request: FullWorkbenchColdRefreshRequest<'_>,
) -> Result<WorkbenchSnapshot, String> {
    let FullWorkbenchColdRefreshRequest {
        app,
        state,
        controller_runtime,
        luaskills_catalog,
        mcp_catalog,
        provider_configuration,
        public_ai_runtime,
        workspace_source,
        logs,
        subagent_navigation_mode,
    } = request;
    let trace_started_at = current_unix_millis_value();
    let trace_lock_wait_started_at = current_unix_millis_value();
    // guard owns one uninterrupted capture, projection, and commit transaction.
    // guard 持有不中断的捕获、投影与提交事务。
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // workspaceSnapshot is resolved only after the state lock is held, so a retained-source refresh cannot overwrite a newer concurrent mutation.
    // workspaceSnapshot 仅在持有状态锁后解析，因此保留来源刷新不会覆盖更新的并发变更。
    let workspace_snapshot = resolve_full_workbench_cold_refresh_workspace(
        guard.workspace_snapshot.as_ref(),
        workspace_source,
    )?;
    let (
        live_context_budgets,
        subagent_navigation,
        pending_questions,
        session_goal_summaries,
        command_session_runtime,
        native_tools,
        vulcan_plugin_catalog,
        mcp_server_states,
        subagent_catalog,
    ) = {
        webui_trace(
            "store_workbench_lock_wait",
            format!(
                "mode={subagent_navigation_mode:?} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(trace_lock_wait_started_at)
            ),
        );
        webui_submit_trace(
            "store_workbench_lock_wait",
            format!(
                "mode={subagent_navigation_mode:?} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(trace_lock_wait_started_at)
            ),
        );
        let trace_surface_started_at = current_unix_millis_value();
        let live_context_budgets = guard.live_context_budgets.clone();
        let subagent_navigation = match subagent_navigation_mode {
            SubagentNavigationRefreshMode::Rebuild => {
                subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)?
            }
        };
        webui_trace(
            "store_workbench_surface_ready",
            format!(
                "mode={subagent_navigation_mode:?} has_subagent_navigation={} elapsed_ms={}",
                subagent_navigation.is_some(),
                current_unix_millis_value().saturating_sub(trace_surface_started_at)
            ),
        );
        webui_submit_trace(
            "store_workbench_surface_ready",
            format!(
                "mode={subagent_navigation_mode:?} has_subagent_navigation={} elapsed_ms={}",
                subagent_navigation.is_some(),
                current_unix_millis_value().saturating_sub(trace_surface_started_at)
            ),
        );
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
        let session_goal_summaries = guard.session_goal_summaries.clone();
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        let native_tools = native_tool_summaries_from_agent_runtime(guard.agent_runtime.as_ref());
        let vulcan_plugin_catalog = guard
            .agent_runtime
            .as_ref()
            .and_then(|runtime| runtime.extension_generation.plugin_catalog());
        let mcp_server_states = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.extension_generation.mcp_runtime().server_states())
            .unwrap_or_default();
        // subagentCatalog is cloned by Arc from the retained runtime generation; cold projection performs no directory traversal.
        // subagentCatalog 通过 Arc 从保留运行时代次克隆；冷投影不执行目录遍历。
        let subagent_catalog = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| Arc::clone(&runtime.subagent_catalog))
            .unwrap_or_else(|| Arc::new(vulcan_task_session::SubagentCatalog::default()));
        (
            live_context_budgets,
            subagent_navigation,
            pending_questions,
            session_goal_summaries,
            command_session_runtime,
            native_tools,
            vulcan_plugin_catalog,
            mcp_server_states,
            subagent_catalog,
        )
    };
    // vmm_session_states are immutable controller projections captured for this cold refresh.
    // vmm_session_states 是为本次冷刷新捕获的不可变控制器投影。
    let vmm_session_states = guard.vmm_controller_registry.snapshots()?;
    let session_provider_usage_summaries = session_provider_usage_summaries_from_v1_registry(
        guard.session_runtime_registry.as_ref(),
        workspace_snapshot.sessions.iter().map(|session| session.id),
    )?;
    let selected_context_usage_facts = session_context_usage_facts_from_v1_registry(
        guard.session_runtime_registry.as_ref(),
        workspace_snapshot.selected_session,
    )?;
    let trace_workbench_started_at = current_unix_millis_value();
    let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
        controller: controller_runtime,
        command_session_runtime: command_session_runtime.as_deref(),
        luaskills_catalog,
        vulcan_plugin_catalog: vulcan_plugin_catalog.as_deref(),
        subagent_catalog: subagent_catalog.as_ref(),
        mcp_server_states: &mcp_server_states,
        mcp_catalog,
        provider_configuration,
        public_ai_runtime,
        workspace: &workspace_snapshot,
        pending_prompt_process_mirrors: &guard.pending_prompt_process_mirrors,
        vmm_session_states: &vmm_session_states,
        managed_vmm_runtime: None,
        subagent_navigation,
        pending_questions,
        logs,
        live_context_budgets: &live_context_budgets,
        session_goal_summaries: &session_goal_summaries,
        session_provider_usage_summaries: &session_provider_usage_summaries,
        selected_context_usage_facts: selected_context_usage_facts.as_ref(),
        native_tools,
    });
    reconcile_workbench_conversation_lifecycle_projection(
        &mut workbench,
        &guard,
        &workspace_snapshot,
    )?;
    webui_trace(
        "store_workbench_built",
        format!(
            "mode={subagent_navigation_mode:?} conversations={} elapsed_ms={}",
            workbench.conversations.len(),
            current_unix_millis_value().saturating_sub(trace_workbench_started_at)
        ),
    );
    webui_submit_trace(
        "store_workbench_built",
        format!(
            "mode={subagent_navigation_mode:?} conversations={} elapsed_ms={}",
            workbench.conversations.len(),
            current_unix_millis_value().saturating_sub(trace_workbench_started_at)
        ),
    );
    let trace_commit_started_at = current_unix_millis_value();
    workbench = commit_cold_workbench_projection(&mut guard, workspace_snapshot, workbench);
    // Releasing the transaction before event serialization prevents emitter callbacks from observing the state lock.
    // 在事件序列化前释放事务，避免事件发射回调观察到状态锁仍被持有。
    drop(guard);
    webui_trace(
        "store_workbench_committed",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_commit_started_at)
        ),
    );
    webui_submit_trace(
        "store_workbench_committed",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_commit_started_at)
        ),
    );
    // Records the emitted workbench payload size only for the focused submit trace.
    // 仅为专项提交链路日志记录即将发送的工作台载荷大小。
    if webui_submit_trace_enabled() {
        let payload_bytes =
            serde_json::to_vec(&ServiceEvent::Workbench(Box::new(workbench.clone())))
                .map(|bytes| bytes.len())
                .unwrap_or(0);
        webui_submit_trace(
            "store_workbench_payload_ready",
            format!("mode={subagent_navigation_mode:?} payload_bytes={payload_bytes}"),
        );
    }
    let trace_emit_started_at = current_unix_millis_value();
    emit_workbench_event(app, &workbench);
    webui_trace(
        "store_workbench_emitted",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={} total_elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_emit_started_at),
            current_unix_millis_value().saturating_sub(trace_started_at)
        ),
    );
    webui_submit_trace(
        "store_workbench_emitted",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={} total_elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_emit_started_at),
            current_unix_millis_value().saturating_sub(trace_started_at)
        ),
    );
    Ok(workbench)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every public adaptive reasoning wire value is accepted by the workbench parser.
    /// 验证工作台解析器接受公共自适应推理传输值。
    #[test]
    fn reasoning_effort_parser_accepts_adaptive() {
        assert_eq!(
            parse_reasoning_effort("adaptive"),
            Some(ReasoningEffortLevel::Adaptive)
        );
    }

    /// Verifies the backend publishes only effort levels declared by the active model.
    /// 验证后端只发布当前模型声明的推理档位。
    #[test]
    fn reasoning_selector_uses_active_model_capabilities() {
        let configuration = ProviderConfigurationFile {
            workbench_reasoning_effort: Some(ReasoningEffortLevel::Ultra),
            workbench_reasoning_effort_explicit: true,
            ..ProviderConfigurationFile::default()
        };
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning_efforts: vec![
                vulcan_ai_contract::ReasoningEffort::Minimal,
                vulcan_ai_contract::ReasoningEffort::High,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };
        let selection = reasoning_selection(&configuration, Some(&capabilities))
            .expect("active reasoning levels should create a selector");
        assert_eq!(
            selection
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["minimal", "high"]
        );
        assert_eq!(selection.selected_effort, "minimal");
        assert!(selection.note.is_some());
    }

    /// Verifies legacy persisted `none` values are treated as stale defaults and re-enable reasoning.
    /// 验证旧版持久化的 `none` 会被视为过期默认值并重新启用思考。
    #[test]
    fn reasoning_selector_ignores_legacy_none_without_explicit_provenance() {
        let configuration = ProviderConfigurationFile {
            workbench_reasoning_effort: Some(ReasoningEffortLevel::None),
            ..ProviderConfigurationFile::default()
        };
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning_efforts: vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Medium,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection = reasoning_selection(&configuration, Some(&capabilities))
            .expect("legacy reasoning preference should still expose a selector");

        assert_eq!(selection.selected_effort, "medium");
    }

    /// Verifies an explicitly selected `none` value still disables reasoning.
    /// 验证用户明确选择的 `none` 仍然能够关闭思考。
    #[test]
    fn reasoning_selector_preserves_explicit_none() {
        let configuration = ProviderConfigurationFile {
            workbench_reasoning_effort: Some(ReasoningEffortLevel::None),
            workbench_reasoning_effort_explicit: true,
            ..ProviderConfigurationFile::default()
        };
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning_efforts: vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Medium,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection = reasoning_selection(&configuration, Some(&capabilities))
            .expect("explicit reasoning preference should still expose a selector");

        assert_eq!(selection.selected_effort, "none");
    }

    /// Verifies MiniMax exposes only its exact switch values and defaults to enabled adaptive thinking.
    /// 验证 MiniMax 仅暴露精确开关值，并默认启用自适应思考。
    #[test]
    fn reasoning_selector_removes_duplicate_default_and_enables_minimax() {
        let configuration = ProviderConfigurationFile::default();
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning_efforts: vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Adaptive,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection = reasoning_selection(&configuration, Some(&capabilities))
            .expect("MiniMax reasoning switch should create a selector");

        assert_eq!(
            selection
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["none", "adaptive"]
        );
        assert_eq!(selection.selected_effort, "adaptive");
    }

    /// Verifies MiniMax M2.x exposes one automatic adaptive value and no disabled selection.
    /// 验证 MiniMax M2.x 只暴露一个自动 adaptive 值且不存在关闭选项。
    #[test]
    fn reasoning_selector_exposes_minimax_m2_as_automatic_only() {
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning: vulcan_ai_contract::CapabilitySupport::Native,
            reasoning_control: vulcan_ai_contract::ReasoningControlMode::ProviderAdaptive,
            reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::Adaptive],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection =
            reasoning_selection(&ProviderConfigurationFile::default(), Some(&capabilities))
                .expect("MiniMax M2.x automatic reasoning should remain visible");

        assert_eq!(selection.options.len(), 1);
        assert_eq!(selection.options[0].value, "adaptive");
        assert_eq!(selection.options[0].label, "自动");
        assert_eq!(selection.selected_effort, "adaptive");
    }

    /// Verifies DeepSeek exposes its documented off, high, and max choices while defaulting on.
    /// 验证 DeepSeek 暴露文档化的关闭、高与最大选项，同时默认保持开启。
    #[test]
    fn reasoning_selector_exposes_deepseek_toggle_and_defaults_high() {
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning: vulcan_ai_contract::CapabilitySupport::Native,
            reasoning_control: vulcan_ai_contract::ReasoningControlMode::Toggle,
            reasoning_efforts: vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Max,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection =
            reasoning_selection(&ProviderConfigurationFile::default(), Some(&capabilities))
                .expect("DeepSeek reasoning toggle should create a selector");

        assert_eq!(
            selection
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["none", "low", "high", "max"]
        );
        assert_eq!(selection.selected_effort, "high");
    }

    /// Verifies models without adjustable levels remain visible as a fixed enabled state.
    /// 验证没有可调档位的思考模型仍以固定开启状态显示。
    #[test]
    fn reasoning_selector_shows_fixed_enabled_models() {
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning: vulcan_ai_contract::CapabilitySupport::Native,
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection =
            reasoning_selection(&ProviderConfigurationFile::default(), Some(&capabilities))
                .expect("fixed reasoning support should remain visible");

        assert_eq!(selection.options.len(), 1);
        assert_eq!(selection.options[0].value, "fixed-on");
        assert_eq!(selection.options[0].label, "开启");
        assert_eq!(selection.selected_effort, "fixed-on");
    }

    /// Verifies models without reasoning support remain visible as a fixed disabled state.
    /// 验证不支持思考的模型仍以固定关闭状态显示。
    #[test]
    fn reasoning_selector_shows_fixed_disabled_models() {
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection =
            reasoning_selection(&ProviderConfigurationFile::default(), Some(&capabilities))
                .expect("unsupported reasoning should remain visible");

        assert_eq!(selection.options.len(), 1);
        assert_eq!(selection.options[0].value, "fixed-off");
        assert_eq!(selection.options[0].label, "关闭");
        assert_eq!(selection.selected_effort, "fixed-off");
    }

    /// Verifies fixed-off metadata remains authoritative when stale levels are present.
    /// 验证存在旧档位时固定关闭元数据仍保持最高优先级。
    #[test]
    fn reasoning_selector_keeps_stale_fixed_off_disabled() {
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
            reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
            reasoning_efforts: vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::High,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection =
            reasoning_selection(&ProviderConfigurationFile::default(), Some(&capabilities))
                .expect("fixed-off reasoning should remain visible");

        assert_eq!(selection.options.len(), 1);
        assert_eq!(selection.selected_effort, "fixed-off");
    }

    /// Verifies an unknown text model receives the host default selectable reasoning levels.
    /// 验证未知文本模型会获得宿主默认可选思考档位。
    #[test]
    fn reasoning_selector_defaults_unknown_text_model_to_enabled_medium() {
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let selection =
            reasoning_selection(&ProviderConfigurationFile::default(), Some(&capabilities))
                .expect("unknown text model should expose host reasoning defaults");

        assert_eq!(
            selection
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["none", "minimal", "low", "medium", "high", "x-high", "max"]
        );
        assert_eq!(selection.selected_effort, "medium");
    }

    /// No-op shell host used to construct isolated persistence-version state.
    /// 用于构造隔离持久化版本状态的空操作壳宿主。
    struct PersistenceVersionTestShellHost;

    impl ServiceShellHost for PersistenceVersionTestShellHost {
        /// Ignores filesystem open requests in persistence-version tests.
        /// 在持久化版本测试中忽略文件系统打开请求。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        /// Ignores URL open requests in persistence-version tests.
        /// 在持久化版本测试中忽略 URL 打开请求。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Ok(())
        }
    }

    /// Builds one empty workspace with a distinct selected-session identity for cold-refresh tests.
    /// 为冷刷新测试构造一个带有不同选中会话身份的空工作区。
    ///
    /// # Parameters
    /// 参数
    /// `selected_session` is the identity used to distinguish candidate and retained workspaces.
    /// `selected_session` 是用于区分候选工作区与保留工作区的身份。
    ///
    /// # Returns
    /// 返回值
    /// An otherwise empty workspace snapshot carrying the requested identity.
    /// 返回一个仅携带指定身份、其他内容为空的工作区快照。
    fn cold_refresh_test_workspace(selected_session: SessionId) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies retained-source cold refreshes resolve the latest locked workspace instead of a detached candidate.
    /// 验证保留来源冷刷新解析锁内最新工作区，而不是分离候选。
    #[test]
    fn cold_refresh_retained_source_uses_latest_locked_workspace() {
        // candidate_session_id identifies the stale detached candidate used by mutation callers.
        // candidate_session_id 标识变更调用方持有的过期分离候选。
        let candidate_session_id = SessionId::from_u128(0x801);
        // retained_session_id identifies the newer workspace visible under the shared lock.
        // retained_session_id 标识共享锁内可见的更新工作区。
        let retained_session_id = SessionId::from_u128(0x802);
        // retained_workspace represents a concurrent update completed before final projection acquired the lock.
        // retained_workspace 表示最终投影取得锁前已经完成的并发更新。
        let retained_workspace = cold_refresh_test_workspace(retained_session_id);

        let resolved_retained = resolve_full_workbench_cold_refresh_workspace(
            Some(&retained_workspace),
            FullWorkbenchColdRefreshWorkspaceSource::Retained,
        )
        .expect("retained cold refresh source should resolve");
        let resolved_candidate = resolve_full_workbench_cold_refresh_workspace(
            Some(&retained_workspace),
            FullWorkbenchColdRefreshWorkspaceSource::Candidate(Box::new(
                cold_refresh_test_workspace(candidate_session_id),
            )),
        )
        .expect("candidate cold refresh source should resolve");

        assert_eq!(resolved_retained.selected_session, retained_session_id);
        assert_eq!(resolved_candidate.selected_session, candidate_session_id);
    }

    /// Verifies cold projection commit advances the current generation and stores one matching pair.
    /// 验证冷投影提交会推进当前代次并存储一组相互匹配的快照。
    #[test]
    fn cold_workbench_projection_commits_workspace_and_revision_together() {
        // state owns the exact generation and retained snapshots under test.
        // state 持有本测试的精确代次与保留快照。
        let state =
            create_service_bridge_state(std::sync::Arc::new(PersistenceVersionTestShellHost));
        // workspace_snapshot is the authoritative cold-path mutation result.
        // workspace_snapshot 是权威冷路径变更结果。
        let workspace_snapshot = cold_refresh_test_workspace(SessionId::nil());
        // workbench starts without a revision because commit owns revision allocation.
        // workbench 初始没有修订号，因为修订号由提交阶段分配。
        let workbench = fallback_workbench_snapshot(&[]);
        let mut guard = state.lock().expect("lock isolated service state");
        guard.workbench_generation = 41;

        let committed =
            commit_cold_workbench_projection(&mut guard, workspace_snapshot.clone(), workbench);

        assert_eq!(committed.revision, 42);
        assert_eq!(guard.workbench_generation, 42);
        assert_eq!(guard.workbench.revision, 42);
        // retained_workspace proves the authoritative workspace was stored in the same critical section.
        // retained_workspace 证明权威工作区已在同一临界区内存储。
        let retained_workspace = guard
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot should commit with workbench");
        assert_eq!(
            retained_workspace.selected_session,
            workspace_snapshot.selected_session
        );
        assert_eq!(
            retained_workspace.sessions.len(),
            workspace_snapshot.sessions.len()
        );
    }
}
