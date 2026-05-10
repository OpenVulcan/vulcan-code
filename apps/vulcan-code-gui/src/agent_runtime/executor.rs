//! Execution-preparation models and real provider execution helpers for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备模型与真实 Provider 执行辅助逻辑。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider::{ProviderRuntimeBlueprint, resolve_active_provider_blueprint};
use crate::agent_runtime::rig_tools::build_vulcan_rig_tools;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::provider_config::{
    PersistedProviderOptions, ProviderConfigurationFile, ProviderRuntimeKind,
};
use crate::runtime_assets;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use async_stream::stream;
use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use rig::agent::{AgentBuilder, MultiTurnStreamItem};
use rig::message::ReasoningContent;
use rig::prelude::CompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingPrompt};
use rig::tool::ToolDyn;
use serde_json::Value;
use std::pin::Pin;
use std::{fs, path::Path};
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

/// Lightweight transcript row forwarded into the real runtime as model-facing context.
/// 作为面向模型上下文转发到真实运行时的轻量转录行。
#[derive(Debug, Clone)]
pub struct ExecutionContextMessage {
    /// Persisted message role label.
    /// 持久化消息角色标签。
    pub role: String,
    /// Human-readable message title.
    /// 人类可读消息标题。
    pub title: String,
    /// Primary message body content.
    /// 主要消息正文内容。
    pub body: String,
    /// Durable record-kind label.
    /// 持久化记录类型标签。
    pub record_kind: String,
}

/// Minimal execution request captured from the current GUI state.
/// 从当前 GUI 状态捕获的最小执行请求。
#[derive(Debug, Clone)]
pub struct ExecutionPreparationRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: usize,
    /// Target session title.
    /// 目标会话标题。
    pub session_title: String,
    /// Current workspace path.
    /// 当前工作区路径。
    pub workspace_path: String,
    /// Current composer prompt.
    /// 当前提示词输入区内容。
    pub prompt: String,
    /// Model-facing context history reconstructed from the full transcript or the latest compaction checkpoint.
    /// 从完整转录或最近一次压缩检查点重建出的面向模型上下文历史。
    pub context_history: Vec<ExecutionContextMessage>,
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
    pub session_id: usize,
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
    pub session_id: usize,
    /// Prepared execution snapshot when preparation succeeded.
    /// 当准备阶段成功时生成的执行准备快照。
    pub preparation: Option<ExecutionPreparation>,
    /// Assistant output returned by the real provider call.
    /// 真实 Provider 调用返回的助手输出。
    pub assistant_output: Option<String>,
    /// Assistant thinking output returned by the real provider call.
    /// 真实 Provider 调用返回的助手思考输出。
    pub thinking_output: Option<String>,
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
        session_id: usize,
        preparation: ExecutionPreparation,
        thinking_output: Option<String>,
        assistant_output: String,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            session_id,
            preparation: Some(preparation),
            assistant_output: Some(assistant_output),
            thinking_output,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: None,
        }
    }

    /// Creates one failure report.
    /// 创建一份失败执行报告。
    fn failure(
        session_id: usize,
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
        context_history_count: request.context_history.len(),
        execution_mode: request.execution_mode.clone(),
        temperature: request.temperature,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors: resolved_tools.descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: foundation.enabled_mcp_server_rows(),
        warnings,
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
        let preparation = match prepare_execution(
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

        yield ExecutionStreamEvent::Prepared(preparation.clone());

        let active_agent = foundation.active_agent_definition().cloned();
        let active_agent_tool_id = active_agent
            .as_ref()
            .map(|agent| agent.id.clone())
            .unwrap_or_else(|| preparation.agent_id.clone());
        let tools_manager = build_vulcan_tools_manager(
            foundation.native_tool_catalog().clone(),
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
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

        let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
            collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
        warnings.extend(discovery_warnings);

        for event in discovery_events {
            runtime_events.push(event.clone());
            yield ExecutionStreamEvent::RuntimeEvent {
                session_id: request.session_id,
                event,
            };
        }

        let system_preamble =
            build_runtime_system_preamble(&preparation, active_agent.as_ref(), &discovery_rows);
        let effective_prompt =
            build_runtime_user_prompt(&request.context_history, request.prompt.trim());

        let mut provider_stream = execute_provider_prompt_stream(
            &config,
            &preparation,
            active_agent.as_ref(),
            &system_preamble,
            &effective_prompt,
            tools_manager,
            resolved_tools.definitions,
        );

        let mut terminal_error = None;

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
                    if !thinking_completed_emitted && !thinking_visible_output.trim().is_empty() {
                        thinking_completed_emitted = true;
                        yield ExecutionStreamEvent::ThinkingCompleted {
                            session_id: request.session_id,
                        };
                    }
                    assistant_output.push_str(&delta);
                    yield ExecutionStreamEvent::OutputDelta {
                        session_id: request.session_id,
                        delta,
                    };
                }
                Ok(ProviderStreamDelta::RuntimeEvent(event)) => {
                    runtime_events.push(event.clone());
                    yield ExecutionStreamEvent::RuntimeEvent {
                        session_id: request.session_id,
                        event,
                    };
                }
                Err(error) => {
                    terminal_error = Some(error);
                    break;
                }
            }
        }

        if let Some(error) = terminal_error {
            if !thinking_completed_emitted && !thinking_visible_output.trim().is_empty() {
                yield ExecutionStreamEvent::ThinkingCompleted {
                    session_id: request.session_id,
                };
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

        if !thinking_completed_emitted && !thinking_visible_output.trim().is_empty() {
            yield ExecutionStreamEvent::ThinkingCompleted {
                session_id: request.session_id,
            };
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
        runtime_events.push(RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "MCP 运行时",
            "当前没有可用的 MCP 运行时句柄，本次执行将跳过 MCP 工具注册。",
        ));
        return (runtime_events, rows, errors, warnings);
    };
    let snapshot = runtime.snapshot();

    if snapshot.servers.is_empty() {
        runtime_events.push(RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "MCP 运行时",
            "当前没有已配置的 MCP 服务器，本次执行将跳过 MCP 工具注册。",
        ));
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
fn build_runtime_system_preamble(
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    discovery_rows: &[String],
) -> String {
    let agent_prompt = active_agent
        .map(|agent| agent.system_prompt.trim())
        .filter(|prompt| !prompt.is_empty())
        .map(str::to_string);
    let agent_description = active_agent
        .map(|agent| agent.description.trim())
        .filter(|description| !description.is_empty())
        .unwrap_or("负责根据当前工作区与会话上下文完成桌面 AI 编码任务。");

    let mut sections = Vec::new();
    sections.push(
        agent_prompt
            .or_else(|| load_provider_base_system_prompt(&preparation.model_blueprint))
            .unwrap_or_else(|| String::from("You are VulcanCode, an AI coding agent.")),
    );
    sections.push(format!("代理说明：{agent_description}"));
    sections.push(format!("当前工作区：{}", preparation.workspace_path));
    sections.push(format!("当前执行模式：{}", preparation.execution_mode));

    if !preparation.skill_names.is_empty() {
        sections.push(format!("绑定技能：{}", preparation.skill_names.join("、")));
    }

    let native_tool_names =
        tool_names_for_source(&preparation.tool_descriptors, VulcanToolSourceKind::Native);
    if !native_tool_names.is_empty() {
        sections.push(format!(
            "原生工具已真实注册到本次模型调用：{}。调用时以工具 schema 为准，不要在文本中猜测参数或返回格式。",
            native_tool_names.join("、")
        ));
    }

    if let Some(luaskills_section) =
        render_luaskills_tool_prompt_section(&preparation.tool_descriptors)
    {
        sections.push(luaskills_section);
    }

    let mcp_tool_names =
        tool_names_for_source(&preparation.tool_descriptors, VulcanToolSourceKind::Mcp);
    if !mcp_tool_names.is_empty() {
        sections.push(format!(
            "MCP 工具已真实注册到本次模型调用：{}。调用时以工具 schema 为准，不要在文本中猜测参数或返回格式。",
            mcp_tool_names.join("、")
        ));
    }

    if !preparation.delegate_agents.is_empty() {
        sections.push(format!(
            "可委派代理：{}。当前阶段仅注入协作上下文，不会自动触发真实子代理调度。",
            preparation.delegate_agents.join("、")
        ));
    }

    if !mcp_tool_names.is_empty() && !discovery_rows.is_empty() {
        sections.push(String::from(
            "当前已连接的 MCP 服务器所暴露工具也已直接注册；若服务器断开，其工具会自动撤销。",
        ));
    }

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

/// Render the LuaSkills runtime tool table in the host prompt format expected by the active AI agent.
/// 按当前 AI 代理预期的宿主提示词格式渲染 LuaSkills 运行时工具表。
fn render_luaskills_tool_prompt_section(descriptors: &[VulcanToolDescriptor]) -> Option<String> {
    let luaskills_descriptors = descriptors
        .iter()
        .filter(|descriptor| descriptor.source == VulcanToolSourceKind::LuaSkills)
        .collect::<Vec<_>>();
    if luaskills_descriptors.is_empty() {
        return None;
    }

    let rows = luaskills_descriptors
        .iter()
        .map(|descriptor| format!("- `{}`：{}", descriptor.name, descriptor.description))
        .collect::<Vec<_>>()
        .join("\n");

    Some(format!(
        "LuaSkills 工具目录（以下工具已真实注册到本次模型调用，调用时以工具 schema 为准，参数与返回结构不要依赖文本猜测）：\n{}",
        rows
    ))
}

/// Collects all tool display names belonging to one unified tool source.
/// 收集属于单个统一工具来源的全部工具展示名称。
fn tool_names_for_source(
    descriptors: &[VulcanToolDescriptor],
    source: VulcanToolSourceKind,
) -> Vec<String> {
    descriptors
        .iter()
        .filter(|descriptor| descriptor.source == source)
        .map(|descriptor| descriptor.name.clone())
        .collect()
}

/// Builds the effective user prompt by prepending the reconstructed session context to the latest user input.
/// 通过在最新用户输入前拼接重建后的会话上下文来构建生效用户提示词。
fn build_runtime_user_prompt(context_history: &[ExecutionContextMessage], prompt: &str) -> String {
    if context_history.is_empty() {
        return prompt.to_string();
    }

    let context_block = context_history
        .iter()
        .map(|message| {
            format!(
                "[{}|{}|{}]\n{}",
                message.role, message.record_kind, message.title, message.body
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "以下是当前会话在最近一次上下文压缩锚点之后需要保留的历史上下文，请结合这些上下文继续完成本次请求。\n\n{}\n\n本轮最新用户请求如下：\n{}",
        context_block, prompt
    )
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
    let build_runtime_tools = || {
        build_vulcan_rig_tools(
            tools_manager.clone(),
            preparation.session_id,
            &resolved_tools,
        )
    };

    match preparation.model_blueprint.runtime_kind {
        ProviderRuntimeKind::OpenAi => execute_openai_responses_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &request_headers,
            build_runtime_tools(),
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
            temperature,
            &request_headers,
            build_runtime_tools(),
            additional_params,
        ),
        ProviderRuntimeKind::OpenRouter => execute_openrouter_stream(
            &api_key,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &request_headers,
            build_runtime_tools(),
            additional_params,
        ),
        ProviderRuntimeKind::Anthropic => execute_anthropic_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &request_headers,
            build_runtime_tools(),
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

/// Executes one OpenAI-compatible prompt using the chat-completions compatible rig client and streams deltas.
/// 使用兼容 Chat Completions 的 rig 客户端执行一次 OpenAI 兼容提示词调用并流式返回增量。
fn execute_openai_compatible_stream(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
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
        let mut stream = agent.stream_prompt(prompt).await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("OpenAI 兼容流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item) {
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
) -> Vec<ProviderStreamDelta> {
    match item {
        MultiTurnStreamItem::StreamAssistantItem(content) => {
            provider_deltas_from_assistant_content(content)
        }
        MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
            tool_result,
            ..
        }) => {
            vec![ProviderStreamDelta::RuntimeEvent(RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                format!("工具返回 · {}", tool_result.id),
                format!(
                    "工具 `{}` 已返回 {} 项结果。",
                    tool_result.id,
                    tool_result.content.iter().count()
                ),
            ))]
        }
        MultiTurnStreamItem::FinalResponse(_) => Vec::new(),
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
            vec![ProviderStreamDelta::RuntimeEvent(RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                format!("工具调用 · {}", tool_call.function.name),
                format!(
                    "模型请求调用工具 `{}`，参数：{}。",
                    tool_call.function.name,
                    truncate_preview(&tool_call.function.arguments.to_string(), 160)
                ),
            ))]
        }
        StreamedAssistantContent::ToolCallDelta { id, content, .. } => {
            vec![ProviderStreamDelta::RuntimeEvent(RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                format!("工具增量 · {id}"),
                format!("模型正在补充工具 `{id}` 的参数片段：{:?}。", content),
            ))]
        }
        StreamedAssistantContent::Final(_) => Vec::new(),
    }
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

/// Executes one OpenAI Responses prompt using rig's default OpenAI responses client and streams deltas.
/// 使用 rig 默认的 OpenAI Responses 客户端执行一次 OpenAI Responses 提示词调用并流式返回增量。
fn execute_openai_responses_stream(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
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
        let mut stream = agent.stream_prompt(prompt).await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("OpenAI Responses 流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item) {
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
    temperature: Option<f64>,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
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
        let mut stream = agent.stream_prompt(prompt).await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("Anthropic 兼容流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item) {
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
    temperature: Option<f64>,
    headers: &HeaderMap,
    luaskills_rig_tools: Vec<Box<dyn ToolDyn>>,
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
        let mut stream = agent.stream_prompt(prompt).await;

        while let Some(item) = stream.next().await {
            let item = item.map_err(|error| format!("OpenRouter 流式执行失败：{}。", error));
            match item {
                Ok(item) => {
                    for delta in provider_deltas_from_multi_turn_item(item) {
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

#[cfg(test)]
mod tests {
    use super::{
        SyntheticThinkingPreambleState, classify_synthetic_thinking_preamble,
        normalize_streamed_thinking_output,
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
}
