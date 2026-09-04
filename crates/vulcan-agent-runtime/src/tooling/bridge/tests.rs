//! host bridge tests for semantic AI fallback aliasing.
//! 面向语义 AI 回退别名的宿主桥接层测试。

use super::{
    build_luaskills_dispatch_candidate, build_luaskills_help_candidate, build_vulcan_tools_manager,
    inject_host_managed_pwd_if_needed, luaskills_entry_to_call_view,
};
use crate::luaskills_runtime::PWD_PARAMETER_NAME;
use crate::tooling::build_native_tool_catalog;
use luaskills::RuntimeEntryDescriptor;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use vulcan_luaskills_prompt::{LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{
    ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};
use vulcan_vmm::VmmConfigLayer;

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

/// Verifies native public ids win provider collisions and duplicate native selections are ignored.
/// 验证被选中的原生文件工具会优先于同名 Provider 候选，保持稳定公开名称。
#[test]
fn native_file_public_ids_are_reserved_and_deduplicated() {
    let runtime_config = crate::agent_runtime::config::AgentRuntimeConfig {
        active_model: None,
        active_agent_id: String::from("agent-a"),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: Default::default(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
    };
    let native_catalog = build_native_tool_catalog(
        &runtime_config,
        None,
        Arc::new(crate::tooling::CommandSessionRuntime::new()),
        VmmConfigLayer::default(),
        None,
        None,
    );
    let provider_candidate = VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("file"),
            name: String::from("file"),
            description: String::from("Provider file tool."),
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: json!({"type": "object"}),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(|_, _| {
            Box::pin(async {
                Ok(VulcanToolInvocationResult::from_model_output(json!(
                    "provider"
                )))
            })
        }),
    };
    let manager = build_vulcan_tools_manager(
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        native_catalog,
        None,
        Vec::new(),
        vec![provider_candidate],
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        vec![String::from("file"), String::from("file")],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer::default(),
        false,
    );

    let resolved: ResolvedToolSet = manager.resolve_for_execution_mode("Build");
    let file_tools = resolved
        .descriptors
        .iter()
        .filter(|descriptor| descriptor.id == "file")
        .collect::<Vec<_>>();
    assert_eq!(file_tools.len(), 1);
    assert_eq!(file_tools[0].source, VulcanToolSourceKind::Native);
}

/// Verifies the canonical VCE public id cannot be replaced by a provider tool collision.
/// 验正规范 VCE 公开标识不能被同名 Provider 工具碰撞替换。
#[test]
fn native_vce_public_id_is_reserved_from_provider_collisions() {
    let runtime_config = crate::agent_runtime::config::AgentRuntimeConfig {
        active_model: None,
        active_agent_id: String::from("agent-a"),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: Default::default(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
    };
    let native_catalog = build_native_tool_catalog(
        &runtime_config,
        None,
        Arc::new(crate::tooling::CommandSessionRuntime::new()),
        VmmConfigLayer::default(),
        None,
        None,
    );
    let provider_candidate = VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from(vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID),
            name: String::from(vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID),
            description: String::from("Provider collision."),
            help_text: None,
            source: VulcanToolSourceKind::AiHostBinding,
        },
        parameters_schema: json!({"type": "object"}),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(|_, _| {
            Box::pin(async {
                Ok(VulcanToolInvocationResult::from_model_output(json!(
                    "provider"
                )))
            })
        }),
    };
    let manager = build_vulcan_tools_manager(
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        native_catalog,
        None,
        Vec::new(),
        vec![provider_candidate],
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        vec![String::from(
            vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID,
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer::default(),
        false,
    );

    let resolved = manager.resolve_for_execution_mode("Build");
    let vce_tools = resolved
        .descriptors
        .iter()
        .filter(|descriptor| descriptor.id == vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID)
        .collect::<Vec<_>>();
    assert_eq!(vce_tools.len(), 1);
    assert_eq!(vce_tools[0].source, VulcanToolSourceKind::Native);
}

/// Verifies VMM memory tools require ready configuration, enabled AI tools, and one live runtime.
/// 验证 VMM 记忆工具要求配置就绪、AI 工具启用且运行时实时可用。
#[test]
fn vmm_memory_tools_are_filtered_by_effective_config_and_live_runtime() {
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
        enabled: Some(true),
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
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        base_ids.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer::default(),
        true,
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
        native_catalog.clone(),
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        base_ids.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer {
            enabled: Some(false),
            ai_tools_enabled: Some(true),
            ..VmmConfigLayer::default()
        },
        true,
    );
    let disabled_ids = disabled_manager
        .resolve_for_execution_mode("Build")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    assert!(!disabled_ids.iter().any(|id| id.starts_with("vmm_")));

    let stopped_manager = build_vulcan_tools_manager(
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        native_catalog,
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        base_ids,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
        Vec::new(),
        None,
        Vec::new(),
        VmmConfigLayer::default(),
        VmmConfigLayer::default(),
        false,
    );
    let stopped_ids = stopped_manager
        .resolve_for_execution_mode("Build")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    assert!(!stopped_ids.iter().any(|id| id.starts_with("vmm_")));
}
