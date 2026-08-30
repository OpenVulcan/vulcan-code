//! OpenAI Chat Completions and Responses wire profiles.
//! OpenAI Chat Completions 与 Responses Wire Profile。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder};
use serde_json::{Value, json};
use std::collections::HashSet;
use vulcan_ai_contract::{
    APPLY_PATCH_TOOL_NAME, AiExecutionRequest, ContentItem, MessageRole, NativeToolKind,
    ParallelToolCallPolicy, PromptCachePolicy, ReasoningOutputPolicy, RequestUsage,
    StructuredOutputPolicy, ToolChoicePolicy, is_replayable_codex_responses_item,
};

/// Wraps one northbound OpenAI reasoning payload with explicit replay provenance.
/// 使用显式重放来源封装一项北向 OpenAI 推理载荷。
///
/// # Parameters
/// 参数
/// `signature` is the opaque `encrypted_content` value received from a standard Responses client.
/// `signature` 是从标准 Responses 客户端收到的不透明 `encrypted_content` 值。
///
/// # Returns
/// 返回值
/// A replay-safe signature, or `None` when the value violates the closed provenance format.
/// 返回可安全重放的签名；值违反封闭来源格式时返回 `None`。
pub fn wrap_openai_reasoning_signature(signature: &str) -> Option<String> {
    crate::wrap_provider_signature(crate::SIGNATURE_PROVIDER_OPENAI, signature)
}

/// Stable OpenAI Chat Completions profile identifier.
/// 稳定的 OpenAI Chat Completions Profile 标识。
pub const OPENAI_CHAT_PROFILE_ID: &str = "openai-chat-completions.v1";
/// Stable OpenAI Responses profile identifier.
/// 稳定的 OpenAI Responses Profile 标识。
pub const OPENAI_RESPONSES_PROFILE_ID: &str = "openai-responses.v1";
/// Stable OpenAI Codex subscription Responses profile identifier.
/// 稳定的 OpenAI Codex 订阅 Responses Profile 标识。
pub const OPENAI_CODEX_RESPONSES_PROFILE_ID: &str = "openai-codex-responses.v1";

/// Official Codex client originator used by the adopted Responses transport.
/// 已采纳 Responses 传输所使用的官方 Codex 客户端来源标识。
const CODEX_RESPONSES_ORIGINATOR: &str = "codex_cli_rs";
/// Frozen user agent copied exactly from the adopted upstream Codex client.
/// 从已采纳的上游 Codex 客户端精确复制的冻结用户代理。
pub const CODEX_RESPONSES_USER_AGENT: &str = "codex_cli_rs/0.144.1 (Windows 11; x86_64)";

/// Normalizes an OpenAI-compatible usage object through the same projector used by streaming responses.
/// 通过与流式响应相同的投影器规范化 OpenAI 兼容用量对象。
///
/// # Parameters
/// 参数
/// `value` is the provider-reported `usage` JSON object.
/// `value` 是供应商报告的 `usage` JSON 对象。
///
/// # Returns
/// 返回值
/// A normalized usage snapshot that preserves unknown fields as unknown values.
/// 返回一份规范化用量快照，并将未报告字段保留为未知值。
pub fn normalize_openai_usage(value: &Value) -> RequestUsage {
    crate::openai_stream::openai_usage(value)
}

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

    /// Returns the bounded Chat Completions SSE transport declaration.
    /// 返回有界 Chat Completions SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
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
        let tools = encode_openai_tools(request);
        let mut body = json!({
            "model": request.model_id,
            "messages": messages,
            "stream": true,
            "stream_options": {
                "include_usage": true,
            },
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if let Some(limit) = request.output_token_limit {
            body["max_completion_tokens"] = json!(limit);
        }
        if let Some(effort) = request.reasoning_effort {
            body["reasoning_effort"] = Value::String(effort.as_str().to_string());
        }
        project_chat_request_controls(request, &mut body)?;
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

    /// Returns the bounded Responses SSE transport declaration.
    /// 返回有界 Responses SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one public request as Responses JSON.
    /// 将一个公共请求编码为 Responses JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        require_default_cache_policy(&request.prompt_cache, self.id())?;
        // Standard Responses keeps store=false; therefore it replays complete public history instead of referencing server-retained state.
        // 标准 Responses 保持 store=false；因此重放完整公共历史，而不是引用服务端保留状态。
        let input = encode_responses_input(request, true, None)?;
        let tools = encode_responses_tools(request);
        let mut body = json!({
            "model": request.model_id,
            "input": input,
            "stream": true,
            "store": false,
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if let Some(limit) = request.output_token_limit {
            body["max_output_tokens"] = json!(limit);
        }
        if let Some(effort) = request.reasoning_effort {
            body["reasoning"] = json!({
                "effort": effort.as_str(),
            });
            project_responses_reasoning_output(request.reasoning_output, &mut body["reasoning"])?;
            body["include"] = json!(["reasoning.encrypted_content"]);
        }
        project_responses_request_controls(request, &mut body)?;
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

    /// Returns the bounded Codex Responses SSE transport declaration.
    /// 返回有界 Codex Responses SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one public request using the isolated Codex request identity.
    /// 使用隔离的 Codex 请求身份编码一个公共请求。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        require_default_cache_policy(&request.prompt_cache, self.id())?;
        let continuation = validated_responses_continuation(request, self.id())?;
        let input = encode_responses_input(request, false, continuation)?;
        let instructions = encode_codex_instructions(request)?;
        let tools = encode_codex_responses_tools(request);
        let mut body = json!({
            "model": request.model_id,
            "instructions": instructions,
            "input": input,
            "tool_choice": "auto",
            "parallel_tool_calls": !matches!(
                request.parallel_tool_calls,
                ParallelToolCallPolicy::Disabled
            ),
            "stream": true,
            "store": false,
            "include": ["reasoning.encrypted_content"],
        });
        if body["instructions"].as_str().is_some_and(str::is_empty) {
            body.as_object_mut()
                .expect("Codex Responses request body is an object")
                .remove("instructions");
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if let Some(continuation) = continuation {
            body["previous_response_id"] = Value::String(continuation.provider_message_id.clone());
        }
        if let Some(effort) = request.reasoning_effort {
            body["reasoning"] = json!({
                "effort": effort.as_str(),
            });
            project_responses_reasoning_output(request.reasoning_output, &mut body["reasoning"])?;
        }
        project_responses_request_controls(request, &mut body)?;
        let mut headers = std::collections::BTreeMap::from([
            (
                String::from("originator"),
                String::from(CODEX_RESPONSES_ORIGINATOR),
            ),
            (
                String::from("user-agent"),
                String::from(CODEX_RESPONSES_USER_AGENT),
            ),
        ]);
        if let Some(cache_scope_id) = request.cache_scope_id.as_deref() {
            body["prompt_cache_key"] = Value::String(cache_scope_id.to_string());
            body["client_metadata"] = json!({
                "session_id": cache_scope_id,
                "thread_id": cache_scope_id,
            });
            headers.insert(
                String::from("x-client-request-id"),
                cache_scope_id.to_string(),
            );
            headers.insert(String::from("session-id"), cache_scope_id.to_string());
            // Root Vulcan task sessions map to both official Codex session and thread identities.
            // Vulcan 根任务会话同时映射为官方 Codex 的会话身份与线程身份。
            headers.insert(String::from("thread-id"), cache_scope_id.to_string());
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

/// Projects public request controls into the OpenAI Chat Completions wire shape.
/// 将公共请求控制投影到 OpenAI Chat Completions Wire 结构。
///
/// # Parameters
/// 参数
/// `request` is the provider-neutral request and `body` is the mutable Chat request object.
/// `request` 是供应商无关请求，`body` 是可变 Chat 请求对象。
///
/// # Returns
/// 返回值
/// A controlled conversion error when a requested control cannot be represented exactly.
/// 当请求控制无法精确表示时返回受控转换错误。
fn project_chat_request_controls(
    request: &AiExecutionRequest,
    body: &mut Value,
) -> Result<(), ProtocolConversionError> {
    if !matches!(request.tool_choice, ToolChoicePolicy::Auto) {
        body["tool_choice"] = encode_chat_tool_choice(request)?;
    }
    match request.parallel_tool_calls {
        ParallelToolCallPolicy::ProviderDefault => {}
        ParallelToolCallPolicy::Allowed => body["parallel_tool_calls"] = Value::Bool(true),
        ParallelToolCallPolicy::Disabled => body["parallel_tool_calls"] = Value::Bool(false),
    }
    match &request.structured_output {
        StructuredOutputPolicy::None => {}
        StructuredOutputPolicy::JsonObject => {
            body["response_format"] = json!({ "type": "json_object" });
        }
        StructuredOutputPolicy::JsonSchema {
            name,
            description,
            schema,
            strict,
        } => {
            let mut json_schema = json!({
                "name": name,
                "schema": schema,
                "strict": strict,
            });
            if let Some(description) = description {
                json_schema["description"] = Value::String(description.clone());
            }
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": json_schema,
            });
        }
    }
    if !request.stop_sequences.is_empty() {
        body["stop"] = json!(request.stop_sequences);
    }
    reject_chat_reasoning_output_without_reasoning(request)?;
    Ok(())
}

/// Projects public request controls into the OpenAI Responses wire shape.
/// 将公共请求控制投影到 OpenAI Responses Wire 结构。
///
/// # Parameters
/// 参数
/// `request` is the provider-neutral request and `body` is the mutable Responses request object.
/// `request` 是供应商无关请求，`body` 是可变 Responses 请求对象。
///
/// # Returns
/// 返回值
/// A controlled conversion error when Responses has no exact representation.
/// 当 Responses 没有精确表示时返回受控转换错误。
fn project_responses_request_controls(
    request: &AiExecutionRequest,
    body: &mut Value,
) -> Result<(), ProtocolConversionError> {
    if !matches!(request.tool_choice, ToolChoicePolicy::Auto) {
        body["tool_choice"] = encode_responses_tool_choice(request)?;
    }
    match request.parallel_tool_calls {
        ParallelToolCallPolicy::ProviderDefault => {}
        ParallelToolCallPolicy::Allowed => body["parallel_tool_calls"] = Value::Bool(true),
        ParallelToolCallPolicy::Disabled => body["parallel_tool_calls"] = Value::Bool(false),
    }
    match &request.structured_output {
        StructuredOutputPolicy::None => {}
        StructuredOutputPolicy::JsonObject => {
            body["text"] = json!({ "format": { "type": "json_object" } });
        }
        StructuredOutputPolicy::JsonSchema {
            name,
            description,
            schema,
            strict,
        } => {
            let mut format = json!({
                "type": "json_schema",
                "name": name,
                "schema": schema,
                "strict": strict,
            });
            if let Some(description) = description {
                format["description"] = Value::String(description.clone());
            }
            body["text"] = json!({ "format": format });
        }
    }
    if !request.stop_sequences.is_empty() {
        return Err(ProtocolConversionError::new(
            "OpenAI Responses has no registered stop-sequence projector.",
        ));
    }
    reject_chat_reasoning_output_without_reasoning(request)?;
    Ok(())
}

/// Encodes one Chat Completions tool-choice policy after validating exact tool ownership.
/// 在验证精确工具归属后编码 Chat Completions 工具选择策略。
///
/// # Parameters
/// 参数
/// `request` contains the selected policy and registered public tools.
/// `request` 包含所选策略与已注册公共工具。
///
/// # Returns
/// 返回值
/// The exact Chat Completions `tool_choice` value.
/// 精确的 Chat Completions `tool_choice` 值。
fn encode_chat_tool_choice(request: &AiExecutionRequest) -> Result<Value, ProtocolConversionError> {
    Ok(match &request.tool_choice {
        ToolChoicePolicy::Auto => Value::String(String::from("auto")),
        ToolChoicePolicy::Required => Value::String(String::from("required")),
        ToolChoicePolicy::Disabled => Value::String(String::from("none")),
        ToolChoicePolicy::Specific { tool_name } => {
            require_registered_tool(request, tool_name)?;
            json!({ "type": "function", "function": { "name": tool_name } })
        }
    })
}

/// Encodes one Responses tool-choice policy after validating exact tool ownership.
/// 在验证精确工具归属后编码 Responses 工具选择策略。
///
/// # Parameters
/// 参数
/// `request` contains the selected policy and registered public tools.
/// `request` 包含所选策略与已注册公共工具。
///
/// # Returns
/// 返回值
/// The exact Responses `tool_choice` value.
/// 精确的 Responses `tool_choice` 值。
fn encode_responses_tool_choice(
    request: &AiExecutionRequest,
) -> Result<Value, ProtocolConversionError> {
    Ok(match &request.tool_choice {
        ToolChoicePolicy::Auto => Value::String(String::from("auto")),
        ToolChoicePolicy::Required => Value::String(String::from("required")),
        ToolChoicePolicy::Disabled => Value::String(String::from("none")),
        ToolChoicePolicy::Specific { tool_name } => {
            require_registered_tool(request, tool_name)?;
            json!({ "type": "function", "name": tool_name })
        }
    })
}

/// Verifies a specific tool-choice target exists in the exact request tool set.
/// 验证特定工具选择目标存在于精确请求工具集合中。
///
/// # Parameters
/// 参数
/// `request` owns the registered tools and `tool_name` is the requested exact name.
/// `request` 拥有已注册工具，`tool_name` 是请求的精确名称。
///
/// # Returns
/// 返回值
/// Success only when the tool is registered.
/// 仅当工具已注册时成功。
fn require_registered_tool(
    request: &AiExecutionRequest,
    tool_name: &str,
) -> Result<(), ProtocolConversionError> {
    if request.tools.iter().any(|tool| tool.name == tool_name) {
        return Ok(());
    }
    Err(ProtocolConversionError::new(format!(
        "Tool choice references unregistered tool `{tool_name}`."
    )))
}

/// Projects the public reasoning-output policy into an existing Responses reasoning object.
/// 将公共思考输出策略投影到现有 Responses reasoning 对象。
///
/// # Parameters
/// 参数
/// `policy` is the requested visibility and `reasoning` is the mutable wire object.
/// `policy` 是请求的可见性，`reasoning` 是可变 Wire 对象。
///
/// # Returns
/// 返回值
/// Success after an exact summary projection.
/// 精确投影摘要后返回成功。
fn project_responses_reasoning_output(
    policy: ReasoningOutputPolicy,
    reasoning: &mut Value,
) -> Result<(), ProtocolConversionError> {
    match policy {
        ReasoningOutputPolicy::ProviderDefault | ReasoningOutputPolicy::SummaryAuto => {
            reasoning["summary"] = Value::String(String::from("auto"));
        }
        ReasoningOutputPolicy::Hidden => {}
        ReasoningOutputPolicy::Detailed => {
            reasoning["summary"] = Value::String(String::from("detailed"));
        }
    }
    Ok(())
}

/// Rejects explicit reasoning-output controls when no reasoning effort is active.
/// 当未启用思考强度时拒绝显式思考输出控制。
///
/// # Parameters
/// 参数
/// `request` is the exact public request being encoded.
/// `request` 是正在编码的精确公共请求。
///
/// # Returns
/// 返回值
/// Success for provider default or an active reasoning request.
/// 供应商默认或已启用思考请求时返回成功。
fn reject_chat_reasoning_output_without_reasoning(
    request: &AiExecutionRequest,
) -> Result<(), ProtocolConversionError> {
    if request.reasoning_effort.is_none()
        && !matches!(
            request.reasoning_output,
            ReasoningOutputPolicy::ProviderDefault
        )
    {
        return Err(ProtocolConversionError::new(
            "Reasoning output control requires an active reasoning effort.",
        ));
    }
    Ok(())
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
    crate::require_no_route_bound_provider_context(request, "openai.chat_completions")?;
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
                // Chat compatibility uses system for both instruction roles; canonical history stays unchanged.
                // Chat 兼容层将两类指令角色都投影为 system；规范历史保持不变。
                MessageRole::System | MessageRole::Developer => "system",
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
    continuation: Option<&vulcan_ai_contract::AiContinuation>,
) -> Result<Vec<Value>, ProtocolConversionError> {
    let mut input = Vec::new();
    let message_start = continuation
        .map(|continuation| continuation.covered_message_count)
        .unwrap_or(0);
    // Legacy Apply Patch call IDs bind exact raw outputs to the standard function replay contract.
    // 旧版 Apply Patch 调用 ID 将精确原始输出绑定到标准函数重放契约。
    let legacy_patch_call_ids: HashSet<&str> = request
        .messages
        .iter()
        .flat_map(|message| &message.provider_context_items)
        .filter(|item| item["type"] == "custom_tool_call" && item["name"] == APPLY_PATCH_TOOL_NAME)
        .filter_map(|item| item.get("call_id").and_then(Value::as_str))
        .collect();
    // Unrelated custom calls stay exact; Apply Patch keeps custom output only when the cursor owns its original call.
    // 无关 custom 调用保持原样；仅当游标持有原调用时，Apply Patch 才保留 custom 输出配对。
    let mut custom_tool_call_ids = responses_custom_tool_call_ids(&request.messages);
    custom_tool_call_ids.retain(|call_id| !legacy_patch_call_ids.contains(call_id));
    custom_tool_call_ids.extend(responses_custom_tool_call_ids(
        &request.messages[..message_start],
    ));
    for message in &request.messages[message_start..] {
        match message.role {
            MessageRole::Tool => {
                for item in &message.content {
                    if let ContentItem::ToolResult { result } = item {
                        let output_type = if custom_tool_call_ids.contains(result.call_id.as_str())
                        {
                            "custom_tool_call_output"
                        } else {
                            "function_call_output"
                        };
                        input.push(json!({
                            "type": output_type,
                            "call_id": result.call_id,
                            "output": encode_responses_tool_output(&result.content)?,
                        }));
                    } else {
                        return Err(ProtocolConversionError::new(
                            "OpenAI Responses tool messages may only contain tool results.",
                        ));
                    }
                }
            }
            MessageRole::Assistant => {
                for provider_context_item in &message.provider_context_items {
                    if !is_replayable_codex_responses_item(provider_context_item) {
                        return Err(ProtocolConversionError::new(
                            "OpenAI Responses provider context items must be replayable Codex Responses items.",
                        ));
                    }
                    input.push(encode_responses_replay_item(
                        provider_context_item,
                        &legacy_patch_call_ids,
                        &custom_tool_call_ids,
                    )?);
                }
                let mut text = String::new();
                for item in &message.content {
                    match item {
                        ContentItem::Text { text: item_text }
                        | ContentItem::Refusal { text: item_text } => text.push_str(item_text),
                        ContentItem::StructuredJson { value } => {
                            text.push_str(encode_json_or_string(value)?.as_str());
                        }
                        ContentItem::Reasoning {
                            text: reasoning_text,
                            signature,
                        } => {
                            push_responses_assistant_text(&mut input, &mut text);
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
                                    "text": reasoning_text,
                                }],
                            });
                            reasoning["encrypted_content"] = Value::String(signature.to_string());
                            input.push(reasoning);
                        }
                        ContentItem::ToolCall { call } => {
                            // A text item before a function call is a distinct ordered Responses item.
                            // 工具调用前的文本是一个独立且有序的 Responses 项。
                            push_responses_assistant_text(&mut input, &mut text);
                            input.push(json!({
                                "type": "function_call",
                                "call_id": call.id,
                                "name": call.name,
                                "arguments": serde_json::to_string(&call.arguments).map_err(|error| {
                                    ProtocolConversionError::new(format!(
                                        "OpenAI Responses tool arguments could not be encoded: {error}"
                                    ))
                                })?,
                            }));
                        }
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
                push_responses_assistant_text(&mut input, &mut text);
            }
            MessageRole::System | MessageRole::Developer | MessageRole::User => {
                if message.role == MessageRole::System && !include_system_messages {
                    continue;
                }
                let role = match message.role {
                    MessageRole::System => "system",
                    MessageRole::Developer => "developer",
                    MessageRole::User => "user",
                    MessageRole::Assistant | MessageRole::Tool => unreachable!(),
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
                        ContentItem::Resource { resource } if message.role == MessageRole::User => {
                            content.push(encode_responses_resource(resource)?)
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

/// Collects exact custom call IDs from raw provider replay items, never from normalized function calls.
/// 从原始供应商重放项收集精确 custom 调用 ID，不将规范函数调用视为 custom。
/// `messages` is the selected history window; returned IDs preserve original call/output pairing where required.
/// `messages` 是选定的历史窗口；返回 ID 用于在必要时保持原始调用与输出的配对。
fn responses_custom_tool_call_ids(messages: &[vulcan_ai_contract::AiMessage]) -> HashSet<&str> {
    let mut call_ids = HashSet::new();
    for message in messages {
        for item in &message.provider_context_items {
            if item.get("type").and_then(Value::as_str) == Some("custom_tool_call")
                && let Some(call_id) = item.get("call_id").and_then(Value::as_str)
            {
                call_ids.insert(call_id);
            }
        }
    }
    call_ids
}

/// Projects historical Apply Patch custom items to ordinary function items without editing stored history.
/// 将历史 Apply Patch custom 项投影为普通函数项，不修改持久化历史。
/// `item` is validated replay data; `legacy_patch_call_ids` binds outputs and `custom_output_call_ids` identifies calls requiring exact custom outputs.
/// `item` 是已校验重放数据；`legacy_patch_call_ids` 绑定输出，`custom_output_call_ids` 标识必须保留精确 custom 输出的调用。
/// Returns the standard replay item, or an explicit error if a legacy call lacks its required string fields.
/// 返回标准重放项；旧调用缺少必填字符串字段时返回明确错误。
fn encode_responses_replay_item(
    item: &Value,
    legacy_patch_call_ids: &HashSet<&str>,
    custom_output_call_ids: &HashSet<&str>,
) -> Result<Value, ProtocolConversionError> {
    if item["type"] == "custom_tool_call" && item["name"] == APPLY_PATCH_TOOL_NAME {
        // New wire items retain semantic call identity, not the old custom item ID or grammar encoding.
        // 新线路项保留语义调用身份，不沿用旧 custom 项 ID 或语法编码。
        let call_id = item.get("call_id").and_then(Value::as_str).ok_or_else(|| {
            ProtocolConversionError::new("Legacy Apply Patch call requires a string call_id.")
        })?;
        let patch = item.get("input").and_then(Value::as_str).ok_or_else(|| {
            ProtocolConversionError::new("Legacy Apply Patch call requires a string input.")
        })?;
        return Ok(json!({
            "type": "function_call",
            "call_id": call_id,
            "name": APPLY_PATCH_TOOL_NAME,
            "arguments": json!({"patch": patch}).to_string(),
        }));
    }
    // All unrelated provider items remain exact, including reasoning and provider-native tools.
    // 所有无关供应商项保持原样，包括推理和供应商原生工具。
    let mut replay_item = item.clone();
    if item["type"] == "custom_tool_call_output"
        && let Some(call_id) = item.get("call_id").and_then(Value::as_str)
        && legacy_patch_call_ids.contains(call_id)
        && !custom_output_call_ids.contains(call_id)
    {
        replay_item["type"] = json!("function_call_output");
    }
    Ok(replay_item)
}

/// Flushes one accumulated assistant text segment without crossing an ordered Responses item.
/// 刷新一段已累积的助手文本，且绝不跨越有序 Responses 项。
///
/// # Parameters
/// 参数
/// - `input`: Ordered Responses input items receiving the assistant message.
/// - `input`：接收助手消息的有序 Responses 输入项。
/// - `text`: Mutable text segment consumed when non-empty.
/// - `text`：非空时会被消费的可变文本片段。
///
/// # Returns
/// 返回值
/// This function has no return value and appends at most one assistant message.
/// 此函数没有返回值，最多追加一条助手消息。
fn push_responses_assistant_text(input: &mut Vec<Value>, text: &mut String) {
    if text.is_empty() {
        return;
    }
    input.push(json!({
        "role": "assistant",
        "content": [{"type": "output_text", "text": std::mem::take(text)}],
    }));
}

/// Encodes one standard Responses image or file input without losing its source-field semantics.
/// 编码一项标准 Responses 图片或文件输入，且不丢失其来源字段语义。
fn encode_responses_resource(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<Value, ProtocolConversionError> {
    use vulcan_ai_contract::{ResourceKind, ResourceSourceKind};
    match resource.kind {
        ResourceKind::Image => {
            let mut part = match resource.source_kind {
                ResourceSourceKind::ProviderFileId => json!({
                    "type": "input_image",
                    "file_id": resource.id,
                }),
                ResourceSourceKind::HostResolved
                | ResourceSourceKind::RemoteUrl
                | ResourceSourceKind::InlineData => json!({
                    "type": "input_image",
                    "image_url": require_resource_url(resource)?,
                }),
            };
            if let Some(detail) = resource.image_detail {
                part["detail"] = serde_json::to_value(detail).map_err(|error| {
                    ProtocolConversionError::new(format!(
                        "OpenAI Responses image detail could not be encoded: {error}"
                    ))
                })?;
            }
            Ok(part)
        }
        ResourceKind::File => match resource.source_kind {
            ResourceSourceKind::ProviderFileId => Ok(json!({
                "type": "input_file",
                "file_id": resource.id,
            })),
            ResourceSourceKind::RemoteUrl => Ok(json!({
                "type": "input_file",
                "file_url": require_resource_url(resource)?,
            })),
            ResourceSourceKind::InlineData => {
                let file_name = resource.file_name.as_deref().ok_or_else(|| {
                    ProtocolConversionError::new(format!(
                        "Inline file resource `{}` is missing its required filename.",
                        resource.id
                    ))
                })?;
                Ok(json!({
                    "type": "input_file",
                    "file_data": require_resource_url(resource)?,
                    "filename": file_name,
                }))
            }
            ResourceSourceKind::HostResolved => {
                let url = require_resource_url(resource)?;
                if url.starts_with("data:") {
                    let file_name = resource.file_name.as_deref().ok_or_else(|| {
                        ProtocolConversionError::new(format!(
                            "Host-resolved inline file resource `{}` is missing its filename.",
                            resource.id
                        ))
                    })?;
                    Ok(json!({
                        "type": "input_file",
                        "file_data": url,
                        "filename": file_name,
                    }))
                } else {
                    Ok(json!({
                        "type": "input_file",
                        "file_url": url,
                    }))
                }
            }
        },
        ResourceKind::Audio | ResourceKind::Video => Err(ProtocolConversionError::new(format!(
            "OpenAI Responses has no registered {:?} input projector.",
            resource.kind
        ))),
    }
}

/// Validates one Responses continuation against the immutable request and protocol profile.
/// 根据不可变请求和协议配置校验一个 Responses 延续信息。
///
/// # Parameters
/// 参数
/// `request` carries the optional proven cursor and `profile_id` is the exact active profile.
/// `request` 携带可选的已证明游标，`profile_id` 是精确的当前协议配置。
///
/// # Returns
/// 返回值
/// The validated cursor, or a conversion error when a stale or malformed cursor is present.
/// 返回已校验游标；存在过期或格式错误游标时返回转换错误。
fn validated_responses_continuation<'a>(
    request: &'a AiExecutionRequest,
    profile_id: &str,
) -> Result<Option<&'a vulcan_ai_contract::AiContinuation>, ProtocolConversionError> {
    let Some(continuation) = request.continuation.as_ref() else {
        return Ok(None);
    };
    if continuation.protocol_profile_id != profile_id {
        return Err(ProtocolConversionError::new(format!(
            "Responses continuation profile `{}` does not match active profile `{profile_id}`.",
            continuation.protocol_profile_id
        )));
    }
    if continuation.model_id != request.model_id {
        return Err(ProtocolConversionError::new(format!(
            "Responses continuation model `{}` does not match request model `{}`.",
            continuation.model_id, request.model_id
        )));
    }
    if continuation.provider_message_id.trim().is_empty()
        || continuation.covered_local_message_id.trim().is_empty()
    {
        return Err(ProtocolConversionError::new(
            "Responses continuation requires non-empty provider and local message identifiers.",
        ));
    }
    if continuation.covered_message_count > request.messages.len() {
        return Err(ProtocolConversionError::new(format!(
            "Responses continuation covers {} messages but the request contains only {}.",
            continuation.covered_message_count,
            request.messages.len()
        )));
    }
    Ok(Some(continuation))
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
    match resource.kind {
        vulcan_ai_contract::ResourceKind::Image => {
            if resource.source_kind == vulcan_ai_contract::ResourceSourceKind::ProviderFileId {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Chat-compatible image input cannot represent provider file id `{}`.",
                    resource.id
                )));
            }
            let mut image_url = json!({ "url": require_resource_url(resource)? });
            if let Some(detail) = resource.image_detail {
                image_url["detail"] = serde_json::to_value(detail).map_err(|error| {
                    ProtocolConversionError::new(format!(
                        "OpenAI Chat image detail could not be encoded: {error}"
                    ))
                })?;
            }
            Ok(json!({
                "type": "image_url",
                "image_url": image_url,
            }))
        }
        vulcan_ai_contract::ResourceKind::Audio => {
            let url = require_resource_url(resource)?;
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
        vulcan_ai_contract::ResourceKind::Video => {
            let url = require_resource_url(resource)?;
            Ok(json!({
                "type": "video_url",
                "video_url": { "url": url },
            }))
        }
        vulcan_ai_contract::ResourceKind::File => match resource.source_kind {
            vulcan_ai_contract::ResourceSourceKind::ProviderFileId => Ok(json!({
                "type": "file",
                "file": { "file_id": resource.id },
            })),
            vulcan_ai_contract::ResourceSourceKind::InlineData
            | vulcan_ai_contract::ResourceSourceKind::HostResolved
                if require_resource_url(resource)?.starts_with("data:") =>
            {
                let filename = resource
                    .file_name
                    .as_deref()
                    .or_else(|| {
                        resource
                            .local_path
                            .as_deref()
                            .and_then(|path| std::path::Path::new(path).file_name())
                            .and_then(|name| name.to_str())
                    })
                    .ok_or_else(|| {
                        ProtocolConversionError::new(format!(
                            "Inline Chat file resource `{}` is missing its filename.",
                            resource.id
                        ))
                    })?;
                Ok(json!({
                    "type": "file",
                    "file": {
                        "filename": filename,
                        "file_data": require_resource_url(resource)?,
                    },
                }))
            }
            vulcan_ai_contract::ResourceSourceKind::RemoteUrl
            | vulcan_ai_contract::ResourceSourceKind::InlineData
            | vulcan_ai_contract::ResourceSourceKind::HostResolved => {
                Err(ProtocolConversionError::new(format!(
                    "OpenAI Chat-compatible file input cannot represent remote URL resource `{}`.",
                    resource.id
                )))
            }
        },
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

/// Encodes a standard Responses function output without flattening supported rich content arrays.
/// 编码标准 Responses 函数输出，且不扁平化受支持的富内容数组。
///
/// # Parameters
/// 参数
/// `value` is the provider-neutral tool result payload.
/// `value` 是供应商无关的工具结果载荷。
///
/// # Returns
/// 返回值
/// Strings and content arrays retain their wire shape; other JSON values use deterministic text.
/// 字符串与内容数组保留其 Wire 形态；其他 JSON 值使用确定性文本。
fn encode_responses_tool_output(value: &Value) -> Result<Value, ProtocolConversionError> {
    match value {
        Value::String(_) | Value::Array(_) => Ok(value.clone()),
        value => encode_json_or_string(value).map(Value::String),
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
                    "strict": tool.strict,
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
    let mut tools = request
        .tools
        .iter()
        .map(|tool| {
            // Host tools share the declared JSON schema on every protocol, including Apply Patch.
            // 宿主工具在所有协议上共用声明的 JSON Schema，包括 Apply Patch。
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
                "strict": tool.strict,
            })
        })
        .collect::<Vec<_>>();
    for native_tool in &request.native_tools {
        tools.push(encode_openai_native_tool(*native_tool));
    }
    tools
}

/// Encodes one provider-neutral native tool into its exact OpenAI Responses shape.
/// 将一个供应商无关原生工具编码为精确的 OpenAI Responses 结构。
///
/// # Parameters
/// 参数
/// `native_tool` is the closed native capability selected for this request.
/// `native_tool` 是本次请求选中的封闭原生能力。
///
/// # Returns
/// 返回值
/// The exact hosted-tool object accepted by the Responses API.
/// 返回 Responses API 接受的精确托管工具对象。
fn encode_openai_native_tool(native_tool: NativeToolKind) -> Value {
    match native_tool {
        NativeToolKind::WebSearch => json!({
            "type": "web_search",
        }),
        NativeToolKind::CodeInterpreter => json!({
            "type": "code_interpreter",
            "container": {
                "type": "auto",
            },
        }),
        NativeToolKind::ImageGeneration => json!({
            "type": "image_generation",
            "output_format": "png",
        }),
    }
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
    let mut request_without_native_tools = request.clone();
    request_without_native_tools.native_tools.clear();
    let mut tools = encode_responses_tools(&request_without_native_tools);
    let has_image_function = request.tools.iter().any(|tool| {
        matches!(
            tool.name.as_str(),
            "image_gen.imagegen" | "image_gen" | "imagegen"
        )
    });
    for native_tool in &request.native_tools {
        if matches!(native_tool, NativeToolKind::ImageGeneration) && has_image_function {
            continue;
        }
        tools.push(encode_openai_native_tool(*native_tool));
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use vulcan_ai_contract::{
        AiMessage, AiOperation, ResourceKind, ResourceReference, ToolCall, ToolDefinition,
        ToolResult,
    };

    /// Builds one request containing system, user, and signed reasoning content.
    /// 构建一个包含系统、用户与已签名推理内容的请求。
    fn codex_request() -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gpt-codex"),
            cache_scope_id: Some(String::from("session-stable")),
            continuation: None,
            messages: vec![
                AiMessage {
                    role: MessageRole::System,
                    content: vec![ContentItem::Text {
                        text: String::from("Follow the repository rules."),
                    }],
                    provider_context_items: Vec::new(),
                },
                AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: String::from("Inspect the project."),
                    }],
                    provider_context_items: Vec::new(),
                },
                AiMessage {
                    role: MessageRole::Assistant,
                    content: vec![ContentItem::Reasoning {
                        text: String::from("Prior reasoning summary"),
                        signature: Some(String::from("openai#encrypted-replay")),
                    }],
                    provider_context_items: Vec::new(),
                },
            ],
            tools: vec![ToolDefinition {
                name: String::from("lookup"),
                description: String::from("Looks up source code."),
                input_schema: json!({"type": "object"}),
                strict: false,
            }],
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
        assert_eq!(encoded.body["tool_choice"], "auto");
        assert_eq!(encoded.body["parallel_tool_calls"], true);
        assert_eq!(
            encoded.body["include"],
            json!(["reasoning.encrypted_content"])
        );
        assert_eq!(
            encoded.body["client_metadata"],
            json!({
                "session_id": "session-stable",
                "thread_id": "session-stable",
            })
        );
        assert_eq!(
            encoded.headers.get("session-id").map(String::as_str),
            Some("session-stable")
        );
        assert_eq!(
            encoded.headers.get("thread-id").map(String::as_str),
            Some("session-stable")
        );
        assert_eq!(
            encoded
                .headers
                .get("x-client-request-id")
                .map(String::as_str),
            Some("session-stable")
        );
        assert_eq!(
            encoded.headers.get("originator").map(String::as_str),
            Some("codex_cli_rs")
        );
        assert_eq!(
            encoded.headers.get("user-agent").map(String::as_str),
            Some("codex_cli_rs/0.144.1 (Windows 11; x86_64)")
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
        assert_eq!(
            encoded.body["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    /// Encodes one Codex Responses image as the official input_image content shape.
    /// 将一张 Codex Responses 图片编码为官方 input_image 内容结构。
    #[test]
    fn codex_responses_encodes_user_image_input() {
        let mut request = codex_request();
        request.messages[1].content.push(ContentItem::Resource {
            resource: ResourceReference {
                id: String::from("image-1"),
                kind: ResourceKind::Image,
                media_type: Some(String::from("image/png")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::InlineData,
                file_name: None,
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(String::from("data:image/png;base64,AA==")),
            },
        });

        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("Codex Responses image input should encode");

        assert_eq!(
            encoded.body["input"][0]["content"][1],
            json!({
                "type": "input_image",
                "image_url": "data:image/png;base64,AA==",
            })
        );
    }

    /// Verifies Codex Responses preserves developer messages as developer input items.
    /// 验证 Codex Responses 将开发者消息保留为 developer 输入项。
    #[test]
    fn codex_responses_preserves_developer_role() {
        let mut request = codex_request();
        request.messages.insert(
            1,
            AiMessage {
                role: MessageRole::Developer,
                content: vec![ContentItem::Text {
                    text: String::from("follow developer policy"),
                }],
                provider_context_items: Vec::new(),
            },
        );

        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("developer input should encode");

        assert!(encoded.body["input"].as_array().is_some_and(|input| {
            input.iter().any(|item| {
                item["role"] == "developer"
                    && item["content"][0]["text"] == "follow developer policy"
            })
        }));
    }

    /// Verifies a route-bound opaque compaction item is replayed before later assistant text.
    /// 验证绑定路由的不透明压缩项会在后续助手文本前重放。
    #[test]
    fn codex_replays_route_bound_compaction_context() {
        let mut request = codex_request();
        request.messages.push(AiMessage {
            role: MessageRole::Assistant,
            content: vec![ContentItem::Text {
                text: String::from("Continue from the checkpoint."),
            }],
            provider_context_items: vec![
                json!({
                    "type": "compaction",
                    "encrypted_content": "opaque-checkpoint",
                }),
                json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "remote summary"}],
                }),
            ],
        });

        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("route-bound compaction context should encode");
        let input = encoded.body["input"]
            .as_array()
            .expect("input should be an array");
        let checkpoint_index = input
            .iter()
            .position(|item| item["type"] == "compaction")
            .expect("compaction item should be present");
        let text_index = input
            .iter()
            .position(|item| item["role"] == "assistant")
            .expect("assistant text should be present");
        assert!(checkpoint_index < text_index);
        assert_eq!(
            input[checkpoint_index]["encrypted_content"],
            "opaque-checkpoint"
        );
        assert_eq!(input[checkpoint_index + 1]["type"], "message");
    }

    /// Verifies native Responses preserves structured function-output content arrays.
    /// 验证原生 Responses 会保留结构化函数输出内容数组。
    #[test]
    fn codex_preserves_rich_function_output_arrays() {
        let mut request = codex_request();
        request.messages.push(AiMessage {
            role: MessageRole::Tool,
            content: vec![ContentItem::ToolResult {
                result: vulcan_ai_contract::ToolResult {
                    call_id: String::from("call-rich"),
                    is_error: false,
                    content: json!([
                        {"type": "input_text", "text": "done"},
                        {"type": "input_image", "image_url": "data:image/png;base64,AA=="}
                    ]),
                },
            }],
            provider_context_items: Vec::new(),
        });

        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("rich function output should encode");
        let output = encoded.body["input"]
            .as_array()
            .expect("input should be an array")
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .expect("function output should be present");

        assert!(output["output"].is_array());
        assert_eq!(output["output"][1]["type"], "input_image");
    }

    /// Verifies direct Chat encoding maps developer to system while Responses retains canonical roles.
    /// 验证直接 Chat 编码将 developer 映射到 system，而 Responses 保留规范角色。
    #[test]
    fn chat_developer_role_is_system_without_mutating_responses_history() {
        // request contains developer text plus structured policy in the same canonical message.
        // request 在同一规范消息内包含 developer 文本与结构化策略。
        let mut request = codex_request();
        request.messages.insert(
            1,
            AiMessage {
                role: MessageRole::Developer,
                content: vec![
                    ContentItem::Text {
                        text: String::from("policy\n"),
                    },
                    ContentItem::StructuredJson {
                        value: json!({"mode":"safe"}),
                    },
                ],
                provider_context_items: Vec::new(),
            },
        );
        // chat is the direct protocol entry point, separate from Responses-baseline projection.
        // chat 是独立于 Responses 基线投影的直接协议入口。
        let chat = OpenAiChatProfile
            .encode(&request)
            .expect("Chat developer conversion");
        assert_eq!(
            chat.body["messages"][1],
            json!({"role":"system","content":"policy\n{\"mode\":\"safe\"}"})
        );
        assert!(
            chat.body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .all(|message| message["role"] != "developer")
        );
        assert_eq!(request.messages[1].role, MessageRole::Developer);
        for profile in [
            &OpenAiResponsesProfile as &dyn ProtocolProfile,
            &OpenAiCodexResponsesProfile,
        ] {
            // responses must still carry developer independently of the selected Chat wire behavior.
            // responses 必须仍独立于 Chat 线路行为保留 developer。
            let responses = profile
                .encode(&request)
                .expect("Responses developer preservation");
            assert!(
                responses.body["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|message| message["role"] == "developer")
            );
        }
    }

    /// Verifies a proven Responses cursor emits `previous_response_id` and only the uncovered suffix.
    /// 验证已证明安全的 Responses 游标会发出 `previous_response_id` 且仅发送未覆盖后缀。
    #[test]
    fn codex_continuation_sends_only_uncovered_messages() {
        let mut request = codex_request();
        request.continuation = Some(vulcan_ai_contract::AiContinuation {
            provider_id: String::from("openai-codex"),
            protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("resp_previous"),
            covered_local_message_id: String::from("user-1"),
            covered_message_count: 2,
        });

        let encoded = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("Codex continuation should encode");

        assert_eq!(encoded.body["previous_response_id"], "resp_previous");
        let input = encoded.body["input"]
            .as_array()
            .expect("input should remain an array");
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "reasoning");
    }

    /// Verifies a continuation cannot cross protocol profiles even when the model identifier matches.
    /// 验证即使模型标识匹配，延续信息也不能跨越协议配置。
    #[test]
    fn responses_rejects_cross_profile_continuation() {
        let mut request = codex_request();
        request.continuation = Some(vulcan_ai_contract::AiContinuation {
            provider_id: String::from("openai"),
            protocol_profile_id: String::from(OPENAI_CHAT_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("resp_previous"),
            covered_local_message_id: String::from("user-1"),
            covered_message_count: 2,
        });

        let error = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect_err("cross-profile continuation must fail");

        assert!(error.message.contains("does not match active profile"));
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
            strict: false,
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

    /// Verifies generic Responses projects every registered hosted-tool shape exactly.
    /// 验证通用 Responses 会精确投影每个已注册托管工具结构。
    #[test]
    fn generic_responses_encodes_registered_native_tools() {
        let mut request = codex_request();
        request.native_tools = vec![
            NativeToolKind::WebSearch,
            NativeToolKind::CodeInterpreter,
            NativeToolKind::ImageGeneration,
        ];
        let encoded = OpenAiResponsesProfile
            .encode(&request)
            .expect("registered Responses native tools should encode");
        let tools = encoded.body["tools"]
            .as_array()
            .expect("tools should be an array");
        assert!(tools.iter().any(|tool| tool["type"] == "web_search"));
        assert!(tools.iter().any(|tool| {
            tool["type"] == "code_interpreter" && tool["container"]["type"] == "auto"
        }));
        assert!(tools.iter().any(|tool| tool["type"] == "image_generation"));
    }

    /// Verifies Apply Patch exposes its declared JSON function schema on every OpenAI profile.
    /// 验证 Apply Patch 在每种 OpenAI 配置上都暴露声明的 JSON 函数 Schema。
    #[test]
    fn apply_patch_uses_standard_function_schema_on_all_openai_profiles() {
        let mut request = codex_request();
        request.tools.push(ToolDefinition {
            name: String::from(APPLY_PATCH_TOOL_NAME),
            description: String::from("Applies an Apply Patch text document."),
            input_schema: json!({
                "type": "object",
                "properties": {"patch": {"type": "string"}},
                "required": ["patch"],
            }),
            strict: false,
        });

        // A named choice must agree with the emitted function declaration, including on Codex Responses.
        // 指定工具选择必须与输出的函数声明一致，包括 Codex Responses。
        request.tool_choice = ToolChoicePolicy::Specific {
            tool_name: String::from(APPLY_PATCH_TOOL_NAME),
        };

        // Both Responses profiles must expose the same ordinary function contract as Chat.
        // 两种 Responses 配置必须暴露与 Chat 相同的普通函数契约。
        for profile in [
            &OpenAiResponsesProfile as &dyn ProtocolProfile,
            &OpenAiCodexResponsesProfile,
        ] {
            let responses = profile
                .encode(&request)
                .expect("Responses apply_patch should encode");
            let function = responses.body["tools"]
                .as_array()
                .and_then(|tools| {
                    tools
                        .iter()
                        .find(|tool| tool["name"] == APPLY_PATCH_TOOL_NAME)
                })
                .expect("Responses apply_patch should be present");
            assert_eq!(function["type"], "function");
            assert_eq!(function["parameters"], request.tools[1].input_schema);
            assert_eq!(function["strict"], false);
            assert!(function.get("format").is_none());
            assert_eq!(
                responses.body["tool_choice"],
                json!({"type":"function","name":APPLY_PATCH_TOOL_NAME})
            );
        }

        let chat = OpenAiChatProfile
            .encode(&request)
            .expect("Chat apply_patch should encode");
        let function = chat.body["tools"]
            .as_array()
            .and_then(|tools| {
                tools.iter().find(|tool| {
                    tool.pointer("/function/name").and_then(Value::as_str)
                        == Some(APPLY_PATCH_TOOL_NAME)
                })
            })
            .expect("Chat apply_patch should be present");
        assert_eq!(function["type"], "function");
        assert_eq!(
            function["function"]["parameters"]["required"],
            json!(["patch"])
        );
    }

    /// Verifies legacy and normalized Apply Patch history replay as standard function pairs.
    /// 验证旧版与规范化 Apply Patch 历史均重放为标准函数配对。
    #[test]
    fn responses_replays_apply_patch_as_function_call_and_output() {
        let patch = "*** Begin Patch\n*** Delete File: obsolete.txt\n*** End Patch";
        let mut exact_request = codex_request();
        exact_request.messages = vec![
            AiMessage {
                role: MessageRole::Assistant,
                content: Vec::new(),
                provider_context_items: vec![json!({
                    "type": "custom_tool_call",
                    "id": "custom-item-1",
                    "call_id": "patch-call-1",
                    "name": APPLY_PATCH_TOOL_NAME,
                    "input": patch,
                })],
            },
            AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id: String::from("patch-call-1"),
                        is_error: false,
                        content: json!({"ok": true}),
                    },
                }],
                provider_context_items: Vec::new(),
            },
        ];

        let exact = OpenAiResponsesProfile
            .encode(&exact_request)
            .expect("exact custom Apply Patch history should encode");
        let exact_input = exact.body["input"]
            .as_array()
            .expect("exact Responses input should be an array");
        assert_eq!(exact_input[0]["type"], "function_call");
        assert_eq!(
            serde_json::from_str::<Value>(exact_input[0]["arguments"].as_str().unwrap()).unwrap(),
            json!({"patch": patch})
        );
        assert_eq!(exact_input[1]["type"], "function_call_output");
        assert_eq!(exact_input[1]["call_id"], "patch-call-1");

        let mut normalized_request = codex_request();
        normalized_request.messages = vec![
            AiMessage {
                role: MessageRole::Assistant,
                content: vec![ContentItem::ToolCall {
                    call: ToolCall {
                        id: String::from("patch-call-2"),
                        name: String::from(APPLY_PATCH_TOOL_NAME),
                        arguments: json!({"patch": patch}),
                        signature: None,
                    },
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id: String::from("patch-call-2"),
                        is_error: false,
                        content: json!({"ok": true}),
                    },
                }],
                provider_context_items: Vec::new(),
            },
        ];

        let normalized = OpenAiResponsesProfile
            .encode(&normalized_request)
            .expect("normalized Apply Patch history should encode as function items");
        let normalized_input = normalized.body["input"]
            .as_array()
            .expect("normalized Responses input should be an array");
        assert_eq!(normalized_input[0]["type"], "function_call");
        assert_eq!(
            serde_json::from_str::<Value>(normalized_input[0]["arguments"].as_str().unwrap())
                .unwrap(),
            json!({"patch": patch})
        );
        assert_eq!(normalized_input[1]["type"], "function_call_output");
        assert_eq!(normalized_input[1]["call_id"], "patch-call-2");
    }

    /// Verifies legacy output conversion respects cursor ownership without changing stored history.
    /// 验证旧输出转换遵守游标所有权，且不修改持久化历史。
    #[test]
    fn apply_patch_legacy_raw_output_and_continuation_keep_matching_call_types() {
        // request holds exact legacy call/output items instead of normalized tool content.
        // request 持有精确旧调用与输出项，而非规范工具内容。
        let mut request = codex_request();
        request.messages = vec![
            AiMessage {
                role: MessageRole::Assistant,
                content: Vec::new(),
                provider_context_items: vec![
                    json!({"type":"custom_tool_call","call_id":"patch-old","name":"apply_patch","input":"*** Begin Patch\n*** Delete File: old.txt\n*** End Patch"}),
                ],
            },
            AiMessage {
                role: MessageRole::Assistant,
                content: Vec::new(),
                provider_context_items: vec![
                    json!({"type":"custom_tool_call_output","call_id":"patch-old","output":"done"}),
                ],
            },
        ];
        // original guards the immutable history while full replay uses only function pairs.
        // original 守护不可变历史，全量重放仅使用函数配对。
        let original = request.messages.clone();
        let full = OpenAiResponsesProfile
            .encode(&request)
            .expect("legacy full replay");
        assert_eq!(full.body["input"][0]["type"], "function_call");
        assert_eq!(full.body["input"][1]["type"], "function_call_output");
        assert_eq!(full.body["input"][1]["output"], "done");
        assert_eq!(request.messages, original);

        // cursor owns the original custom call; only its pending output retains that wire pairing.
        // cursor 持有原始 custom 调用；仅其待提交输出保留对应线路配对。
        request.continuation = Some(vulcan_ai_contract::AiContinuation {
            provider_id: String::from("openai-codex"),
            protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("resp-legacy"),
            covered_local_message_id: String::from("assistant-old"),
            covered_message_count: 1,
        });
        let continued = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("legacy cursor output");
        assert_eq!(continued.body["previous_response_id"], "resp-legacy");
        assert_eq!(continued.body["input"].as_array().unwrap().len(), 1);
        assert_eq!(
            continued.body["input"][0]["type"],
            "custom_tool_call_output"
        );

        // Normalized suffix outputs obey the same server-owned pairing.
        // 规范后缀输出遵循相同的服务端配对。
        request.messages[1] = AiMessage {
            role: MessageRole::Tool,
            content: vec![ContentItem::ToolResult {
                result: ToolResult {
                    call_id: String::from("patch-old"),
                    is_error: false,
                    content: json!("done"),
                },
            }],
            provider_context_items: Vec::new(),
        };
        let continued = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("normalized cursor output");
        assert_eq!(
            continued.body["input"][0]["type"],
            "custom_tool_call_output"
        );
    }

    /// Verifies legacy normalization rejects malformed input and preserves unrelated custom replay.
    /// 验证旧项规范化拒绝错误输入，并保留无关 custom 重放。
    #[test]
    fn apply_patch_legacy_replay_is_narrow_and_validated() {
        // request isolates invalid Apply Patch data from unrelated historical custom tools.
        // request 隔离无效 Apply Patch 数据与无关历史 custom 工具。
        let mut request = codex_request();
        request.messages = vec![AiMessage {
            role: MessageRole::Assistant,
            content: Vec::new(),
            provider_context_items: vec![
                json!({"type":"custom_tool_call","call_id":"old","name":"apply_patch","input":42}),
            ],
        }];
        assert!(
            OpenAiResponsesProfile
                .encode(&request)
                .unwrap_err()
                .to_string()
                .contains("string input")
        );
        request.messages[0].provider_context_items[0] =
            json!({"type":"custom_tool_call","name":"apply_patch","input":"patch"});
        assert!(
            OpenAiResponsesProfile
                .encode(&request)
                .unwrap_err()
                .to_string()
                .contains("string call_id")
        );

        // unrelated is an existing provider custom item, not a newly exposed host tool.
        // unrelated 是已有供应商 custom 项，不是新暴露的宿主工具。
        let unrelated = json!({"type":"custom_tool_call","call_id":"other","name":"other-custom","input":"text"});
        request.messages[0].provider_context_items = vec![unrelated.clone()];
        request.messages.push(AiMessage {
            role: MessageRole::Tool,
            content: vec![ContentItem::ToolResult {
                result: ToolResult {
                    call_id: String::from("other"),
                    is_error: false,
                    content: json!("done"),
                },
            }],
            provider_context_items: Vec::new(),
        });
        let encoded = OpenAiResponsesProfile
            .encode(&request)
            .expect("unrelated exact replay");
        assert_eq!(encoded.body["input"][0], unrelated);
        assert_eq!(encoded.body["input"][1]["type"], "custom_tool_call_output");
    }

    /// Verifies Chat Completions still rejects provider-native hosted tools.
    /// 验证 Chat Completions 仍会拒绝供应商原生托管工具。
    #[test]
    fn chat_completions_rejects_native_tools() {
        let mut request = codex_request();
        request.native_tools = vec![NativeToolKind::WebSearch];
        let error = OpenAiChatProfile
            .encode(&request)
            .expect_err("Chat Completions cannot represent hosted tools");
        assert!(error.to_string().contains(OPENAI_CHAT_PROFILE_ID));
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

    /// Verifies provider-uploaded Kimi and MiniMax videos use the OpenAI-compatible video URL block.
    /// 验证供应商已上传的 Kimi 与 MiniMax 视频使用 OpenAI 兼容 video URL 内容块。
    #[test]
    fn chat_completions_encodes_provider_video_references() {
        // references are the two reviewed provider-native uploaded-file URI schemes.
        // references 是两个已审核供应商原生上传文件 URI 协议。
        let references = ["ms://file-kimi", "mm_file://file-minimax"];
        for reference in references {
            // request carries one already prepared video reference into protocol projection.
            // request 将一个已完成准备的视频引用带入协议投影。
            let mut request = codex_request();
            request.messages[1].content.push(ContentItem::Resource {
                resource: ResourceReference {
                    id: String::from("video-1"),
                    kind: ResourceKind::Video,
                    media_type: Some(String::from("video/mp4")),
                    source_kind: vulcan_ai_contract::ResourceSourceKind::HostResolved,
                    file_name: Some(String::from("clip.mp4")),
                    image_detail: None,
                    resource_path: None,
                    local_path: None,
                    url: Some(String::from(reference)),
                },
            });

            // encoded is the exact OpenAI Chat wire request sent to the selected provider route.
            // encoded 是发送到所选供应商路由的精确 OpenAI Chat Wire 请求。
            let encoded = OpenAiChatProfile
                .encode(&request)
                .expect("provider video reference should encode");
            assert_eq!(
                encoded.body["messages"][1]["content"][1],
                json!({
                    "type": "video_url",
                    "video_url": { "url": reference },
                })
            );
        }
    }

    /// Verifies every OpenAI request profile emits extra-high reasoning only as `xhigh`.
    /// 验证所有 OpenAI 请求 Profile 仅以 `xhigh` 发出超高推理档位。
    #[test]
    fn openai_profiles_encode_xhigh_with_the_canonical_wire_label() {
        let mut request = codex_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::XHigh);

        let chat = OpenAiChatProfile
            .encode(&request)
            .expect("Chat request should encode xhigh");
        let responses = OpenAiResponsesProfile
            .encode(&request)
            .expect("Responses request should encode xhigh");
        let codex = OpenAiCodexResponsesProfile
            .encode(&request)
            .expect("Codex request should encode xhigh");

        assert_eq!(chat.body["reasoning_effort"], "xhigh");
        assert_eq!(responses.body["reasoning"]["effort"], "xhigh");
        assert_eq!(codex.body["reasoning"]["effort"], "xhigh");
    }

    /// Verifies provider-default requests omit empty tools and automatic tool choice controls.
    /// 验证供应商默认请求会省略空工具与自动工具选择控制。
    #[test]
    fn openai_profiles_omit_unconfigured_tool_defaults() {
        let mut request = codex_request();
        request.tools.clear();

        let chat = OpenAiChatProfile
            .encode(&request)
            .expect("Chat request should encode");
        assert!(chat.body.get("tools").is_none());
        assert!(chat.body.get("tool_choice").is_none());

        let responses = OpenAiResponsesProfile
            .encode(&request)
            .expect("Responses request should encode");
        assert!(responses.body.get("tools").is_none());
        assert!(responses.body.get("tool_choice").is_none());
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
        let mut request = codex_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::High);
        request.continuation = Some(vulcan_ai_contract::AiContinuation {
            provider_id: String::from("openai"),
            protocol_profile_id: String::from(OPENAI_RESPONSES_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("resp_not_stored"),
            covered_local_message_id: String::from("assistant-1"),
            covered_message_count: request.messages.len(),
        });
        let encoded = OpenAiResponsesProfile
            .encode(&request)
            .expect("standard Responses request should encode");
        assert!(encoded.body.get("prompt_cache_key").is_none());
        assert!(!encoded.headers.contains_key("session-id"));
        assert!(!encoded.headers.contains_key("thread-id"));
        assert_eq!(encoded.body["input"][0]["role"], "system");
        assert_eq!(encoded.body["store"], false);
        assert!(encoded.body.get("previous_response_id").is_none());
        assert_eq!(
            encoded.body["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    /// Verifies assistant text remains before the first parallel tool call and every matching result.
    /// 验证助手文本仍位于首次并行工具调用及其全部匹配结果之前。
    #[test]
    fn responses_preserves_assistant_text_before_parallel_tool_calls() {
        // Build the exact first-turn replay shape emitted by the runtime after two tools finish.
        // 构造运行时在两个工具完成后产出的精确首次轮次回放结构。
        let mut request = codex_request();
        request.messages = vec![
            AiMessage {
                role: MessageRole::Assistant,
                content: vec![
                    ContentItem::Text {
                        text: String::from("I will inspect the project first."),
                    },
                    ContentItem::ToolCall {
                        call: ToolCall {
                            id: String::from("call-0"),
                            name: String::from("lookup"),
                            arguments: json!({"path": "Cargo.toml"}),
                            signature: None,
                        },
                    },
                    ContentItem::ToolCall {
                        call: ToolCall {
                            id: String::from("call-1"),
                            name: String::from("lookup"),
                            arguments: json!({"path": "src"}),
                            signature: None,
                        },
                    },
                ],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id: String::from("call-0"),
                        is_error: false,
                        content: json!({"found": true}),
                    },
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id: String::from("call-1"),
                        is_error: false,
                        content: json!({"entries": 3}),
                    },
                }],
                provider_context_items: Vec::new(),
            },
        ];

        // Encode the normalized history through the generic Responses profile used by DeepSeek.
        // 通过 DeepSeek 使用的通用 Responses 配置编码规范化历史。
        let encoded = OpenAiResponsesProfile
            .encode(&request)
            .expect("parallel tool replay should encode");
        // Read the ordered wire input once so every assertion checks the same request payload.
        // 只读取一次有序线端输入，确保所有断言检查同一个请求载荷。
        let input = encoded.body["input"]
            .as_array()
            .expect("Responses input should be an array");

        assert_eq!(input.len(), 5);
        assert_eq!(input[0]["role"], "assistant");
        assert_eq!(
            input[0]["content"][0]["text"],
            "I will inspect the project first."
        );
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call-0");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "call-1");
        assert_eq!(input[3]["type"], "function_call_output");
        assert_eq!(input[3]["call_id"], "call-0");
        assert_eq!(input[4]["type"], "function_call_output");
        assert_eq!(input[4]["call_id"], "call-1");
    }

    /// Verifies exact route-bound Responses output items bypass provider-neutral re-encoding.
    /// 验证绑定精确路由的 Responses 原始输出项会绕过供应商无关重编码。
    #[test]
    fn responses_replays_exact_provider_output_items_without_reencoding() {
        // providerItems mirror one reasoning plus function-call response returned by the provider.
        // providerItems 镜像供应商返回的一项推理与函数调用响应。
        let provider_items = vec![
            json!({
                "type": "reasoning",
                "id": "reasoning-item",
                "status": "completed",
                "summary": [{"type": "summary_text", "text": "inspect the workspace"}],
            }),
            json!({
                "type": "function_call",
                "id": "function-item",
                "status": "completed",
                "call_id": "call-0",
                "name": "lookup",
                "arguments": "{\"path\":\"src\"}",
            }),
        ];
        let mut request = codex_request();
        request.messages = vec![AiMessage {
            role: MessageRole::Assistant,
            content: Vec::new(),
            provider_context_items: provider_items.clone(),
        }];

        let encoded = OpenAiResponsesProfile
            .encode(&request)
            .expect("exact provider output items should encode");
        assert_eq!(encoded.body["input"], Value::Array(provider_items));
    }

    /// Verifies returning to Chat preserves the exact prefix encoded before an Anthropic turn.
    /// 验证经过 Anthropic 轮次后切回 Chat 仍保留此前编码过的精确前缀。
    #[test]
    fn chat_history_prefix_is_stable_across_anthropic_protocol_switch() {
        let mut initial_request = codex_request();
        initial_request.messages.truncate(2);
        initial_request.messages.extend([
            AiMessage {
                role: MessageRole::Assistant,
                content: vec![ContentItem::ToolCall {
                    call: ToolCall {
                        id: String::from("stable-call-1"),
                        name: String::from("lookup"),
                        arguments: json!({"path": "src/lib.rs"}),
                        signature: None,
                    },
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id: String::from("stable-call-1"),
                        is_error: false,
                        content: json!({"found": true}),
                    },
                }],
                provider_context_items: Vec::new(),
            },
        ]);
        let initial_chat = OpenAiChatProfile
            .encode(&initial_request)
            .expect("initial Chat history should encode");

        let mut switched_request = initial_request.clone();
        switched_request.messages.extend([
            AiMessage {
                role: MessageRole::Assistant,
                content: vec![ContentItem::Text {
                    text: String::from("Chat answer"),
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("Anthropic question"),
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::Assistant,
                content: vec![ContentItem::Text {
                    text: String::from("Anthropic answer"),
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("Return to Chat"),
                }],
                provider_context_items: Vec::new(),
            },
        ]);
        crate::anthropic::AnthropicMessagesProfile
            .encode(&switched_request)
            .expect("the same canonical history should encode for Anthropic");

        let returned_chat = OpenAiChatProfile
            .encode(&switched_request)
            .expect("returned Chat history should encode");
        let initial_messages = initial_chat.body["messages"]
            .as_array()
            .expect("initial Chat messages should be an array");
        let returned_messages = returned_chat.body["messages"]
            .as_array()
            .expect("returned Chat messages should be an array");

        assert_eq!(
            &returned_messages[..initial_messages.len()],
            initial_messages.as_slice()
        );
        assert_eq!(returned_chat.body["tools"], initial_chat.body["tools"]);
    }
}
