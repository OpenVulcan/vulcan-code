//! host bridge tests for semantic AI fallback aliasing.
//! 面向语义 AI 回退别名的宿主桥接层测试。

use super::{
    AiToolFallbackAlias, append_ai_tool_fallback_aliases, build_luaskills_dispatch_candidate,
    build_luaskills_help_candidate, build_vulcan_tools_manager,
    configured_understand_image_candidate, inject_host_managed_pwd_if_needed,
    luaskills_entry_to_call_view,
};
use crate::luaskills_runtime::PWD_PARAMETER_NAME;
use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    PersistedAiToolConfiguration, PersistedUnderstandImageConfiguration,
    UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID,
};
use crate::tooling::build_native_tool_catalog;
use luaskills::RuntimeEntryDescriptor;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use vulcan_luaskills_prompt::{LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolSourceKind, VulcanToolsManager,
};
use vulcan_vmm::VmmConfigLayer;

/// Builds one configured image-understanding route used by stable registration tests.
/// 构建供稳定注册测试使用的一条已配置图片理解路由。
fn test_understand_image_configuration() -> PersistedAiToolConfiguration {
    PersistedAiToolConfiguration {
        vendors: vec![PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![PersistedAiCapabilityTargetBinding::new(
                AiToolKind::UnderstandImage,
                "minimax",
                "minimax",
            )],
        }],
        understand_image: PersistedUnderstandImageConfiguration {
            target_order: vec![String::from("minimax/minimax")],
            ..PersistedUnderstandImageConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    }
}

/// Builds one inert host-tool candidate used by fallback alias tests.
/// 构建一条供回退别名测试使用的惰性宿主工具候选项。
fn test_candidate(id: &str, source: VulcanToolSourceKind) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description: String::from("Search the web"),
            help_text: None,
            source,
        },
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        invoke: Arc::new(|_args, _session_id| {
            Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ok"))) })
        }),
    }
}

/// Verifies a configured host fallback is exposed as the semantic AI tool and the backing tool is not duplicated.
/// 验证已配置的宿主回退会以语义 AI 工具暴露，并且不会重复暴露底层承载工具。
#[test]
fn ai_tool_fallback_alias_exposes_semantic_name_without_backing_duplicate() {
    let mut candidates = vec![test_candidate("brave-search", VulcanToolSourceKind::Mcp)];

    append_ai_tool_fallback_aliases(
        &mut candidates,
        vec![AiToolFallbackAlias {
            kind: AiToolKind::WebSearch,
            fallback_tool_id: String::from("mcp_brave_search"),
        }],
    );

    let resolved = VulcanToolsManager::new(candidates).resolve_for_execution_mode("Build");
    assert_eq!(resolved.descriptors.len(), 1);
    assert_eq!(resolved.descriptors[0].id, "web_search");
    assert_eq!(resolved.descriptors[0].description, "Search the web");
    assert_eq!(
        resolved.descriptors[0].source,
        VulcanToolSourceKind::AiFallback
    );
    assert_eq!(
        resolved.definitions[0].parameters_schema["properties"]["query"]["type"],
        "string"
    );
}

/// Verifies configured image understanding remains exposed as a semantic fallback tool even before any image attachment exists.
/// 验证已配置的图片理解即使当前还没有图片附件，也会稳定暴露为语义回退工具。
#[test]
fn understand_image_fallback_alias_is_registered_without_image_attachments() {
    let candidate =
        configured_understand_image_candidate(test_understand_image_configuration(), Vec::new())
            .expect("configured image understanding should produce a backing tool");
    let mut candidates = vec![candidate];

    append_ai_tool_fallback_aliases(
        &mut candidates,
        vec![AiToolFallbackAlias {
            kind: AiToolKind::UnderstandImage,
            fallback_tool_id: String::from(UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID),
        }],
    );

    let resolved = VulcanToolsManager::new(candidates).resolve_for_execution_mode("Build");

    assert_eq!(resolved.descriptors.len(), 1);
    assert_eq!(resolved.descriptors[0].id, "understand_image");
    assert_eq!(
        resolved.descriptors[0].source,
        VulcanToolSourceKind::AiFallback
    );
    assert!(
        resolved.definitions[0].parameters_schema["properties"]["image"]
            .get("enum")
            .is_none()
    );
}

/// Verifies host-managed PWD injection uses the native workspace-path format expected by LuaSkills host conventions.
/// 验证宿主管理 PWD 注入会使用 LuaSkills 宿主公约要求的原生工作区路径格式。
#[test]
fn host_managed_pwd_injection_uses_native_workspace_path_text() {
    let workspace_root = if cfg!(windows) {
        Path::new("C:/workspace/demo")
    } else {
        Path::new("/tmp/workspace/demo")
    };
    let expected_pwd = if cfg!(windows) {
        r"C:\workspace\demo"
    } else {
        "/tmp/workspace/demo"
    };

    let injected = inject_host_managed_pwd_if_needed(
        json!({ "query": "demo" }),
        true,
        Some(workspace_root),
        "vulcan-codekit-run",
    )
    .expect("pwd injection should succeed");

    assert_eq!(injected[PWD_PARAMETER_NAME], expected_pwd);
}

/// Verifies host-managed PWD injection preserves an explicit caller-supplied path instead of overwriting it.
/// 验证宿主管理 PWD 注入会保留调用方显式提供的路径，而不是强行覆盖。
#[test]
fn host_managed_pwd_injection_preserves_explicit_value() {
    let injected = inject_host_managed_pwd_if_needed(
        json!({ "PWD": "/caller/path", "query": "demo" }),
        true,
        Some(Path::new("/tmp/workspace/demo")),
        "vulcan-codekit-run",
    )
    .expect("pwd injection should preserve explicit value");

    assert_eq!(injected[PWD_PARAMETER_NAME], "/caller/path");
}

/// Verifies implicit LuaSkills expose help and call tools as one paired tool group.
/// 验证隐式 LuaSkills 会以帮助与调用工具成组暴露。
#[test]
fn implicit_luaskills_expose_help_and_call_tool_pair() {
    let entry = RuntimeEntryDescriptor {
        canonical_name: String::from("vulcan-codekit-ast-tree"),
        skill_id: String::from("vulcan-codekit"),
        local_name: String::from("ast-tree"),
        root_name: String::from("USER"),
        skill_dir: String::from(r"C:\skills\vulcan-codekit"),
        description: String::from("Inspect source tree structure."),
        parameters: Vec::new(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "paths": { "type": "string" }
            },
            "required": ["paths"]
        }),
    };
    let manager = VulcanToolsManager::new(vec![
        build_luaskills_help_candidate(vec![LuaSkillPromptEntry::new(
            "vulcan-codekit-ast-tree",
            "Inspect source tree structure.",
        )]),
        build_luaskills_dispatch_candidate(None, None, vec![entry]),
    ]);

    let resolved = manager.resolve_for_execution_mode("Build");
    let ids = resolved
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<Vec<_>>();

    assert!(ids.contains(&LUASKILL_HELP_TOOL_NAME));
    assert!(ids.contains(&CALL_LUASKILL_TOOL_NAME));
}

/// Verifies implicit LuaSkills dispatch uses LuaSkills canonical names rather than public tool aliases.
/// 验证隐式 LuaSkills 调度使用 LuaSkills canonical 名称，而不是 public tool 别名。
#[test]
fn implicit_luaskills_dispatch_uses_canonical_name() {
    let input_schema = json!({ "type": "object" });
    let entry = RuntimeEntryDescriptor {
        canonical_name: String::from("vulcan-codekit-ast-tree"),
        skill_id: String::from("vulcan-codekit"),
        local_name: String::from("ast-tree"),
        root_name: String::from("USER"),
        skill_dir: String::from(r"C:\skills\vulcan-codekit"),
        description: String::from("Inspect source tree structure."),
        parameters: Vec::new(),
        input_schema,
    };

    let view = luaskills_entry_to_call_view(&entry);

    assert_eq!(view.canonical_name, "vulcan-codekit-ast-tree");
}

/// Verifies VMM memory tools stay hidden until the effective VMM config is ready and AI tools are enabled.
/// 验证 VMM 记忆工具在最终 VMM 配置就绪且 AI 工具启用前保持隐藏。
#[test]
fn vmm_memory_tools_are_filtered_by_effective_config() {
    let runtime_config = crate::agent_runtime::config::AgentRuntimeConfig {
        active_model: None,
        active_agent_id: String::from("agent-a"),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: Default::default(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
    };
    let global_vmm = VmmConfigLayer {
        grpc_target: Some(String::from("127.0.0.1:19090")),
        user_id: Some(1),
        project_id: Some(2),
        ai_tools_enabled: Some(true),
        ..VmmConfigLayer::default()
    };
    let native_catalog = build_native_tool_catalog(
        &runtime_config,
        None,
        Arc::new(crate::tooling::CommandSessionRuntime::new()),
        global_vmm,
        None,
    );
    let base_ids = vec![
        String::from("vmm_memory_search"),
        String::from("vmm_turn_details"),
        String::from("vmm_memory_write"),
        String::from("vmm_memory_delete"),
    ];

    let enabled_manager = build_vulcan_tools_manager(
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        native_catalog.clone(),
        None,
        Vec::new(),
        None,
        None,
        None,
        Vec::new(),
        base_ids.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer::default(),
    );
    let enabled_ids = enabled_manager
        .resolve_for_execution_mode("Build")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    assert!(enabled_ids.iter().any(|id| id == "vmm_memory_search"));
    assert!(enabled_ids.iter().any(|id| id == "vmm_turn_details"));
    assert!(enabled_ids.iter().any(|id| id == "vmm_memory_write"));
    assert!(enabled_ids.iter().any(|id| id == "vmm_memory_delete"));

    let disabled_manager = build_vulcan_tools_manager(
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        native_catalog,
        None,
        Vec::new(),
        None,
        None,
        None,
        Vec::new(),
        base_ids,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer {
            ai_tools_enabled: Some(false),
            ..VmmConfigLayer::default()
        },
    );
    let disabled_ids = disabled_manager
        .resolve_for_execution_mode("Build")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    assert!(!disabled_ids.iter().any(|id| id.starts_with("vmm_")));
}
