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
use luaskills::RuntimeEntryDescriptor;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use vulcan_luaskills_prompt::{LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry};
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
        descriptor: VulcanToolDescriptor::new(
            String::from("file"),
            String::from("file"),
            String::from("Provider file tool."),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::AiHostBinding,
                "test",
                String::from("file"),
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
        None,
        None,
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
