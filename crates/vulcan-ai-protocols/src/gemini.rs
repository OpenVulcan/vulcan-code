//! Provider-independent Google Gemini GenerateContent wire profile.
//! 供应商无关的 Google Gemini GenerateContent Wire Profile。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiStreamEvent, ContentItem, InputTokenCount, MessageRole,
    ModalityTokenCount, PromptCachePolicy, RequestUsage, ToolCall, ToolCallProgress, UsageValue,
    UsageValueSource,
};

/// Stable profile identifier for Gemini GenerateContent.
/// Gemini GenerateContent 的稳定 Profile 标识。
pub const GEMINI_GENERATE_CONTENT_PROFILE_ID: &str = "google.gemini.generate_content";
/// Stable profile identifier for the Google Code Assist internal Gemini envelope.
/// Google Code Assist 内部 Gemini 封装的稳定 Profile 标识。
pub const GOOGLE_CODE_ASSIST_PROFILE_ID: &str = "google.code_assist.generate_content.v1";

/// Encodes and decodes the provider-independent Gemini GenerateContent wire contract.
/// 编解码供应商无关的 Gemini GenerateContent Wire 契约。
pub struct GeminiGenerateContentProfile;

/// Encodes one Gemini GenerateContent request for the provider-authoritative CountTokens method.
/// 将一个 Gemini GenerateContent 请求编码为供应商权威的 CountTokens 方法请求。
///
/// # Parameters
/// 参数
/// `request` is the same immutable public request that would be dispatched for generation.
/// `request` 是生成请求将要分派的同一个不可变公共请求。
///
/// # Returns
/// 返回值
/// The exact CountTokens request preserving system instructions, tools, contents, and resources.
/// 返回保留系统指令、工具、内容与资源的精确 CountTokens 请求。
pub fn encode_gemini_count_tokens_request(
    request: &AiExecutionRequest,
) -> Result<EncodedWireRequest, ProtocolConversionError> {
    let generation = GeminiGenerateContentProfile.encode(request)?;
    Ok(EncodedWireRequest {
        path: format!("/models/{}:countTokens", request.model_id),
        headers: generation.headers,
        body: json!({
            "generateContentRequest": generation.body,
        }),
    })
}

/// Decodes one Gemini CountTokens response into the provider-neutral token-count contract.
/// 将一个 Gemini CountTokens 响应解码为供应商无关的 Token 统计契约。
///
/// # Parameters
/// 参数
/// `body` is the successful JSON response returned by `models.countTokens`.
/// `body` 是 `models.countTokens` 返回的成功 JSON 响应。
///
/// # Returns
/// 返回值
/// The exact total, cached count, and modality details or a controlled protocol failure.
/// 返回精确总数、缓存数与模态明细，或受控协议错误。
pub fn decode_gemini_count_tokens_response(
    body: &Value,
) -> Result<InputTokenCount, ProtocolConversionError> {
    let total_tokens = body
        .get("totalTokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            ProtocolConversionError::new(
                "Gemini CountTokens response did not contain unsigned `totalTokens`.",
            )
        })?;
    let cached_tokens = optional_unsigned_field(body, "cachedContentTokenCount")?;
    Ok(InputTokenCount {
        total_tokens,
        cached_tokens,
        modality_details: decode_modality_token_counts(body, "promptTokensDetails")?,
        cached_modality_details: decode_modality_token_counts(body, "cacheTokensDetails")?,
    })
}

/// Reads one optional unsigned integer from a Gemini response object.
/// 从 Gemini 响应对象读取一个可选无符号整数。
///
/// # Parameters
/// 参数
/// `body` is the response object and `field` is the exact registered field name.
/// `body` 是响应对象，`field` 是精确注册字段名。
///
/// # Returns
/// 返回值
/// `None` when absent, the exact value when valid, or a protocol error for a wrong type.
/// 字段缺失时返回 `None`，有效时返回精确值，类型错误时返回协议错误。
fn optional_unsigned_field(
    body: &Value,
    field: &str,
) -> Result<Option<u64>, ProtocolConversionError> {
    body.get(field)
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                ProtocolConversionError::new(format!(
                    "Gemini CountTokens response field `{field}` must be an unsigned integer."
                ))
            })
        })
        .transpose()
}

/// Decodes one exact Gemini modality-token detail array.
/// 解码一个精确的 Gemini 模态 Token 明细数组。
///
/// # Parameters
/// 参数
/// `body` is the response object and `field` identifies the full-input or cache detail list.
/// `body` 是响应对象，`field` 标识完整输入或缓存明细列表。
///
/// # Returns
/// 返回值
/// Ordered provider modality counts, or a strict protocol failure for malformed data.
/// 返回有序供应商模态统计，数据损坏时返回严格协议错误。
fn decode_modality_token_counts(
    body: &Value,
    field: &str,
) -> Result<Vec<ModalityTokenCount>, ProtocolConversionError> {
    let Some(value) = body.get(field) else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Gemini CountTokens response field `{field}` must be an array."
        ))
    })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let modality = value
                .get("modality")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    ProtocolConversionError::new(format!(
                        "Gemini CountTokens response `{field}[{index}]` did not contain `modality`."
                    ))
                })?;
            let token_count = value
                .get("tokenCount")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    ProtocolConversionError::new(format!(
                        "Gemini CountTokens response `{field}[{index}]` did not contain unsigned `tokenCount`."
                    ))
                })?;
            Ok(ModalityTokenCount {
                modality: modality.to_string(),
                token_count,
            })
        })
        .collect()
}

impl ProtocolProfile for GeminiGenerateContentProfile {
    /// Returns the stable Gemini profile identifier.
    /// 返回稳定的 Gemini Profile 标识。
    fn id(&self) -> &'static str {
        GEMINI_GENERATE_CONTENT_PROFILE_ID
    }

    /// Encodes one public request into Gemini GenerateContent JSON.
    /// 将一个公共请求编码为 Gemini GenerateContent JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        if request.prompt_cache != PromptCachePolicy::ProviderDefault {
            return Err(ProtocolConversionError::new(
                "Gemini explicit prompt-cache projection is not registered.",
            ));
        }
        let mut system_parts = Vec::new();
        let mut contents = Vec::new();
        // Gemini requires a function name on tool results, so resolve it from the canonical
        // preceding tool-call identity instead of inventing a fallback.
        // Gemini 要求工具结果携带函数名，因此从规范的前序工具调用身份解析，禁止臆造回退值。
        let tool_names = request
            .messages
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(|item| match item {
                ContentItem::ToolCall { call } => Some((call.id.clone(), call.name.clone())),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for message in &request.messages {
            if message.role == MessageRole::System {
                system_parts.extend(encode_gemini_parts(message, &tool_names)?);
            } else {
                contents.push(json!({
                    "role": match message.role {
                        MessageRole::Assistant => "model",
                        MessageRole::User | MessageRole::Tool => "user",
                        MessageRole::System => unreachable!("system messages are separated"),
                    },
                    "parts": encode_gemini_parts(message, &tool_names)?,
                }));
            }
        }
        let mut body = Map::new();
        body.insert(String::from("contents"), Value::Array(contents));
        if !system_parts.is_empty() {
            body.insert(
                String::from("systemInstruction"),
                json!({ "parts": system_parts }),
            );
        }
        if !request.tools.is_empty() {
            body.insert(
                String::from("tools"),
                json!([{
                    "functionDeclarations": request.tools.iter().map(|tool| json!({
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.input_schema,
                    })).collect::<Vec<_>>()
                }]),
            );
            body.insert(
                String::from("toolConfig"),
                json!({ "functionCallingConfig": { "mode": "AUTO" } }),
            );
        }
        if let Some(limit) = request.output_token_limit {
            body.insert(
                String::from("generationConfig"),
                json!({ "maxOutputTokens": limit }),
            );
        }
        if let Some(effort) = request.reasoning_effort {
            let thinking_level = gemini_thinking_level(effort)?;
            let generation_config = body
                .entry(String::from("generationConfig"))
                .or_insert_with(|| Value::Object(Map::new()));
            let generation_config = generation_config.as_object_mut().ok_or_else(|| {
                ProtocolConversionError::new("Gemini generationConfig must be an object.")
            })?;
            generation_config.insert(
                String::from("thinkingConfig"),
                json!({
                    "thinkingLevel": thinking_level,
                    "includeThoughts": true,
                }),
            );
        }
        Ok(EncodedWireRequest {
            path: format!("/models/{}:streamGenerateContent?alt=sse", request.model_id),
            headers: BTreeMap::new(),
            body: Value::Object(body),
        })
    }

    /// Creates one isolated Gemini stream decoder.
    /// 创建一个隔离的 Gemini 流解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(GeminiStreamDecoder::new(execution_id))
    }
}

/// Maps only Gemini-supported public effort values into a native thinking level.
/// 仅将 Gemini 支持的公共推理档位映射为原生思考等级。
fn gemini_thinking_level(
    effort: vulcan_ai_contract::ReasoningEffort,
) -> Result<&'static str, ProtocolConversionError> {
    match effort {
        vulcan_ai_contract::ReasoningEffort::Minimal => Ok("minimal"),
        vulcan_ai_contract::ReasoningEffort::Low => Ok("low"),
        vulcan_ai_contract::ReasoningEffort::Medium => Ok("medium"),
        vulcan_ai_contract::ReasoningEffort::High => Ok("high"),
        unsupported => Err(ProtocolConversionError::new(format!(
            "Gemini does not expose reasoning effort `{}`.",
            unsupported.as_str()
        ))),
    }
}

/// Encodes Gemini semantics inside the reviewed Google Code Assist request envelope.
/// 将 Gemini 语义编码到已审核的 Google Code Assist 请求封装中。
pub struct GoogleCodeAssistProfile;

impl ProtocolProfile for GoogleCodeAssistProfile {
    /// Returns the isolated Google Code Assist profile identifier.
    /// 返回隔离的 Google Code Assist Profile 标识。
    fn id(&self) -> &'static str {
        GOOGLE_CODE_ASSIST_PROFILE_ID
    }

    /// Encodes one public request while leaving credential-bound project injection to the adapter.
    /// 编码一个公共请求，并将凭据绑定项目注入留给适配器。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        let encoded = GeminiGenerateContentProfile.encode(request)?;
        Ok(EncodedWireRequest {
            path: String::from("/v1internal:streamGenerateContent?alt=sse"),
            headers: encoded.headers,
            body: json!({
                "model": request.model_id,
                "request": encoded.body,
            }),
        })
    }

    /// Creates one decoder for the Code Assist response envelope.
    /// 为 Code Assist 响应封装创建一个解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(GoogleCodeAssistStreamDecoder {
            inner: GeminiStreamDecoder::new(execution_id),
        })
    }
}

/// Unwraps Code Assist SSE records before delegating to the canonical Gemini decoder.
/// 在委托给规范 Gemini 解码器前解包 Code Assist SSE 记录。
struct GoogleCodeAssistStreamDecoder {
    /// Canonical Gemini semantic decoder.
    /// 规范 Gemini 语义解码器。
    inner: GeminiStreamDecoder,
}

impl ProtocolStreamDecoder for GoogleCodeAssistStreamDecoder {
    /// Pushes one Code Assist SSE record after requiring its exact `response` envelope.
    /// 在要求精确 `response` 封装后推入一条 Code Assist SSE 记录。
    fn push_sse(
        &mut self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if data.trim() == "[DONE]" {
            return self.inner.push_sse(event_name, data);
        }
        let envelope = serde_json::from_str::<Value>(data).map_err(|error| {
            ProtocolConversionError::new(format!(
                "Google Code Assist stream JSON is invalid: {error}"
            ))
        })?;
        let response = envelope.get("response").ok_or_else(|| {
            ProtocolConversionError::new(
                "Google Code Assist stream record did not contain `response`.",
            )
        })?;
        let response = serde_json::to_string(response).map_err(|error| {
            ProtocolConversionError::new(format!(
                "Google Code Assist response could not be decoded: {error}"
            ))
        })?;
        self.inner.push_sse(event_name, response.as_str())
    }

    /// Finishes the delegated Gemini stream.
    /// 结束委托的 Gemini 流。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        self.inner.finish()
    }
}

/// Encodes one public message into ordered Gemini parts.
/// 将一条公共消息编码为有序 Gemini Part。
fn encode_gemini_parts(
    message: &AiMessage,
    tool_names: &BTreeMap<String, String>,
) -> Result<Vec<Value>, ProtocolConversionError> {
    message
        .content
        .iter()
        .map(|item| match item {
            ContentItem::Text { text } => Ok(json!({ "text": text })),
            ContentItem::StructuredJson { value } => Ok(json!({
                "text": serde_json::to_string(value).map_err(|error| {
                    ProtocolConversionError::new(format!(
                        "Structured JSON could not be encoded for Gemini: {error}"
                    ))
                })?
            })),
            ContentItem::Reasoning { text, signature } => {
                let Some(signature) = signature.as_deref().and_then(|signature| {
                    crate::compatible_provider_signature(
                        crate::SIGNATURE_PROVIDER_GEMINI,
                        signature,
                    )
                }) else {
                    return Ok(Value::Null);
                };
                let mut part = Map::from_iter([
                    (String::from("text"), Value::String(text.clone())),
                    (String::from("thought"), Value::Bool(true)),
                ]);
                part.insert(
                    String::from("thoughtSignature"),
                    Value::String(signature.to_string()),
                );
                Ok(Value::Object(part))
            }
            ContentItem::ToolCall { call } => Ok(json!({
                "functionCall": {
                    "id": call.id,
                    "name": call.name,
                    "args": call.arguments,
                }
            })),
            ContentItem::ToolResult { result } => {
                let tool_name = tool_names.get(result.call_id.as_str()).ok_or_else(|| {
                    ProtocolConversionError::new(format!(
                        "Gemini tool result `{}` has no preceding tool call name.",
                        result.call_id
                    ))
                })?;
                Ok(json!({
                    "functionResponse": {
                        "id": result.call_id,
                        "name": tool_name,
                        "response": {
                            "result": result.content,
                            "is_error": result.is_error,
                        }
                    }
                }))
            }
            ContentItem::Resource { resource } => encode_gemini_resource(resource),
            ContentItem::Refusal { text } => Ok(json!({ "text": text })),
            ContentItem::Citation { title, url, text } => Ok(json!({
                "text": format!(
                    "{}: {}{}",
                    title,
                    url,
                    text.as_ref()
                        .map(|value| format!("\n{value}"))
                        .unwrap_or_default()
                )
            })),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.into_iter().filter(|part| !part.is_null()).collect())
}

/// Encodes one host-resolved resource into a Gemini file or inline-data part.
/// 将一个宿主已解析资源编码为 Gemini 文件或内联数据 Part。
fn encode_gemini_resource(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<Value, ProtocolConversionError> {
    let url = resource.url.as_deref().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Resource `{}` was not resolved before Gemini projection.",
            resource.id
        ))
    })?;
    if !url.starts_with("data:") {
        return Ok(json!({
            "fileData": {
                "mimeType": resource.media_type,
                "fileUri": url,
            }
        }));
    }
    let body = url
        .strip_prefix("data:")
        .ok_or_else(|| ProtocolConversionError::new("Gemini data URL is malformed."))?;
    let (metadata, data) = body.split_once(',').ok_or_else(|| {
        ProtocolConversionError::new("Gemini data URL is missing its payload separator.")
    })?;
    let media_type = metadata
        .strip_suffix(";base64")
        .ok_or_else(|| ProtocolConversionError::new("Gemini data URL must use Base64 encoding."))?;
    Ok(json!({
        "inlineData": {
            "mimeType": media_type,
            "data": data,
        }
    }))
}

/// Incrementally normalizes Gemini SSE chunks into public events.
/// 将 Gemini SSE 分块增量归一化为公共事件。
struct GeminiStreamDecoder {
    /// Stable execution identifier.
    /// 稳定执行标识。
    execution_id: String,
    /// Whether the public response-start event was emitted.
    /// 是否已经发出公共响应开始事件。
    started: bool,
    /// Whether visible text is currently open.
    /// 可见文本是否当前处于打开状态。
    text_open: bool,
    /// Whether reasoning is currently open.
    /// 推理是否当前处于打开状态。
    reasoning_open: bool,
    /// Latest reasoning continuation signature.
    /// 最新推理延续签名。
    reasoning_signature: Option<String>,
    /// Whether a terminal event was emitted.
    /// 是否已经发出终态事件。
    completed: bool,
    /// Stable generated tool ordinal.
    /// 稳定生成工具序号。
    tool_index: u32,
}

impl GeminiStreamDecoder {
    /// Creates one fresh decoder.
    /// 创建一个新的解码器。
    fn new(execution_id: String) -> Self {
        Self {
            execution_id,
            started: false,
            text_open: false,
            reasoning_open: false,
            reasoning_signature: None,
            completed: false,
            tool_index: 0,
        }
    }

    /// Emits the public response-start event once.
    /// 仅发出一次公共响应开始事件。
    fn ensure_started(&mut self, events: &mut Vec<AiStreamEvent>) {
        if !self.started {
            self.started = true;
            events.push(AiStreamEvent::ResponseStarted {
                execution_id: self.execution_id.clone(),
            });
        }
    }

    /// Closes all currently open text/reasoning items.
    /// 关闭当前全部打开的文本/推理项目。
    fn close_open_items(&mut self, events: &mut Vec<AiStreamEvent>) {
        if self.text_open {
            self.text_open = false;
            events.push(AiStreamEvent::ResponseItemCompleted {
                item_id: String::from("gemini:text"),
            });
        }
        if self.reasoning_open {
            self.reasoning_open = false;
            events.push(AiStreamEvent::ReasoningCompleted {
                item_id: String::from("gemini:reasoning"),
                signature: self.reasoning_signature.take().and_then(|signature| {
                    crate::wrap_provider_signature(
                        crate::SIGNATURE_PROVIDER_GEMINI,
                        signature.as_str(),
                    )
                }),
            });
        }
    }

    /// Normalizes one Gemini content part.
    /// 归一化一个 Gemini 内容 Part。
    fn push_part(
        &mut self,
        part: &Value,
        events: &mut Vec<AiStreamEvent>,
    ) -> Result<(), ProtocolConversionError> {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            if part.get("thought").and_then(Value::as_bool) == Some(true) {
                if !self.reasoning_open {
                    self.reasoning_open = true;
                    events.push(AiStreamEvent::ReasoningStarted {
                        item_id: String::from("gemini:reasoning"),
                    });
                }
                if let Some(signature) = part.get("thoughtSignature").and_then(Value::as_str) {
                    self.reasoning_signature = Some(signature.to_string());
                }
                if !text.is_empty() {
                    events.push(AiStreamEvent::ReasoningDelta {
                        item_id: String::from("gemini:reasoning"),
                        delta: text.to_string(),
                    });
                }
            } else {
                if !self.text_open {
                    self.text_open = true;
                    events.push(AiStreamEvent::ResponseItemStarted {
                        item_id: String::from("gemini:text"),
                        item: ContentItem::Text {
                            text: String::new(),
                        },
                    });
                }
                if !text.is_empty() {
                    events.push(AiStreamEvent::ResponseTextDelta {
                        item_id: String::from("gemini:text"),
                        delta: text.to_string(),
                    });
                }
            }
        }
        if let Some(call) = part.get("functionCall").and_then(Value::as_object) {
            let name = call.get("name").and_then(Value::as_str).ok_or_else(|| {
                ProtocolConversionError::new("Gemini functionCall is missing `name`.")
            })?;
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("gemini-call-{}", self.tool_index));
            let arguments = call.get("args").cloned().unwrap_or_else(|| json!({}));
            let argument_text = serde_json::to_string(&arguments).map_err(|error| {
                ProtocolConversionError::new(format!(
                    "Gemini function arguments could not be encoded: {error}"
                ))
            })?;
            let item_id = format!("gemini:tool:{}", self.tool_index);
            events.push(AiStreamEvent::ToolCallStarted {
                item_id: item_id.clone(),
                call_id: call_id.clone(),
                tool_name: name.to_string(),
                index: self.tool_index,
            });
            events.push(AiStreamEvent::ToolCallArgumentsDelta {
                item_id: item_id.clone(),
                delta: argument_text.clone(),
                progress: ToolCallProgress {
                    call_id: Some(call_id.clone()),
                    tool_name: Some(name.to_string()),
                    bytes_received: argument_text.len() as u64,
                    characters_received: argument_text.chars().count() as u64,
                    complete: false,
                },
            });
            events.push(AiStreamEvent::ToolCallCompleted {
                item_id,
                call: ToolCall {
                    id: call_id,
                    name: name.to_string(),
                    arguments,
                },
            });
            self.tool_index = self.tool_index.saturating_add(1);
        }
        Ok(())
    }
}

impl ProtocolStreamDecoder for GeminiStreamDecoder {
    /// Pushes one complete Gemini SSE JSON record.
    /// 推入一条完整的 Gemini SSE JSON 记录。
    fn push_sse(
        &mut self,
        _event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if data.trim() == "[DONE]" {
            return self.finish();
        }
        if self.completed {
            return Err(ProtocolConversionError::new(
                "Gemini emitted data after the response completed.",
            ));
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            ProtocolConversionError::new(format!("Gemini stream JSON is invalid: {error}"))
        })?;
        let mut events = Vec::new();
        self.ensure_started(&mut events);
        if let Some(parts) = value
            .pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
        {
            for part in parts {
                self.push_part(part, &mut events)?;
            }
        }
        if let Some(usage) = value.get("usageMetadata") {
            events.push(AiStreamEvent::UsageUpdated {
                usage: gemini_usage(usage),
            });
        }
        if value
            .pointer("/candidates/0/finishReason")
            .and_then(Value::as_str)
            .is_some()
        {
            self.close_open_items(&mut events);
            self.completed = true;
            events.push(AiStreamEvent::ResponseCompleted);
        }
        Ok(events)
    }

    /// Finishes the Gemini stream and closes any open semantic items.
    /// 结束 Gemini 流并关闭全部打开的语义项目。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.completed {
            return Ok(Vec::new());
        }
        Err(ProtocolConversionError::new(
            "Gemini stream ended before a candidate finish reason was received.",
        ))
    }
}

/// Converts Gemini usage metadata without manufacturing missing values.
/// 转换 Gemini 用量元数据且不伪造缺失值。
fn gemini_usage(value: &Value) -> RequestUsage {
    let input = reported_usage(value.get("promptTokenCount"));
    let output = reported_usage(value.get("candidatesTokenCount"));
    let reasoning = reported_usage(value.get("thoughtsTokenCount"));
    let cache_read = reported_usage(value.get("cachedContentTokenCount"));
    let total = reported_usage(value.get("totalTokenCount"));
    RequestUsage {
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: reasoning,
        cache_read_tokens: cache_read,
        total_tokens: total,
        ..RequestUsage::default()
    }
}

/// Converts one optional non-negative JSON integer into reported usage.
/// 将一个可选非负 JSON 整数转换为已报告用量。
fn reported_usage(value: Option<&Value>) -> UsageValue {
    value
        .and_then(Value::as_u64)
        .map(|value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        })
        .unwrap_or(UsageValue::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::{AiMessage, AiOperation, ToolDefinition};

    /// Verifies Gemini encoding remains provider-independent while preserving tools.
    /// 验证 Gemini 编码保持供应商无关并保留工具。
    #[test]
    fn encodes_public_request_and_tool() {
        let encoded = GeminiGenerateContentProfile
            .encode(&AiExecutionRequest {
                operation: AiOperation::ConversationRespond,
                model_id: String::from("gemini-model"),
                cache_scope_id: None,
                messages: vec![AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: String::from("hello"),
                    }],
                }],
                tools: vec![ToolDefinition {
                    name: String::from("lookup"),
                    description: String::from("Lookup"),
                    input_schema: json!({"type":"object"}),
                }],
                native_tools: Vec::new(),
                prompt_cache: PromptCachePolicy::ProviderDefault,
                reasoning_effort: Some(vulcan_ai_contract::ReasoningEffort::Minimal),
                model_parameters: BTreeMap::new(),
                resources: Vec::new(),
                output_token_limit: Some(128),
            })
            .expect("request should encode");
        assert_eq!(encoded.body["contents"][0]["role"], "user");
        assert_eq!(
            encoded.body["tools"][0]["functionDeclarations"][0]["name"],
            "lookup"
        );
        assert_eq!(
            encoded.body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        assert_eq!(
            encoded.body["generationConfig"]["thinkingConfig"]["includeThoughts"],
            true
        );
    }

    /// Verifies CountTokens preserves the complete GenerateContent input inside its exact envelope.
    /// 验证 CountTokens 会在精确封装中保留完整 GenerateContent 输入。
    #[test]
    fn count_tokens_encodes_complete_generate_content_request() {
        let request = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-model"),
            cache_scope_id: None,
            messages: vec![
                AiMessage {
                    role: MessageRole::System,
                    content: vec![ContentItem::Text {
                        text: String::from("Follow instructions."),
                    }],
                },
                AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: String::from("hello"),
                    }],
                },
            ],
            tools: vec![ToolDefinition {
                name: String::from("lookup"),
                description: String::from("Lookup"),
                input_schema: json!({"type":"object"}),
            }],
            native_tools: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(128),
        };
        let encoded =
            encode_gemini_count_tokens_request(&request).expect("count request should encode");
        assert_eq!(encoded.path, "/models/gemini-model:countTokens");
        assert_eq!(
            encoded.body["generateContentRequest"]["systemInstruction"]["parts"][0]["text"],
            "Follow instructions."
        );
        assert_eq!(
            encoded.body["generateContentRequest"]["contents"][0]["parts"][0]["text"],
            "hello"
        );
        assert_eq!(
            encoded.body["generateContentRequest"]["tools"][0]["functionDeclarations"][0]["name"],
            "lookup"
        );
        assert!(encoded.body.get("contents").is_none());
    }

    /// Verifies CountTokens decodes exact totals and both provider modality detail arrays.
    /// 验证 CountTokens 会解码精确总数与两组供应商模态明细。
    #[test]
    fn count_tokens_decodes_total_cache_and_modalities() {
        let decoded = decode_gemini_count_tokens_response(&json!({
            "totalTokens": 321,
            "cachedContentTokenCount": 100,
            "promptTokensDetails": [
                {"modality": "TEXT", "tokenCount": 21},
                {"modality": "IMAGE", "tokenCount": 300}
            ],
            "cacheTokensDetails": [
                {"modality": "TEXT", "tokenCount": 100}
            ]
        }))
        .expect("count response should decode");
        assert_eq!(decoded.total_tokens, 321);
        assert_eq!(decoded.cached_tokens, Some(100));
        assert_eq!(decoded.modality_details[1].modality, "IMAGE");
        assert_eq!(decoded.modality_details[1].token_count, 300);
        assert_eq!(decoded.cached_modality_details[0].token_count, 100);
    }

    /// Verifies Gemini stream output, tool identity, and usage normalize into public events.
    /// 验证 Gemini 流输出、工具身份和用量归一化为公共事件。
    #[test]
    fn decodes_text_tool_and_usage() {
        let mut decoder = GeminiStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                None,
                r#"{"candidates":[{"content":{"parts":[{"text":"hello"},{"functionCall":{"id":"call","name":"lookup","args":{"q":"x"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":7,"candidatesTokenCount":3}}"#,
            )
            .expect("chunk should decode");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTextDelta { delta, .. } if delta == "hello"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. } if call.name == "lookup"
        )));
        assert!(matches!(
            events.last(),
            Some(AiStreamEvent::ResponseCompleted)
        ));
    }

    /// Verifies Code Assist wraps Gemini requests and unwraps its response envelope.
    /// 验证 Code Assist 会封装 Gemini 请求并解开其响应信封。
    #[test]
    fn code_assist_wraps_request_and_unwraps_stream() {
        let request = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-code-assist"),
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
            output_token_limit: Some(128),
        };
        let encoded = GoogleCodeAssistProfile
            .encode(&request)
            .expect("Code Assist request should encode");
        assert_eq!(encoded.path, "/v1internal:streamGenerateContent?alt=sse");
        assert_eq!(encoded.body["model"], "gemini-code-assist");
        assert_eq!(encoded.body["request"]["contents"][0]["role"], "user");
        assert!(encoded.body.get("project").is_none());

        let mut decoder =
            GoogleCodeAssistProfile.create_stream_decoder(String::from("execution"), &request);
        let events = decoder
            .push_sse(
                None,
                r#"{"response":{"candidates":[{"content":{"parts":[{"text":"hello"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":1}}}"#,
            )
            .expect("Code Assist envelope should decode");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTextDelta { delta, .. } if delta == "hello"
        )));
    }
}
