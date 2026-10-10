//! host bridge tests for semantic AI fallback aliasing.
//! 面向语义 AI 回退别名的宿主桥接层测试。

use super::{
    build_luaskills_dispatch_candidate, build_luaskills_help_candidate, build_vulcan_tools_manager,
    inject_host_managed_pwd_if_needed, luaskills_entry_to_call_view,
};
use crate::agent_runtime::task_runtime::collaboration_lifecycle::{
    ChildTaskActivationRequest, ChildTaskLifecycleReceipt, ChildTaskReservationRequest,
    ChildTaskTerminalRequest,
};
use crate::luaskills_runtime::PWD_PARAMETER_NAME;
use crate::tooling::build_native_tool_catalog;
use crate::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, ASK_QUESTION_TOOL_ID, AgentCollaborationCommand, CANCEL_QUESTION_TOOL_ID,
    LIST_SESSIONS_TOOL_ID, NativeAgentCollaborationRuntime, READ_SESSION_TOOL_ID,
    REPLY_QUESTION_TOOL_ID, SEND_NOTIFICATION_TOOL_ID, SEND_SESSION_MESSAGE_TOOL_ID,
    WAIT_AGENTS_RESULT_TOOL_ID,
};
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig, RuntimeEntryDescriptor,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulcan_luaskills_prompt::{
    LUASKILL_HELP_TOOL_NAME, LuaSkillPackageHelp, LuaSkillPromptEntry, LuaSkillRegistration,
};
use vulcan_plugin_runtime::VulcanPluginRuntimeManager;
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{
    ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolInvocationContext, VulcanToolInvocationResult, VulcanToolOrigin,
    VulcanToolSourceKind, VulcanToolsManager,
};
use vulcan_vmm::VmmConfigLayer;

/// Provides a host collaboration port whose candidates can be projected by bridge tests.
/// 提供可由桥接测试投影工具候选的宿主协作端口。
#[derive(Debug)]
struct FakeCollaborationRuntime;

#[async_trait::async_trait]
impl NativeAgentCollaborationRuntime for FakeCollaborationRuntime {
    /// Rejects unexpected compaction in this bridge fixture; no snapshot is returned.
    /// 在此桥接夹具中拒绝非预期压缩；不返回快照。
    async fn compact_child_task(
        &self,
        _: crate::agent_runtime::prompt_compaction::ChildTaskCompactionRequest,
    ) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
        Err(String::from("Unexpected child compaction in test host."))
    }
    /// Rejects unexpected initial creation in this narrowly scoped test host.
    /// 在此限定范围的测试宿主中拒绝非预期初始创建。
    async fn create_child_task(
        &self,
        _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskReservationRequest,
        _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<
        crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskLifecycleReceipt,
        String,
    > {
        Err(String::from("Unexpected initial child creation."))
    }
    /// Accepts host-only profile capture independently of model tool invocation tests.
    /// 独立于模型工具调用测试接纳仅宿主配置捕获。
    async fn capture_child_execution_profile(
        &self,
        _request: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Returns an empty result for candidate registration tests.
    /// 为候选注册测试返回空结果。
    async fn invoke(
        &self,
        _context: VulcanToolInvocationContext,
        _command: AgentCollaborationCommand,
    ) -> Result<Value, String> {
        Ok(Value::Null)
    }

    /// Returns an empty result for the legacy wait wrapper.
    /// 为旧版等待包装器返回空结果。
    async fn wait_for_incoming_question(
        &self,
        _context: VulcanToolInvocationContext,
    ) -> Result<Value, String> {
        Ok(Value::Null)
    }

    /// Rejects child-task reservation because the bridge fake has no durable owner.
    /// 桥接假端口没有持久化所有者，因此拒绝子任务预留。
    async fn reserve_child_task(
        &self,
        _request: ChildTaskReservationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err(String::from(
            "bridge fake does not implement child-task reservation",
        ))
    }

    /// Rejects child-task activation because the bridge fake has no durable owner.
    /// 桥接假端口没有持久化所有者，因此拒绝子任务激活。
    async fn activate_child_task(
        &self,
        _request: ChildTaskActivationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err(String::from(
            "bridge fake does not implement child-task activation",
        ))
    }

    /// Rejects child-task terminalization because the bridge fake has no durable owner.
    /// 桥接假端口没有持久化所有者，因此拒绝子任务终态提交。
    async fn terminal_child_task(
        &self,
        _request: ChildTaskTerminalRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err(String::from(
            "bridge fake does not implement child-task terminalization",
        ))
    }
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
        build_luaskills_help_candidate(vec![LuaSkillPackageHelp::new(
            "vulcan-codekit",
            "",
            "",
            Vec::new(),
            vec![LuaSkillPromptEntry::new(
                "vulcan-codekit-ast-tree",
                "Inspect source tree structure.",
                LuaSkillRegistration::Implicit,
            )],
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
    for tool_id in [
        "file",
        "file_read",
        "file_write",
        "file_edit",
        "apply_patch",
    ] {
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
            descriptor: VulcanToolDescriptor::new(
                String::from(tool_id),
                String::from(tool_id),
                String::from("Provider file tool."),
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::AiHostBinding,
                    "test",
                    String::from(tool_id),
                ),
            ),
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
            vec![String::from(tool_id), String::from(tool_id)],
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
            None,
            None,
        );

        let resolved: ResolvedToolSet = manager.resolve_for_execution_mode("Build");
        let file_tools = resolved
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.id == tool_id)
            .collect::<Vec<_>>();
        assert_eq!(file_tools.len(), 1);
        assert_eq!(file_tools[0].source, VulcanToolSourceKind::Native);
        // Inspect the same schemas and execution restrictions handed to provider request construction.
        // 检查交给供应商请求构建的同一结构及执行限制。
        let plan = manager.resolve_for_execution_mode("Plan");
        let definition = plan
            .definitions
            .iter()
            .find(|definition| definition.descriptor.id == tool_id)
            .expect("registered file tool");
        match tool_id {
            "file_read" => {
                assert!(matches!(
                    definition.plan_policy,
                    VulcanPlanToolPolicy::Visible
                ));
                assert_eq!(definition.parameters_schema["required"], json!(["path"]));
                assert!(
                    definition.parameters_schema["properties"]
                        .get("action")
                        .is_none()
                );
            }
            "file_write" | "file_edit" | "apply_patch" => assert!(matches!(
                definition.plan_policy,
                VulcanPlanToolPolicy::Hidden
            )),
            "file" => assert_eq!(
                definition.parameters_schema["properties"]["action"]["enum"],
                json!(["list", "stat", "delete", "move"])
            ),
            _ => unreachable!(),
        }
    }
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
        descriptor: VulcanToolDescriptor::new(
            String::from(vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID),
            String::from(vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID),
            String::from("Provider collision."),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::AiHostBinding,
                "test",
                String::from(vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID),
            ),
        ),
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
        Some(crate::agent_runtime::VceRetrievalHandler::new(|_, _| {
            Box::pin(async { Err(String::from("not invoked by catalog test")) })
        })),
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
        None,
        None,
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

/// Verifies the model-facing VCE tool is absent while no ready host handler exists.
/// 验证不存在已就绪宿主处理器时，面向模型的 VCE 工具完全不可见。
#[test]
fn native_vce_tool_is_hidden_without_ready_handler() {
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
    let manager = build_vulcan_tools_manager(
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
        None,
        None,
    );

    assert!(
        manager
            .resolve_for_execution_mode("Build")
            .descriptors
            .iter()
            .all(|descriptor| descriptor.id != vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID)
    );
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
        // Freeze native ownership explicitly; unused external connection fields must not select transport.
        // 明确冻结原生归属；未使用的外链连接字段不得选择传输后端。
        backend_mode: Some(vulcan_vmm::VmmBackendMode::Native),
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
        None,
        None,
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

    // Inspect the actual manager definitions passed to provider projection, not descriptor labels alone.
    // 检查实际传给供应商投影的管理器定义，而不仅是描述标签。
    let build_definitions = enabled_manager
        .resolve_for_execution_mode("Build")
        .definitions
        .into_iter()
        .filter(|definition| base_ids.contains(&definition.descriptor.id))
        .collect::<Vec<_>>();
    assert_eq!(build_definitions.len(), 4);
    for definition in &build_definitions {
        assert_eq!(definition.parameters_schema["type"], "object");
        assert!(definition.parameters_schema["properties"].is_object());
        assert!(!definition.descriptor.description.trim().is_empty());
    }
    // The manager deliberately preserves the Plan schema surface and enforces write restrictions at invocation.
    // 管理器刻意保留 Plan 的 Schema 工具面，并在调用时执行写入限制。
    let plan_definitions = enabled_manager
        .resolve_for_execution_mode("Plan")
        .definitions
        .into_iter()
        .filter(|definition| base_ids.contains(&definition.descriptor.id))
        .collect::<Vec<_>>();
    assert_eq!(plan_definitions.len(), 4);
    for definition in plan_definitions {
        let is_write = matches!(
            definition.descriptor.id.as_str(),
            "vmm_memory_write" | "vmm_memory_delete"
        );
        assert_eq!(
            matches!(
                definition.plan_policy,
                vulcan_tools_manager::VulcanPlanToolPolicy::Hidden
            ),
            is_write
        );
    }

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
        None,
        None,
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
        None,
        None,
    );
    let stopped_ids = stopped_manager
        .resolve_for_execution_mode("Build")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    assert!(!stopped_ids.iter().any(|id| id.starts_with("vmm_")));
}

/// Verifies the host bridge registers every collaboration candidate, including one-way notifications.
/// 验证宿主桥接完整注册所有协作候选工具，包括单向通知工具。
#[test]
fn native_collaboration_candidates_are_registered_by_bridge() {
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
    let manager = build_vulcan_tools_manager(
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
        Vec::new(),
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
        None,
        Some(Arc::new(FakeCollaborationRuntime)),
    );

    let build_ids = manager
        .resolve_for_execution_mode("Build")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    for tool_id in [
        ASK_QUESTION_TOOL_ID,
        ASK_PARENT_TOOL_ID,
        REPLY_QUESTION_TOOL_ID,
        WAIT_AGENTS_RESULT_TOOL_ID,
        CANCEL_QUESTION_TOOL_ID,
        LIST_SESSIONS_TOOL_ID,
        READ_SESSION_TOOL_ID,
        SEND_SESSION_MESSAGE_TOOL_ID,
        SEND_NOTIFICATION_TOOL_ID,
    ] {
        assert!(
            build_ids.iter().any(|candidate| candidate == tool_id),
            "bridge must register collaboration tool {tool_id}"
        );
    }
    assert!(
        build_ids
            .iter()
            .all(|id| !matches!(id.as_str(), "wait_agent" | "wait_question" | "wait_session")),
        "only the unified wait may reach the model-facing bridge"
    );

    let notification_definition = manager
        .resolve_for_execution_mode("Build")
        .definitions
        .into_iter()
        .find(|definition| definition.descriptor.id == SEND_NOTIFICATION_TOOL_ID)
        .expect("send_notification definition must be projected by bridge");
    assert_eq!(notification_definition.parameters_schema["type"], "object");
    assert_eq!(
        notification_definition.parameters_schema["additionalProperties"],
        false
    );
    assert_eq!(
        notification_definition.parameters_schema["required"],
        json!(["target_session_id", "message"])
    );

    let plan_ids = manager
        .resolve_for_execution_mode("Plan")
        .descriptors
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    assert!(
        plan_ids
            .iter()
            .any(|candidate| candidate == SEND_NOTIFICATION_TOOL_ID)
    );
    assert!(
        plan_ids
            .iter()
            .any(|candidate| candidate == ASK_QUESTION_TOOL_ID)
    );
    // Hidden is an invocation-time policy, not a visibility filter: the manager lists every
    // ai_visible candidate for any mode label, so the descriptor stays and only the call is refused.
    // Hidden 是调用期策略而非可见性过滤：管理器对任何模式标签都列出全部 ai_visible 候选，
    // 因此描述符保留，仅拒绝实际调用。
    assert!(
        plan_ids
            .iter()
            .any(|candidate| candidate == SEND_SESSION_MESSAGE_TOOL_ID),
        "task message delivery stays listed in Plan and is refused on invocation"
    );
    let hidden_plan_tool = manager
        .resolve_for_execution_mode("Plan")
        .definitions
        .into_iter()
        .find(|definition| definition.descriptor.id == SEND_SESSION_MESSAGE_TOOL_ID)
        .expect("task message delivery definition must be projected by bridge");
    assert!(
        matches!(hidden_plan_tool.plan_policy, VulcanPlanToolPolicy::Hidden),
        "task message delivery must keep its Hidden Plan policy through the bridge"
    );
}

/// Writes the real idle fixture package; root selects its exact deployed parent; returns no value.
/// 写入真实 idle 夹具包；root 选择精确部署父目录；无返回值。
fn frozen_bridge_write_package(root: &Path) {
    // package owns the real source under the exact built-in fixture root.
    // package 拥有精确内置夹具根目录下的真实来源。
    let package = root.join("idle-plugin");
    fs::create_dir_all(package.join("runtime")).expect("create idle runtime directory");
    fs::create_dir_all(package.join("schemas")).expect("create idle schema directory");
    fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
    fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"-- M owns the real fixture module.
-- M 拥有真实夹具模块。
local M = {}
-- Dispatch returns the real accepted result; _args is the declared invocation payload.
-- Dispatch 返回真实接纳结果；_args 是声明的调用载荷。
function M.dispatch(_args) return '{"ok":true,"result":{"accepted":true}}' end
-- Shutdown returns the real close result; _args is the declared cleanup payload.
-- Shutdown 返回真实关闭结果；_args 是声明的清理载荷。
function M.shutdown(_args) return '{"ok":true,"result":{"closed":true}}' end
return M
"#,
    )
    .expect("write idle runtime module");
    fs::write(
        package.join("schemas/open.json"),
        r#"{"type":"object","properties":{}}"#,
    )
    .expect("write open schema");
    fs::write(
            package.join("schemas/close.json"),
            r#"{"type":"object","properties":{"session_id":{"type":"string"}},"required":["session_id"],"additionalProperties":false}"#,
        )
        .expect("write close schema");
    fs::write(
        package.join("plugin.yaml"),
        r#"schema_version: 1
id: idle-plugin
version: 1.0.0
default_enabled: true
display:
  name: Idle Plugin
  capability: Idle cleanup fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  session_idle_timeout_sec: 1
  tools:
    - id: idle_ping
      group: idle_tools
      description: Run one stateless idle test operation.
      parameters_file: schemas/open.json
      execution:
        kind: stateless
        operation: ping
        timeout_ms: 1000
    - id: idle_open
      group: idle_tools
      description: Open one idle test session.
      parameters_file: schemas/open.json
      execution:
        kind: session_open
        operation: open
        timeout_ms: 1000
    - id: idle_close
      group: idle_tools
      description: Close one idle test session.
      parameters_file: schemas/close.json
      execution:
        kind: session_close
        operation: close
        timeout_ms: 1000
"#,
    )
    .expect("write idle manifest");
}
/// Creates the real Lua engine; runtime_root selects its canonical layout; returns the actual engine.
/// 创建真实 Lua 引擎；runtime_root 选择规范布局；返回真实引擎。
fn frozen_bridge_engine(runtime_root: &Path) -> LuaEngine {
    // native_runtime_root keeps the integration-test Lua package paths free of Windows verbatim prefixes.
    // native_runtime_root 使集成测试的 Lua package 路径不包含 Windows verbatim 前缀。
    let native_runtime_root = vulcan_runtime_assets::native_host_path(
        &runtime_root
            .canonicalize()
            .expect("canonicalize LuaSkills runtime root"),
    );
    // host_options uses the real canonical runtime root.
    // host_options 使用真实规范运行时根目录。
    let host_options = LuaRuntimeHostOptions::with_runtime_root(native_runtime_root);
    LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        host_options,
    ))
    .expect("create LuaSkills test engine")
}
/// Constructs the real runtime owner around explicit isolated roots.
/// 围绕明确隔离根目录构建真实运行时所有者。
/// runtime_root hosts engine and state; vulcan_plugin_root owns the real built-in package.
/// runtime_root 持有引擎与状态；vulcan_plugin_root 拥有真实内置插件包。
/// managed_runtime_distribution_root selects the exact bundled dependency root.
/// managed_runtime_distribution_root 选择精确内置依赖根目录。
/// Returns a manager ready for explicit formal initialization and catalog refresh.
/// 返回可显式初始化正式父级并刷新目录的管理器。
fn frozen_bridge_runtime(
    runtime_root: &Path,
    vulcan_plugin_root: PathBuf,
    managed_runtime_distribution_root: PathBuf,
) -> VulcanPluginRuntimeManager {
    // bridge_root deploys the original production bridge package.
    // bridge_root 部署原生产桥接包。
    let bridge_root = runtime_root
        .join("system_lua_lib")
        .join("vulcan-plugin-bridge");
    fs::create_dir_all(bridge_root.join("runtime")).expect("create bridge runtime directory");
    fs::write(bridge_root.join("dependencies.yaml"), "{}\n").expect("write bridge dependencies");
    fs::write(
        bridge_root.join("runtime/bridge.lua"),
        include_str!("../../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .expect("write bridge module");
    // user_plugin_root isolates user packages from this built-in source.
    // user_plugin_root 将用户包与此内置来源隔离。
    let user_plugin_root = runtime_root.join("user_plugins");
    fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
    fs::create_dir_all(&managed_runtime_distribution_root)
        .expect("create managed runtime distribution root");
    VulcanPluginRuntimeManager::new(
        Arc::new(frozen_bridge_engine(runtime_root)),
        vulcan_plugin_root,
        user_plugin_root,
        bridge_root,
        managed_runtime_distribution_root,
        runtime_root.join("plugin_state"),
        runtime_root.join("plugin_runtime"),
    )
}

/// Projects the real enabled descriptors through the production tools-manager bridge.
/// 将真实已启用描述符通过生产工具管理器桥投影。
/// runtime selects the real owner, workspace_root selects its workspace; returns frozen tools.
/// runtime 选择真实所有者，workspace_root 选择工作区；返回冻结工具。
fn frozen_bridge_tools(
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    workspace_root: &Path,
) -> VulcanToolsManager {
    // runtime_config disables unrelated source selections in this exact fixture.
    // runtime_config 在此精确夹具中关闭无关来源选择。
    let runtime_config = crate::agent_runtime::config::AgentRuntimeConfig {
        active_model: None,
        active_agent_id: String::from("frozen-agent"),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: Default::default(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
    };
    // native_catalog uses the constructor already exercised by the public-id tests.
    // native_catalog 使用公开标识测试已使用的构造器。
    let native_catalog = build_native_tool_catalog(
        &runtime_config,
        None,
        Arc::new(crate::tooling::CommandSessionRuntime::new()),
        VmmConfigLayer::default(),
        None,
        None,
    );
    build_vulcan_tools_manager(
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        native_catalog,
        Some(runtime.clone()),
        runtime.snapshot().enabled_tools.values().cloned().collect(),
        Vec::new(),
        None,
        Some(workspace_root.to_path_buf()),
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
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
        None,
        None,
    )
}

/// Constructs typed invocation identities without substituting the execution result.
/// 构建类型化调用身份，不替换执行结果。
/// No parameters; returns a valid owner context for the actual tools-manager invocation.
/// 无参数；返回真实工具管理器调用的合法所有者上下文。
fn frozen_bridge_context() -> VulcanToolInvocationContext {
    use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId};
    VulcanToolInvocationContext {
        session_id: String::from("frozen-owner"),
        request_stream_id: RequestStreamId::try_new("frozen-stream").expect("valid stream id"),
        api_call_id: ApiCallId::try_new(1).expect("valid API id"),
        tool_call_id: ToolCallId::try_new(2).expect("valid tool id"),
        tool_seq: Seq::try_new(3).expect("valid sequence"),
        controller_tool_call_id: String::from("frozen-call"),
        provider_tool_call_id: None,
    }
}

/// Rejects the old frozen closure after a real refresh while preserving fresh and ordinary calls.
/// 真实刷新后拒绝旧冻结闭包，并保留新冻结及普通调用。
/// No parameters; the real parent closes before the critical stale-call assertion.
/// 无参数；真实父级在关键陈旧调用断言之前关闭。
#[test]
fn frozen_plugin_bridge_rejects_changed_generation() {
    // temp retains all real fixture roots until the parent has closed.
    // temp 保留全部真实夹具根目录直到父级关闭。
    let temp = tempfile::tempdir().expect("create isolated root");
    // runtime_root owns engine layout and persistent state.
    // runtime_root 拥有引擎布局与持久状态。
    let runtime_root = temp.path().join("runtime");
    // plugin_root owns the real built-in package.
    // plugin_root 拥有真实内置插件包。
    let plugin_root = runtime_root.join("plugins");
    std::fs::create_dir_all(&plugin_root).expect("create plugin root");
    frozen_bridge_write_package(&plugin_root);
    // manager is the actual production owner, not a descriptor-only fake.
    // manager 是真实生产所有者，而非仅有描述符的模拟对象。
    let manager = frozen_bridge_runtime(
        &runtime_root,
        plugin_root.clone(),
        runtime_root.join("managed_runtime"),
    );
    manager
        .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
        .expect("initialize formal parent");
    manager
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("load first real catalog");
    // old_catalog retains the actual first-generation snapshot during refresh.
    // old_catalog 在刷新期间保留真实第一代次快照。
    let old_catalog = manager.snapshot();
    // old_generation is the descriptor generation before the content changes.
    // old_generation 是内容变更之前的描述符代次。
    let old_generation = old_catalog.enabled_tools["idle_ping"].plugin_generation;
    // old_tools captures the first descriptor through the production bridge.
    // old_tools 通过生产桥捕获第一个描述符。
    let old_tools = frozen_bridge_tools(&manager, temp.path());
    // old_resolved retains the old Provider definition, including its original schema.
    // old_resolved 保留旧 Provider 定义及其原始 Schema。
    let old_resolved = old_tools.resolve_for_execution_mode("Build");
    // old_definition is selected by exact ID rather than an evolving list position.
    // old_definition 按精确标识选择，不依赖会演进的列表位置。
    let old_definition = old_resolved
        .definitions
        .iter()
        .find(|tool| tool.descriptor.id == "idle_ping")
        .expect("find frozen plugin tool");
    // executor drives the real asynchronous native operation.
    // executor 驱动真实异步原生操作。
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create executor");
    // unchanged proves that the captured closure works before its generation changes.
    // unchanged 证明捕获闭包在代次变化之前可用。
    let unchanged = executor.block_on(old_tools.invoke_tool(
        old_definition,
        json!({}),
        frozen_bridge_context(),
        "Build",
    ));
    std::fs::write(
        plugin_root.join("idle-plugin/dependencies.yaml"),
        "# generation change\n",
    )
    .expect("change the real package fingerprint");
    manager
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("refresh changed package");
    // new_generation is read from the real refreshed descriptor.
    // new_generation 从真实刷新后的描述符读取。
    let new_generation = manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
    // stale observes the old closure against the changed actual generation.
    // stale 观察旧闭包面对真实已变化代次的结果。
    let stale = executor.block_on(old_tools.invoke_tool(
        old_definition,
        json!({}),
        frozen_bridge_context(),
        "Build",
    ));
    // fresh_tools captures the changed descriptor through the same bridge.
    // fresh_tools 通过相同桥捕获变化后的描述符。
    let fresh_tools = frozen_bridge_tools(&manager, temp.path());
    // fresh_resolved contains the refreshed actual Provider definition.
    // fresh_resolved 包含刷新后的真实 Provider 定义。
    let fresh_resolved = fresh_tools.resolve_for_execution_mode("Build");
    // fresh_definition selects the same stable ID from the new captured catalog.
    // fresh_definition 从新捕获目录选择相同稳定标识。
    let fresh_definition = fresh_resolved
        .definitions
        .iter()
        .find(|tool| tool.descriptor.id == "idle_ping")
        .expect("find refreshed plugin tool");
    // fresh executes the actual current generation through the frozen bridge.
    // fresh 通过冻结桥执行真实当前代次。
    let fresh = executor.block_on(fresh_tools.invoke_tool(
        fresh_definition,
        json!({}),
        frozen_bridge_context(),
        "Build",
    ));
    // ordinary preserves the existing current-ID late-binding API.
    // ordinary 保留既有按当前标识晚绑定的 API。
    let ordinary = executor.block_on(manager.invoke(
        vulcan_plugin_runtime::VulcanPluginInvocationRequest {
            tool_id: String::from("idle_ping"),
            arguments: json!({}),
            owner_session_id: String::from("frozen-owner"),
            workspace_root: temp.path().to_path_buf(),
        },
    ));
    manager
        .close_formal_runtime_blocking()
        .expect("close original native parent");
    assert_ne!(old_generation, new_generation);
    assert_eq!(
        unchanged.expect("same generation works").model_output["accepted"],
        true
    );
    assert_eq!(
        stale.expect_err("a frozen old descriptor must not execute the new generation"),
        format!(
            "Vulcan Plugin tool `idle_ping` was frozen for plugin `idle-plugin` generation {old_generation}, but current plugin is `idle-plugin` generation {new_generation}."
        ),
    );
    assert_eq!(
        fresh.expect("fresh generation works").model_output["accepted"],
        true
    );
    assert_eq!(
        ordinary.expect("ordinary invocation stays late-bound")["accepted"],
        true
    );
}
