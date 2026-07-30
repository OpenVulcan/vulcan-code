//! Provider-native tool request projection and unified output decoding.
//! 供应商原生工具请求投影与统一输出解码。

use crate::provider_task::{
    ProviderAsyncTaskPoll, ProviderAsyncTaskRecoveryContext, ProviderAsyncTaskStart,
};
use crate::{
    AuthenticationProjection, CredentialExecutionSecret, ProviderToolAdapterKind,
    ProviderToolExecutionResult, ProviderToolExtractFailure, ProviderToolExtractOutput,
    ProviderToolExtractResult, ProviderToolGeneratedResource, ProviderToolNormalizedOutput,
    ProviderToolSearchOutput, ProviderToolSearchResult, ProviderToolTarget, ServiceEntry,
};
use base64::Engine;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum provider tool metadata response accepted before typed decoding.
/// 强类型解码前允许接收的供应商工具元数据响应上限。
const PROVIDER_TOOL_RESPONSE_BYTE_LIMIT: usize = 8 * 1024 * 1024;

/// Resolves the exact native endpoint for one frozen provider tool target.
/// 为一个冻结供应商工具目标解析精确原生端点。
///
/// # Parameters
/// 参数
/// `entry` owns the selected account while `target` may select a native origin distinct from
/// the conversation protocol root.
/// `entry` 持有所选账号，`target` 可以选择与会话协议根地址不同的原生源地址。
///
/// # Returns
/// 返回值
/// The absolute provider tool URL.
/// 返回绝对供应商工具 URL。
pub(crate) fn provider_tool_url(entry: &ServiceEntry, target: &ProviderToolTarget) -> String {
    let base_url = target
        .execution
        .endpoint_base_url
        .as_deref()
        .unwrap_or(entry.base_url.as_str());
    format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        target.execution.endpoint_path
    )
}

/// Reads one provider tool response through a shared bounded stream.
/// 通过共享有界流读取一次供应商工具响应。
///
/// # Parameters
/// 参数
/// `response` owns the unread response body and `operation` scopes diagnostics.
/// `response` 持有尚未读取的响应正文，`operation` 限定诊断信息。
///
/// # Returns
/// 返回值
/// UTF-8 response text no larger than the provider-tool metadata limit.
/// 返回不超过供应商工具元数据上限的 UTF-8 响应文本。
pub(crate) async fn read_bounded_provider_response(
    response: reqwest::Response,
    operation: &str,
) -> Result<String, String> {
    if response
        .content_length()
        .is_some_and(|length| length > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT as u64)
    {
        return Err(format!(
            "{operation} response exceeds the 8 MiB metadata limit."
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("{operation} response could not be read: {error}"))?;
        if bytes.len().saturating_add(chunk.len()) > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT {
            return Err(format!(
                "{operation} response exceeds the 8 MiB metadata limit."
            ));
        }
        bytes.extend_from_slice(chunk.as_ref());
    }
    String::from_utf8(bytes).map_err(|_| format!("{operation} response is not valid UTF-8 text."))
}

/// Executes one exact provider tool adapter.
/// 执行一个精确的供应商工具适配器。
///
/// # Parameters
/// 参数
/// `http`, `entry`, `target`, `arguments`, and `secret` are frozen for this invocation.
/// `http`、`entry`、`target`、`arguments` 与 `secret` 在当前调用期间保持冻结。
///
/// # Returns
/// 返回值
/// A unified typed result or an explicit provider error.
/// 返回统一强类型结果或明确的供应商错误。
pub(crate) async fn execute_provider_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    match target.execution.adapter {
        ProviderToolAdapterKind::MiniMaxImageGenerate => {
            execute_minimax_image_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::MiniMaxCodingPlanVision => {
            execute_minimax_vision_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::MiniMaxSpeechSynthesize => {
            execute_minimax_speech_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::MiniMaxMusicGenerate => {
            execute_minimax_music_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::MiniMaxVideoGenerate => {
            execute_minimax_video_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::MiniMaxCodingPlanSearch => {
            execute_minimax_search_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaModelStudioSearch => {
            execute_alibaba_search_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaQwenImageGenerate
        | ProviderToolAdapterKind::AlibabaQwenImageEdit => {
            execute_alibaba_qwen_image_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaWanImageGenerate => {
            execute_alibaba_wan_image_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaCosyVoiceSynthesize => {
            execute_alibaba_cosyvoice_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaQwenSpeechSynthesize => {
            execute_alibaba_qwen_tts_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaQwenSpeechTranscribe => {
            execute_alibaba_qwen_asr_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::AlibabaFunAsrTranscribe => {
            crate::alibaba_provider_tools::execute_alibaba_fun_asr_tool(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaQwenOmniMediaAnalyze => {
            crate::alibaba_provider_tools::execute_alibaba_qwen_omni_media_tool(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate
        | ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit => {
            crate::alibaba_provider_tools::execute_alibaba_happyhorse_video_tool(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaWanVideoGenerate => {
            crate::alibaba_provider_tools::execute_alibaba_wan_video_tool(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaFunMusicGenerate => {
            crate::alibaba_provider_tools::execute_alibaba_fun_music_tool(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::TavilySearch => {
            execute_tavily_search_tool(http, entry, target, arguments, secret).await
        }
        ProviderToolAdapterKind::TavilyExtract => {
            execute_tavily_extract_tool(http, entry, target, arguments, secret).await
        }
    }
}

/// Reports whether one adapter owns a provider-confirmed asynchronous task lifecycle.
/// 报告某个适配器是否拥有供应商确认的异步任务生命周期。
///
/// # Parameters
/// 参数
/// `adapter` is the exact registered provider-tool adapter.
/// `adapter` 是精确已注册供应商工具适配器。
///
/// # Returns
/// 返回值
/// `true` only for adapters with split start and poll implementations.
/// 仅对具有拆分启动与轮询实现的适配器返回 `true`。
pub(crate) fn is_provider_async_task_adapter(adapter: ProviderToolAdapterKind) -> bool {
    matches!(
        adapter,
        ProviderToolAdapterKind::AlibabaFunAsrTranscribe
            | ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate
            | ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit
            | ProviderToolAdapterKind::AlibabaWanVideoGenerate
    )
}

/// Starts one exact provider-native asynchronous task.
/// 启动一个精确供应商原生异步任务。
///
/// # Parameters
/// 参数
/// Exact route, materialized arguments, and credential are frozen before dispatch.
/// 精确路由、已物化参数与凭据在分派前冻结。
///
/// # Returns
/// 返回值
/// A provider-confirmed task identity and non-secret recovery context.
/// 返回供应商确认的任务身份与非秘密恢复上下文。
pub(crate) async fn start_provider_async_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderAsyncTaskStart, String> {
    match target.execution.adapter {
        ProviderToolAdapterKind::AlibabaFunAsrTranscribe => {
            crate::alibaba_provider_tools::start_alibaba_fun_asr_task(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate
        | ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit => {
            crate::alibaba_provider_tools::start_alibaba_happyhorse_video_task(
                http, entry, target, arguments, secret,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaWanVideoGenerate => {
            crate::alibaba_provider_tools::start_alibaba_wan_video_task(
                http, entry, target, arguments, secret,
            )
            .await
        }
        adapter => Err(format!(
            "Provider tool adapter `{adapter:?}` does not expose an asynchronous task lifecycle."
        )),
    }
}

/// Polls one exact provider-native asynchronous task once.
/// 单次轮询一个精确供应商原生异步任务。
///
/// # Parameters
/// 参数
/// Exact route, task identity, recovery context, and credential remain immutable.
/// 精确路由、任务身份、恢复上下文与凭据保持不可变。
///
/// # Returns
/// 返回值
/// A pending phase or a normalized terminal result.
/// 返回等待阶段或归一化终态结果。
pub(crate) async fn poll_provider_async_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    provider_task_id: &str,
    recovery: &ProviderAsyncTaskRecoveryContext,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderAsyncTaskPoll, String> {
    match target.execution.adapter {
        ProviderToolAdapterKind::AlibabaFunAsrTranscribe => {
            crate::alibaba_provider_tools::poll_alibaba_fun_asr_task(
                http,
                entry,
                secret,
                provider_task_id,
                recovery,
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate
        | ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit => {
            crate::alibaba_provider_tools::poll_alibaba_video_task(
                http,
                entry,
                secret,
                provider_task_id,
                "Alibaba HappyHorse",
            )
            .await
        }
        ProviderToolAdapterKind::AlibabaWanVideoGenerate => {
            crate::alibaba_provider_tools::poll_alibaba_video_task(
                http,
                entry,
                secret,
                provider_task_id,
                "Alibaba Wan video",
            )
            .await
        }
        adapter => Err(format!(
            "Provider tool adapter `{adapter:?}` does not expose an asynchronous task lifecycle."
        )),
    }
}

/// Requests cancellation of one exact provider-native asynchronous task.
/// 请求取消一个精确供应商原生异步任务。
///
/// # Parameters
/// 参数
/// Exact route, provider task identity, and credential remain immutable.
/// 精确路由、供应商任务身份与凭据保持不可变。
///
/// # Returns
/// 返回值
/// Success only after provider confirmation; unsupported adapters fail explicitly.
/// 仅在供应商确认后返回成功；不支持的适配器明确失败。
pub(crate) async fn cancel_provider_async_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    provider_task_id: &str,
    secret: &CredentialExecutionSecret,
) -> Result<(), String> {
    let operation = match target.execution.adapter {
        ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate
        | ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit => "Alibaba HappyHorse",
        ProviderToolAdapterKind::AlibabaWanVideoGenerate => "Alibaba Wan video",
        ProviderToolAdapterKind::AlibabaFunAsrTranscribe => {
            return Err(String::from(
                "Alibaba Fun-ASR does not declare a provider cancellation endpoint.",
            ));
        }
        adapter => {
            return Err(format!(
                "Provider tool adapter `{adapter:?}` does not expose an asynchronous task lifecycle."
            ));
        }
    };
    crate::alibaba_provider_tools::cancel_alibaba_task(
        http,
        entry,
        secret,
        provider_task_id,
        operation,
    )
    .await
}

/// Exact MCP protocol revision used by Alibaba Model Studio WebSearch.
/// 阿里云百炼 Model Studio WebSearch 使用的精确 MCP 协议版本。
const ALIBABA_MCP_PROTOCOL_VERSION: &str = "2025-03-26";
/// Exact Alibaba Model Studio WebSearch MCP tool name.
/// 阿里云百炼 Model Studio WebSearch 的精确 MCP 工具名称。
const ALIBABA_WEB_SEARCH_TOOL_NAME: &str = "bailian_web_search";

/// Strict AI-visible Alibaba WebSearch arguments.
/// 严格的 AI 可见阿里云 WebSearch 参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaSearchToolArguments {
    /// Exact search query.
    /// 精确搜索查询。
    query: String,
    /// Optional ordered result limit.
    /// 可选的有序结果数量限制。
    #[serde(default)]
    max_results: Option<u8>,
}

impl AlibabaSearchToolArguments {
    /// Validates the exact Alibaba search argument boundary.
    /// 验证精确的阿里云搜索参数边界。
    ///
    /// # Returns
    /// 返回值
    /// Success when query and optional result count can be projected losslessly.
    /// 当查询与可选结果数能够无损投影时返回成功。
    fn validate(&self) -> Result<(), String> {
        if self.query.trim().is_empty() {
            return Err(String::from("Alibaba WebSearch query cannot be empty."));
        }
        if self
            .max_results
            .is_some_and(|maximum| !(1..=20).contains(&maximum))
        {
            return Err(String::from(
                "Alibaba WebSearch max_results must be between one and twenty.",
            ));
        }
        Ok(())
    }
}

/// One typed JSON-RPC response envelope returned by Alibaba MCP.
/// 阿里云 MCP 返回的一份强类型 JSON-RPC 响应信封。
#[derive(Debug, Deserialize)]
struct AlibabaMcpResponse {
    /// Returned JSON-RPC version.
    /// 返回的 JSON-RPC 版本。
    jsonrpc: String,
    /// Correlation identifier.
    /// 关联标识。
    id: u64,
    /// Method-specific successful result.
    /// 方法专属成功结果。
    #[serde(default)]
    result: Option<Value>,
    /// Optional provider JSON-RPC error.
    /// 可选的供应商 JSON-RPC 错误。
    #[serde(default)]
    error: Option<AlibabaMcpError>,
}

/// Controlled Alibaba MCP JSON-RPC error subset.
/// 受控的阿里云 MCP JSON-RPC 错误子集。
#[derive(Debug, Deserialize)]
struct AlibabaMcpError {
    /// Stable JSON-RPC error code.
    /// 稳定的 JSON-RPC 错误码。
    code: i64,
}

/// Alibaba MCP tools/call result.
/// 阿里云 MCP tools/call 结果。
#[derive(Debug, Deserialize)]
struct AlibabaMcpToolResult {
    /// Ordered content blocks.
    /// 有序内容块。
    content: Vec<AlibabaMcpContent>,
    /// Application-level failure marker.
    /// 应用层失败标记。
    #[serde(default, rename = "isError")]
    is_error: bool,
}

/// One Alibaba MCP content block.
/// 一项阿里云 MCP 内容块。
#[derive(Debug, Deserialize)]
struct AlibabaMcpContent {
    /// MCP content discriminator.
    /// MCP 内容判别字段。
    #[serde(rename = "type")]
    kind: String,
    /// JSON-encoded WebSearch pages for text content.
    /// 文本内容中 JSON 编码的 WebSearch 页面。
    #[serde(default)]
    text: String,
}

/// Structured page envelope embedded in Alibaba MCP text content.
/// 嵌入阿里云 MCP 文本内容中的结构化页面信封。
#[derive(Debug, Deserialize)]
struct AlibabaMcpPages {
    /// Ordered provider pages.
    /// 有序供应商页面。
    pages: Vec<AlibabaMcpPage>,
}

/// One provider-returned Alibaba WebSearch page.
/// 一条供应商返回的阿里云 WebSearch 页面。
#[derive(Debug, Deserialize)]
struct AlibabaMcpPage {
    /// Provider-returned title.
    /// 供应商返回的标题。
    #[serde(default)]
    title: String,
    /// Provider-returned absolute HTTP(S) URL.
    /// 供应商返回的绝对 HTTP(S) 地址。
    #[serde(default)]
    url: String,
    /// Provider-returned excerpt.
    /// 供应商返回的摘要。
    #[serde(default)]
    snippet: String,
}

/// Executes the reviewed Alibaba Model Studio Streamable HTTP MCP search flow.
/// 执行已审核的阿里云百炼 Model Studio Streamable HTTP MCP 搜索流程。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots for the selected regional Model Studio target.
/// 所有值均为所选区域 Model Studio 目标的不可变快照。
///
/// # Returns
/// 返回值
/// Ordered provider-backed search evidence normalized for the unified search card.
/// 返回为统一搜索卡片归一化的有序供应商搜索证据。
async fn execute_alibaba_search_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaSearchToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba WebSearch arguments are invalid: {error}"))?;
    arguments.validate()?;
    let query = arguments.query.trim().to_string();
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": ALIBABA_MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "VulcanCode",
                "version": "1"
            }
        }
    });
    let (initialize_body, mut session_id) =
        execute_alibaba_mcp_request(http, entry, target, secret, &initialize, None, "initialize")
            .await?;
    decode_alibaba_mcp_response(initialize_body.as_str(), 1)?;

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let (_, refreshed_session_id) = execute_alibaba_mcp_request(
        http,
        entry,
        target,
        secret,
        &initialized,
        session_id.as_deref(),
        "initialized notification",
    )
    .await?;
    if refreshed_session_id.is_some() {
        session_id = refreshed_session_id;
    }

    let mut call_arguments =
        serde_json::Map::from_iter([(String::from("query"), Value::String(query.clone()))]);
    if let Some(maximum) = arguments.max_results {
        call_arguments.insert(
            String::from("count"),
            Value::Number(serde_json::Number::from(maximum)),
        );
    }
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": ALIBABA_WEB_SEARCH_TOOL_NAME,
            "arguments": Value::Object(call_arguments)
        }
    });
    let (call_body, _) = execute_alibaba_mcp_request(
        http,
        entry,
        target,
        secret,
        &call,
        session_id.as_deref(),
        "tool call",
    )
    .await?;
    let result = decode_alibaba_mcp_response(call_body.as_str(), 2)?;
    decode_alibaba_search_result(
        query.as_str(),
        arguments.max_results.map(usize::from),
        result,
    )
}

/// Sends one exact authenticated Alibaba MCP request.
/// 发送一份精确且已鉴权的阿里云 MCP 请求。
///
/// # Parameters
/// 参数
/// `session_id` is absent only for initialization; `operation` scopes controlled diagnostics.
/// `session_id` 仅在初始化时缺失；`operation` 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// The complete response body and an optional refreshed MCP session identifier.
/// 返回完整响应正文与可选的已刷新 MCP 会话标识。
async fn execute_alibaba_mcp_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &Value,
    session_id: Option<&str>,
    operation: &str,
) -> Result<(String, Option<String>), String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    if let Some(session_id) = session_id {
        let value = HeaderValue::from_str(session_id)
            .map_err(|_| String::from("Alibaba MCP session identifier is invalid."))?;
        headers.insert(HeaderName::from_static("mcp-session-id"), value);
    }
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("Alibaba WebSearch {operation} request failed: {error}"))?;
    let status = response.status();
    let refreshed_session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let response_text =
        read_bounded_provider_response(response, format!("Alibaba WebSearch {operation}").as_str())
            .await?;
    if !status.is_success() {
        return Err(format_http_error(
            "Alibaba WebSearch",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    Ok((response_text, refreshed_session_id))
}

/// Decodes one correlated Alibaba MCP JSON-RPC response.
/// 解码一份已关联的阿里云 MCP JSON-RPC 响应。
///
/// # Parameters
/// 参数
/// `body` is the complete JSON document and `expected_id` is the exact request identifier.
/// `body` 是完整 JSON 文档，`expected_id` 是精确请求标识。
///
/// # Returns
/// 返回值
/// The method-specific result after version, correlation, and error validation.
/// 返回经过版本、关联与错误校验的方法专属结果。
fn decode_alibaba_mcp_response(body: &str, expected_id: u64) -> Result<Value, String> {
    let response: AlibabaMcpResponse = serde_json::from_str(body)
        .map_err(|error| format!("Alibaba MCP response JSON is invalid: {error}"))?;
    if response.jsonrpc != "2.0" || response.id != expected_id {
        return Err(String::from("Alibaba MCP response correlation is invalid."));
    }
    if let Some(error) = response.error {
        return Err(format!(
            "Alibaba MCP returned JSON-RPC error code {}.",
            error.code
        ));
    }
    response
        .result
        .ok_or_else(|| String::from("Alibaba MCP response omitted its result."))
}

/// Converts Alibaba MCP text pages into the unified structured search output.
/// 将阿里云 MCP 文本页面转换为统一结构化搜索输出。
///
/// # Parameters
/// 参数
/// `query`, `maximum`, and `result` are the frozen invocation values.
/// `query`、`maximum` 与 `result` 是冻结的调用值。
///
/// # Returns
/// 返回值
/// Validated ordered HTTP(S) results without provider-specific wrapper fields.
/// 返回不含供应商专属包装字段的已验证有序 HTTP(S) 结果。
fn decode_alibaba_search_result(
    query: &str,
    maximum: Option<usize>,
    result: Value,
) -> Result<ProviderToolExecutionResult, String> {
    let tool_result: AlibabaMcpToolResult = serde_json::from_value(result)
        .map_err(|error| format!("Alibaba MCP tool result is invalid: {error}"))?;
    if tool_result.is_error {
        return Err(String::from("Alibaba MCP WebSearch reported failure."));
    }
    let mut pages = Vec::new();
    for content in tool_result.content {
        if content.kind != "text" || content.text.trim().is_empty() {
            continue;
        }
        let envelope: AlibabaMcpPages = serde_json::from_str(content.text.as_str())
            .map_err(|error| format!("Alibaba WebSearch page JSON is invalid: {error}"))?;
        pages.extend(envelope.pages);
    }
    if pages.is_empty() {
        return Err(String::from(
            "Alibaba MCP WebSearch returned no structured pages.",
        ));
    }
    if let Some(maximum) = maximum {
        pages.truncate(maximum);
    }
    let results = pages
        .into_iter()
        .enumerate()
        .map(|(index, page)| {
            let url = reqwest::Url::parse(page.url.as_str())
                .map_err(|_| format!("Alibaba WebSearch result {index} is not a valid URL."))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || url.username() != ""
                || url.password().is_some()
            {
                return Err(format!(
                    "Alibaba WebSearch result {index} is not a credential-free HTTP(S) URL."
                ));
            }
            Ok(ProviderToolSearchResult {
                rank: index + 1,
                title: page.title,
                url: page.url,
                source_domain: url.host_str().unwrap_or_default().to_ascii_lowercase(),
                snippet: page.snippet,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProviderToolExecutionResult {
        upstream_response_id: None,
        output: ProviderToolNormalizedOutput::Search(ProviderToolSearchOutput {
            query: query.to_string(),
            results,
        }),
    })
}

/// Executes and normalizes the selected Tavily structured-search contract.
/// 执行并归一化选定的 Tavily 结构化搜索契约。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots resolved before this provider call.
/// 所有值均为本次供应商调用前解析的不可变快照。
///
/// # Returns
/// 返回值
/// Ordered provider-backed search evidence in the unified output union.
/// 返回统一输出联合中的有序供应商搜索证据。
async fn execute_tavily_search_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: TavilySearchToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Tavily search tool arguments are invalid: {error}"))?;
    arguments.validate()?;
    let query = arguments.query.trim().to_string();
    let body = TavilySearchWireRequest {
        query,
        search_depth: String::from("basic"),
        max_results: arguments.max_results,
        include_domains: arguments.include_domains,
        exclude_domains: arguments.exclude_domains,
        include_answer: false,
        include_raw_content: false,
        include_usage: true,
    };
    let response_text =
        execute_tavily_json_request(http, entry, target, secret, &body, "Tavily search").await?;
    decode_tavily_search_response(response_text.as_str())
}

/// Executes and normalizes the selected Tavily structured-extraction contract.
/// 执行并归一化选定的 Tavily 结构化网页提取契约。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots resolved before this provider call.
/// 所有值均为本次供应商调用前解析的不可变快照。
///
/// # Returns
/// 返回值
/// Structured successful pages and per-URL failures in the unified output union.
/// 返回统一输出联合中的结构化成功页面与逐 URL 失败。
async fn execute_tavily_extract_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: TavilyExtractToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Tavily extract tool arguments are invalid: {error}"))?;
    arguments.validate()?;
    let body = TavilyExtractWireRequest {
        urls: arguments.urls,
        query: arguments.query,
        chunks_per_source: arguments.chunks_per_source,
        extract_depth: arguments.extract_depth,
        include_images: arguments.include_images,
        include_favicon: arguments.include_favicon,
        format: arguments.format,
        timeout: arguments.timeout_seconds,
        include_usage: true,
    };
    let response_text =
        execute_tavily_json_request(http, entry, target, secret, &body, "Tavily extract").await?;
    decode_tavily_extract_response(response_text.as_str())
}

/// Dispatches one authenticated synchronous Tavily JSON request.
/// 分派一个经过鉴权的同步 Tavily JSON 请求。
///
/// # Parameters
/// 参数
/// `body` is the exact adapter-owned wire shape and `operation` scopes diagnostics.
/// `body` 是适配器持有的精确 Wire 结构，`operation` 用于限定诊断范围。
///
/// # Returns
/// 返回值
/// The complete successful JSON body or a controlled provider error.
/// 返回完整成功 JSON 正文或受控供应商错误。
async fn execute_tavily_json_request<T: Serialize>(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &T,
    operation: &str,
) -> Result<String, String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("{operation} request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format_http_error(
            operation,
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    Ok(response_text)
}

/// Strict AI-visible Qwen image generation arguments after host resource materialization.
/// 宿主资源物化后的严格 AI 可见 Qwen 图片生成参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaQwenImageGenerateArguments {
    /// Exact generation prompt.
    /// 精确生成提示词。
    prompt: String,
    /// Optional excluded visual content.
    /// 可选排除视觉内容。
    #[serde(default)]
    negative_prompt: String,
    /// Optional requested output count.
    /// 可选请求输出数量。
    #[serde(default)]
    count: Option<u8>,
    /// Optional output width.
    /// 可选输出宽度。
    #[serde(default)]
    width: Option<u32>,
    /// Optional output height.
    /// 可选输出高度。
    #[serde(default)]
    height: Option<u32>,
    /// Optional provider-relative seed.
    /// 可选供应商相对随机种子。
    #[serde(default)]
    seed: Option<i64>,
    /// Optional provider prompt rewriting switch.
    /// 可选供应商提示词改写开关。
    #[serde(default)]
    prompt_extend: Option<bool>,
    /// Optional provider watermark switch.
    /// 可选供应商水印开关。
    #[serde(default)]
    watermark: Option<bool>,
}

/// Strict Qwen image editing arguments after local paths become bounded Data URLs.
/// 本地路径转换为有界 Data URL 后的严格 Qwen 图片编辑参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaQwenImageEditArguments {
    /// Exact editing instruction.
    /// 精确编辑指令。
    instruction: String,
    /// Ordered host-materialized image Data URLs.
    /// 宿主物化后的有序图片 Data URL。
    source_images: Vec<String>,
    /// Optional excluded visual content.
    /// 可选排除视觉内容。
    #[serde(default)]
    negative_prompt: String,
    /// Optional requested output count.
    /// 可选请求输出数量。
    #[serde(default)]
    count: Option<u8>,
    /// Optional provider-relative seed.
    /// 可选供应商相对随机种子。
    #[serde(default)]
    seed: Option<i64>,
    /// Optional provider prompt rewriting switch.
    /// 可选供应商提示词改写开关。
    #[serde(default)]
    prompt_extend: Option<bool>,
    /// Optional provider watermark switch.
    /// 可选供应商水印开关。
    #[serde(default)]
    watermark: Option<bool>,
}

/// Strict Wan image generation arguments after local paths become bounded Data URLs.
/// 本地路径转换为有界 Data URL 后的严格 Wan 图片生成参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaWanImageGenerateArguments {
    /// Exact generation prompt.
    /// 精确生成提示词。
    prompt: String,
    /// Ordered optional reference image Data URLs.
    /// 有序可选参考图片 Data URL。
    #[serde(default)]
    reference_images: Vec<String>,
    /// Optional excluded visual content.
    /// 可选排除视觉内容。
    #[serde(default)]
    negative_prompt: String,
    /// Optional documented resolution tier.
    /// 可选文档化分辨率档位。
    #[serde(default)]
    resolution: Option<String>,
    /// Optional requested output count.
    /// 可选请求输出数量。
    #[serde(default)]
    count: Option<u8>,
    /// Optional provider-relative seed.
    /// 可选供应商相对随机种子。
    #[serde(default)]
    seed: Option<i64>,
    /// Optional provider prompt rewriting switch.
    /// 可选供应商提示词改写开关。
    #[serde(default)]
    prompt_extend: Option<bool>,
    /// Optional provider watermark switch.
    /// 可选供应商水印开关。
    #[serde(default)]
    watermark: Option<bool>,
}

/// Exact synchronous Alibaba image request shared by reviewed Qwen and Wan contracts.
/// 已审核 Qwen 与 Wan 合同共享的精确同步阿里云图片请求。
#[derive(Debug, Serialize)]
struct AlibabaImageWireRequest {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model: String,
    /// Single-turn ordered multimodal input.
    /// 单轮有序多模态输入。
    input: AlibabaImageWireInput,
    /// Exact optional provider controls.
    /// 精确可选供应商控制项。
    #[serde(skip_serializing_if = "AlibabaImageWireParameters::is_empty")]
    parameters: AlibabaImageWireParameters,
}

/// Contains the single required image-generation message.
/// 包含唯一必需的图片生成消息。
#[derive(Debug, Serialize)]
struct AlibabaImageWireInput {
    /// Exactly one user message.
    /// 恰好一条用户消息。
    messages: Vec<AlibabaImageWireMessage>,
}

/// Contains ordered image sources followed by one text instruction.
/// 包含有序图片来源及其后的一条文本指令。
#[derive(Debug, Serialize)]
struct AlibabaImageWireMessage {
    /// Provider-fixed user role.
    /// 供应商固定的用户角色。
    role: &'static str,
    /// Ordered exact-one-of content blocks.
    /// 有序精确单选内容块。
    content: Vec<AlibabaImageWireContent>,
}

/// Represents one image carrier or text instruction.
/// 表示一个图片载体或文本指令。
#[derive(Debug, Serialize)]
struct AlibabaImageWireContent {
    /// Optional inline image Data URL.
    /// 可选内联图片 Data URL。
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    /// Optional text instruction.
    /// 可选文本指令。
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// Contains the closed controls shared by Qwen Image 2.0 and Wan 2.7.
/// 包含 Qwen Image 2.0 与 Wan 2.7 共享的封闭控制项。
#[derive(Debug, Default, Serialize)]
struct AlibabaImageWireParameters {
    /// Optional excluded visual content.
    /// 可选排除视觉内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    negative_prompt: Option<String>,
    /// Optional provider-native size string.
    /// 可选供应商原生尺寸字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    /// Optional requested output count.
    /// 可选请求输出数量。
    #[serde(rename = "n", skip_serializing_if = "Option::is_none")]
    count: Option<u8>,
    /// Optional provider-relative seed.
    /// 可选供应商相对随机种子。
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    /// Optional provider prompt rewriting switch.
    /// 可选供应商提示词改写开关。
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_extend: Option<bool>,
    /// Optional provider watermark switch.
    /// 可选供应商水印开关。
    #[serde(skip_serializing_if = "Option::is_none")]
    watermark: Option<bool>,
}

impl AlibabaImageWireParameters {
    /// Reports whether every optional provider control is absent.
    /// 报告是否全部可选供应商控制项均缺失。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the request should omit the parameters object.
    /// 仅在请求应省略参数对象时返回 `true`。
    fn is_empty(&self) -> bool {
        self.negative_prompt.is_none()
            && self.size.is_none()
            && self.count.is_none()
            && self.seed.is_none()
            && self.prompt_extend.is_none()
            && self.watermark.is_none()
    }
}

/// Exact successful Alibaba synchronous image response.
/// 精确成功阿里云同步图片响应。
#[derive(Debug, Deserialize)]
struct AlibabaImageWireResponse {
    /// Provider request identifier.
    /// 供应商请求标识。
    #[serde(default)]
    request_id: Option<String>,
    /// Ordered provider output.
    /// 有序供应商输出。
    output: AlibabaImageWireOutput,
}

/// Contains ordered image choices.
/// 包含有序图片 Choice。
#[derive(Debug, Deserialize)]
struct AlibabaImageWireOutput {
    /// Provider choices.
    /// 供应商 Choice。
    choices: Vec<AlibabaImageWireChoice>,
}

/// Contains one assistant response message.
/// 包含一条 Assistant 响应消息。
#[derive(Debug, Deserialize)]
struct AlibabaImageWireChoice {
    /// Generated resource message.
    /// 生成资源消息。
    message: AlibabaImageWireResponseMessage,
}

/// Contains ordered generated image content.
/// 包含有序生成图片内容。
#[derive(Debug, Deserialize)]
struct AlibabaImageWireResponseMessage {
    /// Generated content blocks.
    /// 生成内容块。
    content: Vec<AlibabaImageWireResponseContent>,
}

/// Contains one provider-issued temporary image URL.
/// 包含一个供应商签发的临时图片 URL。
#[derive(Debug, Deserialize)]
struct AlibabaImageWireResponseContent {
    /// Absolute temporary PNG URL.
    /// 绝对临时 PNG URL。
    image: String,
}

/// Executes one exact synchronous Qwen image generation or editing tool.
/// 执行一个精确同步 Qwen 图片生成或编辑工具。
///
/// # Parameters
/// 参数
/// `entry`, `target`, `arguments`, and `secret` are immutable invocation snapshots.
/// `entry`、`target`、`arguments` 与 `secret` 是不可变调用快照。
///
/// # Returns
/// 返回值
/// Ordered generated PNG resources.
/// 返回有序生成 PNG 资源。
async fn execute_alibaba_qwen_image_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let body = match target.execution.adapter {
        ProviderToolAdapterKind::AlibabaQwenImageGenerate => {
            let arguments: AlibabaQwenImageGenerateArguments = serde_json::from_value(arguments)
                .map_err(|error| format!("Alibaba Qwen image arguments are invalid: {error}"))?;
            build_alibaba_qwen_generate_body(target, arguments)?
        }
        ProviderToolAdapterKind::AlibabaQwenImageEdit => {
            let arguments: AlibabaQwenImageEditArguments = serde_json::from_value(arguments)
                .map_err(|error| {
                    format!("Alibaba Qwen image-edit arguments are invalid: {error}")
                })?;
            build_alibaba_qwen_edit_body(target, arguments)?
        }
        _ => {
            return Err(String::from(
                "Alibaba Qwen image executor received an incompatible adapter.",
            ));
        }
    };
    execute_alibaba_image_request(http, entry, target, secret, &body, "Alibaba Qwen image").await
}

/// Builds and validates one exact Qwen image generation body.
/// 构建并验证一份精确 Qwen 图片生成正文。
///
/// # Parameters
/// 参数
/// `target` fixes the model and `arguments` contains model-native controls.
/// `target` 固定模型，`arguments` 包含模型原生控制项。
///
/// # Returns
/// 返回值
/// A lossless synchronous MultiModalConversation request.
/// 返回无损同步 MultiModalConversation 请求。
fn build_alibaba_qwen_generate_body(
    target: &ProviderToolTarget,
    arguments: AlibabaQwenImageGenerateArguments,
) -> Result<AlibabaImageWireRequest, String> {
    validate_alibaba_image_text(arguments.prompt.as_str(), "prompt", None)?;
    validate_alibaba_image_text(
        arguments.negative_prompt.as_str(),
        "negative_prompt",
        Some(500),
    )?;
    validate_alibaba_image_count(arguments.count, 6)?;
    validate_alibaba_image_seed(arguments.seed)?;
    if arguments.width.is_some() != arguments.height.is_some() {
        return Err(String::from(
            "Alibaba Qwen image width and height must be supplied together.",
        ));
    }
    let size = match (arguments.width, arguments.height) {
        (Some(width), Some(height)) => {
            let pixels = u64::from(width) * u64::from(height);
            if !(512 * 512..=2048 * 2048).contains(&pixels) {
                return Err(String::from(
                    "Alibaba Qwen image size must contain between 512*512 and 2048*2048 pixels.",
                ));
            }
            Some(format!("{width}*{height}"))
        }
        (None, None) => None,
        _ => unreachable!("paired width and height were validated"),
    };
    Ok(alibaba_image_wire_request(
        target.upstream_target_id.as_str(),
        Vec::new(),
        arguments.prompt,
        AlibabaImageWireParameters {
            negative_prompt: non_empty(arguments.negative_prompt),
            size,
            count: arguments.count,
            seed: arguments.seed,
            prompt_extend: arguments.prompt_extend,
            watermark: arguments.watermark,
        },
    ))
}

/// Builds and validates one exact Qwen image editing body.
/// 构建并验证一份精确 Qwen 图片编辑正文。
///
/// # Parameters
/// 参数
/// `target` fixes the model and `arguments` contains host-materialized sources.
/// `target` 固定模型，`arguments` 包含宿主物化来源。
///
/// # Returns
/// 返回值
/// A lossless synchronous MultiModalConversation request.
/// 返回无损同步 MultiModalConversation 请求。
fn build_alibaba_qwen_edit_body(
    target: &ProviderToolTarget,
    arguments: AlibabaQwenImageEditArguments,
) -> Result<AlibabaImageWireRequest, String> {
    validate_alibaba_image_text(arguments.instruction.as_str(), "instruction", None)?;
    validate_alibaba_image_text(
        arguments.negative_prompt.as_str(),
        "negative_prompt",
        Some(500),
    )?;
    if !(1..=3).contains(&arguments.source_images.len()) {
        return Err(String::from(
            "Alibaba Qwen image editing requires one to three source images.",
        ));
    }
    for image in &arguments.source_images {
        validate_alibaba_image_data_url(image, 10 * 1024 * 1024)?;
    }
    validate_alibaba_image_count(arguments.count, 6)?;
    validate_alibaba_image_seed(arguments.seed)?;
    Ok(alibaba_image_wire_request(
        target.upstream_target_id.as_str(),
        arguments.source_images,
        arguments.instruction,
        AlibabaImageWireParameters {
            negative_prompt: non_empty(arguments.negative_prompt),
            size: None,
            count: arguments.count,
            seed: arguments.seed,
            prompt_extend: arguments.prompt_extend,
            watermark: arguments.watermark,
        },
    ))
}

/// Executes one exact synchronous Wan 2.7 image generation tool.
/// 执行一个精确同步 Wan 2.7 图片生成工具。
///
/// # Parameters
/// 参数
/// `entry`, `target`, `arguments`, and `secret` are immutable invocation snapshots.
/// `entry`、`target`、`arguments` 与 `secret` 是不可变调用快照。
///
/// # Returns
/// 返回值
/// Ordered generated PNG resources.
/// 返回有序生成 PNG 资源。
async fn execute_alibaba_wan_image_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaWanImageGenerateArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Wan image arguments are invalid: {error}"))?;
    let body = build_alibaba_wan_generate_body(target, arguments)?;
    execute_alibaba_image_request(http, entry, target, secret, &body, "Alibaba Wan image").await
}

/// Builds one exact Wan 2.7 image request after validating model-specific constraints.
/// 在验证模型专属约束后构建一项精确 Wan 2.7 图片请求。
///
/// # Parameters
/// 参数
/// `target` fixes the provider model and `arguments` contains host-materialized inputs.
/// `target` 固定供应商模型，`arguments` 包含宿主已物化输入。
///
/// # Returns
/// 返回值
/// A lossless synchronous request or one pre-dispatch validation failure.
/// 返回无损同步请求，或一项分派前验证失败。
fn build_alibaba_wan_generate_body(
    target: &ProviderToolTarget,
    arguments: AlibabaWanImageGenerateArguments,
) -> Result<AlibabaImageWireRequest, String> {
    validate_alibaba_image_text(arguments.prompt.as_str(), "prompt", Some(5000))?;
    if arguments.reference_images.len() > 9 {
        return Err(String::from(
            "Alibaba Wan image generation accepts at most nine reference images.",
        ));
    }
    for image in &arguments.reference_images {
        validate_alibaba_image_data_url(image, 20 * 1024 * 1024)?;
    }
    validate_alibaba_image_count(arguments.count, 4)?;
    validate_alibaba_image_seed(arguments.seed)?;
    let resolution = arguments.resolution.as_deref().map(str::to_ascii_lowercase);
    if resolution
        .as_deref()
        .is_some_and(|value| !matches!(value, "1k" | "2k" | "4k"))
    {
        return Err(String::from(
            "Alibaba Wan image resolution must be 1k, 2k, or 4k.",
        ));
    }
    if resolution.as_deref() == Some("4k")
        && (target.upstream_target_id != "wan2.7-image-pro"
            || !arguments.reference_images.is_empty())
    {
        return Err(String::from(
            "Alibaba Wan 4k output is limited to wan2.7-image-pro text-only generation.",
        ));
    }
    Ok(alibaba_image_wire_request(
        target.upstream_target_id.as_str(),
        arguments.reference_images,
        arguments.prompt,
        AlibabaImageWireParameters {
            negative_prompt: non_empty(arguments.negative_prompt),
            size: resolution.map(|value| value.to_ascii_uppercase()),
            count: arguments.count,
            seed: arguments.seed,
            prompt_extend: arguments.prompt_extend,
            watermark: arguments.watermark,
        },
    ))
}

/// Builds the shared single-message image request while preserving source ordering.
/// 构建共享单消息图片请求并保留来源顺序。
///
/// # Parameters
/// 参数
/// `model`, `images`, `text`, and `parameters` are already validated.
/// `model`、`images`、`text` 与 `parameters` 已完成验证。
///
/// # Returns
/// 返回值
/// The exact provider wire request.
/// 返回精确供应商 Wire 请求。
fn alibaba_image_wire_request(
    model: &str,
    images: Vec<String>,
    text: String,
    parameters: AlibabaImageWireParameters,
) -> AlibabaImageWireRequest {
    let mut content = images
        .into_iter()
        .map(|image| AlibabaImageWireContent {
            image: Some(image),
            text: None,
        })
        .collect::<Vec<_>>();
    content.push(AlibabaImageWireContent {
        image: None,
        text: Some(text),
    });
    AlibabaImageWireRequest {
        model: model.to_string(),
        input: AlibabaImageWireInput {
            messages: vec![AlibabaImageWireMessage {
                role: "user",
                content,
            }],
        },
        parameters,
    }
}

/// Executes and decodes one synchronous Alibaba native image request.
/// 执行并解码一项同步阿里云原生图片请求。
///
/// # Parameters
/// 参数
/// `body` is the exact reviewed wire request and `operation` scopes diagnostics.
/// `body` 是精确已审核 Wire 请求，`operation` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Ordered normalized image resources or a controlled provider error.
/// 返回有序归一化图片资源或受控供应商错误。
async fn execute_alibaba_image_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &AlibabaImageWireRequest,
    operation: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("{operation} request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format_http_error(
            operation,
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    decode_alibaba_image_response(operation, response_text.as_str())
}

/// Decodes one exact Alibaba synchronous image response.
/// 解码一项精确阿里云同步图片响应。
///
/// # Parameters
/// 参数
/// `operation` scopes diagnostics and `response_text` is the bounded successful body.
/// `operation` 限定诊断范围，`response_text` 是有界成功正文。
///
/// # Returns
/// 返回值
/// Ordered PNG download resources.
/// 返回有序 PNG 下载资源。
fn decode_alibaba_image_response(
    operation: &str,
    response_text: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let response: AlibabaImageWireResponse = serde_json::from_str(response_text)
        .map_err(|error| format!("{operation} response JSON is invalid: {error}"))?;
    let mut resources = Vec::new();
    for choice in response.output.choices {
        for content in choice.message.content {
            let url = reqwest::Url::parse(content.image.as_str())
                .map_err(|_| format!("{operation} response contains an invalid image URL."))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(format!(
                    "{operation} response contains a non-public image URL."
                ));
            }
            resources.push(ProviderToolGeneratedResource {
                output_id: format!("image-{}", resources.len()),
                kind: String::from("image"),
                mime_type: String::from("image/png"),
                bytes: None,
                download_url: Some(content.image),
            });
        }
    }
    if resources.is_empty() {
        return Err(format!(
            "{operation} response contains no generated images."
        ));
    }
    Ok(ProviderToolExecutionResult {
        upstream_response_id: response.request_id,
        output: ProviderToolNormalizedOutput::Resources(resources),
    })
}

/// Validates one required or bounded optional Alibaba image text field.
/// 验证一个必需或有界可选阿里云图片文本字段。
///
/// # Parameters
/// 参数
/// `value`, `name`, and `maximum_characters` define the exact field boundary.
/// `value`、`name` 与 `maximum_characters` 定义精确字段边界。
///
/// # Returns
/// 返回值
/// Success only for a non-empty required value within its character ceiling.
/// 仅在必需值非空且未超过字符上限时返回成功。
fn validate_alibaba_image_text(
    value: &str,
    name: &str,
    maximum_characters: Option<usize>,
) -> Result<(), String> {
    if value.trim().is_empty() && name != "negative_prompt" {
        return Err(format!("Alibaba image {name} cannot be empty."));
    }
    if let Some(maximum) = maximum_characters
        && value.chars().count() > maximum
    {
        return Err(format!(
            "Alibaba image {name} exceeds {maximum} characters."
        ));
    }
    Ok(())
}

/// Validates one optional Alibaba image result count.
/// 验证一个可选阿里云图片结果数量。
///
/// # Parameters
/// 参数
/// `count` is absent or positive and `maximum` is the model-specific ceiling.
/// `count` 可缺失或为正数，`maximum` 是模型专属上限。
///
/// # Returns
/// 返回值
/// Success when the optional count is representable by the selected model.
/// 当可选数量可由所选模型表示时返回成功。
fn validate_alibaba_image_count(count: Option<u8>, maximum: u8) -> Result<(), String> {
    if count.is_some_and(|count| count == 0 || count > maximum) {
        return Err(format!(
            "Alibaba image count must be between one and {maximum}."
        ));
    }
    Ok(())
}

/// Validates one optional Alibaba provider-relative image seed.
/// 验证一个可选阿里云供应商相对图片随机种子。
///
/// # Parameters
/// 参数
/// `seed` is absent or a signed integer from the AI-visible contract.
/// `seed` 可缺失或为 AI 可见合同中的有符号整数。
///
/// # Returns
/// 返回值
/// Success only within the documented non-negative 32-bit range.
/// 仅在文档明确的非负 32 位范围内返回成功。
fn validate_alibaba_image_seed(seed: Option<i64>) -> Result<(), String> {
    if seed.is_some_and(|seed| !(0..=2_147_483_647).contains(&seed)) {
        return Err(String::from(
            "Alibaba image seed must be between 0 and 2147483647.",
        ));
    }
    Ok(())
}

/// Validates one host-created image Data URL and its provider-specific byte ceiling.
/// 验证一个宿主创建的图片 Data URL 及其供应商专属字节上限。
///
/// # Parameters
/// 参数
/// `value` is the carrier and `maximum_bytes` is the exact selected-adapter limit.
/// `value` 是载体，`maximum_bytes` 是精确所选适配器上限。
///
/// # Returns
/// 返回值
/// Success only for a supported non-empty image payload.
/// 仅在图片载荷受支持且非空时返回成功。
fn validate_alibaba_image_data_url(value: &str, maximum_bytes: usize) -> Result<(), String> {
    let encoded = [
        "data:image/jpeg;base64,",
        "data:image/png;base64,",
        "data:image/bmp;base64,",
        "data:image/webp;base64,",
        "data:image/gif;base64,",
    ]
    .into_iter()
    .find_map(|prefix| value.strip_prefix(prefix))
    .ok_or_else(|| {
        String::from("Alibaba image input must be a JPEG, PNG, BMP, WebP, or GIF Data URL.")
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| String::from("Alibaba image input contains invalid Base64."))?;
    if bytes.is_empty() || bytes.len() > maximum_bytes {
        return Err(format!(
            "Alibaba image input must be non-empty and no larger than {maximum_bytes} bytes."
        ));
    }
    Ok(())
}

/// Converts an optional empty provider string into an omitted wire field.
/// 将可选空供应商字符串转换为省略的 Wire 字段。
///
/// # Parameters
/// 参数
/// `value` is an already validated optional provider field.
/// `value` 是已验证的可选供应商字段。
///
/// # Returns
/// 返回值
/// `None` for empty text or the original non-empty text.
/// 空文本返回 `None`，否则返回原始非空文本。
fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// Strict CosyVoice synthesis arguments.
/// 严格 CosyVoice 合成参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaCosyVoiceArguments {
    /// Exact text or SSML document.
    /// 精确文本或 SSML 文档。
    text: String,
    /// System, cloned, or designed voice identifier.
    /// 系统、复刻或设计声音标识。
    voice_id: String,
    /// Requested provider audio encoding.
    /// 请求的供应商音频编码。
    #[serde(default = "default_alibaba_cosyvoice_format")]
    output_format: String,
    /// Optional output sample rate.
    /// 可选输出采样率。
    #[serde(default)]
    sample_rate: Option<u32>,
    /// Optional zero-through-one-hundred volume.
    /// 可选零至一百音量。
    #[serde(default)]
    volume: Option<f64>,
    /// Optional speech rate.
    /// 可选语速。
    #[serde(default)]
    speed: Option<f64>,
    /// Optional pitch multiplier.
    /// 可选音调倍率。
    #[serde(default)]
    pitch: Option<f64>,
    /// Optional deterministic seed.
    /// 可选确定性种子。
    #[serde(default)]
    seed: Option<u16>,
    /// Optional pronunciation language hint.
    /// 可选发音语言提示。
    #[serde(default)]
    language: Option<String>,
    /// Optional delivery instruction.
    /// 可选表达指令。
    #[serde(default)]
    style: Option<String>,
    /// Whether the exact text should be parsed as SSML.
    /// 是否将精确文本解析为 SSML。
    #[serde(default)]
    enable_ssml: bool,
}

/// Returns the documented default CosyVoice output encoding.
/// 返回文档规定的默认 CosyVoice 输出编码。
///
/// # Returns
/// 返回值
/// Lowercase `mp3`.
/// 返回小写 `mp3`。
fn default_alibaba_cosyvoice_format() -> String {
    String::from("mp3")
}

/// Strict Qwen3-TTS synthesis arguments.
/// 严格 Qwen3-TTS 合成参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaQwenTtsArguments {
    /// Exact transcript to synthesize.
    /// 待合成精确逐字稿。
    text: String,
    /// Documented preset voice identifier.
    /// 文档规定的预设声音标识。
    voice_id: String,
    /// Optional documented pronunciation language.
    /// 可选文档化发音语言。
    #[serde(default)]
    language: Option<String>,
    /// Optional instruct-model delivery instruction.
    /// 可选指令模型表达指令。
    #[serde(default)]
    style: Option<String>,
}

/// Strict Qwen3-ASR arguments after host audio materialization.
/// 宿主音频物化后的严格 Qwen3-ASR 参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaQwenAsrArguments {
    /// Host-created audio Data URL.
    /// 宿主创建的音频 Data URL。
    audio_url: String,
    /// Optional documented source language.
    /// 可选文档化源语言。
    #[serde(default)]
    language: Option<String>,
}

/// Exact Alibaba audio locator.
/// 精确阿里云音频定位符。
#[derive(Debug, Deserialize)]
struct AlibabaAudioLocator {
    /// Temporary complete resource URL.
    /// 临时完整资源 URL。
    url: String,
}

/// Exact CosyVoice successful response.
/// 精确 CosyVoice 成功响应。
#[derive(Debug, Deserialize)]
struct AlibabaCosyVoiceResponse {
    /// Provider request identifier.
    /// 供应商请求标识。
    request_id: String,
    /// Terminal audio output.
    /// 终态音频输出。
    output: AlibabaCosyVoiceOutput,
}

/// Exact CosyVoice terminal output.
/// 精确 CosyVoice 终态输出。
#[derive(Debug, Deserialize)]
struct AlibabaCosyVoiceOutput {
    /// Complete audio locator.
    /// 完整音频定位符。
    audio: AlibabaAudioLocator,
    /// Optional terminal reason.
    /// 可选终止原因。
    #[serde(default)]
    finish_reason: String,
}

/// Exact non-streaming Qwen3-TTS response.
/// 精确非流式 Qwen3-TTS 响应。
#[derive(Debug, Deserialize)]
struct AlibabaQwenTtsResponse {
    /// Provider-repeated business status.
    /// 供应商重复业务状态。
    status_code: u16,
    /// Provider request identifier.
    /// 供应商请求标识。
    request_id: String,
    /// Terminal synthesis output.
    /// 终态合成输出。
    output: AlibabaQwenTtsOutput,
}

/// Exact Qwen3-TTS terminal output.
/// 精确 Qwen3-TTS 终态输出。
#[derive(Debug, Deserialize)]
struct AlibabaQwenTtsOutput {
    /// Required terminal reason.
    /// 必需终止原因。
    finish_reason: String,
    /// Complete WAV locator.
    /// 完整 WAV 定位符。
    audio: AlibabaAudioLocator,
}

/// Exact synchronous Qwen3-ASR response.
/// 精确同步 Qwen3-ASR 响应。
#[derive(Debug, Deserialize)]
struct AlibabaQwenAsrResponse {
    /// Provider request identifier.
    /// 供应商请求标识。
    request_id: String,
    /// Terminal recognition output.
    /// 终态识别输出。
    output: AlibabaQwenAsrOutput,
}

/// Contains exactly one terminal recognition choice.
/// 包含恰好一个终态识别 Choice。
#[derive(Debug, Deserialize)]
struct AlibabaQwenAsrOutput {
    /// Ordered recognition choices.
    /// 有序识别 Choice。
    choices: Vec<AlibabaQwenAsrChoice>,
}

/// Contains one terminal assistant transcript.
/// 包含一个终态 Assistant 转录。
#[derive(Debug, Deserialize)]
struct AlibabaQwenAsrChoice {
    /// Required terminal reason.
    /// 必需终止原因。
    finish_reason: String,
    /// Assistant transcript message.
    /// Assistant 转录消息。
    message: AlibabaQwenAsrMessage,
}

/// Contains transcript text and provider annotations.
/// 包含转录文本与供应商注解。
#[derive(Debug, Deserialize)]
struct AlibabaQwenAsrMessage {
    /// Provider-fixed assistant role.
    /// 供应商固定 Assistant 角色。
    role: String,
    /// Exactly one transcript content item.
    /// 恰好一个转录内容项。
    content: Vec<AlibabaQwenAsrContent>,
}

/// Contains one transcript text item.
/// 包含一个转录文本项。
#[derive(Debug, Deserialize)]
struct AlibabaQwenAsrContent {
    /// Complete recognized text.
    /// 完整识别文本。
    text: String,
}

/// Executes one exact CosyVoice synchronous synthesis request.
/// 执行一个精确 CosyVoice 同步合成请求。
///
/// # Parameters
/// 参数
/// Invocation values are immutable after target selection and local validation.
/// 调用值在目标选择与本地验证后保持不可变。
///
/// # Returns
/// 返回值
/// One normalized generated audio resource.
/// 返回一个归一化生成音频资源。
async fn execute_alibaba_cosyvoice_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaCosyVoiceArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba CosyVoice arguments are invalid: {error}"))?;
    let (body, mime_type) = build_alibaba_cosyvoice_body(target, arguments)?;
    let response_text =
        execute_alibaba_native_json(http, entry, target, secret, &body, "Alibaba CosyVoice")
            .await?;
    let response: AlibabaCosyVoiceResponse = serde_json::from_str(response_text.as_str())
        .map_err(|error| format!("Alibaba CosyVoice response JSON is invalid: {error}"))?;
    if response.request_id.trim().is_empty()
        || (!response.output.finish_reason.is_empty() && response.output.finish_reason != "stop")
    {
        return Err(String::from("Alibaba CosyVoice response is not terminal."));
    }
    let url = validated_public_http_url(
        response.output.audio.url.as_str(),
        "Alibaba CosyVoice audio URL",
    )?;
    Ok(audio_resource_result(
        Some(response.request_id),
        url.to_string(),
        mime_type,
    ))
}

/// Builds one exact CosyVoice request and returns its normalized output MIME type.
/// 构建一项精确 CosyVoice 请求并返回其归一化输出 MIME 类型。
///
/// # Parameters
/// 参数
/// `target` freezes the selected model and `arguments` contains strict caller input.
/// `target` 冻结所选模型，`arguments` 包含严格调用方输入。
///
/// # Returns
/// 返回值
/// A provider-native JSON body and reviewed MIME type, or a pre-dispatch validation failure.
/// 返回供应商原生 JSON 正文与已审核 MIME 类型，或分派前验证失败。
fn build_alibaba_cosyvoice_body(
    target: &ProviderToolTarget,
    arguments: AlibabaCosyVoiceArguments,
) -> Result<(Value, &'static str), String> {
    if arguments.text.trim().is_empty() || arguments.voice_id.trim().is_empty() {
        return Err(String::from(
            "Alibaba CosyVoice requires non-empty text and voice_id.",
        ));
    }
    if arguments
        .volume
        .is_some_and(|value| !(0.0..=100.0).contains(&value))
        || arguments
            .speed
            .is_some_and(|value| !(0.5..=2.0).contains(&value))
        || arguments
            .pitch
            .is_some_and(|value| !(0.5..=2.0).contains(&value))
        || arguments.sample_rate == Some(0)
    {
        return Err(String::from(
            "Alibaba CosyVoice numeric controls are outside their verified ranges.",
        ));
    }
    let output_format = arguments.output_format.to_ascii_lowercase();
    let mime_type = alibaba_cosyvoice_mime_type(output_format.as_str()).ok_or_else(|| {
        String::from("Alibaba CosyVoice output_format must be mp3, pcm, wav, or opus.")
    })?;
    let language_hints = arguments
        .language
        .filter(|value| !value.is_empty())
        .map(|value| vec![value]);
    let input = json_object_with_optional_fields(
        serde_json::Map::from_iter([
            (String::from("text"), Value::String(arguments.text)),
            (String::from("voice"), Value::String(arguments.voice_id)),
            (String::from("format"), Value::String(output_format)),
        ]),
        [
            (
                "sample_rate",
                arguments.sample_rate.map(|value| serde_json::json!(value)),
            ),
            (
                "volume",
                arguments.volume.map(|value| serde_json::json!(value)),
            ),
            (
                "rate",
                arguments.speed.map(|value| serde_json::json!(value)),
            ),
            (
                "pitch",
                arguments.pitch.map(|value| serde_json::json!(value)),
            ),
            ("seed", arguments.seed.map(|value| serde_json::json!(value))),
            (
                "language_hints",
                language_hints.map(|value| serde_json::json!(value)),
            ),
            ("instruction", arguments.style.map(Value::String)),
            (
                "enable_ssml",
                arguments.enable_ssml.then_some(Value::Bool(true)),
            ),
        ],
    );
    let body = serde_json::json!({
        "model": target.upstream_target_id,
        "input": input
    });
    Ok((body, mime_type))
}

/// Executes one exact non-streaming Qwen3-TTS request.
/// 执行一个精确非流式 Qwen3-TTS 请求。
///
/// # Parameters
/// 参数
/// Invocation values are immutable after target selection and validation.
/// 调用值在目标选择与验证后保持不可变。
///
/// # Returns
/// 返回值
/// One normalized generated WAV resource.
/// 返回一个归一化生成 WAV 资源。
async fn execute_alibaba_qwen_tts_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaQwenTtsArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Qwen3-TTS arguments are invalid: {error}"))?;
    let body = build_alibaba_qwen_tts_body(target, arguments)?;
    let response_text =
        execute_alibaba_native_json(http, entry, target, secret, &body, "Alibaba Qwen3-TTS")
            .await?;
    let response: AlibabaQwenTtsResponse = serde_json::from_str(response_text.as_str())
        .map_err(|error| format!("Alibaba Qwen3-TTS response JSON is invalid: {error}"))?;
    if response.status_code != 200 || response.output.finish_reason != "stop" {
        return Err(format!(
            "Alibaba Qwen3-TTS returned non-terminal status {}.",
            response.status_code
        ));
    }
    let url = validated_public_http_url(
        response.output.audio.url.as_str(),
        "Alibaba Qwen3-TTS audio URL",
    )?;
    Ok(audio_resource_result(
        Some(response.request_id),
        url.to_string(),
        "audio/wav",
    ))
}

/// Builds one exact non-streaming Qwen3-TTS request.
/// 构建一项精确非流式 Qwen3-TTS 请求。
///
/// # Parameters
/// 参数
/// `target` fixes plain versus instruct behavior and `arguments` contains strict semantic input.
/// `target` 固定普通或指令模式行为，`arguments` 包含严格语义输入。
///
/// # Returns
/// 返回值
/// Provider-native JSON without unsupported explicit null fields.
/// 返回不含不受支持显式空值的供应商原生 JSON。
fn build_alibaba_qwen_tts_body(
    target: &ProviderToolTarget,
    arguments: AlibabaQwenTtsArguments,
) -> Result<Value, String> {
    if arguments.text.trim().is_empty()
        || arguments.text.chars().count() > 600
        || arguments.voice_id.trim().is_empty()
    {
        return Err(String::from(
            "Alibaba Qwen3-TTS requires non-empty text of at most 600 characters and one voice_id.",
        ));
    }
    if !supported_alibaba_qwen_tts_language(arguments.language.as_deref()) {
        return Err(String::from("Alibaba Qwen3-TTS language is unsupported."));
    }
    if arguments.style.is_some() && target.upstream_target_id != "qwen3-tts-instruct-flash" {
        return Err(String::from(
            "Alibaba Qwen3-TTS style requires qwen3-tts-instruct-flash.",
        ));
    }
    let optimize_instructions = arguments.style.is_some();
    let input = json_object_with_optional_fields(
        serde_json::Map::from_iter([
            (String::from("text"), Value::String(arguments.text)),
            (String::from("voice"), Value::String(arguments.voice_id)),
        ]),
        [
            ("language_type", arguments.language.map(Value::String)),
            ("instructions", arguments.style.map(Value::String)),
            (
                "optimize_instructions",
                optimize_instructions.then_some(Value::Bool(true)),
            ),
        ],
    );
    let body = serde_json::json!({
        "model": target.upstream_target_id,
        "input": input
    });
    Ok(body)
}

/// Executes one exact synchronous Qwen3-ASR transcription request.
/// 执行一个精确同步 Qwen3-ASR 转录请求。
///
/// # Parameters
/// 参数
/// Invocation values are immutable after host audio materialization.
/// 调用值在宿主音频物化后保持不可变。
///
/// # Returns
/// 返回值
/// Complete provider transcript text.
/// 返回完整供应商转录文本。
async fn execute_alibaba_qwen_asr_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaQwenAsrArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Qwen3-ASR arguments are invalid: {error}"))?;
    let body = build_alibaba_qwen_asr_body(target, arguments)?;
    let response_text =
        execute_alibaba_native_json(http, entry, target, secret, &body, "Alibaba Qwen3-ASR")
            .await?;
    let response: AlibabaQwenAsrResponse = serde_json::from_str(response_text.as_str())
        .map_err(|error| format!("Alibaba Qwen3-ASR response JSON is invalid: {error}"))?;
    if response.request_id.trim().is_empty() || response.output.choices.len() != 1 {
        return Err(String::from(
            "Alibaba Qwen3-ASR response omitted its terminal choice.",
        ));
    }
    let choice = &response.output.choices[0];
    if choice.finish_reason != "stop"
        || choice.message.role != "assistant"
        || choice.message.content.len() != 1
    {
        return Err(String::from("Alibaba Qwen3-ASR response is incomplete."));
    }
    Ok(ProviderToolExecutionResult {
        upstream_response_id: Some(response.request_id),
        output: ProviderToolNormalizedOutput::Text(choice.message.content[0].text.clone()),
    })
}

/// Builds one exact synchronous Qwen3-ASR request.
/// 构建一项精确同步 Qwen3-ASR 请求。
///
/// # Parameters
/// 参数
/// `target` fixes the ASR model and `arguments` contains host-materialized bounded audio.
/// `target` 固定 ASR 模型，`arguments` 包含宿主已物化的有界音频。
///
/// # Returns
/// 返回值
/// Provider-native JSON with only reviewed optional fields.
/// 返回仅含已审核可选字段的供应商原生 JSON。
fn build_alibaba_qwen_asr_body(
    target: &ProviderToolTarget,
    arguments: AlibabaQwenAsrArguments,
) -> Result<Value, String> {
    if !supported_alibaba_qwen_asr_language(arguments.language.as_deref()) {
        return Err(String::from("Alibaba Qwen3-ASR language is unsupported."));
    }
    validate_alibaba_audio_data_url(arguments.audio_url.as_str())?;
    let asr_options = json_object_with_optional_fields(
        serde_json::Map::from_iter([(String::from("enable_itn"), Value::Bool(false))]),
        [("language", arguments.language.map(Value::String))],
    );
    let body = serde_json::json!({
        "model": target.upstream_target_id,
        "input": {
            "messages": [{
                "role": "user",
                "content": [{"audio": arguments.audio_url}]
            }]
        },
        "parameters": {
            "result_format": "message",
            "asr_options": asr_options
        }
    });
    Ok(body)
}

/// Sends one synchronous Alibaba native JSON request through the selected origin.
/// 通过所选源地址发送一项同步阿里云原生 JSON 请求。
///
/// # Parameters
/// 参数
/// `body` is the adapter-owned wire value and `operation` scopes diagnostics.
/// `body` 是适配器持有的 Wire 值，`operation` 限定诊断范围。
///
/// # Returns
/// 返回值
/// The bounded successful response body.
/// 返回有界成功响应正文。
async fn execute_alibaba_native_json<T: Serialize + ?Sized>(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &T,
    operation: &str,
) -> Result<String, String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format!("{operation} request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format_http_error(
            operation,
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    Ok(response_text)
}

/// Adds only present provider-native optional fields to one JSON object.
/// 仅把存在的供应商原生可选字段加入一个 JSON 对象。
///
/// # Parameters
/// 参数
/// `required` contains exact required fields and `optional` contains reviewed optional carriers.
/// `required` 包含精确必需字段，`optional` 包含已审核可选载体。
///
/// # Returns
/// 返回值
/// A JSON object without unsupported explicit `null` values.
/// 返回不含不支持显式 `null` 值的 JSON 对象。
fn json_object_with_optional_fields<const N: usize>(
    mut required: serde_json::Map<String, Value>,
    optional: [(&str, Option<Value>); N],
) -> Value {
    for (name, value) in optional {
        if let Some(value) = value {
            required.insert(name.to_string(), value);
        }
    }
    Value::Object(required)
}

/// Builds one normalized generated-audio result.
/// 构建一个归一化生成音频结果。
///
/// # Parameters
/// 参数
/// `upstream_response_id`, `download_url`, and `mime_type` are already validated.
/// `upstream_response_id`、`download_url` 与 `mime_type` 已完成验证。
///
/// # Returns
/// 返回值
/// A single-resource output union.
/// 返回单资源输出联合。
fn audio_resource_result(
    upstream_response_id: Option<String>,
    download_url: String,
    mime_type: &str,
) -> ProviderToolExecutionResult {
    ProviderToolExecutionResult {
        upstream_response_id,
        output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
            output_id: String::from("audio-0"),
            kind: String::from("audio"),
            mime_type: mime_type.to_string(),
            bytes: None,
            download_url: Some(download_url),
        }]),
    }
}

/// Maps one verified CosyVoice format to its exact MIME type.
/// 将一个已验证 CosyVoice 格式映射为精确 MIME 类型。
///
/// # Parameters
/// 参数
/// `format` is the normalized provider encoding.
/// `format` 是归一化供应商编码。
///
/// # Returns
/// 返回值
/// The matching MIME type or `None` for unsupported input.
/// 返回匹配 MIME 类型，不支持时返回 `None`。
fn alibaba_cosyvoice_mime_type(format: &str) -> Option<&'static str> {
    match format {
        "mp3" => Some("audio/mpeg"),
        "pcm" => Some("audio/pcm"),
        "wav" => Some("audio/wav"),
        "opus" => Some("audio/opus"),
        _ => None,
    }
}

/// Reports whether Qwen3-TTS documents one pronunciation language.
/// 报告 Qwen3-TTS 是否记录一种发音语言。
///
/// # Parameters
/// 参数
/// `language` is absent or one model-native language name.
/// `language` 可缺失或为一个模型原生语言名称。
///
/// # Returns
/// 返回值
/// Whether the selected value can be projected losslessly.
/// 返回所选值能否无损投影。
fn supported_alibaba_qwen_tts_language(language: Option<&str>) -> bool {
    matches!(
        language,
        None | Some(
            "Auto"
                | "Chinese"
                | "English"
                | "German"
                | "Italian"
                | "Portuguese"
                | "Spanish"
                | "Japanese"
                | "Korean"
                | "French"
                | "Russian"
        )
    )
}

/// Reports whether Qwen3-ASR documents one source language.
/// 报告 Qwen3-ASR 是否记录一种源语言。
///
/// # Parameters
/// 参数
/// `language` is absent or one provider-native source-language code.
/// `language` 可缺失或为一个供应商原生源语言代码。
///
/// # Returns
/// 返回值
/// Whether the selected value can be projected losslessly.
/// 返回所选值能否无损投影。
fn supported_alibaba_qwen_asr_language(language: Option<&str>) -> bool {
    matches!(
        language,
        None | Some(
            "zh" | "yue"
                | "en"
                | "ja"
                | "de"
                | "ko"
                | "ru"
                | "fr"
                | "pt"
                | "ar"
                | "it"
                | "es"
                | "hi"
                | "id"
                | "th"
                | "tr"
                | "uk"
                | "vi"
                | "cs"
                | "da"
                | "fil"
                | "fi"
                | "is"
                | "ms"
                | "no"
                | "pl"
                | "sv"
        )
    )
}

/// Validates one host-created Qwen3-ASR audio Data URL.
/// 验证一个宿主创建的 Qwen3-ASR 音频 Data URL。
///
/// # Parameters
/// 参数
/// `value` is the complete provider audio carrier.
/// `value` 是完整供应商音频载体。
///
/// # Returns
/// 返回值
/// Success only for one supported non-empty bounded audio payload.
/// 仅在音频载荷受支持、非空且有界时返回成功。
fn validate_alibaba_audio_data_url(value: &str) -> Result<(), String> {
    let encoded = [
        "data:audio/mpeg;base64,",
        "data:audio/mp3;base64,",
        "data:audio/wav;base64,",
        "data:audio/x-wav;base64,",
        "data:audio/mp4;base64,",
        "data:audio/aac;base64,",
        "data:audio/ogg;base64,",
        "data:audio/flac;base64,",
        "data:audio/webm;base64,",
    ]
    .into_iter()
    .find_map(|prefix| value.strip_prefix(prefix))
    .ok_or_else(|| String::from("Alibaba Qwen3-ASR audio format is unsupported."))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| String::from("Alibaba Qwen3-ASR audio contains invalid Base64."))?;
    if bytes.is_empty() || bytes.len() > 50 * 1024 * 1024 {
        return Err(String::from(
            "Alibaba Qwen3-ASR audio must be non-empty and no larger than 50 MiB.",
        ));
    }
    Ok(())
}

/// Executes and decodes the exact MiniMax synchronous image contract.
/// 执行并解码精确的 MiniMax 同步图片契约。
///
/// # Parameters
/// 参数
/// All arguments are immutable snapshots for the selected target.
/// 全部参数均为选定目标的不可变快照。
///
/// # Returns
/// 返回值
/// Ordered generated JPEG resources.
/// 返回有序生成的 JPEG 资源。
async fn execute_minimax_image_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let request = MiniMaxImageToolArguments::from_json(arguments)?;
    let body = request.into_wire_body(target.upstream_target_id.as_str())?;
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("MiniMax image request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, "MiniMax image").await?;
    if !status.is_success() {
        return Err(format_http_error(
            "MiniMax image",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    decode_minimax_image_response(response_text.as_str())
}

/// Executes and decodes the exact MiniMax Coding Plan search contract.
/// 执行并解码精确的 MiniMax Coding Plan 搜索契约。
///
/// # Parameters
/// 参数
/// All arguments are immutable snapshots for the selected regional target.
/// 全部参数均为选定区域目标的不可变快照。
///
/// # Returns
/// 返回值
/// Ordered structured search results in the unified output union.
/// 返回统一输出联合中的有序结构化搜索结果。
async fn execute_minimax_search_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: MiniMaxSearchToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("MiniMax search tool arguments are invalid: {error}"))?;
    let query = arguments.query.trim();
    if query.is_empty() {
        return Err(String::from("MiniMax search query cannot be empty."));
    }
    if arguments
        .max_results
        .is_some_and(|maximum| !(1..=10).contains(&maximum))
    {
        return Err(String::from(
            "MiniMax search max_results must be between one and ten.",
        ));
    }
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(&serde_json::json!({"q": query}))
        .send()
        .await
        .map_err(|error| format!("MiniMax search request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, "MiniMax search").await?;
    if !status.is_success() {
        return Err(format_http_error(
            "MiniMax search",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    decode_minimax_search_response(
        query,
        arguments.max_results.unwrap_or(10),
        response_text.as_str(),
    )
}

/// Strict host-materialized MiniMax Coding Plan image-understanding arguments.
/// 严格的宿主物化 MiniMax Coding Plan 图片理解参数。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MiniMaxVisionToolArguments {
    /// Exact analysis instruction.
    /// 精确分析指令。
    prompt: String,
    /// Host-created JPEG, PNG, or WebP Data URL.
    /// 宿主创建的 JPEG、PNG 或 WebP Data URL。
    image_url: String,
}

impl MiniMaxVisionToolArguments {
    /// Validates the exact copied MiniMax Coding Plan VLM carrier.
    /// 验证精确复制的 MiniMax Coding Plan VLM 载体。
    ///
    /// # Returns
    /// 返回值
    /// Success only for a non-empty prompt and a bounded supported image Data URL.
    /// 仅在提示词非空且图片 Data URL 受支持并有界时返回成功。
    fn validate(&self) -> Result<(), String> {
        if self.prompt.trim().is_empty() {
            return Err(String::from(
                "MiniMax image-understanding prompt cannot be empty.",
            ));
        }
        let encoded = [
            "data:image/jpeg;base64,",
            "data:image/png;base64,",
            "data:image/webp;base64,",
        ]
        .into_iter()
        .find_map(|prefix| self.image_url.strip_prefix(prefix))
        .ok_or_else(|| {
            String::from(
                "MiniMax image-understanding image_url must be a JPEG, PNG, or WebP Data URL.",
            )
        })?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| {
                String::from("MiniMax image-understanding image_url contains invalid Base64.")
            })?;
        if bytes.is_empty() || bytes.len() >= 10_000_000 {
            return Err(String::from(
                "MiniMax image-understanding image must be non-empty and smaller than 10 MB.",
            ));
        }
        Ok(())
    }
}

/// Exact successful MiniMax Coding Plan VLM response.
/// 精确的 MiniMax Coding Plan VLM 成功响应。
#[derive(Debug, Deserialize)]
struct MiniMaxVisionWireResponse {
    /// Provider-generated analysis text.
    /// 供应商生成的分析文本。
    content: String,
}

/// Executes and normalizes the selected MiniMax Coding Plan image-understanding target.
/// 执行并归一化所选 MiniMax Coding Plan 图片理解目标。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots after host-side local-resource materialization.
/// 所有值均为宿主侧本地资源物化后的不可变快照。
///
/// # Returns
/// 返回值
/// Provider-generated analysis in the unified text output branch.
/// 返回统一文本输出分支中的供应商生成分析。
async fn execute_minimax_vision_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: MiniMaxVisionToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("MiniMax image-understanding arguments are invalid: {error}"))?;
    arguments.validate()?;
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(&arguments)
        .send()
        .await
        .map_err(|error| {
            format!("MiniMax image-understanding request to `{url}` failed: {error}")
        })?;
    let status = response.status();
    let response_text =
        read_bounded_provider_response(response, "MiniMax image-understanding").await?;
    if !status.is_success() {
        return Err(format_http_error(
            "MiniMax image-understanding",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    let response: MiniMaxVisionWireResponse = serde_json::from_str(response_text.as_str())
        .map_err(|error| {
            format!("MiniMax image-understanding response JSON is invalid: {error}")
        })?;
    if response.content.trim().is_empty() {
        return Err(String::from(
            "MiniMax image-understanding response omitted its content.",
        ));
    }
    Ok(ProviderToolExecutionResult {
        upstream_response_id: None,
        output: ProviderToolNormalizedOutput::Text(response.content),
    })
}

/// Strict AI-visible synchronous MiniMax speech arguments.
/// 严格的 AI 可见同步 MiniMax 语音参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MiniMaxSpeechToolArguments {
    /// Text to synthesize.
    /// 待合成文本。
    text: String,
    /// Built-in or account-owned voice identifier.
    /// 内置或账户自有声音标识。
    voice_id: String,
    /// Optional provider language boost.
    /// 可选供应商语言增强。
    #[serde(default)]
    language: Option<String>,
    /// Optional speaking speed.
    /// 可选语速。
    #[serde(default)]
    speed: Option<f64>,
    /// Optional provider volume.
    /// 可选供应商音量。
    #[serde(default)]
    volume: Option<f64>,
    /// Optional integer pitch.
    /// 可选整数音高。
    #[serde(default)]
    pitch: Option<i8>,
    /// Optional sample rate.
    /// 可选采样率。
    #[serde(default)]
    sample_rate: Option<u32>,
    /// Optional MP3 bitrate.
    /// 可选 MP3 比特率。
    #[serde(default)]
    bitrate: Option<u32>,
    /// Optional channel count.
    /// 可选声道数。
    #[serde(default)]
    channels: Option<u8>,
    /// Requested audio encoding.
    /// 请求的音频编码。
    #[serde(default = "default_minimax_speech_format")]
    output_format: String,
    /// Whether subtitle timing JSON is requested.
    /// 是否请求字幕计时 JSON。
    #[serde(default)]
    timestamps: bool,
    /// Ordered pronunciation dictionary entries.
    /// 有序发音字典条目。
    #[serde(default)]
    pronunciations: Vec<String>,
}

impl MiniMaxSpeechToolArguments {
    /// Validates MiniMax synchronous T2A controls and resolves provider defaults.
    /// 验证 MiniMax 同步 T2A 控制并解析供应商默认值。
    ///
    /// # Returns
    /// 返回值
    /// One exact wire body and its normalized output MIME type.
    /// 返回一份精确 Wire 正文及其归一化输出 MIME 类型。
    fn into_wire_body(self, model_id: &str) -> Result<(Value, String), String> {
        if !matches!(
            model_id,
            "speech-2.8-hd" | "speech-2.8-turbo" | "speech-2.6" | "speech-02"
        ) {
            return Err(String::from("MiniMax speech model is not registered."));
        }
        if self.text.chars().count() == 0 || self.text.chars().count() > 9999 {
            return Err(String::from(
                "MiniMax speech text must contain between one and 9999 characters.",
            ));
        }
        if self.voice_id.trim().is_empty() {
            return Err(String::from("MiniMax speech voice_id cannot be empty."));
        }
        if self
            .language
            .as_deref()
            .is_some_and(|language| !supported_minimax_speech_language(language))
        {
            return Err(String::from("MiniMax speech language is not supported."));
        }
        let speed = self.speed.unwrap_or(1.0);
        let volume = self.volume.unwrap_or(1.0);
        let pitch = self.pitch.unwrap_or(0);
        if !speed.is_finite()
            || !(0.5..=2.0).contains(&speed)
            || !volume.is_finite()
            || volume <= 0.0
            || volume > 10.0
            || !(-12..=12).contains(&pitch)
        {
            return Err(String::from(
                "MiniMax speech speed, volume, or pitch is outside its supported range.",
            ));
        }
        let (format, mime_type) = minimax_speech_format(self.output_format.as_str())?;
        let sample_rate = self.sample_rate.unwrap_or(match format {
            "opus" => 24_000,
            "pcmu_raw" | "pcmu_wav" => 8_000,
            _ => 32_000,
        });
        if !matches!(
            sample_rate,
            8_000 | 16_000 | 22_050 | 24_000 | 32_000 | 44_100
        ) {
            return Err(String::from("MiniMax speech sample_rate is not supported."));
        }
        let channels = self.channels.unwrap_or(1);
        if !matches!(channels, 1 | 2) {
            return Err(String::from("MiniMax speech channels must be one or two."));
        }
        let bitrate = if format == "mp3" {
            let bitrate = self.bitrate.unwrap_or(128_000);
            if !matches!(bitrate, 32_000 | 64_000 | 128_000 | 256_000) {
                return Err(String::from("MiniMax speech MP3 bitrate is not supported."));
            }
            Some(bitrate)
        } else {
            if self.bitrate.is_some() {
                return Err(String::from(
                    "MiniMax speech bitrate is valid only for MP3 output.",
                ));
            }
            None
        };
        if self
            .pronunciations
            .iter()
            .any(|pronunciation| pronunciation.trim().is_empty())
        {
            return Err(String::from(
                "MiniMax speech pronunciations cannot contain empty values.",
            ));
        }
        let mut audio_setting = serde_json::Map::from_iter([
            (
                String::from("sample_rate"),
                Value::Number(serde_json::Number::from(sample_rate)),
            ),
            (String::from("format"), Value::String(format.to_string())),
            (
                String::from("channel"),
                Value::Number(serde_json::Number::from(channels)),
            ),
        ]);
        if let Some(bitrate) = bitrate {
            audio_setting.insert(
                String::from("bitrate"),
                Value::Number(serde_json::Number::from(bitrate)),
            );
        }
        let mut body = serde_json::Map::from_iter([
            (String::from("model"), Value::String(model_id.to_string())),
            (String::from("text"), Value::String(self.text)),
            (String::from("stream"), Value::Bool(false)),
            (
                String::from("output_format"),
                Value::String(String::from("hex")),
            ),
            (
                String::from("voice_setting"),
                serde_json::json!({
                    "voice_id": self.voice_id,
                    "speed": speed,
                    "vol": volume,
                    "pitch": pitch
                }),
            ),
            (String::from("audio_setting"), Value::Object(audio_setting)),
        ]);
        if let Some(language) = self.language {
            body.insert(String::from("language_boost"), Value::String(language));
        }
        if self.timestamps {
            body.insert(String::from("subtitle_enable"), Value::Bool(true));
        }
        if !self.pronunciations.is_empty() {
            body.insert(
                String::from("pronunciation_dict"),
                serde_json::json!({"tone": self.pronunciations}),
            );
        }
        Ok((Value::Object(body), mime_type.to_string()))
    }
}

/// Exact MiniMax synchronous speech response subset.
/// 精确的 MiniMax 同步语音响应子集。
#[derive(Debug, Deserialize)]
struct MiniMaxSpeechWireResponse {
    /// Completed audio result when generation succeeds.
    /// 生成成功时的完整音频结果。
    #[serde(default)]
    data: Option<MiniMaxSpeechWireData>,
    /// Provider application status.
    /// 供应商应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// Completed MiniMax synchronous speech payload.
/// 完成的 MiniMax 同步语音载荷。
#[derive(Debug, Deserialize)]
struct MiniMaxSpeechWireData {
    /// Hexadecimal audio bytes.
    /// 十六进制音频字节。
    audio: String,
    /// Completion status, equal to two only on completion.
    /// 完成状态，仅在完成时等于二。
    status: u8,
    /// Optional subtitle timing JSON URL.
    /// 可选字幕计时 JSON 地址。
    #[serde(default)]
    subtitle_file: String,
}

/// Executes and normalizes one exact MiniMax synchronous T2A target.
/// 执行并归一化一个精确的 MiniMax 同步 T2A 目标。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots for the selected regional speech model.
/// 所有值均为所选区域语音模型的不可变快照。
///
/// # Returns
/// 返回值
/// One audio resource and an optional subtitle resource in the unified resource branch.
/// 返回统一资源分支中的一个音频资源与可选字幕资源。
async fn execute_minimax_speech_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: MiniMaxSpeechToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("MiniMax speech arguments are invalid: {error}"))?;
    let (body, mime_type) = arguments.into_wire_body(target.upstream_target_id.as_str())?;
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("MiniMax speech request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, "MiniMax speech").await?;
    if !status.is_success() {
        return Err(format_http_error(
            "MiniMax speech",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    decode_minimax_speech_response(response_text.as_str(), mime_type.as_str())
}

/// Decodes one MiniMax synchronous speech response into unified resources.
/// 将一份 MiniMax 同步语音响应解码为统一资源。
///
/// # Parameters
/// 参数
/// `body` is the complete provider JSON and `mime_type` is resolved from the selected format.
/// `body` 是完整供应商 JSON，`mime_type` 从所选格式解析。
///
/// # Returns
/// 返回值
/// Validated bounded audio bytes plus an optional public subtitle URL.
/// 返回已验证有界音频字节与可选公开字幕地址。
fn decode_minimax_speech_response(
    body: &str,
    mime_type: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let response: MiniMaxSpeechWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax speech response JSON is invalid: {error}"))?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax speech request failed: code={} | {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    let data = response
        .data
        .filter(|data| data.status == 2 && !data.audio.trim().is_empty())
        .ok_or_else(|| String::from("MiniMax speech response omitted complete audio."))?;
    let audio = decode_hexadecimal_bytes(data.audio.as_str())?;
    if audio.is_empty() || audio.len() > 10_000_000 {
        return Err(String::from(
            "MiniMax speech audio must be non-empty and no larger than 10 MB.",
        ));
    }
    let mut resources = vec![ProviderToolGeneratedResource {
        output_id: String::from("audio-0"),
        kind: String::from("audio"),
        mime_type: mime_type.to_string(),
        bytes: Some(audio),
        download_url: None,
    }];
    if !data.subtitle_file.trim().is_empty() {
        let subtitle_url =
            validated_public_http_url(data.subtitle_file.as_str(), "MiniMax speech subtitle URL")?;
        resources.push(ProviderToolGeneratedResource {
            output_id: String::from("subtitle-0"),
            kind: String::from("file"),
            mime_type: String::from("application/json"),
            bytes: None,
            download_url: Some(subtitle_url.to_string()),
        });
    }
    Ok(ProviderToolExecutionResult {
        upstream_response_id: None,
        output: ProviderToolNormalizedOutput::Resources(resources),
    })
}

/// Returns the provider default MiniMax speech format.
/// 返回供应商默认的 MiniMax 语音格式。
fn default_minimax_speech_format() -> String {
    String::from("mp3")
}

/// Maps one MiniMax speech output format to its exact MIME type.
/// 将一个 MiniMax 语音输出格式映射到精确 MIME 类型。
///
/// # Parameters
/// 参数
/// `format` is the AI-selected closed format value.
/// `format` 是 AI 选择的封闭格式值。
///
/// # Returns
/// 返回值
/// The normalized provider format and MIME type.
/// 返回归一化供应商格式与 MIME 类型。
fn minimax_speech_format(format: &str) -> Result<(&'static str, &'static str), String> {
    match format {
        "mp3" => Ok(("mp3", "audio/mpeg")),
        "wav" => Ok(("wav", "audio/wav")),
        "pcm" => Ok(("pcm", "audio/pcm")),
        "flac" => Ok(("flac", "audio/flac")),
        "pcmu_raw" => Ok(("pcmu_raw", "audio/basic")),
        "pcmu_wav" => Ok(("pcmu_wav", "audio/wav")),
        "opus" => Ok(("opus", "audio/opus")),
        _ => Err(String::from(
            "MiniMax speech output_format is not supported.",
        )),
    }
}

/// Reports whether one language belongs to the reviewed MiniMax language_boost vocabulary.
/// 报告一种语言是否属于已审核的 MiniMax language_boost 词汇表。
fn supported_minimax_speech_language(language: &str) -> bool {
    matches!(
        language,
        "Chinese"
            | "Chinese,Yue"
            | "English"
            | "Arabic"
            | "Russian"
            | "Spanish"
            | "French"
            | "Portuguese"
            | "German"
            | "Turkish"
            | "Dutch"
            | "Ukrainian"
            | "Vietnamese"
            | "Indonesian"
            | "Japanese"
            | "Italian"
            | "Korean"
            | "Thai"
            | "Polish"
            | "Romanian"
            | "Greek"
            | "Czech"
            | "Finnish"
            | "Hindi"
            | "Bulgarian"
            | "Danish"
            | "Hebrew"
            | "Malay"
            | "Persian"
            | "Slovak"
            | "Swedish"
            | "Croatian"
            | "Filipino"
            | "Hungarian"
            | "Norwegian"
            | "Slovenian"
            | "Catalan"
            | "Nynorsk"
            | "Tamil"
            | "Afrikaans"
            | "auto"
    )
}

/// Decodes an even-length hexadecimal string into bytes.
/// 将偶数长度的十六进制字符串解码为字节。
///
/// # Parameters
/// 参数
/// `value` is provider-returned hexadecimal audio.
/// `value` 是供应商返回的十六进制音频。
///
/// # Returns
/// 返回值
/// Decoded bytes or an explicit malformed-audio error.
/// 返回解码字节或明确的音频格式错误。
fn decode_hexadecimal_bytes(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err(String::from(
            "MiniMax speech response contains invalid hexadecimal audio.",
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| String::from("MiniMax speech hexadecimal audio is invalid."))?;
            u8::from_str_radix(pair, 16)
                .map_err(|_| String::from("MiniMax speech hexadecimal audio is invalid."))
        })
        .collect()
}

/// Validates one credential-free public HTTP(S) URL.
/// 验证一个不含凭据的公开 HTTP(S) 地址。
///
/// # Parameters
/// 参数
/// `value` is the provider-returned URL and `label` scopes diagnostics.
/// `value` 是供应商返回的地址，`label` 限定诊断范围。
///
/// # Returns
/// 返回值
/// A parsed URL accepted for controlled host-side download.
/// 返回可供受控宿主侧下载使用的已解析地址。
pub(crate) fn validated_public_http_url(value: &str, label: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
    {
        return Err(format!(
            "{label} must be a credential-free absolute HTTP(S) URL."
        ));
    }
    Ok(url)
}

/// Strict AI-visible synchronous MiniMax music arguments.
/// 严格的 AI 可见同步 MiniMax 音乐参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MiniMaxMusicToolArguments {
    /// Style, mood, and scenario prompt.
    /// 风格、情绪与场景提示词。
    #[serde(default)]
    prompt: String,
    /// Exact song lyrics.
    /// 精确歌曲歌词。
    #[serde(default)]
    lyrics: String,
    /// Requests music without vocals.
    /// 请求无声乐的纯音乐。
    #[serde(default)]
    instrumental: bool,
    /// Requests provider-generated optimized lyrics.
    /// 请求供应商生成优化歌词。
    #[serde(default)]
    lyrics_optimizer: bool,
    /// Optional output sample rate.
    /// 可选输出采样率。
    #[serde(default)]
    sample_rate: Option<u32>,
    /// Optional output bitrate.
    /// 可选输出码率。
    #[serde(default)]
    bitrate: Option<u32>,
    /// Optional output channel count.
    /// 可选输出声道数。
    #[serde(default)]
    channels: Option<u8>,
    /// Optional deterministic provider seed.
    /// 可选供应商确定性种子。
    #[serde(default)]
    seed: Option<i64>,
    /// Requests an AIGC watermark.
    /// 请求 AIGC 水印。
    #[serde(default)]
    watermark: bool,
    /// Exact output audio format.
    /// 精确输出音频格式。
    #[serde(default = "default_minimax_music_format")]
    output_format: String,
}

impl MiniMaxMusicToolArguments {
    /// Validates MiniMax music intent and projects one exact provider request body.
    /// 验证 MiniMax 音乐意图并投影一个精确供应商请求体。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is the frozen selected MiniMax music model.
    /// `model_id` 是冻结的所选 MiniMax 音乐模型。
    ///
    /// # Returns
    /// 返回值
    /// One synchronous URL-output request and its normalized MIME type.
    /// 返回一个同步 URL 输出请求及其归一化 MIME 类型。
    fn into_wire_body(self, model_id: &str) -> Result<(Value, String), String> {
        if !matches!(
            model_id,
            "music-3.0" | "music-2.6" | "music-2.6-free" | "music-2.5+" | "music-2.5"
        ) {
            return Err(String::from("MiniMax music model is not registered."));
        }
        let prompt_length = self.prompt.chars().count();
        let lyrics_length = self.lyrics.chars().count();
        if prompt_length > 2000 || lyrics_length > 3500 {
            return Err(String::from(
                "MiniMax music prompt or lyrics exceeds its supported length.",
            ));
        }
        if self.instrumental {
            if prompt_length == 0 || lyrics_length != 0 || self.lyrics_optimizer {
                return Err(String::from(
                    "MiniMax instrumental music requires a prompt and forbids lyrics or lyric optimization.",
                ));
            }
        } else if self.lyrics_optimizer {
            if prompt_length == 0 || lyrics_length != 0 {
                return Err(String::from(
                    "MiniMax lyric optimization requires a prompt and forbids explicit lyrics.",
                ));
            }
        } else if lyrics_length == 0 {
            return Err(String::from(
                "MiniMax vocal music requires explicit lyrics or lyric optimization.",
            ));
        }
        let (output_format, mime_type) = minimax_music_format(self.output_format.as_str())?;
        let sample_rate = self.sample_rate.unwrap_or(44_100);
        let bitrate = self.bitrate.unwrap_or(256_000);
        if sample_rate == 0 || bitrate == 0 {
            return Err(String::from(
                "MiniMax music sample_rate and bitrate must be positive.",
            ));
        }
        if self
            .channels
            .is_some_and(|channels| !matches!(channels, 1 | 2))
        {
            return Err(String::from("MiniMax music channels must be one or two."));
        }
        let mut audio_setting = serde_json::Map::from_iter([
            (
                String::from("sample_rate"),
                Value::Number(serde_json::Number::from(sample_rate)),
            ),
            (
                String::from("bitrate"),
                Value::Number(serde_json::Number::from(bitrate)),
            ),
            (
                String::from("format"),
                Value::String(output_format.to_string()),
            ),
        ]);
        if let Some(channels) = self.channels {
            audio_setting.insert(
                String::from("channel"),
                Value::Number(serde_json::Number::from(channels)),
            );
        }
        let mut body = serde_json::Map::from_iter([
            (String::from("model"), Value::String(model_id.to_string())),
            (String::from("stream"), Value::Bool(false)),
            (
                String::from("output_format"),
                Value::String(String::from("url")),
            ),
            (String::from("audio_setting"), Value::Object(audio_setting)),
        ]);
        if !self.prompt.is_empty() {
            body.insert(String::from("prompt"), Value::String(self.prompt));
        }
        if !self.lyrics.is_empty() {
            body.insert(String::from("lyrics"), Value::String(self.lyrics));
        }
        if self.instrumental {
            body.insert(String::from("is_instrumental"), Value::Bool(true));
        }
        if self.lyrics_optimizer {
            body.insert(String::from("lyrics_optimizer"), Value::Bool(true));
        }
        if let Some(seed) = self.seed {
            body.insert(
                String::from("seed"),
                Value::Number(serde_json::Number::from(seed)),
            );
        }
        if self.watermark {
            body.insert(String::from("aigc_watermark"), Value::Bool(true));
        }
        Ok((Value::Object(body), mime_type.to_string()))
    }
}

/// Exact MiniMax synchronous music response subset.
/// 精确的 MiniMax 同步音乐响应子集。
#[derive(Debug, Deserialize)]
struct MiniMaxMusicWireResponse {
    /// Completed provider audio data.
    /// 已完成的供应商音频数据。
    data: MiniMaxMusicWireData,
    /// Provider request identifier.
    /// 供应商请求标识。
    trace_id: String,
    /// Provider application status.
    /// 供应商应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// Completed MiniMax synchronous music payload.
/// 已完成的 MiniMax 同步音乐载荷。
#[derive(Debug, Deserialize)]
struct MiniMaxMusicWireData {
    /// Temporary public audio URL.
    /// 临时公网音频地址。
    audio_url: String,
    /// Completion status, equal to two only on completion.
    /// 完成状态，仅在完成时等于二。
    status: u8,
}

/// Executes and normalizes one exact MiniMax synchronous music target.
/// 执行并归一化一个精确的 MiniMax 同步音乐目标。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots for the selected regional music model.
/// 所有值均为所选区域音乐模型的不可变快照。
///
/// # Returns
/// 返回值
/// One unified audio resource or an explicit provider error.
/// 返回一个统一音频资源或明确的供应商错误。
async fn execute_minimax_music_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: MiniMaxMusicToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("MiniMax music arguments are invalid: {error}"))?;
    let (body, mime_type) = arguments.into_wire_body(target.upstream_target_id.as_str())?;
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("MiniMax music request to `{url}` failed: {error}"))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, "MiniMax music").await?;
    if !status.is_success() {
        return Err(format_http_error(
            "MiniMax music",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    decode_minimax_music_response(response_text.as_str(), mime_type.as_str())
}

/// Decodes one MiniMax music response into a unified audio resource.
/// 将一份 MiniMax 音乐响应解码为统一音频资源。
///
/// # Parameters
/// 参数
/// `body` is provider JSON and `mime_type` comes from the selected output format.
/// `body` 是供应商 JSON，`mime_type` 来自所选输出格式。
///
/// # Returns
/// 返回值
/// One validated temporary audio URL and provider request identifier.
/// 返回一个已验证临时音频地址与供应商请求标识。
fn decode_minimax_music_response(
    body: &str,
    mime_type: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let response: MiniMaxMusicWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax music response JSON is invalid: {error}"))?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax music request failed: code={} | {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    if response.data.status != 2 || response.trace_id.trim().is_empty() {
        return Err(String::from(
            "MiniMax music response omitted its completed result.",
        ));
    }
    let audio_url =
        validated_public_http_url(response.data.audio_url.as_str(), "MiniMax music audio URL")?;
    Ok(ProviderToolExecutionResult {
        upstream_response_id: Some(response.trace_id),
        output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
            output_id: String::from("music-0"),
            kind: String::from("audio"),
            mime_type: mime_type.to_string(),
            bytes: None,
            download_url: Some(audio_url.to_string()),
        }]),
    })
}

/// Returns the default MiniMax music output format.
/// 返回默认 MiniMax 音乐输出格式。
fn default_minimax_music_format() -> String {
    String::from("mp3")
}

/// Maps one MiniMax music format to the exact output MIME type.
/// 将一种 MiniMax 音乐格式映射到精确输出 MIME 类型。
///
/// # Parameters
/// 参数
/// `format` is the AI-selected closed output format.
/// `format` 是 AI 选择的封闭输出格式。
///
/// # Returns
/// 返回值
/// The normalized provider format and MIME type.
/// 返回归一化供应商格式与 MIME 类型。
fn minimax_music_format(format: &str) -> Result<(&'static str, &'static str), String> {
    match format {
        "mp3" => Ok(("mp3", "audio/mpeg")),
        "wav" => Ok(("wav", "audio/wav")),
        "pcm" => Ok(("pcm", "audio/L16")),
        _ => Err(String::from(
            "MiniMax music output_format is not supported.",
        )),
    }
}

/// Strict host-materialized MiniMax video arguments.
/// 严格的宿主物化 MiniMax 视频参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MiniMaxVideoToolArguments {
    /// Motion and scene prompt.
    /// 运动与场景提示词。
    prompt: String,
    /// Optional first-frame Data URL.
    /// 可选首帧 Data URL。
    #[serde(default)]
    first_frame_image: Option<String>,
    /// Optional last-frame Data URL.
    /// 可选末帧 Data URL。
    #[serde(default)]
    last_frame_image: Option<String>,
    /// Optional subject-reference Data URL.
    /// 可选主体参考 Data URL。
    #[serde(default)]
    subject_reference_image: Option<String>,
    /// Six- or ten-second duration.
    /// 六秒或十秒时长。
    #[serde(default = "default_minimax_video_duration")]
    duration_seconds: u8,
    /// Optional model-scoped resolution.
    /// 可选模型作用域分辨率。
    #[serde(default)]
    resolution: Option<String>,
    /// Optional provider prompt optimization control.
    /// 可选供应商提示词优化控制。
    #[serde(default)]
    prompt_extend: Option<bool>,
}

impl MiniMaxVideoToolArguments {
    /// Validates model-specific MiniMax video modes and projects the start body.
    /// 验证模型专属 MiniMax 视频模式并投影启动请求体。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is the frozen selected video model.
    /// `model_id` 是冻结的所选视频模型。
    ///
    /// # Returns
    /// 返回值
    /// One exact provider task-creation JSON body.
    /// 返回一个精确供应商任务创建 JSON 请求体。
    fn into_wire_body(self, model_id: &str) -> Result<Value, String> {
        if self.prompt.trim().is_empty() || self.prompt.chars().count() > 2000 {
            return Err(String::from(
                "MiniMax video prompt must contain between one and 2000 characters.",
            ));
        }
        if !matches!(self.duration_seconds, 6 | 10) {
            return Err(String::from(
                "MiniMax video duration_seconds must be six or ten.",
            ));
        }
        for image in [
            self.first_frame_image.as_deref(),
            self.last_frame_image.as_deref(),
            self.subject_reference_image.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !image.starts_with("data:image/") || !image.contains(";base64,") {
                return Err(String::from(
                    "MiniMax video image inputs must be host-materialized Data URLs.",
                ));
            }
        }
        let image_mode = self.first_frame_image.is_some();
        let subject_mode = self.subject_reference_image.is_some();
        match model_id {
            "MiniMax-Hailuo-2.3" => {
                if self.last_frame_image.is_some() || subject_mode {
                    return Err(String::from(
                        "MiniMax Hailuo 2.3 accepts only an optional first frame.",
                    ));
                }
            }
            "MiniMax-Hailuo-2.3-Fast" => {
                if !image_mode || self.last_frame_image.is_some() || subject_mode {
                    return Err(String::from(
                        "MiniMax Hailuo 2.3 Fast requires exactly one first frame.",
                    ));
                }
            }
            "MiniMax-Hailuo-02" => {
                if subject_mode || self.last_frame_image.is_some() && !image_mode {
                    return Err(String::from(
                        "MiniMax Hailuo 02 last-frame mode requires a first frame.",
                    ));
                }
            }
            "S2V-01" => {
                if !subject_mode || image_mode || self.last_frame_image.is_some() {
                    return Err(String::from(
                        "MiniMax S2V 01 requires exactly one subject reference.",
                    ));
                }
            }
            _ => return Err(String::from("MiniMax video model is not registered.")),
        }
        let resolution = self
            .resolution
            .as_deref()
            .map(str::to_ascii_uppercase)
            .unwrap_or_default();
        if !resolution.is_empty() {
            let supported = matches!(resolution.as_str(), "768P" | "1080P")
                || resolution == "512P" && model_id == "MiniMax-Hailuo-02" && image_mode
                || resolution == "512P" && model_id == "S2V-01";
            if !supported {
                return Err(String::from(
                    "MiniMax video resolution is not supported by the selected model and mode.",
                ));
            }
        }
        if self.duration_seconds == 10
            && !resolution.is_empty()
            && resolution != "768P"
            && !(model_id == "MiniMax-Hailuo-02" && image_mode && resolution == "512P")
        {
            return Err(String::from(
                "MiniMax ten-second video resolution is not supported by the selected mode.",
            ));
        }
        let mut body = serde_json::Map::from_iter([
            (String::from("model"), Value::String(model_id.to_string())),
            (String::from("prompt"), Value::String(self.prompt)),
            (
                String::from("duration"),
                Value::Number(serde_json::Number::from(self.duration_seconds)),
            ),
        ]);
        if let Some(first_frame) = self.first_frame_image {
            body.insert(
                String::from("first_frame_image"),
                Value::String(first_frame),
            );
        }
        if let Some(last_frame) = self.last_frame_image {
            body.insert(String::from("last_frame_image"), Value::String(last_frame));
        }
        if let Some(subject_reference) = self.subject_reference_image {
            body.insert(
                String::from("subject_reference"),
                serde_json::json!([{
                    "type": "character",
                    "image": [subject_reference]
                }]),
            );
        }
        if !resolution.is_empty() {
            body.insert(String::from("resolution"), Value::String(resolution));
        }
        if let Some(prompt_extend) = self.prompt_extend {
            body.insert(String::from("prompt_optimizer"), Value::Bool(prompt_extend));
        }
        Ok(Value::Object(body))
    }
}

/// Exact MiniMax video task-creation response.
/// 精确的 MiniMax 视频任务创建响应。
#[derive(Debug, Deserialize)]
struct MiniMaxVideoStartWireResponse {
    /// Private provider task identifier.
    /// 私有供应商任务标识。
    task_id: String,
    /// Provider application status.
    /// 供应商应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// Exact MiniMax video task observation.
/// 精确的 MiniMax 视频任务观测。
#[derive(Debug, Deserialize)]
struct MiniMaxVideoPollWireResponse {
    /// Documented provider task state.
    /// 文档声明的供应商任务状态。
    status: String,
    /// Private generated file identifier.
    /// 私有生成文件标识。
    #[serde(default)]
    file_id: String,
    /// Provider application status.
    /// 供应商应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// Exact MiniMax generated-file lookup response.
/// 精确的 MiniMax 生成文件查询响应。
#[derive(Debug, Deserialize)]
struct MiniMaxVideoFileWireResponse {
    /// Generated file metadata.
    /// 生成文件元数据。
    file: MiniMaxVideoFileWireData,
    /// Provider application status.
    /// 供应商应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// Exact MiniMax generated-file metadata.
/// 精确的 MiniMax 生成文件元数据。
#[derive(Debug, Deserialize)]
struct MiniMaxVideoFileWireData {
    /// Temporary provider download URL.
    /// 临时供应商下载地址。
    download_url: String,
}

/// Executes one MiniMax video task through creation, polling, and file resolution.
/// 通过创建、轮询与文件解析执行一个 MiniMax 视频任务。
///
/// # Parameters
/// 参数
/// All values are frozen for the selected regional video target.
/// 所有值均针对所选区域视频目标保持冻结。
///
/// # Returns
/// 返回值
/// One unified MP4 resource after the provider reaches its terminal success state.
/// 在供应商到达成功终态后返回一个统一 MP4 资源。
async fn execute_minimax_video_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: MiniMaxVideoToolArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("MiniMax video arguments are invalid: {error}"))?;
    let body = arguments.into_wire_body(target.upstream_target_id.as_str())?;
    let start_url = provider_tool_url(entry, target);
    let base_url = target
        .execution
        .endpoint_base_url
        .as_deref()
        .unwrap_or(entry.base_url.as_str())
        .trim_end_matches('/');
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_authentication_header(
        &mut headers,
        target.execution.authentication,
        secret.expose_value(),
    )?;
    let start_response = http
        .post(start_url.as_str())
        .headers(headers.clone())
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("MiniMax video request to `{start_url}` failed: {error}"))?;
    let start_status = start_response.status();
    let start_text = read_bounded_provider_response(start_response, "MiniMax video start").await?;
    if !start_status.is_success() {
        return Err(format_http_error(
            "MiniMax video",
            start_status.as_u16(),
            start_text.as_str(),
        ));
    }
    let start: MiniMaxVideoStartWireResponse = serde_json::from_str(start_text.as_str())
        .map_err(|error| format!("MiniMax video start response JSON is invalid: {error}"))?;
    if start.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax video task creation failed: code={} | {}",
            start.base_resp.status_code, start.base_resp.status_msg
        ));
    }
    if start.task_id.trim().is_empty() {
        return Err(String::from(
            "MiniMax video task creation omitted its task identifier.",
        ));
    }
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        let mut poll_url =
            reqwest::Url::parse(format!("{base_url}/v1/query/video_generation").as_str())
                .map_err(|_| String::from("MiniMax video poll URL is invalid."))?;
        poll_url
            .query_pairs_mut()
            .append_pair("task_id", start.task_id.as_str());
        let poll_response = http
            .get(poll_url.clone())
            .headers(headers.clone())
            .send()
            .await
            .map_err(|error| format!("MiniMax video poll to `{poll_url}` failed: {error}"))?;
        let poll_status = poll_response.status();
        let poll_text = read_bounded_provider_response(poll_response, "MiniMax video poll").await?;
        if !poll_status.is_success() {
            return Err(format_http_error(
                "MiniMax video poll",
                poll_status.as_u16(),
                poll_text.as_str(),
            ));
        }
        let poll: MiniMaxVideoPollWireResponse = serde_json::from_str(poll_text.as_str())
            .map_err(|error| format!("MiniMax video poll response JSON is invalid: {error}"))?;
        if poll.base_resp.status_code != 0 {
            return Err(format!(
                "MiniMax video poll failed: code={} | {}",
                poll.base_resp.status_code, poll.base_resp.status_msg
            ));
        }
        match poll.status.as_str() {
            "Preparing" | "Queueing" | "Processing" => continue,
            "Failed" => {
                return Err(String::from(
                    "MiniMax video generation reached the provider failed state.",
                ));
            }
            "Success" if !poll.file_id.trim().is_empty() => {
                let mut file_url =
                    reqwest::Url::parse(format!("{base_url}/v1/files/retrieve").as_str())
                        .map_err(|_| String::from("MiniMax video file URL is invalid."))?;
                file_url
                    .query_pairs_mut()
                    .append_pair("file_id", poll.file_id.as_str());
                let file_response = http
                    .get(file_url.clone())
                    .headers(headers)
                    .send()
                    .await
                    .map_err(|error| {
                        format!("MiniMax video file request to `{file_url}` failed: {error}")
                    })?;
                let file_status = file_response.status();
                let file_text =
                    read_bounded_provider_response(file_response, "MiniMax video file").await?;
                if !file_status.is_success() {
                    return Err(format_http_error(
                        "MiniMax video file",
                        file_status.as_u16(),
                        file_text.as_str(),
                    ));
                }
                return decode_minimax_video_file_response(
                    file_text.as_str(),
                    start.task_id.as_str(),
                );
            }
            "Success" => {
                return Err(String::from(
                    "MiniMax video successful task omitted its file identifier.",
                ));
            }
            _ => {
                return Err(format!(
                    "MiniMax video returned unknown task status `{}`.",
                    poll.status
                ));
            }
        }
    }
}

/// Decodes a completed MiniMax file lookup into one unified video resource.
/// 将完成的 MiniMax 文件查询解码为一个统一视频资源。
///
/// # Parameters
/// 参数
/// `body` is the file response and `task_id` is the provider task identity.
/// `body` 是文件响应，`task_id` 是供应商任务标识。
///
/// # Returns
/// 返回值
/// One validated temporary MP4 resource.
/// 返回一个已验证临时 MP4 资源。
fn decode_minimax_video_file_response(
    body: &str,
    task_id: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let response: MiniMaxVideoFileWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax video file response JSON is invalid: {error}"))?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax video file request failed: code={} | {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    let download_url = validated_public_http_url(
        response.file.download_url.as_str(),
        "MiniMax video download URL",
    )?;
    Ok(ProviderToolExecutionResult {
        upstream_response_id: Some(task_id.to_string()),
        output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
            output_id: String::from("video-0"),
            kind: String::from("video"),
            mime_type: String::from("video/mp4"),
            bytes: None,
            download_url: Some(download_url.to_string()),
        }]),
    })
}

/// Returns the provider default MiniMax video duration.
/// 返回供应商默认 MiniMax 视频时长。
fn default_minimax_video_duration() -> u8 {
    6
}

/// Strict AI-visible Tavily search arguments.
/// 严格的 AI 可见 Tavily 搜索参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TavilySearchToolArguments {
    /// Exact query text.
    /// 精确查询文本。
    query: String,
    /// Optional result limit.
    /// 可选结果数量上限。
    #[serde(default)]
    max_results: Option<u8>,
    /// Optional exact domain allowlist.
    /// 可选精确域名允许列表。
    #[serde(default)]
    include_domains: Vec<String>,
    /// Optional exact domain blocklist.
    /// 可选精确域名阻止列表。
    #[serde(default)]
    exclude_domains: Vec<String>,
}

impl TavilySearchToolArguments {
    /// Validates semantic constraints not expressible through deserialization alone.
    /// 验证仅靠反序列化无法表达的语义约束。
    ///
    /// # Returns
    /// 返回值
    /// Success when the selected Tavily target accepts every argument.
    /// 当选定 Tavily 目标接受全部参数时返回成功。
    fn validate(&self) -> Result<(), String> {
        if self.query.trim().is_empty() {
            return Err(String::from("Tavily search query cannot be empty."));
        }
        if self
            .max_results
            .is_some_and(|maximum| !(1..=20).contains(&maximum))
        {
            return Err(String::from(
                "Tavily search max_results must be between one and twenty.",
            ));
        }
        validate_domain_list("include_domains", &self.include_domains, 300)?;
        validate_domain_list("exclude_domains", &self.exclude_domains, 150)
    }
}

/// Exact adapter-owned Tavily search wire request.
/// 适配器持有的精确 Tavily 搜索 Wire 请求。
#[derive(Debug, Serialize)]
struct TavilySearchWireRequest {
    /// Exact query text.
    /// 精确查询文本。
    query: String,
    /// Fixed predictable search depth.
    /// 固定且可预测的搜索深度。
    search_depth: String,
    /// Optional result limit.
    /// 可选结果数量上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    max_results: Option<u8>,
    /// Optional domain allowlist.
    /// 可选域名允许列表。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    include_domains: Vec<String>,
    /// Optional domain blocklist.
    /// 可选域名阻止列表。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    exclude_domains: Vec<String>,
    /// Fixed result-only behavior.
    /// 固定的仅结果行为。
    include_answer: bool,
    /// Prevents implicit full-page extraction.
    /// 阻止隐式完整网页提取。
    include_raw_content: bool,
    /// Requests exact provider credit accounting.
    /// 请求精确的供应商 Credit 计量。
    include_usage: bool,
}

/// Exact Tavily structured-search response subset consumed by the adapter.
/// 适配器消费的精确 Tavily 结构化搜索响应子集。
#[derive(Debug, Deserialize)]
struct TavilySearchWireResponse {
    /// Actual executed query.
    /// 实际执行的查询。
    query: String,
    /// Ordered provider results.
    /// 有序供应商结果。
    results: Vec<TavilySearchWireResult>,
    /// Provider support identifier.
    /// 供应商支持标识。
    request_id: String,
}

/// One Tavily structured-search result.
/// 一条 Tavily 结构化搜索结果。
#[derive(Debug, Deserialize)]
struct TavilySearchWireResult {
    /// Provider-returned title.
    /// 供应商返回的标题。
    title: String,
    /// Provider-returned destination.
    /// 供应商返回的目标地址。
    url: String,
    /// Provider-returned excerpt.
    /// 供应商返回的摘要。
    content: String,
}

/// Decodes one Tavily search response into the unified search output.
/// 将一份 Tavily 搜索响应解码为统一搜索输出。
///
/// # Parameters
/// 参数
/// `body` is the complete provider JSON document.
/// `body` 是完整供应商 JSON 文档。
///
/// # Returns
/// 返回值
/// Validated ordered search results and the provider request identifier.
/// 返回已验证有序搜索结果与供应商请求标识。
fn decode_tavily_search_response(body: &str) -> Result<ProviderToolExecutionResult, String> {
    let response: TavilySearchWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("Tavily search response JSON is invalid: {error}"))?;
    if response.query.trim().is_empty() {
        return Err(String::from(
            "Tavily search response omitted the executed query.",
        ));
    }
    let results = response
        .results
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let url = validated_https_url(
                result.url.as_str(),
                format!("Tavily search result {index}").as_str(),
            )?;
            Ok(ProviderToolSearchResult {
                rank: index + 1,
                title: result.title,
                url: result.url,
                source_domain: url.host_str().unwrap_or_default().to_ascii_lowercase(),
                snippet: result.content,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProviderToolExecutionResult {
        upstream_response_id: non_empty_identifier(response.request_id),
        output: ProviderToolNormalizedOutput::Search(ProviderToolSearchOutput {
            query: response.query,
            results,
        }),
    })
}

/// Strict AI-visible Tavily extraction arguments.
/// 严格的 AI 可见 Tavily 网页提取参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TavilyExtractToolArguments {
    /// One to twenty HTTPS URLs.
    /// 一到二十个 HTTPS 地址。
    urls: Vec<String>,
    /// Optional relevance query.
    /// 可选相关性查询。
    #[serde(default)]
    query: Option<String>,
    /// Optional relevant chunks per source.
    /// 每个来源的可选相关片段数。
    #[serde(default)]
    chunks_per_source: Option<u8>,
    /// Extraction depth.
    /// 提取深度。
    #[serde(default = "default_tavily_extract_depth")]
    extract_depth: String,
    /// Output text format.
    /// 输出文本格式。
    #[serde(default = "default_tavily_extract_format")]
    format: String,
    /// Whether extracted images are requested.
    /// 是否请求提取图片。
    #[serde(default)]
    include_images: bool,
    /// Whether favicons are requested.
    /// 是否请求站点图标。
    #[serde(default)]
    include_favicon: bool,
    /// Optional provider timeout in seconds.
    /// 可选供应商超时秒数。
    #[serde(default)]
    timeout_seconds: Option<f64>,
}

impl TavilyExtractToolArguments {
    /// Validates selected-target extraction constraints.
    /// 验证选定目标的网页提取约束。
    ///
    /// # Returns
    /// 返回值
    /// Success when all URLs and dependent fields match Tavily's contract.
    /// 当全部地址与依赖字段符合 Tavily 契约时返回成功。
    fn validate(&self) -> Result<(), String> {
        if self.urls.is_empty() || self.urls.len() > 20 {
            return Err(String::from(
                "Tavily extract requires between one and twenty URLs.",
            ));
        }
        let mut unique = std::collections::BTreeSet::new();
        for (index, url) in self.urls.iter().enumerate() {
            validated_https_url(url, format!("Tavily extract URL {index}").as_str())?;
            if !unique.insert(url) {
                return Err(format!("Tavily extract URL {index} is duplicated."));
            }
        }
        if self
            .query
            .as_deref()
            .is_some_and(|query| query.trim().is_empty())
        {
            return Err(String::from(
                "Tavily extract query cannot be empty when provided.",
            ));
        }
        if self.chunks_per_source.is_some() && self.query.is_none() {
            return Err(String::from(
                "Tavily extract chunks_per_source requires query.",
            ));
        }
        if self
            .chunks_per_source
            .is_some_and(|chunks| !(1..=5).contains(&chunks))
        {
            return Err(String::from(
                "Tavily extract chunks_per_source must be between one and five.",
            ));
        }
        if !matches!(self.extract_depth.as_str(), "basic" | "advanced") {
            return Err(String::from(
                "Tavily extract extract_depth must be basic or advanced.",
            ));
        }
        if !matches!(self.format.as_str(), "markdown" | "text") {
            return Err(String::from(
                "Tavily extract format must be markdown or text.",
            ));
        }
        if self
            .timeout_seconds
            .is_some_and(|timeout| !(1.0..=60.0).contains(&timeout))
        {
            return Err(String::from(
                "Tavily extract timeout_seconds must be between one and sixty.",
            ));
        }
        Ok(())
    }
}

/// Exact adapter-owned Tavily extraction wire request.
/// 适配器持有的精确 Tavily 网页提取 Wire 请求。
#[derive(Debug, Serialize)]
struct TavilyExtractWireRequest {
    /// Requested HTTPS URLs.
    /// 请求的 HTTPS 地址。
    urls: Vec<String>,
    /// Optional relevance query.
    /// 可选相关性查询。
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    /// Optional relevant chunks per source.
    /// 每个来源的可选相关片段数。
    #[serde(skip_serializing_if = "Option::is_none")]
    chunks_per_source: Option<u8>,
    /// Provider extraction depth.
    /// 供应商提取深度。
    extract_depth: String,
    /// Whether images are requested.
    /// 是否请求图片。
    include_images: bool,
    /// Whether favicons are requested.
    /// 是否请求站点图标。
    include_favicon: bool,
    /// Provider output format.
    /// 供应商输出格式。
    format: String,
    /// Optional provider timeout.
    /// 可选供应商超时。
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<f64>,
    /// Requests exact provider credit accounting.
    /// 请求精确供应商 Credit 计量。
    include_usage: bool,
}

/// Exact Tavily extraction response subset consumed by the adapter.
/// 适配器消费的精确 Tavily 网页提取响应子集。
#[derive(Debug, Deserialize)]
struct TavilyExtractWireResponse {
    /// Ordered successful pages.
    /// 有序成功页面。
    #[serde(default)]
    results: Vec<TavilyExtractWireResult>,
    /// Ordered provider failures.
    /// 有序供应商失败。
    #[serde(default)]
    failed_results: Vec<TavilyExtractWireFailure>,
    /// Provider support identifier.
    /// 供应商支持标识。
    request_id: String,
}

/// One successful Tavily extraction result.
/// 一条成功 Tavily 网页提取结果。
#[derive(Debug, Deserialize)]
struct TavilyExtractWireResult {
    /// Extracted page URL.
    /// 已提取页面地址。
    url: String,
    /// Requested Markdown or text content.
    /// 请求的 Markdown 或文本内容。
    raw_content: String,
    /// Optional extracted image URLs.
    /// 可选提取图片地址。
    #[serde(default)]
    images: Vec<String>,
    /// Optional favicon URL.
    /// 可选站点图标地址。
    #[serde(default)]
    favicon: Option<String>,
}

/// One provider-reported Tavily extraction failure.
/// 一条供应商报告的 Tavily 网页提取失败。
#[derive(Debug, Deserialize)]
struct TavilyExtractWireFailure {
    /// Failed page URL.
    /// 失败页面地址。
    url: String,
    /// Provider failure message.
    /// 供应商失败消息。
    error: String,
}

/// Decodes one Tavily extraction response into the unified extract output.
/// 将一份 Tavily 网页提取响应解码为统一提取输出。
///
/// # Parameters
/// 参数
/// `body` is the complete provider JSON document.
/// `body` 是完整供应商 JSON 文档。
///
/// # Returns
/// 返回值
/// Validated successful pages, failures, and the provider request identifier.
/// 返回已验证成功页面、失败与供应商请求标识。
fn decode_tavily_extract_response(body: &str) -> Result<ProviderToolExecutionResult, String> {
    let response: TavilyExtractWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("Tavily extract response JSON is invalid: {error}"))?;
    let results = response
        .results
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            validated_https_url(
                result.url.as_str(),
                format!("Tavily extract result {index}").as_str(),
            )?;
            let images = result
                .images
                .into_iter()
                .enumerate()
                .map(|(image_index, url)| {
                    validated_https_url(
                        url.as_str(),
                        format!("Tavily extract result {index} image {image_index}").as_str(),
                    )?;
                    Ok::<String, String>(url)
                })
                .collect::<Result<Vec<_>, String>>()?;
            let favicon = result
                .favicon
                .filter(|value| !value.trim().is_empty())
                .map(|url| {
                    validated_https_url(
                        url.as_str(),
                        format!("Tavily extract result {index} favicon").as_str(),
                    )?;
                    Ok::<String, String>(url)
                })
                .transpose()?;
            Ok(ProviderToolExtractResult {
                url: result.url,
                content: result.raw_content,
                images,
                favicon,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let failures = response
        .failed_results
        .into_iter()
        .enumerate()
        .map(|(index, failure)| {
            validated_https_url(
                failure.url.as_str(),
                format!("Tavily extract failure {index}").as_str(),
            )?;
            Ok(ProviderToolExtractFailure {
                url: failure.url,
                error: failure.error,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProviderToolExecutionResult {
        upstream_response_id: non_empty_identifier(response.request_id),
        output: ProviderToolNormalizedOutput::Extract(ProviderToolExtractOutput {
            results,
            failures,
        }),
    })
}

/// Validates an absolute HTTPS URL without accepting credentials or fragments as alternate sources.
/// 验证绝对 HTTPS 地址，且不把凭据或片段接受为其他来源。
///
/// # Parameters
/// 参数
/// `value` is the provider or AI supplied URL and `label` scopes diagnostics.
/// `value` 是供应商或 AI 提供的地址，`label` 用于限定诊断。
///
/// # Returns
/// 返回值
/// The parsed URL when it is an absolute credential-free HTTPS resource.
/// 当地址为不含凭据的绝对 HTTPS 资源时返回解析结果。
fn validated_https_url(value: &str, label: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!(
            "{label} must be an absolute credential-free HTTPS URL."
        ));
    }
    Ok(url)
}

/// Validates one Tavily domain filter list.
/// 验证一份 Tavily 域名过滤列表。
///
/// # Parameters
/// 参数
/// `name` scopes diagnostics, `domains` are exact values, and `maximum` is provider-owned.
/// `name` 用于限定诊断，`domains` 是精确值，`maximum` 由供应商规定。
///
/// # Returns
/// 返回值
/// Success when every value is non-empty, unique, and within the provider count limit.
/// 当每个值非空、唯一且未超过供应商数量上限时返回成功。
fn validate_domain_list(name: &str, domains: &[String], maximum: usize) -> Result<(), String> {
    if domains.len() > maximum {
        return Err(format!(
            "Tavily search {name} exceeds the maximum of {maximum} domains."
        ));
    }
    let mut unique = std::collections::BTreeSet::new();
    for (index, domain) in domains.iter().enumerate() {
        if domain.trim().is_empty() {
            return Err(format!("Tavily search {name}[{index}] cannot be empty."));
        }
        if !unique.insert(domain) {
            return Err(format!("Tavily search {name}[{index}] is duplicated."));
        }
    }
    Ok(())
}

/// Retains one non-empty provider support identifier.
/// 保留一个非空供应商支持标识。
///
/// # Parameters
/// 参数
/// `value` is the exact provider-returned identifier.
/// `value` 是供应商返回的精确标识。
///
/// # Returns
/// 返回值
/// The identifier when non-empty, otherwise `None`.
/// 标识非空时返回该标识，否则返回 `None`。
fn non_empty_identifier(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

/// Returns Tavily's default extraction depth.
/// 返回 Tavily 默认提取深度。
fn default_tavily_extract_depth() -> String {
    String::from("basic")
}

/// Returns Tavily's default extraction format.
/// 返回 Tavily 默认提取格式。
fn default_tavily_extract_format() -> String {
    String::from("markdown")
}

/// Strict AI-visible MiniMax search arguments.
/// 严格的 AI 可见 MiniMax 搜索参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MiniMaxSearchToolArguments {
    /// Exact query text.
    /// 精确查询文本。
    query: String,
    /// Optional client-side result limit.
    /// 可选客户端结果数量限制。
    #[serde(default)]
    max_results: Option<usize>,
}

/// Exact MiniMax structured search response.
/// 精确的 MiniMax 结构化搜索响应。
#[derive(Debug, Deserialize)]
struct MiniMaxSearchWireResponse {
    /// Ordered organic search results.
    /// 有序自然搜索结果。
    #[serde(default)]
    organic: Vec<MiniMaxSearchWireResult>,
    /// Application status.
    /// 应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// One MiniMax upstream search result.
/// 一条 MiniMax 上游搜索结果。
#[derive(Debug, Deserialize)]
struct MiniMaxSearchWireResult {
    /// Result title.
    /// 结果标题。
    title: String,
    /// Result destination.
    /// 结果目标地址。
    link: String,
    /// Result excerpt.
    /// 结果摘要。
    snippet: String,
}

/// Decodes and normalizes one MiniMax structured search response.
/// 解码并归一化一份 MiniMax 结构化搜索响应。
///
/// # Parameters
/// 参数
/// `query`, `maximum`, and `body` are frozen invocation values.
/// `query`、`maximum` 与 `body` 是冻结的调用值。
///
/// # Returns
/// 返回值
/// Validated ordered HTTP(S) search results.
/// 返回已校验的有序 HTTP(S) 搜索结果。
fn decode_minimax_search_response(
    query: &str,
    maximum: usize,
    body: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let response: MiniMaxSearchWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax search response JSON is invalid: {error}"))?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax search request failed: code={} | {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    let mut results = Vec::new();
    for (index, result) in response.organic.into_iter().take(maximum).enumerate() {
        let url = reqwest::Url::parse(result.link.as_str())
            .map_err(|_| format!("MiniMax search result {index} contains an invalid URL."))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(format!(
                "MiniMax search result {index} must use an absolute HTTP(S) URL."
            ));
        }
        results.push(ProviderToolSearchResult {
            rank: index + 1,
            title: result.title,
            url: result.link,
            source_domain: url.host_str().unwrap_or_default().to_ascii_lowercase(),
            snippet: result.snippet,
        });
    }
    Ok(ProviderToolExecutionResult {
        upstream_response_id: None,
        output: ProviderToolNormalizedOutput::Search(ProviderToolSearchOutput {
            query: query.to_string(),
            results,
        }),
    })
}

/// Parsed AI-visible MiniMax image arguments after local resources are materialized.
/// 本地资源物化后的已解析 AI 可见 MiniMax 图片参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MiniMaxImageToolArguments {
    /// Image description.
    /// 图片描述。
    prompt: String,
    /// Host-materialized JPEG or PNG Data URLs.
    /// 宿主物化后的 JPEG 或 PNG Data URL。
    #[serde(default)]
    subject_references: Vec<String>,
    /// Optional preset aspect ratio.
    /// 可选预设宽高比。
    #[serde(default)]
    aspect_ratio: Option<String>,
    /// Requested output count.
    /// 请求输出数量。
    #[serde(default = "default_image_count")]
    count: u8,
    /// Optional deterministic seed.
    /// 可选确定性随机种子。
    #[serde(default)]
    seed: Option<i64>,
    /// Provider prompt optimizer.
    /// 供应商提示词优化开关。
    #[serde(default)]
    prompt_optimizer: bool,
    /// Provider AIGC watermark.
    /// 供应商 AIGC 水印开关。
    #[serde(default)]
    aigc_watermark: bool,
    /// image-01 custom width.
    /// image-01 自定义宽度。
    #[serde(default)]
    width: Option<u16>,
    /// image-01 custom height.
    /// image-01 自定义高度。
    #[serde(default)]
    height: Option<u16>,
    /// image-01-live style.
    /// image-01-live 风格。
    #[serde(default)]
    style_type: Option<String>,
    /// image-01-live style strength.
    /// image-01-live 风格强度。
    #[serde(default)]
    style_weight: Option<f64>,
}

impl MiniMaxImageToolArguments {
    /// Parses a strict selected-model argument object.
    /// 解析严格的选定模型参数对象。
    ///
    /// # Parameters
    /// 参数
    /// `arguments` is the AI-visible JSON object.
    /// `arguments` 是 AI 可见 JSON 对象。
    ///
    /// # Returns
    /// 返回值
    /// Typed arguments or a controlled schema error.
    /// 返回类型化参数或受控 Schema 错误。
    fn from_json(arguments: Value) -> Result<Self, String> {
        serde_json::from_value(arguments)
            .map_err(|error| format!("MiniMax image tool arguments are invalid: {error}"))
    }

    /// Projects exact model-specific MiniMax JSON.
    /// 投影精确的模型专属 MiniMax JSON。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is frozen by the selected target.
    /// `model_id` 由选定目标冻结。
    ///
    /// # Returns
    /// 返回值
    /// A validated request containing no local paths.
    /// 返回不含本地路径的已校验请求。
    fn into_wire_body(self, model_id: &str) -> Result<MiniMaxImageWireRequest, String> {
        validate_image_arguments(model_id, &self)?;
        let style = image_style(model_id, self.style_type.as_deref(), self.style_weight)?;
        let subject_reference = self
            .subject_references
            .into_iter()
            .map(|image_file| {
                if !image_file.starts_with("data:image/jpeg;base64,")
                    && !image_file.starts_with("data:image/png;base64,")
                {
                    return Err(String::from(
                        "MiniMax subject references must be host-materialized JPEG or PNG Data URLs.",
                    ));
                }
                Ok(MiniMaxImageSubjectReference {
                    kind: String::from("character"),
                    image_file,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(MiniMaxImageWireRequest {
            model: model_id.to_string(),
            prompt: self.prompt,
            subject_reference,
            aspect_ratio: self.aspect_ratio,
            width: self.width,
            height: self.height,
            response_format: String::from("base64"),
            seed: self.seed,
            count: self.count,
            prompt_optimizer: self.prompt_optimizer,
            aigc_watermark: self.aigc_watermark,
            style,
        })
    }
}

/// Exact MiniMax image request body.
/// 精确 MiniMax 图片请求体。
#[derive(Debug, Serialize)]
struct MiniMaxImageWireRequest {
    /// Frozen upstream model.
    /// 冻结的上游模型。
    model: String,
    /// Image prompt.
    /// 图片提示词。
    prompt: String,
    /// Character references.
    /// 角色参考图。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    subject_reference: Vec<MiniMaxImageSubjectReference>,
    /// Preset ratio.
    /// 预设宽高比。
    #[serde(skip_serializing_if = "Option::is_none")]
    aspect_ratio: Option<String>,
    /// Custom width.
    /// 自定义宽度。
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u16>,
    /// Custom height.
    /// 自定义高度。
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u16>,
    /// Private inline result carrier.
    /// 私有内联结果载体。
    response_format: String,
    /// Deterministic seed.
    /// 确定性随机种子。
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    /// Output count.
    /// 输出数量。
    #[serde(rename = "n")]
    count: u8,
    /// Prompt optimizer.
    /// 提示词优化开关。
    prompt_optimizer: bool,
    /// AIGC watermark.
    /// AIGC 水印开关。
    aigc_watermark: bool,
    /// Live-model style.
    /// Live 模型风格。
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<MiniMaxImageStyle>,
}

/// One MiniMax character reference.
/// 一项 MiniMax 角色参考图。
#[derive(Debug, Serialize)]
struct MiniMaxImageSubjectReference {
    /// Official fixed reference type.
    /// 官方固定参考类型。
    #[serde(rename = "type")]
    kind: String,
    /// Inline image carrier.
    /// 内联图片载体。
    image_file: String,
}

/// MiniMax live-model style.
/// MiniMax Live 模型风格。
#[derive(Debug, Serialize)]
struct MiniMaxImageStyle {
    /// Official style identifier.
    /// 官方风格标识。
    style_type: String,
    /// Optional style strength.
    /// 可选风格强度。
    #[serde(skip_serializing_if = "Option::is_none")]
    style_weight: Option<f64>,
}

/// Closed MiniMax image response.
/// 封闭 MiniMax 图片响应。
#[derive(Debug, Deserialize)]
struct MiniMaxImageWireResponse {
    /// Image output data.
    /// 图片输出数据。
    data: MiniMaxImageWireResponseData,
    /// Application status.
    /// 应用状态。
    base_resp: MiniMaxBaseResponse,
}

/// Ordered MiniMax image carriers.
/// 有序 MiniMax 图片载体。
#[derive(Debug, Deserialize)]
struct MiniMaxImageWireResponseData {
    /// Provider task identifier.
    /// 供应商任务标识。
    task_id: String,
    /// Private Base64 JPEG outputs.
    /// 私有 Base64 JPEG 输出。
    #[serde(default)]
    image_base64: Vec<String>,
    /// Temporary output URLs.
    /// 临时输出 URL。
    #[serde(default)]
    image_urls: Vec<String>,
    /// Successful count.
    /// 成功数量。
    success_count: usize,
    /// Failed count.
    /// 失败数量。
    failed_count: usize,
}

/// MiniMax application result.
/// MiniMax 应用结果。
#[derive(Debug, Deserialize)]
struct MiniMaxBaseResponse {
    /// Zero means success.
    /// 零表示成功。
    status_code: i64,
    /// Safe provider status message.
    /// 安全的供应商状态消息。
    status_msg: String,
}

/// Returns the provider default image count.
/// 返回供应商默认图片数量。
fn default_image_count() -> u8 {
    1
}

/// Validates the exact selected-model argument constraints.
/// 校验精确的选定模型参数约束。
///
/// # Parameters
/// 参数
/// `model_id` and `arguments` are frozen invocation values.
/// `model_id` 与 `arguments` 是冻结的调用值。
///
/// # Returns
/// 返回值
/// Success only when the provider model accepts every value.
/// 仅当供应商模型接受全部值时返回成功。
fn validate_image_arguments(
    model_id: &str,
    arguments: &MiniMaxImageToolArguments,
) -> Result<(), String> {
    if !matches!(model_id, "image-01" | "image-01-live") {
        return Err(format!(
            "MiniMax image model `{model_id}` is not registered."
        ));
    }
    if !(1..=1_500).contains(&arguments.prompt.chars().count()) {
        return Err(String::from(
            "MiniMax image prompt must contain one to 1500 Unicode characters.",
        ));
    }
    if !(1..=9).contains(&arguments.count) {
        return Err(String::from(
            "MiniMax image count must be between one and nine.",
        ));
    }
    if !image_ratio_is_valid(model_id, arguments.aspect_ratio.as_deref()) {
        return Err(String::from(
            "MiniMax image aspect ratio is not supported by the selected model.",
        ));
    }
    if model_id == "image-01-live" && (arguments.width.is_some() || arguments.height.is_some()) {
        return Err(String::from(
            "MiniMax image-01-live does not accept width or height.",
        ));
    }
    if arguments.width.is_some() != arguments.height.is_some()
        || (arguments.width.is_some() && arguments.aspect_ratio.is_some())
    {
        return Err(String::from(
            "MiniMax image width and height must appear together and cannot be combined with aspect_ratio.",
        ));
    }
    if let (Some(width), Some(height)) = (arguments.width, arguments.height)
        && (!(512..=2_048).contains(&width)
            || !(512..=2_048).contains(&height)
            || width % 8 != 0
            || height % 8 != 0)
    {
        return Err(String::from(
            "MiniMax image dimensions must be 512 to 2048 and divisible by eight.",
        ));
    }
    Ok(())
}

/// Reports whether one optional ratio belongs to the selected model.
/// 报告一个可选宽高比是否属于选定模型。
fn image_ratio_is_valid(model_id: &str, aspect_ratio: Option<&str>) -> bool {
    let Some(aspect_ratio) = aspect_ratio else {
        return true;
    };
    matches!(
        aspect_ratio,
        "1:1" | "16:9" | "4:3" | "3:2" | "2:3" | "3:4" | "9:16"
    ) || (model_id == "image-01" && aspect_ratio == "21:9")
}

/// Builds and validates image-01-live style parameters.
/// 构建并校验 image-01-live 风格参数。
fn image_style(
    model_id: &str,
    style_type: Option<&str>,
    style_weight: Option<f64>,
) -> Result<Option<MiniMaxImageStyle>, String> {
    if model_id == "image-01" {
        if style_type.is_some() || style_weight.is_some() {
            return Err(String::from(
                "MiniMax image-01 does not accept live-model style parameters.",
            ));
        }
        return Ok(None);
    }
    let Some(style_type) = style_type else {
        if style_weight.is_some() {
            return Err(String::from("MiniMax style_weight requires style_type."));
        }
        return Ok(None);
    };
    if !matches!(style_type, "漫画" | "元气" | "中世纪" | "水彩") {
        return Err(String::from("MiniMax style_type is not supported."));
    }
    if style_weight.is_some_and(|weight| !weight.is_finite() || weight <= 0.0 || weight > 1.0) {
        return Err(String::from(
            "MiniMax style_weight must be greater than zero and at most one.",
        ));
    }
    Ok(Some(MiniMaxImageStyle {
        style_type: style_type.to_string(),
        style_weight,
    }))
}

/// Inserts one exact direct-tool authentication header.
/// 插入一个精确的直连工具鉴权请求头。
fn insert_authentication_header(
    headers: &mut HeaderMap,
    projection: AuthenticationProjection,
    secret: &str,
) -> Result<(), String> {
    let (name, value) = match projection {
        AuthenticationProjection::BearerAuthorization => {
            (reqwest::header::AUTHORIZATION, format!("Bearer {secret}"))
        }
        AuthenticationProjection::XApiKey => {
            (HeaderName::from_static("x-api-key"), secret.to_string())
        }
        _ => {
            return Err(String::from(
                "The selected provider tool authentication projection is not registered.",
            ));
        }
    };
    let value = HeaderValue::from_str(value.as_str())
        .map_err(|_| String::from("Provider tool authentication contains invalid header bytes."))?;
    headers.insert(name, value);
    Ok(())
}

/// Decodes and validates one successful MiniMax image response.
/// 解码并校验一份成功的 MiniMax 图片响应。
fn decode_minimax_image_response(body: &str) -> Result<ProviderToolExecutionResult, String> {
    let response: MiniMaxImageWireResponse = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax image response JSON is invalid: {error}"))?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax image request failed: code={} | {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    let base64_count = response.data.image_base64.len();
    let url_count = response.data.image_urls.len();
    if response.data.task_id.trim().is_empty()
        || (base64_count == 0 && url_count == 0)
        || (base64_count != 0 && url_count != 0)
        || response.data.success_count != base64_count + url_count
        || response.data.failed_count > 0
    {
        return Err(String::from(
            "MiniMax image response fields are inconsistent.",
        ));
    }
    let mut resources = Vec::with_capacity(response.data.success_count);
    for (index, encoded) in response.data.image_base64.into_iter().enumerate() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| format!("MiniMax image output {index} contains invalid Base64."))?;
        if bytes.is_empty() {
            return Err(format!("MiniMax image output {index} is empty."));
        }
        resources.push(ProviderToolGeneratedResource {
            output_id: format!("image-{index}"),
            kind: String::from("image"),
            mime_type: String::from("image/jpeg"),
            bytes: Some(bytes),
            download_url: None,
        });
    }
    for (index, download_url) in response.data.image_urls.into_iter().enumerate() {
        let parsed = reqwest::Url::parse(download_url.as_str())
            .map_err(|_| format!("MiniMax image output {index} contains an invalid URL."))?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err(format!(
                "MiniMax image output {index} must use an absolute HTTPS URL."
            ));
        }
        resources.push(ProviderToolGeneratedResource {
            output_id: format!("image-{index}"),
            kind: String::from("image"),
            mime_type: String::from("image/jpeg"),
            bytes: None,
            download_url: Some(download_url),
        });
    }
    Ok(ProviderToolExecutionResult {
        upstream_response_id: Some(response.data.task_id),
        output: ProviderToolNormalizedOutput::Resources(resources),
    })
}

/// Formats one provider tool HTTP error without exposing request or credential data.
/// 格式化一个供应商工具 HTTP 错误且不暴露请求或凭据数据。
fn format_http_error(label: &str, status: u16, body: &str) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/base_resp/status_msg")
                .or_else(|| value.pointer("/error/message"))
                .or_else(|| value.pointer("/detail/error"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from("The provider returned an unreadable error response."));
    format!("{label} request failed: HTTP {status} | {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies MiniMax inline images normalize into typed resource bytes.
    /// 验证 MiniMax 内联图片会归一化为强类型资源字节。
    #[test]
    fn decodes_minimax_inline_image_response() {
        let result = decode_minimax_image_response(
            r#"{"data":{"task_id":"task-1","image_base64":["aGVsbG8="],"success_count":1,"failed_count":0},"base_resp":{"status_code":0,"status_msg":""}}"#,
        )
        .expect("valid image response");
        assert_eq!(result.upstream_response_id.as_deref(), Some("task-1"));
        let ProviderToolNormalizedOutput::Resources(resources) = result.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources[0].bytes.as_deref(), Some("hello".as_bytes()));
    }

    /// Verifies Qwen image generation projects the reviewed synchronous wire shape.
    /// 验证 Qwen 图片生成投影已审核同步 Wire 结构。
    #[test]
    fn projects_alibaba_qwen_image_request() {
        let target = crate::built_in_provider_tool_targets("alibaba")
            .into_iter()
            .find(|target| {
                target.service_entry_id == "model-studio-cn"
                    && target.execution.adapter == ProviderToolAdapterKind::AlibabaQwenImageGenerate
            })
            .expect("Qwen generation target");
        let arguments: AlibabaQwenImageGenerateArguments =
            serde_json::from_value(serde_json::json!({
                "prompt": "A blue forge",
                "width": 1024,
                "height": 1024,
                "count": 2,
                "prompt_extend": false
            }))
            .expect("typed Qwen arguments");
        let body =
            build_alibaba_qwen_generate_body(&target, arguments).expect("valid Qwen request");
        let value = serde_json::to_value(body).expect("serializable Qwen body");
        assert_eq!(value["model"], "qwen-image-2.0-pro");
        assert_eq!(value["input"]["messages"][0]["role"], "user");
        assert_eq!(
            value["input"]["messages"][0]["content"][0]["text"],
            "A blue forge"
        );
        assert_eq!(value["parameters"]["size"], "1024*1024");
        assert_eq!(value["parameters"]["n"], 2);
        assert_eq!(value["parameters"]["prompt_extend"], false);
    }

    /// Verifies Wan 4k output rejects reference-image generation exactly as documented.
    /// 验证 Wan 4k 输出会按文档精确拒绝参考图生成。
    #[test]
    fn rejects_alibaba_wan_4k_reference_request() {
        let target = crate::built_in_provider_tool_targets("alibaba")
            .into_iter()
            .find(|target| {
                target.upstream_target_id == "wan2.7-image-pro"
                    && target.execution.adapter == ProviderToolAdapterKind::AlibabaWanImageGenerate
            })
            .expect("Wan Pro target");
        let arguments = serde_json::json!({
            "prompt": "Retain this composition.",
            "resolution": "4k",
            "reference_images": ["data:image/png;base64,aGVsbG8="]
        });
        let parsed: AlibabaWanImageGenerateArguments =
            serde_json::from_value(arguments).expect("typed Wan arguments");
        let error = build_alibaba_wan_generate_body(&target, parsed)
            .expect_err("Wan Pro 4k reference generation must be rejected");
        assert_eq!(
            error,
            "Alibaba Wan 4k output is limited to wan2.7-image-pro text-only generation."
        );
    }

    /// Verifies Alibaba temporary image URLs normalize into shared resource output.
    /// 验证阿里云临时图片 URL 会归一化到共享资源输出。
    #[test]
    fn decodes_alibaba_image_response() {
        let result = decode_alibaba_image_response(
            "Alibaba image",
            r#"{"request_id":"request-1","output":{"choices":[{"message":{"content":[{"image":"https://example.com/generated.png"}]}}]}}"#,
        )
        .expect("valid Alibaba image response");
        assert_eq!(result.upstream_response_id.as_deref(), Some("request-1"));
        let ProviderToolNormalizedOutput::Resources(resources) = result.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].mime_type, "image/png");
        assert_eq!(
            resources[0].download_url.as_deref(),
            Some("https://example.com/generated.png")
        );
    }

    /// Verifies CosyVoice projects only supplied native fields and the reviewed MIME type.
    /// 验证 CosyVoice 仅投影已提供原生字段与已审核 MIME 类型。
    #[test]
    fn projects_alibaba_cosyvoice_request_without_null_fields() {
        let target = crate::built_in_provider_tool_targets("alibaba")
            .into_iter()
            .find(|target| {
                target.service_entry_id == "model-studio-cn"
                    && target.upstream_target_id == "cosyvoice-v3-flash"
            })
            .expect("CosyVoice target");
        let arguments: AlibabaCosyVoiceArguments = serde_json::from_value(serde_json::json!({
            "text": "你好，Vulcan。",
            "voice_id": "longxiaochun",
            "output_format": "wav",
            "speed": 1.2
        }))
        .expect("typed CosyVoice arguments");
        let (body, mime_type) =
            build_alibaba_cosyvoice_body(&target, arguments).expect("valid CosyVoice body");
        assert_eq!(body["model"], "cosyvoice-v3-flash");
        assert_eq!(body["input"]["voice"], "longxiaochun");
        assert_eq!(body["input"]["format"], "wav");
        assert_eq!(body["input"]["rate"], 1.2);
        assert!(body["input"].get("sample_rate").is_none());
        assert!(body["input"].get("language_hints").is_none());
        assert_eq!(mime_type, "audio/wav");
    }

    /// Verifies Qwen3-TTS exposes instructions only for the instruct model.
    /// 验证 Qwen3-TTS 仅为指令模型暴露表达指令。
    #[test]
    fn projects_alibaba_qwen_tts_model_specific_request() {
        let targets = crate::built_in_provider_tool_targets("alibaba");
        let plain = targets
            .iter()
            .find(|target| {
                target.service_entry_id == "model-studio-cn"
                    && target.upstream_target_id == "qwen3-tts-flash"
            })
            .expect("plain Qwen3-TTS target");
        let instruct = targets
            .iter()
            .find(|target| {
                target.service_entry_id == "model-studio-cn"
                    && target.upstream_target_id == "qwen3-tts-instruct-flash"
            })
            .expect("instruct Qwen3-TTS target");
        let plain_error = build_alibaba_qwen_tts_body(
            plain,
            AlibabaQwenTtsArguments {
                text: String::from("Hello."),
                voice_id: String::from("Cherry"),
                language: None,
                style: Some(String::from("Speak warmly.")),
            },
        )
        .expect_err("plain TTS must reject instructions");
        assert!(plain_error.contains("requires qwen3-tts-instruct-flash"));
        let body = build_alibaba_qwen_tts_body(
            instruct,
            AlibabaQwenTtsArguments {
                text: String::from("Hello."),
                voice_id: String::from("Cherry"),
                language: Some(String::from("English")),
                style: Some(String::from("Speak warmly.")),
            },
        )
        .expect("valid instruct TTS body");
        assert_eq!(body["input"]["instructions"], "Speak warmly.");
        assert_eq!(body["input"]["optimize_instructions"], true);
        assert_eq!(body["input"]["language_type"], "English");
    }

    /// Verifies Qwen3-ASR emits one bounded Data URL and omits an absent language.
    /// 验证 Qwen3-ASR 发送一个有界 Data URL 并省略缺失语言。
    #[test]
    fn projects_alibaba_qwen_asr_request_without_null_language() {
        let target = crate::built_in_provider_tool_targets("alibaba")
            .into_iter()
            .find(|target| {
                target.service_entry_id == "model-studio-singapore"
                    && target.upstream_target_id == "qwen3-asr-flash"
            })
            .expect("Qwen3-ASR target");
        let body = build_alibaba_qwen_asr_body(
            &target,
            AlibabaQwenAsrArguments {
                audio_url: String::from("data:audio/mpeg;base64,aGVsbG8="),
                language: None,
            },
        )
        .expect("valid Qwen3-ASR body");
        assert_eq!(
            body["input"]["messages"][0]["content"][0]["audio"],
            "data:audio/mpeg;base64,aGVsbG8="
        );
        assert_eq!(body["parameters"]["asr_options"]["enable_itn"], false);
        assert!(body["parameters"]["asr_options"].get("language").is_none());
    }

    /// Verifies image-01-live rejects image-01-only dimensions.
    /// 验证 image-01-live 会拒绝仅 image-01 支持的尺寸。
    #[test]
    fn live_model_rejects_dimensions() {
        let arguments = MiniMaxImageToolArguments::from_json(serde_json::json!({
            "prompt": "test",
            "width": 1024,
            "height": 1024
        }))
        .expect("typed arguments");
        assert!(validate_image_arguments("image-01-live", &arguments).is_err());
    }

    /// Verifies MiniMax search responses normalize into the shared search union.
    /// 验证 MiniMax 搜索响应会归一化到共享搜索联合。
    #[test]
    fn decodes_minimax_search_response() {
        let result = decode_minimax_search_response(
            "Vulcan",
            1,
            r#"{"organic":[{"title":"First","link":"https://example.com/first","snippet":"one"},{"title":"Second","link":"https://example.org/second","snippet":"two"}],"base_resp":{"status_code":0,"status_msg":"success"}}"#,
        )
        .expect("valid search response");
        let ProviderToolNormalizedOutput::Search(search) = result.output else {
            panic!("expected search output");
        };
        assert_eq!(search.query, "Vulcan");
        assert_eq!(search.results.len(), 1);
        assert_eq!(search.results[0].source_domain, "example.com");
    }

    /// Verifies MiniMax image-understanding accepts only the copied Data URL contract.
    /// 验证 MiniMax 图片理解仅接受复制的 Data URL 契约。
    #[test]
    fn validates_minimax_vision_data_url() {
        let arguments = MiniMaxVisionToolArguments {
            prompt: String::from("Describe this image."),
            image_url: String::from("data:image/webp;base64,aGVsbG8="),
        };
        arguments.validate().expect("supported WebP Data URL");
        let invalid = MiniMaxVisionToolArguments {
            prompt: String::from("Describe this image."),
            image_url: String::from("https://example.com/image.webp"),
        };
        assert!(invalid.validate().is_err());
    }

    /// Verifies MiniMax speech arguments project exact provider defaults and fields.
    /// 验证 MiniMax 语音参数会投影精确的供应商默认值与字段。
    #[test]
    fn projects_minimax_speech_request() {
        let arguments: MiniMaxSpeechToolArguments = serde_json::from_value(serde_json::json!({
            "text": "Hello",
            "voice_id": "English_Graceful_Lady",
            "timestamps": true,
            "pronunciations": ["Vulcan/ˈvʌlkən"]
        }))
        .expect("typed speech arguments");
        let (body, mime_type) = arguments
            .into_wire_body("speech-2.8-hd")
            .expect("valid speech request");
        assert_eq!(mime_type, "audio/mpeg");
        assert_eq!(body["model"], "speech-2.8-hd");
        assert_eq!(body["stream"], false);
        assert_eq!(body["output_format"], "hex");
        assert_eq!(body["voice_setting"]["speed"], 1.0);
        assert_eq!(body["audio_setting"]["sample_rate"], 32_000);
        assert_eq!(body["audio_setting"]["bitrate"], 128_000);
        assert_eq!(body["subtitle_enable"], true);
        assert_eq!(body["pronunciation_dict"]["tone"][0], "Vulcan/ˈvʌlkən");
    }

    /// Verifies non-MP3 MiniMax speech requests reject the MP3-only bitrate field.
    /// 验证非 MP3 MiniMax 语音请求会拒绝仅适用于 MP3 的码率字段。
    #[test]
    fn rejects_minimax_speech_bitrate_for_non_mp3() {
        let arguments: MiniMaxSpeechToolArguments = serde_json::from_value(serde_json::json!({
            "text": "Hello",
            "voice_id": "English_Graceful_Lady",
            "output_format": "flac",
            "bitrate": 128000
        }))
        .expect("typed speech arguments");
        assert!(arguments.into_wire_body("speech-2.8-hd").is_err());
    }

    /// Verifies MiniMax speech responses normalize hexadecimal audio and subtitle resources.
    /// 验证 MiniMax 语音响应会归一化十六进制音频与字幕资源。
    #[test]
    fn decodes_minimax_speech_response() {
        let result = decode_minimax_speech_response(
            r#"{"data":{"audio":"68656c6c6f","status":2,"subtitle_file":"https://example.com/subtitle.json"},"base_resp":{"status_code":0,"status_msg":"success"}}"#,
            "audio/mpeg",
        )
        .expect("valid speech response");
        let ProviderToolNormalizedOutput::Resources(resources) = result.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].bytes.as_deref(), Some("hello".as_bytes()));
        assert_eq!(resources[0].mime_type, "audio/mpeg");
        assert_eq!(resources[1].kind, "file");
        assert_eq!(
            resources[1].download_url.as_deref(),
            Some("https://example.com/subtitle.json")
        );
    }

    /// Verifies MiniMax music intent projects an exact synchronous URL request.
    /// 验证 MiniMax 音乐意图会投影精确的同步 URL 请求。
    #[test]
    fn projects_minimax_music_request() {
        let arguments: MiniMaxMusicToolArguments = serde_json::from_value(serde_json::json!({
            "prompt": "A cinematic instrumental theme",
            "instrumental": true,
            "output_format": "wav",
            "channels": 2
        }))
        .expect("typed music arguments");
        let (body, mime_type) = arguments
            .into_wire_body("music-3.0")
            .expect("valid music request");
        assert_eq!(mime_type, "audio/wav");
        assert_eq!(body["model"], "music-3.0");
        assert_eq!(body["stream"], false);
        assert_eq!(body["output_format"], "url");
        assert_eq!(body["is_instrumental"], true);
        assert_eq!(body["audio_setting"]["sample_rate"], 44_100);
        assert_eq!(body["audio_setting"]["bitrate"], 256_000);
        assert_eq!(body["audio_setting"]["channel"], 2);
    }

    /// Verifies MiniMax music intent does not silently infer missing lyrics behavior.
    /// 验证 MiniMax 音乐意图不会静默推测缺失的歌词行为。
    #[test]
    fn rejects_ambiguous_minimax_music_intent() {
        let arguments: MiniMaxMusicToolArguments = serde_json::from_value(serde_json::json!({
            "prompt": "A vocal pop song"
        }))
        .expect("typed music arguments");
        assert!(arguments.into_wire_body("music-3.0").is_err());
    }

    /// Verifies MiniMax music responses normalize one validated audio resource.
    /// 验证 MiniMax 音乐响应会归一化一个已验证音频资源。
    #[test]
    fn decodes_minimax_music_response() {
        let result = decode_minimax_music_response(
            r#"{"data":{"audio_url":"https://example.com/music.mp3","status":2},"trace_id":"trace-1","base_resp":{"status_code":0,"status_msg":"success"}}"#,
            "audio/mpeg",
        )
        .expect("valid music response");
        assert_eq!(result.upstream_response_id.as_deref(), Some("trace-1"));
        let ProviderToolNormalizedOutput::Resources(resources) = result.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].kind, "audio");
        assert_eq!(
            resources[0].download_url.as_deref(),
            Some("https://example.com/music.mp3")
        );
    }

    /// Verifies MiniMax Fast video requests require and project one first frame.
    /// 验证 MiniMax Fast 视频请求要求并投影一个首帧。
    #[test]
    fn projects_minimax_fast_video_request() {
        let arguments: MiniMaxVideoToolArguments = serde_json::from_value(serde_json::json!({
            "prompt": "Camera pushes toward the subject.",
            "first_frame_image": "data:image/png;base64,aGVsbG8=",
            "duration_seconds": 6,
            "resolution": "1080P",
            "prompt_extend": true
        }))
        .expect("typed video arguments");
        let body = arguments
            .into_wire_body("MiniMax-Hailuo-2.3-Fast")
            .expect("valid video request");
        assert_eq!(body["model"], "MiniMax-Hailuo-2.3-Fast");
        assert_eq!(body["first_frame_image"], "data:image/png;base64,aGVsbG8=");
        assert_eq!(body["duration"], 6);
        assert_eq!(body["resolution"], "1080P");
        assert_eq!(body["prompt_optimizer"], true);
    }

    /// Verifies MiniMax video model-specific input modes do not cross over.
    /// 验证 MiniMax 视频模型专属输入模式不会相互串用。
    #[test]
    fn rejects_cross_model_minimax_video_input() {
        let arguments: MiniMaxVideoToolArguments = serde_json::from_value(serde_json::json!({
            "prompt": "A character turns around.",
            "subject_reference_image": "data:image/jpeg;base64,aGVsbG8="
        }))
        .expect("typed video arguments");
        assert!(arguments.into_wire_body("MiniMax-Hailuo-2.3").is_err());
    }

    /// Verifies MiniMax video file responses normalize one MP4 resource.
    /// 验证 MiniMax 视频文件响应会归一化一个 MP4 资源。
    #[test]
    fn decodes_minimax_video_file_response() {
        let result = decode_minimax_video_file_response(
            r#"{"file":{"download_url":"https://example.com/video.mp4"},"base_resp":{"status_code":0,"status_msg":"success"}}"#,
            "task-1",
        )
        .expect("valid video file response");
        assert_eq!(result.upstream_response_id.as_deref(), Some("task-1"));
        let ProviderToolNormalizedOutput::Resources(resources) = result.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources[0].mime_type, "video/mp4");
        assert_eq!(
            resources[0].download_url.as_deref(),
            Some("https://example.com/video.mp4")
        );
    }

    /// Verifies Tavily search output preserves request identity and ordered evidence.
    /// 验证 Tavily 搜索输出保留请求标识与有序证据。
    #[test]
    fn decodes_tavily_search_response() {
        let result = decode_tavily_search_response(
            r#"{"query":"Vulcan","results":[{"title":"First","url":"https://example.com/first","content":"one","score":0.9}],"usage":{"credits":1},"request_id":"request-1"}"#,
        )
        .expect("valid Tavily search response");
        assert_eq!(result.upstream_response_id.as_deref(), Some("request-1"));
        let ProviderToolNormalizedOutput::Search(search) = result.output else {
            panic!("expected search output");
        };
        assert_eq!(search.query, "Vulcan");
        assert_eq!(search.results[0].source_domain, "example.com");
    }

    /// Verifies Tavily extraction output preserves successful pages and per-URL failures.
    /// 验证 Tavily 网页提取输出保留成功页面与逐 URL 失败。
    #[test]
    fn decodes_tavily_extract_response() {
        let result = decode_tavily_extract_response(
            r##"{"results":[{"url":"https://example.com/page","raw_content":"# Page","images":["https://example.com/image.png"],"favicon":"https://example.com/favicon.ico"}],"failed_results":[{"url":"https://example.org/missing","error":"not found"}],"usage":{"credits":1},"request_id":"request-2"}"##,
        )
        .expect("valid Tavily extract response");
        assert_eq!(result.upstream_response_id.as_deref(), Some("request-2"));
        let ProviderToolNormalizedOutput::Extract(extract) = result.output else {
            panic!("expected extract output");
        };
        assert_eq!(extract.results[0].content, "# Page");
        assert_eq!(extract.failures[0].error, "not found");
    }

    /// Verifies Alibaba MCP text pages normalize into the shared search union.
    /// 验证阿里云 MCP 文本页面会归一化到共享搜索联合。
    #[test]
    fn decodes_alibaba_mcp_search_result() {
        let result = decode_alibaba_search_result(
            "Vulcan",
            Some(1),
            serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": "{\"pages\":[{\"title\":\"First\",\"url\":\"https://example.com/first\",\"snippet\":\"one\"},{\"title\":\"Second\",\"url\":\"https://example.org/second\",\"snippet\":\"two\"}]}"
                }]
            }),
        )
        .expect("valid Alibaba search result");
        let ProviderToolNormalizedOutput::Search(search) = result.output else {
            panic!("expected search output");
        };
        assert_eq!(search.query, "Vulcan");
        assert_eq!(search.results.len(), 1);
        assert_eq!(search.results[0].source_domain, "example.com");
    }

    /// Verifies Alibaba MCP JSON-RPC errors remain controlled and correlated.
    /// 验证阿里云 MCP JSON-RPC 错误保持受控且完成关联。
    #[test]
    fn rejects_alibaba_mcp_error_response() {
        let error = decode_alibaba_mcp_response(
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"secret detail"}}"#,
            2,
        )
        .expect_err("provider error must be rejected");
        assert!(error.contains("-32000"));
        assert!(!error.contains("secret detail"));
    }
}
