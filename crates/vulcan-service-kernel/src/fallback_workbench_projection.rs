//! fallback workbench projection helpers.
//! fallback workbench projection 辅助逻辑。

use super::*;

/// Builds a fallback workbench snapshot used before real startup completes or after failure.
/// 构建真实启动完成前或失败后的回退工作台快照。
pub(crate) fn fallback_workbench_snapshot(logs: &[String]) -> WorkbenchSnapshot {
    let workspace_root = workspace_root();
    WorkbenchSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ui_locale: ProviderConfigurationFile::default().ui_locale,
        modules: vec![
            runtime_module(
                "core",
                "Core Service",
                StartupStepStatus::Running,
                "服务正在启动",
            ),
            runtime_module(
                "vldb",
                "VLDB",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "luaskills",
                "LuaSkills",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "token-budget",
                "Token Budget",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "mcp",
                "MCP Runtime",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "workspace",
                "Workspace",
                StartupStepStatus::Pending,
                "等待真实工作区快照",
            ),
        ],
        projects: vec![ProjectSummary {
            id: String::from("current-workspace"),
            name: String::from("vulcan-code"),
            path: normalized_workspace_path_text_from_path(&workspace_root),
            pinned: false,
            icon_pack: None,
            icon_key: None,
        }],
        expanded_project_ids: vec![String::from("current-workspace")],
        conversations: Vec::new(),
        archived_projects: Vec::new(),
        archived_conversations: Vec::new(),
        selected_conversation_id: None,
        messages: Vec::new(),
        hydrated_conversation_windows: Vec::new(),
        conversation_hydration: None,
        subagent_navigation: None,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: Vec::new(),
        lua_skills: lua_skills_catalog_summary(
            &empty_luaskills_catalog(),
            &ProviderConfigurationFile::default(),
        ),
        agent_skills: agent_skills_catalog_summary(&ProviderConfigurationFile::default()),
        settings: settings(),
        providers: providers(&ProviderConfigurationFile::default()),
        mcp_servers: mcp_servers(&ProviderConfigurationFile::default()),
        reasoning_selection: reasoning_selection(&ProviderConfigurationFile::default()),
        thinking_display_mode: workbench_thinking_display_mode(
            &ProviderConfigurationFile::default(),
        ),
        git_commit: git_commit_settings(&ProviderConfigurationFile::default()),
        host_tools: host_tools_settings(&ProviderConfigurationFile::default()),
        ai_capabilities: ai_capabilities(&ProviderConfigurationFile::default().ai_tools, None),
        default_ai_capabilities: ai_capabilities(
            &ProviderConfigurationFile::default().ai_tools,
            None,
        ),
        ai_capability_vendors: ai_capability_vendors(
            &ProviderConfigurationFile::default().ai_tools,
        ),
        ai_capability_vendor_catalog: ai_capability_vendor_catalog(),
        composer_attachment_capabilities: composer_attachment_capabilities(
            &ProviderConfigurationFile::default(),
            &ProviderConfigurationFile::default().ai_tools,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: Vec::new(),
        pending_question: None,
        session_todolist: None,
        session_goal: None,
        session_plan_mode: None,
        memories: Vec::new(),
        vmm_memory: VmmMemorySnapshot::default(),
        command_sessions: Vec::new(),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("fallback-{index}"), "info", "service", message)
            })
            .collect(),
        context_injection: None,
        context_usage: None,
    }
}

/// Appends retained hydrated conversation bodies that are not already present in one WebUI snapshot.
/// 追加尚未出现在 WebUI 快照中的已保留水合会话正文。
pub(crate) fn append_hydrated_workspace_messages(
    messages: &mut Vec<ConversationMessage>,
    workspace: &WorkspaceSnapshot,
) {
    let mut projected_conversation_ids = messages
        .iter()
        .map(|message| message.conversation_id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    for session_id in &workspace.hydrated_session_ids {
        let conversation_id = session_id.to_string();
        if projected_conversation_ids.contains(&conversation_id) {
            continue;
        }
        let Some(session_messages) = workspace.session_messages.get(session_id) else {
            continue;
        };
        messages.extend(conversation_messages_from_session(
            &conversation_id,
            session_messages,
            false,
        ));
        projected_conversation_ids.insert(conversation_id);
    }
}

/// Creates feature areas matching the WebUI navigation surface.
/// 创建与 WebUI 导航面相匹配的功能区域。
pub(crate) fn feature_areas() -> Vec<FeatureAreaSummary> {
    vec![
        feature_area("chat", "控制台", "控制台、消息流与模型输入区", "真实快照"),
        feature_area("projects", "项目", "工作区项目列表与绑定状态", "真实快照"),
        feature_area(
            "providers",
            "模型",
            "Provider、模型、能力配置",
            "待接入配置详情",
        ),
        feature_area("tools", "工具", "工具调用、命令会话与输出", "待接入运行态"),
        feature_area(
            "plugins",
            "插件",
            "LuaSkills 与高级插件控制面板",
            "真实目录",
        ),
        feature_area(
            "memory",
            "VMM 记忆",
            "用户、项目、画像与记忆节点",
            "会话绑定快照",
        ),
        feature_area(
            "settings",
            "设置",
            "运行时、界面、权限与远程访问设置",
            "基础区域",
        ),
        feature_area("logs", "诊断", "启动日志、错误与运行时事件", "真实启动日志"),
    ]
}

/// Creates one feature area summary.
/// 创建单个功能区域摘要。
pub(crate) fn feature_area(
    id: &str,
    label: &str,
    description: &str,
    status: &str,
) -> FeatureAreaSummary {
    FeatureAreaSummary {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        status: status.to_string(),
    }
}
