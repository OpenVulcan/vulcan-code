//! Runtime stream launch payload model and bootstrap-to-launch assembly helpers.
//! 面向运行时流启动载荷模型与 bootstrap 到 launch 组装转换的辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::runtime_stream_bootstrap::RuntimeStreamBootstrap;
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::model::SessionId;
use std::sync::Arc;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Launch payload that captures everything the outer runtime stream needs after preparation and bootstrap have both succeeded.
/// 捕获准备阶段与启动阶段都成功后外层运行时流所需全部内容的启动载荷。
pub struct RuntimeStreamLaunch {
    /// Session identifier owned by the current runtime run.
    /// 当前运行时执行所属的会话标识。
    pub(crate) session_id: SessionId,
    /// Trimmed effective user prompt that will be sent to the provider transport layer.
    /// 将发送给 Provider 传输层的裁剪后生效用户提示词。
    pub(crate) effective_prompt: String,
    /// Updated execution preparation after request-snapshot seeding.
    /// 完成请求快照播种之后的更新执行准备对象。
    pub(crate) preparation: ExecutionPreparation,
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub(crate) active_agent: Option<RuntimeAgentDefinition>,
    /// Shared resumable task runtime reused by the current execution turn.
    /// 被当前执行回合复用的共享可恢复任务运行时。
    pub(crate) task_runtime: Arc<TaskRuntime>,
    /// Shared token-budget runtime reused for terminal budget backfills.
    /// 用于终态预算回填的共享 Token 预算运行时。
    pub(crate) token_budget_runtime: Arc<TokenBudgetRuntime>,
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
    /// Provider-visible history reconstructed from canonical conversation messages.
    /// 从规范会话消息重建得到的 Provider 可见历史记录。
    pub(crate) request_history: Vec<CoreConversationMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub(crate) core_snapshot: CoreConversationSnapshot,
    /// Mutable streaming-loop state seeded from the bootstrapped core controller.
    /// 由已启动核心控制器播种得到的可变流式循环状态。
    pub(crate) stream_state: RuntimeStreamDeltaState,
}

/// Builds the final runtime-stream launch payload from one execution request and one completed bootstrap payload.
/// 根据一份执行请求与一份完成后的 bootstrap 载荷构建最终运行时流启动载荷。
pub(crate) fn build_runtime_stream_launch_payload(
    request: &ExecutionPreparationRequest,
    bootstrap: RuntimeStreamBootstrap,
) -> RuntimeStreamLaunch {
    let effective_prompt =
        controller_effective_prompt_for_launch(request, &bootstrap.core_snapshot);
    RuntimeStreamLaunch {
        session_id: request.session_id,
        effective_prompt,
        preparation: bootstrap.preparation,
        active_agent: bootstrap.active_agent,
        task_runtime: bootstrap.task_runtime,
        token_budget_runtime: bootstrap.token_budget_runtime,
        tools_manager: bootstrap.tools_manager,
        resolved_tool_definitions: bootstrap.resolved_tool_definitions,
        discovery_events: bootstrap.discovery_events,
        discovery_rows: bootstrap.discovery_rows,
        discovery_errors: bootstrap.discovery_errors,
        warnings: bootstrap.warnings,
        system_preamble: bootstrap.system_preamble,
        request_history: bootstrap.request_history,
        core_snapshot: bootstrap.core_snapshot,
        stream_state: RuntimeStreamDeltaState::new(bootstrap.core_controller),
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
    let raw_prompt = request.prompt.trim().to_string();
    core_snapshot
        .messages
        .last()
        .and_then(|message| match &message.payload {
            vulcan_core::types::ConversationPayload::UserText { text, .. } => {
                Some(text.trim().to_string())
            }
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
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::runtime_stream_bootstrap_core::build_runtime_bootstrap_core;
    use crate::conversation_protocol::SessionConversationState;
    use crate::provider_config::{PersistedAiToolConfiguration, ProviderRuntimeKind};
    use crate::workbench_prompt_turn::{
        SharedPromptTurnContinueInput, SharedPromptTurnRuntimeBeginInput,
        begin_shared_prompt_turn_with_runtime_state,
        continue_shared_prompt_turn_after_precheck_with_runtime_state,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
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
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                provider_name: String::from("OpenAI"),
                runtime_kind: ProviderRuntimeKind::OpenAi,
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({}),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 1,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            prompt_skill_developer_segment: None,
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: None,
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
        }
    }

    /// Builds one minimal execution request used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小执行请求。
    fn build_test_request() -> ExecutionPreparationRequest {
        ExecutionPreparationRequest {
            session_id: Uuid::nil(),
            session_title: String::from("测试会话"),
            workspace_path: String::from(r"D:\workspace"),
            prompt: String::from("  请分析主入口文件  "),
            conversation_state: SessionConversationState::default(),
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            reasoning_effort: crate::provider_config::ReasoningEffortLevel::Off,
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            project_prompt_skills: Default::default(),
            session_prompt_skill_overrides: Default::default(),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
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
        );

        RuntimeStreamBootstrap {
            preparation: build_test_preparation(),
            active_agent: None,
            task_runtime: Arc::new(TaskRuntime::new()),
            token_budget_runtime: Arc::new(TokenBudgetRuntime::initialize_from_resource_directory(
                std::env::temp_dir(),
            )),
            tools_manager: VulcanToolsManager::default(),
            resolved_tool_definitions: Vec::new(),
            discovery_events: Vec::new(),
            discovery_rows: vec![String::from("mcp-row")],
            discovery_errors: vec![String::from("mcp-error")],
            warnings: vec![String::from("warning")],
            system_preamble: String::from("system preamble"),
            request_history: Vec::new(),
            core_snapshot,
            core_controller,
        }
    }

    /// Trims the effective prompt and preserves the bootstrapped controller snapshot inside the launch delta state.
    /// 裁剪生效提示词，并在启动增量状态中保留已 bootstrap 的控制器快照。
    #[test]
    fn launch_payload_assembly_trims_prompt_and_preserves_controller_seed() {
        let request = build_test_request();
        let launch = build_runtime_stream_launch_payload(&request, build_test_bootstrap());

        assert_eq!(launch.session_id, request.session_id);
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

        let launch = build_runtime_stream_launch_payload(&request, bootstrap);

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

        let launch = build_runtime_stream_launch_payload(&request, bootstrap);

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

        let launch = build_runtime_stream_launch_payload(&request, bootstrap);

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

        let launch = build_runtime_stream_launch_payload(&request, bootstrap);

        assert_eq!(
            launch.effective_prompt,
            "INFO: Attachment \"image.png\" is kept locally.\n\n请分析主入口文件\n\nERROR: Attachment \"demo.pdf\" cannot be uploaded by this API path."
        );
    }

    /// Covers the WebUI-style begin/continue/launch chain so the preseeded user prompt remains the controller prompt.
    /// 覆盖 WebUI 风格的 begin/continue/launch 链路，确保预播种用户提示词仍作为控制器提示词。
    #[test]
    fn launch_payload_preserves_webui_preseeded_user_prompt_through_shared_turn_flow() {
        let session_id = Uuid::nil();
        let prompt = "请分析主入口文件";
        let attachment = ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: String::from("image.png"),
            local_path: String::from(r"D:\workspace\image.png"),
            source_path: None,
            asset_id: None,
            byte_len: Some(1024),
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
                prompt,
                attachments: vec![attachment],
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                disabled_ai_capability_kinds: Default::default(),
                disabled_ai_capability_target_selectors: Default::default(),
                ai_capability_target_order_overrides: Default::default(),
                owning_project_id: None,
                prior_session_messages: None,
            },
        )
        .expect("begin shared prompt turn");
        let precheck_outcome = start.precheck_request.clone().skip();
        let continuation = continue_shared_prompt_turn_after_precheck_with_runtime_state(
            &mut runtime_state,
            SharedPromptTurnContinueInput {
                start,
                precheck_outcome,
                project_configuration: Default::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                ai_tool_configuration: PersistedAiToolConfiguration::default(),
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
        preparation.model_blueprint.capabilities.input_modalities = vec![String::from("text")];
        let runtime_core = build_runtime_bootstrap_core(&preparation, &request, None);
        let crate::agent_runtime::runtime_stream_bootstrap_core::RuntimeBootstrapCore {
            system_preamble,
            prompt_layers: _,
            request_history,
            core_snapshot,
            core_controller,
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
        assert!(projected_text.contains("Attachment"));
        assert_ne!(projected_text, prompt);
        assert!(projected_attachments.is_empty());
        let expected_effective_prompt = projected_text.clone();
        let bootstrap = RuntimeStreamBootstrap {
            preparation,
            active_agent: None,
            task_runtime: Arc::new(TaskRuntime::new()),
            token_budget_runtime: Arc::new(TokenBudgetRuntime::initialize_from_resource_directory(
                std::env::temp_dir(),
            )),
            tools_manager: VulcanToolsManager::default(),
            resolved_tool_definitions: Vec::new(),
            discovery_events: Vec::new(),
            discovery_rows: Vec::new(),
            discovery_errors: Vec::new(),
            warnings: Vec::new(),
            system_preamble,
            request_history,
            core_snapshot,
            core_controller,
        };

        let launch = build_runtime_stream_launch_payload(&request, bootstrap);

        assert_eq!(launch.effective_prompt, expected_effective_prompt);
        assert_eq!(
            launch.stream_state.controller().snapshot().messages.len(),
            1
        );
    }
}
