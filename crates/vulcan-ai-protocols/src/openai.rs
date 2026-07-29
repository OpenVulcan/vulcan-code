//! OpenAI Chat Completions and Responses wire profiles.
//! OpenAI Chat Completions 与 Responses Wire Profile。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder};
use serde_json::{Value, json};
use vulcan_ai_contract::{
    AiExecutionRequest, ContentItem, MessageRole, NativeToolKind, PromptCachePolicy,
};

/// Stable OpenAI Chat Completions profile identifier.
/// 稳定的 OpenAI Chat Completions Profile 标识。
pub const OPENAI_CHAT_PROFILE_ID: &str = "openai-chat-completions.v1";
/// Stable OpenAI Responses profile identifier.
/// 稳定的 OpenAI Responses Profile 标识。
pub const OPENAI_RESPONSES_PROFILE_ID: &str = "openai-responses.v1";
/// Stable OpenAI Codex subscription Responses profile identifier.
/// 稳定的 OpenAI Codex 订阅 Responses Profile 标识。
pub const OPENAI_CODEX_RESPONSES_PROFILE_ID: &str = "openai-codex-responses.v1";

/// Encodes public requests as OpenAI Chat Completions requests.
/// 将公共请求编码为 OpenAI Chat Completions 请求。
#[derive(Debug, Default)]
pub struct OpenAiChatProfile;

impl ProtocolProfile for OpenAiChatProfile {
    /// Returns the stable Chat Completions profile identifier.
    /// 返回稳定的 Chat Completions Profile 标识。
    fn id(&self) -> &'static str {
        OPENAI_CHAT_PROFILE_ID
    }

    /// Encodes one public request as Chat Completions JSON.
    /// 将一个公共请求编码为 Chat Completions JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        require_default_cache_policy(&request.prompt_cache, self.id())?;
        let messages = encode_messages(request)?;
        let mut body = json!({
            "model": request.model_id,
            "messages": messages,
            "tools": encode_openai_tools(request),
            "stream": true,
            "stream_options": {
                "include_usage": true,
            },
        });
        if let Some(limit) = request.output_token_limit {
            body["max_completion_tokens"] = json!(limit);
        }
        if let Some(effort) = request.reasoning_effort {
            body["reasoning_effort"] = Value::String(effort.as_str().to_string());
        }
        Ok(EncodedWireRequest {
            path: String::from("/v1/chat/completions"),
            headers: Default::default(),
            body,
        })
    }

    fn create_stream_decoder(
        &self,
        execution_id: String,
        request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(
            crate::openai_stream::OpenAiChatStreamDecoder::with_registered_tools(
                execution_id,
                request.tools.iter().map(|tool| tool.name.clone()).collect(),
            ),
        )
    }
}

/// Encodes public requests as OpenAI Responses requests.
/// 将公共请求编码为 OpenAI Responses 请求。
#[derive(Debug, Default)]
pub struct OpenAiResponsesProfile;

impl ProtocolProfile for OpenAiResponsesProfile {
    /// Returns the stable Responses profile identifier.
    /// 返回稳定的 Responses Profile 标识。
    fn id(&self) -> &'static str {
        OPENAI_RESPONSES_PROFILE_ID
    }

    /// Encodes one public request as Responses JSON.
    /// 将一个公共请求编码为 Responses JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        require_default_cache_policy(&request.prompt_cache, self.id())?;
        let input = encode_responses_input(request, true)?;
        let mut body = json!({
            "model": request.model_id,
            "input": input,
            "tools": encode_responses_tools(request),
            "stream": true,
            "store": false,
        });
        if let Some(limit) = request.output_token_limit {
            body["max_output_tokens"] = json!(limit);
        }
        if let Some(effort) = request.reasoning_effort {
            body["reasoning"] = json!({
                "effort": effort.as_str(),
                "summary": "auto",
            });
        }
        Ok(EncodedWireRequest {
            path: String::from("/v1/responses"),
            headers: Default::default(),
            body,
        })
    }

    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(crate::openai_stream::OpenAiResponsesStreamDecoder::new(
            execution_id,
        ))
    }
}

/// Encodes public requests for the Codex subscription Responses endpoint.
/// 为 Codex 订阅 Responses 端点编码公共请求。
#[derive(Debug, Default)]
pub struct OpenAiCodexResponsesProfile;

impl ProtocolProfile for OpenAiCodexResponsesProfile {
    /// Returns the stable Codex Responses profile identifier.
    /// 返回稳定的 Codex Responses Profile 标识。
    fn id(&self) -> &'static str {
        OPENAI_CODEX_RESPONSES_PROFILE_ID
    }

    /// Encodes one public request using the isolated Codex request identity.
    /// 使用隔离的 Codex 请求身份编码一个公共请求。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        require_default_cache_policy(&request.prompt_cache, self.id())?;
        let input = encode_responses_input(request, false)?;
        let instructions = encode_codex_instructions(request)?;
        let mut body = json!({
            "model": request.model_id,
            "instructions": instructions,
            "input": input,
            "tools": encode_codex_responses_tools(request),
            "stream": true,
            "store": false,
        });
        if !request.tools.is_empty() {
            body["parallel_tool_calls"] = Value::Bool(true);
        }
        if let Some(effort) = request.reasoning_effort {
            body["reasoning"] = json!({
                "effort": effort.as_str(),
                "summary": "auto",
            });
        }
        let mut headers = std::collections::BTreeMap::new();
        if let Some(cache_scope_id) = request.cache_scope_id.as_deref() {
            body["prompt_cache_key"] = Value::String(cache_scope_id.to_string());
            headers.insert(String::from("session_id"), cache_scope_id.to_string());
        }
        Ok(EncodedWireRequest {
            path: String::from("/responses"),
            headers,
            body,
        })
    }

    /// Creates one isolated Codex Responses stream decoder.
    /// 创建一个隔离的 Codex Responses 流解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(crate::openai_stream::OpenAiResponsesStreamDecoder::new(
            execution_id,
        ))
    }
}

/// Requires provider-default caching until an adapter registers a projector.
/// 在适配器注册投影器前要求使用供应商默认缓存。
fn require_default_cache_policy(
    policy: &PromptCachePolicy,
    profile_id: &str,
) -> Result<(), ProtocolConversionError> {
    if matches!(policy, PromptCachePolicy::ProviderDefault) {
        return Ok(());
    }
    Err(ProtocolConversionError::new(format!(
        "Protocol profile `{profile_id}` requires a provider cache projector for explicit caching."
    )))
}

/// Encodes public messages into OpenAI-compatible messages.
/// 将公共消息编码为 OpenAI 兼容消息。
fn encode_messages(request: &AiExecutionRequest) -> Result<Vec<Value>, ProtocolConversionError> {
    request
        .messages
        .iter()
        .map(|message| {
            if message.role == MessageRole::Tool {
                let result = message
                    .content
                    .iter()
                    .find_map(|item| match item {
                        ContentItem::ToolResult { result } => Some(result),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        ProtocolConversionError::new(
                            "OpenAI tool messages require one public tool result.",
                        )
                    })?;
                return Ok(json!({
                    "role": "tool",
                    "tool_call_id": result.call_id,
                    "content": encode_json_or_string(&result.content)?,
                }));
            }
            let role = match message.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => {
                    return Err(ProtocolConversionError::new(
                        "OpenAI tool messages require one public tool result.",
                    ));
                }
            };
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            let mut resource_parts = Vec::new();
            for item in &message.content {
                match item {
                    ContentItem::Text { text: item_text }
                    | ContentItem::Refusal { text: item_text } => text.push_str(item_text),
                    ContentItem::Reasoning { .. } => {}
                    ContentItem::StructuredJson { value } => {
                        text.push_str(
                            serde_json::to_string(value)
                                .map_err(|error| {
                                    ProtocolConversionError::new(format!(
                                        "Structured JSON could not be encoded for OpenAI: {error}"
                                    ))
                                })?
                                .as_str(),
                        );
                    }
                    ContentItem::Citation { .. } => {
                        return Err(ProtocolConversionError::new(
                            "OpenAI citation projection is not registered for request input.",
                        ));
                    }
                    ContentItem::ToolCall { call } if message.role == MessageRole::Assistant => {
                        tool_calls.push(json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": serde_json::to_string(&call.arguments).map_err(|error| {
                                    ProtocolConversionError::new(format!(
                                        "OpenAI tool arguments could not be encoded: {error}"
                                    ))
                                })?,
                            },
                        }));
                    }
                    ContentItem::Resource { resource }
                        if message.role == MessageRole::User =>
                    {
                        resource_parts.push(encode_chat_resource(resource)?);
                    }
                    _ => {
                        return Err(ProtocolConversionError::new(
                            "OpenAI complex content requires a registered provider projector.",
                        ));
                    }
                }
            }
            let content = if resource_parts.is_empty() {
                Value::String(text)
            } else {
                let mut parts = Vec::with_capacity(resource_parts.len() + 1);
                if !text.is_empty() {
                    parts.push(json!({ "type": "text", "text": text }));
                }
                parts.extend(resource_parts);
                Value::Array(parts)
            };
            let mut encoded = json!({ "role": role, "content": content });
            if !tool_calls.is_empty() {
                encoded["tool_calls"] = Value::Array(tool_calls);
            }
            Ok(encoded)
        })
        .collect()
}

/// Encodes public messages into OpenAI Responses input items.
/// 将公共消息编码为 OpenAI Responses 输入项。
fn encode_responses_input(
    request: &AiExecutionRequest,
    include_system_messages: bool,
) -> Result<Vec<Value>, ProtocolConversionError> {
    let mut input = Vec::new();
    for message in &request.messages {
        match message.role {
            MessageRole::Tool => {
                for item in &message.content {
                    if let ContentItem::ToolResult { result } = item {
                        input.push(json!({
                            "type": "function_call_output",
                            "call_id": result.call_id,
                            "output": encode_json_or_string(&result.content)?,
                        }));
                    } else {
                        return Err(ProtocolConversionError::new(
                            "OpenAI Responses tool messages may only contain tool results.",
                        ));
                    }
                }
            }
            MessageRole::Assistant => {
                let mut text = String::new();
                for item in &message.content {
                    match item {
                        ContentItem::Text { text: item_text }
                        | ContentItem::Refusal { text: item_text } => text.push_str(item_text),
                        ContentItem::StructuredJson { value } => {
                            text.push_str(encode_json_or_string(value)?.as_str());
                        }
                        ContentItem::Reasoning { text, signature } => {
                            let Some(signature) = signature.as_deref().and_then(|signature| {
                                crate::compatible_provider_signature(
                                    crate::SIGNATURE_PROVIDER_OPENAI,
                                    signature,
                                )
                            }) else {
                                continue;
                            };
                            let mut reasoning = json!({
                                "type": "reasoning",
                                "summary": [{
                                    "type": "summary_text",
                                    "text": text,
                                }],
                            });
                            reasoning["encrypted_content"] = Value::String(signature.to_string());
                            input.push(reasoning);
                        }
                        ContentItem::ToolCall { call } => input.push(json!({
                            "type": "function_call",
                            "call_id": call.id,
                            "name": call.name,
                            "arguments": serde_json::to_string(&call.arguments).map_err(|error| {
                                ProtocolConversionError::new(format!(
                                    "OpenAI Responses tool arguments could not be encoded: {error}"
                                ))
                            })?,
                        })),
                        ContentItem::Resource { .. } | ContentItem::Citation { .. } => {
                            return Err(ProtocolConversionError::new(
                                "OpenAI Responses complex assistant content requires a registered provider projector.",
                            ));
                        }
                        ContentItem::ToolResult { .. } => {
                            return Err(ProtocolConversionError::new(
                                "OpenAI Responses assistant messages cannot contain tool results.",
                            ));
                        }
                    }
                }
                if !text.is_empty() {
                    input.push(json!({
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": text}],
                    }));
                }
            }
            MessageRole::System | MessageRole::User => {
                if message.role == MessageRole::System && !include_system_messages {
                    continue;
                }
                let role = if message.role == MessageRole::System {
                    "system"
                } else {
                    "user"
                };
                let mut content = Vec::new();
                for item in &message.content {
                    match item {
                        ContentItem::Text { text } | ContentItem::Refusal { text } => {
                            content.push(json!({
                                "type": "input_text",
                                "text": text,
                            }))
                        }
                        ContentItem::StructuredJson { value } => content.push(json!({
                            "type": "input_text",
                            "text": encode_json_or_string(value)?,
                        })),
                        ContentItem::Resource { resource }
                            if message.role == MessageRole::User
                                && resource.kind == vulcan_ai_contract::ResourceKind::Image =>
                        {
                            content.push(json!({
                                "type": "input_image",
                                "image_url": require_resource_url(resource)?,
                            }))
                        }
                        _ => {
                            return Err(ProtocolConversionError::new(
                                "OpenAI Responses complex input requires a registered provider projector.",
                            ));
                        }
                    }
                }
                input.push(json!({ "role": role, "content": content }));
            }
        }
    }
    Ok(input)
}

/// Encodes Codex system content into its required top-level instructions field.
/// 将 Codex 系统内容编码到其必需的顶层 Instructions 字段。
///
/// # Parameters
/// 参数
/// `request` is the immutable public request whose system messages are separated from input.
/// `request` 是不可变公共请求，其系统消息会与 Input 分离。
///
/// # Returns
/// 返回值
/// Ordered instruction text, including an empty value when no system message exists.
/// 返回有序指令文本；不存在系统消息时返回空值。
fn encode_codex_instructions(
    request: &AiExecutionRequest,
) -> Result<String, ProtocolConversionError> {
    let mut instructions = Vec::new();
    for message in request
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::System)
    {
        for item in &message.content {
            match item {
                ContentItem::Text { text } | ContentItem::Refusal { text } => {
                    instructions.push(text.clone());
                }
                ContentItem::StructuredJson { value } => {
                    instructions.push(encode_json_or_string(value)?);
                }
                _ => {
                    return Err(ProtocolConversionError::new(
                        "OpenAI Codex system instructions support text and structured JSON only.",
                    ));
                }
            }
        }
    }
    Ok(instructions.join("\n"))
}

/// Encodes one validated resource for OpenAI Chat-compatible content.
/// 为 OpenAI Chat 兼容内容编码一个已校验资源。
///
/// # Parameters
/// 参数
/// `resource` contains the request-scoped URL or data URL prepared by the host.
/// `resource` 包含宿主准备的请求作用域 URL 或 Data URL。
///
/// # Returns
/// 返回值
/// Returns the exact registered OpenRouter/OpenAI-compatible content block.
/// 返回精确注册的 OpenRouter/OpenAI 兼容内容块。
fn encode_chat_resource(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<Value, ProtocolConversionError> {
    let url = require_resource_url(resource)?;
    match resource.kind {
        vulcan_ai_contract::ResourceKind::Image => Ok(json!({
            "type": "image_url",
            "image_url": { "url": url },
        })),
        vulcan_ai_contract::ResourceKind::Audio => {
            let (_, data) = parse_data_url(url)?;
            let media_type = resource.media_type.as_deref().ok_or_else(|| {
                ProtocolConversionError::new("OpenAI audio input requires a MIME type.")
            })?;
            Ok(json!({
                "type": "input_audio",
                "input_audio": {
                    "data": data,
                    "format": audio_format_from_media_type(media_type)?,
                },
            }))
        }
        vulcan_ai_contract::ResourceKind::Video => Ok(json!({
            "type": "video_url",
            "video_url": { "url": url },
        })),
        vulcan_ai_contract::ResourceKind::File => {
            let filename = resource
                .local_path
                .as_deref()
                .and_then(|path| std::path::Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or(resource.id.as_str());
            Ok(json!({
                "type": "file",
                "file": {
                    "filename": filename,
                    "file_data": url,
                },
            }))
        }
    }
}

/// Returns the resolved URL of one public resource.
/// 返回一个公共资源的已解析 URL。
///
/// # Parameters
/// 参数
/// `resource` is the resource being projected.
/// `resource` 是正在投影的资源。
///
/// # Returns
/// 返回值
/// Returns the URL or a conversion error when host resource preparation was skipped.
/// 返回 URL；若宿主跳过资源准备则返回转换错误。
fn require_resource_url(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<&str, ProtocolConversionError> {
    resource.url.as_deref().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Resource `{}` was not resolved before protocol projection.",
            resource.id
        ))
    })
}

/// Splits one Base64 data URL into MIME type and raw payload.
/// 将一个 Base64 Data URL 拆分为 MIME 类型与原始载荷。
///
/// # Parameters
/// 参数
/// `url` is the host-generated data URL.
/// `url` 是宿主生成的 Data URL。
///
/// # Returns
/// 返回值
/// Returns the declared MIME type and raw Base64 data.
/// 返回声明的 MIME 类型与原始 Base64 数据。
fn parse_data_url(url: &str) -> Result<(&str, &str), ProtocolConversionError> {
    let body = url.strip_prefix("data:").ok_or_else(|| {
        ProtocolConversionError::new("This resource input requires a Base64 data URL.")
    })?;
    let (metadata, data) = body.split_once(',').ok_or_else(|| {
        ProtocolConversionError::new("The resource data URL is missing its payload separator.")
    })?;
    let media_type = metadata.strip_suffix(";base64").ok_or_else(|| {
        ProtocolConversionError::new("The resource data URL must use Base64 encoding.")
    })?;
    Ok((media_type, data))
}

/// Maps one audio MIME type to the OpenAI-compatible format identifier.
/// 将一个音频 MIME 类型映射为 OpenAI 兼容格式标识。
///
/// # Parameters
/// 参数
/// `media_type` is the validated attachment MIME type.
/// `media_type` 是已校验附件 MIME 类型。
///
/// # Returns
/// 返回值
/// Returns a registered format or a controlled conversion error.
/// 返回已注册格式或受控转换错误。
fn audio_format_from_media_type(media_type: &str) -> Result<&'static str, ProtocolConversionError> {
    match media_type {
        "audio/wav" | "audio/x-wav" => Ok("wav"),
        "audio/mpeg" | "audio/mp3" => Ok("mp3"),
        "audio/aiff" | "audio/x-aiff" => Ok("aiff"),
        "audio/aac" => Ok("aac"),
        "audio/ogg" => Ok("ogg"),
        "audio/flac" => Ok("flac"),
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => Ok("m4a"),
        _ => Err(ProtocolConversionError::new(format!(
            "Audio MIME type `{media_type}` has no registered OpenAI-compatible format."
        ))),
    }
}

/// Encodes a JSON value as provider-safe text.
/// 将 JSON 值编码为供应商安全文本。
fn encode_json_or_string(value: &Value) -> Result<String, ProtocolConversionError> {
    match value {
        Value::String(string) => Ok(string.clone()),
        value => serde_json::to_string(value).map_err(|error| {
            ProtocolConversionError::new(format!(
                "OpenAI JSON content could not be encoded: {error}"
            ))
        }),
    }
}

/// Encodes public tool definitions into OpenAI function tools.
/// 将公共工具定义编码为 OpenAI 函数工具。
fn encode_openai_tools(request: &AiExecutionRequest) -> Vec<Value> {
    request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect()
}

/// Encodes public tool definitions into the flat Responses function-tool shape.
/// 将公共工具定义编码为扁平的 Responses 函数工具结构。
///
/// # Parameters
/// 参数
/// `request` contains the immutable model-visible tool set.
/// `request` 包含不可变的模型可见工具集合。
///
/// # Returns
/// 返回值
/// The exact tool array accepted by OpenAI Responses and Codex Responses.
/// 返回 OpenAI Responses 与 Codex Responses 接受的精确工具数组。
fn encode_responses_tools(request: &AiExecutionRequest) -> Vec<Value> {
    request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
            })
        })
        .collect()
}

/// Encodes Codex function tools plus capability-selected provider-native tools.
/// 编码 Codex 函数工具与根据能力选择的供应商原生工具。
///
/// # Parameters
/// 参数
/// `request` contains the immutable function and native tool selections.
/// `request` 包含不可变的函数工具与原生工具选择。
///
/// # Returns
/// 返回值
/// The exact Codex Responses tool array without duplicate image-generation semantics.
/// 返回不含重复图片生成语义的精确 Codex Responses 工具数组。
fn encode_codex_responses_tools(request: &AiExecutionRequest) -> Vec<Value> {
    let mut tools = encode_responses_tools(request);
    let has_image_function = request.tools.iter().any(|tool| {
        matches!(
            tool.name.as_str(),
            "image_gen.imagegen" | "image_gen" | "imagegen"
        )
    });
    if !has_image_function
        && request
            .native_tools
            .contains(&NativeToolKind::ImageGeneration)
    {
        tools.push(json!({
            "type": "image_generation",
            "output_format": "png",
        }));
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use vulcan_ai_contract::{AiMessage, AiOperation, ToolDefinition};

    /// Builds one request containing system, user, and signed reasoning content.
    /// 构建一个包含系统、用户与已签名推理内容的请求。
    fn codex_request() -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gpt-codex"),
            cache_scope_id: Some(String::from("session-stable")),
            messages: vec![
                AiMessage {
                    role: MessageRole::System,
                    content: vec![ContentItem::Text {
                        text: String::from("Follow the repository rules."),
                    }],
                },
                AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: String::from("Inspect the project."),
                    }],
                },
                AiMessage {
                    role: MessageRole::Assistant,
                    content: vec![ContentItem::Reasoning {
                        text: String::from("Prior reasoning summary"),
                        signature: Some(String::from("openai#encrypted-replay")),
                    }],
                },
            ],
            tools: vec![ToolDefinition {
                name: String::from("lookup"),
                description: String::from("Looks up source code."),
                input_schema: json!({"type": "object"}),
            }],
            native_tools: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(1024),
        }
    }

    /// Verifies Codex separates instructions, keeps a stable cache scope, and replays reasoning.
    /// 验证 Codex 分离 instructions、保持稳定缓存作用域并重放推理内容。
    #[test]
    fn codex_encodes_required_identity_and_continuation_fields() {
        let mut request = codex_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Ultra);
        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("Codex request should encode");
        assert_eq!(encoded.path, "/responses");
        assert_eq!(encoded.body["instructions"], "Follow the repository rules.");
        assert_eq!(encoded.body["prompt_cache_key"], "session-stable");
        assert_eq!(
            encoded.headers.get("session_id").map(String::as_str),
            Some("session-stable")
        );
        assert!(
            encoded.body["input"]
                .as_array()
                .expect("input should be an array")
                .iter()
                .all(|item| item.get("role").and_then(Value::as_str) != Some("system"))
        );
        assert_eq!(encoded.body["input"][1]["type"], "reasoning");
        assert_eq!(
            encoded.body["input"][1]["encrypted_content"],
            "encrypted-replay"
        );
        assert_eq!(encoded.body["parallel_tool_calls"], true);
        assert!(encoded.body.get("max_output_tokens").is_none());
        assert_eq!(encoded.body["tools"][0]["name"], "lookup");
        assert!(encoded.body["tools"][0].get("function").is_none());
        assert_eq!(encoded.body["reasoning"]["effort"], "ultra");
        assert_eq!(encoded.body["reasoning"]["summary"], "auto");
    }

    /// Verifies capability-selected Codex image generation uses a native tool without duplicating host image tools.
    /// 验证根据能力选择的 Codex 图片生成使用原生工具，且不会重复宿主图片工具。
    #[test]
    fn codex_encodes_native_image_generation_without_duplicates() {
        let mut request = codex_request();
        request.native_tools = vec![NativeToolKind::ImageGeneration];
        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("Codex native image tool should encode");
        assert!(
            encoded.body["tools"].as_array().is_some_and(|tools| {
                tools.iter().any(|tool| tool["type"] == "image_generation")
            })
        );

        request.tools.push(ToolDefinition {
            name: String::from("image_gen.imagegen"),
            description: String::from("Generates an image."),
            input_schema: json!({"type": "object"}),
        });
        let deduplicated = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("Codex host image tool should encode");
        assert_eq!(
            deduplicated.body["tools"]
                .as_array()
                .expect("tools should be an array")
                .iter()
                .filter(|tool| tool["type"] == "image_generation")
                .count(),
            0
        );
    }

    /// Verifies native-tool semantics cannot be silently dropped by an unsupported profile.
    /// 验证不受支持的 Profile 无法静默丢弃原生工具语义。
    #[test]
    fn generic_responses_rejects_codex_native_tools() {
        let mut request = codex_request();
        request.native_tools = vec![NativeToolKind::ImageGeneration];
        let error = OpenAiResponsesProfile
            .encode(&request)
            .expect_err("generic Responses must reject unregistered native tools");
        assert!(error.to_string().contains(OPENAI_RESPONSES_PROFILE_ID));
    }

    /// Verifies Chat Completions projects a selected public reasoning effort.
    /// 验证 Chat Completions 会投影选中的公共推理档位。
    #[test]
    fn chat_completions_encodes_reasoning_effort() {
        let mut request = codex_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Minimal);
        let encoded = OpenAiChatProfile
            .encode(&request)
            .expect("Chat request should encode");
        assert_eq!(encoded.body["reasoning_effort"], "minimal");
        assert_eq!(encoded.body["stream_options"]["include_usage"], true);
    }

    /// Verifies Responses drops reasoning history signed by another provider family.
    /// 验证 Responses 会丢弃由其他供应商族签名的推理历史。
    #[test]
    fn responses_drops_cross_provider_reasoning_signature() {
        let mut request = codex_request();
        let ContentItem::Reasoning { signature, .. } = &mut request.messages[2].content[0] else {
            panic!("fixture should contain reasoning");
        };
        *signature = Some(String::from("anthropic#foreign-signature"));
        let encoded = OpenAiResponsesProfile
            .encode(&request)
            .expect("request should encode after dropping incompatible reasoning");
        assert!(
            encoded.body["input"]
                .as_array()
                .expect("input should be an array")
                .iter()
                .all(|item| item["type"] != "reasoning")
        );
    }

    /// Verifies ordinary Responses requests do not inherit Codex-only cache fields or headers.
    /// 验证普通 Responses 请求不会继承 Codex 专属缓存字段或请求头。
    #[test]
    fn standard_responses_omits_codex_cache_identity() {
        let encoded = OpenAiResponsesProfile
            .encode(&codex_request())
            .expect("standard Responses request should encode");
        assert!(encoded.body.get("prompt_cache_key").is_none());
        assert!(!encoded.headers.contains_key("session_id"));
        assert_eq!(encoded.body["input"][0]["role"], "system");
    }
}
