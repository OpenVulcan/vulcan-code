//! fallback workbench projection helpers.
//! fallback workbench projection 辅助逻辑。

use super::*;

/// Builds a fallback workbench snapshot used before real startup completes or after failure.
/// 构建真实启动完成前或失败后的回退工作台快照。
pub(crate) fn fallback_workbench_snapshot(logs: &[String]) -> WorkbenchSnapshot {
    let workspace_root = workspace_root();
    let provider_configuration = ProviderConfigurationFile::default();
    WorkbenchSnapshot {
        revision: 0,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ui_locale: provider_configuration.ui_locale.clone(),
        modules: vec![
            runtime_module(
                "core",
                "Core Service",
                StartupStepStatus::Running,
                "Service is starting",
            ),
            runtime_module(
                "data-store",
                "Data store",
                StartupStepStatus::Pending,
                "Waiting for the real startup flow",
            ),
            runtime_module(
                "luaskills",
                "LuaSkills",
                StartupStepStatus::Pending,
                "Waiting for the real startup flow",
            ),
            runtime_module(
                "mcp",
                "MCP Runtime",
                StartupStepStatus::Pending,
                "Waiting for the real startup flow",
            ),
            runtime_module(
                "workspace",
                "Workspace",
                StartupStepStatus::Pending,
                "Waiting for the real workspace snapshot",
            ),
        ],
        projects: vec![ProjectSummary {
            id: String::from("current-workspace"),
            name: String::from("vulcan-code"),
            path: normalized_workspace_path_text_from_path(&workspace_root),
            pinned: false,
            icon_pack: None,
            icon_key: None,
            session_sort_mode: Default::default(),
        }],
        expanded_project_ids: vec![String::from("current-workspace")],
        project_tree: Default::default(),
        project_settings_revisions: vulcan_service_api::ProjectSettingsRevisionSnapshot::default(),
        conversations: Vec::new(),
        archived_projects: Vec::new(),
        archived_conversations: Vec::new(),
        selected_conversation_id: None,
        subagent_navigation: None,
        tool_calls: tool_calls(),
        plugins: Vec::new(),
        lua_skills: lua_skills_catalog_summary(&empty_luaskills_catalog(), &provider_configuration),
        agent_skills: agent_skills_catalog_summary(&provider_configuration, None),
        // Bootstrap fallback must remain allocation-only; the real retained catalog is published after startup loading.
        // 启动降级快照必须仅执行内存分配；真实保留目录会在启动加载后发布。
        subagent_catalog: subagent_catalog_summary_from_catalog(
            &vulcan_task_session::SubagentCatalog::default(),
            None,
        ),
        settings: settings(),
        providers: Vec::new(),
        mcp_servers: mcp_servers(&provider_configuration),
        mcp_tools: Vec::new(),
        reasoning_selection: reasoning_selection(&provider_configuration, None),
        thinking_display_mode: workbench_thinking_display_mode(&provider_configuration),
        tool_collapse_default_enabled: Some(
            provider_configuration
                .tool_collapse_default_enabled
                .unwrap_or(true),
        ),
        git_commit: git_commit_settings(&provider_configuration),
        provider_retry: provider_retry_settings(&provider_configuration),
        provider_request_trace_enabled: provider_configuration.provider_request_trace_enabled,
        provider_error_trace_enabled: provider_configuration.provider_error_trace_enabled,
        host_tools: host_tools_settings(&provider_configuration),
        composer_attachment_capabilities: composer_attachment_capabilities(
            None,
            None,
            &BTreeSet::new(),
        ),
        output_style_catalog: output_style_catalog_summary(&provider_configuration),
        default_answer_mode: crate::answer_mode::global_answer_mode(&provider_configuration),
        permission_judge_backend: provider_configuration.permission_judge.backend,
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: Vec::new(),
        pending_prompts_by_conversation: Vec::new(),
        pending_question: None,
        pending_questions: Vec::new(),
        session_todolist: None,
        session_goal: None,
        session_plan_mode: None,
        session_vmm_policy: None,
        session_output_style: None,
        context_compaction_window_nodes: Vec::new(),
        session_context_compaction_window_node_id: None,
        session_provider_usage: None,
        session_addons_by_conversation: Vec::new(),
        session_vmm_policies_by_conversation: Vec::new(),
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
        context_injection_by_conversation: Vec::new(),
        context_usage: None,
    }
}
