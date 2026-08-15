//! Provider-independent Google Gemini GenerateContent wire profile.
//! 供应商无关的 Google Gemini GenerateContent Wire Profile。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiStreamEvent, ContentItem, MessageRole, NativeToolKind,
    ParallelToolCallPolicy, PromptCachePolicy, ReasoningOutputPolicy, RequestUsage,
    StructuredOutputPolicy, ToolCall, ToolCallProgress, ToolChoicePolicy, UsageValue,
    UsageValueSource, derive_uncached_input,
};

/// Stable profile identifier for Gemini GenerateContent.
/// Gemini GenerateContent 的稳定 Profile 标识。
pub const GEMINI_GENERATE_CONTENT_PROFILE_ID: &str = "google.gemini.generate_content";
/// Stable profile identifier for the Google Code Assist internal Gemini envelope.
/// Google Code Assist 内部 Gemini 封装的稳定 Profile 标识。
pub const GOOGLE_CODE_ASSIST_PROFILE_ID: &str = "google.code_assist.generate_content.v1";

/// Returns the opaque payload only when a replay signature belongs to Gemini.
/// 仅当重放签名属于 Gemini 时返回其不透明载荷。
///
/// # Parameters
/// 参数
/// `signature` is the provider-scoped replay signature carried by the public contract.
/// `signature` 是公共契约携带的供应商作用域重放签名。
///
/// # Returns
/// 返回值
/// The opaque Gemini payload, or `None` for another provider or malformed provenance.
/// 返回 Gemini 不透明载荷；若属于其他供应商或来源格式错误则返回 `None`。
pub fn gemini_replay_signature_payload(signature: &str) -> Option<&str> {
    crate::compatible_provider_signature(crate::SIGNATURE_PROVIDER_GEMINI, signature)
}

/// Builds the native Gemini streaming GenerateContent path for one exact model identifier.
/// 为一个精确模型标识构建 Gemini 原生流式 GenerateContent 路径。
///
/// # Arguments
/// 参数
/// - `model_id`: The exact provider model identifier carried by the canonical request.
/// - `model_id`：规范请求携带的精确供应商模型标识。
///
/// # Returns
/// 返回值
/// The same-origin request path expected by Gemini GenerateContent endpoints.
/// Gemini GenerateContent 端点所需的同源请求路径。
pub fn gemini_generate_content_path(model_id: &str) -> String {
    format!("/models/{model_id}:streamGenerateContent?alt=sse")
}

/// Encodes and decodes the provider-independent Gemini GenerateContent wire contract.
/// 编解码供应商无关的 Gemini GenerateContent Wire 契约。
pub struct GeminiGenerateContentProfile;

impl ProtocolProfile for GeminiGenerateContentProfile {
    /// Returns the stable Gemini profile identifier.
    /// 返回稳定的 Gemini Profile 标识。
    fn id(&self) -> &'static str {
        GEMINI_GENERATE_CONTENT_PROFILE_ID
    }

    /// Returns the bounded Gemini SSE transport declaration.
    /// 返回有界 Gemini SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one public request into Gemini GenerateContent JSON.
    /// 将一个公共请求编码为 Gemini GenerateContent JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::require_no_route_bound_provider_context(request, self.id())?;
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
            if matches!(message.role, MessageRole::System | MessageRole::Developer) {
                system_parts.extend(encode_gemini_parts(message, &tool_names)?);
            } else {
                contents.push(json!({
                    "role": match message.role {
                        MessageRole::Assistant => "model",
                        MessageRole::User | MessageRole::Tool => "user",
                        MessageRole::System | MessageRole::Developer => {
                            unreachable!("instruction messages are separated")
                        }
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
        let mut tools = Vec::new();
        if !request.tools.is_empty() {
            if let Some(tool) = request.tools.iter().find(|tool| tool.strict) {
                return Err(ProtocolConversionError::new(format!(
                    "Gemini generateContent has no registered strict-schema projector for tool `{}`.",
                    tool.name
                )));
            }
            tools.push(json!({
                "functionDeclarations": request.tools.iter().map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                })).collect::<Vec<_>>()
            }));
        }
        for native_tool in &request.native_tools {
            tools.push(match native_tool {
                NativeToolKind::WebSearch => json!({ "googleSearch": {} }),
                NativeToolKind::CodeInterpreter => json!({ "codeExecution": {} }),
                NativeToolKind::ImageGeneration => {
                    return Err(ProtocolConversionError::new(
                        "Gemini GenerateContent has no registered native image-generation tool projector.",
                    ));
                }
            });
        }
        if !tools.is_empty() {
            body.insert(String::from("tools"), Value::Array(tools));
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
        project_gemini_request_controls(request, &mut body)?;
        Ok(EncodedWireRequest {
            path: gemini_generate_content_path(request.model_id.as_str()),
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

/// Projects provider-neutral request controls into Gemini GenerateContent fields.
/// 将供应商无关请求控制投影到 Gemini GenerateContent 字段。
///
/// # Parameters
/// 参数
/// `request` is the exact public request and `body` is the mutable Gemini object.
/// `request` 是精确公共请求，`body` 是可变 Gemini 对象。
///
/// # Returns
/// 返回值
/// A controlled conversion error when Gemini cannot represent a requested policy.
/// 当 Gemini 无法表示请求策略时返回受控转换错误。
fn project_gemini_request_controls(
    request: &AiExecutionRequest,
    body: &mut Map<String, Value>,
) -> Result<(), ProtocolConversionError> {
    if !request.tools.is_empty() && !matches!(request.tool_choice, ToolChoicePolicy::Auto) {
        let function_calling_config = match &request.tool_choice {
            ToolChoicePolicy::Auto => json!({ "mode": "AUTO" }),
            ToolChoicePolicy::Required => json!({ "mode": "ANY" }),
            ToolChoicePolicy::Disabled => json!({ "mode": "NONE" }),
            ToolChoicePolicy::Specific { tool_name } => {
                require_gemini_registered_tool(request, tool_name)?;
                json!({
                    "mode": "ANY",
                    "allowedFunctionNames": [tool_name],
                })
            }
        };
        body.insert(
            String::from("toolConfig"),
            json!({ "functionCallingConfig": function_calling_config }),
        );
    }
    if !request.tools.is_empty()
        && matches!(
            request.parallel_tool_calls,
            ParallelToolCallPolicy::Disabled
        )
    {
        return Err(ProtocolConversionError::new(
            "Gemini GenerateContent has no registered serial-tool-call projector.",
        ));
    }
    match request.reasoning_output {
        ReasoningOutputPolicy::Hidden if request.reasoning_effort.is_none() => {
            return Err(ProtocolConversionError::new(
                "Gemini hidden reasoning output requires an active reasoning effort.",
            ));
        }
        ReasoningOutputPolicy::SummaryAuto => {
            return Err(ProtocolConversionError::new(
                "Gemini GenerateContent has no registered automatic reasoning-summary projector.",
            ));
        }
        ReasoningOutputPolicy::ProviderDefault
        | ReasoningOutputPolicy::Hidden
        | ReasoningOutputPolicy::Detailed => {}
    }
    let needs_generation_config =
        !matches!(request.structured_output, StructuredOutputPolicy::None)
            || !request.stop_sequences.is_empty()
            || matches!(request.reasoning_output, ReasoningOutputPolicy::Hidden);
    if !needs_generation_config {
        return Ok(());
    }
    let generation_config = body
        .entry(String::from("generationConfig"))
        .or_insert_with(|| Value::Object(Map::new()));
    match &request.structured_output {
        StructuredOutputPolicy::None => {}
        StructuredOutputPolicy::JsonObject => {
            generation_config["responseMimeType"] = Value::String(String::from("application/json"));
        }
        StructuredOutputPolicy::JsonSchema { schema, .. } => {
            generation_config["responseMimeType"] = Value::String(String::from("application/json"));
            generation_config["responseJsonSchema"] = schema.clone();
        }
    }
    if !request.stop_sequences.is_empty() {
        generation_config["stopSequences"] = json!(request.stop_sequences);
    }
    match request.reasoning_output {
        ReasoningOutputPolicy::ProviderDefault | ReasoningOutputPolicy::Detailed => {}
        ReasoningOutputPolicy::Hidden => {
            generation_config["thinkingConfig"]["includeThoughts"] = Value::Bool(false);
        }
        ReasoningOutputPolicy::SummaryAuto => unreachable!("validated before projection"),
    }
    Ok(())
}

/// Verifies a Gemini specific-tool selection references a registered request tool.
/// 验证 Gemini 特定工具选择引用了已注册请求工具。
///
/// # Parameters
/// 参数
/// `request` owns the registered tools and `tool_name` is the exact selected name.
/// `request` 拥有已注册工具，`tool_name` 是精确所选名称。
///
/// # Returns
/// 返回值
/// Success only when the tool exists in this request.
/// 仅当工具存在于本次请求时成功。
fn require_gemini_registered_tool(
    request: &AiExecutionRequest,
    tool_name: &str,
) -> Result<(), ProtocolConversionError> {
    if request.tools.iter().any(|tool| tool.name == tool_name) {
        return Ok(());
    }
    Err(ProtocolConversionError::new(format!(
        "Gemini tool choice references unregistered tool `{tool_name}`."
    )))
}

/// Maps only Gemini-supported public effort values into a native thinking level.
/// 仅将 Gemini 支持的公共推理档位映射为原生思考等级。
fn gemini_thinking_level(
    effort: vulcan_ai_contract::ReasoningEffort,
) -> Result<&'static str, ProtocolConversionError> {
    match effort {
        vulcan_ai_contract::ReasoningEffort::Minimal => Ok("MINIMAL"),
        vulcan_ai_contract::ReasoningEffort::Low => Ok("LOW"),
        vulcan_ai_contract::ReasoningEffort::Medium => Ok("MEDIUM"),
        vulcan_ai_contract::ReasoningEffort::High => Ok("HIGH"),
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

    /// Returns the bounded Google Code Assist SSE transport declaration.
    /// 返回有界 Google Code Assist SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
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
            ContentItem::ToolCall { call } => {
                // part preserves a Gemini-owned function signature only when its provider scope is valid.
                // part 仅在供应商作用域有效时保留 Gemini 自有的函数签名。
                let mut part = json!({
                    "functionCall": {
                        "id": call.id,
                        "name": call.name,
                        "args": call.arguments,
                    }
                });
                if let Some(signature) = call.signature.as_deref().and_then(|signature| {
                    crate::compatible_provider_signature(
                        crate::SIGNATURE_PROVIDER_GEMINI,
                        signature,
                    )
                }) {
                    part["thoughtSignature"] = Value::String(signature.to_string());
                }
                Ok(part)
            }
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
    if resource.image_detail.is_some() {
        return Err(ProtocolConversionError::new(format!(
            "Gemini generateContent cannot preserve per-image detail for resource `{}`.",
            resource.id
        )));
    }
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
    /// Stable identifier of the currently open visible-text item.
    /// 当前打开的可见文本项目稳定标识。
    text_item_id: Option<String>,
    /// Stable identifier of the currently open reasoning item.
    /// 当前打开的推理项目稳定标识。
    reasoning_item_id: Option<String>,
    /// Latest reasoning continuation signature.
    /// 最新推理延续签名。
    reasoning_signature: Option<String>,
    /// Whether a terminal event was emitted.
    /// 是否已经发出终态事件。
    completed: bool,
    /// Stable generated tool ordinal.
    /// 稳定生成工具序号。
    tool_index: u32,
    /// Stable ordinal shared by ordered text and reasoning items.
    /// 由有序文本与推理项目共享的稳定序号。
    content_item_index: u32,
}

impl GeminiStreamDecoder {
    /// Creates one fresh decoder.
    /// 创建一个新的解码器。
    fn new(execution_id: String) -> Self {
        Self {
            execution_id,
            started: false,
            text_item_id: None,
            reasoning_item_id: None,
            reasoning_signature: None,
            completed: false,
            tool_index: 0,
            content_item_index: 0,
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

    /// Closes the currently open visible-text item.
    /// 关闭当前打开的可见文本项目。
    ///
    /// # Parameters
    /// 参数
    /// `events` receives the matching completion event when a text item is open.
    /// `events` 在存在打开文本项目时接收匹配的完成事件。
    fn close_text_item(&mut self, events: &mut Vec<AiStreamEvent>) {
        if let Some(item_id) = self.text_item_id.take() {
            events.push(AiStreamEvent::ResponseItemCompleted { item_id });
        }
    }

    /// Closes the currently open reasoning item with its own continuation signature.
    /// 使用其自有延续签名关闭当前打开的推理项目。
    ///
    /// # Parameters
    /// 参数
    /// `events` receives the matching signed completion event when reasoning is open.
    /// `events` 在存在打开推理项目时接收匹配的带签名完成事件。
    fn close_reasoning_item(&mut self, events: &mut Vec<AiStreamEvent>) {
        if let Some(item_id) = self.reasoning_item_id.take() {
            events.push(AiStreamEvent::ReasoningCompleted {
                item_id,
                signature: self.reasoning_signature.take().and_then(|signature| {
                    crate::wrap_provider_signature(
                        crate::SIGNATURE_PROVIDER_GEMINI,
                        signature.as_str(),
                    )
                }),
            });
        }
    }

    /// Closes all currently open text and reasoning items in stream order.
    /// 按流顺序关闭当前全部打开的文本与推理项目。
    ///
    /// # Parameters
    /// 参数
    /// `events` receives any required completion events.
    /// `events` 接收所有必要的完成事件。
    fn close_open_items(&mut self, events: &mut Vec<AiStreamEvent>) {
        self.close_text_item(events);
        self.close_reasoning_item(events);
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
                self.close_text_item(events);
                let incoming_signature = part.get("thoughtSignature").and_then(Value::as_str);
                if self.reasoning_item_id.is_some()
                    && incoming_signature.is_some()
                    && self.reasoning_signature.as_deref() != incoming_signature
                {
                    self.close_reasoning_item(events);
                }
                if self.reasoning_item_id.is_none() {
                    let item_id = format!("gemini:reasoning:{}", self.content_item_index);
                    self.content_item_index = self.content_item_index.saturating_add(1);
                    self.reasoning_item_id = Some(item_id.clone());
                    events.push(AiStreamEvent::ReasoningStarted { item_id });
                }
                if let Some(signature) = incoming_signature {
                    self.reasoning_signature = Some(signature.to_string());
                }
                if !text.is_empty() {
                    events.push(AiStreamEvent::ReasoningDelta {
                        item_id: self
                            .reasoning_item_id
                            .clone()
                            .expect("reasoning item must be open before its delta"),
                        delta: text.to_string(),
                    });
                }
            } else {
                self.close_reasoning_item(events);
                if self.text_item_id.is_none() {
                    let item_id = format!("gemini:text:{}", self.content_item_index);
                    self.content_item_index = self.content_item_index.saturating_add(1);
                    self.text_item_id = Some(item_id.clone());
                    events.push(AiStreamEvent::ResponseItemStarted {
                        item_id,
                        item: ContentItem::Text {
                            text: String::new(),
                        },
                    });
                }
                if !text.is_empty() {
                    events.push(AiStreamEvent::ResponseTextDelta {
                        item_id: self
                            .text_item_id
                            .clone()
                            .expect("text item must be open before its delta"),
                        delta: text.to_string(),
                    });
                }
            }
        }
        if let Some(call) = part.get("functionCall").and_then(Value::as_object) {
            self.close_open_items(events);
            let name = call.get("name").and_then(Value::as_str).ok_or_else(|| {
                ProtocolConversionError::new("Gemini functionCall is missing `name`.")
            })?;
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("gemini-call-{}", self.tool_index));
            let arguments = call.get("args").cloned().unwrap_or_else(|| json!({}));
            // signature belongs to this exact function-call part and must remain positionally attached.
            // signature 属于当前精确函数调用 Part，必须保持位置绑定。
            let signature = part
                .get("thoughtSignature")
                .and_then(Value::as_str)
                .and_then(|signature| {
                    crate::wrap_provider_signature(crate::SIGNATURE_PROVIDER_GEMINI, signature)
                });
            let argument_text = serde_json::to_string(&arguments).map_err(|error| {
                ProtocolConversionError::new(format!(
                    "Gemini function arguments could not be encoded: {error}"
                ))
            })?;
            if argument_text.len() > crate::MAX_STREAMED_TOOL_ARGUMENT_BYTES {
                return Err(ProtocolConversionError::new(format!(
                    "Gemini tool arguments exceeded the registered {} byte limit.",
                    crate::MAX_STREAMED_TOOL_ARGUMENT_BYTES
                )));
            }
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
                    signature,
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
                usage: Box::new(gemini_usage(usage)),
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
        uncached_input_tokens: derive_uncached_input(input, cache_read),
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
                continuation: None,
                messages: vec![AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: String::from("hello"),
                    }],
                    provider_context_items: Vec::new(),
                }],
                tools: vec![ToolDefinition {
                    name: String::from("lookup"),
                    description: String::from("Lookup"),
                    input_schema: json!({"type":"object"}),
                    strict: false,
                }],
                native_tools: Vec::new(),
                tool_choice: Default::default(),
                parallel_tool_calls: Default::default(),
                structured_output: Default::default(),
                stop_sequences: Vec::new(),
                prompt_cache: PromptCachePolicy::ProviderDefault,
                reasoning_effort: Some(vulcan_ai_contract::ReasoningEffort::Minimal),
                reasoning_output: Default::default(),
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
            "MINIMAL"
        );
        assert_eq!(
            encoded.body["generationConfig"]["thinkingConfig"]["includeThoughts"],
            true
        );
        assert!(encoded.body.get("toolConfig").is_none());
    }

    /// Verifies neutral disabled-tool controls are omitted when no function tools exist.
    /// 验证不存在函数工具时会省略中性的禁用工具控制。
    #[test]
    fn encodes_tool_free_validation_request_without_serial_projection() {
        let request = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-model"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("Reply with exactly OK."),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: ToolChoicePolicy::Disabled,
            parallel_tool_calls: ParallelToolCallPolicy::Disabled,
            structured_output: StructuredOutputPolicy::None,
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            reasoning_output: ReasoningOutputPolicy::ProviderDefault,
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(256),
        };

        let encoded = GeminiGenerateContentProfile
            .encode(&request)
            .expect("tool-free validation request should encode");

        assert!(encoded.body.get("toolConfig").is_none());
    }

    /// Verifies GenerateContent projects native search and managed code execution separately.
    /// 验证 GenerateContent 会分别投影原生搜索与托管代码执行。
    #[test]
    fn encodes_native_search_and_code_execution() {
        let mut request = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-model"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("research and calculate"),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: vec![NativeToolKind::WebSearch, NativeToolKind::CodeInterpreter],
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(128),
        };
        let encoded = GeminiGenerateContentProfile
            .encode(&request)
            .expect("Gemini native tools should encode");
        assert_eq!(encoded.body["tools"][0]["googleSearch"], json!({}));
        assert_eq!(encoded.body["tools"][1]["codeExecution"], json!({}));

        request.native_tools = vec![NativeToolKind::ImageGeneration];
        let error = GeminiGenerateContentProfile
            .encode(&request)
            .expect_err("unregistered native image generation should fail");
        assert!(error.to_string().contains("image-generation"));
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

    /// Verifies a function-call thought signature stays attached to the exact call during replay.
    /// 验证函数调用思考签名在重放期间始终绑定到精确调用。
    #[test]
    fn preserves_function_call_thought_signature() {
        let mut decoder = GeminiStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                None,
                r#"{"candidates":[{"content":{"parts":[{"functionCall":{"id":"call","name":"lookup","args":{"q":"x"}},"thoughtSignature":"signed-call"}]},"finishReason":"STOP"}]}"#,
            )
            .expect("signed function call should decode");
        let call = events
            .iter()
            .find_map(|event| match event {
                AiStreamEvent::ToolCallCompleted { call, .. } => Some(call.clone()),
                _ => None,
            })
            .expect("tool call should be emitted");
        assert_eq!(call.signature.as_deref(), Some("gemini#signed-call"));

        let request = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-model"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::Assistant,
                content: vec![ContentItem::ToolCall { call }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: None,
        };
        let encoded = GeminiGenerateContentProfile
            .encode(&request)
            .expect("signed function call should encode");
        assert_eq!(
            encoded.body["contents"][0]["parts"][0]["thoughtSignature"],
            "signed-call"
        );
    }

    /// Verifies interleaved Gemini reasoning blocks retain separate signatures and item order.
    /// 验证交错的 Gemini 推理块保留独立签名与项目顺序。
    #[test]
    fn preserves_multiple_interleaved_reasoning_signatures() {
        let mut decoder = GeminiStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                None,
                r#"{"candidates":[{"content":{"parts":[{"text":"first","thought":true,"thoughtSignature":"sig-a"},{"text":"visible"},{"text":"second","thought":true,"thoughtSignature":"sig-b"}]},"finishReason":"STOP"}]}"#,
            )
            .expect("interleaved signed reasoning should decode");
        let completed = events
            .iter()
            .filter_map(|event| match event {
                AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                    Some((item_id.as_str(), signature.as_deref()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            completed,
            vec![
                ("gemini:reasoning:0", Some("gemini#sig-a")),
                ("gemini:reasoning:2", Some("gemini#sig-b")),
            ]
        );
    }

    /// Verifies Code Assist wraps Gemini requests and unwraps its response envelope.
    /// 验证 Code Assist 会封装 Gemini 请求并解开其响应信封。
    #[test]
    fn code_assist_wraps_request_and_unwraps_stream() {
        let request = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-code-assist"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("hello"),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            reasoning_output: Default::default(),
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
