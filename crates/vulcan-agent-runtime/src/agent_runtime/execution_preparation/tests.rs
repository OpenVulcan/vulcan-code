use super::tool_exposure::{
    ExecutionToolExposureSelection, ToolExposureSelectorState, filter_luaskills_entries,
    filter_native_tool_ids, filter_resolved_tool_set_by_tool_exposure,
    retain_visible_luaskill_help_packages, vulcan_tool_descriptor_is_visible,
};
use super::{append_vulcan_plugin_prompt_contributions, prepare_execution_with_tool_snapshot};
use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::PLAN_AGENT_ID;
use crate::agent_runtime::execution_models::{
    ExecutionKnowledgeAccessContext, ExecutionPreparationRequest, ExecutionPromptPurpose,
};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::SessionConversationState;
use crate::mcp::configured_mcp_runtime_access;
use crate::provider_config::{
    ProjectPromptSkillsConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
};
use crate::tooling::native::{
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, KNOWLEDGE_BROWSE_TOOL_ID, KNOWLEDGE_READ_TOOL_ID,
    KNOWLEDGE_SEARCH_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use luaskills::RuntimeEntryDescriptor;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use vulcan_hooks::{SystemPromptHookBlock, SystemPromptHookPhase};
use vulcan_luaskills_prompt::{LuaSkillPackageHelp, LuaSkillPromptEntry, LuaSkillRegistration};
use vulcan_plugin_runtime::{VulcanPluginCatalogSnapshot, VulcanPluginPromptContribution};
use vulcan_prompt_composer::PromptBaselineKind;
use vulcan_session_core::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ExecutionRuntimePolicy, ExecutionScope,
    SessionNodeId, ToolExposureSnapshot,
};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{
    ResolvedToolDefinition, ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolDescriptor,
    VulcanToolInvocationResult, VulcanToolOrigin, VulcanToolSourceKind,
};
use vulcan_vmm::VmmConfigLayer;

/// Verifies visible CodeKit entries inject the package main help with its stable provenance; returns no value.
/// 验证可见的 CodeKit 入口按稳定来源标识注入包主帮助；无参数与返回值。
#[test]
fn visible_codekit_injects_package_main_help_without_vce() {
    // A fixture marker proves the main-help payload reaches the prompt without freezing published wording.
    // 夹具标识证明主帮助载荷进入提示词，不绑定已发布文案。
    let package = LuaSkillPackageHelp::new(
        "vulcan-codekit",
        "",
        "codekit_main_help_fixture",
        Vec::new(),
        vec![LuaSkillPromptEntry::new(
            "vulcan-codekit-rg",
            "",
            LuaSkillRegistration::Direct,
        )],
    );
    // The descriptor uses the same LuaSkills origin identity as real registered entries.
    // 描述符使用真实注册入口相同的 LuaSkills 来源身份。
    let visible = test_resolved_tool("vulcan-codekit-rg", VulcanToolSourceKind::LuaSkills);
    // Collect only the package contribution produced by the request assembly helper.
    // 仅收集请求组装辅助函数产生的包贡献。
    let mut segments = Vec::new();
    super::append_luaskill_package_segments(
        &mut segments,
        std::slice::from_ref(&package),
        &[visible.descriptor],
    );
    // Locate by producer identity so unrelated future contributions cannot shift the assertion.
    // 按生产者身份定位，防止后续无关贡献增删导致断言位置漂移。
    let injected = segments
        .iter()
        .find(|segment| {
            segment.baseline_provenance.as_ref().is_some_and(|provenance| {
                provenance.kind == PromptBaselineKind::PromptSkillInjection
                    && provenance.source_id == "luaskills/vulcan-codekit"
            })
        })
        .expect("visible CodeKit must inject its package main help");
    assert!(injected.body.contains(&package.main_content));

    segments.clear();
    super::append_luaskill_package_segments(&mut segments, &[package], &[]);
    assert!(segments.is_empty());
}

/// Plugin prompt contributions retain one producer identity per contribution.
/// 插件提示词贡献为每条贡献保留独立的生产者身份。
#[test]
fn plugin_prompt_contributions_keep_distinct_typed_identity() {
    let mut catalog = VulcanPluginCatalogSnapshot::empty(PathBuf::from("plugins"));
    catalog.prompt_contributions = vec![
        VulcanPluginPromptContribution {
            plugin_id: String::from("plugin.alpha"),
            id: String::from("rules"),
            body: String::from("Alpha rules."),
            required_tools: Vec::new(),
        },
        VulcanPluginPromptContribution {
            plugin_id: String::from("plugin.beta"),
            id: String::from("workflow"),
            body: String::from("Beta workflow."),
            required_tools: Vec::new(),
        },
    ];
    let mut segments = Vec::new();

    append_vulcan_plugin_prompt_contributions(&mut segments, Some(&catalog), &[]);

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].body, "Alpha rules.");
    assert_eq!(segments[1].body, "Beta workflow.");
    assert_eq!(
        segments[0]
            .baseline_provenance
            .as_ref()
            .map(|provenance| (provenance.kind, provenance.source_id.as_str())),
        Some((PromptBaselineKind::PluginInstructions, "plugin.alpha/rules"))
    );
    assert_eq!(
        segments[1]
            .baseline_provenance
            .as_ref()
            .map(|provenance| (provenance.kind, provenance.source_id.as_str())),
        Some((
            PromptBaselineKind::PluginInstructions,
            "plugin.beta/workflow"
        ))
    );
}

/// Verifies request-scoped agent routing makes preparation use the requested agent instead of the global active worker.
/// 验证请求级代理路由会让执行准备使用指定代理，而不是全局激活的默认工作代理。
/// Adds one protocol variant for each protocol id to every model of one catalog.
/// 给一个目录的每个模型按协议 ID 各添加一个协议变体。
///
/// # Parameters
/// 参数
/// `protocol_profile_ids` are the exact protocols the models bind to; every model receives
/// a variant for each id so system-manifest bindings can prove their exact variant before
/// execution (plan §R5.1).
/// `protocol_profile_ids` 是模型绑定的精确协议；每个模型为每个 ID 获得一个变体，
/// 使系统清单绑定能够在执行前证明其精确变体（方案 §R5.1）。
fn with_protocol_variants(
    mut catalog: vulcan_provider_adapters::ConnectionModelCatalog,
    protocol_profile_ids: &[&str],
) -> vulcan_provider_adapters::ConnectionModelCatalog {
    for model in &mut catalog.models {
        for protocol_profile_id in protocol_profile_ids {
            model.protocol_variants.insert(
                (*protocol_profile_id).to_string(),
                vulcan_provider_adapters::ProtocolModelVariant {
                    capabilities: model.capabilities.clone(),
                    capability_evidence: Default::default(),
                    discovery_signature: String::from("test-signature"),
                    catalog_generation: catalog.generation,
                },
            );
        }
    }
    catalog
}

#[test]
fn prepare_execution_uses_request_execution_agent_override() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.execution_agent_id = Some(String::from(PLAN_AGENT_ID));

    let preparation = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("preparation should route to the requested agent")
        .preparation;

    assert_eq!(preparation.agent_id, PLAN_AGENT_ID);
    assert_eq!(preparation.agent_name, "Planning agent");
    assert_eq!(preparation.agent_kind, "Planning agent");
    assert!(preparation.delegate_agents.is_empty());
}

/// Verifies an unknown request-scoped execution agent fails instead of silently falling back to the active agent.
/// 验证未知的请求级执行代理会失败，而不是静默回退到当前激活代理。
#[test]
fn prepare_execution_rejects_missing_request_execution_agent() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.execution_agent_id = Some(String::from("missing-agent"));

    let error = match prepare_execution_with_tool_snapshot(&foundation, None, &config, &request) {
        Ok(_) => panic!("missing requested agent should fail"),
        Err(error) => error,
    };

    assert!(error.contains("missing-agent"));
}

/// Verifies an empty allow-only prompt-skill policy removes every globally available skill from one execution.
/// 验证空的仅允许提示词技能策略会从单次执行中移除全部全局可用技能。
#[test]
fn execution_prompt_skill_policy_blocks_unselected_global_skills() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.prompt_skill_policy_override = CapabilityPolicySnapshot {
        mode: CapabilityPolicyMode::AllowOnly,
        selectors: Vec::new(),
    };

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("empty allow-only prompt-skill policy should prepare");

    assert!(prepared.preparation.prompt_skills.is_empty());
    assert!(
        prepared
            .preparation
            .tool_descriptors
            .iter()
            .all(|descriptor| descriptor.id != "skill")
    );
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

/// Verifies explicit empty allow-only mode blocks regular tools instead of falling back to all tools.
/// 验证显式空仅允许模式会阻止普通工具，而不是回退成开放全部工具。
#[test]
fn execution_tool_exposure_empty_allow_only_hides_regular_tools() {
    let snapshot = ToolExposureSnapshot {
        policy: CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::AllowOnly,
            selectors: Vec::new(),
        },
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

    let filtered = filter_native_tool_ids(
        vec![
            String::from("shell_command"),
            String::from("vulcan_file_read"),
            String::from(GET_GOAL_TOOL_ID),
            String::from(TODOLIST_TOOL_ID),
        ],
        &selectors,
    );

    assert_eq!(filtered, vec![GET_GOAL_TOOL_ID, TODOLIST_TOOL_ID]);
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
    let exposure = test_exposure_with_selectors(selectors);

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
            &exposure,
        ));
    }
}

/// Verifies request-forced hidden tools can remove an explicitly allowed universal todolist.
/// 验证请求级强制隐藏工具能移除已被显式允许的通用 todolist。
#[test]
fn request_hidden_tools_remove_universal_todolist_after_exposure() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.tool_exposure_override.allowed_tools = vec![String::from(TODOLIST_TOOL_ID)];
    request
        .tool_request_hidden_tool_ids
        .push(String::from(TODOLIST_TOOL_ID));

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("preparation should resolve with explicitly allowed request-hidden todolist");

    assert!(
        !prepared
            .preparation
            .tool_descriptors
            .iter()
            .any(|descriptor| descriptor.id == TODOLIST_TOOL_ID),
        "request-hidden todolist must not remain visible to the provider"
    );
    assert!(
        !prepared
            .tool_snapshot
            .resolved_tool_definitions
            .iter()
            .any(|definition| definition.descriptor.id == TODOLIST_TOOL_ID),
        "request-hidden todolist must not remain executable"
    );
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

/// Verifies LuaSkills file tools remain available until the real-model migration gate passes.
/// 验证真实模型迁移门槛通过前 LuaSkills 文件工具仍保持可用。
#[test]
fn execution_tool_exposure_keeps_luaskills_file_tools_before_migration_gate() {
    let selectors = ToolExposureSelectorState::default();
    let entries = vec![
        test_luaskills_entry("vulcan-file", "read"),
        test_luaskills_entry("vulcan-file", "list"),
        test_luaskills_entry("vulcan-file", "create"),
        test_luaskills_entry("vulcan-file", "delete"),
        test_luaskills_entry("vulcan-file", "edit"),
        test_luaskills_entry("vulcan-file-tools", "readme"),
    ];

    let filtered = filter_luaskills_entries(entries, &selectors);

    assert_eq!(filtered.len(), 6);
    assert_eq!(filtered[0].canonical_name, "vulcan-file-read");
    assert_eq!(filtered[5].canonical_name, "vulcan-file-tools-readme");
}

/// Verifies LuaSkills family overrides limit package help trees to the visible entries.
/// 验证 LuaSkills 族覆盖会将包帮助树限定为可见入口。
#[test]
fn execution_tool_exposure_limits_luaskill_help_packages_to_visible_entries() {
    let snapshot = ToolExposureSnapshot {
        luaskills_overrides: json!({
            "enabled": ["vulcan-codekit-rg"],
            "disabled": ["ast-detail"]
        }),
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);
    let visible = filter_luaskills_entries(
        vec![
            test_luaskills_entry("vulcan-codekit", "rg"),
            test_luaskills_entry("vulcan-codekit", "ast-detail"),
            test_luaskills_entry("vulcan-curl", "get"),
        ],
        &selectors,
    );
    let package = |skill_id: &str, local_names: &[&str]| {
        LuaSkillPackageHelp::new(
            skill_id,
            "",
            "",
            Vec::new(),
            local_names
                .iter()
                .map(|local_name| {
                    LuaSkillPromptEntry::new(
                        format!("{skill_id}-{local_name}"),
                        "",
                        LuaSkillRegistration::Direct,
                    )
                })
                .collect(),
        )
    };

    let retained = retain_visible_luaskill_help_packages(
        vec![
            package("vulcan-codekit", &["rg", "ast-detail"]),
            package("vulcan-curl", &["get"]),
        ],
        &visible,
        &[],
    );

    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].skill_id, "vulcan-codekit");
    let names = retained[0]
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["vulcan-codekit-rg"]);
}

/// Verifies legacy global tool allow-lists do not hide configured AI fallback tools.
/// 验证旧版全局工具白名单不会隐藏已配置的 AI 回退工具。
#[test]
fn ai_host_binding_visibility_ignores_legacy_global_tool_allow_list() {
    let snapshot = ToolExposureSnapshot {
        allowed_tools: vec![String::from("shell_command")],
        ..ToolExposureSnapshot::default()
    };
    let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);
    let exposure = test_exposure_with_selectors(selectors);

    assert!(vulcan_tool_descriptor_is_visible(
        "understand_image",
        "understand_image",
        "Read image attachments",
        VulcanToolSourceKind::AiHostBinding,
        &exposure,
    ));
    assert!(!vulcan_tool_descriptor_is_visible(
        "read_file",
        "read_file",
        "Read one file",
        VulcanToolSourceKind::Native,
        &exposure,
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
        luaskills_dispatch_tools: vec![test_luaskills_entry("vulcan-codekit", "rg")],
        luaskill_help_packages: Vec::new(),
        mcp_tools: Vec::new(),
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
    let foundation = test_runtime_foundation(&config);
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

/// Verifies the provider schema hides knowledge tools when the turn has no effective library.
/// 验证回合没有生效知识库时 Provider Schema 会隐藏知识库工具。
#[test]
fn prepare_execution_hides_knowledge_tools_without_effective_access() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let request = test_execution_preparation_request();

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("preparation should resolve");
    let definition_ids = prepared
        .tool_snapshot
        .resolved_tool_definitions
        .iter()
        .map(|definition| definition.descriptor.id.as_str())
        .collect::<Vec<_>>();

    assert!(!definition_ids.contains(&KNOWLEDGE_SEARCH_TOOL_ID));
    assert!(!definition_ids.contains(&KNOWLEDGE_BROWSE_TOOL_ID));
    assert!(!definition_ids.contains(&KNOWLEDGE_READ_TOOL_ID));
}

/// Verifies all three read-only tools become visible for one non-empty immutable access snapshot.
/// 验证非空不可变访问快照会让三个只读知识库工具全部可见。
#[test]
fn prepare_execution_exposes_knowledge_tools_for_effective_access() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.knowledge_access = Some(ExecutionKnowledgeAccessContext {
        project_id: String::from("project-a"),
        library_ids: vec![String::from("550e8400-e29b-41d4-a716-446655440000")],
        retrieval_mode: None,
        knowledge_search_handler: None,
    });

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("knowledge-enabled preparation should resolve");
    let definition_ids = prepared
        .tool_snapshot
        .resolved_tool_definitions
        .iter()
        .map(|definition| definition.descriptor.id.as_str())
        .collect::<Vec<_>>();

    assert!(definition_ids.contains(&KNOWLEDGE_SEARCH_TOOL_ID));
    assert!(definition_ids.contains(&KNOWLEDGE_BROWSE_TOOL_ID));
    assert!(definition_ids.contains(&KNOWLEDGE_READ_TOOL_ID));
}

/// Auxiliary execution excludes business mutators from both visibility and invocation.
/// 辅助执行同时从可见目录和调用定义中排除业务修改工具。
#[test]
fn question_execution_removes_business_tools_from_both_surfaces() {
    // Include task, mode and collaboration controls alongside tools that remain usable.
    // 将任务、模式和协作控制与仍可使用的工具一起构造。
    let definitions = [
        vulcan_subagent_core::SPAWN_AGENT_TOOL,
        vulcan_subagent_core::SEND_MESSAGE_TOOL,
        vulcan_subagent_core::INTERRUPT_AGENT_TOOL,
        vulcan_subagent_core::CLOSE_AGENT_TOOL,
        crate::tooling::collaboration::SEND_SESSION_MESSAGE_TOOL_ID,
        CREATE_GOAL_TOOL_ID,
        UPDATE_GOAL_TOOL_ID,
        crate::tooling::native::SUBMIT_PLAN_TOOL_ID,
        TODOLIST_TOOL_ID,
        crate::tooling::collaboration::REPLY_QUESTION_TOOL_ID,
        GET_GOAL_TOOL_ID,
    ]
    .into_iter()
    .map(|id| test_resolved_tool(id, VulcanToolSourceKind::Native))
    .collect::<Vec<_>>();
    // Both surfaces start with the exact same permitted tool set.
    // 两个入口初始持有完全相同的已准入工具集合。
    let mut resolved = ResolvedToolSet {
        descriptors: definitions
            .iter()
            .map(|tool| tool.descriptor.clone())
            .collect(),
        definitions,
    };
    super::remove_question_execution_business_tools(&mut resolved);
    assert_eq!(resolved.descriptors.len(), 2);
    assert_eq!(resolved.definitions.len(), 2);
    for id in [
        crate::tooling::collaboration::REPLY_QUESTION_TOOL_ID,
        GET_GOAL_TOOL_ID,
    ] {
        assert!(resolved.descriptors.iter().any(|tool| tool.id == id));
        assert!(
            resolved
                .definitions
                .iter()
                .any(|tool| tool.descriptor.id == id)
        );
    }
}

/// Original tool origins constrain both advertised tools and callable definitions after catalog changes.
/// 工具目录变化后，原工具来源同时限制展示工具和可调用定义。
#[test]
fn captured_child_tool_admission_tracks_origin_and_denies_replacements() {
    use vulcan_session_core::v1::child_execution::{ChildToolOrigin, ChildToolSourceKind};
    // The original tool has been renamed; its previous name now belongs to another server.
    // 原工具已经改名；旧名称现在属于另一服务。
    let mut original = test_resolved_tool("read_2", VulcanToolSourceKind::Mcp);
    original.descriptor = VulcanToolDescriptor::new(
        "read_2",
        "read_2",
        "Read",
        None,
        VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "original-server", "read"),
    );
    let mut replacement = test_resolved_tool("read", VulcanToolSourceKind::Mcp);
    replacement.descriptor = VulcanToolDescriptor::new(
        "read",
        "read",
        "Read",
        None,
        VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, "replacement-server", "read"),
    );
    let definitions = vec![
        original,
        replacement,
        test_resolved_tool("new-tool", VulcanToolSourceKind::Native),
    ];
    let mut resolved = ResolvedToolSet {
        descriptors: definitions
            .iter()
            .map(|definition| definition.descriptor.clone())
            .collect(),
        definitions,
    };
    let admitted = BTreeMap::from([(
        String::from("read"),
        ChildToolOrigin {
            source_kind: ChildToolSourceKind::Mcp,
            namespace: String::from("original-server"),
            original_name: String::from("read"),
        },
    )]);
    super::captured_child_tool_admission::filter_tools(&mut resolved, Some(&admitted));
    assert_eq!(
        resolved
            .descriptors
            .iter()
            .map(|tool| tool.id.as_str())
            .collect::<Vec<_>>(),
        vec!["read_2"]
    );
    assert_eq!(
        resolved
            .definitions
            .iter()
            .map(|tool| tool.descriptor.id.as_str())
            .collect::<Vec<_>>(),
        vec!["read_2"]
    );
    super::captured_child_tool_admission::filter_tools(&mut resolved, Some(&BTreeMap::new()));
    assert!(resolved.descriptors.is_empty());
    assert!(resolved.definitions.is_empty());
}

/// The production preparation entry rejects child Q&A without an explicit original admission.
/// 生产准备入口拒绝未提供明确原准入记录的子答疑。
#[test]
fn captured_child_tool_admission_is_required_by_child_preparation() {
    // Identity and purpose come from host request fields, never message text.
    // 身份与用途来自宿主请求字段，不来自消息正文。
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.prompt_purpose = ExecutionPromptPurpose::QuestionExecution;
    request.prompt.clear();
    request.execution_scope = vulcan_session_core::ExecutionScope::TaskSession {
        root_session_id: vulcan_session_core::SessionNodeId::from(Uuid::new_v4()),
        task_session_id: vulcan_session_core::SessionNodeId::from(request.session_id),
    };
    let error = match prepare_execution_with_tool_snapshot(&foundation, None, &config, &request) {
        Ok(_) => panic!("missing child admission unexpectedly accepted"),
        Err(error) => error,
    };
    assert!(error.contains("original tool admission"));
    request.captured_child_tool_admission = Some(BTreeMap::new());
    let prepared =
        prepare_execution_with_tool_snapshot(&foundation, None, &config, &request).unwrap();
    assert!(prepared.preparation.tool_descriptors.is_empty());
    assert!(prepared.tool_snapshot.resolved_tool_definitions.is_empty());
}

/// Auxiliary admission refuses a business prompt and an explicit user-input launch mode.
/// 辅助准入拒绝业务提示词和显式用户输入启动方式。
#[test]
fn question_admission_rejects_business_input() {
    // The existing nonempty prompt must fail even for a main-session auxiliary request.
    // 即使是主会话辅助请求，现有非空提示词也必须失败。
    let mut request = test_execution_preparation_request();
    request.prompt_purpose = ExecutionPromptPurpose::QuestionExecution;
    assert!(super::captured_child_tool_admission::validate_request(&request).is_err());
    request.prompt.clear();
    request.turn_start_mode =
        Some(crate::agent_runtime::execution_models::ExecutionTurnStartMode::UserPrompt);
    assert!(super::captured_child_tool_admission::validate_request(&request).is_err());
    request.turn_start_mode = None;
    assert!(super::captured_child_tool_admission::validate_request(&request).is_ok());
}

/// Verifies context-compaction preparation clears every provider-visible tool snapshot.
/// 验证上下文压缩准备阶段会清空所有 Provider 可见工具快照。
#[test]
fn prepare_execution_hides_tools_for_context_compaction_purpose() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.prompt_purpose = ExecutionPromptPurpose::ContextCompaction;
    request.runtime_policy = ExecutionRuntimePolicy::context_compaction();
    request.execution_scope = ExecutionScope::ContextCompaction {
        node_id: SessionNodeId::from(request.session_id),
    };
    request.execution_mode = String::from("Compaction");
    // A parent may reject one tool; that selective policy cannot weaken internal execution isolation.
    // 父级可以只拒绝一个工具；该选择性策略不能削弱内部执行隔离。
    request.tool_request_rejection_reason = Some(String::from("Parent forbids shell only."));
    request.tool_request_rejection_tool_ids = vec![String::from("shell_command")];
    request.tool_request_rejection_all_tools = false;

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("context compaction preparation should still resolve");

    assert!(prepared.preparation.tool_descriptors.is_empty());
    assert!(prepared.tool_snapshot.resolved_tool_definitions.is_empty());
    assert!(prepared.preparation.tool_request_rejection_all_tools);
    assert_eq!(
        prepared
            .preparation
            .tool_request_rejection_reason
            .as_deref(),
        Some("Tool calls are not allowed during this internal execution.")
    );
}

/// Verifies cache-preserving compaction preparation keeps provider-visible tool snapshots.
/// 验证保缓存压缩准备阶段会保留 Provider 可见工具快照。
#[test]
fn prepare_execution_keeps_tools_for_cache_preserving_context_compaction_purpose() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.prompt_purpose = ExecutionPromptPurpose::ContextCompactionCachePreserving;
    request.runtime_policy = ExecutionRuntimePolicy::context_compaction();
    request.execution_scope = ExecutionScope::ContextCompaction {
        node_id: SessionNodeId::from(request.session_id),
    };
    request.execution_mode = String::from("Compaction");
    // Keeping a visible catalog does not inherit permission to execute its unlisted tools.
    // 保留可见目录不代表继承执行未被列入拒绝清单的工具权限。
    request.tool_request_rejection_reason = Some(String::from("Parent forbids shell only."));
    request.tool_request_rejection_tool_ids = vec![String::from("shell_command")];
    request.tool_request_rejection_all_tools = false;

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
    assert!(prepared.preparation.tool_request_rejection_all_tools);
    assert_eq!(
        prepared
            .preparation
            .tool_request_rejection_reason
            .as_deref(),
        Some("Tool calls are not allowed during this internal execution.")
    );
}

/// Verifies TaskSession runtime policy strips request-carried hook and VMM side-effect inputs.
/// 验证 TaskSession 运行时策略会剥离请求携带的 hook 与 VMM 副作用输入。
#[test]
fn prepare_execution_clears_side_effect_inputs_for_task_session_policy() {
    let config = test_provider_config();
    let foundation = test_runtime_foundation(&config);
    let mut request = test_execution_preparation_request();
    request.runtime_policy = ExecutionRuntimePolicy::task_session();
    request.execution_scope = ExecutionScope::TaskSession {
        root_session_id: SessionNodeId::from(Uuid::new_v4()),
        task_session_id: SessionNodeId::from(request.session_id),
    };
    request.system_prompt_hook_blocks = vec![SystemPromptHookBlock {
        tag: String::from("vmm_profile"),
        description: String::from("Profile details"),
        content: String::from("User memory that must not enter a child agent."),
        source: Some(String::from("test")),
        phase: SystemPromptHookPhase::FirstSession,
        baseline_provenance: None,
    }];
    request.project_vmm = VmmConfigLayer {
        enabled: Some(true),
        grpc_target: Some(String::from("127.0.0.1:19090")),
        precheck_enabled: Some(true),
        memory_writeback_enabled: Some(true),
        ai_tools_enabled: Some(true),
        ..VmmConfigLayer::default()
    };
    request.session_vmm = VmmConfigLayer {
        visible_memory_injection: Some(true),
        implicit_memory_turns: Some(7),
        profile_refresh_turns: Some(1),
        session_compact_recall: Some(true),
        ..VmmConfigLayer::default()
    };

    let prepared = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("TaskSession preparation should resolve");

    assert!(prepared.preparation.system_prompt_hook_blocks.is_empty());
    assert_eq!(prepared.preparation.project_vmm, VmmConfigLayer::default());
    assert_eq!(prepared.preparation.session_vmm, VmmConfigLayer::default());
}

/// Builds one provider configuration that can resolve a provider blueprint without touching the network.
/// 构建一份无需访问网络即可解析 Provider 蓝图的供应商配置。
fn test_provider_config() -> ProviderConfigurationFile {
    ProviderConfigurationFile::default()
}

/// Builds a runtime foundation with one explicit public-AI route and in-memory model catalog.
/// 构建带有一条显式公共 AI 路由与内存模型目录的运行时基础。
pub(in crate::agent_runtime) fn test_runtime_foundation(
    config: &ProviderConfigurationFile,
) -> RuntimeFoundation {
    let mut foundation = RuntimeFoundation::build_from_optional_mcp_access(
        config,
        &AgentProfileCatalog::default(),
        None,
        Some(configured_mcp_runtime_access(config)),
    );
    let runtime = vulcan_provider_adapters::PublicAiRuntime::unavailable()
        .expect("test public AI runtime should initialize");
    let connection_id = String::from("test-openrouter");
    let mut public_configuration = vulcan_provider_adapters::PersistedAiConfiguration {
        active: Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: connection_id.clone(),
        }),
        ..vulcan_provider_adapters::PersistedAiConfiguration::default()
    };
    public_configuration.connection_graph.connections.insert(
        connection_id.clone(),
        vulcan_provider_adapters::CredentialConnection {
            id: connection_id.clone(),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: vulcan_provider_adapters::CredentialMode::Token,
            credential_id: String::from("credential-a"),
            display_name: String::from("Account A"),
            secret_reference: String::from("secret-a"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        },
    );
    public_configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
    runtime
        .replace_configuration(public_configuration.clone())
        .expect("test public AI configuration should install");
    runtime
        .stage_source_catalog_for_testing(with_protocol_variants(
            vulcan_provider_adapters::parse_openrouter_model_catalog(
                connection_id.as_str(),
                1,
                r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Model","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("test public AI model catalog should parse"),
            &[vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID],
        ))
        .expect("test public AI source catalog should stage");
    runtime
        .reapply_model_catalog_overlays(&public_configuration)
        .expect("test public AI generation should publish");
    foundation.install_public_ai_runtime(Arc::new(runtime));
    foundation
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

/// Builds one minimal exposure selection for public selector tests.
/// 为公开选择器测试构建一份最小可见性选择。
fn test_exposure_with_selectors(
    selectors: ToolExposureSelectorState,
) -> ExecutionToolExposureSelection {
    ExecutionToolExposureSelection {
        native_tool_ids: Vec::new(),
        luaskills_tools: Vec::new(),
        luaskills_dispatch_tools: Vec::new(),
        luaskill_help_packages: Vec::new(),
        mcp_tools: Vec::new(),
        selectors,
    }
}

/// Builds one minimal resolved tool definition for public selector tests.
/// 为公开选择器测试构建一条最小已解析工具定义。
fn test_resolved_tool(id: &str, source: VulcanToolSourceKind) -> ResolvedToolDefinition {
    ResolvedToolDefinition {
        descriptor: VulcanToolDescriptor::new(
            id.to_string(),
            id.to_string(),
            String::from("test tool"),
            None,
            VulcanToolOrigin::new(source, "test", id.to_string()),
        ),
        parameters_schema: json!({ "type": "object" }),
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
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
        session_storage_directory: None,
        vce_retrieval_handler: None,
        execution_scope: ExecutionScope::MainSession {
            session_id: SessionNodeId::from(session_id),
        },
        runtime_policy: ExecutionRuntimePolicy::main_session(),
        conversation_hook_turn_id: Some(String::from("test-hook-turn")),
        continuation_action_id: None,
        provider_request_continuation: None,
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        dedicated_session: None,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        provider_route_lease_identity: None,
        provider_tool_selections_override: Default::default(),
        provider_tool_media_understanding_selections_override: Default::default(),
        provider_tool_enabled_capabilities_override: Default::default(),
        provider_extension_target_sets_override: None,
        subagent_prompt_context: None,
        session_title: String::from("测试会话"),
        workspace_path: String::from(r"D:\workspace"),
        project_id: None,
        prompt: String::from("请规划当前任务"),
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: None,
        conversation_state: SessionConversationState::default(),
        context_compaction_policy: None,
        context_compaction_window_node_id: None,
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: String::from("Build"),
        temperature: None,
        reasoning_effort: ReasoningEffortLevel::Off,
        tool_exposure_override: Default::default(),
        captured_child_tool_admission: None,
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose: ExecutionPromptPurpose::UserTurn,
        prompt_skill_policy_override: Default::default(),
        project_prompt_skills: ProjectPromptSkillsConfiguration::default(),
        session_prompt_skill_overrides: BTreeMap::new(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: VmmConfigLayer::default(),
        session_vmm: VmmConfigLayer::default(),
        vmm_runtime_ready: true,
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        native_goal_runtime: None,
        native_browser_runtime: None,
        native_agent_collaboration_runtime: None,
        supplemental_mcp_tools: Vec::new(),
        media_resource_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    }
}
