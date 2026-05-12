//! Execution-preparation models and real provider execution helpers for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备模型与真实 Provider 执行辅助逻辑。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider::{ProviderRuntimeBlueprint, resolve_active_provider_blueprint};
use crate::agent_runtime::rig_tools::{RigToolOutcomeCapture, build_vulcan_rig_tools};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    conversation_messages_to_rig_history, replace_messages_from_rig_history,
    rig_history_to_conversation_messages,
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
use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use rig::agent::{AgentBuilder, MultiTurnStreamItem};
use rig::message::{Message as RigMessage, ReasoningContent, ToolResultContent};
use rig::prelude::CompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingPrompt};
use rig::tool::ToolDyn;
use serde_json::{Map as JsonMap, Value, json};
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::Path, path::PathBuf};
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
    /// Canonical conversation history reconstructed from the session protocol state.
    /// 从会话协议状态重建出的规范会话历史。
    pub conversation_history: Vec<SessionConversationMessage>,
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
        context_history_count: request.conversation_history.len(),
        execution_mode: request.execution_mode.clone(),
        temperature: request.temperature,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors: resolved_tools.descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: foundation.enabled_mcp_server_rows(),
        warnings,
        provider_request_preview_markdown: String::new(),
    })
}

/// Lightweight provider-stream delta emitted while one real run is still in progress.
/// 单次真实运行仍在进行中时发出的轻量 Provider 流式增量。
enum ProviderStreamDelta {
    /// Assistant thinking delta returned by the provider stream.
    /// Provider 流返回的助手思考增量。
    Thinking(String),
    /// Assistant final-output delta returned by the provider stream.
    /// Provider 流返回的助手正式输出增量。
    Output(String),
    /// Runtime event emitted from one streamed provider-side tool interaction.
    /// 来自某次 Provider 侧流式工具交互的运行时事件。
    RuntimeEvent(RuntimeEvent),
    /// Final rig history snapshot captured after the provider finishes one multi-turn run.
    /// 在 Provider 完成一次多轮运行后捕获的最终 rig 历史快照。
    FinalConversationHistory(Vec<RigMessage>),
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
        let request_history = conversation_messages_to_rig_history(&request.conversation_history);
        let preview_history = rig_history_to_conversation_messages(&request_history);
        preparation.provider_request_preview_markdown = build_provider_request_preview_markdown(
            &preparation,
            &system_preamble,
            &preview_history,
            request.prompt.trim(),
            &resolved_tools.definitions,
            resolve_runtime_max_turns(active_agent.as_ref()),
        );

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
            tools_manager,
            resolved_tools.definitions,
        );

        let mut terminal_error = None;
        let mut final_conversation_state = None;

        while let Some(item) = provider_stream.next().await {
            match item {
                Ok(ProviderStreamDelta::Thinking(delta)) => {
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
                    runtime_events.push(event.clone());
                    yield ExecutionStreamEvent::RuntimeEvent {
                        session_id: request.session_id,
                        event,
                    };
                }
                Ok(ProviderStreamDelta::FinalConversationHistory(history)) => {
                    let mut conversation_state = SessionConversationState::default();
                    replace_messages_from_rig_history(
                        &mut conversation_state,
                        &history,
                        Some(&preparation.model_blueprint.provider_id),
                        Some(&preparation.model_blueprint.model_id),
                        current_unix_millis_text(),
                    );
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
    system_preamble: &str,
    conversation_history: &[SessionConversationMessage],
    latest_prompt: &str,
    resolved_tools: &[ResolvedToolDefinition],
    max_turns: Option<usize>,
) -> String {
    let temperature_label = preparation
        .temperature
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| String::from("使用模型默认值"));
    let max_turns_label = max_turns
        .map(|value| format!("`{value}`"))
        .unwrap_or_else(|| String::from("不限制"));
    let extra_body_markdown = preparation
        .model_blueprint
        .extra_body
        .as_object()
        .filter(|object| !object.is_empty())
        .map(|_| render_json_code_fence(&preparation.model_blueprint.extra_body))
        .unwrap_or_else(|| String::from("（无附加 Body 参数）"));
    let tool_rows = if resolved_tools.is_empty() {
        String::from("（本次请求未直接暴露工具 schema）")
    } else {
        resolved_tools
            .iter()
            .map(render_resolved_tool_request_section)
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let outgoing_history_markdown = render_conversation_history_markdown(conversation_history);

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
            "## Outgoing History\n",
            "{}\n\n",
            "## Effective User Prompt\n",
            "{}\n\n",
            "## Tool Schemas\n",
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
        conversation_history.len(),
        resolved_tools.len(),
        render_text_code_fence(system_preamble),
        outgoing_history_markdown,
        render_text_code_fence(latest_prompt),
        tool_rows,
        extra_body_markdown,
    )
}

/// Renders the canonical history into a structured Markdown snapshot that is easier to inspect than one flattened prompt string.
/// 将规范历史渲染为结构化 Markdown 快照，使其比单条扁平 Prompt 字符串更易检查。
fn render_conversation_history_markdown(history: &[SessionConversationMessage]) -> String {
    if history.is_empty() {
        return String::from("（本次请求没有历史消息）");
    }

    history
        .iter()
        .enumerate()
        .map(|(index, message)| render_conversation_message_preview(index, message))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders one canonical conversation message into the provider-preview Markdown.
/// 将单条规范会话消息渲染到 Provider 预览 Markdown 中。
fn render_conversation_message_preview(
    index: usize,
    message: &SessionConversationMessage,
) -> String {
    match &message.payload {
        SessionConversationPayload::UserText { text } => format!(
            "### {}. user · `{}`\n{}",
            index + 1,
            message.id,
            render_text_code_fence(text)
        ),
        SessionConversationPayload::Assistant {
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
        SessionConversationPayload::ToolResult {
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

/// Renders one resolved tool into a Markdown section used by the request-preview tab.
/// 将单个已解析工具渲染为请求预览页签使用的 Markdown 分节。
fn render_resolved_tool_request_section(definition: &ResolvedToolDefinition) -> String {
    format!(
        concat!(
            "### `{}`\n",
            "- 显示名：{}\n",
            "- 来源：{}\n",
            "- 描述：{}\n",
            "{}"
        ),
        definition.descriptor.id,
        definition.descriptor.name,
        tool_source_label(definition.descriptor.source),
        definition.descriptor.description,
        render_json_code_fence(&definition.parameters_schema),
    )
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
    let additional_params = preparation
        .model_blueprint
        .extra_body
        .as_object()
        .filter(|object| !object.is_empty())
        .map(|object| Value::Object(object.clone()));
    let tool_outcome_capture = RigToolOutcomeCapture::default();
    let build_runtime_tools = || {
        build_vulcan_rig_tools(
            tools_manager.clone(),
            preparation.session_id,
            &resolved_tools,
            tool_outcome_capture.clone(),
        )
    };

    match preparation.model_blueprint.runtime_kind {
        ProviderRuntimeKind::OpenAi => execute_openai_responses_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            request_history,
            temperature,
            max_turns,
            &request_headers,
            build_runtime_tools(),
            tool_outcome_capture.clone(),
            additional_params,
        ),
        ProviderRuntimeKind::OpenAiCompatible
        | ProviderRuntimeKind::XAi
        | ProviderRuntimeKind::Mistral
        | ProviderRuntimeKind::Groq
        | ProviderRuntimeKind::TogetherAi
        | ProviderRuntimeKind::Perplexity => execute_openai_compatible_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            request_history,
            temperature,
            max_turns,
            &request_headers,
            build_runtime_tools(),
            tool_outcome_capture.clone(),
            additional_params,
        ),
        ProviderRuntimeKind::OpenRouter => execute_openrouter_stream(
            &api_key,
            model_id,
            system_preamble,
            prompt,
            request_history,
            temperature,
            max_turns,
            &request_headers,
            build_runtime_tools(),
            tool_outcome_capture.clone(),
            additional_params,
        ),
        ProviderRuntimeKind::Anthropic => execute_anthropic_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            request_history,
            temperature,
            max_turns,
            &request_headers,
            build_runtime_tools(),
            tool_outcome_capture.clone(),
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
fn resolve_runtime_default_max_turns_limit(
    active_agent: Option<&RuntimeAgentDefinition>,
) -> usize {
    active_agent
        .and_then(|agent| agent.max_turns)
        .filter(|value| *value > 0)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_RUNTIME_MAX_TURNS_LIMIT)
}

/// Executes one OpenAI-compatible prompt using the chat-completions compatible rig client and streams deltas.
/// 使用兼容 Chat Completions 的 rig 客户端执行一次 OpenAI 兼容提示词调用并流式返回增量。
fn execute_openai_compatible_stream(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<RigMessage>,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
    tool_outcome_capture: RigToolOutcomeCapture,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let mut builder = openai::CompletionsClient::builder()
        .api_key(api_key)
        .http_headers(headers.clone());

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = match builder.build() {
        Ok(client) => client,
        Err(error) => {
            return one_shot_provider_error_stream(format!(
                "创建 OpenAI 兼容客户端失败：{}。",
                error
            ));
        }
    };
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code Worker")
        .preamble(system_preamble);

    agent_builder = agent_builder.default_max_turns(max_turns);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }
    if let Some(additional_params) = additional_params {
        agent_builder = agent_builder.additional_params(additional_params);
    }

    let agent = if luaskills_rig_tools.is_empty() {
        agent_builder.build()
    } else {
        agent_builder.tools(luaskills_rig_tools).build()
    };
    let prompt = prompt.to_string();

    Box::pin(stream! {
        let mut stream = agent
            .stream_prompt(prompt)
            .with_history(request_history)
            .multi_turn(max_turns)
            .await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("OpenAI 兼容流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item, &tool_outcome_capture) {
                        yield Ok(delta);
                    }
                }
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    })
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

/// Converts one multi-turn streaming item into zero or more GUI-facing provider deltas.
/// 将一条多轮流式项转换为零条或多条面向图形界面的 Provider 增量。
fn provider_deltas_from_multi_turn_item<R>(
    item: MultiTurnStreamItem<R>,
    tool_outcome_capture: &RigToolOutcomeCapture,
) -> Vec<ProviderStreamDelta> {
    match item {
        MultiTurnStreamItem::StreamAssistantItem(content) => {
            provider_deltas_from_assistant_content(content)
        }
        MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
            tool_result,
            ..
        }) => {
            let tool_call_id = tool_result
                .call_id
                .clone()
                .unwrap_or_else(|| tool_result.id.clone());
            let tool_metadata = tool_outcome_capture.take_next().and_then(|outcome| outcome.metadata);
            let event_metadata = build_tool_result_runtime_metadata(
                &tool_call_id,
                &tool_result.id,
                tool_metadata.as_ref(),
            );
            vec![ProviderStreamDelta::RuntimeEvent(RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                format!("工具返回 · {}", tool_call_id),
                format_tool_result_runtime_detail(&tool_result),
            )
            .with_metadata(event_metadata))]
        }
        MultiTurnStreamItem::FinalResponse(final_response) => final_response
            .history()
            .map(|history| vec![ProviderStreamDelta::FinalConversationHistory(history.to_vec())])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Converts one assistant streaming content chunk into zero or more GUI-facing provider deltas.
/// 将一条助手流式内容块转换为零条或多条面向图形界面的 Provider 增量。
fn provider_deltas_from_assistant_content<R>(
    content: StreamedAssistantContent<R>,
) -> Vec<ProviderStreamDelta> {
    match content {
        StreamedAssistantContent::Text(text) => {
            if text.text.is_empty() {
                Vec::new()
            } else {
                vec![ProviderStreamDelta::Output(text.text)]
            }
        }
        StreamedAssistantContent::Reasoning(reasoning) => {
            let flattened = flatten_reasoning_content(&reasoning.content);
            if flattened.is_empty() {
                Vec::new()
            } else {
                vec![ProviderStreamDelta::Thinking(flattened)]
            }
        }
        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
            if reasoning.is_empty() {
                Vec::new()
            } else {
                vec![ProviderStreamDelta::Thinking(reasoning)]
            }
        }
        StreamedAssistantContent::ToolCall { tool_call, .. } => {
            let tool_call_id = tool_call
                .call_id
                .clone()
                .unwrap_or_else(|| tool_call.id.clone());
            vec![ProviderStreamDelta::RuntimeEvent(RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                format!("工具调用 · {} · {}", tool_call_id, tool_call.function.name),
                format_tool_call_runtime_detail(
                    &tool_call.function.name,
                    &tool_call.function.arguments,
                ),
            )
            .with_metadata(Some(json!({
                "tool_call_id": tool_call_id,
                "tool_name": tool_call.function.name,
                "arguments": tool_call.function.arguments,
            }))))]
        }
        StreamedAssistantContent::ToolCallDelta { .. } => Vec::new(),
        StreamedAssistantContent::Final(_) => Vec::new(),
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

/// Formats one tool-result payload into the full Markdown body shown by the inline transcript and modal.
/// 将单次工具返回载荷格式化为内联转录与弹窗使用的完整 Markdown 正文。
fn format_tool_result_runtime_detail(tool_result: &rig::message::ToolResult) -> String {
    let rendered_content = render_tool_result_content_markdown(&tool_result.content);
    format!(
        "## 返回标识\n\n`{}`\n\n{}",
        tool_result.id, rendered_content
    )
}

/// Renders one tool-result content list into readable Markdown text.
/// 将一组工具返回内容渲染为可读的 Markdown 文本。
fn render_tool_result_content_markdown(content: &rig::OneOrMany<ToolResultContent>) -> String {
    let segments = content
        .iter()
        .map(|item| match item {
            ToolResultContent::Text(text) => text.text.trim().to_string(),
            ToolResultContent::Image(image) => serde_json::to_string_pretty(image)
                .map(|json| format!("```json\n{json}\n```"))
                .unwrap_or_else(|_| String::from("```json\n{}\n```")),
        })
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>();

    if segments.is_empty() {
        return String::from("## 工具返回\n\n（空结果）");
    }

    format!("## 工具返回\n\n{}", segments.join("\n\n---\n\n"))
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

/// Executes one OpenAI Responses prompt using rig's default OpenAI responses client and streams deltas.
/// 使用 rig 默认的 OpenAI Responses 客户端执行一次 OpenAI Responses 提示词调用并流式返回增量。
fn execute_openai_responses_stream(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<RigMessage>,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
    tool_outcome_capture: RigToolOutcomeCapture,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let mut builder = openai::Client::builder()
        .api_key(api_key)
        .http_headers(headers.clone());

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = match builder.build() {
        Ok(client) => client,
        Err(error) => {
            return one_shot_provider_error_stream(format!(
                "创建 OpenAI Responses 客户端失败：{}。",
                error
            ));
        }
    };
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code Responses Worker")
        .preamble(system_preamble);

    agent_builder = agent_builder.default_max_turns(max_turns);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }
    if let Some(additional_params) = additional_params {
        agent_builder = agent_builder.additional_params(additional_params);
    }

    let agent = if luaskills_rig_tools.is_empty() {
        agent_builder.build()
    } else {
        agent_builder.tools(luaskills_rig_tools).build()
    };
    let prompt = prompt.to_string();

    Box::pin(stream! {
        let mut stream = agent
            .stream_prompt(prompt)
            .with_history(request_history)
            .multi_turn(max_turns)
            .await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("OpenAI Responses 流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item, &tool_outcome_capture) {
                        yield Ok(delta);
                    }
                }
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    })
}

/// Executes one Anthropic-compatible prompt using rig's native Anthropic client and streams deltas.
/// 使用 rig 原生 Anthropic 客户端执行一次 Anthropic 兼容提示词调用并流式返回增量。
fn execute_anthropic_stream(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<RigMessage>,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
    tool_outcome_capture: RigToolOutcomeCapture,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let mut builder = anthropic::Client::builder()
        .api_key(api_key)
        .http_headers(headers.clone());

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = match builder.build() {
        Ok(client) => client,
        Err(error) => {
            return one_shot_provider_error_stream(format!(
                "创建 Anthropic 兼容客户端失败：{}。",
                error
            ));
        }
    };
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code Anthropic Worker")
        .preamble(system_preamble);

    agent_builder = agent_builder.default_max_turns(max_turns);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }
    if let Some(additional_params) = additional_params {
        agent_builder = agent_builder.additional_params(additional_params);
    }

    let agent = if luaskills_rig_tools.is_empty() {
        agent_builder.build()
    } else {
        agent_builder.tools(luaskills_rig_tools).build()
    };
    let prompt = prompt.to_string();

    Box::pin(stream! {
        let mut stream = agent
            .stream_prompt(prompt)
            .with_history(request_history)
            .multi_turn(max_turns)
            .await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("Anthropic 兼容流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item, &tool_outcome_capture) {
                        yield Ok(delta);
                    }
                }
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    })
}

/// Executes one OpenRouter prompt using the native OpenRouter rig client and streams deltas.
/// 使用原生 OpenRouter rig 客户端执行一次 OpenRouter 提示词调用并流式返回增量。
fn execute_openrouter_stream(
    api_key: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<RigMessage>,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
    tool_outcome_capture: RigToolOutcomeCapture,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let client = match openrouter::Client::builder()
        .api_key(api_key)
        .http_headers(headers.clone())
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return one_shot_provider_error_stream(format!(
                "创建 OpenRouter 客户端失败：{}。",
                error
            ));
        }
    };
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code OpenRouter Worker")
        .preamble(system_preamble);

    agent_builder = agent_builder.default_max_turns(max_turns);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }
    if let Some(additional_params) = additional_params {
        agent_builder = agent_builder.additional_params(additional_params);
    }

    let agent = if luaskills_rig_tools.is_empty() {
        agent_builder.build()
    } else {
        agent_builder.tools(luaskills_rig_tools).build()
    };
    let prompt = prompt.to_string();

    Box::pin(stream! {
        let mut stream = agent
            .stream_prompt(prompt)
            .with_history(request_history)
            .multi_turn(max_turns)
            .await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("OpenRouter 流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item, &tool_outcome_capture) {
                        yield Ok(delta);
                    }
                }
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    })
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
        DEFAULT_RUNTIME_MAX_TURNS_LIMIT, ExecutionPreparation,
        SyntheticThinkingPreambleState, build_provider_request_preview_markdown,
        build_runtime_system_preamble, classify_synthetic_thinking_preamble,
        normalize_streamed_thinking_output, resolve_runtime_default_max_turns_limit,
        resolve_runtime_max_turns,
    };
    use crate::agent_runtime::config::{RuntimeAgentDefinition, RuntimeAgentKind};
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, conversation_messages_to_rig_history,
        rig_history_to_conversation_messages,
    };
    use crate::provider_config::ProviderRuntimeKind;
    use serde_json::json;
    use std::collections::BTreeMap;
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
        let conversation_history = vec![
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
        ];
        let outgoing_history =
            rig_history_to_conversation_messages(&conversation_messages_to_rig_history(
                &conversation_history,
            ));

        let preview = build_provider_request_preview_markdown(
            &preparation,
            "system prompt body",
            &outgoing_history,
            "effective prompt body",
            &resolved_tools,
            None,
        );

        assert!(preview.contains("# 最后一次真实 API 请求快照"));
        assert!(preview.contains("## System Preamble"));
        assert!(preview.contains("system prompt body"));
        assert!(preview.contains("## Outgoing History"));
        assert!(preview.contains("之前的用户问题"));
        assert!(preview.contains("## Effective User Prompt"));
        assert!(preview.contains("effective prompt body"));
        assert!(preview.contains("### `read_file`"));
        assert!(preview.contains("\"path\""));
        assert!(preview.contains("\"effort\": \"high\""));
        assert!(preview.contains("- 最大轮次：不限制"));
        assert!(!preview.contains("之前的推理摘要"));
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
}
