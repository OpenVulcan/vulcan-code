//! Execution-preparation models and real provider execution helpers for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备模型与真实 Provider 执行辅助逻辑。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider::{ProviderRuntimeBlueprint, resolve_active_provider_blueprint};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::mcp::McpServerRegistry;
use crate::provider_config::{
    PersistedProviderOptions, ProviderConfigurationFile, ProviderRuntimeKind,
};
use reqwest::header::{
    AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, USER_AGENT,
};
use rig::agent::AgentBuilder;
use rig::completion::Prompt;
use rig::prelude::CompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use serde_json::{Value, json};

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
    /// Runtime events collected during MCP discovery and provider execution.
    /// 在 MCP 发现与 Provider 执行期间收集到的运行时事件集合。
    pub runtime_events: Vec<RuntimeEvent>,
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
        assistant_output: String,
        runtime_events: Vec<RuntimeEvent>,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            session_id,
            preparation: Some(preparation),
            assistant_output: Some(assistant_output),
            runtime_events,
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
        runtime_events: Vec<RuntimeEvent>,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            preparation,
            assistant_output: None,
            runtime_events,
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
        execution_mode: request.execution_mode.clone(),
        temperature: request.temperature,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        native_tools: foundation.resolved_native_tools_for_agent(&active_agent.id),
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: foundation.enabled_mcp_server_rows(),
        warnings,
    })
}

/// Executes one real provider run and collects MCP discovery events for the current runtime state.
/// 基于当前运行时状态执行一次真实 Provider 调用，并收集 MCP 发现事件。
pub async fn execute_runtime_run(
    config: ProviderConfigurationFile,
    request: ExecutionPreparationRequest,
) -> ExecutionRunReport {
    let foundation = RuntimeFoundation::build(&config);
    let preparation = match prepare_execution(&foundation, &config, &request) {
        Ok(preparation) => preparation,
        Err(error) => {
            return ExecutionRunReport::failure(
                request.session_id,
                None,
                vec![RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    "执行准备失败",
                    error.clone(),
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                error,
            );
        }
    };

    let active_agent = foundation.active_agent_definition().cloned();
    let mut runtime_events = Vec::new();
    let mut warnings = preparation.warnings.clone();

    runtime_events.push(RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "执行上下文",
        format!(
            "已进入真实执行阶段，目标模型为 `{}`，工作区为 `{}`。",
            preparation.model_blueprint.model_name, preparation.workspace_path
        ),
    ));

    let (discovery_rows, discovery_errors) =
        discover_enabled_mcp_tools(&config, &mut runtime_events, &mut warnings).await;

    let system_preamble =
        build_runtime_system_preamble(&preparation, active_agent.as_ref(), &discovery_rows);

    match execute_provider_prompt(
        &config,
        &preparation,
        active_agent.as_ref(),
        &system_preamble,
        request.prompt.trim(),
    )
    .await
    {
        Ok(output) => {
            runtime_events.push(RuntimeEvent::new(
                RuntimeEventKind::TokenDelta,
                "模型输出完成",
                format!(
                    "真实 Provider 已返回 {} 个字符的输出内容。",
                    output.chars().count()
                ),
            ));
            ExecutionRunReport::success(
                request.session_id,
                preparation,
                normalize_assistant_output(output),
                runtime_events,
                discovery_rows,
                discovery_errors,
                warnings,
            )
        }
        Err(error) => {
            runtime_events.push(RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "真实执行失败",
                error.clone(),
            ));
            ExecutionRunReport::failure(
                request.session_id,
                Some(preparation),
                runtime_events,
                discovery_rows,
                discovery_errors,
                warnings,
                error,
            )
        }
    }
}

/// Discovers enabled MCP tools and converts the result into UI-facing runtime events.
/// 发现已启用 MCP 工具并将结果转换为面向界面的运行时事件。
async fn discover_enabled_mcp_tools(
    config: &ProviderConfigurationFile,
    runtime_events: &mut Vec<RuntimeEvent>,
    warnings: &mut Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let registry = McpServerRegistry::from_provider_configuration(config);
    let enabled_servers = registry.enabled_servers();
    let mut rows = Vec::new();
    let mut errors = Vec::new();

    if enabled_servers.is_empty() {
        runtime_events.push(RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "MCP 工具发现",
            "当前没有启用的 MCP 服务器，本次执行将跳过远端工具发现。",
        ));
        return (rows, errors);
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

    (rows, errors)
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

/// Executes one provider prompt against the currently selected runtime family.
/// 针对当前选中的运行时族执行一次 Provider 提示词调用。
async fn execute_provider_prompt(
    config: &ProviderConfigurationFile,
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    system_preamble: &str,
    prompt: &str,
) -> Result<String, String> {
    let options = resolve_provider_options(config, &preparation.model_blueprint.provider_id)?;
    let api_key = options.resolved_api_key()?.ok_or_else(|| {
        format!(
            "供应商 `{}` 当前缺少可用 API Key，无法启动真实执行。",
            preparation.model_blueprint.provider_name
        )
    })?;
    let temperature = active_agent
        .and_then(|agent| agent.temperature)
        .or(preparation.temperature)
        .map(|value| value.clamp(0.0, 2.0) as f64);
    let model_id = preparation.model_blueprint.model_id.as_str();
    let base_url = preparation.model_blueprint.base_url.as_deref();
    let request_headers = build_provider_request_headers(&preparation.model_blueprint)?;
    let user_agent_preview = request_headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<<unset>>")
        .to_string();

    if is_alibaba_coding_plan_provider(&preparation.model_blueprint.provider_id) {
        return execute_alibaba_coding_plan_prompt(
            &preparation.model_blueprint.provider_id,
            &request_headers,
            &api_key,
            base_url,
            model_id,
            system_preamble,
            prompt,
            temperature,
            &user_agent_preview,
        )
        .await;
    }

    match preparation.model_blueprint.runtime_kind {
        ProviderRuntimeKind::OpenAi => {
            execute_openai_responses_prompt(
                &api_key,
                base_url,
                model_id,
                system_preamble,
                prompt,
                temperature,
                &request_headers,
            )
            .await
        }
        ProviderRuntimeKind::OpenAiCompatible
        | ProviderRuntimeKind::XAi
        | ProviderRuntimeKind::Mistral
        | ProviderRuntimeKind::Groq
        | ProviderRuntimeKind::TogetherAi
        | ProviderRuntimeKind::Perplexity => {
            execute_openai_compatible_prompt(
                &api_key,
                base_url,
                model_id,
                system_preamble,
                prompt,
                temperature,
                &request_headers,
            )
            .await
        }
        ProviderRuntimeKind::OpenRouter => {
            execute_openrouter_prompt(
                &api_key,
                model_id,
                system_preamble,
                prompt,
                temperature,
                &request_headers,
            )
            .await
        }
        ProviderRuntimeKind::Anthropic => {
            execute_anthropic_prompt(
                &api_key,
                base_url,
                model_id,
                system_preamble,
                prompt,
                temperature,
                &request_headers,
            )
            .await
        }
        ProviderRuntimeKind::AzureOpenAi
        | ProviderRuntimeKind::GoogleGemini
        | ProviderRuntimeKind::Cohere => Err(format!(
            "当前阶段尚未接通 `{}` 的真实执行链路，请优先选择 OpenAI 兼容接口、百炼 Coding Plan 或 OpenRouter。",
            preparation.model_blueprint.runtime_label
        )),
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

/// Executes one OpenAI-compatible prompt using the chat-completions compatible rig client.
/// 使用兼容 Chat Completions 的 rig 客户端执行一次 OpenAI 兼容提示词调用。
async fn execute_openai_compatible_prompt(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    headers: &HeaderMap,
) -> Result<String, String> {
    let mut builder = openai::CompletionsClient::builder()
        .api_key(api_key)
        .http_headers(headers.clone());

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = builder
        .build()
        .map_err(|error| format!("创建 OpenAI 兼容客户端失败：{}。", error))?;
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code Worker")
        .preamble(system_preamble);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }

    let agent = agent_builder.build();

    agent
        .prompt(prompt.to_string())
        .await
        .map_err(|error| format!("OpenAI 兼容执行失败：{}。", error))
}

/// Executes one Alibaba Coding Plan prompt through the exact OpenAI chat-completions wire format.
/// 通过精确的 OpenAI chat-completions 线协议执行一次阿里百炼 Coding Plan 提示词调用。
async fn execute_alibaba_coding_plan_prompt(
    provider_id: &str,
    headers: &HeaderMap,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    user_agent_preview: &str,
) -> Result<String, String> {
    let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(String::from(
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
    );
    let request_preview = serde_json::to_string_pretty(&request_body)
        .unwrap_or_else(|_| String::from("{\"error\":\"request-preview-unavailable\"}"));
    let request_header_preview = format!("User-Agent: {user_agent_preview}");

    let client = reqwest::Client::new();
    let mut request_builder = client
        .post(&endpoint)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json");

    for (header_name, header_value) in headers {
        request_builder = request_builder.header(header_name, header_value);
    }

    let response = request_builder
        .json(&request_body)
        .send()
        .await
        .map_err(|error| {
            format!(
                "阿里百炼 Coding Plan 请求发送失败：{}。接口：{}。请求头：{}。请求体：{}",
                error, endpoint, request_header_preview, request_preview
            )
        })?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .unwrap_or_else(|_| String::from("<<response-body-unavailable>>"));

    if !status.is_success() {
        return Err(format!(
            "阿里百炼 Coding Plan 执行失败：HTTP {}。接口：{}。请求头：{}。请求体：{}。响应：{}",
            status, endpoint, request_header_preview, request_preview, response_text
        ));
    }

    let response_json = serde_json::from_str::<Value>(&response_text).map_err(|error| {
        format!(
            "阿里百炼 Coding Plan 返回了无法解析的 JSON：{}。接口：{}。原始响应：{}",
            error, endpoint, response_text
        )
    })?;

    extract_alibaba_coding_plan_output(&response_json).ok_or_else(|| {
        format!(
            "阿里百炼 Coding Plan 返回成功，但未提取到可见输出。接口：{}。响应：{}",
            endpoint, response_text
        )
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

/// Extracts visible assistant output from one Alibaba Coding Plan chat-completions response.
/// 从一份阿里百炼 Coding Plan chat-completions 响应中提取可见助手输出。
fn extract_alibaba_coding_plan_output(response_json: &Value) -> Option<String> {
    let first_choice = response_json.get("choices")?.as_array()?.first()?;
    let message = first_choice.get("message")?;
    let reasoning = message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let content = extract_openai_message_content(message.get("content")?)
        .trim()
        .to_string();

    match (reasoning, content.is_empty()) {
        (Some(reasoning), false) => Some(format!("{reasoning}\n\n{content}")),
        (Some(reasoning), true) => Some(reasoning),
        (None, false) => Some(content),
        (None, true) => None,
    }
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

/// Executes one OpenAI Responses prompt using rig's default OpenAI responses client.
/// 使用 rig 默认的 OpenAI Responses 客户端执行一次 OpenAI Responses 提示词调用。
async fn execute_openai_responses_prompt(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    headers: &HeaderMap,
) -> Result<String, String> {
    let mut builder = openai::Client::builder()
        .api_key(api_key)
        .http_headers(headers.clone());

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = builder
        .build()
        .map_err(|error| format!("创建 OpenAI Responses 客户端失败：{}。", error))?;
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code Responses Worker")
        .preamble(system_preamble);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }

    let agent = agent_builder.build();

    agent
        .prompt(prompt.to_string())
        .await
        .map_err(|error| format!("OpenAI Responses 执行失败：{}。", error))
}

/// Executes one Anthropic-compatible prompt using rig's native Anthropic client.
/// 使用 rig 原生 Anthropic 客户端执行一次 Anthropic 兼容提示词调用。
async fn execute_anthropic_prompt(
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    headers: &HeaderMap,
) -> Result<String, String> {
    let mut builder = anthropic::Client::builder()
        .api_key(api_key)
        .http_headers(headers.clone());

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = builder
        .build()
        .map_err(|error| format!("创建 Anthropic 兼容客户端失败：{}。", error))?;
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code Anthropic Worker")
        .preamble(system_preamble);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }

    let agent = agent_builder.build();

    agent
        .prompt(prompt.to_string())
        .await
        .map_err(|error| format!("Anthropic 兼容执行失败：{}。", error))
}

/// Executes one OpenRouter prompt using the native OpenRouter rig client.
/// 使用原生 OpenRouter rig 客户端执行一次 OpenRouter 提示词调用。
async fn execute_openrouter_prompt(
    api_key: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    headers: &HeaderMap,
) -> Result<String, String> {
    let client = openrouter::Client::builder()
        .api_key(api_key)
        .http_headers(headers.clone())
        .build()
        .map_err(|error| format!("创建 OpenRouter 客户端失败：{}。", error))?;
    let model = client.completion_model(model_id);
    let mut agent_builder = AgentBuilder::new(model)
        .name("Vulcan Code OpenRouter Worker")
        .preamble(system_preamble);

    if let Some(temperature) = temperature {
        agent_builder = agent_builder.temperature(temperature);
    }

    let agent = agent_builder.build();

    agent
        .prompt(prompt.to_string())
        .await
        .map_err(|error| format!("OpenRouter 执行失败：{}。", error))
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
