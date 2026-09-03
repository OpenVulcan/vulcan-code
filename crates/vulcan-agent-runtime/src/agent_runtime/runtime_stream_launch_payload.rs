//! Runtime stream launch payload model and bootstrap-to-launch assembly helpers.
//! 面向运行时流启动载荷模型与 bootstrap 到 launch 组装转换的辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionTurnStartMode,
};
use crate::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryContext;
use crate::agent_runtime::runtime_stream_bootstrap::RuntimeStreamBootstrap;
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::agent_runtime::task_runtime::{TaskExecutionRuntimeContext, TaskRuntime};
use crate::model::SessionId;
use std::sync::Arc;
use vulcan_context::RuntimeProviderLoweringRepairEvent;
use vulcan_core::types::{
    ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot,
    ProviderPromptLayers as CoreProviderPromptLayers,
};
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Turn-start mode used by the host-controlled provider loop.
/// 宿主控制 Provider 循环使用的回合启动模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeStreamTurnStartMode {
    /// Start by appending or reusing the user-authored prompt.
    /// 通过追加或复用用户提示词启动。
    UserPrompt,
    /// Start from provider-visible messages already seeded into the snapshot.
    /// 从已经播种到快照中的 Provider 可见消息启动。
    RestoredCheckpoint,
}

/// Launch payload that captures everything the outer runtime stream needs after preparation and bootstrap have both succeeded.
/// 捕获准备阶段与启动阶段都成功后外层运行时流所需全部内容的启动载荷。
pub struct RuntimeStreamLaunch {
    /// Shared runtime context registered for spawn-agent child launches while this provider loop is alive.
    /// 当前 Provider 循环存活期间为 spawn_agent 子代理启动注册的共享运行时上下文。
    pub(crate) task_execution_context: Option<TaskExecutionRuntimeContext>,
    /// Session identifier owned by the current runtime run.
    /// 当前运行时执行所属的会话标识。
    pub(crate) session_id: SessionId,
    /// Trimmed effective user prompt that will be sent to the provider transport layer.
    /// 将发送给 Provider 传输层的裁剪后生效用户提示词。
    pub(crate) effective_prompt: String,
    /// Explicit turn-start mode consumed by the provider transport layer.
    /// Provider 传输层消费的显式回合启动模式。
    pub(crate) turn_start_mode: RuntimeStreamTurnStartMode,
    /// Updated execution preparation after request-snapshot seeding.
    /// 完成请求快照播种之后的更新执行准备对象。
    pub(crate) preparation: ExecutionPreparation,
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub(crate) active_agent: Option<RuntimeAgentDefinition>,
    /// Shared resumable task runtime reused by the current execution turn.
    /// 被当前执行回合复用的共享可恢复任务运行时。
    pub(crate) task_runtime: Arc<TaskRuntime>,
    /// Host-owned unified tool manager used by the transport layer.
    /// 供传输层使用的宿主自有统一工具管理器。
    pub(crate) tools_manager: VulcanToolsManager,
    /// Final resolved tool definitions exposed to the provider for this turn.
    /// 当前回合向 Provider 暴露的最终已解析工具定义集合。
    pub(crate) resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    /// Discovery events projected from enabled MCP runtime servers.
    /// 从已启用 MCP 运行时服务器投影得到的发现事件集合。
    pub(crate) discovery_events: Vec<RuntimeEvent>,
    /// Human-readable MCP discovery rows captured for the final execution report.
    /// 为最终执行报告捕获的人类可读 MCP 发现摘要行集合。
    pub(crate) discovery_rows: Vec<String>,
    /// MCP discovery errors captured for the final execution report.
    /// 为最终执行报告捕获的 MCP 发现错误集合。
    pub(crate) discovery_errors: Vec<String>,
    /// Mutable warning list accumulated before provider streaming begins.
    /// 在 Provider 流式执行开始前累计得到的可变警告列表。
    pub(crate) warnings: Vec<String>,
    /// Effective system preamble sent to the provider transport layer.
    /// 发送给 Provider 传输层的生效系统前导语。
    pub(crate) system_preamble: String,
    /// Canonical typed prompt baseline proposed for the physical request boundary.
    /// 为物理请求边界提出的规范强类型提示词基线。
    pub(crate) locked_baseline: LockedBaselineProjection,
    /// Request-dynamic prompt layers preserved for the physical Provider permit.
    /// 为物理 Provider 许可保留的请求动态提示词层。
    pub(crate) dynamic_prompt_layers: CoreProviderPromptLayers,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub(crate) core_snapshot: CoreConversationSnapshot,
    /// Mutable streaming-loop state seeded from the bootstrapped core controller.
    /// 由已启动核心控制器播种得到的可变流式循环状态。
    pub(crate) stream_state: RuntimeStreamDeltaState,
    /// Provider lowering repair events detected before the provider loop starts.
    /// Provider 循环启动前检测到的 Provider 转换修复事件。
    pub(crate) lowering_repair_events: Vec<RuntimeProviderLoweringRepairEvent>,
    /// Process-local public AI runtime installed for this launch.
    /// 为本次启动安装的进程内公共 AI 运行时。
    pub(crate) public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
}

/// Builds the final runtime-stream launch payload from one execution request and one completed bootstrap payload.
/// 根据一份执行请求与一份完成后的 bootstrap 载荷构建最终运行时流启动载荷。
///
/// # Parameters
/// 参数
/// `request` owns replay and Provider-continuation identity, `bootstrap` owns validated runtime facts,
/// and `task_execution_context` optionally binds a child task runtime.
/// `request` 持有重放与 Provider 延续身份，`bootstrap` 持有已校验运行时事实，
/// `task_execution_context` 可选绑定子任务运行时。
///
/// # Returns
/// 返回值
/// Returns the launch payload, or an explicit error when mutually exclusive continuation sources conflict.
/// 返回启动载荷；当互斥的延续来源冲突时返回显式错误。
pub(crate) fn build_runtime_stream_launch_payload(
    request: &ExecutionPreparationRequest,
    bootstrap: RuntimeStreamBootstrap,
    task_execution_context: Option<TaskExecutionRuntimeContext>,
) -> Result<RuntimeStreamLaunch, String> {
    let turn_start_mode = runtime_stream_turn_start_mode_for_request(request);
    let effective_prompt =
        controller_effective_prompt_for_launch(request, &bootstrap.core_snapshot);
    // providerTelemetryContext captures model and owner facts before preparation moves into the launch payload.
    // providerTelemetryContext 在执行准备移入启动载荷前捕获模型与归属事实。
    let provider_telemetry_context = ProviderRequestTelemetryContext::from_preparation(
        &bootstrap.preparation,
        request.provider_request_continuation.as_ref(),
    )?;
    // provider_api_family preserves the canonical typed API family before preparation moves into the launch payload.
    // provider_api_family 在执行准备移入启动载荷前保留规范强类型 API 协议族。
    let protocol_profile_id = bootstrap
        .preparation
        .model_blueprint
        .protocol_profile_id
        .clone();
    // streamResolvedTools freezes parser capabilities before definitions move into the launch payload.
    // streamResolvedTools 在工具定义移入启动载荷前冻结解析器能力。
    let stream_resolved_tools = bootstrap.resolved_tool_definitions.clone();
    Ok(RuntimeStreamLaunch {
        task_execution_context,
        session_id: request.session_id,
        effective_prompt,
        turn_start_mode,
        preparation: bootstrap.preparation,
        active_agent: bootstrap.active_agent,
        task_runtime: bootstrap.task_runtime,
        tools_manager: bootstrap.tools_manager,
        resolved_tool_definitions: bootstrap.resolved_tool_definitions,
        discovery_events: bootstrap.discovery_events,
        discovery_rows: bootstrap.discovery_rows,
        discovery_errors: bootstrap.discovery_errors,
        warnings: bootstrap.warnings,
        system_preamble: bootstrap.system_preamble,
        dynamic_prompt_layers: bootstrap.dynamic_prompt_layers,
        locked_baseline: bootstrap.locked_baseline,
        core_snapshot: bootstrap.core_snapshot,
        stream_state: RuntimeStreamDeltaState::new(bootstrap.core_controller, protocol_profile_id)
            .with_tool_request_stream(&stream_resolved_tools)
            .with_provider_request_telemetry_context(provider_telemetry_context),
        lowering_repair_events: bootstrap.lowering_repair_events,
        public_ai_runtime: bootstrap.public_ai_runtime,
    })
}

/// Resolves how the provider loop should start the next controller turn.
/// 解析 Provider 循环应如何启动下一次控制器回合。
fn runtime_stream_turn_start_mode_for_request(
    request: &ExecutionPreparationRequest,
) -> RuntimeStreamTurnStartMode {
    if let Some(mode) = request.turn_start_mode {
        return match mode {
            ExecutionTurnStartMode::UserPrompt => RuntimeStreamTurnStartMode::UserPrompt,
            ExecutionTurnStartMode::RestoredCheckpoint => {
                RuntimeStreamTurnStartMode::RestoredCheckpoint
            }
        };
    }
    if request.provider_visible_agent_messages.is_empty() {
        RuntimeStreamTurnStartMode::UserPrompt
    } else {
        RuntimeStreamTurnStartMode::RestoredCheckpoint
    }
}

/// Resolves the prompt text handed to `vulcan-core` so preseeded user messages are not duplicated after provider projection.
/// 解析交给 `vulcan-core` 的提示词文本，避免 Provider 投影后重复追加已预播种的用户消息。
///
/// # Parameters
/// 参数
/// `request` is the original execution request carrying the user-authored prompt.
/// `request` 是携带用户原始提示词的执行请求。
/// `core_snapshot` is the provider-visible snapshot already prepared for the controller.
/// `core_snapshot` 是已经为控制器准备好的 Provider 可见快照。
///
/// # Returns
/// 返回值
/// The projected latest user text when present, otherwise the raw request prompt.
/// 当存在时返回投影后的最新用户文本，否则返回原始请求提示词。
fn controller_effective_prompt_for_launch(
    request: &ExecutionPreparationRequest,
    core_snapshot: &CoreConversationSnapshot,
) -> String {
    if !request.provider_visible_agent_messages.is_empty() {
        return request
            .provider_visible_agent_messages
            .iter()
            .map(|message| {
                CoreConversationPayload::render_agent_message_text(
                    &message.message_type,
                    &message.task_name,
                    &message.sender,
                    &message.payload,
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
    }
    let raw_prompt = request.prompt.trim().to_string();
    core_snapshot
        .messages
        .last()
        .and_then(|message| match &message.payload {
            CoreConversationPayload::UserText { text, .. } => Some(text.trim().to_string()),
            _ => None,
        })
        .filter(|text| projected_user_prompt_matches_request(text, &raw_prompt))
        .unwrap_or(raw_prompt)
}

/// Returns whether one projected user prompt can safely stand in for the raw request prompt.
/// 判断一条投影后的用户提示词是否可以安全代表原始请求提示词。
///
/// # Parameters
/// 参数
/// `projected_prompt` is the latest user text already present in the controller snapshot.
/// `projected_prompt` 是控制器快照中已经存在的最新用户文本。
/// `raw_prompt` is the trimmed user prompt carried by the execution request.
/// `raw_prompt` 是执行请求携带的裁剪后用户提示词。
///
/// # Returns
/// 返回值
/// `true` when the projected prompt keeps the raw prompt as one complete paragraph.
/// 当投影提示词仍把原始提示词保留为一个完整段落时返回 `true`。
fn projected_user_prompt_matches_request(projected_prompt: &str, raw_prompt: &str) -> bool {
    if projected_prompt.is_empty() || raw_prompt.is_empty() {
        return false;
    }
    projected_prompt
        .split("\n\n")
        .any(|paragraph| paragraph.trim() == raw_prompt)
}

#[cfg(test)]
mod tests {
    //! Launch-payload assembly tests for runtime stream orchestration.
    //! 面向运行时流编排的启动载荷组装测试。

    use super::*;
    use crate::agent_runtime::conversation_hooks::ConversationHookRuntime;
    use crate::agent_runtime::execution_models::ExecutionProviderVisibleAgentMessage;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::runtime_stream_bootstrap_core::build_runtime_bootstrap_core;
    use crate::conversation_protocol::SessionConversationState;
    use crate::workbench_prompt_turn::{
        SharedPromptTurnContinueInput, SharedPromptTurnRuntimeBeginInput,
        begin_shared_prompt_turn_with_runtime_state,
        continue_shared_prompt_turn_after_preparation_with_runtime_state,
    };
    use std::sync::Arc;
    use uuid::Uuid;
    use vulcan_core::controller::VulcanCoreController;
    use vulcan_core::types::{
        ControllerConfig, ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
    };
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
    use vulcan_workbench_model::{PendingSessionPromptSource, SessionRuntimeStateEntry};

    /// Builds one minimal execution preparation used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小执行准备对象。
    fn build_test_preparation() -> ExecutionPreparation {
        ExecutionPreparation {
            session_id: Uuid::nil(),
            session_storage_directory: None,
            conversation_hook_turn_id: None,
            continuation_action_id: None,
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            agent_prompt_snapshot: None,
            subagent_prompt_context: None,
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                service_entry_id: String::from("public-api"),
                connection_id: String::from("test-connection"),
                provider_name: String::from("OpenAI"),
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
                manifest_revision: 0,
                frozen_selection: None,
                route_revision: String::from("sha256:test-route"),
                secondary_route: Default::default(),
                public_capabilities: vulcan_ai_contract::ModelCapabilities::default(),
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                reasoning: Default::default(),
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 1,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            output_style_snapshot: None,
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            additional_developer_segments: Vec::new(),
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: None,
            context_compaction_policy: None,
            context_budget_override: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: None,
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            vmm_runtime_ready: false,
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
        }
    }

    /// Builds one minimal execution request used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小执行请求。
    fn build_test_request() -> ExecutionPreparationRequest {
        ExecutionPreparationRequest {
            session_id: Uuid::nil(),
            session_storage_directory: None,
            managed_codebase_memory: None,
            execution_scope: vulcan_session_core::ExecutionScope::MainSession {
                session_id: vulcan_session_core::SessionNodeId::from(Uuid::nil()),
            },
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::main_session(),
            conversation_hook_turn_id: Some(String::from("test-hook-turn")),
            continuation_action_id: None,
            provider_request_continuation: None,
            execution_agent_id: None,
            agent_prompt_snapshot: None,
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
            prompt: String::from("  请分析主入口文件  "),
            output_style_snapshot: None,
            provider_visible_agent_messages: Vec::new(),
            turn_start_mode: None,
            conversation_state: SessionConversationState::default(),
            context_compaction_policy: None,
            context_compaction_window_node_id: None,
            context_budget_override: None,
            context_window_prefill_input_tokens: None,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            reasoning_effort: crate::provider_config::ReasoningEffortLevel::Off,
            tool_exposure_override: Default::default(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            tool_request_hidden_tool_ids: Vec::new(),
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skill_policy_override: Default::default(),
            project_prompt_skills: Default::default(),
            session_prompt_skill_overrides: Default::default(),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            vmm_runtime_ready: true,
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            controller_runtime: None,
            native_goal_runtime: None,
            media_resource_runtime: None,
            knowledge_access: None,
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
        }
    }

    /// Builds one minimal canonical snapshot used to verify controller seeding during launch assembly.
    /// 构建一份最小规范快照，用于验证启动组装期间的控制器播种行为。
    fn build_test_core_snapshot() -> CoreConversationSnapshot {
        CoreConversationSnapshot {
            messages: vec![CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("请分析主入口文件"),
                    attachments: Vec::new(),
                },
            }],
            ..CoreConversationSnapshot::default()
        }
    }

    /// Builds one minimal bootstrap payload used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小 bootstrap 载荷。
    fn build_test_bootstrap() -> RuntimeStreamBootstrap {
        let core_snapshot = build_test_core_snapshot();
        let core_controller = VulcanCoreController::new(
            ControllerConfig {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-test"),
                system_prompt: String::from("system"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    "system",
                ),
                temperature: Some(0.2),
                max_turns: 4,
                additional_params: None,
                context_injection: None,
            },
            core_snapshot.clone(),
        )
        .expect("valid core conversation snapshot");

        RuntimeStreamBootstrap {
            preparation: build_test_preparation(),
            active_agent: None,
            public_ai_runtime: None,
            task_runtime: Arc::new(TaskRuntime::new()),
            tools_manager: VulcanToolsManager::default(),
            resolved_tool_definitions: Vec::new(),
            discovery_events: Vec::new(),
            discovery_rows: vec![String::from("mcp-row")],
            discovery_errors: vec![String::from("mcp-error")],
            warnings: vec![String::from("warning")],
            system_preamble: String::from("system preamble"),
            dynamic_prompt_layers: CoreProviderPromptLayers::default(),
            locked_baseline: LockedBaselineProjection::empty(),
            core_snapshot,
            core_controller,
            lowering_repair_events: Vec::new(),
        }
    }

    /// Trims the effective prompt and preserves the bootstrapped controller snapshot inside the launch delta state.
    /// 裁剪生效提示词，并在启动增量状态中保留已 bootstrap 的控制器快照。
    #[test]
    fn launch_payload_assembly_trims_prompt_and_preserves_controller_seed() {
        let request = build_test_request();
        let launch = build_runtime_stream_launch_payload(&request, build_test_bootstrap(), None)
            .expect("valid runtime stream launch payload");

        assert_eq!(launch.session_id, request.session_id);
        assert_eq!(
            launch.turn_start_mode,
            RuntimeStreamTurnStartMode::UserPrompt
        );
        assert_eq!(launch.effective_prompt, "请分析主入口文件");
        assert_eq!(launch.discovery_rows, vec![String::from("mcp-row")]);
        assert_eq!(launch.discovery_errors, vec![String::from("mcp-error")]);
        assert_eq!(launch.warnings, vec![String::from("warning")]);
        assert_eq!(launch.core_snapshot.messages.len(), 1);
        assert_eq!(
            launch.stream_state.controller().snapshot().messages.len(),
            1
        );
        assert!(launch.stream_state.controller().events().is_empty());
    }

    /// Uses seeded agent messages as the controller turn start without creating a user prompt.
    /// 使用已播种智能体消息作为控制器回合起点，不创建用户提示词。
    #[test]
    fn launch_payload_uses_seeded_agent_messages_without_user_prompt() {
        let mut request = build_test_request();
        request.prompt.clear();
        request.provider_visible_agent_messages = vec![ExecutionProviderVisibleAgentMessage {
            delivery_id: String::from("delivery-1"),
            message_type: String::from("FINAL_ANSWER"),
            task_name: String::from("quick_agent"),
            sender: String::from("quick_agent"),
            recipient: String::from("parent:00000000-0000-0000-0000-000000000000"),
            payload: String::from("abc"),
            trigger_turn: true,
            timestamp_ms: 1,
            metadata: None,
        }];
        let preparation = build_test_preparation();
        let runtime_core = build_runtime_bootstrap_core(&preparation, &request, None)
            .expect("runtime bootstrap core should seed agent messages");
        assert!(matches!(
            runtime_core.core_snapshot.messages.last().map(|message| &message.payload),
            Some(CoreConversationPayload::AgentMessage { payload, .. }) if payload == "abc"
        ));
        let crate::agent_runtime::runtime_stream_bootstrap_core::RuntimeBootstrapCore {
            system_preamble,
            dynamic_prompt_layers,
            locked_baseline,
            core_snapshot,
            core_controller,
            ..
        } = runtime_core;
        let bootstrap = RuntimeStreamBootstrap {
            preparation,
            active_agent: None,
            public_ai_runtime: None,
            task_runtime: Arc::new(TaskRuntime::new()),
            tools_manager: VulcanToolsManager::default(),
            resolved_tool_definitions: Vec::new(),
            discovery_events: Vec::new(),
            discovery_rows: Vec::new(),
            discovery_errors: Vec::new(),
            warnings: Vec::new(),
            system_preamble,
            dynamic_prompt_layers,
            locked_baseline,
            core_snapshot,
            core_controller,
            lowering_repair_events: Vec::new(),
        };

        let launch = build_runtime_stream_launch_payload(&request, bootstrap, None)
            .expect("valid runtime stream launch payload");

        assert_eq!(
            launch.turn_start_mode,
            RuntimeStreamTurnStartMode::RestoredCheckpoint
        );
        assert!(launch.effective_prompt.contains("<agent_message>"));
        assert_eq!(
            launch.stream_state.controller().snapshot().messages.len(),
            1
        );
    }

    /// Uses projected latest user text as the controller prompt so attachment bridge text does not duplicate the raw user message.
    /// 使用投影后的最新用户文本作为控制器提示词，避免附件桥接文本重复追加原始用户消息。
    #[test]
    fn launch_payload_uses_projected_latest_user_prompt_for_controller_reuse() {
        let request = build_test_request();
        let mut bootstrap = build_test_bootstrap();
        bootstrap.core_snapshot.messages = vec![CoreConversationMessage {
            id: String::from("user-projected"),
            payload: CoreConversationPayload::UserText {
                text: String::from(
                    "INFO: Attachment \"image.png\" is kept locally.\n\n请分析主入口文件",
                ),
                attachments: Vec::new(),
            },
        }];

        let launch = build_runtime_stream_launch_payload(&request, bootstrap, None)
            .expect("valid runtime stream launch payload");

        assert_eq!(
            launch.effective_prompt,
            "INFO: Attachment \"image.png\" is kept locally.\n\n请分析主入口文件"
        );
    }

    /// Falls back to the request prompt when the latest projected user text belongs to history.
    /// 当最新投影用户文本属于历史记录时回退使用请求提示词。
    #[test]
    fn launch_payload_falls_back_when_projected_user_prompt_mismatches_request() {
        let request = build_test_request();
        let mut bootstrap = build_test_bootstrap();
        bootstrap.core_snapshot.messages = vec![CoreConversationMessage {
            id: String::from("historical-user"),
            payload: CoreConversationPayload::UserText {
                text: String::from("历史用户输入"),
                attachments: Vec::new(),
            },
        }];

        let launch = build_runtime_stream_launch_payload(&request, bootstrap, None)
            .expect("valid runtime stream launch payload");

        assert_eq!(launch.effective_prompt, "请分析主入口文件");
    }

    /// Reuses projected text when an unsupported attachment note follows the raw prompt.
    /// 当不支持的附件提示跟随原始提示词时复用投影文本。
    #[test]
    fn launch_payload_reuses_projected_prompt_when_suffix_note_follows_request() {
        let request = build_test_request();
        let mut bootstrap = build_test_bootstrap();
        bootstrap.core_snapshot.messages = vec![CoreConversationMessage {
            id: String::from("user-projected-suffix"),
            payload: CoreConversationPayload::UserText {
                text: String::from(
                    "请分析主入口文件\n\nERROR: Attachment \"image.png\" cannot be uploaded by this API path.",
                ),
                attachments: Vec::new(),
            },
        }];

        let launch = build_runtime_stream_launch_payload(&request, bootstrap, None)
            .expect("valid runtime stream launch payload");

        assert_eq!(
            launch.effective_prompt,
            "请分析主入口文件\n\nERROR: Attachment \"image.png\" cannot be uploaded by this API path."
        );
    }

    /// Reuses projected text when bridge and downgrade notes wrap the raw prompt.
    /// 当桥接与降级提示包裹原始提示词时复用投影文本。
    #[test]
    fn launch_payload_reuses_projected_prompt_when_notes_wrap_request() {
        let request = build_test_request();
        let mut bootstrap = build_test_bootstrap();
        bootstrap.core_snapshot.messages = vec![CoreConversationMessage {
            id: String::from("user-projected-wrapped"),
            payload: CoreConversationPayload::UserText {
                text: String::from(
                    "INFO: Attachment \"image.png\" is kept locally.\n\n请分析主入口文件\n\nERROR: Attachment \"demo.pdf\" cannot be uploaded by this API path.",
                ),
                attachments: Vec::new(),
            },
        }];

        let launch = build_runtime_stream_launch_payload(&request, bootstrap, None)
            .expect("valid runtime stream launch payload");

        assert_eq!(
            launch.effective_prompt,
            "INFO: Attachment \"image.png\" is kept locally.\n\n请分析主入口文件\n\nERROR: Attachment \"demo.pdf\" cannot be uploaded by this API path."
        );
    }

    /// Covers the workbench begin/continue/launch chain so the preseeded user prompt remains the controller prompt.
    /// 覆盖工作台 begin/continue/launch 链路，确保预播种用户提示词仍作为控制器提示词。
    #[test]
    fn launch_payload_preserves_workbench_preseeded_user_prompt_through_shared_turn_flow() {
        let session_id = Uuid::nil();
        let prompt = "请分析主入口文件";
        let attachment = ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: String::from("image.png"),
            local_path: String::from(r"D:\workspace\image.png"),
            resource_path: None,
            source_path: None,
            asset_id: None,
            byte_len: Some(1024),
            source_sha256: None,
            delivery_mode: None,
            preview_local_path: None,
            mime_type: String::from("image/png"),
        };
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut hook_runtime = ConversationHookRuntime::new();
        let start = begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id,
                session_title: "测试会话",
                workspace_path: r"D:\workspace",
                prompt_entry_id: String::from("prompt-entry-1"),
                continuation_action_id: None,
                prompt,
                hidden_model_guidance: None,
                provider_visible_agent_messages: Vec::new(),
                attachments: vec![attachment],
                provider_attachments: None,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                vmm_runtime_ready: true,
                vmm_precheck_ready: true,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                owning_project_id: None,
            },
        )
        .expect("begin shared prompt turn");
        let precheck_outcome = start.precheck_request.clone().skip();
        // profile_outcome keeps profile preparation distinct in this transport-focused test.
        // profile_outcome 在该传输链路测试中保持画像准备的独立性。
        let profile_outcome = start.profile_request.clone().skip();
        // vmm_session_memory models the Core controller candidate for this launch path.
        // vmm_session_memory 模拟该启动链路中的 Core 控制器候选状态。
        let mut vmm_session_memory =
            vulcan_vmm::VmmSessionMemoryState::new(session_id.to_string(), "1")
                .expect("test VMM session owner should be valid");
        let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
            &mut runtime_state,
            &mut vmm_session_memory,
            SharedPromptTurnContinueInput {
                start,
                precheck_outcome,
                profile_outcome,
                project_configuration: Default::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                patch_visible_user_prompt_record: true,
            },
        )
        .expect("continue shared prompt turn");
        let request = continuation.request;
        let latest_request_message = request
            .conversation_state
            .messages
            .last()
            .expect("request should contain preseeded user message");
        assert!(matches!(
            &latest_request_message.payload,
            crate::conversation_protocol::SessionConversationPayload::UserText {
                text,
                attachments,
            } if text == prompt && attachments.len() == 1
        ));

        let mut preparation = build_test_preparation();
        preparation.session_id = session_id;
        preparation.session_title = String::from("测试会话");
        preparation.workspace_path = String::from(r"D:\workspace");
        let runtime_core = build_runtime_bootstrap_core(&preparation, &request, None)
            .expect("runtime bootstrap core should build");
        let crate::agent_runtime::runtime_stream_bootstrap_core::RuntimeBootstrapCore {
            system_preamble,
            prompt_layers: _,
            dynamic_prompt_layers,
            locked_baseline,
            core_snapshot,
            core_controller,
            lowering_repair_events: _,
        } = runtime_core;
        let latest_projected_message = core_snapshot
            .messages
            .last()
            .expect("projected snapshot should contain latest user message");
        let CoreConversationPayload::UserText {
            text: projected_text,
            attachments: projected_attachments,
        } = &latest_projected_message.payload
        else {
            panic!("latest projected message should be user text");
        };
        assert!(projected_text.contains(prompt));
        assert!(projected_text.contains("ATTACHMENT:"));
        assert_ne!(projected_text, prompt);
        assert!(projected_attachments.is_empty());
        let expected_effective_prompt = projected_text.clone();
        let bootstrap = RuntimeStreamBootstrap {
            preparation,
            active_agent: None,
            public_ai_runtime: None,
            task_runtime: Arc::new(TaskRuntime::new()),
            tools_manager: VulcanToolsManager::default(),
            resolved_tool_definitions: Vec::new(),
            discovery_events: Vec::new(),
            discovery_rows: Vec::new(),
            discovery_errors: Vec::new(),
            warnings: Vec::new(),
            system_preamble,
            dynamic_prompt_layers,
            locked_baseline,
            core_snapshot,
            core_controller,
            lowering_repair_events: Vec::new(),
        };

        let launch = build_runtime_stream_launch_payload(&request, bootstrap, None)
            .expect("valid runtime stream launch payload");

        assert_eq!(launch.effective_prompt, expected_effective_prompt);
        assert_eq!(
            launch.stream_state.controller().snapshot().messages.len(),
            1
        );
    }
}
