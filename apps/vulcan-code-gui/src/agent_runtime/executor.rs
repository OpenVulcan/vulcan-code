//! Execution-preparation models and real provider execution helpers for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备模型与真实 Provider 执行辅助逻辑。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider::{ProviderRuntimeBlueprint, resolve_active_provider_blueprint};
use crate::agent_runtime::rig_tools::VulcanManagedToolRuntimeBridge;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    conversation_messages_to_rig_history, replace_messages_from_rig_history,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::model::SessionId;
use crate::provider_config::{
    PersistedProviderOptions, ProviderConfigurationFile, ProviderRuntimeKind,
};
use crate::runtime_assets;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use async_stream::stream;
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use rig::OneOrMany;
use rig::completion::{CompletionModel, ToolDefinition as RigToolDefinition};
use rig::message::{
    AssistantContent as RigAssistantContent, Message as RigMessage, ReasoningContent,
    ToolResultContent, UserContent as RigUserContent,
};
use rig::prelude::CompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use rig::streaming::StreamedAssistantContent;
use serde_json::{Map as JsonMap, Value, json};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::Path, path::PathBuf};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::tool::{AllowAllToolPolicy, VulcanToolExecutor};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationMessage as CoreConversationMessage,
    ConversationPayload as CoreConversationPayload, ConversationRecord as CoreConversationRecord,
    ConversationSnapshot as CoreConversationSnapshot,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    CoreEvent as CoreControllerEvent, CoreEventKind as CoreControllerEventKind,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
    TranscriptRecordKind as CoreTranscriptRecordKind, TranscriptRole as CoreTranscriptRole,
};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolDescriptor, VulcanToolSourceKind, VulcanToolsManager,
};

/// Relative resource directory name that stores provider base system prompts.
/// 存放 Provider 基础系统提示词的相对资源目录名称。
const SYSTEM_PROMPTS_DIR_NAME: &str = "system-prompts";

/// Default provider prompt file name.
/// 默认 Provider 提示词文件名。
const DEFAULT_SYSTEM_PROMPT_FILE_NAME: &str = "default.txt";

/// GPT-family provider prompt file name.
/// GPT 家族 Provider 提示词文件名。
const GPT_SYSTEM_PROMPT_FILE_NAME: &str = "gpt.txt";

/// Codex-family provider prompt file name.
/// Codex 家族 Provider 提示词文件名。
const CODEX_SYSTEM_PROMPT_FILE_NAME: &str = "codex.txt";

/// Beast-style provider prompt file name.
/// Beast 风格 Provider 提示词文件名。
const BEAST_SYSTEM_PROMPT_FILE_NAME: &str = "beast.txt";

/// Anthropic provider prompt file name.
/// Anthropic Provider 提示词文件名。
const ANTHROPIC_SYSTEM_PROMPT_FILE_NAME: &str = "anthropic.txt";

/// Gemini provider prompt file name.
/// Gemini Provider 提示词文件名。
const GEMINI_SYSTEM_PROMPT_FILE_NAME: &str = "gemini.txt";

/// Kimi-family provider prompt file name.
/// Kimi 家族 Provider 提示词文件名。
const KIMI_SYSTEM_PROMPT_FILE_NAME: &str = "kimi.txt";

/// Trinity-family provider prompt file name.
/// Trinity 家族 Provider 提示词文件名。
const TRINITY_SYSTEM_PROMPT_FILE_NAME: &str = "trinity.txt";

/// Minimal execution request captured from the current GUI state.
/// 从当前 GUI 状态捕获的最小执行请求。
#[derive(Debug, Clone)]
pub struct ExecutionPreparationRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: SessionId,
    /// Target session title.
    /// 目标会话标题。
    pub session_title: String,
    /// Current workspace path.
    /// 当前工作区路径。
    pub workspace_path: String,
    /// Current composer prompt.
    /// 当前提示词输入区内容。
    pub prompt: String,
    /// Canonical conversation state reconstructed from the session protocol state.
    /// 从会话协议状态重建出的规范会话状态。
    pub conversation_state: SessionConversationState,
    /// Current execution-mode label.
    /// 当前执行模式标签。
    pub execution_mode: String,
    /// Optional sampling temperature override from the run-time controls.
    /// 来自运行参数控制区的可选采样温度覆盖值。
    pub temperature: Option<f32>,
}

/// Prepared execution plan rendered by the GUI before real LLM execution begins.
/// 在真实 LLM 执行开始前由 GUI 渲染的执行准备计划。
#[derive(Debug, Clone)]
pub struct ExecutionPreparation {
    /// Active session identifier.
    /// 当前激活会话标识。
    pub session_id: SessionId,
    /// Active session title.
    /// 当前激活会话标题。
    pub session_title: String,
    /// Active agent identifier.
    /// 当前激活代理标识。
    pub agent_id: String,
    /// Active agent display name.
    /// 当前激活代理显示名称。
    pub agent_name: String,
    /// Active agent kind label.
    /// 当前激活代理类型标签。
    pub agent_kind: String,
    /// Validated provider/model runtime blueprint.
    /// 通过校验的 Provider/Model 运行时蓝图。
    pub model_blueprint: ProviderRuntimeBlueprint,
    /// Prompt preview shortened for activity logs.
    /// 为活动日志截断后的提示词预览。
    pub prompt_preview: String,
    /// Workspace path captured for the run.
    /// 为本次运行捕获的工作区路径。
    pub workspace_path: String,
    /// Number of history rows that will be forwarded into the real model call.
    /// 将被转发到真实模型调用中的历史行数量。
    pub context_history_count: usize,
    /// Execution-mode label captured for the run.
    /// 为本次运行捕获的执行模式标签。
    pub execution_mode: String,
    /// Optional sampling temperature captured for the run.
    /// 为本次运行捕获的可选采样温度。
    pub temperature: Option<f32>,
    /// Resolved skill names attached to the active agent.
    /// 附着在当前代理上的技能名称集合。
    pub skill_names: Vec<String>,
    /// Unified AI-visible tool descriptors resolved for the active mode.
    /// 为当前模式解析出的统一 AI 可见工具描述集合。
    pub tool_descriptors: Vec<VulcanToolDescriptor>,
    /// Delegate agent identifiers exposed to the active agent.
    /// 暴露给当前代理的委派代理标识集合。
    pub delegate_agents: Vec<String>,
    /// Enabled MCP server summaries visible to the runtime foundation.
    /// 对运行时基础架构可见的已启用 MCP 服务器摘要集合。
    pub enabled_mcp_servers: Vec<String>,
    /// Non-fatal preparation warnings.
    /// 非致命的准备阶段警告集合。
    pub warnings: Vec<String>,
    /// Full provider-request preview captured right before the real API call starts.
    /// 在真实 API 调用开始前捕获的完整 Provider 请求预览。
    pub provider_request_preview_markdown: String,
    /// Structured provider-request snapshot JSON captured by `vulcan-core` right before dispatch.
    /// 由 `vulcan-core` 在分发前捕获的结构化 Provider 请求快照 JSON。
    pub provider_request_snapshot_json: Option<Value>,
}

impl ExecutionPreparation {
    /// Returns a compact list of human-readable summary rows for UI cards and logs.
    /// 返回供界面卡片与日志使用的紧凑人类可读摘要行列表。
    pub fn summary_rows(&self) -> Vec<String> {
        let mut rows = vec![
            format!("会话：#{} · {}", self.session_id, self.session_title),
            format!(
                "代理：{}（{}，id={})",
                self.agent_name, self.agent_kind, self.agent_id
            ),
            format!(
                "模型：{} / {} · 运行时 {}",
                self.model_blueprint.provider_name,
                self.model_blueprint.model_name,
                self.model_blueprint.runtime_label
            ),
            format!(
                "路由：{}/{} · kind={}",
                self.model_blueprint.provider_id,
                self.model_blueprint.model_id,
                self.model_blueprint.runtime_kind_key()
            ),
            format!("接口：{}", self.model_blueprint.endpoint_summary()),
            format!("凭据：{}", self.model_blueprint.credential_source),
            format!(
                "模型发现：{}",
                if self.model_blueprint.supports_remote_model_refresh {
                    "支持远端刷新"
                } else {
                    "使用静态目录"
                }
            ),
            format!(
                "模式：{} · 温度 {}",
                self.execution_mode,
                self.temperature
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| String::from("使用模型默认值"))
            ),
            format!("工作区：{}", self.workspace_path),
            format!("上下文：{} 条历史消息", self.context_history_count),
        ];

        if !self.skill_names.is_empty() {
            rows.push(format!("技能：{}", self.skill_names.join("、")));
        }

        if !self.tool_descriptors.is_empty() {
            rows.push(format!(
                "工具：{} 个已直接暴露",
                self.tool_descriptors.len()
            ));
        }

        let native_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Native)
            .count();
        if native_tool_count > 0 {
            rows.push(format!("原生工具：{} 个", native_tool_count));
        }

        let luaskills_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::LuaSkills)
            .count();
        if luaskills_tool_count > 0 {
            rows.push(format!("LuaSkills 工具：{} 个", luaskills_tool_count));
        }

        let mcp_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Mcp)
            .count();
        if mcp_tool_count > 0 {
            rows.push(format!("MCP 工具：{} 个", mcp_tool_count));
        }

        if !self.delegate_agents.is_empty() {
            rows.push(format!("委派：{}", self.delegate_agents.join("、")));
        }

        if !self.enabled_mcp_servers.is_empty() {
            rows.push(format!("MCP：{}", self.enabled_mcp_servers.join("；")));
        }

        rows.push(format!("提示词：{}", self.prompt_preview));
        rows
    }
}

/// Real execution report returned by the async provider runtime after one run attempt.
/// 异步 Provider 运行时在单次执行后返回的真实执行报告。
#[derive(Debug, Clone)]
pub struct ExecutionRunReport {
    /// Session identifier owning the run.
    /// 拥有本次运行的会话标识。
    pub session_id: SessionId,
    /// Prepared execution snapshot when preparation succeeded.
    /// 当准备阶段成功时生成的执行准备快照。
    pub preparation: Option<ExecutionPreparation>,
    /// Assistant output returned by the real provider call.
    /// 真实 Provider 调用返回的助手输出。
    pub assistant_output: Option<String>,
    /// Assistant thinking output returned by the real provider call.
    /// 真实 Provider 调用返回的助手思考输出。
    pub thinking_output: Option<String>,
    /// Canonical conversation state rebuilt from the final rig history when the run succeeds.
    /// 当运行成功时根据最终 rig 历史重建出的规范会话状态。
    pub conversation_state: Option<SessionConversationState>,
    /// Successful MCP discovery summary rows collected during the run.
    /// 运行期间收集到的 MCP 成功发现摘要行集合。
    pub mcp_discovery_rows: Vec<String>,
    /// Failed MCP discovery summary rows collected during the run.
    /// 运行期间收集到的 MCP 失败发现摘要行集合。
    pub mcp_discovery_errors: Vec<String>,
    /// Non-fatal warnings collected during the run.
    /// 运行期间收集到的非致命警告集合。
    pub warnings: Vec<String>,
    /// Terminal error when the run failed.
    /// 当运行失败时的最终错误信息。
    pub error: Option<String>,
    /// Controller-owned event trace captured while the run was executing.
    /// 运行执行期间捕获的控制器自有事件轨迹。
    pub controller_events: Vec<CoreControllerEvent>,
}

impl ExecutionRunReport {
    /// Creates one success report.
    /// 创建一份成功执行报告。
    fn success(
        session_id: SessionId,
        preparation: ExecutionPreparation,
        thinking_output: Option<String>,
        assistant_output: String,
        conversation_state: Option<SessionConversationState>,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        controller_events: Vec<CoreControllerEvent>,
    ) -> Self {
        Self {
            session_id,
            preparation: Some(preparation),
            assistant_output: Some(assistant_output),
            thinking_output,
            conversation_state,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: None,
            controller_events,
        }
    }

    /// Creates one failure report.
    /// 创建一份失败执行报告。
    fn failure(
        session_id: SessionId,
        preparation: Option<ExecutionPreparation>,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        controller_events: Vec<CoreControllerEvent>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            preparation,
            assistant_output: None,
            thinking_output: None,
            conversation_state: None,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: Some(error.into()),
            controller_events,
        }
    }

    /// Returns whether the current report completed successfully.
    /// 返回当前报告是否成功完成。
    pub const fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// Builds one validated execution-preparation object from the current runtime foundation and GUI request.
/// 根据当前运行时基础架构与 GUI 请求构建一份通过校验的执行准备对象。
pub fn prepare_execution(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    config: &ProviderConfigurationFile,
    request: &ExecutionPreparationRequest,
) -> Result<ExecutionPreparation, String> {
    let Some(active_agent) = foundation.active_agent_definition() else {
        return Err(String::from("当前激活代理未能在运行时基础架构中解析到。"));
    };

    if !foundation.has_built_agent(&active_agent.id) {
        return Err(format!(
            "代理 `{}` 当前尚未成功构建，无法启动执行。",
            active_agent.name
        ));
    }

    let model_blueprint = resolve_active_provider_blueprint(config)?;
    let mut warnings = Vec::new();
    let workspace_path = request.workspace_path.trim().to_string();
    let tools_manager = build_vulcan_tools_manager(
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        mcp_runtime,
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone())),
        foundation.resolved_native_tools_for_agent(&active_agent.id),
        foundation.resolved_luaskills_tools_for_agent(&active_agent.id),
        foundation.resolved_mcp_tools_for_agent(&active_agent.id),
    );
    let resolved_tools = tools_manager.resolve_for_execution_mode(&request.execution_mode);

    if workspace_path.is_empty() {
        warnings.push(String::from(
            "当前工作区路径为空，后续原生工具可能无法提供完整上下文。",
        ));
    }

    if request.prompt.trim().is_empty() {
        warnings.push(String::from(
            "当前提示词为空，本次准备计划仅验证运行时链路。",
        ));
    }

    Ok(ExecutionPreparation {
        session_id: request.session_id,
        session_title: request.session_title.clone(),
        agent_id: active_agent.id.clone(),
        agent_name: active_agent.name.clone(),
        agent_kind: active_agent.kind.label().to_string(),
        model_blueprint,
        prompt_preview: truncate_preview(request.prompt.trim(), 180),
        workspace_path,
        context_history_count: request.conversation_state.messages.len(),
        execution_mode: request.execution_mode.clone(),
        temperature: request.temperature,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors: resolved_tools.descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: foundation.enabled_mcp_server_rows(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
    })
}

/// Lightweight provider-stream delta emitted while one real run is still in progress.
/// 单次真实运行仍在进行中时发出的轻量 Provider 流式增量。
enum ProviderStreamDelta {
    /// Structured request snapshot captured right before the next provider request is dispatched.
    /// 在下一次 Provider 请求真正发出前捕获的结构化请求快照。
    RequestSnapshot(CoreProviderRequestSnapshot),
    /// Assistant thinking delta returned by the provider stream.
    /// Provider 流返回的助手思考增量。
    Thinking(String),
    /// Assistant final-output delta returned by the provider stream.
    /// Provider 流返回的助手正式输出增量。
    Output(String),
    /// Runtime event emitted from one streamed provider-side tool interaction.
    /// 来自某次 Provider 侧流式工具交互的运行时事件。
    RuntimeEvent(RuntimeEvent),
    /// Final canonical conversation state captured after one host-owned controller run finishes.
    /// 在一次宿主自有控制器运行结束后捕获的最终规范会话状态。
    FinalConversationState(SessionConversationState),
}

/// Streams one real provider run and yields preparation, thinking, output, and final-report events incrementally.
/// 以增量方式流式执行一次真实 Provider 调用，并依次产出准备、思考、输出与最终报告事件。
pub fn stream_runtime_run(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: ExecutionPreparationRequest,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let foundation = RuntimeFoundation::build(
            &config,
            &agent_profile_catalog,
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
        );
        let mut preparation = match prepare_execution(
            &foundation,
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
            &config,
            &request,
        ) {
            Ok(preparation) => preparation,
            Err(error) => {
                let report = ExecutionRunReport::failure(
                    request.session_id,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    error,
                );
                yield ExecutionStreamEvent::Finished(report);
                return;
            }
        };

        let active_agent = foundation.active_agent_definition().cloned();
        let active_agent_tool_id = active_agent
            .as_ref()
            .map(|agent| agent.id.clone())
            .unwrap_or_else(|| preparation.agent_id.clone());
        let tools_manager = build_vulcan_tools_manager(
            foundation.native_tool_catalog().clone(),
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
            (!preparation.workspace_path.is_empty())
                .then(|| PathBuf::from(preparation.workspace_path.clone())),
            foundation.resolved_native_tools_for_agent(&active_agent_tool_id),
            foundation.resolved_luaskills_tools_for_agent(&active_agent_tool_id),
            foundation.resolved_mcp_tools_for_agent(&active_agent_tool_id),
        );
        let resolved_tools = tools_manager.resolve_for_execution_mode(&preparation.execution_mode);
        let mut runtime_events = Vec::new();
        let mut warnings = preparation.warnings.clone();
        let mut thinking_raw_output = String::new();
        let mut thinking_visible_output = String::new();
        let mut assistant_output = String::new();
        let mut thinking_completed_emitted = false;

        let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
            collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
        warnings.extend(discovery_warnings);

        let system_preamble = build_runtime_system_preamble(&preparation);
        let request_history = conversation_messages_to_rig_history(&request.conversation_state.messages);
        let core_snapshot = build_core_conversation_snapshot(&request.conversation_state);
        let mut core_controller = VulcanCoreController::new(
            build_core_controller_config(
                &preparation,
                &system_preamble,
                resolve_runtime_max_turns(active_agent.as_ref()),
                resolve_provider_additional_params(&preparation.model_blueprint),
            ),
            core_snapshot.clone(),
        );
        match build_core_provider_request_snapshot(
            &preparation,
            &request.conversation_state,
            &system_preamble,
            request.prompt.trim(),
            &resolved_tools.definitions,
            resolve_runtime_max_turns(active_agent.as_ref()),
        )
        .await
        {
            Ok(request_snapshot) => {
                preparation.provider_request_preview_markdown =
                    build_provider_request_preview_markdown(&preparation, &request_snapshot);
                preparation.provider_request_snapshot_json =
                    serde_json::to_value(&request_snapshot).ok();
            }
            Err(error) => {
                preparation.provider_request_preview_markdown = format!(
                    "# 最后一次真实 API 请求快照\n\n请求快照构建失败：{}",
                    error
                );
                preparation.provider_request_snapshot_json = None;
                warnings.push(format!("请求快照构建失败：{error}"));
            }
        }

        yield ExecutionStreamEvent::Prepared(preparation.clone());

        let context_event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "执行上下文",
            format!(
                "已进入真实执行阶段，目标模型为 `{}`，工作区为 `{}`。",
                preparation.model_blueprint.model_name, preparation.workspace_path
            ),
        );
        runtime_events.push(context_event.clone());
        yield ExecutionStreamEvent::RuntimeEvent {
            session_id: request.session_id,
            event: context_event,
        };

        for event in discovery_events {
            runtime_events.push(event.clone());
            yield ExecutionStreamEvent::RuntimeEvent {
                session_id: request.session_id,
                event,
            };
        }

        let mut provider_stream = execute_provider_prompt_stream(
            &config,
            &preparation,
            active_agent.as_ref(),
            &system_preamble,
            request.prompt.trim(),
            request_history,
            core_snapshot,
            tools_manager,
            resolved_tools.definitions,
        );

        let mut terminal_error = None;
        let mut final_conversation_state = None;
        let mut latest_request_snapshot = None;

        while let Some(item) = provider_stream.next().await {
            match item {
                Ok(ProviderStreamDelta::RequestSnapshot(snapshot)) => {
                    latest_request_snapshot = Some(snapshot);
                }
                Ok(ProviderStreamDelta::Thinking(delta)) => {
                    core_controller.record_thinking(delta.clone());
                    thinking_raw_output.push_str(&delta);
                    let normalized_visible_output =
                        normalize_streamed_thinking_output(&thinking_raw_output);
                    let visible_delta = normalized_visible_output
                        .strip_prefix(&thinking_visible_output)
                        .unwrap_or(normalized_visible_output.as_str())
                        .to_string();
                    thinking_visible_output = normalized_visible_output;

                    if !visible_delta.is_empty() {
                        yield ExecutionStreamEvent::ThinkingDelta {
                            session_id: request.session_id,
                            delta: visible_delta,
                        };
                    }
                }
                Ok(ProviderStreamDelta::Output(delta)) => {
                    core_controller.record_output(delta.clone());
                    if let Some(thinking_completed_event) =
                        take_current_thinking_phase_completed_event(
                            request.session_id,
                            &thinking_visible_output,
                            &mut thinking_completed_emitted,
                        )
                    {
                        yield thinking_completed_event;
                    }
                    assistant_output.push_str(&delta);
                    yield ExecutionStreamEvent::OutputDelta {
                        session_id: request.session_id,
                        delta,
                    };
                }
                Ok(ProviderStreamDelta::RuntimeEvent(event)) => {
                    if is_provider_tool_boundary_runtime_event(&event) {
                        if let Some(thinking_completed_event) =
                            take_current_thinking_phase_completed_event(
                                request.session_id,
                                &thinking_visible_output,
                                &mut thinking_completed_emitted,
                            )
                        {
                            yield thinking_completed_event;
                        }
                            reset_current_thinking_phase_tracking(
                                &mut thinking_raw_output,
                                &mut thinking_visible_output,
                                &mut thinking_completed_emitted,
                            );
                        }
                    if let Some(core_event) = runtime_event_to_core_event(&event) {
                        core_controller.push_event(core_event);
                    }
                    runtime_events.push(event.clone());
                    yield ExecutionStreamEvent::RuntimeEvent {
                        session_id: request.session_id,
                        event,
                    };
                }
                Ok(ProviderStreamDelta::FinalConversationState(conversation_state)) => {
                    final_conversation_state = Some(conversation_state);
                }
                Err(error) => {
                    terminal_error = Some(error);
                    break;
                }
            }
        }

        if let Some(error) = terminal_error {
            if let Some(thinking_completed_event) = take_current_thinking_phase_completed_event(
                request.session_id,
                &thinking_visible_output,
                &mut thinking_completed_emitted,
            ) {
                yield thinking_completed_event;
            }
            if let Some(request_snapshot) = latest_request_snapshot {
                preparation.provider_request_preview_markdown =
                    build_provider_request_preview_markdown(&preparation, &request_snapshot);
                preparation.provider_request_snapshot_json =
                    serde_json::to_value(&request_snapshot).ok();
            }
            core_controller.mark_failed(error.clone());
            let failed_event = RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "真实执行失败",
                error.clone(),
            );
            runtime_events.push(failed_event.clone());
            yield ExecutionStreamEvent::RuntimeEvent {
                session_id: request.session_id,
                event: failed_event,
            };
            let report = ExecutionRunReport::failure(
                request.session_id,
                Some(preparation),
                discovery_rows,
                discovery_errors,
                warnings,
                core_controller.events().to_vec(),
                error,
            );
            yield ExecutionStreamEvent::Finished(report);
            return;
        }

        if let Some(thinking_completed_event) = take_current_thinking_phase_completed_event(
            request.session_id,
            &thinking_visible_output,
            &mut thinking_completed_emitted,
        ) {
            yield thinking_completed_event;
        }

        if let Some(request_snapshot) = latest_request_snapshot {
            preparation.provider_request_preview_markdown =
                build_provider_request_preview_markdown(&preparation, &request_snapshot);
            preparation.provider_request_snapshot_json =
                serde_json::to_value(&request_snapshot).ok();
        }

        let normalized_output = normalize_assistant_output(assistant_output);
        let output_completed_event = RuntimeEvent::new(
            RuntimeEventKind::TokenDelta,
            "模型输出完成",
            format!(
                "真实 Provider 已返回 {} 个字符的正式输出内容。",
                normalized_output.chars().count()
            ),
        );
        runtime_events.push(output_completed_event.clone());
        yield ExecutionStreamEvent::RuntimeEvent {
            session_id: request.session_id,
            event: output_completed_event,
        };

        let report = ExecutionRunReport::success(
            request.session_id,
            preparation,
            normalize_optional_stream_output(thinking_visible_output),
            normalized_output,
            final_conversation_state,
            discovery_rows,
            discovery_errors,
            warnings,
            core_controller.events().to_vec(),
        );
        yield ExecutionStreamEvent::Finished(report);
    }
}

/// Collects enabled MCP runtime state and converts the latest snapshot into UI-facing runtime events.
/// 收集已启用 MCP 运行时状态，并将最新快照转换为面向界面的运行时事件。
fn collect_enabled_mcp_runtime_events(
    mcp_runtime: Option<&McpRuntimeHandle>,
) -> (Vec<RuntimeEvent>, Vec<String>, Vec<String>, Vec<String>) {
    let mut runtime_events = Vec::new();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let Some(runtime) = mcp_runtime else {
        return (runtime_events, rows, errors, warnings);
    };
    let snapshot = runtime.snapshot();

    if snapshot.servers.is_empty() {
        return (runtime_events, rows, errors, warnings);
    }

    for server in snapshot.servers.values().filter(|server| server.enabled) {
        let summary = format!(
            "服务器 `{}` 当前状态为 `{}`，已注册 {} 个工具。",
            server.name,
            server.status.label(),
            server.tools.len()
        );
        rows.push(summary.clone());
        match &server.last_error {
            Some(error) => {
                let combined = format!("{summary} 最近一次错误：{error}");
                warnings.push(combined.clone());
                errors.push(combined.clone());
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    format!("MCP 服务器离线 · {}", server.name),
                    combined,
                ));
            }
            None => {
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::ToolFinished,
                    format!("MCP 服务器在线 · {}", server.name),
                    summary,
                ));
            }
        }
    }

    (runtime_events, rows, errors, warnings)
}

/// Builds the effective runtime system prompt sent to the real provider agent.
/// 构建发送给真实 Provider 代理的生效运行时系统提示词。
fn build_runtime_system_preamble(preparation: &ExecutionPreparation) -> String {
    let mut sections = Vec::new();
    sections.push(
        load_provider_base_system_prompt(&preparation.model_blueprint)
            .unwrap_or_else(|| String::from("You are VulcanCode, an AI coding agent.")),
    );
    sections.push(format!("当前工作区：{}", preparation.workspace_path));
    sections.push(format!("当前执行模式：{}", preparation.execution_mode));

    sections.join("\n")
}

/// Builds one `vulcan-core` request snapshot from the current execution preparation and canonical session state.
/// 根据当前执行准备信息与规范会话状态构建一份 `vulcan-core` 请求快照。
async fn build_core_provider_request_snapshot(
    preparation: &ExecutionPreparation,
    conversation_state: &SessionConversationState,
    system_preamble: &str,
    prompt: &str,
    resolved_tools: &[ResolvedToolDefinition],
    max_turns_override: Option<usize>,
) -> Result<CoreProviderRequestSnapshot, String> {
    let request_headers = build_provider_request_headers(&preparation.model_blueprint)?;
    let serialized_headers = serialize_provider_request_headers(&request_headers);
    let additional_params = resolve_provider_additional_params(&preparation.model_blueprint);
    let tool_schemas = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_core_tool_schema)
        .collect::<Vec<_>>();
    let mut controller = VulcanCoreController::new(
        build_core_controller_config(
            preparation,
            system_preamble,
            max_turns_override,
            additional_params.clone(),
        ),
        build_core_conversation_snapshot(conversation_state),
    );
    let adapter = PreviewSnapshotProviderAdapter {
        provider_id: preparation.model_blueprint.provider_id.clone(),
        model_id: preparation.model_blueprint.model_id.clone(),
        prompt: prompt.to_string(),
        request_headers: serialized_headers,
        tool_schemas,
    };

    controller
        .build_request(&adapter)
        .await
        .map_err(|error| error.to_string())
}

/// Builds one `vulcan-core` controller configuration from the current execution preparation.
/// 根据当前执行准备信息构建一份 `vulcan-core` 控制器配置。
fn build_core_controller_config(
    preparation: &ExecutionPreparation,
    system_preamble: &str,
    max_turns_override: Option<usize>,
    additional_params: Option<Value>,
) -> CoreControllerConfig {
    CoreControllerConfig {
        provider_id: preparation.model_blueprint.provider_id.clone(),
        model_id: preparation.model_blueprint.model_id.clone(),
        system_prompt: system_preamble.to_string(),
        temperature: preparation.temperature.map(f64::from),
        max_turns: max_turns_override.unwrap_or(DEFAULT_RUNTIME_MAX_TURNS_LIMIT),
        additional_params,
    }
}

/// Converts one session-owned canonical conversation state into the `vulcan-core` snapshot shape.
/// 将一份会话持有的规范会话状态转换为 `vulcan-core` 快照形状。
fn build_core_conversation_snapshot(
    conversation_state: &SessionConversationState,
) -> CoreConversationSnapshot {
    CoreConversationSnapshot {
        messages: conversation_state
            .messages
            .iter()
            .map(session_message_to_core_message)
            .collect(),
        records: conversation_state
            .records
            .iter()
            .map(session_record_to_core_record)
            .collect(),
        continuation: session_continuation_to_core_continuation(&conversation_state.continuation),
    }
}

/// Converts one session-owned canonical conversation message into the `vulcan-core` message shape.
/// 将一条会话持有的规范会话消息转换为 `vulcan-core` 消息形状。
fn session_message_to_core_message(
    message: &SessionConversationMessage,
) -> CoreConversationMessage {
    let payload = match &message.payload {
        SessionConversationPayload::UserText { text } => {
            CoreConversationPayload::UserText { text: text.clone() }
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
        } => CoreConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            tool_calls: tool_calls
                .iter()
                .map(session_tool_call_to_core_tool_call)
                .collect(),
        },
        SessionConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => CoreConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one session-owned canonical tool-call entry into the `vulcan-core` tool-call shape.
/// 将一条会话持有的规范工具调用项转换为 `vulcan-core` 工具调用形状。
fn session_tool_call_to_core_tool_call(
    tool_call: &crate::conversation_protocol::SessionConversationToolCall,
) -> CoreConversationToolCall {
    CoreConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

/// Converts one session-owned provider continuation snapshot into the `vulcan-core` continuation shape.
/// 将一份会话持有的 Provider continuation 快照转换为 `vulcan-core` continuation 形状。
fn session_continuation_to_core_continuation(
    continuation: &crate::conversation_protocol::SessionProviderContinuationState,
) -> CoreProviderContinuation {
    CoreProviderContinuation {
        provider_id: continuation.provider_id.clone(),
        model_id: continuation.model_id.clone(),
        last_provider_message_id: continuation.last_provider_message_id.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Converts one session-owned durable record into the `vulcan-core` record shape so pre-existing transcript records survive controller round-trips.
/// 将一条会话持有的持久化记录转换为 `vulcan-core` 记录形状，从而让既有转录记录在控制器往返过程中得以保留。
fn session_record_to_core_record(record: &SessionConversationRecord) -> CoreConversationRecord {
    CoreConversationRecord {
        id: record.id.clone(),
        role: session_record_to_core_role(record),
        record_kind: session_record_to_core_record_kind(record.record_kind),
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Derives the `vulcan-core` record role from one GUI-owned durable record.
/// 根据一条 GUI 持有的持久化记录推导 `vulcan-core` 记录角色。
fn session_record_to_core_role(record: &SessionConversationRecord) -> CoreTranscriptRole {
    match record.record_kind {
        crate::model::SessionMessageRecordKind::ThinkingOutput => CoreTranscriptRole::Thinking,
        crate::model::SessionMessageRecordKind::ToolCallOutput
        | crate::model::SessionMessageRecordKind::ToolResultOutput => CoreTranscriptRole::Tool,
        crate::model::SessionMessageRecordKind::SummaryOutput
        | crate::model::SessionMessageRecordKind::CompactionSummary => CoreTranscriptRole::Summary,
        crate::model::SessionMessageRecordKind::ExploreOutput => CoreTranscriptRole::Explore,
        _ => match record.kind {
            crate::model::SessionMessageKind::User => CoreTranscriptRole::User,
            crate::model::SessionMessageKind::Assistant => CoreTranscriptRole::Assistant,
            crate::model::SessionMessageKind::Error => CoreTranscriptRole::System,
        },
    }
}

/// Converts one GUI-owned durable record kind into the `vulcan-core` record-kind classification.
/// 将一种 GUI 持有的持久化记录类型转换为 `vulcan-core` 记录类型分类。
fn session_record_to_core_record_kind(
    record_kind: crate::model::SessionMessageRecordKind,
) -> CoreTranscriptRecordKind {
    match record_kind {
        crate::model::SessionMessageRecordKind::Transcript
        | crate::model::SessionMessageRecordKind::AssistantOutput
        | crate::model::SessionMessageRecordKind::ToolCallOutput
        | crate::model::SessionMessageRecordKind::ToolResultOutput => {
            CoreTranscriptRecordKind::Visible
        }
        crate::model::SessionMessageRecordKind::ThinkingOutput
        | crate::model::SessionMessageRecordKind::TitleOutput
        | crate::model::SessionMessageRecordKind::SummaryOutput => CoreTranscriptRecordKind::Hidden,
        crate::model::SessionMessageRecordKind::ExploreOutput => {
            CoreTranscriptRecordKind::ProjectionOnly
        }
        crate::model::SessionMessageRecordKind::CompactionSummary => {
            CoreTranscriptRecordKind::Checkpoint
        }
    }
}

/// Converts one resolved host tool definition into the `vulcan-core` request-tool schema shape.
/// 将一条已解析宿主工具定义转换为 `vulcan-core` 请求工具 schema 形状。
fn resolved_tool_definition_to_core_tool_schema(
    definition: &ResolvedToolDefinition,
) -> CoreProviderRequestToolSchema {
    CoreProviderRequestToolSchema {
        id: definition.descriptor.id.clone(),
        name: definition.descriptor.name.clone(),
        description: definition.descriptor.description.clone(),
        source_label: tool_source_label(definition.descriptor.source).to_string(),
        parameters_schema: definition.parameters_schema.clone(),
    }
}

/// Converts one resolved host tool definition into the low-level rig tool-definition shape used by explicit completion requests.
/// 将一条已解析宿主工具定义转换为显式 completion 请求使用的低层 rig 工具定义形状。
fn resolved_tool_definition_to_rig_tool_definition(
    definition: &ResolvedToolDefinition,
) -> RigToolDefinition {
    RigToolDefinition {
        name: definition.descriptor.id.clone(),
        description: definition.descriptor.description.clone(),
        parameters: definition.parameters_schema.clone(),
    }
}

/// Converts canonical core messages into the provider-visible history that will actually be sent upstream.
/// 将规范 core 消息转换为真正会发送给上游 Provider 的可见历史。
fn core_messages_to_provider_visible_history(
    messages: &[CoreConversationMessage],
) -> Vec<CoreConversationMessage> {
    messages
        .iter()
        .map(|message| {
            let payload = match &message.payload {
                CoreConversationPayload::UserText { text } => {
                    CoreConversationPayload::UserText { text: text.clone() }
                }
                CoreConversationPayload::Assistant {
                    text,
                    reasoning: _,
                    provider_message_id,
                    tool_calls,
                } => CoreConversationPayload::Assistant {
                    text: text.clone(),
                    reasoning: String::new(),
                    provider_message_id: provider_message_id.clone(),
                    tool_calls: tool_calls.clone(),
                },
                CoreConversationPayload::ToolResult {
                    tool_result_id,
                    tool_call_id,
                    text,
                } => CoreConversationPayload::ToolResult {
                    tool_result_id: tool_result_id.clone(),
                    tool_call_id: tool_call_id.clone(),
                    text: text.clone(),
                },
            };

            CoreConversationMessage {
                id: message.id.clone(),
                payload,
            }
        })
        .collect()
}

/// Converts one `vulcan-core` snapshot back into the GUI-owned canonical session state without dropping durable records or continuation data.
/// 将一份 `vulcan-core` 快照转换回 GUI 持有的规范会话状态，同时不丢失持久化记录与 continuation 数据。
fn core_snapshot_to_session_conversation_state(
    snapshot: &CoreConversationSnapshot,
) -> SessionConversationState {
    SessionConversationState {
        messages: snapshot
            .messages
            .iter()
            .map(core_message_to_session_message)
            .collect(),
        records: snapshot
            .records
            .iter()
            .map(core_record_to_session_record)
            .collect(),
        continuation: core_continuation_to_session_continuation(&snapshot.continuation),
    }
}

/// Converts one `vulcan-core` message back into the GUI-owned canonical session message shape.
/// 将一条 `vulcan-core` 消息转换回 GUI 持有的规范会话消息形状。
fn core_message_to_session_message(
    message: &CoreConversationMessage,
) -> SessionConversationMessage {
    let payload = match &message.payload {
        CoreConversationPayload::UserText { text } => {
            SessionConversationPayload::UserText { text: text.clone() }
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
        } => SessionConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            tool_calls: tool_calls
                .iter()
                .map(core_tool_call_to_session_tool_call)
                .collect(),
        },
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => SessionConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    SessionConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one `vulcan-core` tool-call entry back into the GUI-owned canonical tool-call shape.
/// 将一条 `vulcan-core` 工具调用项转换回 GUI 持有的规范工具调用形状。
fn core_tool_call_to_session_tool_call(
    tool_call: &CoreConversationToolCall,
) -> SessionConversationToolCall {
    SessionConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

/// Converts one `vulcan-core` durable record back into the GUI-owned record shape while preserving display metadata.
/// 将一条 `vulcan-core` 持久化记录转换回 GUI 持有的记录形状，同时保留显示元数据。
fn core_record_to_session_record(record: &CoreConversationRecord) -> SessionConversationRecord {
    SessionConversationRecord {
        id: record.id.clone(),
        kind: core_record_to_session_message_kind(record),
        record_kind: core_record_to_session_record_kind(record),
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Converts one `vulcan-core` continuation snapshot back into the GUI-owned continuation shape.
/// 将一份 `vulcan-core` continuation 快照转换回 GUI 持有的 continuation 形状。
fn core_continuation_to_session_continuation(
    continuation: &CoreProviderContinuation,
) -> SessionProviderContinuationState {
    SessionProviderContinuationState {
        provider_id: continuation.provider_id.clone(),
        model_id: continuation.model_id.clone(),
        last_provider_message_id: continuation.last_provider_message_id.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Derives the GUI transcript message kind from one `vulcan-core` durable record role.
/// 根据一条 `vulcan-core` 持久化记录角色推导 GUI 转录消息类型。
fn core_record_to_session_message_kind(
    record: &CoreConversationRecord,
) -> crate::model::SessionMessageKind {
    match record.role {
        CoreTranscriptRole::User => crate::model::SessionMessageKind::User,
        _ => crate::model::SessionMessageKind::Assistant,
    }
}

/// Derives the GUI record kind from one `vulcan-core` durable record while preserving the semantic difference between tool calls, tool results, and hidden nodes.
/// 根据一条 `vulcan-core` 持久化记录推导 GUI 记录类型，同时保留工具调用、工具结果与隐藏节点之间的语义差异。
fn core_record_to_session_record_kind(
    record: &CoreConversationRecord,
) -> crate::model::SessionMessageRecordKind {
    match (&record.role, &record.record_kind) {
        (CoreTranscriptRole::Thinking, _) => crate::model::SessionMessageRecordKind::ThinkingOutput,
        (CoreTranscriptRole::Summary, _) | (_, CoreTranscriptRecordKind::Checkpoint) => {
            crate::model::SessionMessageRecordKind::CompactionSummary
        }
        (CoreTranscriptRole::Explore, _) | (_, CoreTranscriptRecordKind::ProjectionOnly) => {
            crate::model::SessionMessageRecordKind::ExploreOutput
        }
        (CoreTranscriptRole::Tool, _) if core_record_is_tool_result(record) => {
            crate::model::SessionMessageRecordKind::ToolResultOutput
        }
        (CoreTranscriptRole::Tool, _) => crate::model::SessionMessageRecordKind::ToolCallOutput,
        (CoreTranscriptRole::Assistant, _) if record.visible => {
            crate::model::SessionMessageRecordKind::AssistantOutput
        }
        (CoreTranscriptRole::Assistant, _) => crate::model::SessionMessageRecordKind::SummaryOutput,
        _ => crate::model::SessionMessageRecordKind::Transcript,
    }
}

/// Returns whether one `vulcan-core` tool record semantically represents a tool result rather than the original tool call request.
/// 返回一条 `vulcan-core` 工具记录在语义上是否表示工具结果，而不是原始工具调用请求。
fn core_record_is_tool_result(record: &CoreConversationRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(Value::as_object)
        .map(|object| object.contains_key("is_error") || object.contains_key("kind"))
        .unwrap_or(false)
}

/// Converts one `vulcan-core` canonical message slice into rig history without replaying reasoning text back to the upstream provider.
/// 将一段 `vulcan-core` 规范消息切片转换为 rig 历史，同时避免把思考文本反向回放给上游 Provider。
fn core_messages_to_rig_history(messages: &[CoreConversationMessage]) -> Vec<RigMessage> {
    messages.iter().map(core_message_to_rig_message).collect()
}

/// Converts one `vulcan-core` canonical message into one rig message visible to the upstream provider.
/// 将一条 `vulcan-core` 规范消息转换为上游 Provider 可见的 rig 消息。
fn core_message_to_rig_message(message: &CoreConversationMessage) -> RigMessage {
    match &message.payload {
        CoreConversationPayload::UserText { text } => RigMessage::user(text.clone()),
        CoreConversationPayload::Assistant {
            text,
            reasoning: _,
            provider_message_id,
            tool_calls,
        } => {
            let mut contents = Vec::new();
            if !text.is_empty() {
                contents.push(RigAssistantContent::text(text.clone()));
            }
            contents.extend(
                tool_calls
                    .iter()
                    .map(core_tool_call_to_rig_assistant_content),
            );
            let content = OneOrMany::many(contents)
                .unwrap_or_else(|_| OneOrMany::one(RigAssistantContent::text(String::new())));
            RigMessage::Assistant {
                id: provider_message_id.clone(),
                content,
            }
        }
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => RigMessage::tool_result_with_call_id(
            tool_result_id.clone(),
            tool_call_id.clone(),
            text.clone(),
        ),
    }
}

/// Converts one `vulcan-core` tool-call entry into rig assistant content that keeps both the provider-emitted item identifier and the optional call identifier.
/// 将一条 `vulcan-core` 工具调用项转换为 rig 助手内容，同时保留 Provider 产出的项标识与可选调用标识。
fn core_tool_call_to_rig_assistant_content(
    tool_call: &CoreConversationToolCall,
) -> RigAssistantContent {
    match &tool_call.tool_call_id {
        Some(call_id) => RigAssistantContent::tool_call_with_call_id(
            tool_call.id.clone(),
            call_id.clone(),
            tool_call.tool_name.clone(),
            tool_call.arguments.clone(),
        ),
        None => RigAssistantContent::tool_call(
            tool_call.id.clone(),
            tool_call.tool_name.clone(),
            tool_call.arguments.clone(),
        ),
    }
}

/// Resolves one provider blueprint into the JSON payload passed through `additional_params`.
/// 将一份 Provider 蓝图解析为通过 `additional_params` 传递的 JSON 载荷。
fn resolve_provider_additional_params(blueprint: &ProviderRuntimeBlueprint) -> Option<Value> {
    blueprint
        .extra_body
        .as_object()
        .filter(|object| !object.is_empty())
        .map(|object| Value::Object(object.clone()))
}

/// Serializes one request-header map into plain JSON for request-snapshot persistence and inspection.
/// 将一份请求头映射序列化为普通 JSON，供请求快照持久化与检查使用。
fn serialize_provider_request_headers(headers: &HeaderMap) -> Option<Value> {
    let mut object = JsonMap::new();
    for (name, value) in headers {
        if let Ok(value_text) = value.to_str() {
            object.insert(
                name.as_str().to_string(),
                Value::String(value_text.to_string()),
            );
        }
    }

    (!object.is_empty()).then_some(Value::Object(object))
}

/// Snapshot-only provider adapter used to let `vulcan-core` own request construction before real transport execution is migrated.
/// 在真实传输执行迁移完成前，仅供快照使用的 Provider 适配器，用于让 `vulcan-core` 持有请求构造权。
#[derive(Debug, Clone)]
struct PreviewSnapshotProviderAdapter {
    /// Stable provider identifier used by the snapshot adapter.
    /// 快照适配器使用的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier used by the snapshot adapter.
    /// 快照适配器使用的稳定模型标识。
    model_id: String,
    /// Latest outgoing prompt body attached to the snapshot request.
    /// 附着在快照请求中的最新出站提示词正文。
    prompt: String,
    /// Serialized request headers captured from the active provider blueprint.
    /// 从当前 Provider 蓝图捕获的序列化请求头。
    request_headers: Option<Value>,
    /// Tool schemas exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的工具 schema 集合。
    tool_schemas: Vec<CoreProviderRequestToolSchema>,
}

#[async_trait]
impl VulcanProviderAdapter for PreviewSnapshotProviderAdapter {
    type Request = CoreProviderRequestSnapshot;
    type Response = ();

    /// Returns the stable provider identifier used by the snapshot adapter.
    /// 返回快照适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the snapshot adapter.
    /// 返回快照适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one structured request snapshot owned by `vulcan-core`.
    /// 构建一份由 `vulcan-core` 持有的结构化请求快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        Ok(CoreProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt: self.prompt.clone(),
            history: core_messages_to_provider_visible_history(&snapshot.messages),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: self.tool_schemas.clone(),
            additional_params: config.additional_params.clone(),
            request_headers: self.request_headers.clone(),
            continuation: snapshot.continuation.clone(),
        })
    }

    /// Snapshot adapters do not perform real network execution and therefore always return a no-op response.
    /// 快照适配器不会执行真实网络调用，因此始终返回一次空操作响应。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Ok(())
    }
}

/// Loads one provider-family base system prompt from the editable resource directory.
/// 从可编辑资源目录中加载对应 Provider 家族的基础系统提示词。
fn load_provider_base_system_prompt(blueprint: &ProviderRuntimeBlueprint) -> Option<String> {
    let file_name = resolve_provider_base_prompt_file_name(blueprint);
    let packaged_prompt_path = runtime_assets::resolve_app_resources_directory()
        .join(SYSTEM_PROMPTS_DIR_NAME)
        .join(file_name);
    if let Some(prompt) = read_non_empty_system_prompt_file(&packaged_prompt_path) {
        return Some(prompt);
    }

    let source_prompt_path = runtime_assets::source_resource_directory()
        .join(SYSTEM_PROMPTS_DIR_NAME)
        .join(file_name);
    if source_prompt_path != packaged_prompt_path {
        if let Some(prompt) = read_non_empty_system_prompt_file(&source_prompt_path) {
            return Some(prompt);
        }
    }

    if file_name != DEFAULT_SYSTEM_PROMPT_FILE_NAME {
        let packaged_default_path = runtime_assets::resolve_app_resources_directory()
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
        if let Some(prompt) = read_non_empty_system_prompt_file(&packaged_default_path) {
            return Some(prompt);
        }

        let source_default_path = runtime_assets::source_resource_directory()
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
        if source_default_path != packaged_default_path {
            if let Some(prompt) = read_non_empty_system_prompt_file(&source_default_path) {
                return Some(prompt);
            }
        }
    }

    None
}

/// Resolves one provider/model combination into the matching base prompt file name.
/// 将单个 Provider/模型组合解析为对应的基础提示词文件名。
fn resolve_provider_base_prompt_file_name(blueprint: &ProviderRuntimeBlueprint) -> &'static str {
    let model_id = blueprint.model_id.to_ascii_lowercase();

    if model_id.contains("gpt-4") || model_id.contains("o1") || model_id.contains("o3") {
        return BEAST_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("gpt") {
        if model_id.contains("codex") {
            return CODEX_SYSTEM_PROMPT_FILE_NAME;
        }
        return GPT_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("gemini-") {
        return GEMINI_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("claude") {
        return ANTHROPIC_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("trinity") {
        return TRINITY_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("kimi") {
        return KIMI_SYSTEM_PROMPT_FILE_NAME;
    }
    DEFAULT_SYSTEM_PROMPT_FILE_NAME
}

/// Reads one prompt text file and trims only the outer blank region.
/// 读取单个提示词文本文件，并仅裁剪外围空白区域。
fn read_non_empty_system_prompt_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

/// Builds the full Markdown preview of the effective provider request captured right before dispatch.
/// 构建在真正分发前捕获的生效 Provider 请求完整 Markdown 预览。
fn build_provider_request_preview_markdown(
    preparation: &ExecutionPreparation,
    request_snapshot: &CoreProviderRequestSnapshot,
) -> String {
    let temperature_label = request_snapshot
        .temperature
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| String::from("使用模型默认值"));
    let max_turns_label = if request_snapshot.max_turns == DEFAULT_RUNTIME_MAX_TURNS_LIMIT {
        String::from("不限制")
    } else {
        format!("`{}`", request_snapshot.max_turns)
    };
    let extra_body_markdown = request_snapshot
        .additional_params
        .as_ref()
        .map(render_json_code_fence)
        .unwrap_or_else(|| String::from("（无附加 Body 参数）"));
    let request_headers_markdown = request_snapshot
        .request_headers
        .as_ref()
        .map(render_json_code_fence)
        .unwrap_or_else(|| String::from("（无附加请求头）"));
    let continuation_markdown =
        render_provider_continuation_markdown(&request_snapshot.continuation);
    let tool_rows = if request_snapshot.tool_schemas.is_empty() {
        String::from("（本次请求未直接暴露工具 schema）")
    } else {
        request_snapshot
            .tool_schemas
            .iter()
            .map(render_core_tool_request_section)
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let outgoing_history_markdown =
        render_core_conversation_history_markdown(&request_snapshot.history);

    format!(
        concat!(
            "# 最后一次真实 API 请求快照\n\n",
            "## 路由摘要\n",
            "- 会话：`#{}` · {}\n",
            "- 代理：`{}` · {} · id=`{}`\n",
            "- Provider：`{}` · `{}`\n",
            "- 模型：`{}` · `{}`\n",
            "- 运行时：`{}` · kind=`{}`\n",
            "- 接口：`{}`\n",
            "- 执行模式：`{}`\n",
            "- 温度：{}\n",
            "- 最大轮次：{}\n",
            "- 历史上下文：`{}` 条\n",
            "- 工具数量：`{}` 个\n\n",
            "## System Preamble\n",
            "{}\n\n",
            "## Continuation\n",
            "{}\n\n",
            "## Outgoing History\n",
            "{}\n\n",
            "## Effective Outgoing Prompt\n",
            "{}\n\n",
            "## Tool Schemas\n",
            "{}\n\n",
            "## Request Headers\n",
            "{}\n\n",
            "## Additional Body Parameters\n",
            "{}\n"
        ),
        preparation.session_id,
        preparation.session_title,
        preparation.agent_name,
        preparation.agent_kind,
        preparation.agent_id,
        preparation.model_blueprint.provider_name,
        preparation.model_blueprint.provider_id,
        preparation.model_blueprint.model_name,
        preparation.model_blueprint.model_id,
        preparation.model_blueprint.runtime_label,
        preparation.model_blueprint.runtime_kind_key(),
        preparation.model_blueprint.endpoint_summary(),
        preparation.execution_mode,
        temperature_label,
        max_turns_label,
        request_snapshot.history.len(),
        request_snapshot.tool_schemas.len(),
        render_text_code_fence(&request_snapshot.system_prompt),
        continuation_markdown,
        outgoing_history_markdown,
        render_text_code_fence(&request_snapshot.prompt),
        tool_rows,
        request_headers_markdown,
        extra_body_markdown,
    )
}

/// Renders the `vulcan-core` canonical history into a structured Markdown snapshot that is easier to inspect than one flattened prompt string.
/// 将 `vulcan-core` 规范历史渲染为结构化 Markdown 快照，使其比单条扁平 Prompt 字符串更易检查。
fn render_core_conversation_history_markdown(history: &[CoreConversationMessage]) -> String {
    if history.is_empty() {
        return String::from("（本次请求没有历史消息）");
    }

    history
        .iter()
        .enumerate()
        .map(|(index, message)| render_core_conversation_message_preview(index, message))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders one `vulcan-core` canonical conversation message into the provider-preview Markdown.
/// 将单条 `vulcan-core` 规范会话消息渲染到 Provider 预览 Markdown 中。
fn render_core_conversation_message_preview(
    index: usize,
    message: &CoreConversationMessage,
) -> String {
    match &message.payload {
        CoreConversationPayload::UserText { text } => format!(
            "### {}. user · `{}`\n{}",
            index + 1,
            message.id,
            render_text_code_fence(text)
        ),
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
        } => {
            let mut sections = vec![format!(
                "### {}. assistant · `{}`{}",
                index + 1,
                message.id,
                provider_message_id
                    .as_deref()
                    .map(|id| format!(" · provider_id=`{id}`"))
                    .unwrap_or_default()
            )];
            if !text.trim().is_empty() {
                sections.push(String::from("#### Text"));
                sections.push(render_text_code_fence(text));
            }
            if !reasoning.trim().is_empty() {
                sections.push(String::from("#### Reasoning"));
                sections.push(render_text_code_fence(reasoning));
            }
            if !tool_calls.is_empty() {
                sections.push(String::from("#### Tool Calls"));
                sections.extend(tool_calls.iter().map(|tool_call| {
                    let arguments = serde_json::to_string_pretty(&tool_call.arguments)
                        .unwrap_or_else(|_| tool_call.arguments.to_string());
                    format!(
                        "- `{}` · id=`{}`{}\n```json\n{}\n```",
                        tool_call.tool_name,
                        tool_call.id,
                        tool_call
                            .tool_call_id
                            .as_deref()
                            .map(|call_id| format!(" · call_id=`{call_id}`"))
                            .unwrap_or_default(),
                        arguments
                    )
                }));
            }
            sections.join("\n\n")
        }
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => format!(
            "### {}. tool_result · `{}`{}\n{}",
            index + 1,
            tool_result_id,
            tool_call_id
                .as_deref()
                .map(|call_id| format!(" · call_id=`{call_id}`"))
                .unwrap_or_default(),
            render_text_code_fence(text)
        ),
    }
}

/// Renders one `vulcan-core` tool schema into a Markdown section used by the request-preview tab.
/// 将单个 `vulcan-core` 工具 schema 渲染为请求预览页签使用的 Markdown 分节。
fn render_core_tool_request_section(definition: &CoreProviderRequestToolSchema) -> String {
    format!(
        concat!(
            "### `{}`\n",
            "- 显示名：{}\n",
            "- 来源：{}\n",
            "- 描述：{}\n",
            "{}"
        ),
        definition.id,
        definition.name,
        definition.source_label,
        definition.description,
        render_json_code_fence(&definition.parameters_schema),
    )
}

/// Renders one provider continuation snapshot into readable Markdown for request inspection.
/// 将一份 Provider continuation 快照渲染为便于请求检查的可读 Markdown。
fn render_provider_continuation_markdown(continuation: &CoreProviderContinuation) -> String {
    if continuation.provider_id.is_none()
        && continuation.model_id.is_none()
        && continuation.last_provider_message_id.is_none()
        && continuation.updated_at.is_none()
    {
        return String::from("（本次请求没有 continuation 游标）");
    }

    let payload = json!({
        "provider_id": continuation.provider_id,
        "model_id": continuation.model_id,
        "last_provider_message_id": continuation.last_provider_message_id,
        "updated_at": continuation.updated_at,
    });
    render_json_code_fence(&payload)
}

/// Renders one tool-source label used by the request-preview tab.
/// 渲染请求预览页签使用的单个工具来源标签。
fn tool_source_label(source: VulcanToolSourceKind) -> &'static str {
    match source {
        VulcanToolSourceKind::Native => "原生工具",
        VulcanToolSourceKind::LuaSkills => "LuaSkills",
        VulcanToolSourceKind::Mcp => "MCP",
    }
}

/// Wraps arbitrary text inside a safe fenced code block for request previews.
/// 将任意文本包装进请求预览使用的安全围栏代码块中。
fn render_text_code_fence(content: &str) -> String {
    format!("````text\n{}\n````", content)
}

/// Wraps one JSON value inside a pretty-printed fenced code block for request previews.
/// 将单个 JSON 值包装进请求预览使用的格式化围栏代码块中。
fn render_json_code_fence(value: &Value) -> String {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    format!("````json\n{}\n````", pretty)
}

/// Boxed provider stream used by the runtime to forward thinking and output deltas incrementally.
/// 供运行时逐步转发思考与输出增量的装箱 Provider 流类型。
type ProviderDeltaStream = Pin<Box<dyn Stream<Item = Result<ProviderStreamDelta, String>> + Send>>;

/// Executes one provider prompt against the currently selected runtime family and returns a delta stream.
/// 针对当前选中的运行时族执行一次 Provider 提示词调用，并返回增量流。
fn execute_provider_prompt_stream(
    config: &ProviderConfigurationFile,
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
) -> ProviderDeltaStream {
    let options = match resolve_provider_options(config, &preparation.model_blueprint.provider_id) {
        Ok(options) => options,
        Err(error) => return one_shot_provider_error_stream(error),
    };
    let api_key = match options.resolved_api_key() {
        Ok(Some(api_key)) => api_key,
        Ok(None) => {
            return one_shot_provider_error_stream(format!(
                "供应商 `{}` 当前缺少可用 API Key，无法启动真实执行。",
                preparation.model_blueprint.provider_name
            ));
        }
        Err(error) => return one_shot_provider_error_stream(error),
    };
    let temperature = active_agent
        .and_then(|agent| agent.temperature)
        .or(preparation.temperature)
        .map(|value| value.clamp(0.0, 2.0) as f64);
    let max_turns = resolve_runtime_default_max_turns_limit(active_agent);
    let model_id = preparation.model_blueprint.model_id.as_str();
    let base_url = preparation.model_blueprint.base_url.as_deref();
    let request_headers = match build_provider_request_headers(&preparation.model_blueprint) {
        Ok(headers) => headers,
        Err(error) => return one_shot_provider_error_stream(error),
    };
    let additional_params = resolve_provider_additional_params(&preparation.model_blueprint);

    match preparation.model_blueprint.runtime_kind {
        ProviderRuntimeKind::OpenAi => execute_openai_responses_stream(
            &preparation.model_blueprint.provider_id,
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            request_history,
            core_snapshot,
            temperature,
            max_turns,
            &request_headers,
            tools_manager,
            preparation.session_id,
            resolved_tools,
            additional_params,
        ),
        ProviderRuntimeKind::OpenAiCompatible
        | ProviderRuntimeKind::XAi
        | ProviderRuntimeKind::Mistral
        | ProviderRuntimeKind::Groq
        | ProviderRuntimeKind::TogetherAi
        | ProviderRuntimeKind::Perplexity => execute_openai_compatible_stream(
            &preparation.model_blueprint.provider_id,
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            request_history,
            core_snapshot,
            temperature,
            max_turns,
            &request_headers,
            tools_manager,
            preparation.session_id,
            resolved_tools,
            additional_params,
        ),
        ProviderRuntimeKind::OpenRouter => execute_openrouter_stream(
            &preparation.model_blueprint.provider_id,
            &api_key,
            model_id,
            system_preamble,
            prompt,
            request_history,
            core_snapshot,
            temperature,
            max_turns,
            &request_headers,
            tools_manager,
            preparation.session_id,
            resolved_tools,
            additional_params,
        ),
        ProviderRuntimeKind::Anthropic => execute_anthropic_stream(
            &preparation.model_blueprint.provider_id,
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            request_history,
            core_snapshot,
            temperature,
            max_turns,
            &request_headers,
            tools_manager,
            preparation.session_id,
            resolved_tools,
            additional_params,
        ),
        ProviderRuntimeKind::AzureOpenAi
        | ProviderRuntimeKind::GoogleGemini
        | ProviderRuntimeKind::Cohere => {
            let runtime_label = preparation.model_blueprint.runtime_label.clone();
            Box::pin(stream! {
                yield Err(format!(
                    "当前阶段尚未接通 `{}` 的真实流式执行链路，请优先选择 OpenAI 兼容接口、百炼 Coding Plan、OpenRouter 或 Anthropic。",
                    runtime_label
                ));
            })
        }
    }
}

/// Resolves one optional max-turn override for multi-turn tool-calling providers.
/// 为支持多轮工具调用的 Provider 解析一份可选的最大轮次覆盖值。
fn resolve_runtime_max_turns(active_agent: Option<&RuntimeAgentDefinition>) -> Option<usize> {
    active_agent
        .and_then(|agent| agent.max_turns)
        .filter(|value| *value > 0)
        .map(|value| value as usize)
}

/// Returns one safe runtime default turn budget when no explicit override is configured.
/// 在未配置显式覆盖时返回一个安全的运行时默认轮次预算。
const DEFAULT_RUNTIME_MAX_TURNS_LIMIT: usize = usize::MAX - 1;

/// Resolves one effective max-turn budget for the rig agent builder.
/// 为 rig 代理构建器解析一个生效中的最大轮次预算。
fn resolve_runtime_default_max_turns_limit(active_agent: Option<&RuntimeAgentDefinition>) -> usize {
    active_agent
        .and_then(|agent| agent.max_turns)
        .filter(|value| *value > 0)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_RUNTIME_MAX_TURNS_LIMIT)
}

/// Executes one host-controlled OpenAI-compatible prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenAI 兼容提示词调用，并流式返回规范化增量。
fn execute_openai_compatible_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();
    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| OpenAiCompatibleCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "OpenAI 兼容",
    )
}

/// Executes one host-controlled provider stream through `vulcan-core`, forwards live deltas, and returns the final canonical state to the GUI layer.
/// 通过 `vulcan-core` 执行一次宿主控制的 Provider 流，转发实时增量，并把最终规范状态返回给图形界面层。
fn execute_host_controlled_stream<A, F>(
    controller_config: CoreControllerConfig,
    core_snapshot: CoreConversationSnapshot,
    prompt: String,
    provider_factory: F,
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    provider_label: &'static str,
) -> ProviderDeltaStream
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Send + Sync + 'static,
    F: FnOnce(ProviderLiveDeltaSender) -> A + Send + 'static,
{
    Box::pin(stream! {
        let (live_sender, mut live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let task_sender = live_sender.clone();
        let mut controller = VulcanCoreController::new(controller_config, core_snapshot);
        let provider_adapter = provider_factory(live_sender.clone());
        let tool_executor = StreamingRuntimeToolExecutor {
            runtime_bridge,
            live_sender: live_sender.clone(),
        };
        let controller_prompt = prompt.clone();
        let controller_task = tokio::spawn(async move {
            let tool_policy = AllowAllToolPolicy;
            match controller
                .run_user_turn_streaming(
                    &provider_adapter,
                    &tool_executor,
                    &tool_policy,
                    controller_prompt,
                )
                .await
            {
                Ok(_) => {
                    let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                        core_snapshot_to_session_conversation_state(controller.snapshot()),
                    )));
                }
                Err(error) => {
                    let _ = task_sender.send(Err(format!(
                        "{}流式执行失败：{}。",
                        provider_label,
                        error
                    )));
                }
            }
        });
        drop(live_sender);

        while let Some(item) = live_receiver.recv().await {
            yield item;
        }

        if let Err(error) = controller_task.await {
            yield Err(format!(
                "{}控制器后台任务失败：{}。",
                provider_label,
                error
            ));
        }
    })
}

/// Shared live-delta sender used to forward request snapshots, streaming chunks, and runtime events from the host-owned controller task.
/// 用于从宿主自有控制器任务转发请求快照、流式分片与运行时事件的共享实时增量发送器。
type ProviderLiveDeltaSender = UnboundedSender<Result<ProviderStreamDelta, String>>;

/// Serializable OpenAI-compatible single-turn request owned by `vulcan-core` while preserving transport-only rig messages outside persistence.
/// 由 `vulcan-core` 持有的可序列化 OpenAI 兼容单回合请求，同时在持久化之外保留仅供传输使用的 rig 消息。
#[derive(Debug, Clone, serde::Serialize)]
struct OpenAiCompatibleCoreTurnRequest {
    /// Structured request snapshot persisted for inspection and debugging.
    /// 被持久化用于检查与调试的结构化请求快照。
    #[serde(flatten)]
    snapshot: CoreProviderRequestSnapshot,
    /// Provider-visible history converted into rig messages for the real transport layer.
    /// 为真实传输层转换出的 Provider 可见 rig 历史消息。
    #[serde(skip_serializing)]
    current_history: Vec<RigMessage>,
    /// Last outgoing prompt message converted into the rig shape expected by the transport layer.
    /// 被转换为传输层所需 rig 形状的最后一条出站提示消息。
    #[serde(skip_serializing)]
    current_prompt: RigMessage,
}

/// Streaming OpenAI-compatible provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenAI 兼容 Provider 适配器。
#[derive(Clone)]
struct OpenAiCompatibleCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level OpenAI-compatible transport.
    /// 被低层 OpenAI 兼容传输层使用的已解析 API Key。
    api_key: String,
    /// Optional base URL override used by the low-level OpenAI-compatible transport.
    /// 被低层 OpenAI 兼容传输层使用的可选基础地址覆盖值。
    base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for OpenAiCompatibleCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the OpenAI-compatible streaming adapter.
    /// 返回 OpenAI 兼容流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the OpenAI-compatible streaming adapter.
    /// 返回 OpenAI 兼容流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "OpenAI 兼容单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            config.additional_params.clone(),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the OpenAI-compatible host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenAI 兼容宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "OpenAI 兼容流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for OpenAiCompatibleCoreStreamingAdapter {
    /// Executes one exact OpenAI-compatible transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 OpenAI 兼容传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let mut builder = openai::CompletionsClient::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone());

        if let Some(base_url) = self
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.base_url(base_url);
        }

        let client = builder
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 OpenAI 兼容客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("OpenAI 兼容流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("OpenAI 兼容流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming OpenRouter provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenRouter Provider 适配器。
#[derive(Clone)]
struct OpenRouterCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level OpenRouter transport.
    /// 被低层 OpenRouter 传输层使用的已解析 API Key。
    api_key: String,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for OpenRouterCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the OpenRouter streaming adapter.
    /// 返回 OpenRouter 流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the OpenRouter streaming adapter.
    /// 返回 OpenRouter 流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "OpenRouter 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            config.additional_params.clone(),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the OpenRouter host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenRouter 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "OpenRouter 流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for OpenRouterCoreStreamingAdapter {
    /// Executes one exact OpenRouter transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 OpenRouter 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let client = openrouter::Client::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone())
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 OpenRouter 客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("OpenRouter 流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("OpenRouter 流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming Anthropic provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 Anthropic Provider 适配器。
#[derive(Clone)]
struct AnthropicCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level Anthropic transport.
    /// 被低层 Anthropic 传输层使用的已解析 API Key。
    api_key: String,
    /// Optional base URL override used by the low-level Anthropic transport.
    /// 被低层 Anthropic 传输层使用的可选基础地址覆盖值。
    base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for AnthropicCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the Anthropic streaming adapter.
    /// 返回 Anthropic 流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the Anthropic streaming adapter.
    /// 返回 Anthropic 流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "Anthropic 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            config.additional_params.clone(),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the Anthropic host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 Anthropic 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "Anthropic 流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for AnthropicCoreStreamingAdapter {
    /// Executes one exact Anthropic transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 Anthropic 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let mut builder = anthropic::Client::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone());

        if let Some(base_url) = self
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.base_url(base_url);
        }

        let client = builder
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 Anthropic 兼容客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("Anthropic 兼容流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("Anthropic 兼容流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming OpenAI Responses provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenAI Responses Provider 适配器。
#[derive(Clone)]
struct OpenAiResponsesCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level OpenAI Responses transport.
    /// 被低层 OpenAI Responses 传输层使用的已解析 API Key。
    api_key: String,
    /// Optional base URL override used by the low-level OpenAI Responses transport.
    /// 被低层 OpenAI Responses 传输层使用的可选基础地址覆盖值。
    base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for OpenAiResponsesCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the OpenAI Responses streaming adapter.
    /// 返回 OpenAI Responses 流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the OpenAI Responses streaming adapter.
    /// 返回 OpenAI Responses 流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot, injects `previous_response_id` when possible, and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，在可行时注入 `previous_response_id`，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "OpenAI Responses 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let effective_additional_params = inject_previous_response_id_into_additional_params(
            config.additional_params.clone(),
            &snapshot.continuation,
        );
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            effective_additional_params,
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the OpenAI Responses host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenAI Responses 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "OpenAI Responses 流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for OpenAiResponsesCoreStreamingAdapter {
    /// Executes one exact OpenAI Responses transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 OpenAI Responses 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let mut builder = openai::Client::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone());

        if let Some(base_url) = self
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.base_url(base_url);
        }

        let client = builder
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 OpenAI Responses 客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("OpenAI Responses 流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("OpenAI Responses 流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming tool executor that forwards host-side tool boundary events live while delegating the real tool invocation to the shared bridge.
/// 在把真实工具调用委派给共享桥接层的同时，实时转发宿主侧工具边界事件的流式工具执行器。
#[derive(Clone)]
struct StreamingRuntimeToolExecutor {
    /// Shared host-managed bridge that owns the real tool dispatch and metadata normalization.
    /// 持有真实工具调度与元数据规范化能力的共享宿主管理桥接层。
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Live sender used to forward tool boundary runtime events into the GUI stream.
    /// 用于把工具边界运行时事件转发进图形界面流中的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanToolExecutor for StreamingRuntimeToolExecutor {
    /// Executes one host-managed tool request, forwards start and finish runtime events, and returns the normalized tool result to the controller.
    /// 执行一次宿主管理工具请求，转发开始与完成运行时事件，并将规范化工具结果返回给控制器。
    async fn execute_tool(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<CoreToolInvocationResult, VulcanCoreError> {
        let _ = self.live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
            RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                format!(
                    "工具调用 · {} · {}",
                    request.tool_call_id, request.tool_name
                ),
                format_tool_call_runtime_detail(&request.tool_name, &request.arguments),
            )
            .with_metadata(Some(json!({
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "arguments": request.arguments,
            }))),
        )));
        let result = self.runtime_bridge.execute_tool(request).await?;
        let tool_result_id = build_host_tool_result_id(&request.tool_call_id);
        let event_metadata = attach_tool_name_to_runtime_metadata(
            build_tool_result_runtime_metadata(
                &request.tool_call_id,
                &tool_result_id,
                result.host_result.as_ref(),
            ),
            &request.tool_name,
        );
        let _ = self.live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
            RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                format!("工具返回 · {}", request.tool_call_id),
                format_host_tool_result_runtime_detail(&tool_result_id, &result.model_output),
            )
            .with_metadata(event_metadata),
        )));
        Ok(result)
    }
}

/// Resolves the host-visible tool-call identifier used by transcript cards and tool-result correlation.
/// 解析供转录卡片与工具结果关联使用的宿主可见工具调用标识。
#[cfg(test)]
fn resolve_host_visible_tool_call_id(tool_call: &rig::message::ToolCall) -> String {
    tool_call
        .call_id
        .clone()
        .unwrap_or_else(|| tool_call.id.clone())
}

/// Builds one synthetic host-owned tool-result identifier that stays stable within the current execution turn.
/// 构建一个在当前执行回合内保持稳定的宿主自有合成工具结果标识。
fn build_host_tool_result_id(tool_call_id: &str) -> String {
    format!("tool-result-{tool_call_id}")
}

/// Builds the exact structured request snapshot for one host-controlled OpenAI-compatible turn right before dispatch.
/// 在真正分发前为单次宿主控制的 OpenAI 兼容回合构建精确结构化请求快照。
fn build_openai_compatible_turn_request_snapshot(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    current_history: &[RigMessage],
    current_prompt: &RigMessage,
    temperature: Option<f64>,
    max_turns: usize,
    resolved_tools: &[ResolvedToolDefinition],
    headers: &HeaderMap,
    additional_params: Option<Value>,
) -> CoreProviderRequestSnapshot {
    let mut conversation_state = SessionConversationState::default();
    replace_messages_from_rig_history(
        &mut conversation_state,
        current_history,
        Some(provider_id),
        Some(model_id),
        current_unix_millis_text(),
    );
    let core_snapshot = build_core_conversation_snapshot(&conversation_state);

    CoreProviderRequestSnapshot {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        system_prompt: system_preamble.to_string(),
        prompt: render_provider_visible_prompt_message(current_prompt),
        history: core_messages_to_provider_visible_history(&core_snapshot.messages),
        temperature,
        max_turns,
        tool_schemas: resolved_tools
            .iter()
            .map(resolved_tool_definition_to_core_tool_schema)
            .collect(),
        additional_params,
        request_headers: serialize_provider_request_headers(headers),
        continuation: core_snapshot.continuation,
    }
}

/// Injects `previous_response_id` into one additional-params payload when the current continuation snapshot already carries a usable upstream response identifier.
/// 当当前 continuation 快照已经携带可用的上游响应标识时，将 `previous_response_id` 注入到附加参数载荷中。
fn inject_previous_response_id_into_additional_params(
    additional_params: Option<Value>,
    continuation: &CoreProviderContinuation,
) -> Option<Value> {
    let Some(previous_response_id) = continuation
        .last_provider_message_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
    else {
        return additional_params;
    };

    let mut object = match additional_params {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = JsonMap::new();
            object.insert(String::from("host_additional_params"), other);
            object
        }
        None => JsonMap::new(),
    };

    object
        .entry(String::from("previous_response_id"))
        .or_insert_with(|| Value::String(previous_response_id));
    Some(Value::Object(object))
}

/// Renders one provider-visible prompt message into plain text so request snapshots can reflect the exact last outgoing message body.
/// 将单条 Provider 可见提示消息渲染为纯文本，使请求快照能够反映精确的最后一条出站消息正文。
fn render_provider_visible_prompt_message(message: &RigMessage) -> String {
    match message {
        RigMessage::System { content } => content.trim().to_string(),
        RigMessage::User { content } => content
            .iter()
            .filter_map(|item| match item {
                RigUserContent::Text(text) => Some(text.text.trim().to_string()),
                RigUserContent::ToolResult(tool_result) => {
                    Some(flatten_rig_tool_result_content(&tool_result.content))
                }
                RigUserContent::Image(_)
                | RigUserContent::Audio(_)
                | RigUserContent::Video(_)
                | RigUserContent::Document(_) => None,
            })
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
        RigMessage::Assistant { content, .. } => content
            .iter()
            .filter_map(|item| match item {
                RigAssistantContent::Text(text) => Some(text.text.trim().to_string()),
                _ => None,
            })
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
    }
}

/// Flattens rig tool-result content into plain text so request snapshots can store the exact outgoing prompt payload safely.
/// 将 rig 工具结果内容压平成纯文本，使请求快照能够安全保存精确的出站提示词载荷。
fn flatten_rig_tool_result_content(content: &rig::OneOrMany<ToolResultContent>) -> String {
    content
        .iter()
        .map(|item| match item {
            ToolResultContent::Text(text) => text.text.trim().to_string(),
            ToolResultContent::Image(image) => {
                serde_json::to_string(image).unwrap_or_else(|_| String::from("{}"))
            }
        })
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Attaches one tool name to runtime-event metadata so display-layer fallbacks can still recover the semantic tool title.
/// 将工具名称附着到运行时事件元数据中，以便显示层在回退路径下仍能恢复语义化工具标题。
fn attach_tool_name_to_runtime_metadata(metadata: Option<Value>, tool_name: &str) -> Option<Value> {
    let mut object = match metadata {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = JsonMap::new();
            object.insert(String::from("host_metadata"), other);
            object
        }
        None => JsonMap::new(),
    };
    object.insert(
        String::from("tool_name"),
        Value::String(tool_name.to_string()),
    );
    Some(Value::Object(object))
}

/// Formats one host-side tool result into the full Markdown payload shown by the inline transcript and modal.
/// 将单次宿主侧工具结果格式化为内联转录与弹窗使用的完整 Markdown 载荷。
fn format_host_tool_result_runtime_detail(tool_result_id: &str, model_output: &str) -> String {
    let normalized_output = if model_output.trim().is_empty() {
        String::from("（空结果）")
    } else {
        model_output.trim().to_string()
    };
    format!("## 返回标识\n\n`{tool_result_id}`\n\n## 工具返回\n\n{normalized_output}")
}

/// Builds one request-header map from the configured strategy and provider-defined extra headers.
/// 根据配置策略与供应商定义的附加头构建一份请求头映射。
fn build_provider_request_headers(
    blueprint: &ProviderRuntimeBlueprint,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    let user_agent = blueprint
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("vulcancode/{}", env!("CARGO_PKG_VERSION")));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&user_agent).map_err(|error| {
            format!(
                "构建供应商 `{}` 的 User-Agent 失败：{}。",
                blueprint.provider_name, error
            )
        })?,
    );

    for (header_name, header_value) in &blueprint.extra_headers {
        let parsed_name =
            HeaderName::from_bytes(header_name.trim().as_bytes()).map_err(|error| {
                format!(
                    "供应商 `{}` 的附加请求头名称 `{}` 无效：{}。",
                    blueprint.provider_name, header_name, error
                )
            })?;
        let parsed_value = HeaderValue::from_str(header_value.trim()).map_err(|error| {
            format!(
                "供应商 `{}` 的附加请求头 `{}` 值无效：{}。",
                blueprint.provider_name, header_name, error
            )
        })?;
        headers.insert(parsed_name, parsed_value);
    }

    Ok(headers)
}

/// Builds a one-shot provider stream that yields one terminal error and then closes immediately.
/// 构建一条一次性 Provider 流，使其仅产出一条最终错误后立即关闭。
fn one_shot_provider_error_stream(error: impl Into<String>) -> ProviderDeltaStream {
    let error = error.into();

    Box::pin(stream! {
        yield Err(error);
    })
}

/// Converts one assistant streaming content chunk into zero or more `vulcan-core` provider-turn deltas while intentionally ignoring tool-boundary presentation details.
/// 将一条助手流式内容块转换为零条或多条 `vulcan-core` Provider 回合增量，并刻意忽略工具边界展示细节。
fn core_provider_turn_deltas_from_assistant_content<R>(
    content: StreamedAssistantContent<R>,
) -> Vec<CoreProviderTurnDelta> {
    match content {
        StreamedAssistantContent::Text(text) => {
            if text.text.is_empty() {
                Vec::new()
            } else {
                vec![CoreProviderTurnDelta::Output { delta: text.text }]
            }
        }
        StreamedAssistantContent::Reasoning(reasoning) => {
            let flattened = flatten_reasoning_content(&reasoning.content);
            if flattened.is_empty() {
                Vec::new()
            } else {
                vec![CoreProviderTurnDelta::Thinking { delta: flattened }]
            }
        }
        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
            if reasoning.is_empty() {
                Vec::new()
            } else {
                vec![CoreProviderTurnDelta::Thinking { delta: reasoning }]
            }
        }
        StreamedAssistantContent::ToolCall { .. }
        | StreamedAssistantContent::ToolCallDelta { .. }
        | StreamedAssistantContent::Final(_) => Vec::new(),
    }
}

/// Converts one `vulcan-core` provider-turn delta into the GUI-facing runtime delta shape without changing the semantic payload.
/// 将一条 `vulcan-core` Provider 回合增量转换为面向 GUI 的运行时增量形状，同时不改变其语义载荷。
fn core_provider_turn_delta_to_runtime_delta(delta: CoreProviderTurnDelta) -> ProviderStreamDelta {
    match delta {
        CoreProviderTurnDelta::Thinking { delta } => ProviderStreamDelta::Thinking(delta),
        CoreProviderTurnDelta::Output { delta } => ProviderStreamDelta::Output(delta),
    }
}

/// Builds one normalized `vulcan-core` single-turn response from the final rig assistant choice list and provider message identifier.
/// 根据最终 rig 助手 choice 列表与 Provider 消息标识构建一份规范化的 `vulcan-core` 单回合响应。
fn build_openai_compatible_turn_response(
    provider_id: &str,
    model_id: &str,
    provider_message_id: Option<String>,
    assistant_content: &OneOrMany<RigAssistantContent>,
) -> CoreProviderTurnResponse {
    let assistant_text = assistant_content
        .iter()
        .filter_map(|item| match item {
            RigAssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    let assistant_reasoning = assistant_content
        .iter()
        .filter_map(|item| match item {
            RigAssistantContent::Reasoning(reasoning) => {
                let flattened = flatten_reasoning_content(&reasoning.content);
                (!flattened.is_empty()).then_some(flattened)
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tool_calls = assistant_content
        .iter()
        .filter_map(|item| match item {
            RigAssistantContent::ToolCall(tool_call) => Some(CoreConversationToolCall {
                id: tool_call.id.clone(),
                tool_call_id: tool_call.call_id.clone(),
                tool_name: tool_call.function.name.clone(),
                arguments: tool_call.function.arguments.clone(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    CoreProviderTurnResponse {
        assistant_text,
        assistant_reasoning,
        provider_message_id: provider_message_id.clone(),
        tool_calls,
        continuation: Some(CoreProviderContinuation {
            provider_id: Some(provider_id.to_string()),
            model_id: Some(model_id.to_string()),
            last_provider_message_id: provider_message_id,
            updated_at: Some(current_unix_millis_text()),
        }),
    }
}

/// Returns whether one provider runtime event represents a real tool boundary that should seal the current floating thinking phase.
/// 返回某条 Provider 运行时事件是否代表真实工具边界，从而应封口当前浮动思考阶段。
fn is_provider_tool_boundary_runtime_event(event: &RuntimeEvent) -> bool {
    match event.kind {
        RuntimeEventKind::ToolStarted => event.title.starts_with("工具调用 · "),
        RuntimeEventKind::ToolFinished => event.title.starts_with("工具返回 · "),
        _ => false,
    }
}

/// Converts the currently accumulated thinking phase into one completion event exactly once when visible content exists.
/// 当当前累计的思考阶段存在可见内容时，将其恰好一次转换为完成事件。
fn take_current_thinking_phase_completed_event(
    session_id: SessionId,
    thinking_visible_output: &str,
    thinking_completed_emitted: &mut bool,
) -> Option<ExecutionStreamEvent> {
    if *thinking_completed_emitted || thinking_visible_output.trim().is_empty() {
        return None;
    }

    *thinking_completed_emitted = true;
    Some(ExecutionStreamEvent::ThinkingCompleted { session_id })
}

/// Clears the current thinking-phase buffers after one tool boundary so later reasoning starts a fresh floating segment.
/// 在一次工具边界后清空当前思考阶段缓冲，使后续推理从新的浮动片段开始。
fn reset_current_thinking_phase_tracking(
    thinking_raw_output: &mut String,
    thinking_visible_output: &mut String,
    thinking_completed_emitted: &mut bool,
) {
    thinking_raw_output.clear();
    thinking_visible_output.clear();
    *thinking_completed_emitted = false;
}

/// Flattens structured reasoning content into one readable text block for the transcript thinking section.
/// 将结构化推理内容压平成一段适合转录思考区展示的可读文本。
fn flatten_reasoning_content(content: &[ReasoningContent]) -> String {
    content
        .iter()
        .filter_map(|part| match part {
            ReasoningContent::Text { text, .. } => Some(text.trim().to_string()),
            ReasoningContent::Summary(summary) => Some(summary.trim().to_string()),
            ReasoningContent::Encrypted(_) | ReasoningContent::Redacted { .. } => None,
            _ => None,
        })
        .filter(|part: &String| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats one tool-call detail body into the full Markdown payload shown by the inline transcript and modal.
/// 将单次工具调用详情正文格式化为内联转录与弹窗使用的完整 Markdown 载荷。
fn format_tool_call_runtime_detail(tool_name: &str, arguments: &Value) -> String {
    let pretty_arguments =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string());
    format!("## 调用工具\n\n`{tool_name}`\n\n## 调用参数\n\n```json\n{pretty_arguments}\n```")
}

/// Builds one structured runtime-event metadata payload for a completed tool result while preserving host-side metadata fields.
/// 在保留宿主侧元数据字段的前提下，为已完成工具结果构建一份结构化运行时事件元数据载荷。
fn build_tool_result_runtime_metadata(
    tool_call_id: &str,
    tool_result_id: &str,
    host_metadata: Option<&Value>,
) -> Option<Value> {
    let mut metadata = JsonMap::new();
    metadata.insert(
        String::from("tool_call_id"),
        Value::String(tool_call_id.to_string()),
    );
    metadata.insert(
        String::from("tool_result_id"),
        Value::String(tool_result_id.to_string()),
    );

    if let Some(Value::Object(object)) = host_metadata {
        metadata.extend(object.clone());
    } else if let Some(other) = host_metadata {
        metadata.insert(String::from("host_metadata"), other.clone());
    }

    Some(Value::Object(metadata))
}

/// Converts one runtime event into one controller-owned event for execution tracing and later audit projection.
/// 将一条运行时事件转换为一条控制器自有事件，供执行追踪与后续审计投影使用。
fn runtime_event_to_core_event(event: &RuntimeEvent) -> Option<CoreControllerEvent> {
    let kind = match event.kind {
        RuntimeEventKind::ToolStarted => CoreControllerEventKind::ToolStarted,
        RuntimeEventKind::ToolFinished => CoreControllerEventKind::ToolFinished,
        _ => return None,
    };

    Some(CoreControllerEvent {
        kind,
        title: event.title.clone(),
        body: event.detail.clone(),
        metadata: event.metadata.clone(),
    })
}

/// Executes one host-controlled OpenAI Responses prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenAI Responses 提示词调用，并流式返回规范化增量。
fn execute_openai_responses_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| OpenAiResponsesCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "OpenAI Responses",
    )
}

/// Executes one host-controlled Anthropic prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 Anthropic 提示词调用，并流式返回规范化增量。
fn execute_anthropic_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| AnthropicCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "Anthropic 兼容",
    )
}

/// Executes one host-controlled OpenRouter prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenRouter 提示词调用，并流式返回规范化增量。
fn execute_openrouter_stream(
    provider_id: &str,
    api_key: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| OpenRouterCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "OpenRouter",
    )
}

/// Resolves the persisted provider options for one provider identifier.
/// 解析某个供应商标识对应的持久化 Provider 选项。
fn resolve_provider_options<'a>(
    config: &'a ProviderConfigurationFile,
    provider_id: &str,
) -> Result<&'a PersistedProviderOptions, String> {
    config
        .provider
        .get(provider_id)
        .map(|entry| &entry.options)
        .ok_or_else(|| format!("供应商 `{provider_id}` 当前尚未保存配置。"))
}

/// Normalizes assistant output so empty or whitespace-only responses become one readable fallback.
/// 规范化助手输出，确保空白响应会被替换成一条可读的回退文本。
fn normalize_assistant_output(output: String) -> String {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        String::from("模型未返回可见文本输出。")
    } else {
        trimmed.to_string()
    }
}

/// Normalizes one optional streamed text buffer so blank values become `None`.
/// 规范化一段可选流式文本缓冲，使纯空白值转换为 `None`。
fn normalize_optional_stream_output(output: String) -> Option<String> {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Classifies whether the current raw thinking buffer is still pending one synthetic preamble match, has fully matched it, or should be preserved verbatim.
/// 判断当前原始思考缓冲是仍在等待合成前导语完整匹配、已经完整命中该前导语，还是应按原样保留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntheticThinkingPreambleState {
    /// The current buffer only matches a prefix of the synthetic preamble and should not emit visible content yet.
    /// 当前缓冲仅匹配到合成前导语的前缀，因此暂时不应产出可见内容。
    Pending,
    /// The synthetic preamble has been fully matched and visible content should start from the provided byte offset.
    /// 合成前导语已经被完整匹配，可见内容应从给定字节偏移开始。
    Matched { visible_start_byte: usize },
    /// The current buffer is not the synthetic preamble and should be preserved exactly as received.
    /// 当前缓冲并非合成前导语，应按接收结果原样保留。
    Rejected,
}

/// Normalizes one streamed thinking buffer so provider-added synthetic preamble text never enters transcript storage or copies.
/// 规范化一段流式思考缓冲，使 Provider 额外插入的合成前导语不会进入转录存储或复制内容。
fn normalize_streamed_thinking_output(raw_output: &str) -> String {
    match classify_synthetic_thinking_preamble(raw_output) {
        SyntheticThinkingPreambleState::Pending => String::new(),
        SyntheticThinkingPreambleState::Matched { visible_start_byte } => {
            raw_output[visible_start_byte..].to_string()
        }
        SyntheticThinkingPreambleState::Rejected => raw_output.to_string(),
    }
}

/// Detects whether the current raw thinking buffer starts with the synthetic "Here's a thinking process:" preamble emitted by some providers.
/// 检测当前原始思考缓冲是否以部分 Provider 额外插入的 “Here's a thinking process:” 合成前导语开头。
fn classify_synthetic_thinking_preamble(raw_output: &str) -> SyntheticThinkingPreambleState {
    let expected = "here's a thinking process:";
    let mut characters = raw_output.char_indices().peekable();

    while let Some((_, character)) = characters.peek().copied() {
        if matches!(character, ' ' | '\t' | '\r' | '\n' | '·' | '•') {
            characters.next();
        } else {
            break;
        }
    }

    for expected_character in expected.chars() {
        let Some((_, actual_character)) = characters.next() else {
            return SyntheticThinkingPreambleState::Pending;
        };

        if normalize_synthetic_thinking_preamble_character(actual_character)
            != Some(expected_character)
        {
            return SyntheticThinkingPreambleState::Rejected;
        }
    }

    let mut visible_start_byte = characters
        .peek()
        .map(|(byte_index, _)| *byte_index)
        .unwrap_or(raw_output.len());

    while let Some((byte_index, character)) = characters.peek().copied() {
        if matches!(character, ' ' | '\t' | '·' | '•') {
            characters.next();
            visible_start_byte = byte_index + character.len_utf8();
        } else {
            break;
        }
    }

    while let Some((byte_index, character)) = characters.peek().copied() {
        if matches!(character, '\r' | '\n' | ' ' | '\t') {
            characters.next();
            visible_start_byte = byte_index + character.len_utf8();
        } else {
            break;
        }
    }

    SyntheticThinkingPreambleState::Matched { visible_start_byte }
}

/// Normalizes one candidate preamble character into the lowercase comparable form expected by the synthetic-thinking matcher.
/// 将一个候选前导语字符规范化为合成思考匹配器期望的小写可比较形式。
fn normalize_synthetic_thinking_preamble_character(character: char) -> Option<char> {
    match character {
        '’' => Some('\''),
        '\'' | ':' | ' ' => Some(character),
        _ if character.is_ascii() => Some(character.to_ascii_lowercase()),
        _ => None,
    }
}

/// Truncates one preview string while preserving a readable Unicode-safe suffix.
/// 在保留可读 Unicode 后缀的前提下截断一段预览文本。
fn truncate_preview(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        compact
    } else {
        let truncated = compact
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value for protocol-state refreshes inside the executor.
/// 以文本形式返回当前 Unix 毫秒时间戳，供执行器内部刷新协议状态时使用。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_RUNTIME_MAX_TURNS_LIMIT, ExecutionPreparation, SyntheticThinkingPreambleState,
        attach_tool_name_to_runtime_metadata, build_core_conversation_snapshot,
        build_core_provider_request_snapshot, build_host_tool_result_id,
        build_openai_compatible_turn_request_snapshot, build_provider_request_preview_markdown,
        build_runtime_system_preamble, classify_synthetic_thinking_preamble,
        core_messages_to_rig_history, core_snapshot_to_session_conversation_state,
        inject_previous_response_id_into_additional_params, normalize_streamed_thinking_output,
        resolve_host_visible_tool_call_id, resolve_runtime_default_max_turns_limit,
        resolve_runtime_max_turns,
    };
    use crate::agent_runtime::config::{RuntimeAgentDefinition, RuntimeAgentKind};
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
        SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    };
    use crate::model::{SessionMessageKind, SessionMessageRecordKind};
    use crate::provider_config::ProviderRuntimeKind;
    use rig::message::Message as RigMessage;
    use rig::message::ToolCall as RigToolCall;
    use serde_json::json;
    use std::collections::BTreeMap;
    use vulcan_core::types::{
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
        ProviderRequestToolSchema as CoreProviderRequestToolSchema,
    };
    use vulcan_tools_manager::{
        ResolvedToolDefinition, VulcanToolDescriptor, VulcanToolSourceKind,
    };

    /// Verifies synthetic reasoning preamble text is removed before streamed thinking content enters transcript storage.
    /// 验证合成推理前导语会在流式思考内容进入转录存储之前被移除。
    #[test]
    fn streamed_thinking_preamble_is_removed_before_storage() {
        let chunk_one = "Here's a think";
        let chunk_two = "ing process:\n\n1. 分析问题";

        assert_eq!(normalize_streamed_thinking_output(chunk_one), "");
        assert_eq!(
            normalize_streamed_thinking_output(&format!("{chunk_one}{chunk_two}")),
            "1. 分析问题"
        );
    }

    /// Verifies normal reasoning text is preserved verbatim instead of being mistaken for the synthetic preamble.
    /// 验证正常推理文本会被原样保留，而不会被误判成合成前导语。
    #[test]
    fn normal_reasoning_text_is_not_mistaken_for_synthetic_preamble() {
        let raw_output = "Hello reasoning\n- 实际思考内容";

        assert!(matches!(
            classify_synthetic_thinking_preamble(raw_output),
            SyntheticThinkingPreambleState::Rejected
        ));
        assert_eq!(normalize_streamed_thinking_output(raw_output), raw_output);
    }

    /// Verifies the last-request preview contains the full system prompt, user prompt, and tool schema sections.
    /// 验证最后一次请求预览会包含完整系统提示词、用户提示词与工具 schema 分节。
    #[test]
    fn provider_request_preview_contains_core_sections() {
        let preparation = ExecutionPreparation {
            session_id: uuid::Uuid::nil(),
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
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({ "reasoning": { "effort": "high" } }),
            },
            prompt_preview: String::from("请分析这段代码"),
            workspace_path: String::from("D:/workspace"),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: vec![String::from("codekit")],
            tool_descriptors: vec![VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            }],
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
        };
        let request_snapshot = CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system prompt body"),
            prompt: String::from("effective prompt body"),
            history: vec![
                CoreConversationMessage {
                    id: String::from("history-user-1"),
                    payload: CoreConversationPayload::UserText {
                        text: String::from("之前的用户问题"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("history-assistant-1"),
                    payload: CoreConversationPayload::Assistant {
                        text: String::from("之前的助手回答"),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("resp_1")),
                        tool_calls: Vec::new(),
                    },
                },
            ],
            temperature: Some(0.2),
            max_turns: DEFAULT_RUNTIME_MAX_TURNS_LIMIT,
            tool_schemas: vec![CoreProviderRequestToolSchema {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source_label: String::from("原生工具"),
                parameters_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            additional_params: Some(json!({ "reasoning": { "effort": "high" } })),
            request_headers: Some(json!({
                "user-agent": "vulcancode/test",
                "authorization": "Bearer masked"
            })),
            continuation: CoreProviderContinuation {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp_prev")),
                updated_at: Some(String::from("123")),
            },
        };

        let preview = build_provider_request_preview_markdown(&preparation, &request_snapshot);

        assert!(preview.contains("# 最后一次真实 API 请求快照"));
        assert!(preview.contains("## System Preamble"));
        assert!(preview.contains("system prompt body"));
        assert!(preview.contains("## Continuation"));
        assert!(preview.contains("resp_prev"));
        assert!(preview.contains("## Outgoing History"));
        assert!(preview.contains("之前的用户问题"));
        assert!(preview.contains("## Effective Outgoing Prompt"));
        assert!(preview.contains("effective prompt body"));
        assert!(preview.contains("### `read_file`"));
        assert!(preview.contains("\"path\""));
        assert!(preview.contains("## Request Headers"));
        assert!(preview.contains("vulcancode/test"));
        assert!(preview.contains("\"effort\": \"high\""));
        assert!(preview.contains("- 最大轮次：不限制"));
        assert!(!preview.contains("之前的推理摘要"));
    }

    /// Verifies `vulcan-core` request snapshots preserve continuation, prompt, and tool-schema details.
    /// 验证 `vulcan-core` 请求快照会保留 continuation、提示词与工具 schema 细节。
    #[test]
    fn core_request_snapshot_preserves_continuation_and_tool_details() {
        let preparation = ExecutionPreparation {
            session_id: uuid::Uuid::nil(),
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
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: Some(String::from("vulcancode/test")),
                extra_headers: BTreeMap::from([(
                    String::from("x-trace-id"),
                    String::from("trace-1"),
                )]),
                extra_body: json!({ "reasoning": { "effort": "medium" } }),
            },
            prompt_preview: String::from("请分析这段代码"),
            workspace_path: String::from("D:/workspace"),
            context_history_count: 1,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: vec![String::from("codekit")],
            tool_descriptors: vec![VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            }],
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
        };
        let conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("history-user-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("之前的用户问题"),
                    },
                },
                SessionConversationMessage {
                    id: String::from("history-assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("之前的助手回答"),
                        reasoning: String::from("之前的推理摘要"),
                        provider_message_id: Some(String::from("resp_1")),
                        tool_calls: Vec::new(),
                    },
                },
            ],
            records: Vec::new(),
            continuation: crate::conversation_protocol::SessionProviderContinuationState {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp_prev")),
                updated_at: Some(String::from("123")),
            },
        };
        let resolved_tools = vec![ResolvedToolDefinition {
            descriptor: VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            invoke: std::sync::Arc::new(|_, _| unreachable!("test preview does not invoke tools")),
        }];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let snapshot = runtime
            .block_on(build_core_provider_request_snapshot(
                &preparation,
                &conversation_state,
                "system prompt body",
                "effective prompt body",
                &resolved_tools,
                Some(5),
            ))
            .expect("request snapshot should build");

        assert_eq!(snapshot.prompt, "effective prompt body");
        assert_eq!(snapshot.max_turns, 5);
        assert_eq!(snapshot.tool_schemas.len(), 1);
        assert_eq!(
            snapshot.continuation.last_provider_message_id.as_deref(),
            Some("resp_prev")
        );
        let Some(CoreConversationMessage {
            payload: CoreConversationPayload::Assistant { reasoning, .. },
            ..
        }) = snapshot.history.get(1)
        else {
            panic!("assistant history item should exist");
        };
        assert!(reasoning.is_empty());
        assert_eq!(
            snapshot.request_headers,
            Some(json!({
                "user-agent": "vulcancode/test",
                "x-trace-id": "trace-1"
            }))
        );
    }

    /// Verifies `vulcan-core` snapshot round-trips preserve pre-existing durable records, metadata, and continuation state.
    /// 验证 `vulcan-core` 快照往返过程会保留既有持久化记录、元数据与 continuation 状态。
    #[test]
    fn core_snapshot_round_trip_preserves_records_and_continuation() {
        let conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("user-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("请分析这个项目"),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("我先读取配置文件。"),
                        reasoning: String::from("先定位入口再读取配置。"),
                        provider_message_id: Some(String::from("resp-1")),
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("tool-1"),
                            tool_call_id: Some(String::from("call-1")),
                            tool_name: String::from("read_file"),
                            arguments: json!({ "path": "Cargo.toml" }),
                        }],
                    },
                },
            ],
            records: vec![
                SessionConversationRecord {
                    id: String::from("record-user"),
                    kind: SessionMessageKind::User,
                    record_kind: SessionMessageRecordKind::Transcript,
                    visible: true,
                    title: String::from("用户输入"),
                    body: String::from("请分析这个项目"),
                    metadata: None,
                    created_at: String::from("100"),
                },
                SessionConversationRecord {
                    id: String::from("record-tool-call"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::ToolCallOutput,
                    visible: true,
                    title: String::from("调用 read_file 工具"),
                    body: String::from("{\"path\":\"Cargo.toml\"}"),
                    metadata: Some(json!({
                        "tool_call_id": "call-1",
                        "tool_name": "read_file",
                        "arguments": { "path": "Cargo.toml" }
                    })),
                    created_at: String::from("101"),
                },
                SessionConversationRecord {
                    id: String::from("record-tool-result"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::ToolResultOutput,
                    visible: true,
                    title: String::from("调用 read_file 工具"),
                    body: String::from("name = \"vulcan-code\""),
                    metadata: Some(json!({
                        "tool_call_id": "call-1",
                        "tool_name": "read_file",
                        "kind": "file_read"
                    })),
                    created_at: String::from("102"),
                },
            ],
            continuation: SessionProviderContinuationState {
                provider_id: Some(String::from("openai-compatible")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-1")),
                updated_at: Some(String::from("123")),
            },
        };

        let snapshot = build_core_conversation_snapshot(&conversation_state);
        let rebuilt_state = core_snapshot_to_session_conversation_state(&snapshot);

        assert_eq!(rebuilt_state.records.len(), 3);
        assert_eq!(
            rebuilt_state.records[1].record_kind,
            SessionMessageRecordKind::ToolCallOutput
        );
        assert_eq!(
            rebuilt_state.records[2].record_kind,
            SessionMessageRecordKind::ToolResultOutput
        );
        assert_eq!(
            rebuilt_state.records[2].metadata,
            conversation_state.records[2].metadata
        );
        assert_eq!(
            rebuilt_state
                .continuation
                .last_provider_message_id
                .as_deref(),
            Some("resp-1")
        );
    }

    /// Verifies `vulcan-core` messages projected back into rig history preserve tool calls while still excluding reasoning replay.
    /// 验证 `vulcan-core` 消息投影回 rig 历史时会保留工具调用，同时仍然排除思考回放。
    #[test]
    fn core_messages_to_rig_history_preserve_tool_calls_without_reasoning_replay() {
        let messages = vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先读取文件。"),
                reasoning: String::from("这段思考不应回放。"),
                provider_message_id: Some(String::from("resp-1")),
                tool_calls: vec![vulcan_core::types::ConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
            },
        }];

        let history = core_messages_to_rig_history(&messages);
        let [RigMessage::Assistant { id, content }] = history.as_slice() else {
            panic!("history should contain one assistant message");
        };

        assert_eq!(id.as_deref(), Some("resp-1"));
        assert!(content.iter().any(|item| matches!(
            item,
            rig::message::AssistantContent::Text(text) if text.text == "我先读取文件。"
        )));
        assert!(content.iter().any(|item| matches!(
            item,
            rig::message::AssistantContent::ToolCall(tool_call)
                if tool_call.id == "tool-1"
                && tool_call.call_id.as_deref() == Some("call-1")
                && tool_call.function.name == "read_file"
        )));
        assert!(
            !content
                .iter()
                .any(|item| matches!(item, rig::message::AssistantContent::Reasoning(_)))
        );
    }

    /// Verifies the runtime leaves multi-turn providers unlimited when no explicit max-turn override exists.
    /// 验证当未显式提供最大轮次覆盖值时，运行时会让多轮 Provider 保持不限制状态。
    #[test]
    fn runtime_max_turns_default_to_unlimited_without_override() {
        assert_eq!(resolve_runtime_max_turns(None), None);
        assert_eq!(
            resolve_runtime_default_max_turns_limit(None),
            DEFAULT_RUNTIME_MAX_TURNS_LIMIT
        );

        let worker_agent = RuntimeAgentDefinition {
            id: String::from("worker"),
            name: String::from("worker"),
            kind: RuntimeAgentKind::Worker,
            description: String::new(),
            system_prompt: String::new(),
            skill_ids: Vec::new(),
            native_tools: Vec::new(),
            mcp_tools: Vec::new(),
            delegates: Vec::new(),
            tool_policy: None,
            routes: Vec::new(),
            fallback_delegate: None,
            model: None,
            temperature: None,
            max_turns: None,
        };
        assert_eq!(resolve_runtime_max_turns(Some(&worker_agent)), None);
        assert_eq!(
            resolve_runtime_default_max_turns_limit(Some(&worker_agent)),
            DEFAULT_RUNTIME_MAX_TURNS_LIMIT
        );
    }

    /// Verifies runtime system preamble no longer embeds agent-authored prompt text or duplicated tool catalogs.
    /// 验证运行时系统提示词不再嵌入代理自定义提示词文本与重复工具目录。
    #[test]
    fn runtime_system_preamble_uses_base_prompt_without_tool_catalog_text() {
        let preparation = ExecutionPreparation {
            session_id: uuid::Uuid::nil(),
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
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({}),
            },
            prompt_preview: String::from("请分析这段代码"),
            workspace_path: String::from("D:/workspace"),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: vec![String::from("codekit")],
            tool_descriptors: vec![VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            }],
            delegate_agents: vec![String::from("delegate-a")],
            enabled_mcp_servers: vec![String::from("demo-server")],
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
        };
        let preamble = build_runtime_system_preamble(&preparation);

        assert!(!preamble.contains("AGENT OVERRIDE PROMPT"));
        assert!(!preamble.contains("原生工具已真实注册"));
        assert!(!preamble.contains("LuaSkills 工具目录"));
        assert!(!preamble.contains("MCP 工具已真实注册"));
        assert!(!preamble.contains("可委派代理"));
        assert!(preamble.contains("当前工作区：D:/workspace"));
        assert!(preamble.contains("当前执行模式：Build"));
    }

    /// Verifies host-side tool-result correlation prefers provider call ids while still generating stable synthetic tool-result ids.
    /// 验证宿主侧工具结果关联会优先使用 Provider call id，同时仍生成稳定的合成工具结果标识。
    #[test]
    fn host_tool_result_correlation_prefers_provider_call_id() {
        let tool_call = RigToolCall {
            id: String::from("provider-tool-id"),
            call_id: Some(String::from("provider-call-id")),
            function: rig::message::ToolFunction {
                name: String::from("read_file"),
                arguments: json!({ "path": "src/main.rs" }),
            },
            signature: None,
            additional_params: None,
        };

        let resolved_tool_call_id = resolve_host_visible_tool_call_id(&tool_call);

        assert_eq!(resolved_tool_call_id, "provider-call-id");
        assert_eq!(
            build_host_tool_result_id(&resolved_tool_call_id),
            "tool-result-provider-call-id"
        );
    }

    /// Verifies tool-finished metadata always keeps the tool name even when host metadata already carries other structured payloads.
    /// 验证工具完成元数据即使已经携带其他结构化载荷，也会始终保留工具名称字段。
    #[test]
    fn tool_finished_metadata_keeps_tool_name_for_display_fallback() {
        let metadata = attach_tool_name_to_runtime_metadata(
            Some(json!({
                "tool_call_id": "call-1",
                "tool_result_id": "result-1",
                "kind": "change_set"
            })),
            "write_file",
        )
        .expect("metadata should exist");

        assert_eq!(metadata["tool_call_id"], "call-1");
        assert_eq!(metadata["tool_result_id"], "result-1");
        assert_eq!(metadata["kind"], "change_set");
        assert_eq!(metadata["tool_name"], "write_file");
    }

    /// Verifies OpenAI Responses continuation injection preserves existing additional params while adding one missing `previous_response_id`.
    /// 验证 OpenAI Responses 的 continuation 注入会在补入缺失 `previous_response_id` 的同时保留既有附加参数。
    #[test]
    fn openai_responses_continuation_injection_preserves_existing_additional_params() {
        let continuation = CoreProviderContinuation {
            provider_id: Some(String::from("openai")),
            model_id: Some(String::from("gpt-5")),
            last_provider_message_id: Some(String::from("resp_prev")),
            updated_at: Some(String::from("123")),
        };

        let merged = inject_previous_response_id_into_additional_params(
            Some(json!({
                "reasoning": { "effort": "high" }
            })),
            &continuation,
        )
        .expect("merged params should exist");
        assert_eq!(merged["reasoning"]["effort"], "high");
        assert_eq!(merged["previous_response_id"], "resp_prev");

        let preserved = inject_previous_response_id_into_additional_params(
            Some(json!({
                "previous_response_id": "resp_existing",
                "reasoning": { "effort": "medium" }
            })),
            &continuation,
        )
        .expect("preserved params should exist");
        assert_eq!(preserved["previous_response_id"], "resp_existing");
        assert_eq!(preserved["reasoning"]["effort"], "medium");
    }

    /// Verifies host-controlled OpenAI-compatible snapshots treat tool-result messages as the real outgoing prompt of the next turn.
    /// 验证宿主控制的 OpenAI 兼容请求快照会把工具结果消息视作下一轮真实出站提示词。
    #[test]
    fn openai_compatible_turn_snapshot_uses_tool_result_as_outgoing_prompt() {
        let headers = reqwest::header::HeaderMap::new();
        let history = vec![RigMessage::assistant_with_id(
            String::from("assistant-1"),
            "请调用工具处理文件。",
        )];
        let prompt = RigMessage::tool_result_with_call_id(
            String::from("result-1"),
            Some(String::from("call-1")),
            "文件修改完成",
        );

        let snapshot = build_openai_compatible_turn_request_snapshot(
            "groq",
            "llama-4",
            "system prompt body",
            &history,
            &prompt,
            Some(0.3),
            6,
            &[],
            &headers,
            None,
        );

        assert_eq!(snapshot.provider_id, "groq");
        assert_eq!(snapshot.model_id, "llama-4");
        assert_eq!(snapshot.prompt, "文件修改完成");
        assert_eq!(snapshot.history.len(), 1);
        assert_eq!(
            snapshot.continuation.last_provider_message_id.as_deref(),
            Some("assistant-1")
        );
    }
}
