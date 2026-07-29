//! OpenAI Chat Completions and Responses stream decoders.
//! OpenAI Chat Completions 与 Responses 流解码器。

use crate::{
    ProtocolConversionError, ProtocolStreamDecoder, merge_streamed_tool_name,
    shared::append_bounded_tool_arguments,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiContinuationUpdate, AiErrorCategory, AiExecutionError, AiStreamEvent, ContentItem,
    RequestUsage, ResourceKind, ResourceReference, ResourceTransferState, ToolCall,
    ToolCallProgress, UsageValue, UsageValueSource, derive_uncached_input,
};

/// Tracks one OpenAI-compatible streamed tool call.
/// 跟踪一个 OpenAI 兼容流式工具调用。
#[derive(Debug, Default)]
struct OpenAiToolState {
    /// Stable public stream item identifier.
    /// 稳定公共流项目标识。
    item_id: String,
    /// Upstream call identifier.
    /// 上游调用标识。
    call_id: String,
    /// Tool name.
    /// 工具名称。
    tool_name: String,
    /// Accumulated raw argument JSON.
    /// 累积的原始参数 JSON。
    arguments: String,
    /// Whether the public start event was emitted.
    /// 是否已经发送公共开始事件。
    started: bool,
    /// Whether the public terminal tool event was emitted.
    /// 是否已经发送公共工具终态事件。
    terminal: bool,
}

/// Decodes OpenAI-compatible Chat Completions SSE chunks.
/// 解码 OpenAI 兼容 Chat Completions SSE 分片。
#[derive(Debug)]
pub(crate) struct OpenAiChatStreamDecoder {
    /// Stable execution identifier.
    /// 稳定执行标识。
    execution_id: String,
    /// Whether the public response start event was emitted.
    /// 是否已经发送公共响应开始事件。
    started: bool,
    /// Whether a terminal response event was emitted.
    /// 是否已经发送响应终态事件。
    terminal: bool,
    /// Whether a provider finish reason already established semantic completion.
    /// 供应商结束原因是否已经建立语义完成状态。
    finish_signal_seen: bool,
    /// Whether the text item is currently open.
    /// 文本项当前是否打开。
    text_open: bool,
    /// Whether the reasoning item is currently open.
    /// 推理项当前是否打开。
    reasoning_open: bool,
    /// Last cumulative MiniMax `reasoning_details` text.
    /// 最近一次累计的 MiniMax `reasoning_details` 文本。
    reasoning_details_text: String,
    /// Tool calls keyed by stable parallel index.
    /// 按稳定并行索引保存的工具调用。
    tools: BTreeMap<u32, OpenAiToolState>,
    /// Exact tool names frozen from the validated request.
    /// 从已校验请求冻结的精确工具名称。
    registered_tools: BTreeSet<String>,
}

impl OpenAiChatStreamDecoder {
    /// Creates one Chat Completions decoder.
    /// 创建一个 Chat Completions 解码器。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the stable public execution identifier.
    /// `execution_id` 是稳定公共执行标识。
    ///
    /// # Returns
    /// 返回值
    /// A fresh decoder.
    /// 返回新的解码器。
    /// Creates one Chat Completions decoder with an exact registered-tool snapshot.
    /// 使用精确的已注册工具快照创建 Chat Completions 解码器。
    pub(crate) fn with_registered_tools(
        execution_id: String,
        registered_tools: BTreeSet<String>,
    ) -> Self {
        Self {
            execution_id,
            started: false,
            terminal: false,
            finish_signal_seen: false,
            text_open: false,
            reasoning_open: false,
            reasoning_details_text: String::new(),
            tools: BTreeMap::new(),
            registered_tools,
        }
    }

    /// Starts the public response once.
    /// 仅启动一次公共响应。
    fn start_if_needed(&mut self) -> Vec<AiStreamEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![AiStreamEvent::ResponseStarted {
            execution_id: self.execution_id.clone(),
        }]
    }

    /// Decodes one Chat Completions choice delta.
    /// 解码一个 Chat Completions 选择增量。
    fn decode_choice(
        &mut self,
        choice: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let mut events = Vec::new();
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            let item_id = format!("openai:{}:text", self.execution_id);
            if !self.text_open {
                self.text_open = true;
                events.push(AiStreamEvent::ResponseItemStarted {
                    item_id: item_id.clone(),
                    item: ContentItem::Text {
                        text: String::new(),
                    },
                });
            }
            if !text.is_empty() {
                events.push(AiStreamEvent::ResponseTextDelta {
                    item_id,
                    delta: text.to_string(),
                });
            }
        }
        let direct_reasoning = delta
            .get("reasoning_content")
            .and_then(Value::as_str)
            .or_else(|| delta.get("reasoning").and_then(Value::as_str));
        let reasoning = direct_reasoning
            .map(str::to_string)
            .or_else(|| self.decode_reasoning_details_delta(delta));
        if let Some(reasoning) = reasoning {
            let item_id = format!("openai:{}:reasoning", self.execution_id);
            if !self.reasoning_open {
                self.reasoning_open = true;
                events.push(AiStreamEvent::ReasoningStarted {
                    item_id: item_id.clone(),
                });
            }
            if !reasoning.is_empty() {
                events.push(AiStreamEvent::ReasoningDelta {
                    item_id,
                    delta: reasoning,
                });
            }
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_delta in tool_calls {
                events.extend(self.decode_tool_delta(tool_delta)?);
            }
        }
        if choice
            .get("finish_reason")
            .is_some_and(|value| !value.is_null())
        {
            self.finish_signal_seen = true;
            events.extend(self.close_open_items());
            events.extend(self.complete_tools());
        }
        Ok(events)
    }

    /// Decodes MiniMax cumulative `reasoning_details` into one true text delta.
    /// 将 MiniMax 累计式 `reasoning_details` 解码为真实文本增量。
    ///
    /// # Parameters
    /// 参数
    /// `delta` is one OpenAI-compatible choice delta.
    /// `delta` 是一个 OpenAI 兼容选择增量。
    ///
    /// # Returns
    /// 返回值
    /// A newly observed reasoning suffix, or `None` when the field is absent.
    /// 返回新观测到的推理后缀；字段缺失时返回 `None`。
    fn decode_reasoning_details_delta(&mut self, delta: &Value) -> Option<String> {
        let cumulative = delta
            .get("reasoning_details")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(|detail| detail.get("text").and_then(Value::as_str))
            .collect::<String>();
        if cumulative.starts_with(self.reasoning_details_text.as_str()) {
            let suffix = cumulative[self.reasoning_details_text.len()..].to_string();
            self.reasoning_details_text = cumulative;
            return Some(suffix);
        }
        self.reasoning_details_text.push_str(cumulative.as_str());
        Some(cumulative)
    }

    /// Decodes one indexed Chat Completions tool-call delta.
    /// 解码一个带索引的 Chat Completions 工具调用增量。
    fn decode_tool_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index_number = value
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| ProtocolConversionError::new("OpenAI tool call index is missing."))?;
        let index = u32::try_from(index_number)
            .map_err(|_| ProtocolConversionError::new("OpenAI tool call index exceeds u32."))?;
        let state = self.tools.entry(index).or_insert_with(|| OpenAiToolState {
            item_id: format!("openai:{}:tool:{index}", self.execution_id),
            ..OpenAiToolState::default()
        });
        if let Some(call_id) = value.get("id").and_then(Value::as_str) {
            state.call_id.push_str(call_id);
        }
        if let Some(name) = value.pointer("/function/name").and_then(Value::as_str) {
            state.tool_name =
                merge_streamed_tool_name(state.tool_name.as_str(), name, &self.registered_tools);
        }
        let argument_delta = value
            .pointer("/function/arguments")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut events = Vec::new();
        if !state.started
            && !state.call_id.is_empty()
            && self.registered_tools.contains(&state.tool_name)
        {
            state.started = true;
            events.push(AiStreamEvent::ToolCallStarted {
                item_id: state.item_id.clone(),
                call_id: state.call_id.clone(),
                tool_name: state.tool_name.clone(),
                index,
            });
            if !state.arguments.is_empty() {
                events.push(tool_argument_delta(state, state.arguments.as_str())?);
            }
        }
        if !argument_delta.is_empty() {
            append_bounded_tool_arguments(&mut state.arguments, argument_delta, "OpenAI Chat")?;
            if state.started {
                events.push(tool_argument_delta(state, argument_delta)?);
            }
        }
        Ok(events)
    }

    /// Closes currently open text and reasoning items.
    /// 关闭当前打开的文本与推理项。
    fn close_open_items(&mut self) -> Vec<AiStreamEvent> {
        let mut events = Vec::new();
        if self.reasoning_open {
            self.reasoning_open = false;
            events.push(AiStreamEvent::ReasoningCompleted {
                item_id: format!("openai:{}:reasoning", self.execution_id),
                signature: None,
            });
        }
        if self.text_open {
            self.text_open = false;
            events.push(AiStreamEvent::ResponseItemCompleted {
                item_id: format!("openai:{}:text", self.execution_id),
            });
        }
        events
    }

    /// Completes every started tool call after the provider finish signal.
    /// 在供应商结束信号后完成每个已开始工具调用。
    fn complete_tools(&mut self) -> Vec<AiStreamEvent> {
        let mut events = Vec::new();
        for state in self.tools.values_mut().filter(|state| !state.terminal) {
            state.terminal = true;
            if !state.started {
                events.push(AiStreamEvent::ToolCallAborted {
                    item_id: state.item_id.clone(),
                    message: String::from(
                        "OpenAI tool input ended before the tool identity was complete.",
                    ),
                });
                continue;
            }
            match serde_json::from_str::<Value>(state.arguments.as_str()) {
                Ok(arguments) => events.push(AiStreamEvent::ToolCallCompleted {
                    item_id: state.item_id.clone(),
                    call: ToolCall {
                        id: state.call_id.clone(),
                        name: state.tool_name.clone(),
                        arguments,
                        signature: None,
                    },
                }),
                Err(error) => events.push(AiStreamEvent::ToolCallAborted {
                    item_id: state.item_id.clone(),
                    message: format!(
                        "OpenAI tool input ended before valid JSON was complete: {error}"
                    ),
                }),
            }
        }
        events
    }
}

impl ProtocolStreamDecoder for OpenAiChatStreamDecoder {
    fn push_sse(
        &mut self,
        _event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            return Err(ProtocolConversionError::new(
                "OpenAI Chat emitted data after a terminal event.",
            ));
        }
        let mut events = self.start_if_needed();
        if data.trim() == "[DONE]" {
            events.extend(self.close_open_items());
            events.extend(self.complete_tools());
            self.terminal = true;
            events.push(AiStreamEvent::ResponseCompleted);
            return Ok(events);
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            ProtocolConversionError::new(format!("OpenAI Chat SSE JSON is invalid: {error}"))
        })?;
        if let Some(error) = value.get("error") {
            self.terminal = true;
            events.push(AiStreamEvent::ResponseFailed {
                error: normalize_openai_error(error, value.get("error_type")),
            });
            return Ok(events);
        }
        if let Some(usage) = value.get("usage") {
            events.push(AiStreamEvent::UsageUpdated {
                usage: openai_usage(usage),
            });
        }
        if let Some(choices) = value.get("choices").and_then(Value::as_array) {
            for choice in choices {
                events.extend(self.decode_choice(choice)?);
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            return Ok(Vec::new());
        }
        if self.finish_signal_seen {
            self.terminal = true;
            return Ok(vec![AiStreamEvent::ResponseCompleted]);
        }
        Err(ProtocolConversionError::new(
            "OpenAI Chat stream ended before the [DONE] terminal record.",
        ))
    }
}

/// Decodes OpenAI Responses SSE events.
/// 解码 OpenAI Responses SSE 事件。
#[derive(Debug)]
pub(crate) struct OpenAiResponsesStreamDecoder {
    /// Stable execution identifier.
    /// 稳定执行标识。
    execution_id: String,
    /// Whether the public response start event was emitted.
    /// 是否已经发送公共响应开始事件。
    started: bool,
    /// Whether a terminal response event was emitted.
    /// 是否已经发送响应终态事件。
    terminal: bool,
    /// Stable upstream response identifier used by a later continuation request.
    /// 后续延续请求使用的稳定上游响应标识。
    response_id: Option<String>,
    /// Open text items keyed by output index.
    /// 按输出索引保存的打开文本项。
    text_items: BTreeMap<u32, String>,
    /// Open reasoning items keyed by summary index.
    /// 按摘要索引保存的打开推理项。
    reasoning_items: BTreeMap<u32, String>,
    /// Encrypted reasoning replay content reported by the completed reasoning item.
    /// 已完成推理项报告的加密推理重放内容。
    reasoning_signature: Option<String>,
    /// Tool calls keyed by output index.
    /// 按输出索引保存的工具调用。
    tools: BTreeMap<u32, OpenAiToolState>,
    /// Image-generation item identifiers keyed by output index.
    /// 按输出索引保存的图片生成项标识。
    image_items: BTreeMap<u32, String>,
    /// Image-generation output indices that already reached their item completion event.
    /// 已到达项完成事件的图片生成输出索引。
    completed_images: BTreeSet<u32>,
}

impl OpenAiResponsesStreamDecoder {
    /// Creates one Responses decoder.
    /// 创建一个 Responses 解码器。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the stable public execution identifier.
    /// `execution_id` 是稳定公共执行标识。
    ///
    /// # Returns
    /// 返回值
    /// A fresh decoder.
    /// 返回新的解码器。
    pub(crate) fn new(execution_id: String) -> Self {
        Self {
            execution_id,
            started: false,
            terminal: false,
            response_id: None,
            text_items: BTreeMap::new(),
            reasoning_items: BTreeMap::new(),
            reasoning_signature: None,
            tools: BTreeMap::new(),
            image_items: BTreeMap::new(),
            completed_images: BTreeSet::new(),
        }
    }

    /// Starts the public response once.
    /// 仅启动一次公共响应。
    fn start_if_needed(&mut self) -> Vec<AiStreamEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![AiStreamEvent::ResponseStarted {
            execution_id: self.execution_id.clone(),
        }]
    }

    /// Captures one immutable upstream response identifier and emits it exactly once.
    /// 捕获一个不可变上游响应标识并且仅发送一次。
    ///
    /// # Parameters
    /// 参数
    /// `value` is one Responses lifecycle event that may carry `/response/id`.
    /// `value` 是可能携带 `/response/id` 的 Responses 生命周期事件。
    ///
    /// # Returns
    /// 返回值
    /// A metadata update for the first identifier, no event for a duplicate, or an error for mutation.
    /// 首次标识返回元数据更新；重复标识不返回事件；标识变化时返回错误。
    fn capture_response_id(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let Some(response_id) = value
            .pointer("/response/id")
            .or_else(|| value.get("response_id"))
            .and_then(Value::as_str)
            .filter(|response_id| !response_id.trim().is_empty())
        else {
            return Ok(Vec::new());
        };
        if let Some(existing) = self.response_id.as_deref() {
            if existing != response_id {
                return Err(ProtocolConversionError::new(
                    "OpenAI Responses changed response identity during one stream.",
                ));
            }
            return Ok(Vec::new());
        }
        self.response_id = Some(response_id.to_string());
        Ok(vec![AiStreamEvent::ResponseMetadataUpdated {
            continuation: AiContinuationUpdate {
                provider_message_id: response_id.to_string(),
            },
        }])
    }

    /// Returns a required bounded output index.
    /// 返回必需且有界的输出索引。
    fn output_index(value: &Value) -> Result<u32, ProtocolConversionError> {
        let number = value
            .get("output_index")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                ProtocolConversionError::new("OpenAI Responses output_index is missing.")
            })?;
        u32::try_from(number)
            .map_err(|_| ProtocolConversionError::new("OpenAI Responses output_index exceeds u32."))
    }

    /// Decodes a response output-item add event.
    /// 解码响应输出项添加事件。
    fn output_item_added(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = Self::output_index(value)?;
        let item = value
            .get("item")
            .ok_or_else(|| ProtocolConversionError::new("OpenAI Responses item is missing."))?;
        if item.get("type").and_then(Value::as_str) == Some("image_generation_call") {
            if self.completed_images.contains(&index) {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Responses image output index `{index}` was added after completion."
                )));
            }
            let reported_item_id = item.get("id").and_then(Value::as_str);
            self.image_item_id(index, reported_item_id)?;
            return Ok(Vec::new());
        }
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            return Ok(Vec::new());
        }
        let call_id = item
            .get("call_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolConversionError::new("OpenAI Responses call_id is missing."))?
            .to_string();
        let tool_name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolConversionError::new("OpenAI Responses tool name is missing."))?
            .to_string();
        let item_id = item
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("responses:{}:tool:{index}", self.execution_id));
        self.tools.insert(
            index,
            OpenAiToolState {
                item_id: item_id.clone(),
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                arguments: item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                started: true,
                terminal: false,
            },
        );
        Ok(vec![AiStreamEvent::ToolCallStarted {
            item_id,
            call_id,
            tool_name,
            index,
        }])
    }

    /// Decodes a response function-call argument delta.
    /// 解码响应函数调用参数增量。
    fn tool_argument_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = Self::output_index(value)?;
        let delta = value.get("delta").and_then(Value::as_str).ok_or_else(|| {
            ProtocolConversionError::new("OpenAI Responses argument delta is missing.")
        })?;
        let state = self.tools.get_mut(&index).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "OpenAI Responses argument delta references unopened index `{index}`."
            ))
        })?;
        append_bounded_tool_arguments(&mut state.arguments, delta, "OpenAI Responses")?;
        Ok(vec![tool_argument_delta(state, delta)?])
    }

    /// Completes one response function call.
    /// 完成一个响应函数调用。
    fn complete_tool(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = Self::output_index(value)?;
        let state = self.tools.get_mut(&index).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "OpenAI Responses tool completion references unopened index `{index}`."
            ))
        })?;
        if state.terminal {
            return Ok(Vec::new());
        }
        if let Some(arguments) = value.get("arguments").and_then(Value::as_str) {
            state.arguments.clear();
            append_bounded_tool_arguments(&mut state.arguments, arguments, "OpenAI Responses")?;
        }
        state.terminal = true;
        match serde_json::from_str::<Value>(state.arguments.as_str()) {
            Ok(arguments) => Ok(vec![AiStreamEvent::ToolCallCompleted {
                item_id: state.item_id.clone(),
                call: ToolCall {
                    id: state.call_id.clone(),
                    name: state.tool_name.clone(),
                    arguments,
                    signature: None,
                },
            }]),
            Err(error) => Ok(vec![AiStreamEvent::ToolCallAborted {
                item_id: state.item_id.clone(),
                message: format!(
                    "OpenAI Responses tool input ended before valid JSON was complete: {error}"
                ),
            }]),
        }
    }

    /// Closes any remaining semantic items before terminal response events.
    /// 在响应终态事件前关闭所有剩余语义项。
    fn close_semantic_items(&mut self) -> Vec<AiStreamEvent> {
        let reasoning_count = self.reasoning_items.len();
        let mut events = self
            .reasoning_items
            .values()
            .cloned()
            .enumerate()
            .map(|(index, item_id)| AiStreamEvent::ReasoningCompleted {
                item_id,
                signature: (index + 1 == reasoning_count)
                    .then(|| self.reasoning_signature.clone())
                    .flatten()
                    .and_then(|signature| {
                        crate::wrap_provider_signature(
                            crate::SIGNATURE_PROVIDER_OPENAI,
                            signature.as_str(),
                        )
                    }),
            })
            .collect::<Vec<_>>();
        events.extend(
            self.text_items
                .values()
                .cloned()
                .map(|item_id| AiStreamEvent::ResponseItemCompleted { item_id }),
        );
        self.reasoning_items.clear();
        self.reasoning_signature = None;
        self.text_items.clear();
        events
    }

    /// Aborts every tool call that did not receive its dedicated completion event.
    /// 中止所有未收到专用完成事件的工具调用。
    ///
    /// # Parameters
    /// 参数
    /// `terminal_event` identifies the provider terminal event that closed the response.
    /// `terminal_event` 标识关闭响应的供应商终态事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns deterministic abort events for every still-open tool call.
    /// 返回每个仍处于打开状态工具调用的确定性中止事件。
    fn abort_incomplete_tools(&mut self, terminal_event: &str) -> Vec<AiStreamEvent> {
        let mut events = Vec::new();
        for state in self.tools.values_mut().filter(|state| !state.terminal) {
            state.terminal = true;
            events.push(AiStreamEvent::ToolCallAborted {
                item_id: state.item_id.clone(),
                message: format!(
                    "OpenAI Responses emitted `{terminal_event}` before the tool input completion event."
                ),
            });
        }
        events
    }

    /// Fails every image-generation resource that did not receive a completed output item.
    /// 将所有未收到完成输出项的图片生成资源标记为失败。
    ///
    /// # Parameters
    /// 参数
    /// `terminal_event` identifies the provider terminal event that closed the response.
    /// `terminal_event` 标识关闭响应的供应商终态事件。
    ///
    /// # Returns
    /// 返回值
    /// Deterministic failed progress events for every still-open image resource.
    /// 返回每个仍打开图片资源的确定性失败进度事件。
    fn fail_incomplete_images(&mut self, terminal_event: &str) -> Vec<AiStreamEvent> {
        let events = self
            .image_items
            .iter()
            .filter(|(index, _)| !self.completed_images.contains(index))
            .map(|(_, item_id)| {
                AiStreamEvent::ResourceProgress {
                    resource_id: format!("{item_id}:resource"),
                    state: ResourceTransferState::Failed {
                        message: format!(
                            "OpenAI Responses emitted `{terminal_event}` before the image output item completed."
                        ),
                    },
                }
            })
            .collect();
        self.image_items.clear();
        self.completed_images.clear();
        events
    }

    /// Decodes one completed Responses image-generation item without exposing provider payloads as text.
    /// 解码一项已完成的 Responses 图片生成结果，且不把供应商载荷暴露为文本。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the exact `response.output_item.done` event.
    /// `value` 是精确的 `response.output_item.done` 事件。
    ///
    /// # Returns
    /// 返回值
    /// A provider-neutral resource lifecycle when the item is an image result, otherwise no events.
    /// 当该项为图片结果时返回供应商无关资源生命周期，否则不返回事件。
    fn completed_image_item(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let item = value
            .get("item")
            .ok_or_else(|| ProtocolConversionError::new("OpenAI Responses item is missing."))?;
        if item.get("type").and_then(Value::as_str) != Some("image_generation_call") {
            return Ok(Vec::new());
        }
        let index = Self::output_index(value)?;
        let item_id = self.image_item_id(index, item.get("id").and_then(Value::as_str))?;
        if self.completed_images.contains(&index) {
            return Ok(Vec::new());
        }
        let result = item
            .get("result")
            .and_then(Value::as_str)
            .filter(|result| !result.is_empty())
            .ok_or_else(|| {
                ProtocolConversionError::new("OpenAI Responses image-generation result is missing.")
            })?;
        let media_type = openai_image_media_type(
            item.get("output_format")
                .and_then(Value::as_str)
                .unwrap_or("png"),
        )?;
        let resource_id = format!("{item_id}:resource");
        self.completed_images.insert(index);
        Ok(vec![
            AiStreamEvent::ResponseItemStarted {
                item_id: item_id.clone(),
                item: ContentItem::Resource {
                    resource: ResourceReference {
                        id: resource_id.clone(),
                        kind: ResourceKind::Image,
                        media_type: Some(media_type.to_string()),
                        local_path: None,
                        url: Some(format!("data:{media_type};base64,{result}")),
                    },
                },
            },
            AiStreamEvent::ResourceProgress {
                resource_id,
                state: ResourceTransferState::Completed,
            },
            AiStreamEvent::ResponseItemCompleted { item_id },
        ])
    }

    /// Resolves one stable image item identity across added, partial, and completed events.
    /// 在添加、部分进度与完成事件之间解析一个稳定的图片项身份。
    ///
    /// # Parameters
    /// 参数
    /// `index` is the exact Responses output index and `reported_item_id` is the optional upstream identity.
    /// `index` 是精确的 Responses 输出索引，`reported_item_id` 是可选的上游身份。
    ///
    /// # Returns
    /// 返回值
    /// The stable item identifier, or a protocol error when upstream identities conflict.
    /// 返回稳定项标识；上游身份冲突时返回协议错误。
    fn image_item_id(
        &mut self,
        index: u32,
        reported_item_id: Option<&str>,
    ) -> Result<String, ProtocolConversionError> {
        let reported_item_id = reported_item_id.filter(|item_id| !item_id.is_empty());
        if let Some(existing) = self.image_items.get(&index) {
            if reported_item_id.is_some_and(|reported| reported != existing) {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Responses image output index `{index}` changed item identity."
                )));
            }
            return Ok(existing.clone());
        }
        let item_id = reported_item_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("responses:{}:image:{index}", self.execution_id));
        self.image_items.insert(index, item_id.clone());
        Ok(item_id)
    }
}

impl ProtocolStreamDecoder for OpenAiResponsesStreamDecoder {
    fn push_sse(
        &mut self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses emitted data after a terminal event.",
            ));
        }
        if data.trim() == "[DONE]" {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            ProtocolConversionError::new(format!("OpenAI Responses SSE JSON is invalid: {error}"))
        })?;
        let wire_type = value
            .get("type")
            .and_then(Value::as_str)
            .or(event_name)
            .ok_or_else(|| {
                ProtocolConversionError::new("OpenAI Responses SSE event type is missing.")
            })?;
        let mut events = self.start_if_needed();
        match wire_type {
            "response.created" | "response.in_progress" => {
                events.extend(self.capture_response_id(&value)?);
            }
            "response.output_item.added" => events.extend(self.output_item_added(&value)?),
            "response.output_text.delta" => {
                let index = Self::output_index(&value)?;
                let is_new = !self.text_items.contains_key(&index);
                let item_id = self
                    .text_items
                    .entry(index)
                    .or_insert_with(|| format!("responses:{}:text:{index}", self.execution_id))
                    .clone();
                if is_new {
                    events.push(AiStreamEvent::ResponseItemStarted {
                        item_id: item_id.clone(),
                        item: ContentItem::Text {
                            text: String::new(),
                        },
                    });
                }
                let delta = value.get("delta").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new("OpenAI Responses output text delta is missing.")
                })?;
                events.push(AiStreamEvent::ResponseTextDelta {
                    item_id,
                    delta: delta.to_string(),
                });
            }
            "response.reasoning_summary_text.delta" => {
                let index_number = value
                    .get("summary_index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        ProtocolConversionError::new(
                            "OpenAI Responses reasoning summary index is missing.",
                        )
                    })?;
                let index = u32::try_from(index_number).map_err(|_| {
                    ProtocolConversionError::new(
                        "OpenAI Responses reasoning summary index exceeds u32.",
                    )
                })?;
                let is_new = !self.reasoning_items.contains_key(&index);
                let item_id = self
                    .reasoning_items
                    .entry(index)
                    .or_insert_with(|| format!("responses:{}:reasoning:{index}", self.execution_id))
                    .clone();
                if is_new {
                    events.push(AiStreamEvent::ReasoningStarted {
                        item_id: item_id.clone(),
                    });
                }
                let delta = value.get("delta").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new("OpenAI Responses reasoning delta is missing.")
                })?;
                events.push(AiStreamEvent::ReasoningDelta {
                    item_id,
                    delta: delta.to_string(),
                });
            }
            "response.function_call_arguments.delta" => {
                events.extend(self.tool_argument_delta(&value)?);
            }
            "response.function_call_arguments.done" => {
                events.extend(self.complete_tool(&value)?);
            }
            "response.completed" => {
                events.extend(self.capture_response_id(&value)?);
                events.extend(self.close_semantic_items());
                events.extend(self.abort_incomplete_tools("response.completed"));
                events.extend(self.fail_incomplete_images("response.completed"));
                if let Some(usage) = value
                    .pointer("/response/usage")
                    .or_else(|| value.get("usage"))
                {
                    events.push(AiStreamEvent::UsageUpdated {
                        usage: openai_usage(usage),
                    });
                }
                self.terminal = true;
                events.push(AiStreamEvent::ResponseCompleted);
            }
            "response.failed" | "response.incomplete" => {
                events.extend(self.capture_response_id(&value)?);
                events.extend(self.close_semantic_items());
                events.extend(self.abort_incomplete_tools(wire_type));
                events.extend(self.fail_incomplete_images(wire_type));
                self.terminal = true;
                let error = value
                    .pointer("/response/error")
                    .or_else(|| value.get("error"))
                    .unwrap_or(&Value::Null);
                events.push(AiStreamEvent::ResponseFailed {
                    error: normalize_openai_error(
                        error,
                        value
                            .get("error_type")
                            .or_else(|| value.pointer("/response/error_type")),
                    ),
                });
            }
            "response.output_item.done" => {
                if value.pointer("/item/type").and_then(Value::as_str) == Some("reasoning") {
                    self.reasoning_signature = value
                        .pointer("/item/encrypted_content")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                } else {
                    events.extend(self.completed_image_item(&value)?);
                }
            }
            "response.image_generation_call.partial_image" => {
                let index = Self::output_index(&value)?;
                if self.completed_images.contains(&index) {
                    return Err(ProtocolConversionError::new(format!(
                        "OpenAI Responses image output index `{index}` emitted progress after completion."
                    )));
                }
                let item_id =
                    self.image_item_id(index, value.get("item_id").and_then(Value::as_str))?;
                let encoded = value
                    .get("partial_image_b64")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ProtocolConversionError::new(
                            "OpenAI Responses partial image payload is missing.",
                        )
                    })?;
                let bytes_processed = u64::try_from(encoded.len())
                    .map_err(|_| {
                        ProtocolConversionError::new(
                            "OpenAI Responses partial image byte count overflowed.",
                        )
                    })?
                    .saturating_mul(3)
                    / 4;
                events.push(AiStreamEvent::ResourceProgress {
                    resource_id: format!("{item_id}:resource"),
                    state: ResourceTransferState::Running {
                        bytes_processed,
                        bytes_total: None,
                    },
                });
            }
            "response.content_part.added"
            | "response.content_part.done"
            | "response.output_text.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
            | "response.reasoning_summary_text.done" => {}
            other => {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Responses SSE event type `{other}` is not registered."
                )));
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            return Ok(Vec::new());
        }
        Err(ProtocolConversionError::new(
            "OpenAI Responses stream ended before a terminal response event.",
        ))
    }
}

/// Resolves one exact OpenAI image output format to its MIME type.
/// 将一个精确的 OpenAI 图片输出格式解析为 MIME 类型。
///
/// # Parameters
/// 参数
/// `output_format` is the provider-returned image output format.
/// `output_format` 是供应商返回的图片输出格式。
///
/// # Returns
/// 返回值
/// The matching MIME type, or a controlled protocol error for an unknown format.
/// 返回匹配的 MIME 类型；未知格式则返回受控协议错误。
fn openai_image_media_type(output_format: &str) -> Result<&'static str, ProtocolConversionError> {
    match output_format {
        "png" => Ok("image/png"),
        "jpeg" | "jpg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        other => Err(ProtocolConversionError::new(format!(
            "OpenAI Responses image output format `{other}` is not registered."
        ))),
    }
}

/// Builds one public argument delta event with exact receive counters.
/// 使用精确接收计数构建一个公共参数增量事件。
fn tool_argument_delta(
    state: &OpenAiToolState,
    delta: &str,
) -> Result<AiStreamEvent, ProtocolConversionError> {
    Ok(AiStreamEvent::ToolCallArgumentsDelta {
        item_id: state.item_id.clone(),
        delta: delta.to_string(),
        progress: ToolCallProgress {
            call_id: Some(state.call_id.clone()),
            tool_name: Some(state.tool_name.clone()),
            bytes_received: u64::try_from(state.arguments.len()).map_err(|_| {
                ProtocolConversionError::new("OpenAI tool argument byte count overflowed.")
            })?,
            characters_received: u64::try_from(state.arguments.chars().count()).map_err(|_| {
                ProtocolConversionError::new("OpenAI tool argument character count overflowed.")
            })?,
            complete: false,
        },
    })
}

/// Converts OpenAI usage while preserving unknown fields.
/// 转换 OpenAI 用量并保留未知字段。
fn openai_usage(value: &Value) -> RequestUsage {
    let input_tokens = reported_usage(value, &["input_tokens", "prompt_tokens"]);
    let cache_read_tokens = reported_pointer_usage(
        value,
        &[
            "/input_tokens_details/cached_tokens",
            "/prompt_tokens_details/cached_tokens",
        ],
    );
    RequestUsage {
        input_tokens,
        uncached_input_tokens: derive_uncached_input(input_tokens, cache_read_tokens),
        output_tokens: reported_usage(value, &["output_tokens", "completion_tokens"]),
        reasoning_tokens: reported_pointer_usage(
            value,
            &[
                "/output_tokens_details/reasoning_tokens",
                "/completion_tokens_details/reasoning_tokens",
            ],
        ),
        cache_read_tokens,
        cache_write_tokens: reported_pointer_usage(
            value,
            &[
                "/input_tokens_details/cache_creation_tokens",
                "/input_tokens_details/cache_write_tokens",
                "/prompt_tokens_details/cache_creation_tokens",
                "/prompt_tokens_details/cache_write_tokens",
            ],
        ),
        total_tokens: reported_usage(value, &["total_tokens"]),
        ..RequestUsage::default()
    }
}

/// Reads the first reported usage field from an explicit alias list.
/// 从显式别名列表读取第一个已报告用量字段。
fn reported_usage(value: &Value, fields: &[&str]) -> UsageValue {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(Value::as_u64))
        .map(reported)
        .unwrap_or(UsageValue::Unknown)
}

/// Reads the first reported usage field from explicit protocol-specific JSON pointers.
/// 从显式的协议专属 JSON 指针列表读取第一个已报告用量字段。
fn reported_pointer_usage(value: &Value, pointers: &[&str]) -> UsageValue {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_u64))
        .map(reported)
        .unwrap_or(UsageValue::Unknown)
}

/// Marks one usage value as provider-reported.
/// 将一个用量值标记为供应商已报告。
fn reported(value: u64) -> UsageValue {
    UsageValue::Known {
        value,
        source: UsageValueSource::Reported,
    }
}

/// Normalizes OpenAI and OpenRouter error payloads.
/// 归一化 OpenAI 与 OpenRouter 错误载荷。
fn normalize_openai_error(error: &Value, outer_error_type: Option<&Value>) -> AiExecutionError {
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .map(str::to_string);
    let error_type = outer_error_type
        .and_then(Value::as_str)
        .or_else(|| error.get("type").and_then(Value::as_str))
        .map(str::to_string);
    let discriminator = error_type.as_deref().or(code.as_deref());
    let category = match discriminator {
        Some("authentication") | Some("authentication_error") | Some("invalid_api_key") => {
            AiErrorCategory::Authentication
        }
        Some("permission_denied") | Some("permission_error") => AiErrorCategory::PermissionDenied,
        Some("rate_limit_exceeded") | Some("rate_limit_error") => AiErrorCategory::RateLimited,
        Some("provider_overloaded") | Some("overloaded_error") => {
            AiErrorCategory::ProviderOverloaded
        }
        Some("provider_unavailable") => AiErrorCategory::ProviderUnavailable,
        Some("timeout") => AiErrorCategory::Timeout,
        Some("invalid_request") | Some("invalid_prompt") => AiErrorCategory::InvalidRequest,
        Some("content_policy_violation") | Some("image_content_policy_violation") => {
            AiErrorCategory::ContentPolicy
        }
        _ => AiErrorCategory::Unclassified,
    };
    let upstream_message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string);
    AiExecutionError {
        category,
        subcategory: error_type.clone(),
        stage: String::from("stream"),
        http_status: None,
        upstream_code: code,
        upstream_type: error_type,
        upstream_parameter: error
            .get("param")
            .and_then(Value::as_str)
            .map(str::to_string),
        upstream_message: upstream_message.clone(),
        request_id: error
            .get("request_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        provider_id: None,
        entry_id: None,
        model_id: error
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string),
        message: upstream_message
            .as_deref()
            .unwrap_or("OpenAI-compatible stream failed.")
            .to_string(),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: matches!(
            category,
            AiErrorCategory::RateLimited
                | AiErrorCategory::ProviderOverloaded
                | AiErrorCategory::ProviderUnavailable
                | AiErrorCategory::Timeout
        ),
        retry_after: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies Chat tool arguments remain streaming until the finish signal.
    /// 验证 Chat 工具参数在结束信号前保持流式接收。
    #[test]
    fn chat_tool_arguments_stream_before_completion() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::from([String::from("edit")]),
        );
        let first = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"edit","arguments":"{\"text\":\""}}]},"finish_reason":null}]}"#,
            )
            .expect("first chunk should decode");
        assert!(first.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. } if tool_name == "edit"
        )));
        assert!(first.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallArgumentsDelta { progress, .. }
                if progress.bytes_received > 0 && !progress.complete
        )));
    }

    /// Verifies MiniMax cumulative reasoning details produce only newly received text.
    /// 验证 MiniMax 累计式推理明细只产生新接收的文本。
    #[test]
    fn chat_minimax_reasoning_details_are_normalized_to_deltas() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::new(),
        );
        let first = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"text":"先分析"}]},"finish_reason":null}]}"#,
            )
            .expect("first reasoning chunk should decode");
        let second = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"text":"先分析，再验证"}]},"finish_reason":null}]}"#,
            )
            .expect("cumulative reasoning chunk should decode");
        assert!(first.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { delta, .. } if delta == "先分析"
        )));
        assert!(second.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { delta, .. } if delta == "，再验证"
        )));
    }

    /// Verifies Chat Completions usage reads MiniMax reasoning-token details.
    /// 验证 Chat Completions 用量会读取 MiniMax 推理 Token 明细。
    #[test]
    fn chat_minimax_usage_reads_completion_reasoning_tokens() {
        let usage = openai_usage(&json!({
            "prompt_tokens": 123,
            "completion_tokens": 45,
            "total_tokens": 168,
            "completion_tokens_details": {
                "reasoning_tokens": 30
            }
        }));

        assert_eq!(usage.input_tokens, reported(123));
        assert_eq!(usage.output_tokens, reported(45));
        assert_eq!(usage.reasoning_tokens, reported(30));
        assert_eq!(usage.total_tokens, reported(168));
    }

    /// Verifies Chat Completions usage reads cache-read and cache-write prompt tokens from Chat-specific details fields.
    /// 验证 Chat Completions 用量会从 Chat 专属明细字段读取缓存读取与缓存写入 Token。
    #[test]
    fn chat_usage_reads_cached_prompt_tokens() {
        let usage = openai_usage(&json!({
            "prompt_tokens": 123,
            "completion_tokens": 45,
            "total_tokens": 168,
            "prompt_tokens_details": {
                "cached_tokens": 96,
                "cache_creation_tokens": 27
            }
        }));

        assert_eq!(usage.input_tokens, reported(123));
        assert_eq!(usage.cache_read_tokens, reported(96));
        assert_eq!(usage.cache_write_tokens, reported(27));
    }

    /// Verifies a documented finish reason can terminate providers that close without `[DONE]`.
    /// 验证文档化结束原因可终止未发送 `[DONE]` 即关闭的供应商流。
    #[test]
    fn chat_finish_reason_allows_transport_close_without_done_record() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::new(),
        );
        decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"content":"完成"},"finish_reason":"stop"}]}"#,
            )
            .expect("finish-reason chunk should decode");

        let terminal = decoder
            .finish()
            .expect("transport close after finish reason should complete");
        assert_eq!(terminal, vec![AiStreamEvent::ResponseCompleted]);
    }

    /// Verifies arguments received before identity are published after the tool card can start.
    /// 验证在身份前收到的参数会在工具卡可启动后发布。
    #[test]
    fn chat_tool_arguments_received_before_identity_are_not_lost() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::from([String::from("edit")]),
        );
        let early = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"text\":\""}}]},"finish_reason":null}]}"#,
            )
            .expect("early arguments should buffer");
        assert!(
            !early
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ToolCallArgumentsDelta { .. }))
        );

        let identity = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"edit"}}]},"finish_reason":null}]}"#,
            )
            .expect("identity should start the tool");
        assert!(identity.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. } if tool_name == "edit"
        )));
        assert!(identity.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallArgumentsDelta { delta, progress, .. }
                if delta == "{\"text\":\"" && progress.bytes_received == 9
        )));
    }

    /// Verifies a fragmented tool name does not start a card until it exactly matches a registered tool.
    /// 验证分片工具名只有精确匹配已注册工具后才会启动工具卡。
    #[test]
    fn chat_fragmented_tool_name_waits_for_exact_registered_identity() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::from([String::from("vulcan_file_edit")]),
        );
        let prefix = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"vulcan_"}}]},"finish_reason":null}]}"#,
            )
            .expect("tool-name prefix should remain buffered");
        assert!(
            !prefix
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ToolCallStarted { .. }))
        );

        let completed_name = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"file_edit","arguments":"{}"}}]},"finish_reason":null}]}"#,
            )
            .expect("complete registered identity should start");
        assert!(completed_name.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. }
                if tool_name == "vulcan_file_edit"
        )));
    }

    /// Verifies Responses incomplete tool JSON becomes an aborted tool event.
    /// 验证 Responses 不完整工具 JSON 转换为工具中止事件。
    #[test]
    fn responses_incomplete_tool_is_aborted_at_done() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"edit"}}"#,
            )
            .expect("tool should start");
        let events = decoder
            .push_sse(
                Some("response.function_call_arguments.done"),
                r#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"text\":\""}"#,
            )
            .expect("done should normalize");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ToolCallAborted { .. }))
        );
    }

    /// Verifies Responses publishes the immutable upstream response identifier exactly once.
    /// 验证 Responses 只会发布一次不可变的上游响应标识。
    #[test]
    fn responses_publishes_response_identifier_once() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let created = decoder
            .push_sse(
                Some("response.created"),
                r#"{"type":"response.created","response":{"id":"resp_123"}}"#,
            )
            .expect("response identity should decode");
        assert!(created.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseMetadataUpdated { continuation }
                if continuation.provider_message_id == "resp_123"
        )));

        let in_progress = decoder
            .push_sse(
                Some("response.in_progress"),
                r#"{"type":"response.in_progress","response":{"id":"resp_123"}}"#,
            )
            .expect("stable response identity should decode");
        assert!(
            !in_progress
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseMetadataUpdated { .. }))
        );
    }

    /// Verifies a terminal response cannot leave an unfinished tool card open.
    /// 验证响应终态不会让未完成工具卡保持打开。
    #[test]
    fn responses_terminal_event_aborts_unfinished_tool() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"edit"}}"#,
            )
            .expect("tool should start");
        let events = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("terminal response should close the tool");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallAborted { item_id, message }
                if item_id == "item" && message.contains("response.completed")
        )));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
    }

    /// Verifies a Responses reasoning summary delta cannot collapse a missing index into slot zero.
    /// 验证 Responses 推理摘要增量不会把缺失索引折叠为第零槽位。
    #[test]
    fn responses_reasoning_summary_requires_index() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let error = decoder
            .push_sse(
                Some("response.reasoning_summary_text.delta"),
                r#"{"type":"response.reasoning_summary_text.delta","delta":"thinking"}"#,
            )
            .expect_err("missing summary index must fail");
        assert!(error.to_string().contains("summary index is missing"));
    }

    /// Verifies encrypted Responses reasoning is attached to the completed public reasoning item.
    /// 验证 Responses 加密推理内容会附加到已完成的公共推理项。
    #[test]
    fn responses_preserves_encrypted_reasoning_for_replay() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.reasoning_summary_text.delta"),
                r#"{"type":"response.reasoning_summary_text.delta","summary_index":0,"delta":"summary"}"#,
            )
            .expect("reasoning summary should decode");
        decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","item":{"type":"reasoning","encrypted_content":"encrypted-replay"}}"#,
            )
            .expect("reasoning signature should decode");
        let events = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("response should complete");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningCompleted {
                signature: Some(signature),
                ..
            } if signature == "openai#encrypted-replay"
        )));
    }

    /// Verifies Responses image output becomes a resource and never a visible Base64 text delta.
    /// 验证 Responses 图片输出会转换为资源，且绝不会成为可见 Base64 文本增量。
    #[test]
    fn responses_image_generation_becomes_resource() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":2,"item":{"id":"image-item","type":"image_generation_call","status":"completed","result":"aW1hZ2U=","output_format":"png"}}"#,
            )
            .expect("image result should decode");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseItemStarted {
                item_id,
                item: ContentItem::Resource { resource },
            } if item_id == "image-item"
                && resource.id == "image-item:resource"
                && resource.media_type.as_deref() == Some("image/png")
                && resource.url.as_deref() == Some("data:image/png;base64,aW1hZ2U=")
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTextDelta { delta, .. } if delta.contains("aW1hZ2U=")
        )));
    }

    /// Verifies streamed image progress and completion share the upstream item identity.
    /// 验证流式图片进度与完成事件共享上游项身份。
    #[test]
    fn responses_image_progress_and_completion_share_resource_identity() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":2,"item":{"id":"image-item","type":"image_generation_call","status":"in_progress"}}"#,
            )
            .expect("image item should register");
        let progress = decoder
            .push_sse(
                Some("response.image_generation_call.partial_image"),
                r#"{"type":"response.image_generation_call.partial_image","output_index":2,"item_id":"image-item","partial_image_b64":"aW1hZ2U="}"#,
            )
            .expect("partial image should decode");
        assert!(progress.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResourceProgress { resource_id, .. }
                if resource_id == "image-item:resource"
        )));

        let completed = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":2,"item":{"id":"image-item","type":"image_generation_call","status":"completed","result":"aW1hZ2U=","output_format":"png"}}"#,
            )
            .expect("image result should decode");
        assert!(completed.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResourceProgress {
                resource_id,
                state: ResourceTransferState::Completed,
            } if resource_id == "image-item:resource"
        )));
    }

    /// Verifies a terminal response fails an image resource that never completed.
    /// 验证响应终态会把从未完成的图片资源标记为失败。
    #[test]
    fn responses_terminal_event_fails_incomplete_image() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":2,"item":{"id":"image-item","type":"image_generation_call","status":"in_progress"}}"#,
            )
            .expect("image item should register");
        let events = decoder
            .push_sse(
                Some("response.incomplete"),
                r#"{"type":"response.incomplete","response":{"error":{"message":"incomplete"}}}"#,
            )
            .expect("terminal response should close the image");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResourceProgress {
                resource_id,
                state: ResourceTransferState::Failed { message },
            } if resource_id == "image-item:resource"
                && message.contains("response.incomplete")
        )));
    }

    /// Verifies image progress cannot reopen a resource after its completed item.
    /// 验证图片进度不能在项完成后重新打开资源。
    #[test]
    fn responses_image_progress_after_completion_is_rejected() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":2,"item":{"id":"image-item","type":"image_generation_call","status":"completed","result":"aW1hZ2U=","output_format":"png"}}"#,
            )
            .expect("image result should decode");
        let error = decoder
            .push_sse(
                Some("response.image_generation_call.partial_image"),
                r#"{"type":"response.image_generation_call.partial_image","output_index":2,"item_id":"image-item","partial_image_b64":"aW1hZ2U="}"#,
            )
            .expect_err("progress after image completion must fail");
        assert!(error.to_string().contains("progress after completion"));
    }
}
