//! Google Gemini Interactions wire profile.
//! Google Gemini Interactions 线级协议 Profile。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiStreamEvent, ContentItem, MessageRole, PromptCachePolicy,
    RequestUsage, ToolCall, ToolCallProgress, UsageValue, UsageValueSource,
};

/// Stable Google Gemini Interactions profile identifier.
/// 稳定的 Google Gemini Interactions Profile 标识。
pub const GEMINI_INTERACTIONS_PROFILE_ID: &str = "gemini-interactions.v1";

/// Current Google Interactions revision required by the post-May-2026 step schema.
/// 2026 年 5 月之后 Steps 结构所需的当前 Google Interactions 修订号。
const GEMINI_INTERACTIONS_API_REVISION: &str = "2026-05-20";

/// Encodes provider-neutral requests for the native Gemini Interactions endpoint.
/// 为原生 Gemini Interactions 端点编码供应商无关请求。
#[derive(Debug, Default)]
pub struct GeminiInteractionsProfile;

impl ProtocolProfile for GeminiInteractionsProfile {
    /// Returns the stable Interactions profile identifier.
    /// 返回稳定的 Interactions Profile 标识。
    fn id(&self) -> &'static str {
        GEMINI_INTERACTIONS_PROFILE_ID
    }

    /// Encodes one public request into the revised Interactions schema.
    /// 将一个公共请求编码为修订后的 Interactions 结构。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        if !matches!(request.prompt_cache, PromptCachePolicy::ProviderDefault) {
            return Err(ProtocolConversionError::new(
                "Gemini Interactions explicit cache projection is not registered.",
            ));
        }
        let (system_instruction, input) = encode_interactions_input(&request.messages)?;
        let mut body = json!({
            "model": request.model_id,
            "input": input,
            "tools": encode_interactions_tools(request),
            "stream": true,
            "store": false,
        });
        if let Some(system_instruction) = system_instruction {
            body["system_instruction"] = Value::String(system_instruction);
        }
        if let Some(limit) = request.output_token_limit {
            body["generation_config"] = json!({"max_output_tokens": limit});
        }
        if let Some(effort) = request.reasoning_effort {
            let thinking_level = interactions_thinking_level(effort)?;
            let generation_config = body
                .as_object_mut()
                .expect("Interactions request body is always an object")
                .entry(String::from("generation_config"))
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .ok_or_else(|| {
                    ProtocolConversionError::new(
                        "Gemini Interactions generation_config must be an object.",
                    )
                })?;
            generation_config.insert(
                String::from("thinking_level"),
                Value::String(thinking_level.to_string()),
            );
            generation_config.insert(String::from("thinking_summaries"), json!("auto"));
        }
        Ok(EncodedWireRequest {
            path: String::from("/interactions"),
            headers: BTreeMap::from([(
                String::from("api-revision"),
                String::from(GEMINI_INTERACTIONS_API_REVISION),
            )]),
            body,
        })
    }

    /// Creates one isolated revised-Interactions stream decoder.
    /// 创建一个隔离的修订版 Interactions 流解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(GeminiInteractionsStreamDecoder::new(execution_id))
    }
}

/// Maps only Interactions-supported public effort values into a native thinking level.
/// 仅将 Interactions 支持的公共推理档位映射为原生思考等级。
fn interactions_thinking_level(
    effort: vulcan_ai_contract::ReasoningEffort,
) -> Result<&'static str, ProtocolConversionError> {
    match effort {
        vulcan_ai_contract::ReasoningEffort::Minimal => Ok("minimal"),
        vulcan_ai_contract::ReasoningEffort::Low => Ok("low"),
        vulcan_ai_contract::ReasoningEffort::Medium => Ok("medium"),
        vulcan_ai_contract::ReasoningEffort::High => Ok("high"),
        unsupported => Err(ProtocolConversionError::new(format!(
            "Gemini Interactions does not expose reasoning effort `{}`.",
            unsupported.as_str()
        ))),
    }
}

/// Encodes ordered public messages into Interactions system text and steps.
/// 将有序公共消息编码为 Interactions 系统文本与 Steps。
fn encode_interactions_input(
    messages: &[AiMessage],
) -> Result<(Option<String>, Vec<Value>), ProtocolConversionError> {
    let mut system_parts = Vec::new();
    let mut input = Vec::new();
    let mut tool_names = BTreeMap::<String, String>::new();
    for message in messages {
        for item in &message.content {
            if let ContentItem::ToolCall { call } = item {
                tool_names.insert(call.id.clone(), call.name.clone());
            }
        }
    }
    for message in messages {
        match message.role {
            MessageRole::System => {
                for item in &message.content {
                    match item {
                        ContentItem::Text { text } | ContentItem::Refusal { text } => {
                            system_parts.push(text.clone());
                        }
                        ContentItem::StructuredJson { value } => {
                            system_parts.push(serde_json::to_string(value).map_err(|error| {
                                ProtocolConversionError::new(format!(
                                    "Gemini Interactions system JSON could not be encoded: {error}"
                                ))
                            })?);
                        }
                        _ => {
                            return Err(ProtocolConversionError::new(
                                "Gemini Interactions system messages support text and structured JSON only.",
                            ));
                        }
                    }
                }
            }
            MessageRole::User | MessageRole::Assistant => {
                input.extend(encode_interactions_message(message)?);
            }
            MessageRole::Tool => {
                for item in &message.content {
                    let ContentItem::ToolResult { result } = item else {
                        return Err(ProtocolConversionError::new(
                            "Gemini Interactions tool messages may only contain tool results.",
                        ));
                    };
                    let name = tool_names.get(result.call_id.as_str()).ok_or_else(|| {
                        ProtocolConversionError::new(format!(
                            "Gemini Interactions tool result `{}` has no preceding tool identity.",
                            result.call_id
                        ))
                    })?;
                    input.push(json!({
                        "type": "function_result",
                        "call_id": result.call_id,
                        "name": name,
                        "result": result.content,
                    }));
                }
            }
        }
    }
    let system_instruction = (!system_parts.is_empty()).then(|| system_parts.join("\n"));
    Ok((system_instruction, input))
}

/// Encodes one user or assistant message into one or more Interactions steps.
/// 将一条用户或助手消息编码为一个或多个 Interactions Step。
fn encode_interactions_message(message: &AiMessage) -> Result<Vec<Value>, ProtocolConversionError> {
    let step_type = if message.role == MessageRole::User {
        "user_input"
    } else {
        "model_output"
    };
    let mut content = Vec::new();
    let mut steps = Vec::new();
    for item in &message.content {
        match item {
            ContentItem::Text { text } | ContentItem::Refusal { text } => {
                content.push(json!({"type": "text", "text": text}));
            }
            ContentItem::Reasoning { text, signature }
                if message.role == MessageRole::Assistant =>
            {
                let Some(signature) = signature.as_deref().and_then(|signature| {
                    crate::compatible_provider_signature(
                        crate::SIGNATURE_PROVIDER_GEMINI,
                        signature,
                    )
                }) else {
                    continue;
                };
                let mut step = json!({
                    "type": "thought",
                    "content": [{"type": "text", "text": text}],
                });
                step["signature"] = Value::String(signature.to_string());
                steps.push(step);
            }
            ContentItem::StructuredJson { value } => {
                content.push(json!({"type": "text", "text": value.to_string()}));
            }
            ContentItem::Resource { resource } if message.role == MessageRole::User => {
                content.push(encode_interactions_resource(resource)?);
            }
            ContentItem::ToolCall { call } if message.role == MessageRole::Assistant => {
                steps.push(json!({
                    "type": "function_call",
                    "call_id": call.id,
                    "name": call.name,
                    "arguments": call.arguments,
                }));
            }
            ContentItem::Citation { .. } => {
                return Err(ProtocolConversionError::new(
                    "Gemini Interactions citations are output-only.",
                ));
            }
            ContentItem::ToolResult { .. }
            | ContentItem::ToolCall { .. }
            | ContentItem::Resource { .. }
            | ContentItem::Reasoning { .. } => {
                return Err(ProtocolConversionError::new(
                    "Gemini Interactions message content does not match its role.",
                ));
            }
        }
    }
    if !content.is_empty() {
        steps.insert(0, json!({"type": step_type, "content": content}));
    }
    Ok(steps)
}

/// Encodes one host-resolved Base64 resource into Interactions content.
/// 将一个宿主已解析的 Base64 资源编码为 Interactions 内容。
fn encode_interactions_resource(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<Value, ProtocolConversionError> {
    let media_type = resource.media_type.as_deref().ok_or_else(|| {
        ProtocolConversionError::new("Gemini Interactions resources require a MIME type.")
    })?;
    let url = resource.url.as_deref().ok_or_else(|| {
        ProtocolConversionError::new(
            "Gemini Interactions resources must be resolved before protocol projection.",
        )
    })?;
    let data = url
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
        .and_then(|(metadata, data)| metadata.ends_with(";base64").then_some(data))
        .ok_or_else(|| {
            ProtocolConversionError::new(
                "Gemini Interactions currently requires host-resolved Base64 resources.",
            )
        })?;
    let resource_type = match resource.kind {
        vulcan_ai_contract::ResourceKind::Image => "image",
        vulcan_ai_contract::ResourceKind::Audio => "audio",
        vulcan_ai_contract::ResourceKind::Video => "video",
        vulcan_ai_contract::ResourceKind::File => "document",
    };
    Ok(json!({
        "type": resource_type,
        "mime_type": media_type,
        "data": data,
    }))
}

/// Encodes public function tools into the native Interactions tool shape.
/// 将公共函数工具编码为原生 Interactions 工具结构。
fn encode_interactions_tools(request: &AiExecutionRequest) -> Vec<Value> {
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

/// Tracks one active streamed Interactions step.
/// 跟踪一个活动的 Interactions 流式 Step。
#[derive(Debug)]
enum ActiveInteractionStep {
    /// Model-visible output text.
    /// 模型可见输出文本。
    Text {
        /// Public response item identifier.
        /// 公共响应项标识。
        item_id: String,
    },
    /// Model reasoning summary.
    /// 模型推理摘要。
    Reasoning {
        /// Public reasoning item identifier.
        /// 公共推理项标识。
        item_id: String,
        /// Optional continuation signature.
        /// 可选延续签名。
        signature: Option<String>,
    },
    /// Function call whose JSON arguments may be fragmented.
    /// JSON 参数可能分片的函数调用。
    Tool {
        /// Public stream item identifier.
        /// 公共流项目标识。
        item_id: String,
        /// Provider call identifier.
        /// 供应商调用标识。
        call_id: String,
        /// Registered tool name.
        /// 已注册工具名称。
        name: String,
        /// Accumulated raw argument JSON.
        /// 累积的原始参数 JSON。
        arguments: String,
    },
}

/// Decodes revised Gemini Interactions SSE lifecycle events.
/// 解码修订版 Gemini Interactions SSE 生命周期事件。
struct GeminiInteractionsStreamDecoder {
    /// Stable public execution identifier.
    /// 稳定公共执行标识。
    execution_id: String,
    /// Whether the public response started.
    /// 公共响应是否已经启动。
    started: bool,
    /// Whether a terminal interaction event was observed.
    /// 是否已经观察到终态 Interaction 事件。
    terminal: bool,
    /// Active steps keyed by provider index.
    /// 按供应商索引保存的活动 Step。
    steps: BTreeMap<u32, ActiveInteractionStep>,
}

impl GeminiInteractionsStreamDecoder {
    /// Creates one fresh Interactions stream decoder.
    /// 创建一个新的 Interactions 流解码器。
    fn new(execution_id: String) -> Self {
        Self {
            execution_id,
            started: false,
            terminal: false,
            steps: BTreeMap::new(),
        }
    }

    /// Emits the public response start exactly once.
    /// 精确一次发送公共响应启动事件。
    fn start_if_needed(&mut self) -> Vec<AiStreamEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![AiStreamEvent::ResponseStarted {
            execution_id: self.execution_id.clone(),
        }]
    }

    /// Closes one active step and validates complete tool JSON.
    /// 关闭一个活动 Step 并校验完整工具 JSON。
    fn close_step(&mut self, index: u32) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let Some(step) = self.steps.remove(&index) else {
            return Ok(Vec::new());
        };
        match step {
            ActiveInteractionStep::Text { item_id } => {
                Ok(vec![AiStreamEvent::ResponseItemCompleted { item_id }])
            }
            ActiveInteractionStep::Reasoning { item_id, signature } => {
                Ok(vec![AiStreamEvent::ReasoningCompleted {
                    item_id,
                    signature: signature.and_then(|signature| {
                        crate::wrap_provider_signature(
                            crate::SIGNATURE_PROVIDER_GEMINI,
                            signature.as_str(),
                        )
                    }),
                }])
            }
            ActiveInteractionStep::Tool {
                item_id,
                call_id,
                name,
                arguments,
            } => match serde_json::from_str::<Value>(arguments.as_str()) {
                Ok(arguments) => Ok(vec![AiStreamEvent::ToolCallCompleted {
                    item_id,
                    call: ToolCall {
                        id: call_id,
                        name,
                        arguments,
                    },
                }]),
                Err(error) => Ok(vec![AiStreamEvent::ToolCallAborted {
                    item_id,
                    message: format!(
                        "Gemini Interactions tool arguments ended before valid JSON: {error}"
                    ),
                }]),
            },
        }
    }

    /// Closes every active step before a terminal event.
    /// 在终态事件前关闭每个活动 Step。
    fn close_all_steps(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let indices = self.steps.keys().copied().collect::<Vec<_>>();
        let mut events = Vec::new();
        for index in indices {
            events.extend(self.close_step(index)?);
        }
        Ok(events)
    }
}

impl ProtocolStreamDecoder for GeminiInteractionsStreamDecoder {
    /// Pushes one revised Interactions SSE record.
    /// 推入一条修订版 Interactions SSE 记录。
    fn push_sse(
        &mut self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if data.trim() == "[DONE]" {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<Value>(data).map_err(|error| {
            ProtocolConversionError::new(format!(
                "Gemini Interactions stream JSON is invalid: {error}"
            ))
        })?;
        let event_type = value
            .get("event_type")
            .and_then(Value::as_str)
            .or(event_name)
            .ok_or_else(|| {
                ProtocolConversionError::new("Gemini Interactions stream event type is missing.")
            })?;
        let mut events = self.start_if_needed();
        match event_type {
            "interaction.created" | "interaction.status_update" => {}
            "step.start" => {
                let index = required_u32(&value, "index")?;
                let step = value.get("step").ok_or_else(|| {
                    ProtocolConversionError::new(
                        "Gemini Interactions step.start is missing `step`.",
                    )
                })?;
                let step_type = step.get("type").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new(
                        "Gemini Interactions step.start is missing step type.",
                    )
                })?;
                if self.steps.contains_key(&index) {
                    return Err(ProtocolConversionError::new(format!(
                        "Gemini Interactions step index `{index}` started twice."
                    )));
                }
                let active = match step_type {
                    "model_output" => {
                        let item_id = format!("interactions:{}:text:{index}", self.execution_id);
                        events.push(AiStreamEvent::ResponseItemStarted {
                            item_id: item_id.clone(),
                            item: ContentItem::Text {
                                text: String::new(),
                            },
                        });
                        ActiveInteractionStep::Text { item_id }
                    }
                    "thought" => {
                        let item_id =
                            format!("interactions:{}:reasoning:{index}", self.execution_id);
                        events.push(AiStreamEvent::ReasoningStarted {
                            item_id: item_id.clone(),
                        });
                        ActiveInteractionStep::Reasoning {
                            item_id,
                            signature: step
                                .get("signature")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        }
                    }
                    "function_call" => {
                        let call_id = step
                            .get("call_id")
                            .or_else(|| step.get("id"))
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                ProtocolConversionError::new(
                                    "Gemini Interactions function call id is missing.",
                                )
                            })?
                            .to_string();
                        let name = step
                            .get("name")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                ProtocolConversionError::new(
                                    "Gemini Interactions function name is missing.",
                                )
                            })?
                            .to_string();
                        let item_id = format!("interactions:{}:tool:{index}", self.execution_id);
                        events.push(AiStreamEvent::ToolCallStarted {
                            item_id: item_id.clone(),
                            call_id: call_id.clone(),
                            tool_name: name.clone(),
                            index,
                        });
                        ActiveInteractionStep::Tool {
                            item_id,
                            call_id,
                            name,
                            arguments: String::new(),
                        }
                    }
                    other => {
                        return Err(ProtocolConversionError::new(format!(
                            "Gemini Interactions step type `{other}` is not registered."
                        )));
                    }
                };
                self.steps.insert(index, active);
            }
            "step.delta" => {
                let index = required_u32(&value, "index")?;
                let delta = value.get("delta").ok_or_else(|| {
                    ProtocolConversionError::new(
                        "Gemini Interactions step.delta is missing `delta`.",
                    )
                })?;
                let delta_type = delta.get("type").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new("Gemini Interactions step.delta type is missing.")
                })?;
                let step = self.steps.get_mut(&index).ok_or_else(|| {
                    ProtocolConversionError::new(format!(
                        "Gemini Interactions delta references unknown step `{index}`."
                    ))
                })?;
                match (step, delta_type) {
                    (ActiveInteractionStep::Text { item_id }, "text") => {
                        let text = delta.get("text").and_then(Value::as_str).ok_or_else(|| {
                            ProtocolConversionError::new(
                                "Gemini Interactions text delta is missing text.",
                            )
                        })?;
                        events.push(AiStreamEvent::ResponseTextDelta {
                            item_id: item_id.clone(),
                            delta: text.to_string(),
                        });
                    }
                    (ActiveInteractionStep::Reasoning { item_id, .. }, "thought_summary") => {
                        let text = delta
                            .pointer("/content/text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                ProtocolConversionError::new(
                                    "Gemini Interactions thought summary is missing text.",
                                )
                            })?;
                        events.push(AiStreamEvent::ReasoningDelta {
                            item_id: item_id.clone(),
                            delta: text.to_string(),
                        });
                    }
                    (ActiveInteractionStep::Reasoning { signature, .. }, "thought_signature") => {
                        *signature = Some(
                            delta
                                .get("signature")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    ProtocolConversionError::new(
                                        "Gemini Interactions thought signature is missing.",
                                    )
                                })?
                                .to_string(),
                        );
                    }
                    (
                        ActiveInteractionStep::Tool {
                            item_id,
                            call_id,
                            name,
                            arguments,
                        },
                        "arguments_delta",
                    ) => {
                        let fragment =
                            delta
                                .get("arguments")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    ProtocolConversionError::new(
                                        "Gemini Interactions argument delta is missing arguments.",
                                    )
                                })?;
                        arguments.push_str(fragment);
                        events.push(AiStreamEvent::ToolCallArgumentsDelta {
                            item_id: item_id.clone(),
                            delta: fragment.to_string(),
                            progress: ToolCallProgress {
                                call_id: Some(call_id.clone()),
                                tool_name: Some(name.clone()),
                                bytes_received: u64::try_from(arguments.len()).map_err(|_| {
                                    ProtocolConversionError::new(
                                        "Gemini Interactions argument byte count overflowed.",
                                    )
                                })?,
                                characters_received: u64::try_from(arguments.chars().count())
                                    .map_err(|_| {
                                        ProtocolConversionError::new(
                                            "Gemini Interactions argument character count overflowed.",
                                        )
                                    })?,
                                complete: false,
                            },
                        });
                    }
                    (_, other) => {
                        return Err(ProtocolConversionError::new(format!(
                            "Gemini Interactions delta `{other}` does not match step `{index}`."
                        )));
                    }
                }
            }
            "step.stop" => {
                events.extend(self.close_step(required_u32(&value, "index")?)?);
            }
            "interaction.completed" | "finish" => {
                events.extend(self.close_all_steps()?);
                let usage = value
                    .pointer("/interaction/usage")
                    .or_else(|| value.pointer("/metadata/total_usage"));
                if let Some(usage) = usage {
                    events.push(AiStreamEvent::UsageUpdated {
                        usage: interactions_usage(usage),
                    });
                }
                self.terminal = true;
                events.push(AiStreamEvent::ResponseCompleted);
            }
            "interaction.failed" => {
                events.extend(self.close_all_steps()?);
                self.terminal = true;
                events.push(AiStreamEvent::ResponseFailed {
                    error: vulcan_ai_contract::AiExecutionError {
                        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
                        subcategory: None,
                        stage: String::from("stream"),
                        http_status: None,
                        upstream_code: value
                            .pointer("/interaction/error/code")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        message: value
                            .pointer("/interaction/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("Gemini Interactions execution failed.")
                            .to_string(),
                        retryable: false,
                        retry_after_seconds: None,
                        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
                        semantic_output_observed: false,
                        terminal: true,
                    },
                });
            }
            "interaction.cancelled" => {
                events.extend(self.close_all_steps()?);
                self.terminal = true;
                events.push(AiStreamEvent::ResponseCancelled);
            }
            "done" => {}
            other => {
                return Err(ProtocolConversionError::new(format!(
                    "Gemini Interactions SSE event type `{other}` is not registered."
                )));
            }
        }
        Ok(events)
    }

    /// Rejects streams that end without a terminal interaction event.
    /// 拒绝未出现终态 Interaction 事件就结束的流。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            Ok(Vec::new())
        } else {
            Err(ProtocolConversionError::new(
                "Gemini Interactions stream ended before a terminal event.",
            ))
        }
    }
}

/// Reads one required non-negative step index.
/// 读取一个必需的非负 Step 索引。
fn required_u32(value: &Value, field: &str) -> Result<u32, ProtocolConversionError> {
    let raw = value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        ProtocolConversionError::new(format!("Gemini Interactions event is missing `{field}`."))
    })?;
    u32::try_from(raw).map_err(|_| {
        ProtocolConversionError::new(format!("Gemini Interactions `{field}` exceeds u32."))
    })
}

/// Normalizes Interactions usage without deriving uncached input from cached totals.
/// 归一化 Interactions 用量，且不从缓存总量推导非缓存输入。
fn interactions_usage(value: &Value) -> RequestUsage {
    RequestUsage {
        input_tokens: reported_usage(value, "total_input_tokens"),
        output_tokens: reported_usage(value, "total_output_tokens"),
        reasoning_tokens: reported_usage(value, "total_thought_tokens"),
        cache_read_tokens: reported_usage(value, "total_cached_tokens"),
        total_tokens: reported_usage(value, "total_tokens"),
        ..RequestUsage::default()
    }
}

/// Converts one optional reported usage integer.
/// 转换一个可选的已报告用量整数。
fn reported_usage(value: &Value, field: &str) -> UsageValue {
    value
        .get(field)
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
    use vulcan_ai_contract::{AiOperation, ToolDefinition};

    /// Builds one request that exercises instructions, tools, and tool continuation.
    /// 构建一个覆盖 Instructions、工具与工具延续的请求。
    fn request() -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("gemini-test"),
            cache_scope_id: None,
            messages: vec![
                AiMessage {
                    role: MessageRole::System,
                    content: vec![ContentItem::Text {
                        text: String::from("Be concise."),
                    }],
                },
                AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: String::from("Look up x."),
                    }],
                },
                AiMessage {
                    role: MessageRole::Assistant,
                    content: vec![ContentItem::ToolCall {
                        call: ToolCall {
                            id: String::from("call"),
                            name: String::from("lookup"),
                            arguments: json!({"q": "x"}),
                        },
                    }],
                },
                AiMessage {
                    role: MessageRole::Tool,
                    content: vec![ContentItem::ToolResult {
                        result: vulcan_ai_contract::ToolResult {
                            call_id: String::from("call"),
                            is_error: false,
                            content: json!({"value": 1}),
                        },
                    }],
                },
            ],
            tools: vec![ToolDefinition {
                name: String::from("lookup"),
                description: String::from("Looks up a value."),
                input_schema: json!({"type": "object"}),
            }],
            native_tools: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(256),
        }
    }

    /// Verifies revised Interactions request fields remain separate from GenerateContent.
    /// 验证修订版 Interactions 请求字段与 GenerateContent 保持分离。
    #[test]
    fn encodes_revised_interactions_request() {
        let mut request = request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::High);
        let encoded = GeminiInteractionsProfile
            .encode(&request)
            .expect("Interactions request should encode");
        assert_eq!(encoded.path, "/interactions");
        assert_eq!(encoded.body["system_instruction"], "Be concise.");
        assert_eq!(encoded.body["input"][0]["type"], "user_input");
        assert_eq!(encoded.body["input"][1]["type"], "function_call");
        assert_eq!(encoded.body["input"][2]["type"], "function_result");
        assert_eq!(encoded.body["tools"][0]["name"], "lookup");
        assert_eq!(encoded.body["generation_config"]["thinking_level"], "high");
        assert_eq!(
            encoded.body["generation_config"]["thinking_summaries"],
            "auto"
        );
        assert_eq!(
            encoded.headers.get("api-revision").map(String::as_str),
            Some(GEMINI_INTERACTIONS_API_REVISION)
        );
    }

    /// Verifies Interactions rejects a public effort that has no native wire value.
    /// 验证 Interactions 会拒绝不存在原生传输值的公共推理档位。
    #[test]
    fn rejects_unsupported_reasoning_effort() {
        let mut request = request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Ultra);
        let error = GeminiInteractionsProfile
            .encode(&request)
            .expect_err("unsupported effort should fail before dispatch");
        assert!(
            error
                .to_string()
                .contains("does not expose reasoning effort")
        );
    }

    /// Verifies text, tool arguments, usage, and terminal events decode in order.
    /// 验证文本、工具参数、用量与终态事件按序解码。
    #[test]
    fn decodes_revised_interactions_stream() {
        let mut decoder = GeminiInteractionsStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("interaction.created"),
                r#"{"event_type":"interaction.created","interaction":{"id":"i"}}"#,
            )
            .expect("interaction should start");
        decoder
            .push_sse(
                Some("step.start"),
                r#"{"event_type":"step.start","index":0,"step":{"type":"function_call","id":"call","name":"lookup"}}"#,
            )
            .expect("tool should start");
        let delta = decoder
            .push_sse(
                Some("step.delta"),
                r#"{"event_type":"step.delta","index":0,"delta":{"type":"arguments_delta","arguments":"{\"q\":\"x\"}"}}"#,
            )
            .expect("arguments should decode");
        assert!(delta.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallArgumentsDelta { progress, .. }
                if progress.bytes_received == 9
        )));
        let stop = decoder
            .push_sse(Some("step.stop"), r#"{"event_type":"step.stop","index":0}"#)
            .expect("tool should complete");
        assert!(stop.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. } if call.name == "lookup"
        )));
        let completed = decoder
            .push_sse(
                Some("interaction.completed"),
                r#"{"event_type":"interaction.completed","interaction":{"status":"completed","usage":{"total_input_tokens":2,"total_output_tokens":3,"total_cached_tokens":1,"total_tokens":5}}}"#,
            )
            .expect("interaction should complete");
        assert!(
            completed
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
        decoder.finish().expect("terminal stream should finish");
    }
}
