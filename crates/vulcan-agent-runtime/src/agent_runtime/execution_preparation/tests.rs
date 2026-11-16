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
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, KNOWLEDGE_LIST_TOOL_ID, KNOWLEDGE_READ_TOOL_ID,
    KNOWLEDGE_SEARCH_TOOL_ID, KNOWLEDGE_WRITE_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
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

/// Verifies effective model policy controls both advertised definitions and registered handlers.
/// 验证生效模型策略同时控制对外定义和已注册处理器。
/// No external requests are issued; all choices come from the fixture catalog.
/// 不发送外部请求；所有选择均来自夹具目录。
#[test]
fn subagent_policy_controls_registration_and_model_availability() {
    // Exercise inclusion, explicit grants, unavailable grants and complete session replacement.
    // 覆盖包含、显式授权、不可用授权及完整会话替换。
    let mut config = test_provider_config();
    // One foundation supplies the same valid calling route across policy changes.
    // 同一基础架构在策略变化时提供相同有效调用路由。
    let foundation = test_runtime_foundation(&config);
    // Request overrides deliberately replace the whole policy, including an empty list.
    // 请求覆盖会有意替换完整策略，包括空清单。
    let mut request = test_execution_preparation_request();
    for (exclude, grant, session, expected) in [
        (false, None, None, true),
        (true, None, None, false),
        (true, Some("unavailable/model"), None, false),
        (true, Some("openai/gpt-5.6-sol"), None, true),
        (false, Some("openai/gpt-5.6-sol"), Some(false), false),
        (true, None, Some(true), true),
    ] {
        config.subagent_exclude_session_model = exclude;
        config.subagent_models = grant
            .map(|model_id| crate::provider_config::SubagentModelGrant {
                provider_id: String::from("openrouter"),
                model_id: model_id.to_owned(),
                note: String::new(),
            })
            .into_iter()
            .collect();
        request.subagent_model_policy =
            session.map(
                |include_session_model| crate::provider_config::SubagentModelPolicy {
                    include_session_model,
                    models: Vec::new(),
                },
            );
        // Inspect the production tool assembly, not a standalone boolean helper.
        // 检查生产工具组装，而非孤立布尔辅助函数。
        let prepared =
            prepare_execution_with_tool_snapshot(&foundation, None, &config, &request).unwrap();
        // Handler resolution must agree with the actual request definitions.
        // 处理器解析必须与真实请求定义一致。
        let registered = prepared
            .tool_snapshot
            .tools_manager
            .resolve_for_execution_mode("Build");
        for tool in vulcan_subagent_core::CORE_SUBAGENT_TOOLS {
            if tool == vulcan_subagent_core::WAIT_AGENTS_RESULT_TOOL {
                continue;
            }
            assert_eq!(
                registered
                    .descriptors
                    .iter()
                    .any(|descriptor| descriptor.id == tool),
                expected,
                "handler {tool}"
            );
            assert_eq!(
                prepared
                    .tool_snapshot
                    .resolved_tool_definitions
                    .iter()
                    .any(|definition| definition.descriptor.id == tool),
                expected,
                "definition {tool}"
            );
        }
        assert!(
            registered
                .descriptors
                .iter()
                .any(|descriptor| descriptor.id == "ask")
        );
    }
}

/// Verifies visible CodeKit entries inject the package main help with its stable provenance; returns no value.
/// 验证可见的 CodeKit 入口按稳定来源标识注入包主帮助；无参数与返回值。
#[test]
fn visible_codekit_injects_package_main_help() {
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
            segment
                .baseline_provenance
                .as_ref()
                .is_some_and(|provenance| {
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
        native_tool_warnings: Vec::new(),
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
    assert!(!definition_ids.contains(&KNOWLEDGE_LIST_TOOL_ID));
    assert!(!definition_ids.contains(&KNOWLEDGE_READ_TOOL_ID));
    assert!(!definition_ids.contains(&KNOWLEDGE_WRITE_TOOL_ID));
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
        search_enabled: true,
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
    assert!(definition_ids.contains(&KNOWLEDGE_LIST_TOOL_ID));
    assert!(definition_ids.contains(&KNOWLEDGE_READ_TOOL_ID));
    assert!(definition_ids.contains(&KNOWLEDGE_WRITE_TOOL_ID));

    // An all-browse snapshot retains listing/reading/writing but has no search capability.
    // 全浏览快照保留列举、读取及写入，但没有检索能力。
    request.knowledge_access.as_mut().unwrap().search_enabled = false;
    let browse = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("browse preparation");
    let browse_ids = browse
        .tool_snapshot
        .resolved_tool_definitions
        .iter()
        .map(|definition| definition.descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert!(!browse_ids.contains(&KNOWLEDGE_SEARCH_TOOL_ID));
    assert!(browse_ids.contains(&KNOWLEDGE_LIST_TOOL_ID));
    assert!(browse_ids.contains(&KNOWLEDGE_READ_TOOL_ID));
    assert!(browse_ids.contains(&KNOWLEDGE_WRITE_TOOL_ID));

    // Plan requests retain all read tools and remove the persistent write from both surfaces.
    // 计划请求保留全部只读工具，并从两个表面移除持久写入。
    request.execution_mode = "Plan".into();
    request.knowledge_access.as_mut().unwrap().search_enabled = true;
    let plan = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request)
        .expect("plan knowledge preparation");
    assert!(
        !plan
            .tool_snapshot
            .resolved_tool_definitions
            .iter()
            .any(|definition| definition.descriptor.id == KNOWLEDGE_WRITE_TOOL_ID)
    );
    assert!(
        !plan
            .preparation
            .tool_descriptors
            .iter()
            .any(|descriptor| descriptor.id == KNOWLEDGE_WRITE_TOOL_ID)
    );
    for id in [
        KNOWLEDGE_READ_TOOL_ID,
        KNOWLEDGE_LIST_TOOL_ID,
        KNOWLEDGE_SEARCH_TOOL_ID,
    ] {
        assert!(
            plan.tool_snapshot
                .resolved_tool_definitions
                .iter()
                .any(|definition| definition.descriptor.id == id)
        );
    }
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

/// Verifies discovery and dispatch through the real Context7 package and Provider tool projection.
/// 通过真实 Context7 包及 Provider 工具投影验证发现与调度。
/// The test does not grant network access; a valid action reaches the plugin runtime boundary.
/// 测试不访问网络；有效动作到达插件运行时边界。
#[tokio::test]
async fn context7_is_discovered_and_dispatched_as_a_prompt_skill() {
    use crate::agent_runtime::provider_transport_tool_context::prepare_host_tool_runtime_context;
    use crate::agent_runtime::question_runtime::{ApprovalAnswer, ToolApprovalPrompt};
    use crate::agent_runtime::tool_permission::ToolPermissionHost;
    use vulcan_core::tool::VulcanToolExecutor;
    use vulcan_core::types::ToolInvocationRequest;
    use vulcan_session_core::v1::config::{AccessConfig, PermissionMode};
    use vulcan_session_core::v1::document::PreparedToolReceipt;
    use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId, TurnId};
    use vulcan_tools_manager::VulcanToolInvocationContext;

    /// Permits reads under the actual permission gate and rejects unexpected approvals.
    /// 通过真实权限闸门允许读取，并拒绝意外审批。
    struct ReadOnlyHost;

    #[async_trait::async_trait]
    impl ToolPermissionHost for ReadOnlyHost {
        /// Returns read-only mode for the fixture session.
        /// 返回夹具会话的只读模式。
        fn access(&self, _: Uuid) -> Result<AccessConfig, String> {
            Ok(AccessConfig {
                permission: PermissionMode::ReadOnly,
                unattended: None,
            })
        }

        /// Rejects any approval request because Context7 checks should classify it as a read.
        /// 拒绝任何审批请求，因为 Context7 检查应将其归为读取。
        async fn request_approval(
            &self,
            _: Uuid,
            _: VulcanToolInvocationContext,
            tool: String,
            _: serde_json::Value,
            _: ToolApprovalPrompt,
        ) -> Result<ApprovalAnswer, String> {
            Err(format!("Unexpected approval for {tool}."))
        }
    }

    // Load the shipped manifests and skill without starting any plugin side effects.
    // 加载发行包清单与技能，无需启动插件副作用。
    let plugin_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    // The published catalog supplies the same tool definitions and SKILL.md as production.
    // 发布目录提供与生产相同的工具定义与 SKILL.md。
    let catalog = vulcan_plugin_runtime::load_vulcan_plugin_catalog(
        &plugin_root,
        None,
        &Default::default(),
        &Default::default(),
        1,
    );
    // Expected actions identify the Context7 operations whose schemas must stay off the wire.
    // 期望动作指明 Schema 不应进入请求的 Context7 操作。
    let actions = ["context7_resolve_library_id", "context7_query_docs"];
    assert!(
        actions
            .iter()
            .all(|action| catalog.enabled_tools.contains_key(*action)),
        "{:?}",
        catalog.plugins["context7"].diagnostics
    );
    // The named skill must be available from the plugin's published snapshot.
    // 命名技能必须存在于插件发布快照中。
    let skill = "context7:library-docs";
    assert!(catalog.enabled_skills.contains_key(skill));
    // Reuse the real request fixture and its offline model route.
    // 复用真实请求夹具及其离线模型路由。
    let config = test_provider_config();
    // The same catalog is used for a regular coding request and a documentation request.
    // 普通编码请求与文档查询请求使用同一目录。
    let foundation = test_runtime_foundation_with_plugins(&config, Some(Arc::new(catalog)));
    // A distinct question ensures the test covers both task types.
    // 不同问题确保测试覆盖两类任务。
    let mut request = test_execution_preparation_request();
    request.prompt = String::from("Rename a local variable in the current project.");
    // The normal request goes through production preparation independently.
    // 普通请求独立通过生产准备流程。
    let ordinary =
        prepare_execution_with_tool_snapshot(&foundation, None, &config, &request).unwrap();
    request.prompt = String::from("Look up the current React useEffect documentation.");
    // Plan mode exercised here was already permitted for both original Context7 tools.
    // 此处验证计划模式，两个原 Context7 工具原本均允许在该模式中使用。
    request.execution_mode = String::from("Plan");
    // Model discovery comes from the normal generated skills inventory.
    // 模型通过正常生成的技能列表发现能力。
    let docs = prepare_execution_with_tool_snapshot(&foundation, None, &config, &request).unwrap();
    assert!(
        docs.preparation
            .additional_developer_segments
            .iter()
            .any(|segment| segment.body.contains(skill))
    );
    // Each preparation is projected with the same function used by the Provider transport.
    // 每份准备结果都使用 Provider 传输路径的同一函数进行投影。
    let runtime = [ordinary, docs].map(|prepared| {
        prepare_host_tool_runtime_context(
            prepared.tool_snapshot.tools_manager,
            &prepared.tool_snapshot.resolved_tool_definitions,
            request.session_id,
            &prepared.preparation.execution_mode,
            foundation.task_runtime(),
        )
    });
    for round in &runtime {
        assert!(
            round
                .provider_tool_definitions
                .iter()
                .any(|tool| tool.name == "skill")
        );
        assert!(
            round
                .provider_tool_definitions
                .iter()
                .any(|tool| tool.name == "plugin_call")
        );
        assert!(
            round
                .provider_tool_definitions
                .iter()
                .all(|tool| !actions.contains(&tool.name.as_str()))
        );
    }
    // Execute the requests a model emits using the skill description and returned operation definitions.
    // 执行模型根据技能简介与返回动作定义发出的请求。
    let bridge = runtime[1]
        .runtime_bridge
        .as_ref()
        .clone()
        .with_tool_permission_host(Some(Arc::new(ReadOnlyHost)));
    // Keep the same bridge across the preview and subsequent tool calls.
    // 预览及后续工具调用继续使用同一桥接层。
    let bridge = &bridge;
    // The durable request receipt is installed before dispatch as it is in the streaming path.
    // 与流式路径一致，在调度前安装可靠请求回执。
    let invoke = |number, name: &str, arguments| {
        // Each invocation receives a distinct durable identity and preserves its wire tool name.
        // 每次调用获得独立的可靠身份，并保留线协议工具名。
        let call = ToolInvocationRequest {
            request_stream_id: format!("context7-skill-stream-{number}"),
            tool_call_id: format!("context7-skill-call-{number}"),
            tool_name: name.to_owned(),
            arguments,
            argument_parse_error: None,
            raw_arguments: Default::default(),
            user_request: Default::default(),
        };
        bridge
            .install_prepared_invocation_identities(
                std::slice::from_ref(&call),
                vec![PreparedToolReceipt {
                    turn_id: TurnId::try_new(1).unwrap(),
                    api_call_id: ApiCallId::try_new(number).unwrap(),
                    tool_call_id: ToolCallId::try_new(number).unwrap(),
                    tool_seq: Seq::try_new(number).unwrap(),
                    request_stream_id: RequestStreamId::try_new(call.request_stream_id.clone())
                        .unwrap(),
                }],
            )
            .unwrap();
        async move { bridge.execute_tool(&call).await.unwrap() }
    };
    // Use a valid object whose fields belong to the original Context7 query schema.
    // 使用字段符合原 Context7 查询 Schema 的对象。
    let valid = json!({"skill": skill, "action": actions[1], "arguments": {"libraryId": "/react/react", "query": "How does useEffect work?"}});
    assert!(invoke(1, "plugin_call", valid.clone()).await.is_error);
    // The first accepted request needs only the skill's discovered canonical name.
    // 首次获准的请求只需要发现的规范技能名。
    let preview = invoke(2, "skill", json!({"name": skill})).await;
    assert!(!preview.is_error);
    assert!(
        actions
            .iter()
            .all(|action| preview.model_output.contains(action))
    );
    assert!(preview.model_output.contains("input_schema"));
    // Read the schema returned to the model and select the doc query by its declared input.
    // 读取返回给模型的 Schema，按声明输入选择文档查询动作。
    let (_, schema_section) = preview.model_output.rsplit_once("```json\n").unwrap();
    // Decode the exact action declarations following the skill instructions.
    // 解码技能说明后附带的精确动作声明。
    let declared: Vec<serde_json::Value> =
        serde_json::from_str(schema_section.split_once("\n```").unwrap().0).unwrap();
    // The action accepting a library ID is the query route selected by the caller.
    // 接收库 ID 的动作为调用方选择的查询入口。
    let discovered = declared
        .iter()
        .find(|entry| entry["input_schema"]["properties"]["libraryId"].is_object())
        .unwrap();
    // The raw Context7 IDs stay unregistered even after discovery.
    // 读取技能后，原始 Context7 ID 依旧未注册。
    assert!(
        invoke(
            3,
            actions[1],
            json!({"libraryId": "/react/react", "query": "useEffect"})
        )
        .await
        .is_error
    );
    // The original action schema rejects wrong parameters before runtime lookup.
    // 原动作 Schema 在查询运行时前拒绝错误参数。
    let invalid = invoke(
        4,
        "plugin_call",
        json!({"skill": skill, "action": actions[1], "arguments": {"query": "useEffect"}}),
    )
    .await;
    assert!(invalid.is_error);
    assert!(invalid.model_output.contains("parameters schema"));
    // The valid call reaches the original plugin boundary and retains its Provider call identity.
    // 有效调用到达原插件边界并保留 Provider 调用身份。
    let result = invoke(
        5,
        "plugin_call",
        json!({
            "skill": skill, "action": discovered["action"], "arguments": valid["arguments"],
        }),
    )
    .await;
    assert!(
        result
            .model_output
            .contains("Vulcan Plugin runtime is unavailable")
    );
    assert_eq!(result.tool_name, "plugin_call");
    // A known action cannot be selected through a different skill or another native tool name.
    // 已知动作无法通过其他技能或另一原生工具名调用。
    assert!(
        invoke(
            6,
            "plugin_call",
            json!({"skill": skill, "action": "shell_command", "arguments": {}})
        )
        .await
        .is_error
    );
    // Hiding the source tools also removes their dispatcher and discovery entry.
    // 隐藏来源工具也会移除其调度入口与发现条目。
    request.tool_request_hidden_tool_ids = actions.into_iter().map(String::from).collect();
    // Reprepare from the same catalog under the scoped tool restriction.
    // 在限定工具限制下使用同一目录重新准备。
    let hidden =
        prepare_execution_with_tool_snapshot(&foundation, None, &config, &request).unwrap();
    assert!(
        !hidden
            .preparation
            .tool_descriptors
            .iter()
            .any(|tool| tool.id == "plugin_call")
    );
    assert!(
        !hidden
            .preparation
            .additional_developer_segments
            .iter()
            .any(|segment| segment.body.contains(skill))
    );
}

/// Builds a runtime foundation with one explicit public-AI route and in-memory model catalog.
/// 构建带有一条显式公共 AI 路由与内存模型目录的运行时基础。
pub(in crate::agent_runtime) fn test_runtime_foundation(
    config: &ProviderConfigurationFile,
) -> RuntimeFoundation {
    test_runtime_foundation_with_plugins(config, None)
}

/// Builds the offline model fixture with an optional published plugin catalog.
/// 使用可选的已发布插件目录构建离线模型夹具。
/// Returns a foundation with the supplied configuration and no plugin executor.
/// 返回使用给定配置且无插件执行器的运行时基础。
fn test_runtime_foundation_with_plugins(
    config: &ProviderConfigurationFile,
    plugins: Option<Arc<VulcanPluginCatalogSnapshot>>,
) -> RuntimeFoundation {
    // Build the transport snapshot with the same producer used by normal agent requests.
    // 使用正常代理请求采用的同一生产者构建传输快照。
    let generation = Arc::new(crate::agent_runtime::ExtensionGeneration::new(
        1,
        plugins,
        configured_mcp_runtime_access(config),
        None,
    ));
    // Keep the model fixture independent of any running host or network client.
    // 使模型夹具独立于正在运行的宿主或网络客户端。
    let mut foundation =
        RuntimeFoundation::build_from_extension_generation_with_runtimes_and_plugins(
            config,
            &AgentProfileCatalog::default(),
            None,
            None,
            generation,
            Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
            Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
            Arc::new(crate::tooling::CommandSessionRuntime::new()),
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
        native_tool_warnings: Vec::new(),
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
pub(in crate::agent_runtime) fn test_execution_preparation_request() -> ExecutionPreparationRequest
{
    let session_id = Uuid::nil();
    ExecutionPreparationRequest {
        session_id,
        session_storage_directory: None,
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
        subagent_model_policy: None,
        provider_route_lease_identity: None,
        provider_tool_selections_override: Default::default(),
        provider_tool_media_understanding_selections_override: Default::default(),
        provider_tool_enabled_capabilities_override: Default::default(),
        provider_extension_target_sets_override: None,
        provider_extension_modes_override: None,
        subagent_prompt_context: None,
        session_title: String::from("测试会话"),
        workspace_path: String::from(r"D:\workspace"),
        project_id: None,
        project_shell_guard: None,
        prompt: String::from("请规划当前任务"),
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: None,
        conversation_state: SessionConversationState::default(),
        context_compaction_policy: None,
        context_window_selection: None,
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: String::from("Build"),
        temperature: None,
        session_request_options: Default::default(),
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
/// Verifies local preview reuses real tool admission without acquiring a model route lease.
/// 验证本地预览复用真实工具准入，且不建立模型路由租约。
#[test]
fn request_context_tool_preview_matches_prepared_tools_without_leasing() {
    // Use the same configured route for both preparation paths.
    // 两条准备路径使用同一份已配置路由。
    let config = test_provider_config();
    // The fixture runtime owns an in-memory provider catalog and no network dispatch.
    // 夹具运行时拥有内存供应商目录且无网络分派。
    let foundation = test_runtime_foundation(&config);
    // Plan mode proves that preview tools are filtered by request mode.
    // 计划模式用于验证工具预览会按请求模式过滤。
    let mut request = test_execution_preparation_request();
    request.execution_mode = String::from("plan");
    // Prepare against the same public runtime so a stray lease is observable.
    // 使用同一个公共运行时准备，便于观察误建的租约。
    let runtime = foundation.public_ai_runtime().unwrap();
    // The preview must succeed before any real preparation.
    // 预览必须在任何真实准备之前成功。
    let preview = super::preview_request_tools(
        foundation,
        runtime.clone(),
        None,
        &config,
        request.clone(),
        false,
        None,
    )
    .unwrap();
    assert!(
        runtime
            .session_selection_lease(&request.session_id.to_string())
            .unwrap()
            .is_none()
    );
    // Prepare the fixture normally to compare its resolved schemas, not just the names.
    // 常规准备夹具以比较实际解析的 Schema，而不只是名称。
    let prepared = prepare_execution_with_tool_snapshot(
        &test_runtime_foundation(&config),
        None,
        &config,
        &request,
    )
    .unwrap();
    assert!(!preview.is_empty());
    assert_eq!(
        preview.len(),
        prepared.tool_snapshot.resolved_tool_definitions.len()
    );
    for (tool, definition) in preview
        .iter()
        .zip(&prepared.tool_snapshot.resolved_tool_definitions)
    {
        assert_eq!(tool["name"], definition.descriptor.id);
        assert_eq!(tool["parameters"], definition.parameters_schema);
    }
}
