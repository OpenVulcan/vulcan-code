//! Deterministic provider execution routing and SSE transport.
//! 确定性的供应商执行路由与 SSE 传输。

use crate::{
    AuthenticationProjection, CredentialConnection, CredentialExecutionSecret,
    MINIMAX_REASONING_SPLIT_PARAMETER_ID, OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID,
    ProtocolRoute, ProviderDescriptor, SAMPLING_TEMPERATURE_PARAMETER_ID, SecondaryRouteSelection,
    ServiceEntry,
};
use async_stream::try_stream;
use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::Arc;
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiExecutionRequest, AiStreamEvent, DispatchOutcome,
    InputTokenCount, ModelParameterValue, PromptCachePolicy, ReasoningEffort,
};
use vulcan_ai_protocols::{
    EncodedWireRequest, ProtocolConversionError, ProtocolRegistry,
    decode_gemini_count_tokens_response, encode_gemini_count_tokens_request,
};

/// MiniMax documented maximum JSON request-body size.
/// MiniMax 文档化的 JSON 请求体最大字节数。
const MINIMAX_REQUEST_BODY_BYTE_LIMIT: usize = 64_000_000;

/// Immutable route selected before an execution starts.
/// 在执行开始前选择的不可变路由。
#[derive(Debug, Clone)]
pub struct ProviderExecutionRoute {
    /// Provider descriptor snapshot.
    /// 供应商描述快照。
    pub provider: ProviderDescriptor,
    /// Service entry snapshot.
    /// 服务入口快照。
    pub entry: ServiceEntry,
    /// Exact credential connection snapshot without secret material.
    /// 不含秘钥材料的精确凭据连接快照。
    pub connection: CredentialConnection,
    /// Exact verified protocol route.
    /// 精确的已验证协议路由。
    pub protocol_route: ProtocolRoute,
    /// Optional provider-declared secondary route.
    /// 可选供应商声明二级路由。
    pub secondary_route: SecondaryRouteSelection,
}

impl ProviderExecutionRoute {
    /// Resolves an immutable execution route from registered snapshots.
    /// 从已注册快照解析不可变执行路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider`, `entry`, and `connection` are exact configuration snapshots;
    /// `secondary_route` is the user-selected provider-internal route.
    /// `provider`、`entry` 与 `connection` 是精确配置快照；`secondary_route` 是用户选择的供应商内部路由。
    ///
    /// # Returns
    /// 返回值
    /// A validated route or a controlled pre-dispatch failure.
    /// 返回已校验路由或受控的分派前失败。
    pub fn resolve(
        provider: ProviderDescriptor,
        entry: ServiceEntry,
        connection: CredentialConnection,
        secondary_route: SecondaryRouteSelection,
    ) -> Result<Self, ProviderExecutionFailure> {
        if provider.id != connection.provider_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not belong to provider `{}`.",
                connection.id, provider.id
            )));
        }
        if entry.id != connection.service_entry_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not use service entry `{}`.",
                connection.id, entry.id
            )));
        }
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        if credential_profile.credential_mode != connection.credential_mode {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` credential mode does not match profile `{}`.",
                connection.id, credential_profile.id
            )));
        }
        let protocol_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == connection.selected_protocol)
            .cloned()
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Protocol `{}` is not registered for service entry `{}`.",
                    connection.selected_protocol, entry.id
                ))
            })?;
        if !protocol_route.verified
            || !connection
                .verified_protocols
                .contains(&connection.selected_protocol)
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol `{}` is not verified for connection `{}`.",
                connection.selected_protocol, connection.id
            )));
        }
        if !matches!(secondary_route, SecondaryRouteSelection::Automatic)
            && provider.id != "openrouter"
        {
            return Err(pre_dispatch_failure(format!(
                "Provider `{}` does not register secondary routing.",
                provider.id
            )));
        }
        Ok(Self {
            provider,
            entry,
            connection,
            protocol_route,
            secondary_route,
        })
    }

    /// Returns the exact request URL for this immutable route.
    /// 返回该不可变路由的精确请求 URL。
    ///
    /// # Returns
    /// 返回值
    /// The service base URL joined with the registered endpoint path.
    /// 返回服务基础 URL 与已注册端点路径的拼接结果。
    pub fn request_url(&self, encoded_path: &str) -> String {
        format!(
            "{}{}",
            self.entry.base_url.trim_end_matches('/'),
            ensure_leading_slash(encoded_path)
        )
    }
}

/// Carries one structured provider execution failure.
/// 承载一个结构化供应商执行失败。
#[derive(Debug, Clone)]
pub struct ProviderExecutionFailure {
    /// Normalized public error.
    /// 归一化公共错误。
    pub error: Box<AiExecutionError>,
}

impl std::fmt::Display for ProviderExecutionFailure {
    /// Formats the controlled execution message.
    /// 格式化受控执行消息。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.error.message.as_str())
    }
}

impl std::error::Error for ProviderExecutionFailure {}

/// Stream of normalized public execution events.
/// 归一化公共执行事件流。
pub type ProviderExecutionStream =
    Pin<Box<dyn Stream<Item = Result<AiStreamEvent, ProviderExecutionFailure>> + Send>>;

/// Executes immutable provider routes through public protocol profiles.
/// 通过公共协议 Profile 执行不可变供应商路由。
#[derive(Clone)]
pub struct ProviderExecutionClient {
    /// Shared configured HTTP client.
    /// 共享的已配置 HTTP 客户端。
    http: reqwest::Client,
    /// Strict protocol profile registry.
    /// 严格协议 Profile 注册表。
    protocols: Arc<ProtocolRegistry>,
}

impl ProviderExecutionClient {
    /// Creates one execution client.
    /// 创建一个执行客户端。
    ///
    /// # Parameters
    /// 参数
    /// `http` is the shared HTTP client and `protocols` is the strict profile registry.
    /// `http` 是共享 HTTP 客户端，`protocols` 是严格 Profile 注册表。
    ///
    /// # Returns
    /// 返回值
    /// A reusable execution client.
    /// 返回可复用执行客户端。
    pub fn new(http: reqwest::Client, protocols: Arc<ProtocolRegistry>) -> Self {
        Self { http, protocols }
    }

    /// Encodes one immutable execution request without dispatching it.
    /// 编码一个不可变执行请求，但不进行网络分派。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the provider-neutral request.
    /// `route` 是冻结的供应商路由，`request` 是供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// The exact projected wire request or a controlled validation failure.
    /// 返回精确投影的线级请求或受控校验失败。
    pub fn encode_request(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let profile = self
            .protocols
            .require(route.protocol_route.protocol_profile_id.as_str())
            .map_err(protocol_failure)?;
        validate_request_policy(route, request)?;
        let encoded = profile.encode(request).map_err(protocol_failure)?;
        project_provider_request(route, request, encoded)
    }

    /// Encodes one immutable request with credential-bound non-secret identity projection.
    /// 编码一个不可变请求，并投影凭据绑定的非秘密身份信息。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` form the public route projection; `secret` supplies only the
    /// account metadata required in the wire body.
    /// `route` 与 `request` 构成公共路由投影；`secret` 仅提供线级请求体所需的账号元数据。
    ///
    /// # Returns
    /// 返回值
    /// The exact request body used by dispatch and wire preview, without authentication headers.
    /// 返回分派与线级预览共同使用的精确请求体，不包含认证请求头。
    pub fn encode_request_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let encoded = self.encode_request(route, request)?;
        project_credential_request(route, secret, encoded)
    }

    /// Counts one complete Gemini API request with the provider-authoritative tokenizer.
    /// 使用供应商权威 Tokenizer 统计一个完整 Gemini API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `route` must be the verified public Gemini GenerateContent route, `request` is the
    /// immutable provider-neutral request, and `secret` is its exact selected credential.
    /// `route` 必须是已验证的公共 Gemini GenerateContent 路由，`request` 是不可变的
    /// 供应商无关请求，`secret` 是其精确选定凭据。
    ///
    /// # Returns
    /// 返回值
    /// The exact provider count and modality details, or a controlled pre-dispatch,
    /// transport, HTTP, or protocol failure.
    /// 返回精确供应商统计与模态明细，或受控的分派前、传输、HTTP 或协议错误。
    pub async fn count_input_tokens(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<InputTokenCount, ProviderExecutionFailure> {
        if route.provider.id != "google"
            || !matches!(route.entry.id.as_str(), "gemini-api" | "gemini-oauth")
            || route.protocol_route.protocol_profile_id
                != vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        {
            return Err(pre_dispatch_failure(
                "Provider-authoritative CountTokens is only registered for the public Gemini GenerateContent route.",
            ));
        }
        validate_request_policy(route, request)?;
        let encoded = encode_gemini_count_tokens_request(request).map_err(protocol_failure)?;
        let url = route.request_url(encoded.path.as_str());
        let headers = execution_headers(route, &encoded.headers, secret)?;
        let response = self
            .http
            .post(url.as_str())
            .headers(headers)
            .json(&encoded.body)
            .send()
            .await
            .map_err(|error| {
                transport_failure(format!(
                    "Provider CountTokens request to `{url}` failed: {error}"
                ))
            })?;
        let status = response.status();
        if !status.is_success() {
            let retry_after_seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok());
            let body = response.text().await.map_err(|error| {
                transport_failure(format!(
                    "Provider CountTokens error response from `{url}` could not be read: {error}"
                ))
            })?;
            return Err(http_failure(
                status.as_u16(),
                retry_after_seconds,
                body.as_str(),
            ));
        }
        let body = response.json::<Value>().await.map_err(|error| {
            protocol_failure(ProtocolConversionError::new(format!(
                "Gemini CountTokens response JSON is invalid: {error}"
            )))
        })?;
        decode_gemini_count_tokens_response(&body).map_err(protocol_failure)
    }

    /// Starts one immutable execution without protocol or account fallback.
    /// 启动一次不可变执行，不进行协议或账号回退。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the resolved immutable route, `request` is the public request,
    /// `secret` is the exact host-loaded credential, and `execution_id` is stable.
    /// `route` 是解析后的不可变路由，`request` 是公共请求，`secret` 是宿主加载的精确凭据，
    /// `execution_id` 是稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream.
    /// 返回归一化事件流。
    pub fn execute(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let encoded = client.encode_request_with_credential(&route, &request, &secret)?;
            let url = route.request_url(encoded.path.as_str());
            let headers = execution_headers(&route, &encoded.headers, &secret)?;
            let response = client
                .http
                .post(url.as_str())
                .headers(headers)
                .json(&encoded.body)
                .send()
                .await
                .map_err(|error| transport_failure(format!(
                    "Provider request to `{url}` failed: {error}"
                )))?;
            let status = response.status();
            let response = if status.is_success() {
                response
            } else {
                let retry_after_seconds = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok());
                let body = response
                    .text()
                    .await
                    .map_err(|error| transport_failure(format!(
                        "Provider error response from `{url}` could not be read: {error}"
                    )))?;
                Err(http_failure(
                    status.as_u16(),
                    retry_after_seconds,
                    body.as_str(),
                ))?
            };
            let profile = client
                .protocols
                .require(route.protocol_route.protocol_profile_id.as_str())
                .map_err(protocol_failure)?;
            let mut decoder = profile.create_stream_decoder(execution_id, &request);
            let mut parser = SseFrameParser::default();
            let mut bytes = response.bytes_stream();
            let mut semantic_output_observed = false;
            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|error| transport_failure(format!(
                    "Provider stream from `{url}` failed: {error}"
                )))?;
                for frame in parser.push(chunk.as_ref())? {
                    for mut event in decoder
                        .push_sse(frame.event_name.as_deref(), frame.data.as_str())
                        .map_err(protocol_failure)?
                    {
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut event,
                        );
                        yield event;
                    }
                }
            }
            for frame in parser.finish()? {
                for mut event in decoder
                    .push_sse(frame.event_name.as_deref(), frame.data.as_str())
                    .map_err(protocol_failure)?
                {
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    yield event;
                }
            }
            for mut event in decoder.finish().map_err(protocol_failure)? {
                update_semantic_output_observation(
                    &mut semantic_output_observed,
                    &mut event,
                );
                yield event;
            }
        })
    }
}

/// Tracks semantic output and annotates terminal public failures with the observed state.
/// 跟踪语义输出并用已观测状态标注公共终态失败。
fn update_semantic_output_observation(observed: &mut bool, event: &mut AiStreamEvent) {
    match event {
        AiStreamEvent::ResponseTextDelta { .. }
        | AiStreamEvent::ReasoningDelta { .. }
        | AiStreamEvent::ToolCallStarted { .. }
        | AiStreamEvent::ResourceProgress { .. } => {
            *observed = true;
        }
        AiStreamEvent::ResponseFailed { error } => {
            error.semantic_output_observed = *observed;
        }
        _ => {}
    }
}

/// Applies route-scoped request policy before encoding.
/// 在编码前应用路由作用域请求策略。
fn validate_request_policy(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<(), ProviderExecutionFailure> {
    for parameter_id in request.model_parameters.keys() {
        let registered_temperature_route = parameter_id == SAMPLING_TEMPERATURE_PARAMETER_ID
            && ((route.provider.id == "minimax"
                && matches!(
                    route.protocol_route.protocol_profile_id.as_str(),
                    vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
                        | vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID
                ))
                || route.provider.id == "openrouter");
        let registered_minimax_reasoning_split = parameter_id
            == MINIMAX_REASONING_SPLIT_PARAMETER_ID
            && route.provider.id == "minimax"
            && route.protocol_route.protocol_profile_id
                == vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID;
        let registered_openrouter_reasoning_budget = parameter_id
            == OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID
            && route.provider.id == "openrouter";
        if !registered_temperature_route
            && !registered_minimax_reasoning_split
            && !registered_openrouter_reasoning_budget
        {
            return Err(pre_dispatch_failure(format!(
                "Model-specific parameter `{parameter_id}` has no registered route projector."
            )));
        }
    }
    if !matches!(request.prompt_cache, PromptCachePolicy::ProviderDefault)
        && route.provider.id == "openrouter"
    {
        return Err(pre_dispatch_failure(
            "OpenRouter explicit prompt caching requires a verified model-endpoint capability.",
        ));
    }
    if request.reasoning_effort.is_some()
        && request
            .model_parameters
            .contains_key(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID)
    {
        return Err(pre_dispatch_failure(
            "OpenRouter reasoning effort and reasoning max_tokens are mutually exclusive.",
        ));
    }
    Ok(())
}

/// Applies provider-only fields after protocol encoding.
/// 在协议编码后应用仅供应商字段。
fn project_provider_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if let Some(endpoint_path) = route.protocol_route.endpoint_path.as_ref() {
        encoded.path = endpoint_path.clone();
    }
    if let Some(value) = request
        .model_parameters
        .get(SAMPLING_TEMPERATURE_PARAMETER_ID)
    {
        let ModelParameterValue::Decimal(decimal) = value else {
            return Err(pre_dispatch_failure(
                "The registered sampling temperature value must be a decimal string.",
            ));
        };
        let temperature = serde_json::from_str::<Value>(decimal).map_err(|error| {
            pre_dispatch_failure(format!(
                "Sampling temperature could not be projected as JSON: {error}"
            ))
        })?;
        let body = encoded
            .body
            .as_object_mut()
            .ok_or_else(|| pre_dispatch_failure("Protocol request body must be a JSON object."))?;
        if body.contains_key("temperature") {
            return Err(pre_dispatch_failure(
                "Sampling temperature projection conflicts with an existing protocol field.",
            ));
        }
        body.insert(String::from("temperature"), temperature);
    }
    if let Some(value) = request
        .model_parameters
        .get(MINIMAX_REASONING_SPLIT_PARAMETER_ID)
    {
        let ModelParameterValue::Boolean(reasoning_split) = value else {
            return Err(pre_dispatch_failure(
                "The registered MiniMax reasoning split value must be boolean.",
            ));
        };
        let body = encoded
            .body
            .as_object_mut()
            .ok_or_else(|| pre_dispatch_failure("Protocol request body must be a JSON object."))?;
        body.insert(
            String::from("reasoning_split"),
            Value::Bool(*reasoning_split),
        );
    }
    if let Some(value) = request
        .model_parameters
        .get(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID)
    {
        let ModelParameterValue::Integer(max_tokens) = value else {
            return Err(pre_dispatch_failure(
                "The registered OpenRouter reasoning token budget must be an integer.",
            ));
        };
        let body = encoded
            .body
            .as_object_mut()
            .ok_or_else(|| pre_dispatch_failure("Protocol request body must be a JSON object."))?;
        let reasoning = body
            .entry(String::from("reasoning"))
            .or_insert_with(|| json!({}));
        let reasoning = reasoning.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure("OpenRouter reasoning projection must be a JSON object.")
        })?;
        reasoning.insert(String::from("max_tokens"), json!(max_tokens));
    }
    if route.provider.id == "openrouter"
        && let Some(effort) = request.reasoning_effort
    {
        let body = encoded.body.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure("OpenRouter request body must be a JSON object.")
        })?;
        match route.protocol_route.protocol_profile_id.as_str() {
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => {
                body.remove("reasoning_effort");
            }
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
                body.remove("output_config");
            }
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID => {}
            protocol_id => {
                return Err(pre_dispatch_failure(format!(
                    "OpenRouter reasoning projection is not registered for protocol `{protocol_id}`."
                )));
            }
        }
        let reasoning = body
            .entry(String::from("reasoning"))
            .or_insert_with(|| json!({}));
        let reasoning = reasoning.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure("OpenRouter reasoning projection must be a JSON object.")
        })?;
        reasoning.insert(String::from("effort"), json!(effort.as_str()));
    }
    if route.provider.id == "openrouter"
        && let SecondaryRouteSelection::PinnedEndpoint { route_id, .. } = &route.secondary_route
    {
        let body = encoded.body.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure("OpenRouter request body must be a JSON object.")
        })?;
        if body.contains_key("provider") {
            return Err(pre_dispatch_failure(
                "OpenRouter secondary-route projection conflicts with an existing provider field.",
            ));
        }
        body.insert(
            String::from("provider"),
            json!({
                "order": [route_id],
                "allow_fallbacks": false,
            }),
        );
    }
    if route.provider.id == "minimax"
        && route.protocol_route.protocol_profile_id
            == vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        && let Some(effort) = request.reasoning_effort
    {
        let thinking_type = match effort {
            ReasoningEffort::None => "disabled",
            ReasoningEffort::Adaptive => "adaptive",
            unsupported => {
                return Err(pre_dispatch_failure(format!(
                    "MiniMax Anthropic-compatible thinking does not expose reasoning effort `{}`.",
                    unsupported.as_str()
                )));
            }
        };
        let body = encoded
            .body
            .as_object_mut()
            .ok_or_else(|| pre_dispatch_failure("MiniMax request body must be a JSON object."))?;
        body.remove("output_config");
        body.insert(String::from("thinking"), json!({ "type": thinking_type }));
    }
    if route.provider.id == "minimax" {
        let body_size = serde_json::to_vec(&encoded.body)
            .map_err(|error| {
                pre_dispatch_failure(format!(
                    "MiniMax request body could not be measured before dispatch: {error}"
                ))
            })?
            .len();
        if body_size > MINIMAX_REQUEST_BODY_BYTE_LIMIT {
            return Err(pre_dispatch_failure(format!(
                "MiniMax request body is {body_size} bytes and exceeds the documented {MINIMAX_REQUEST_BODY_BYTE_LIMIT}-byte limit."
            )));
        }
    }
    Ok(encoded)
}

/// Applies credential-bound non-secret identity fields immediately before dispatch.
/// 在分派前立即应用凭据绑定的非秘密身份字段。
///
/// # Parameters
/// 参数
/// `route` fixes the exact protocol, `secret` carries validated account metadata, and
/// `encoded` is the provider-projected request.
/// `route` 固定精确协议，`secret` 承载已验证账号元数据，`encoded` 是供应商投影请求。
///
/// # Returns
/// 返回值
/// A request containing required credential identity without exposing access material in JSON.
/// 返回包含所需凭据身份且不在 JSON 中暴露访问材料的请求。
fn project_credential_request(
    route: &ProviderExecutionRoute,
    secret: &CredentialExecutionSecret,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if route.protocol_route.protocol_profile_id
        != vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID
    {
        return Ok(encoded);
    }
    let project_id = secret.project_id().ok_or_else(|| {
        pre_dispatch_failure(
            "Google Code Assist execution requires a validated credential project id.",
        )
    })?;
    let body = encoded.body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Google Code Assist request body must be a JSON object.")
    })?;
    if body.contains_key("project") {
        return Err(pre_dispatch_failure(
            "Google Code Assist project projection conflicts with a protocol field.",
        ));
    }
    body.insert(
        String::from("project"),
        Value::String(project_id.to_string()),
    );
    Ok(encoded)
}

/// Builds provider authentication and public application headers.
/// 构建供应商鉴权与公共应用请求头。
///
/// # Parameters
/// 参数
/// `route` identifies the exact provider, `protocol_headers` contains non-secret Profile
/// requirements, and `secret` is the selected credential material.
/// `route` 标识精确供应商，`protocol_headers` 包含非秘密 Profile 要求，
/// `secret` 是选中的凭据材料。
///
/// # Returns
/// 返回值
/// A validated merged header map or a controlled pre-dispatch failure.
/// 返回已校验的合并请求头，或受控的分派前失败。
fn execution_headers(
    route: &ProviderExecutionRoute,
    protocol_headers: &std::collections::BTreeMap<String, String>,
    secret: &CredentialExecutionSecret,
) -> Result<HeaderMap, ProviderExecutionFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Streaming requests require byte-transparent SSE framing; compressed error responses
    // are still decoded by the configured reqwest content decoders if an upstream ignores this.
    // 流式请求要求 SSE 分帧保持字节透明；若上游忽略该设置，已配置的 reqwest 内容
    // 解码器仍会解码压缩错误响应。
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );
    for (name, value) in protocol_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol profile must not provide credential header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.entry.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
        {
            return Err(pre_dispatch_failure(format!(
                "Service entry must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    let credential_profile = route
        .entry
        .credential_profile(route.connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                route.connection.credential_profile_id, route.entry.id
            ))
        })?;
    match route.protocol_route.resolved_authentication(
        route.entry.authentication,
        credential_profile.authentication,
    ) {
        AuthenticationProjection::XApiKey => {
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("API key contains invalid header characters.")
                })?,
            );
        }
        AuthenticationProjection::BearerAuthorization => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Bearer credential contains invalid header characters.",
                        )
                    })?,
            );
        }
        AuthenticationProjection::OpenAiCodexBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Codex bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let account_id = secret.account_id().ok_or_else(|| {
                pre_dispatch_failure("Codex credential does not contain an account identity.")
            })?;
            headers.insert(
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_str(account_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Codex account identity contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleCodeAssistBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            if let Some(quota_project_id) = secret.quota_project_id() {
                headers.insert(
                    HeaderName::from_static("x-goog-user-project"),
                    HeaderValue::from_str(quota_project_id).map_err(|_| {
                        pre_dispatch_failure(
                            "Google quota project contains invalid header characters.",
                        )
                    })?,
                );
            }
        }
        AuthenticationProjection::GoogleGeminiBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google Gemini bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let quota_project_id = secret.quota_project_id().ok_or_else(|| {
                pre_dispatch_failure(
                    "Public Gemini OAuth credential does not contain a Google Cloud project id.",
                )
            })?;
            headers.insert(
                HeaderName::from_static("x-goog-user-project"),
                HeaderValue::from_str(quota_project_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Google Gemini quota project contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleApiKey => {
            headers.insert(
                HeaderName::from_static("x-goog-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("Google API key contains invalid header characters.")
                })?,
            );
        }
    }
    Ok(headers)
}

/// One parsed SSE event and data record.
/// 一条解析后的 SSE event 与 data 记录。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SseFrame {
    /// Optional event name.
    /// 可选事件名称。
    event_name: Option<String>,
    /// Joined data lines.
    /// 合并后的数据行。
    data: String,
}

/// Incrementally frames arbitrary byte chunks as complete SSE records.
/// 将任意字节分片增量组装为完整 SSE 记录。
#[derive(Debug, Default)]
struct SseFrameParser {
    /// Unprocessed bytes retained across transport chunks.
    /// 在传输分片间保留的未处理字节。
    buffer: Vec<u8>,
}

impl SseFrameParser {
    /// Pushes arbitrary transport bytes.
    /// 推入任意传输字节。
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, ProviderExecutionFailure> {
        self.buffer.extend_from_slice(chunk);
        self.drain_frames(false)
    }

    /// Finishes framing and includes a final non-empty record.
    /// 完成分帧并包含最后一条非空记录。
    fn finish(&mut self) -> Result<Vec<SseFrame>, ProviderExecutionFailure> {
        self.drain_frames(true)
    }

    /// Drains complete records from the internal byte buffer.
    /// 从内部字节缓冲区提取完整记录。
    fn drain_frames(
        &mut self,
        include_tail: bool,
    ) -> Result<Vec<SseFrame>, ProviderExecutionFailure> {
        let mut frames = Vec::new();
        while let Some((record_end, delimiter_len)) = find_sse_record_end(&self.buffer) {
            let record = self.buffer[..record_end].to_vec();
            self.buffer.drain(..record_end + delimiter_len);
            if let Some(frame) = parse_sse_record(&record)? {
                frames.push(frame);
            }
        }
        if include_tail && !self.buffer.is_empty() {
            let record = std::mem::take(&mut self.buffer);
            if let Some(frame) = parse_sse_record(&record)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }
}

/// Finds the first SSE blank-line delimiter.
/// 查找第一个 SSE 空行分隔符。
fn find_sse_record_end(buffer: &[u8]) -> Option<(usize, usize)> {
    for index in 0..buffer.len() {
        if buffer.get(index..index + 2) == Some(b"\n\n") {
            return Some((index, 2));
        }
        if buffer.get(index..index + 4) == Some(b"\r\n\r\n") {
            return Some((index, 4));
        }
    }
    None
}

/// Parses one complete SSE record.
/// 解析一条完整 SSE 记录。
fn parse_sse_record(record: &[u8]) -> Result<Option<SseFrame>, ProviderExecutionFailure> {
    let text = std::str::from_utf8(record).map_err(|error| {
        protocol_failure(ProtocolConversionError::new(format!(
            "Provider SSE record is not valid UTF-8: {error}"
        )))
    })?;
    let mut event_name = None;
    let mut data_lines = Vec::new();
    for line in text.lines() {
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(SseFrame {
        event_name,
        data: data_lines.join("\n"),
    }))
}

/// Ensures one endpoint path begins with a slash.
/// 确保一个端点路径以斜杠开头。
fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Creates a structured pre-dispatch failure.
/// 创建结构化分派前失败。
fn pre_dispatch_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("route_validation")),
            stage: String::from("prepare"),
            http_status: None,
            upstream_code: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            terminal: true,
            retryable: false,
            retry_after_seconds: None,
        }),
    }
}

/// Converts a deterministic protocol failure.
/// 转换确定性协议失败。
fn protocol_failure(error: ProtocolConversionError) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("protocol"),
            http_status: None,
            upstream_code: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            terminal: true,
            retryable: false,
            retry_after_seconds: None,
        }),
    }
}

/// Creates a structured transport failure.
/// 创建结构化传输失败。
fn transport_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("transport"),
            http_status: None,
            upstream_code: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            terminal: true,
            retryable: true,
            retry_after_seconds: None,
        }),
    }
}

/// Converts one non-success HTTP response.
/// 转换一个非成功 HTTP 响应。
fn http_failure(
    status: u16,
    retry_after_seconds: Option<u64>,
    body: &str,
) -> ProviderExecutionFailure {
    let parsed = serde_json::from_str::<Value>(body).ok();
    let error = parsed
        .as_ref()
        .map(|value| value.get("error").unwrap_or(value));
    let upstream_code = error.and_then(|value| {
        value
            .get("code")
            .and_then(|code| match code {
                Value::String(code) => Some(code.clone()),
                Value::Number(code) => Some(code.to_string()),
                _ => None,
            })
            .or_else(|| {
                value
                    .get("error_type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                value
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    });
    let error_type = error
        .and_then(|value| value.get("error_type").and_then(Value::as_str))
        .or_else(|| error.and_then(|value| value.get("type").and_then(Value::as_str)));
    let category = match error_type {
        Some("authentication" | "authentication_error") => AiErrorCategory::Authentication,
        Some("permission_denied" | "permission_error") => AiErrorCategory::PermissionDenied,
        Some("payment_required") => AiErrorCategory::PaymentRequired,
        Some("rate_limit_exceeded" | "rate_limit_error") => AiErrorCategory::RateLimited,
        Some("provider_overloaded" | "overloaded_error") => AiErrorCategory::ProviderOverloaded,
        Some("provider_unavailable" | "server" | "api_error") => {
            AiErrorCategory::ProviderUnavailable
        }
        Some("timeout") => AiErrorCategory::Timeout,
        Some("content_policy_violation") => AiErrorCategory::ContentPolicy,
        Some(
            "invalid_request"
            | "invalid_request_error"
            | "context_length_exceeded"
            | "max_tokens_exceeded"
            | "token_limit_exceeded"
            | "string_too_long",
        ) => AiErrorCategory::InvalidRequest,
        _ => match status {
            401 => AiErrorCategory::Authentication,
            403 => AiErrorCategory::PermissionDenied,
            402 => AiErrorCategory::PaymentRequired,
            408 => AiErrorCategory::Timeout,
            429 => AiErrorCategory::RateLimited,
            500..=599 => AiErrorCategory::ProviderUnavailable,
            _ => AiErrorCategory::InvalidRequest,
        },
    };
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category,
            subcategory: error_type
                .map(str::to_string)
                .or_else(|| Some(String::from("http"))),
            stage: String::from("response_headers"),
            http_status: Some(status),
            upstream_code,
            message: error
                .and_then(|value| value.get("message").and_then(Value::as_str))
                .unwrap_or(if body.is_empty() {
                    "Provider returned an empty error response."
                } else {
                    body
                })
                .chars()
                .take(2_000)
                .collect(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            terminal: true,
            retryable: matches!(
                category,
                AiErrorCategory::RateLimited
                    | AiErrorCategory::ProviderOverloaded
                    | AiErrorCategory::ProviderUnavailable
                    | AiErrorCategory::Timeout
            ),
            retry_after_seconds,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CredentialMode,
        providers::{minimax_descriptor, openrouter_descriptor},
    };
    use std::collections::{BTreeMap, BTreeSet};
    use vulcan_ai_contract::{AiMessage, AiOperation, ContentItem, MessageRole};
    use vulcan_ai_protocols::{
        ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    };

    /// Builds one minimal provider-neutral conversation request.
    /// 构建一个最小供应商无关对话请求。
    fn minimal_request() -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("openai/gpt-4.1"),
            cache_scope_id: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("hello"),
                }],
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(512),
        }
    }

    /// Resolves one pinned OpenRouter route for an exact verified protocol.
    /// 为一个精确的已验证协议解析一条固定二级节点的 OpenRouter 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` is one exact protocol profile registered by OpenRouter.
    /// `protocol_id` 是 OpenRouter 注册的一个精确协议 Profile。
    ///
    /// # Returns
    /// 返回值
    /// A frozen route with fallback disabled.
    /// 返回禁用回退的冻结路由。
    fn pinned_openrouter_route(protocol_id: &str) -> ProviderExecutionRoute {
        let provider = openrouter_descriptor();
        let entry = provider
            .service_entries
            .first()
            .expect("OpenRouter public entry must exist")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Primary"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::PinnedEndpoint {
                route_id: String::from("anthropic"),
                provider_display_name: String::from("Anthropic"),
                endpoint_display_name: String::from("Anthropic"),
            },
        )
        .expect("route should resolve")
    }

    /// Resolves one MiniMax route for an exact compatible protocol.
    /// 为一个精确兼容协议解析 MiniMax 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
    /// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
    ///
    /// # Returns
    /// 返回值
    /// A frozen MiniMax international API route.
    /// 返回冻结的 MiniMax 国际 API 路由。
    fn minimax_route(protocol_id: &str) -> ProviderExecutionRoute {
        let provider = minimax_descriptor();
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == "global")
            .expect("MiniMax global API entry must exist")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("payg-api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Primary"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::Automatic,
        )
        .expect("MiniMax route should resolve")
    }

    /// Verifies SSE framing survives arbitrary UTF-8 and record chunk boundaries.
    /// 验证 SSE 分帧可以跨越任意 UTF-8 与记录分片边界。
    #[test]
    fn sse_parser_handles_arbitrary_chunks() {
        let bytes = "event: delta\r\ndata: {\"text\":\"你好\"}\r\n\r\n".as_bytes();
        let split = bytes
            .windows(3)
            .position(|window| window == "你".as_bytes())
            .expect("UTF-8 text must exist")
            + 1;
        let mut parser = SseFrameParser::default();
        assert!(
            parser
                .push(&bytes[..split])
                .expect("first chunk should buffer")
                .is_empty()
        );
        let frames = parser
            .push(&bytes[split..])
            .expect("second chunk should complete");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
        assert!(frames[0].data.contains("你好"));
    }

    /// Verifies capture encoding includes provider-specific secondary routing.
    /// 验证捕获编码包含供应商特有的二级路由字段。
    #[test]
    fn encode_request_projects_pinned_openrouter_route_for_every_protocol() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for (protocol_id, expected_path) in [
            (OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
            (OPENAI_RESPONSES_PROFILE_ID, "/responses"),
            (ANTHROPIC_MESSAGES_PROFILE_ID, "/messages"),
        ] {
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &minimal_request())
                .expect("request should encode");

            assert_eq!(encoded.path, expected_path);
            assert_eq!(encoded.body["model"], "openai/gpt-4.1");
            assert_eq!(encoded.body["provider"]["order"], json!(["anthropic"]));
            assert_eq!(encoded.body["provider"]["allow_fallbacks"], false);
        }
    }

    /// Verifies every OpenRouter protocol route converges on the unified reasoning object.
    /// 验证每条 OpenRouter 协议路由都会收敛到统一 reasoning 对象。
    #[test]
    fn encode_request_normalizes_openrouter_reasoning_for_every_protocol() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Max);
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("reasoning request should encode");
            assert_eq!(encoded.body["reasoning"]["effort"], "max");
            assert!(encoded.body.get("reasoning_effort").is_none());
            assert!(encoded.body.get("output_config").is_none());
        }
    }

    /// Verifies OpenRouter rejects simultaneous effort and token-budget reasoning controls.
    /// 验证 OpenRouter 会拒绝同时设置推理档位和 Token 预算。
    #[test]
    fn encode_request_rejects_conflicting_openrouter_reasoning_controls() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::High);
        request.model_parameters.insert(
            String::from(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID),
            ModelParameterValue::Integer(2048),
        );
        let error = client
            .encode_request(&pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect_err("conflicting reasoning controls should fail");
        assert!(error.error.message.contains("mutually exclusive"));
    }

    /// Verifies protocol-required headers are merged while credential headers remain adapter-owned.
    /// 验证协议必需请求头会被合并，同时凭据请求头仍由适配器独占管理。
    #[test]
    fn execution_headers_merge_protocol_headers_and_reject_credential_overrides() {
        let route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let encoded = client
            .encode_request(&route, &minimal_request())
            .expect("Anthropic request should encode");
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");
        let headers = execution_headers(&route, &encoded.headers, &secret)
            .expect("non-secret protocol headers should merge");
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert_eq!(
            headers
                .get(reqwest::header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok()),
            Some("identity")
        );

        let forbidden =
            BTreeMap::from([(String::from("authorization"), String::from("Bearer other"))]);
        assert!(execution_headers(&route, &forbidden, &secret).is_err());
    }

    /// Verifies one MiniMax credential selects protocol-specific paths and authentication.
    /// 验证同一个 MiniMax 凭据会选择协议专属路径与鉴权方式。
    #[test]
    fn minimax_protocol_selection_controls_wire_route_and_authentication() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");

        let anthropic_route = minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let anthropic_request = minimal_request();
        let anthropic = client
            .encode_request(&anthropic_route, &anthropic_request)
            .expect("Anthropic request should encode");
        let anthropic_headers = execution_headers(&anthropic_route, &anthropic.headers, &secret)
            .expect("Anthropic headers should project");
        assert_eq!(anthropic.path, "/anthropic/v1/messages");
        assert_eq!(
            anthropic_headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("secret")
        );
        assert!(!anthropic_headers.contains_key(reqwest::header::AUTHORIZATION));

        let openai_route = minimax_route(OPENAI_CHAT_PROFILE_ID);
        let mut openai_request = minimal_request();
        openai_request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );
        let openai = client
            .encode_request(&openai_route, &openai_request)
            .expect("OpenAI request should encode");
        let openai_headers = execution_headers(&openai_route, &openai.headers, &secret)
            .expect("OpenAI headers should project");
        assert_eq!(openai.path, "/v1/chat/completions");
        assert_eq!(openai.body["reasoning_split"], true);
        assert_eq!(
            openai_headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert!(!openai_headers.contains_key("x-api-key"));
    }
}
