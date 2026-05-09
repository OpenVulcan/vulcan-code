//! Execution-preparation models and real provider execution helpers for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备模型与真实 Provider 执行辅助逻辑。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider::{ProviderRuntimeBlueprint, resolve_active_provider_blueprint};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::mcp::McpServerRegistry;
use crate::provider_config::{
    PersistedProviderOptions, ProviderConfigurationFile, ProviderRuntimeKind,
};
use async_stream::stream;
use futures_util::{Stream, StreamExt};
use reqwest::header::{
    AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, USER_AGENT,
};
use rig::agent::{AgentBuilder, MultiTurnStreamItem};
use rig::message::ReasoningContent;
use rig::prelude::CompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingPrompt};
use serde_json::{Value, json};
use std::pin::Pin;

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
    /// Resolved native tool identifiers attached to the active agent.
    /// 附着在当前代理上的原生工具标识集合。
    pub native_tools: Vec<String>,
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

        if !self.native_tools.is_empty() {
            rows.push(format!("原生工具：{}", self.native_tools.join("、")));
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
        native_tools: foundation.resolved_native_tools_for_agent(&active_agent.id),
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
    request: ExecutionPreparationRequest,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let foundation = RuntimeFoundation::build(&config);
        let preparation = match prepare_execution(&foundation, &config, &request) {
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
            discover_enabled_mcp_tools(&config).await;
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

/// Discovers enabled MCP tools and converts the result into UI-facing runtime events.
/// 发现已启用 MCP 工具并将结果转换为面向界面的运行时事件。
async fn discover_enabled_mcp_tools(
    config: &ProviderConfigurationFile,
) -> (Vec<RuntimeEvent>, Vec<String>, Vec<String>, Vec<String>) {
    let registry = McpServerRegistry::from_provider_configuration(config);
    let enabled_servers = registry.enabled_servers();
    let mut runtime_events = Vec::new();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if enabled_servers.is_empty() {
        runtime_events.push(RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "MCP 工具发现",
            "当前没有启用的 MCP 服务器，本次执行将跳过远端工具发现。",
        ));
        return (runtime_events, rows, errors, warnings);
    }

    for server in enabled_servers {
        runtime_events.push(RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            format!("MCP 工具发现 · {}", server.name),
            format!("正在连接 `{}` 并发现可用工具。", server.command_summary()),
        ));

        match registry.discover_tools_for_server(&server.id).await {
            Ok(tools) => {
                let names = tools
                    .into_iter()
                    .map(|tool| tool.name())
                    .collect::<Vec<_>>();
                let summary = if names.is_empty() {
                    format!("服务器 `{}` 未暴露可用工具。", server.name)
                } else {
                    format!("服务器 `{}` 暴露工具：{}。", server.name, names.join("、"))
                };
                rows.push(summary.clone());
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::ToolFinished,
                    format!("MCP 工具发现 · {}", server.name),
                    summary,
                ));
            }
            Err(error) => {
                let summary = format!("MCP 服务器 `{}` 工具发现失败：{}。", server.name, error);
                warnings.push(summary.clone());
                errors.push(summary);
                runtime_events.push(RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    format!("MCP 工具发现失败 · {}", server.name),
                    error,
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
        .unwrap_or("你是 Vulcan Code 的执行代理。");
    let agent_description = active_agent
        .map(|agent| agent.description.trim())
        .filter(|description| !description.is_empty())
        .unwrap_or("负责根据当前工作区与会话上下文完成桌面 AI 编码任务。");

    let mut sections = vec![
        agent_prompt.to_string(),
        format!("代理说明：{agent_description}"),
        format!("当前工作区：{}", preparation.workspace_path),
        format!("当前执行模式：{}", preparation.execution_mode),
    ];

    if !preparation.skill_names.is_empty() {
        sections.push(format!("绑定技能：{}", preparation.skill_names.join("、")));
    }

    if !preparation.native_tools.is_empty() {
        sections.push(format!(
            "原生工具目录：{}。当前阶段仅对真实模型执行链进行接通，工具注册仍在后续阶段继续增强。",
            preparation.native_tools.join("、")
        ));
    }

    if !preparation.delegate_agents.is_empty() {
        sections.push(format!(
            "可委派代理：{}。当前阶段仅注入协作上下文，不会自动触发真实子代理调度。",
            preparation.delegate_agents.join("、")
        ));
    }

    if !discovery_rows.is_empty() {
        sections.push(format!(
            "MCP 发现结果：{}。当前阶段已完成工具发现，但尚未把这些工具真实注册到本次模型调用中。",
            discovery_rows.join("；")
        ));
    }

    sections.join("\n")
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
    let user_agent_preview = request_headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<<unset>>")
        .to_string();

    if is_alibaba_coding_plan_provider(&preparation.model_blueprint.provider_id) {
        return execute_alibaba_coding_plan_stream(
            &preparation.model_blueprint.provider_id,
            &request_headers,
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &user_agent_preview,
        );
    }

    match preparation.model_blueprint.runtime_kind {
        ProviderRuntimeKind::OpenAi => execute_openai_responses_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &request_headers,
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
        ),
        ProviderRuntimeKind::OpenRouter => execute_openrouter_stream(
            &api_key,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &request_headers,
        ),
        ProviderRuntimeKind::Anthropic => execute_anthropic_stream(
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &request_headers,
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

/// Returns whether one provider identifier should use the dedicated Alibaba Coding Plan chat-completions path.
/// 返回某个供应商标识是否应使用专用的阿里百炼 Coding Plan chat-completions 执行路径。
fn is_alibaba_coding_plan_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        "alibaba-coding-plan" | "alibaba-coding-plan-cn"
    )
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

    let agent = agent_builder.build();
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

/// Executes one Alibaba Coding Plan prompt through the exact OpenAI chat-completions wire format and parses SSE deltas.
/// 通过精确的 OpenAI chat-completions 线协议执行一次阿里百炼 Coding Plan 提示词调用并解析 SSE 增量。
fn execute_alibaba_coding_plan_stream(
    provider_id: &str,
    headers: &HeaderMap,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    user_agent_preview: &str,
) -> ProviderDeltaStream {
    let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return one_shot_provider_error_stream(String::from(
            "阿里百炼 Coding Plan 当前缺少可用的接口基址，无法启动真实执行。",
        ));
    };

    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let request_body = build_alibaba_coding_plan_request_body(
        provider_id,
        model_id,
        system_preamble,
        prompt,
        temperature,
        true,
    );
    let request_preview = serde_json::to_string_pretty(&request_body)
        .unwrap_or_else(|_| String::from("{\"error\":\"request-preview-unavailable\"}"));
    let request_header_preview = format!("User-Agent: {user_agent_preview}");
    let headers = headers.clone();
    let api_key = api_key.to_string();
    let endpoint_clone = endpoint.clone();

    Box::pin(stream! {
        let client = reqwest::Client::new();
        let mut request_builder = client
            .post(&endpoint_clone)
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .header(CONTENT_TYPE, "application/json");

        for (header_name, header_value) in &headers {
            request_builder = request_builder.header(header_name, header_value);
        }

        let response = match request_builder.json(&request_body).send().await {
            Ok(response) => response,
            Err(error) => {
                yield Err(format!(
                    "阿里百炼 Coding Plan 请求发送失败：{}。接口：{}。请求头：{}。请求体：{}",
                    error, endpoint, request_header_preview, request_preview
                ));
                return;
            }
        };

        let status = response.status();
        if !status.is_success() {
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<<response-body-unavailable>>"));
            yield Err(format!(
                "阿里百炼 Coding Plan 执行失败：HTTP {}。接口：{}。请求头：{}。请求体：{}。响应：{}",
                status, endpoint, request_header_preview, request_preview, response_text
            ));
            return;
        }

        let mut pending = String::new();
        let mut response = response;

        loop {
            let next_chunk = response.chunk().await;
            let Some(chunk) = (match next_chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(format!(
                        "阿里百炼 Coding Plan 流式响应读取失败：{}。接口：{}。",
                        error, endpoint
                    ));
                    return;
                }
            }) else {
                break;
            };

            pending.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_break_index) = pending.find('\n') {
                let mut line = pending.drain(..=line_break_index).collect::<String>();
                line = line.trim().to_string();
                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }

                let payload = line["data:".len()..].trim();
                if payload == "[DONE]" {
                    break;
                }

                match serde_json::from_str::<Value>(payload) {
                    Ok(json_payload) => {
                        for delta in provider_deltas_from_alibaba_chunk(&json_payload) {
                            yield Ok(delta);
                        }
                    }
                    Err(error) => {
                        yield Err(format!(
                            "阿里百炼 Coding Plan 返回了无法解析的流式 JSON：{}。接口：{}。原始片段：{}",
                            error, endpoint, payload
                        ));
                        return;
                    }
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

/// Builds the exact chat-completions payload used for Alibaba Coding Plan.
/// 构建阿里百炼 Coding Plan 使用的精确 chat-completions 请求体。
fn build_alibaba_coding_plan_request_body(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    stream: bool,
) -> Value {
    let mut body = json!({
        "model": model_id,
        "messages": [
            {
                "role": "system",
                "content": system_preamble
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "stream": stream,
    });

    if let Some(temperature) = temperature {
        body["temperature"] = json!(temperature);
    }

    if provider_id == "alibaba-coding-plan-cn"
        && model_supports_dashscope_thinking(model_id)
        && !model_id.to_ascii_lowercase().contains("kimi-k2-thinking")
    {
        body["enable_thinking"] = Value::Bool(true);
    }

    body
}

/// Returns whether one Alibaba model should request DashScope reasoning output explicitly.
/// 返回某个阿里模型是否应显式请求 DashScope 推理输出。
fn model_supports_dashscope_thinking(model_id: &str) -> bool {
    let normalized = model_id.to_ascii_lowercase();

    normalized.contains("qwen")
        || normalized.contains("kimi")
        || normalized.contains("deepseek")
        || normalized.contains("glm")
        || normalized.contains("qwq")
        || normalized.contains("minimax")
}

/// Extracts OpenAI-compatible message content from either a string or structured content array.
/// 从字符串或结构化内容数组中提取 OpenAI 兼容消息内容。
fn extract_openai_message_content(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| match part {
                Value::Object(map) => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
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

/// Extracts zero or more thinking/output deltas from one Alibaba Coding Plan streaming chunk.
/// 从一条阿里百炼 Coding Plan 流式数据块中提取零条或多条思考或输出增量。
fn provider_deltas_from_alibaba_chunk(chunk: &Value) -> Vec<ProviderStreamDelta> {
    let Some(first_choice) = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return Vec::new();
    };
    let Some(delta) = first_choice.get("delta") else {
        return Vec::new();
    };

    let mut deltas = Vec::new();

    for key in ["reasoning_content", "reasoning"] {
        if let Some(reasoning) = delta.get(key).and_then(Value::as_str) {
            if !reasoning.is_empty() {
                deltas.push(ProviderStreamDelta::Thinking(reasoning.to_string()));
            }
        }
    }

    if let Some(content_delta) = delta.get("content") {
        let flattened = extract_openai_message_content(content_delta);
        if !flattened.is_empty() {
            deltas.push(ProviderStreamDelta::Output(flattened));
        }
    }

    deltas
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

    let agent = agent_builder.build();
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

    let agent = agent_builder.build();
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

    let agent = agent_builder.build();
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
        ProviderStreamDelta, SyntheticThinkingPreambleState, classify_synthetic_thinking_preamble,
        normalize_streamed_thinking_output, provider_deltas_from_alibaba_chunk,
    };
    use serde_json::json;

    /// Verifies Alibaba content deltas preserve leading and trailing newlines so streamed Markdown code fences do not glue lines together.
    /// 验证阿里百炼内容增量会保留首尾换行，从而避免流式 Markdown 代码围栏与下一行黏连。
    #[test]
    fn alibaba_output_deltas_preserve_newline_boundaries() {
        let chunk = json!({
            "choices": [
                {
                    "delta": {
                        "content": "\n```lua\n"
                    }
                }
            ]
        });

        let deltas = provider_deltas_from_alibaba_chunk(&chunk);

        assert!(matches!(
            deltas.as_slice(),
            [ProviderStreamDelta::Output(delta)] if delta == "\n```lua\n"
        ));
    }

    /// Verifies Alibaba content deltas preserve whitespace-only fragments when they carry meaningful indentation inside streamed code blocks.
    /// 验证阿里百炼内容增量在承载代码块缩进时会保留仅含空白的片段。
    #[test]
    fn alibaba_output_deltas_preserve_whitespace_only_fragments() {
        let chunk = json!({
            "choices": [
                {
                    "delta": {
                        "content": "    "
                    }
                }
            ]
        });

        let deltas = provider_deltas_from_alibaba_chunk(&chunk);

        assert!(matches!(
            deltas.as_slice(),
            [ProviderStreamDelta::Output(delta)] if delta == "    "
        ));
    }

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
