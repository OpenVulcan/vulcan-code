use super::prepare_execution_with_tool_snapshot;
use super::tool_exposure::{
    ExecutionToolExposureSelection, ToolExposureSelectorState,
    ai_tool_configuration_for_execution_request, filter_luaskills_entries, filter_native_tool_ids,
    filter_resolved_tool_set_by_tool_exposure, vulcan_tool_descriptor_is_visible,
};
use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::PLAN_AGENT_ID;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionPromptPurpose};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::SessionConversationState;
use crate::mcp::configured_mcp_runtime_access;
use crate::provider_config::{
    AiToolKind, PersistedAiToolConfiguration, PersistedProviderEntry, PersistedProviderOptions,
    ProjectPromptSkillsConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
};
use crate::tooling::native::{
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use luaskills::RuntimeEntryDescriptor;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;
use vulcan_session_core::{
    ExecutionRuntimePolicy, ExecutionScope, SessionNodeId, ToolExposureSnapshot,
};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{
    ResolvedToolDefinition, ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolDescriptor,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};
use vulcan_vmm::VmmConfigLayer;

/// Verifies request-scoped agent routing makes preparation use the requested agent instead of the global active worker.
/// 验证请求级代理路由会让执行准备使用指定代理，而不是全局激活的默认工作代理。
#[test]
fn prepare_execution_uses_request_execution_agent_override() {
    let config = test_provider_config();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &config,
        &AgentProfileCatalog::default(),
        None,
        Some(configured_mcp_runtime_access(&config)),
    );
    let mut request = test_execution_preparation_request();
    request.execution_agent_id = Some(String::from(PLAN_AGENT_ID));

    let preparation = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("preparation should route to the requested agent")
        .preparation;

    assert_eq!(preparation.agent_id, PLAN_AGENT_ID);
    assert_eq!(preparation.agent_name, "方案代理");
    assert_eq!(preparation.agent_kind, "规划代理");
    assert!(preparation.delegate_agents.is_empty());
}

/// Verifies an unknown request-scoped execution agent fails instead of silently falling back to the active agent.
/// 验证未知的请求级执行代理会失败，而不是静默回退到当前激活代理。
#[test]
fn prepare_execution_rejects_missing_request_execution_agent() {
    let config = test_provider_config();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &config,
        &AgentProfileCatalog::default(),
        None,
        Some(configured_mcp_runtime_access(&config)),
    );
    let mut request = test_execution_preparation_request();
    request.execution_agent_id = Some(String::from("missing-agent"));

    let error = match prepare_execution_with_tool_snapshot(&foundation, None, &config, &request) {
        Ok(_) => panic!("missing requested agent should fail"),
        Err(error) => error,
    };

    assert!(error.contains("missing-agent"));
    assert!(error.contains("未能在运行时基础架构中解析到"));
}

/// Verifies global request selectors filter native tool identifiers through the shared preparation helper.
/// 验证全局请求选择器会通过共享准备辅助逻辑过滤原生工具标识。
#[test]
fn execution_tool_exposure_filters_native_ids_with_global_selectors() {
    let snapshot = ToolExposureSnapshot {
        allowed_tools: vec![
            String::from("shell command"),
            String::from("vulcan_file_read"),
        ],
        disabled_tools: vec![String::from("vulcan-file-read")],
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

    let filtered = filter_native_tool_ids(
        vec![
            String::from("shell_command"),
            String::from("vulcan_file_read"),
            String::from("vmm_memory_write"),
            String::from(GET_GOAL_TOOL_ID),
            String::from(TODOLIST_TOOL_ID),
        ],
        &selectors,
    );

    assert_eq!(
        filtered,
        vec!["shell_command", GET_GOAL_TOOL_ID, TODOLIST_TOOL_ID]
    );
}

/// Verifies request-scoped selectors cannot hide universal Goal and todolist native tools.
/// 验证请求级选择器无法隐藏通用 Goal 与 todolist 原生工具。
#[test]
fn execution_tool_exposure_keeps_universal_goal_and_todolist_tools() {
    let snapshot = ToolExposureSnapshot {
        allowed_tools: vec![String::from("shell_command")],
        disabled_tools: vec![
            String::from(CREATE_GOAL_TOOL_ID),
            String::from(GET_GOAL_TOOL_ID),
            String::from(UPDATE_GOAL_TOOL_ID),
            String::from(TODOLIST_TOOL_ID),
        ],
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

    for tool_id in [
        TODOLIST_TOOL_ID,
        GET_GOAL_TOOL_ID,
        CREATE_GOAL_TOOL_ID,
        UPDATE_GOAL_TOOL_ID,
    ] {
        assert!(vulcan_tool_descriptor_is_visible(
            tool_id,
            tool_id,
            "Universal native tool.",
            VulcanToolSourceKind::Native,
            &selectors,
        ));
    }
}

/// Verifies LuaSkills JSON overrides filter direct and implicit entries without TaskRuntime-side cloning.
/// 验证 LuaSkills JSON 覆盖会过滤直接与隐式入口，而不依赖 TaskRuntime 侧克隆逻辑。
#[test]
fn execution_tool_exposure_filters_luaskills_with_family_overrides() {
    let snapshot = ToolExposureSnapshot {
        luaskills_overrides: json!({
            "enabled": ["vulcan-codekit"],
            "disabled": ["ast-detail"]
        }),
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);
    let entries = vec![
        test_luaskills_entry("vulcan-codekit", "rg"),
        test_luaskills_entry("vulcan-codekit", "ast-detail"),
        test_luaskills_entry("vulcan-curl", "get"),
    ];

    let filtered = filter_luaskills_entries(entries, &selectors);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].canonical_name, "vulcan-codekit-rg");
}

/// Verifies AI capability selectors narrow the effective AI-tool configuration before provider routing.
/// 验证 AI 能力选择器会在 Provider 路由之前收窄生效 AI 工具配置。
#[test]
fn execution_tool_exposure_applies_ai_capability_allow_and_deny() {
    let snapshot = ToolExposureSnapshot {
        allowed_ai_capabilities: vec![String::from("image-generation"), String::from("web_search")],
        disabled_ai_capabilities: vec![String::from("web search")],
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

    let configuration = ai_tool_configuration_for_execution_request(
        &PersistedAiToolConfiguration::default(),
        &selectors,
    );

    assert!(
        !configuration
            .disabled_capability_kinds
            .contains(&AiToolKind::ImageGeneration)
    );
    assert!(
        configuration
            .disabled_capability_kinds
            .contains(&AiToolKind::WebSearch)
    );
    assert!(
        configuration
            .disabled_capability_kinds
            .contains(&AiToolKind::MusicGeneration)
    );
}

/// Verifies legacy global tool allow-lists do not hide configured AI fallback tools.
/// 验证旧版全局工具白名单不会隐藏已配置的 AI 回退工具。
#[test]
fn ai_fallback_visibility_ignores_legacy_global_tool_allow_list() {
    let snapshot = ToolExposureSnapshot {
        allowed_tools: vec![String::from("shell_command")],
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

    assert!(vulcan_tool_descriptor_is_visible(
        "understand_image",
        "understand_image",
        "Read image attachments",
        VulcanToolSourceKind::AiFallback,
        &selectors,
    ));
    assert!(!vulcan_tool_descriptor_is_visible(
        "read_file",
        "read_file",
        "Read one file",
        VulcanToolSourceKind::Native,
        &selectors,
    ));
}

/// Verifies LuaSkills family allow-list keeps the shared dispatcher even when global tool allow-list is also present.
/// 验证 LuaSkills 族允许列表与全局工具允许列表并存时仍会保留共享调度器。
#[test]
fn execution_tool_exposure_keeps_luaskills_dispatcher_with_family_allow() {
    let snapshot = ToolExposureSnapshot {
        allowed_tools: vec![String::from("shell_command")],
        luaskills_overrides: json!({
            "enabled": ["vulcan-codekit"]
        }),
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);
    // Candidate native shell tool expected to survive both descriptor and definition filtering.
    // 预期在描述与定义两侧过滤后都保留的原生命令工具候选。
    let shell_tool = test_resolved_tool("shell_command", VulcanToolSourceKind::Native);
    // Candidate LuaSkills dispatcher expected to survive because the family allow-list needs it.
    // 由于 LuaSkills 族允许列表需要调度器，预期在过滤后保留的调度工具候选。
    let luaskills_dispatcher =
        test_resolved_tool(CALL_LUASKILL_TOOL_NAME, VulcanToolSourceKind::Native);
    // Candidate ask tool expected to be removed by the legacy global allow-list.
    // 预期会被旧版全局允许列表移除的 ask 工具候选。
    let ask_tool = test_resolved_tool("ask", VulcanToolSourceKind::Native);
    let mut resolved = ResolvedToolSet {
        descriptors: vec![
            shell_tool.descriptor.clone(),
            luaskills_dispatcher.descriptor.clone(),
            ask_tool.descriptor.clone(),
        ],
        definitions: vec![shell_tool, luaskills_dispatcher, ask_tool],
    };
    let exposure = ExecutionToolExposureSelection {
        native_tool_ids: Vec::new(),
        luaskills_tools: Vec::new(),
        luaskills_dispatch_tools: Vec::new(),
        luaskill_prompt_entries: Vec::new(),
        mcp_tools: Vec::new(),
        ai_tool_configuration: PersistedAiToolConfiguration::default(),
        selectors,
    };

    filter_resolved_tool_set_by_tool_exposure(&mut resolved, &exposure);

    let remaining = resolved
        .definitions
        .iter()
        .map(|definition| definition.descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(remaining, vec!["shell_command", CALL_LUASKILL_TOOL_NAME]);
    let remaining_descriptors = resolved
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        remaining_descriptors,
        vec!["shell_command", CALL_LUASKILL_TOOL_NAME]
    );
}

/// Verifies execution preparation stores prompt descriptors and provider definitions from one resolved tool snapshot.
/// 验证执行准备会从同一份已解析工具快照保存提示词描述与 Provider 定义。
#[test]
fn prepare_execution_captures_matching_tool_descriptor_and_definition_snapshot() {
    let config = test_provider_config();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &config,
        &AgentProfileCatalog::default(),
        None,
        Some(configured_mcp_runtime_access(&config)),
    );
    let request = test_execution_preparation_request();

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("preparation should resolve one stable tool snapshot");

    let descriptor_ids = prepared
        .preparation
        .tool_descriptors
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<Vec<_>>();
    let definition_ids = prepared
        .tool_snapshot
        .resolved_tool_definitions
        .iter()
        .map(|definition| definition.descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert!(
        !descriptor_ids.is_empty(),
        "test fixture should expose at least one tool"
    );
    assert_eq!(descriptor_ids, definition_ids);
}

/// Verifies context-compaction preparation clears every provider-visible tool snapshot.
/// 验证上下文压缩准备阶段会清空所有 Provider 可见工具快照。
#[test]
fn prepare_execution_hides_tools_for_context_compaction_purpose() {
    let config = test_provider_config();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &config,
        &AgentProfileCatalog::default(),
        None,
        Some(configured_mcp_runtime_access(&config)),
    );
    let mut request = test_execution_preparation_request();
    request.prompt_purpose = ExecutionPromptPurpose::ContextCompaction;
    request.runtime_policy = ExecutionRuntimePolicy::context_compaction();
    request.execution_scope = ExecutionScope::ContextCompaction {
        node_id: SessionNodeId::from(request.session_id),
    };
    request.execution_mode = String::from("Compaction");

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("context compaction preparation should still resolve");

    assert!(prepared.preparation.tool_descriptors.is_empty());
    assert!(prepared.tool_snapshot.resolved_tool_definitions.is_empty());
}

/// Verifies cache-preserving compaction preparation keeps provider-visible tool snapshots.
/// 验证保缓存压缩准备阶段会保留 Provider 可见工具快照。
#[test]
fn prepare_execution_keeps_tools_for_cache_preserving_context_compaction_purpose() {
    let config = test_provider_config();
    let foundation = RuntimeFoundation::build_from_optional_mcp_access(
        &config,
        &AgentProfileCatalog::default(),
        None,
        Some(configured_mcp_runtime_access(&config)),
    );
    let mut request = test_execution_preparation_request();
    request.prompt_purpose = ExecutionPromptPurpose::ContextCompactionCachePreserving;
    request.runtime_policy = ExecutionRuntimePolicy::context_compaction();
    request.execution_scope = ExecutionScope::ContextCompaction {
        node_id: SessionNodeId::from(request.session_id),
    };
    request.execution_mode = String::from("Compaction");

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("cache-preserving context compaction preparation should still resolve");

    assert!(
        !prepared.preparation.tool_descriptors.is_empty(),
        "cache-preserving compaction must keep tool descriptors for provider cache continuity"
    );
    assert!(
        !prepared.tool_snapshot.resolved_tool_definitions.is_empty(),
        "cache-preserving compaction must keep provider-visible tool definitions"
    );
}

/// Builds one provider configuration that can resolve a provider blueprint without touching the network.
/// 构建一份无需访问网络即可解析 Provider 蓝图的供应商配置。
fn test_provider_config() -> ProviderConfigurationFile {
    let mut provider = BTreeMap::new();
    provider.insert(
        String::from("deepseek"),
        PersistedProviderEntry {
            name: Some(String::from("DeepSeek")),
            options: PersistedProviderOptions {
                api_key: Some(String::from("test-key")),
                ..PersistedProviderOptions::default()
            },
            ..PersistedProviderEntry::default()
        },
    );
    ProviderConfigurationFile {
        model: Some(String::from("deepseek/deepseek-v4-pro")),
        provider,
        ..ProviderConfigurationFile::default()
    }
}

/// Builds one minimal LuaSkills runtime entry descriptor for selector tests.
/// 为选择器测试构建一条最小 LuaSkills 运行时入口描述。
fn test_luaskills_entry(skill_id: &str, local_name: &str) -> RuntimeEntryDescriptor {
    RuntimeEntryDescriptor {
        canonical_name: format!("{skill_id}-{local_name}"),
        skill_id: skill_id.to_string(),
        local_name: local_name.to_string(),
        root_name: String::from("ROOT"),
        skill_dir: String::from("skill-dir"),
        description: String::from("test entry"),
        parameters: Vec::new(),
        input_schema: json!({
            "type": "object"
        }),
    }
}

/// Builds one minimal resolved tool definition for public selector tests.
/// 为公开选择器测试构建一条最小已解析工具定义。
fn test_resolved_tool(id: &str, source: VulcanToolSourceKind) -> ResolvedToolDefinition {
    ResolvedToolDefinition {
        descriptor: VulcanToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description: String::from("test tool"),
            help_text: None,
            source,
        },
        parameters_schema: json!({ "type": "object" }),
        plan_policy: VulcanPlanToolPolicy::Visible,
        invoke: Arc::new(|_, _| {
            Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!({}))) })
        }),
    }
}

/// Builds one minimal execution-preparation request for agent-routing tests.
/// 为代理路由测试构建一份最小执行准备请求。
fn test_execution_preparation_request() -> ExecutionPreparationRequest {
    let session_id = Uuid::nil();
    ExecutionPreparationRequest {
        session_id,
        execution_scope: ExecutionScope::MainSession {
            session_id: SessionNodeId::from(session_id),
        },
        runtime_policy: ExecutionRuntimePolicy::main_session(),
        execution_agent_id: None,
        agent_system_prompt_override: None,
        task_session_store_root: None,
        session_title: String::from("测试会话"),
        workspace_path: String::from(r"D:\workspace"),
        prompt: String::from("请规划当前任务"),
        conversation_state: SessionConversationState::default(),
        execution_mode: String::from("Build"),
        temperature: None,
        reasoning_effort: ReasoningEffortLevel::Off,
        ai_tool_configuration: PersistedAiToolConfiguration::default(),
        tool_exposure_override: Default::default(),
        prompt_purpose: ExecutionPromptPurpose::UserTurn,
        project_prompt_skills: ProjectPromptSkillsConfiguration::default(),
        session_prompt_skill_overrides: BTreeMap::new(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: VmmConfigLayer::default(),
        session_vmm: VmmConfigLayer::default(),
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    }
}
