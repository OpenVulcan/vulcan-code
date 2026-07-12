use super::*;

/// Builds the workspace-describe host-owned native tool.
/// 构建工作区概览宿主自有原生工具。
pub(crate) fn build_workspace_describe_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("native_workspace_describe"),
        name: String::from("Workspace Summary"),
        description: String::from(
            "Return the current workspace directory, active model, and basic runtime statistics.",
        ),
        help_text: None,
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "activeModel": snapshot.active_model,
                    "agentCount": snapshot.agent_count,
                    "skillCount": snapshot.skill_count,
                    "configuredMcpServers": snapshot.configured_mcp_server_ids,
                    "enabledMcpServers": snapshot.enabled_mcp_server_ids,
                })))
            })
        },
    )
}

/// Builds the agent-list host-owned native tool.
/// 构建代理列表宿主自有原生工具。
pub(crate) fn build_agent_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<NativeToolDefinition> {
    let items = config
        .agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "kind": match agent.kind {
                    RuntimeAgentKind::Worker => "worker",
                    RuntimeAgentKind::Planner => "planner",
                    RuntimeAgentKind::Explorer => "explorer",
                    RuntimeAgentKind::Specialist => "specialist",
                    RuntimeAgentKind::Coordinator => "coordinator",
                },
                "description": agent.description,
                "systemPrompt": agent.system_prompt,
                "skills": agent.skill_ids,
                "nativeTools": agent.native_tools,
                "pluginTools": agent.plugin_tools,
                "mcpTools": agent.mcp_tools,
                "delegates": agent.delegates,
                "toolPolicy": agent.tool_policy,
                "fallbackDelegate": agent.fallback_delegate,
                "model": agent.model,
                "temperature": agent.temperature,
                "maxTurns": agent.max_turns,
                "routes": agent
                    .routes
                    .iter()
                    .map(|route| {
                        json!({
                            "signal": route.signal,
                            "delegate": route.delegate,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let descriptor = NativeToolDescriptor {
        id: String::from("native_registry_list_agents"),
        name: String::from("List Agents"),
        description: String::from("Return the currently declared agent catalog."),
        help_text: None,
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let items = items.clone();
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "agents": &*items,
                })))
            })
        },
    )
}

/// Builds the skill-list host-owned native tool.
/// 构建技能列表宿主自有原生工具。
pub(crate) fn build_skill_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
    config: &AgentRuntimeConfig,
) -> Arc<NativeToolDefinition> {
    let items = config
        .skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "kind": match skill.kind {
                    RuntimeSkillKind::Regular => "regular",
                    RuntimeSkillKind::LuaSkills => "luaskills",
                    RuntimeSkillKind::Automation => "automation",
                    RuntimeSkillKind::Memory => "memory",
                },
                "description": skill.description,
                "enabled": skill.enabled,
                "triggerSignals": skill.trigger_signals,
                "emittedSignal": skill.emitted_signal,
                "confidenceDelta": skill.confidence_delta,
            })
        })
        .collect::<Vec<_>>();
    let items = Arc::new(items);
    let descriptor = NativeToolDescriptor {
        id: String::from("native_registry_list_skills"),
        name: String::from("List Skills"),
        description: String::from("Return the currently declared skill catalog."),
        help_text: None,
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let items = items.clone();
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "workspaceDirectory": snapshot.workspace_directory,
                    "skills": &*items,
                })))
            })
        },
    )
}

/// Builds the MCP-server list host-owned native tool.
/// 构建 MCP 服务器列表宿主自有原生工具。
pub(crate) fn build_mcp_server_list_tool(
    snapshot: Arc<NativeToolRuntimeSnapshot>,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from("native_mcp_list_servers"),
        name: String::from("List MCP Servers"),
        description: String::from("Return the current MCP server configuration and enabled state."),
        help_text: None,
        ai_visible: false,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({"type": "object"}),
        move |_request: NativeToolExecutionRequest| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "configured": snapshot.configured_mcp_server_ids,
                    "enabled": snapshot.enabled_mcp_server_ids,
                })))
            })
        },
    )
}
