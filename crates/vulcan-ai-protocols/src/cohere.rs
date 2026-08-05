//! Provider-independent Cohere v2 Chat wire profile.
//! 供应商无关的 Cohere v2 Chat Wire Profile。

use crate::{
    EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder,
    merge_streamed_tool_name, shared::append_bounded_tool_arguments,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiStreamEvent, ContentItem, MessageRole, ParallelToolCallPolicy,
    PromptCachePolicy, ReasoningOutputPolicy, RequestUsage, StructuredOutputPolicy, ToolCall,
    ToolCallProgress, ToolChoicePolicy, UsageValue, UsageValueSource,
};

/// Stable profile identifier for Cohere v2 Chat.
/// Cohere v2 Chat 的稳定 Profile 标识。
pub const COHERE_CHAT_PROFILE_ID: &str = "cohere.chat.v2";

/// Encodes and decodes the provider-independent Cohere v2 Chat wire contract.
/// 编解码供应商无关的 Cohere v2 Chat Wire 契约。
pub struct CohereChatProfile;

impl ProtocolProfile for CohereChatProfile {
    /// Returns the stable Cohere profile identifier.
    /// 返回稳定的 Cohere Profile 标识。
    fn id(&self) -> &'static str {
        COHERE_CHAT_PROFILE_ID
    }

    /// Returns the bounded Cohere SSE transport declaration.
    /// 返回有界 Cohere SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one public request into Cohere v2 Chat JSON.
    /// 将一个公共请求编码为 Cohere v2 Chat JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        if request.prompt_cache != PromptCachePolicy::ProviderDefault {
            return Err(ProtocolConversionError::new(
                "Cohere explicit prompt-cache projection is not registered.",
            ));
        }
        let mut messages = Vec::new();
        for message in &request.messages {
            messages.extend(encode_cohere_message(message)?);
        }
        let mut body = serde_json::Map::from_iter([
            (
                String::from("model"),
                Value::String(request.model_id.clone()),
            ),
            (String::from("stream"), Value::Bool(true)),
            (String::from("messages"), Value::Array(messages)),
        ]);
        if !request.tools.is_empty() {
            body.insert(
                String::from("tools"),
                Value::Array(
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
                        .collect(),
                ),
            );
        }
        if let Some(limit) = request.output_token_limit {
            body.insert(String::from("max_tokens"), json!(limit));
        }
        project_cohere_request_controls(request, &mut body)?;
        Ok(EncodedWireRequest {
            path: String::from("/v2/chat"),
            headers: BTreeMap::new(),
            body: Value::Object(body),
        })
    }

    /// Creates one isolated Cohere stream decoder.
    /// 创建一个隔离的 Cohere 流解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(CohereStreamDecoder::with_registered_tools(
            execution_id,
            request.tools.iter().map(|tool| tool.name.clone()).collect(),
        ))
    }
}

/// Projects provider-neutral controls into Cohere v2 Chat fields.
/// 将供应商无关控制投影到 Cohere v2 Chat 字段。
///
/// # Parameters
/// 参数
/// `request` is the exact public request and `body` is the mutable Cohere object.
/// `request` 是精确公共请求，`body` 是可变 Cohere 对象。
///
/// # Returns
/// 返回值
/// A controlled conversion error when Cohere v2 has no exact representation.
/// 当 Cohere v2 没有精确表示时返回受控转换错误。
fn project_cohere_request_controls(
    request: &AiExecutionRequest,
    body: &mut serde_json::Map<String, Value>,
) -> Result<(), ProtocolConversionError> {
    match &request.tool_choice {
        ToolChoicePolicy::Auto => {}
        ToolChoicePolicy::Required => {
            body.insert(
                String::from("tool_choice"),
                Value::String(String::from("REQUIRED")),
            );
        }
        ToolChoicePolicy::Disabled => {
            body.insert(
                String::from("tool_choice"),
                Value::String(String::from("NONE")),
            );
        }
        ToolChoicePolicy::Specific { .. } => {
            return Err(ProtocolConversionError::new(
                "Cohere v2 Chat has no registered specific-tool-choice projector.",
            ));
        }
    }
    if !matches!(
        request.parallel_tool_calls,
        ParallelToolCallPolicy::ProviderDefault
    ) {
        return Err(ProtocolConversionError::new(
            "Cohere v2 Chat has no registered parallel-tool-call projector.",
        ));
    }
    match &request.structured_output {
        StructuredOutputPolicy::None => {}
        StructuredOutputPolicy::JsonObject => {
            body.insert(
                String::from("response_format"),
                json!({ "type": "json_object" }),
            );
        }
        StructuredOutputPolicy::JsonSchema { schema, .. } => {
            body.insert(
                String::from("response_format"),
                json!({ "type": "json_object", "schema": schema }),
            );
        }
    }
    if !request.stop_sequences.is_empty() {
        body.insert(
            String::from("stop_sequences"),
            json!(request.stop_sequences),
        );
    }
    if request.reasoning_effort.is_some()
        || !matches!(
            request.reasoning_output,
            ReasoningOutputPolicy::ProviderDefault
        )
    {
        return Err(ProtocolConversionError::new(
            "Cohere v2 Chat has no registered public reasoning projector.",
        ));
    }
    Ok(())
}

/// Encodes one public message into one or more native Cohere messages.
/// 将一条公共消息编码为一条或多条原生 Cohere 消息。
fn encode_cohere_message(message: &AiMessage) -> Result<Vec<Value>, ProtocolConversionError> {
    let mut ordinary_content = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();
    for item in &message.content {
        match item {
            ContentItem::Text { text } | ContentItem::Refusal { text } => {
                ordinary_content.push(json!({ "type": "text", "text": text }));
            }
            ContentItem::StructuredJson { value } => {
                ordinary_content.push(json!({
                    "type": "text",
                    "text": serde_json::to_string(value).map_err(|error| {
                        ProtocolConversionError::new(format!(
                            "Structured JSON could not be encoded for Cohere: {error}"
                        ))
                    })?
                }));
            }
            ContentItem::Reasoning { .. } => {
                // Cohere reasoning is not replayed because the public contract cannot prove that
                // an upstream signature permits replay.
                // Cohere 推理不参与回放，因为公共契约无法证明上游签名允许回放。
            }
            ContentItem::ToolCall { call } => {
                tool_calls.push(json!({
                    "id": call.id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": serde_json::to_string(&call.arguments).map_err(|error| {
                            ProtocolConversionError::new(format!(
                                "Cohere tool arguments could not be encoded: {error}"
                            ))
                        })?,
                    }
                }));
            }
            ContentItem::ToolResult { result } => {
                tool_results.push(json!({
                    "role": "tool",
                    "tool_call_id": result.call_id,
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&json!({
                            "result": result.content,
                            "is_error": result.is_error,
                        })).map_err(|error| {
                            ProtocolConversionError::new(format!(
                                "Cohere tool result could not be encoded: {error}"
                            ))
                        })?,
                    }],
                }));
            }
            ContentItem::Citation { title, url, text } => {
                ordinary_content.push(json!({
                    "type": "text",
                    "text": format!(
                        "{}: {}{}",
                        title,
                        url,
                        text.as_ref()
                            .map(|value| format!("\n{value}"))
                            .unwrap_or_default()
                    )
                }));
            }
            ContentItem::Resource { resource } => {
                return Err(ProtocolConversionError::new(format!(
                    "Cohere v2 Chat has no registered {:?} input projector.",
                    resource.kind
                )));
            }
        }
    }
    let mut encoded = Vec::new();
    match message.role {
        MessageRole::System => {
            let content = ordinary_content
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            encoded.push(json!({ "role": "system", "content": content }));
        }
        MessageRole::User => {
            encoded.push(json!({ "role": "user", "content": ordinary_content }));
        }
        MessageRole::Assistant => {
            encoded.push(json!({
                "role": "assistant",
                "content": ordinary_content,
                "tool_calls": tool_calls,
                "tool_plan": Value::Null,
                "citations": [],
            }));
        }
        MessageRole::Tool => {
            if tool_results.is_empty() {
                return Err(ProtocolConversionError::new(
                    "A Cohere tool-role message must contain at least one tool result.",
                ));
            }
            encoded.extend(tool_results);
        }
    }
    Ok(encoded)
}

/// Tracks one incomplete Cohere tool call by stable stream index.
/// 按稳定流索引跟踪一条未完成的 Cohere 工具调用。
#[derive(Default)]
struct PendingCohereTool {
    /// Provider-visible call identifier.
    /// Provider 可见调用标识。
    call_id: String,
    /// Complete tool name once received.
    /// 接收完成后的完整工具名称。
    name: String,
    /// Raw accumulated argument JSON.
    /// 累积的原始参数 JSON。
    arguments: String,
    /// Whether the public tool-start event was emitted.
    /// 是否已经发出公共工具开始事件。
    started: bool,
}

/// Incrementally normalizes Cohere v2 SSE records.
/// 增量归一化 Cohere v2 SSE 记录。
struct CohereStreamDecoder {
    /// Stable execution identifier.
    /// 稳定执行标识。
    execution_id: String,
    /// Whether the public response-start event was emitted.
    /// 是否已经发出公共响应开始事件。
    started: bool,
    /// Active text item identifier.
    /// 当前文本项目标识。
    text_item: Option<String>,
    /// Active reasoning item identifier.
    /// 当前推理项目标识。
    reasoning_item: Option<String>,
    /// Pending tool calls by provider index.
    /// 按供应商索引保存的待定工具调用。
    tools: HashMap<usize, PendingCohereTool>,
    /// Exact tool names frozen from the validated request.
    /// 从已校验请求冻结的精确工具名称。
    registered_tools: BTreeSet<String>,
    /// Whether a terminal event was emitted.
    /// 是否已经发出终态事件。
    completed: bool,
}

impl CohereStreamDecoder {
    /// Creates one fresh Cohere decoder.
    /// 创建一个新的 Cohere 解码器。
    /// Creates one fresh Cohere decoder with an exact registered-tool snapshot.
    /// 使用精确的已注册工具快照创建一个新的 Cohere 解码器。
    fn with_registered_tools(execution_id: String, registered_tools: BTreeSet<String>) -> Self {
        Self {
            execution_id,
            started: false,
            text_item: None,
            reasoning_item: None,
            tools: HashMap::new(),
            registered_tools,
            completed: false,
        }
    }

    /// Emits the response-start event once.
    /// 仅发出一次响应开始事件。
    fn ensure_started(&mut self, events: &mut Vec<AiStreamEvent>) {
        if !self.started {
            self.started = true;
            events.push(AiStreamEvent::ResponseStarted {
                execution_id: self.execution_id.clone(),
            });
        }
    }

    /// Starts or appends one Cohere text/thinking content block.
    /// 启动或追加一个 Cohere 文本/思考内容块。
    fn push_content(&mut self, index: usize, content: &Value, events: &mut Vec<AiStreamEvent>) {
        let kind = content.get("type").and_then(Value::as_str);
        let reasoning = kind == Some("thinking") || content.get("thinking").is_some();
        let delta = if reasoning {
            content.get("thinking").and_then(Value::as_str)
        } else {
            content.get("text").and_then(Value::as_str)
        };
        if reasoning {
            let item_id = format!("cohere:reasoning:{index}");
            if self.reasoning_item.as_deref() != Some(item_id.as_str()) {
                self.reasoning_item = Some(item_id.clone());
                events.push(AiStreamEvent::ReasoningStarted {
                    item_id: item_id.clone(),
                });
            }
            if let Some(delta) = delta.filter(|value| !value.is_empty()) {
                events.push(AiStreamEvent::ReasoningDelta {
                    item_id,
                    delta: delta.to_string(),
                });
            }
        } else {
            let item_id = format!("cohere:text:{index}");
            if self.text_item.as_deref() != Some(item_id.as_str()) {
                self.text_item = Some(item_id.clone());
                events.push(AiStreamEvent::ResponseItemStarted {
                    item_id: item_id.clone(),
                    item: ContentItem::Text {
                        text: String::new(),
                    },
                });
            }
            if let Some(delta) = delta.filter(|value| !value.is_empty()) {
                events.push(AiStreamEvent::ResponseTextDelta {
                    item_id,
                    delta: delta.to_string(),
                });
            }
        }
    }

    /// Merges one Cohere tool call fragment and emits visible receive progress.
    /// 合并一个 Cohere 工具调用分片并发出可见接收进度。
    fn push_tool_fragment(
        &mut self,
        index: usize,
        fragment: &Value,
        events: &mut Vec<AiStreamEvent>,
    ) -> Result<(), ProtocolConversionError> {
        let pending = self.tools.entry(index).or_default();
        if let Some(call_id) = fragment.get("id").and_then(Value::as_str) {
            pending.call_id = call_id.to_string();
        }
        if let Some(function) = fragment.get("function") {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                pending.name =
                    merge_streamed_tool_name(pending.name.as_str(), name, &self.registered_tools);
            }
            if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                append_bounded_tool_arguments(&mut pending.arguments, arguments, "Cohere")?;
            }
        }
        if pending.call_id.is_empty() {
            pending.call_id = format!("cohere-call-{index}");
        }
        let item_id = format!("cohere:tool:{index}");
        if !pending.started && self.registered_tools.contains(&pending.name) {
            pending.started = true;
            events.push(AiStreamEvent::ToolCallStarted {
                item_id: item_id.clone(),
                call_id: pending.call_id.clone(),
                tool_name: pending.name.clone(),
                index: u32::try_from(index).unwrap_or(u32::MAX),
            });
        }
        if pending.started {
            let delta = fragment
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !delta.is_empty() {
                events.push(AiStreamEvent::ToolCallArgumentsDelta {
                    item_id,
                    delta: delta.to_string(),
                    progress: ToolCallProgress {
                        call_id: Some(pending.call_id.clone()),
                        tool_name: Some(pending.name.clone()),
                        bytes_received: pending.arguments.len() as u64,
                        characters_received: pending.arguments.chars().count() as u64,
                        complete: false,
                    },
                });
            }
        }
        Ok(())
    }

    /// Completes one indexed Cohere tool call or emits an explicit truncation event.
    /// 完成一个带索引的 Cohere 工具调用，或发出显式截断事件。
    fn finish_tool(&mut self, index: usize, events: &mut Vec<AiStreamEvent>) {
        let Some(pending) = self.tools.remove(&index) else {
            return;
        };
        let item_id = format!("cohere:tool:{index}");
        if !pending.started {
            events.push(AiStreamEvent::ToolCallAborted {
                item_id,
                message: String::from(
                    "Cohere tool call ended before a complete tool identity was received.",
                ),
            });
            return;
        }
        let arguments = if pending.arguments.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str::<Value>(pending.arguments.as_str()) {
                Ok(arguments) => arguments,
                Err(error) => {
                    events.push(AiStreamEvent::ToolCallAborted {
                        item_id,
                        message: format!(
                            "Cohere tool call ended before valid JSON was complete: {error}"
                        ),
                    });
                    return;
                }
            }
        };
        events.push(AiStreamEvent::ToolCallCompleted {
            item_id,
            call: ToolCall {
                id: pending.call_id,
                name: pending.name,
                arguments,
                signature: None,
            },
        });
    }

    /// Closes the currently active Cohere content item.
    /// 关闭当前活动的 Cohere 内容项目。
    fn finish_content(&mut self, events: &mut Vec<AiStreamEvent>) {
        if let Some(item_id) = self.text_item.take() {
            events.push(AiStreamEvent::ResponseItemCompleted { item_id });
        }
        if let Some(item_id) = self.reasoning_item.take() {
            events.push(AiStreamEvent::ReasoningCompleted {
                item_id,
                signature: None,
            });
        }
    }
}

impl ProtocolStreamDecoder for CohereStreamDecoder {
    /// Pushes one complete Cohere SSE JSON record.
    /// 推入一条完整的 Cohere SSE JSON 记录。
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
                "Cohere emitted data after the response completed.",
            ));
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            ProtocolConversionError::new(format!("Cohere stream JSON is invalid: {error}"))
        })?;
        let event_type = value.get("type").and_then(Value::as_str).ok_or_else(|| {
            ProtocolConversionError::new("Cohere stream event is missing `type`.")
        })?;
        let mut events = Vec::new();
        self.ensure_started(&mut events);
        match event_type {
            "content-start" | "content-delta" => {
                let index = required_cohere_index(&value)?;
                if let Some(content) = value.pointer("/delta/message/content") {
                    self.push_content(index, content, &mut events);
                }
            }
            "content-end" => self.finish_content(&mut events),
            "tool-plan-delta" => {
                if let Some(delta) = value
                    .pointer("/delta/message/tool_plan")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    let item_id = String::from("cohere:tool-plan");
                    if self.reasoning_item.as_deref() != Some(item_id.as_str()) {
                        self.reasoning_item = Some(item_id.clone());
                        events.push(AiStreamEvent::ReasoningStarted {
                            item_id: item_id.clone(),
                        });
                    }
                    events.push(AiStreamEvent::ReasoningDelta {
                        item_id,
                        delta: delta.to_string(),
                    });
                }
            }
            "tool-call-start" | "tool-call-delta" => {
                let index = required_cohere_index(&value)?;
                if let Some(fragment) = value.pointer("/delta/message/tool_calls") {
                    self.push_tool_fragment(index, fragment, &mut events)?;
                }
            }
            "tool-call-end" => {
                self.finish_tool(required_cohere_index(&value)?, &mut events);
            }
            "message-end" => {
                self.finish_content(&mut events);
                let mut indices = self.tools.keys().copied().collect::<Vec<_>>();
                indices.sort_unstable();
                for index in indices {
                    self.finish_tool(index, &mut events);
                }
                if let Some(usage) = value.pointer("/delta/usage") {
                    events.push(AiStreamEvent::UsageUpdated {
                        usage: cohere_usage(usage),
                    });
                }
                self.completed = true;
                events.push(AiStreamEvent::ResponseCompleted);
            }
            "message-start" | "citation-start" | "citation-end" | "debug" => {}
            other => {
                return Err(ProtocolConversionError::new(format!(
                    "Cohere SSE event type `{other}` is not registered."
                )));
            }
        }
        Ok(events)
    }

    /// Finishes the Cohere stream and aborts incomplete tool JSON deterministically.
    /// 结束 Cohere 流并确定性中止未完成工具 JSON。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.completed {
            return Ok(Vec::new());
        }
        Err(ProtocolConversionError::new(
            "Cohere stream ended before a message-end event was received.",
        ))
    }
}

/// Reads one mandatory Cohere stream index without collapsing missing identities into slot zero.
/// 读取一个必需的 Cohere 流索引，且不会把缺失身份折叠为第零槽位。
///
/// # Parameters
/// 参数
/// `value` is the complete Cohere SSE event object.
/// `value` 是完整的 Cohere SSE 事件对象。
///
/// # Returns
/// 返回值
/// The exact platform-sized index or a controlled protocol error.
/// 返回精确的平台大小索引或受控协议错误。
fn required_cohere_index(value: &Value) -> Result<usize, ProtocolConversionError> {
    let index = value.get("index").and_then(Value::as_u64).ok_or_else(|| {
        ProtocolConversionError::new("Cohere indexed stream event is missing `index`.")
    })?;
    usize::try_from(index)
        .map_err(|_| ProtocolConversionError::new("Cohere stream index exceeds usize."))
}

/// Converts Cohere token or billed-unit usage without inventing missing fields.
/// 转换 Cohere Token 或计费单位用量且不伪造缺失字段。
fn cohere_usage(value: &Value) -> RequestUsage {
    let input = value
        .pointer("/tokens/input_tokens")
        .or_else(|| value.pointer("/billed_units/input_tokens"));
    let output = value
        .pointer("/tokens/output_tokens")
        .or_else(|| value.pointer("/billed_units/output_tokens"));
    let input_tokens = reported_cohere_usage(input);
    let output_tokens = reported_cohere_usage(output);
    let total_tokens = match (known_usage(input_tokens), known_usage(output_tokens)) {
        (Some(input), Some(output)) => UsageValue::Known {
            value: input.saturating_add(output),
            source: UsageValueSource::Derived,
        },
        _ => UsageValue::Unknown,
    };
    RequestUsage {
        input_tokens,
        uncached_input_tokens: input_tokens,
        output_tokens,
        total_tokens,
        ..RequestUsage::default()
    }
}

/// Converts one non-negative Cohere numeric usage field into reported integer usage.
/// 将一个非负 Cohere 数值用量字段转换为已报告整数用量。
fn reported_cohere_usage(value: Option<&Value>) -> UsageValue {
    let integer = value.and_then(|value| {
        value.as_u64().or_else(|| {
            value.as_f64().and_then(|value| {
                (value.is_finite()
                    && value >= 0.0
                    && value.fract() == 0.0
                    && value <= u64::MAX as f64)
                    .then_some(value as u64)
            })
        })
    });
    integer
        .map(|value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        })
        .unwrap_or(UsageValue::Unknown)
}

/// Returns a known usage count without changing provenance.
/// 返回已知用量计数且不改变来源。
fn known_usage(value: UsageValue) -> Option<u64> {
    match value {
        UsageValue::Known { value, .. } => Some(value),
        UsageValue::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::{AiMessage, AiOperation, ToolDefinition};

    /// Verifies Cohere request encoding preserves native tool message shapes.
    /// 验证 Cohere 请求编码保留原生工具消息形状。
    #[test]
    fn encodes_public_tool_shape() {
        let encoded = CohereChatProfile
            .encode(&AiExecutionRequest {
                operation: AiOperation::ConversationRespond,
                model_id: String::from("command"),
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
                output_token_limit: Some(128),
            })
            .expect("request should encode");
        assert_eq!(encoded.body["messages"][0]["role"], "user");
        assert_eq!(encoded.body["tools"][0]["function"]["name"], "lookup");
    }

    /// Verifies fragmented Cohere tool arguments remain incremental until tool-call-end.
    /// 验证分片 Cohere 工具参数在 tool-call-end 前保持增量状态。
    #[test]
    fn decodes_fragmented_tool_arguments() {
        let mut decoder = CohereStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::from([String::from("lookup")]),
        );
        let started = decoder
            .push_sse(
                None,
                r#"{"type":"tool-call-start","index":0,"delta":{"message":{"tool_calls":{"id":"call","function":{"name":"lookup","arguments":"{\"q\":\""}}}}}"#,
            )
            .expect("start should decode");
        assert!(started.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. } if tool_name == "lookup"
        )));
        decoder
            .push_sse(
                None,
                r#"{"type":"tool-call-delta","index":0,"delta":{"message":{"tool_calls":{"function":{"arguments":"x\"}"}}}}}"#,
            )
            .expect("delta should decode");
        let completed = decoder
            .push_sse(None, r#"{"type":"tool-call-end","index":0}"#)
            .expect("tool should complete");
        assert!(completed.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. } if call.arguments == json!({"q":"x"})
        )));
    }

    /// Verifies an unknown Cohere stream event fails instead of being silently discarded.
    /// 验证未知 Cohere 流事件会失败，而不是被静默丢弃。
    #[test]
    fn rejects_unknown_stream_event() {
        let mut decoder =
            CohereStreamDecoder::with_registered_tools(String::from("execution"), BTreeSet::new());
        let error = decoder
            .push_sse(None, r#"{"type":"future-event"}"#)
            .expect_err("unknown event must fail");
        assert!(error.to_string().contains("future-event"));
    }

    /// Verifies indexed Cohere events reject a missing index instead of using slot zero.
    /// 验证带索引的 Cohere 事件会拒绝缺失索引，而不是使用第零槽位。
    #[test]
    fn rejects_missing_index_on_indexed_event() {
        let mut decoder =
            CohereStreamDecoder::with_registered_tools(String::from("execution"), BTreeSet::new());
        let error = decoder
            .push_sse(
                None,
                r#"{"type":"content-delta","delta":{"message":{"content":{"type":"text","text":"x"}}}}"#,
            )
            .expect_err("missing content index must fail");
        assert!(error.to_string().contains("missing `index`"));
    }
}
