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
    is_replayable_codex_responses_item,
};

/// Tracks one OpenAI-compatible streamed tool call.
/// 跟踪一个 OpenAI 兼容流式工具调用。
#[derive(Debug, Default)]
struct OpenAiToolState {
    /// Stable public stream item identifier.
    /// 稳定公共流项目标识。
    item_id: String,
    /// Exact optional upstream output-item identifier.
    /// 精确可选上游输出项标识。
    provider_item_id: Option<String>,
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

/// Returns the newly observed suffix of one cumulative provider text field.
/// 返回单个供应商累计文本字段中新观测到的后缀。
///
/// # Parameters
/// 参数
/// `previous` stores the last cumulative value and `current` is the newly decoded value.
/// `previous` 保存上次累计值，`current` 是本次解码值。
///
/// # Returns
/// 返回值
/// The new suffix, a standalone non-cumulative fragment, or `None` for empty input.
/// 返回新增后缀、独立的非累计片段，或在输入为空时返回 `None`。
fn cumulative_suffix(previous: &mut String, current: String) -> Option<String> {
    if current.is_empty() {
        return None;
    }
    if current.starts_with(previous.as_str()) {
        // suffix excludes the cumulative prefix already projected to semantic output.
        // suffix 排除已经投影到语义输出的累计前缀。
        let suffix = current[previous.len()..].to_string();
        *previous = current;
        return Some(suffix);
    }
    // A non-prefix value is a standalone provider fragment, not a replacement snapshot.
    // 非前缀值是供应商独立片段，而不是替换式快照。
    previous.push_str(current.as_str());
    Some(current)
}

/// Builds a replay-safe provider context item without transient reasoning fields.
/// 构建不含瞬态推理字段且可安全重放的供应商上下文项目。
///
/// # Parameters
/// 参数
/// `item` is one completed provider-owned Responses output item.
/// `item` 是一个已完成且由供应商持有的 Responses 输出项目。
///
/// # Returns
/// 返回值
/// A cloned replay item, or `None` when the item contains only summary or encrypted reasoning.
/// 返回克隆后的重放项目；项目仅含摘要或加密推理时返回 `None`。
fn replayable_provider_context_item(item: &Value) -> Option<Value> {
    if item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return Some(item.clone());
    }
    // replay_item is scrubbed independently so the original diagnostic event remains intact.
    // replay_item 独立清理，使原始诊断事件保持不变。
    let mut replay_item = item.clone();
    // object owns the only accepted Responses reasoning object shape.
    // object 持有唯一接受的 Responses 推理对象结构。
    let object = replay_item.as_object_mut()?;
    object.remove("summary");
    object.remove("encrypted_content");
    // has_full_reasoning admits only provider content that can reconstruct durable reasoning.
    // has_full_reasoning 只允许能够重建持久化完整思考的供应商正文。
    let has_full_reasoning = object
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|content| !content.is_empty());
    has_full_reasoning.then_some(replay_item)
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
    /// Last cumulative transient `reasoning.summary` text.
    /// 最近一次累计的瞬态 `reasoning.summary` 文本。
    reasoning_summary_text: String,
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
            reasoning_summary_text: String::new(),
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
        // detail_reasoning and summary keep durable thinking separate from replacement status.
        // detail_reasoning 与 summary 将持久化思考和替换式状态分离。
        let (detail_reasoning, summary) = self.decode_reasoning_details_delta(delta);
        // reasoning prefers the protocol's direct durable field when both representations exist.
        // reasoning 在两种表示同时存在时优先采用协议直接持久化字段。
        let reasoning = direct_reasoning.map(str::to_string).or(detail_reasoning);
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
        if let Some(summary) = summary {
            events.push(AiStreamEvent::ReasoningSummaryUpdated { text: summary });
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
            // A choice finish reason proves semantic completion but is not the transport boundary.
            // 选择结束原因只证明语义完成，并不是传输边界。
            self.finish_signal_seen = true;
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
    /// Newly observed full-reasoning suffix and complete transient-summary replacement.
    /// 返回新观测到的完整推理后缀与完整瞬态摘要替换值。
    fn decode_reasoning_details_delta(
        &mut self,
        delta: &Value,
    ) -> (Option<String>, Option<String>) {
        // details is the exact provider-owned typed reasoning array for this chunk.
        // details 是当前分片中由供应商持有的精确类型化推理数组。
        let Some(details) = delta.get("reasoning_details").and_then(Value::as_array) else {
            return (None, None);
        };
        // reasoning concatenates only durable text parts from the current array snapshot.
        // reasoning 只拼接当前数组快照中的持久化文本部分。
        let reasoning = details
            .iter()
            .filter_map(|detail| match detail.get("type").and_then(Value::as_str) {
                Some("reasoning.text") | None => detail.get("text").and_then(Value::as_str),
                Some(_) => None,
            })
            .collect::<String>();
        // summary concatenates only readable replacement-summary parts.
        // summary 只拼接可读的替换式摘要部分。
        let summary = details
            .iter()
            .filter_map(|detail| {
                (detail.get("type").and_then(Value::as_str) == Some("reasoning.summary"))
                    .then(|| detail.get("summary").and_then(Value::as_str))
                    .flatten()
            })
            .collect::<String>();
        // Chat summary values are wire deltas; the public event always replaces with accumulated text.
        // Chat 摘要值是线路增量；公共事件始终使用累计完整文本进行替换。
        let summary_update = if summary.is_empty() {
            None
        } else {
            self.reasoning_summary_text.push_str(summary.as_str());
            Some(self.reasoning_summary_text.clone())
        };
        (
            cumulative_suffix(&mut self.reasoning_details_text, reasoning),
            summary_update,
        )
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
                error: normalize_openai_error(error, value.get("error_type"), None),
            });
            return Ok(events);
        }
        if let Some(usage) = value.get("usage") {
            events.push(AiStreamEvent::UsageUpdated {
                usage: Box::new(openai_usage(usage)),
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
            // EOF after a finish reason is the registered terminal substitute for a missing `[DONE]`.
            // 带结束原因的 EOF 是缺失 `[DONE]` 时已注册的终态替代边界。
            let mut events = self.close_open_items();
            events.extend(self.complete_tools());
            self.terminal = true;
            events.push(AiStreamEvent::ResponseCompleted);
            return Ok(events);
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
    /// Exact upstream message identifiers keyed by output index.
    /// 按输出索引保存的精确上游消息标识。
    text_provider_ids: BTreeMap<u32, String>,
    /// Accumulated visible text keyed by output index.
    /// 按输出索引保存的已累积可见文本。
    text_content: BTreeMap<u32, String>,
    /// Text output indices already completed by output-item completion.
    /// 已由输出项完成事件结束的文本输出索引。
    completed_text_items: BTreeSet<u32>,
    /// Open reasoning items keyed by event family, upstream item scope, and local content index.
    /// 按事件族、上游项目作用域与局部内容索引保存的打开推理项。
    reasoning_items: BTreeMap<String, String>,
    /// Reasoning keys in their first-observed provider stream order.
    /// 按供应商流首次出现顺序保存的推理键。
    reasoning_order: Vec<String>,
    /// Exact upstream item scope owned by every public reasoning key.
    /// 每个公共推理键所属的精确上游项目作用域。
    reasoning_scopes: BTreeMap<String, String>,
    /// Accumulated reasoning text keyed by the exact semantic block.
    /// 按精确语义块保存的已累积推理文本。
    reasoning_text: BTreeMap<String, String>,
    /// Reasoning keys that already received their dedicated text completion event.
    /// 已收到专用文本完成事件的推理键。
    completed_reasoning_items: BTreeSet<String>,
    /// Tool calls keyed by output index.
    /// 按输出索引保存的工具调用。
    tools: BTreeMap<u32, OpenAiToolState>,
    /// Image-generation item identifiers keyed by output index.
    /// 按输出索引保存的图片生成项标识。
    image_items: BTreeMap<u32, String>,
    /// Image-generation output indices that already reached their item completion event.
    /// 已到达项完成事件的图片生成输出索引。
    completed_images: BTreeSet<u32>,
    /// Provider output-item scopes already captured for exact route-bound replay.
    /// 已为精确路由绑定重放捕获的供应商输出项作用域。
    captured_provider_context_scopes: BTreeSet<String>,
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
            text_provider_ids: BTreeMap::new(),
            text_content: BTreeMap::new(),
            completed_text_items: BTreeSet::new(),
            reasoning_items: BTreeMap::new(),
            reasoning_order: Vec::new(),
            reasoning_scopes: BTreeMap::new(),
            reasoning_text: BTreeMap::new(),
            completed_reasoning_items: BTreeSet::new(),
            tools: BTreeMap::new(),
            image_items: BTreeMap::new(),
            completed_images: BTreeSet::new(),
            captured_provider_context_scopes: BTreeSet::new(),
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
        if self.tools.contains_key(&index) {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses added duplicate tool output index `{index}`."
            )));
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
        let provider_item_id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|item_id| !item_id.is_empty())
            .map(str::to_string);
        let item_id = provider_item_id
            .clone()
            .unwrap_or_else(|| format!("responses:{}:tool:{index}", self.execution_id));
        self.tools.insert(
            index,
            OpenAiToolState {
                item_id: item_id.clone(),
                provider_item_id,
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

    /// Validates immutable tool identity fields across added, delta, and completion events.
    /// 在添加、增量与完成事件之间校验不可变工具身份字段。
    ///
    /// # Parameters
    /// 参数
    /// `state` is the opened tool state and `value` is the next exact provider event.
    /// `state` 是已打开工具状态，`value` 是下一项精确供应商事件。
    ///
    /// # Returns
    /// 返回值
    /// Success for matching identity, or a protocol error when the provider mutates it.
    /// 身份匹配时成功，供应商修改身份时返回协议错误。
    fn validate_tool_identity(
        state: &mut OpenAiToolState,
        value: &Value,
    ) -> Result<(), ProtocolConversionError> {
        // reportedItemId supports both incremental top-level and completed nested item shapes.
        // reportedItemId 同时支持增量顶层结构与完成事件嵌套项目结构。
        let reported_item_id = value
            .get("item_id")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/item/id").and_then(Value::as_str))
            .filter(|item_id| !item_id.is_empty());
        if let Some(reported_item_id) = reported_item_id {
            if let Some(existing) = state.provider_item_id.as_deref() {
                if existing != reported_item_id {
                    return Err(ProtocolConversionError::new(
                        "OpenAI Responses changed tool output-item identity.",
                    ));
                }
            } else {
                state.provider_item_id = Some(reported_item_id.to_string());
            }
        }
        // reportedCallId prevents one output slot from being rebound to another tool invocation.
        // reportedCallId 防止一个输出槽位被重新绑定到另一项工具调用。
        let reported_call_id = value
            .get("call_id")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/item/call_id").and_then(Value::as_str));
        if reported_call_id.is_some_and(|reported| reported != state.call_id) {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses changed tool call identity.",
            ));
        }
        // reportedToolName prevents completion from changing the registered tool target.
        // reportedToolName 防止完成事件修改已注册工具目标。
        let reported_tool_name = value
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/item/name").and_then(Value::as_str));
        if reported_tool_name.is_some_and(|reported| reported != state.tool_name) {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses changed tool name during one output item.",
            ));
        }
        Ok(())
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
        if state.terminal {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses tool output index `{index}` emitted arguments after completion."
            )));
        }
        Self::validate_tool_identity(state, value)?;
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
        Self::validate_tool_identity(state, value)?;
        if state.terminal {
            return Ok(Vec::new());
        }
        if let Some(arguments) = value
            .get("arguments")
            .or_else(|| value.pointer("/item/arguments"))
            .and_then(Value::as_str)
        {
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

    /// Registers one stable public text identity and validates immutable upstream identity.
    /// 注册一个稳定公共文本身份，并校验不可变上游身份。
    ///
    /// # Parameters
    /// 参数
    /// `index` is the provider output slot and `reported_item_id` is its optional upstream id.
    /// `index` 是供应商输出槽位，`reported_item_id` 是其可选上游标识。
    ///
    /// # Returns
    /// 返回值
    /// The stable public item id and whether this call opened the text block.
    /// 返回稳定公共项标识，以及本次调用是否新建文本块。
    fn register_text_item(
        &mut self,
        index: u32,
        reported_item_id: Option<&str>,
    ) -> Result<(String, bool), ProtocolConversionError> {
        // reportedItemId excludes empty provider identifiers before immutable identity checks.
        // reportedItemId 在不可变身份校验前排除空供应商标识。
        let reported_item_id = reported_item_id.filter(|item_id| !item_id.is_empty());
        if let Some(reported_item_id) = reported_item_id {
            if let Some(existing) = self.text_provider_ids.get(&index) {
                if existing != reported_item_id {
                    return Err(ProtocolConversionError::new(format!(
                        "OpenAI Responses changed text item identity at output index `{index}`."
                    )));
                }
            } else {
                self.text_provider_ids
                    .insert(index, reported_item_id.to_string());
            }
        }
        // isNew records whether the public stream needs a matching start event.
        // isNew 记录公共流是否需要匹配的开始事件。
        let is_new = !self.text_items.contains_key(&index);
        if is_new {
            // itemId keeps the provider id when known before falling back to a stable local id.
            // itemId 在已知时保留供应商标识，否则回退到稳定本地标识。
            let item_id = reported_item_id
                .map(str::to_string)
                .unwrap_or_else(|| format!("responses:{}:text:{index}", self.execution_id));
            self.text_items.insert(index, item_id);
            self.text_content.insert(index, String::new());
        }
        // itemId exists because the new-item branch inserted it.
        // itemId 由新项目分支保证已经插入。
        let item_id = self
            .text_items
            .get(&index)
            .cloned()
            .expect("registered text index must own a public item id");
        Ok((item_id, is_new))
    }

    /// Decodes one complete assistant message delivered by output_item.done.
    /// 解码一条由 output_item.done 交付的完整助手消息。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the exact output-item completion event.
    /// `value` 是精确的输出项完成事件。
    ///
    /// # Returns
    /// 返回值
    /// A complete buffered item or only the suffix missing from preceding deltas.
    /// 返回完整缓冲项，或仅返回前序增量中缺失的后缀。
    fn completed_message_item(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        // index is the provider-owned message slot shared by buffered and incremental delivery.
        // index 是缓冲与增量交付共享的供应商消息槽位。
        let index = Self::output_index(value)?;
        // item is the exact completed message object carried by the lifecycle event.
        // item 是生命周期事件携带的精确已完成消息对象。
        let item = value
            .get("item")
            .ok_or_else(|| ProtocolConversionError::new("OpenAI Responses item is missing."))?;
        // text preserves provider content order and mirrors the concatenation produced by deltas.
        // text 保留供应商内容顺序，并镜像增量交付产生的拼接结果。
        let text = item
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(
                |content| match content.get("type").and_then(Value::as_str) {
                    Some("output_text") => content.get("text").and_then(Value::as_str),
                    Some("refusal") => content
                        .get("refusal")
                        .or_else(|| content.get("text"))
                        .and_then(Value::as_str),
                    _ => None,
                },
            )
            .collect::<String>();
        if text.is_empty() && !self.text_items.contains_key(&index) {
            return Ok(Vec::new());
        }
        // reportedItemId is validated against any identity observed by preceding delta events.
        // reportedItemId 会与前序增量事件观察到的身份进行校验。
        let reported_item_id = item.get("id").and_then(Value::as_str);
        let (item_id, is_new) = self.register_text_item(index, reported_item_id)?;
        // accumulatedText is the exact visible prefix already sent to the public stream.
        // accumulatedText 是已经发送到公共流的精确可见前缀。
        let accumulated_text = self
            .text_content
            .get_mut(&index)
            .expect("registered text index must own accumulated content");
        if self.completed_text_items.contains(&index) {
            if accumulated_text == &text {
                return Ok(Vec::new());
            }
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses text output index `{index}` changed after completion."
            )));
        }
        if !text.starts_with(accumulated_text.as_str()) {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses completed text at output index `{index}` does not extend its streamed prefix."
            )));
        }
        // events opens a buffered item only when no preceding delta already opened it.
        // events 仅在没有前序增量打开该项时才打开缓冲项。
        let mut events = Vec::new();
        if is_new {
            events.push(AiStreamEvent::ResponseItemStarted {
                item_id: item_id.clone(),
                item: ContentItem::Text {
                    text: String::new(),
                },
            });
        }
        // suffix is the authoritative portion not delivered by earlier deltas.
        // suffix 是前序增量尚未交付的权威部分。
        let suffix = &text[accumulated_text.len()..];
        if !suffix.is_empty() {
            events.push(AiStreamEvent::ResponseTextDelta {
                item_id: item_id.clone(),
                delta: suffix.to_string(),
            });
            accumulated_text.push_str(suffix);
        }
        self.completed_text_items.insert(index);
        events.push(AiStreamEvent::ResponseItemCompleted { item_id });
        Ok(events)
    }

    /// Resolves the exact upstream scope shared by reasoning deltas and output-item completion.
    /// 解析推理增量与输出项完成事件共享的精确上游作用域。
    ///
    /// # Parameters
    /// 参数
    /// `value` is one reasoning lifecycle event or one completed reasoning output item.
    /// `value` 是一项推理生命周期事件或一项已完成推理输出项。
    ///
    /// # Returns
    /// 返回值
    /// A stable scope preferring the official item id, then output index, then legacy scope.
    /// 返回优先采用官方项目标识、其次输出索引、最后旧版作用域的稳定值。
    fn reasoning_wire_scope(value: &Value) -> String {
        value
            .get("item_id")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/item/id").and_then(Value::as_str))
            .map(|item_id| format!("item:{item_id}"))
            .or_else(|| {
                value
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .map(|output_index| format!("output:{output_index}"))
            })
            .unwrap_or_else(|| String::from("legacy"))
    }

    /// Registers one provider-owned reasoning block without losing first-observed stream order.
    /// 注册一个供应商持有的推理块，同时保留首次出现的流顺序。
    ///
    /// # Parameters
    /// 参数
    /// `wire_scope` owns the upstream item, `event_family` distinguishes summary from raw text,
    /// and `index` is the provider-owned local part index.
    /// `wire_scope` 标识上游项目，`event_family` 区分摘要与原始文本，`index` 是供应商持有的局部部分索引。
    ///
    /// # Returns
    /// 返回值
    /// The stable reasoning key, public item id, and whether this call opened the block.
    /// 返回稳定推理键、公共项标识，以及本次调用是否新建该块。
    fn register_reasoning_item(
        &mut self,
        wire_scope: &str,
        event_family: &str,
        index: u32,
    ) -> (String, String, bool) {
        // reasoningKey prevents local indexes from colliding across output items or event families.
        // reasoningKey 防止局部索引在不同输出项或事件族之间碰撞。
        let reasoning_key = format!("{event_family}:{wire_scope}:{index}");
        // isNew records whether the public semantic block must be opened by this event.
        // isNew 记录本事件是否需要打开公共语义块。
        let is_new = !self.reasoning_items.contains_key(reasoning_key.as_str());
        if is_new {
            // nextItemOrdinal provides a bounded provider-neutral internal identity.
            // nextItemOrdinal 提供有界且供应商无关的内部身份。
            let next_item_ordinal = self.reasoning_order.len();
            // itemId remains stable across every event for the same wire coordinates.
            // itemId 在同一线上坐标的所有事件之间保持稳定。
            let item_id = format!(
                "responses:{}:reasoning:{next_item_ordinal}",
                self.execution_id
            );
            self.reasoning_items.insert(reasoning_key.clone(), item_id);
            self.reasoning_order.push(reasoning_key.clone());
            self.reasoning_scopes
                .insert(reasoning_key.clone(), wire_scope.to_string());
            self.reasoning_text
                .insert(reasoning_key.clone(), String::new());
        }
        // itemId is guaranteed by the insertion branch for a new key.
        // itemId 对新键由插入分支保证存在。
        let item_id = self
            .reasoning_items
            .get(reasoning_key.as_str())
            .cloned()
            .expect("registered reasoning key must own a public item id");
        (reasoning_key, item_id, is_new)
    }

    /// Resolves one stable public reasoning identity from an exact reasoning text event.
    /// 从一项精确的推理文本事件解析稳定的公共推理身份。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the exact provider event and `wire_type` selects summary or full reasoning text.
    /// `value` 是精确的供应商事件，`wire_type` 选择摘要或完整推理文本。
    ///
    /// # Returns
    /// 返回值
    /// The stable reasoning key, public item id, and whether this event opened the item.
    /// 返回稳定推理键、公共项标识，以及本事件是否新建了该项。
    fn reasoning_item_identity(
        &mut self,
        value: &Value,
        wire_type: &str,
    ) -> Result<(String, String, bool), ProtocolConversionError> {
        // eventShape fixes the exact local index field and family for the registered wire event.
        // eventShape 为已注册线事件固定精确的局部索引字段与事件族。
        let (index_field, index_label, event_family) = match wire_type {
            "response.reasoning_text.delta" | "response.reasoning_text.done" => {
                ("content_index", "reasoning content index", "text")
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_summary_text.done" => {
                ("summary_index", "reasoning summary index", "summary")
            }
            _ => {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Responses reasoning event type `{wire_type}` is not registered."
                )));
            }
        };
        // indexNumber is the exact provider-owned local reasoning slot.
        // indexNumber 是供应商持有的精确局部推理槽位。
        let index_number = value
            .get(index_field)
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                ProtocolConversionError::new(format!("OpenAI Responses {index_label} is missing."))
            })?;
        // index is the bounded public representation of the provider slot.
        // index 是供应商槽位的有界公共表示。
        let index = u32::try_from(index_number).map_err(|_| {
            ProtocolConversionError::new(format!("OpenAI Responses {index_label} exceeds u32."))
        })?;
        // wireScope binds all reasoning parts to their exact upstream output item.
        // wireScope 将全部推理部分绑定到精确的上游输出项。
        let wire_scope = Self::reasoning_wire_scope(value);
        Ok(self.register_reasoning_item(wire_scope.as_str(), event_family, index))
    }

    /// Completes one reasoning block from an authoritative full-text event.
    /// 使用权威完整文本事件完成一个推理块。
    ///
    /// # Parameters
    /// 参数
    /// `reasoning_key` and `item_id` identify the block, `full_text` is the provider's complete
    /// value, `is_new` controls start emission, and `events` receives any missing suffix.
    /// `reasoning_key` 与 `item_id` 标识该块，`full_text` 是供应商完整值，`is_new` 控制开始事件，
    /// `events` 接收尚未交付的后缀。
    ///
    /// # Returns
    /// 返回值
    /// Success when the full value extends the streamed prefix, otherwise a protocol error.
    /// 完整值延续已流式交付前缀时成功，否则返回协议错误。
    fn complete_reasoning_text(
        &mut self,
        reasoning_key: &str,
        item_id: &str,
        full_text: &str,
        is_new: bool,
        transient_summary: bool,
        events: &mut Vec<AiStreamEvent>,
    ) -> Result<(), ProtocolConversionError> {
        // currentText is the exact prefix already emitted for this semantic block.
        // currentText 是该语义块已经发送的精确前缀。
        let current_text = self.reasoning_text.get_mut(reasoning_key).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "OpenAI Responses reasoning item `{reasoning_key}` is not registered."
            ))
        })?;
        if self.completed_reasoning_items.contains(reasoning_key) {
            if current_text == full_text {
                return Ok(());
            }
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses reasoning item `{reasoning_key}` changed after completion."
            )));
        }
        if !full_text.starts_with(current_text.as_str()) {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses reasoning completion for `{reasoning_key}` does not extend its streamed prefix."
            )));
        }
        if is_new && !transient_summary {
            events.push(AiStreamEvent::ReasoningStarted {
                item_id: item_id.to_string(),
            });
        }
        // suffix is the part not already emitted by earlier delta events.
        // suffix 是前序增量事件尚未发送的部分。
        let suffix = &full_text[current_text.len()..];
        if !suffix.is_empty() {
            if !transient_summary {
                events.push(AiStreamEvent::ReasoningDelta {
                    item_id: item_id.to_string(),
                    delta: suffix.to_string(),
                });
            }
            current_text.push_str(suffix);
            if transient_summary {
                events.push(AiStreamEvent::ReasoningSummaryUpdated {
                    text: current_text.clone(),
                });
            }
        }
        self.completed_reasoning_items
            .insert(reasoning_key.to_string());
        Ok(())
    }

    /// Decodes a complete reasoning item when a provider buffers part or all of its stream.
    /// 解码供应商缓冲部分或全部流时交付的完整推理项。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the exact `response.output_item.done` event.
    /// `value` 是精确的 `response.output_item.done` 事件。
    ///
    /// # Returns
    /// 返回值
    /// Missing reasoning suffix events plus an item-scoped replay signature, or a protocol error.
    /// 返回缺失推理后缀事件与项目作用域重放签名，或返回协议错误。
    fn completed_reasoning_item(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        // item is the authoritative complete Responses reasoning object.
        // item 是权威的完整 Responses 推理对象。
        let item = value
            .get("item")
            .ok_or_else(|| ProtocolConversionError::new("OpenAI Responses item is missing."))?;
        // wireScope binds full-item fallback to the same identities used by deltas.
        // wireScope 将完整项回退绑定到增量使用的相同身份。
        let wire_scope = Self::reasoning_wire_scope(value);
        // events preserves the provider's summary-before-content item order.
        // events 保留供应商摘要先于正文的项目顺序。
        let mut events = Vec::new();
        for (event_family, field_name, content_type) in [
            ("summary", "summary", "summary_text"),
            ("text", "content", "reasoning_text"),
        ] {
            for (index, part) in item
                .get(field_name)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if part.get("type").and_then(Value::as_str) != Some(content_type) {
                    continue;
                }
                // publicIndex bounds the provider-owned JSON array index.
                // publicIndex 对供应商持有的 JSON 数组索引进行有界转换。
                let public_index = u32::try_from(index).map_err(|_| {
                    ProtocolConversionError::new(
                        "OpenAI Responses reasoning part index exceeds u32.",
                    )
                })?;
                // text is authoritative full content for this reasoning part.
                // text 是该推理部分的权威完整内容。
                let text = part.get("text").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new(
                        "OpenAI Responses completed reasoning part text is missing.",
                    )
                })?;
                let (reasoning_key, item_id, is_new) =
                    self.register_reasoning_item(wire_scope.as_str(), event_family, public_index);
                self.complete_reasoning_text(
                    reasoning_key.as_str(),
                    item_id.as_str(),
                    text,
                    is_new,
                    event_family == "summary",
                    &mut events,
                )?;
            }
        }
        // Encrypted-only items intentionally produce no semantic or replay state.
        // 仅加密项目被刻意排除在语义状态与重放状态之外。
        Ok(events)
    }

    /// Closes any remaining semantic items before terminal response events.
    /// 在响应终态事件前关闭所有剩余语义项。
    fn close_semantic_items(&mut self) -> Vec<AiStreamEvent> {
        // events follows first-observed provider order instead of map lexical order.
        // events 遵循供应商首次出现顺序，而不是映射字典序。
        let mut events = self
            .reasoning_order
            .iter()
            .filter_map(|reasoning_key| {
                if reasoning_key.starts_with("summary:") {
                    return None;
                }
                let item_id = self.reasoning_items.get(reasoning_key)?.clone();
                Some(AiStreamEvent::ReasoningCompleted {
                    item_id,
                    signature: None,
                })
            })
            .collect::<Vec<_>>();
        events.extend(
            self.text_items
                .iter()
                .filter(|(index, _)| !self.completed_text_items.contains(index))
                .map(|(_, item_id)| AiStreamEvent::ResponseItemCompleted {
                    item_id: item_id.clone(),
                }),
        );
        self.reasoning_items.clear();
        self.reasoning_order.clear();
        self.reasoning_scopes.clear();
        self.reasoning_text.clear();
        self.completed_reasoning_items.clear();
        self.text_items.clear();
        self.text_provider_ids.clear();
        self.text_content.clear();
        self.completed_text_items.clear();
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
                        source_kind: vulcan_ai_contract::ResourceSourceKind::InlineData,
                        file_name: None,
                        image_detail: None,
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
        // A few Codex-compatible proxies append the generic OpenAI sentinel after the real
        // Responses terminal event. It is framing-only here and must not become a substitute
        // for response.completed, response.failed, response.incomplete, or error.
        // 少数 Codex 兼容代理会在真实 Responses 终态事件后追加通用 OpenAI 哨兵；该记录在此仅属于
        // 帧边界，不能替代 response.completed、response.failed、response.incomplete 或 error。
        if data.trim() == "[DONE]" {
            return Ok(Vec::new());
        }
        if self.terminal {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses emitted data after a terminal event.",
            ));
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
                if self.completed_text_items.contains(&index) {
                    return Err(ProtocolConversionError::new(format!(
                        "OpenAI Responses text output index `{index}` emitted a delta after completion."
                    )));
                }
                // reportedItemId preserves the official identity when the delta carries one.
                // reportedItemId 在增量携带官方身份时予以保留。
                let reported_item_id = value.get("item_id").and_then(Value::as_str);
                let (item_id, is_new) = self.register_text_item(index, reported_item_id)?;
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
                self.text_content
                    .get_mut(&index)
                    .expect("registered text index must own accumulated content")
                    .push_str(delta);
                events.push(AiStreamEvent::ResponseTextDelta {
                    item_id,
                    delta: delta.to_string(),
                });
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                // identity binds the delta to its exact upstream item, family, and local index.
                // identity 将增量绑定到精确的上游项、事件族与局部索引。
                let (reasoning_key, item_id, is_new) =
                    self.reasoning_item_identity(&value, wire_type)?;
                if self
                    .completed_reasoning_items
                    .contains(reasoning_key.as_str())
                {
                    return Err(ProtocolConversionError::new(format!(
                        "OpenAI Responses reasoning item `{reasoning_key}` emitted a delta after completion."
                    )));
                }
                let transient_summary = wire_type == "response.reasoning_summary_text.delta";
                if is_new && !transient_summary {
                    events.push(AiStreamEvent::ReasoningStarted {
                        item_id: item_id.clone(),
                    });
                }
                let delta = value.get("delta").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new("OpenAI Responses reasoning delta is missing.")
                })?;
                // accumulatedText makes the later full-text completion authoritative without duplication.
                // accumulatedText 让后续完整文本完成事件可作为权威值且不会重复输出。
                let summary_text = {
                    let accumulated_text = self
                        .reasoning_text
                        .get_mut(reasoning_key.as_str())
                        .expect("registered reasoning key must own accumulated text");
                    accumulated_text.push_str(delta);
                    transient_summary.then(|| accumulated_text.clone())
                };
                if transient_summary {
                    events.push(AiStreamEvent::ReasoningSummaryUpdated {
                        text: summary_text.expect("transient summary text should be assembled"),
                    });
                } else {
                    events.push(AiStreamEvent::ReasoningDelta {
                        item_id,
                        delta: delta.to_string(),
                    });
                }
            }
            "response.reasoning_summary_text.done" | "response.reasoning_text.done" => {
                // identity binds the completion to the same semantic block used by preceding deltas.
                // identity 将完成事件绑定到前序增量使用的同一语义块。
                let (reasoning_key, item_id, is_new) =
                    self.reasoning_item_identity(&value, wire_type)?;
                // text is the authoritative complete value and may supply a missing streamed suffix.
                // text 是权威完整值，并可补充流式交付中缺失的后缀。
                let text = value.get("text").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new(
                        "OpenAI Responses reasoning completion text is missing.",
                    )
                })?;
                self.complete_reasoning_text(
                    reasoning_key.as_str(),
                    item_id.as_str(),
                    text,
                    is_new,
                    wire_type == "response.reasoning_summary_text.done",
                    &mut events,
                )?;
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
                        usage: Box::new(openai_usage(usage)),
                    });
                }
                self.terminal = true;
                events.push(AiStreamEvent::ResponseCompleted);
            }
            "error" => {
                // error_event is a terminal public Responses event, not an unknown protocol record.
                // error_event 是 Responses 公共协议的终止错误事件，不是未知协议记录。
                events.extend(self.capture_response_id(&value)?);
                events.extend(self.close_semantic_items());
                events.extend(self.abort_incomplete_tools("error"));
                events.extend(self.fail_incomplete_images("error"));
                self.terminal = true;
                // error_payload supports both the flat public event and proxy-wrapped error objects.
                // error_payload 同时支持扁平公共事件与代理嵌套错误对象。
                let error_payload = value.get("error").unwrap_or(&value);
                events.push(AiStreamEvent::ResponseFailed {
                    error: normalize_openai_error(
                        error_payload,
                        value
                            .get("error_type")
                            .or_else(|| value.pointer("/response/error_type")),
                        value.get("status"),
                    ),
                });
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
                        value.get("status"),
                    ),
                });
            }
            "response.output_item.done" => {
                match value.pointer("/item/type").and_then(Value::as_str) {
                    Some("reasoning") => {
                        events.extend(self.completed_reasoning_item(&value)?);
                    }
                    Some("function_call") => {
                        // index identifies whether a preceding added event already opened the tool.
                        // index 标识前序 added 事件是否已经打开该工具。
                        let index = Self::output_index(&value)?;
                        if !self.tools.contains_key(&index) {
                            events.extend(self.output_item_added(&value)?);
                        }
                        events.extend(self.complete_tool(&value)?);
                    }
                    Some("message") => events.extend(self.completed_message_item(&value)?),
                    Some("image_generation_call") => {
                        events.extend(self.completed_image_item(&value)?);
                    }
                    _ => {}
                }
                // providerItem is the authoritative completed Responses item required for exact
                // same-route history replay and provider prefix-cache continuity.
                // providerItem 是同路由精确历史重放与供应商前缀缓存连续性所需的权威
                // Responses 已完成输出项。
                let provider_item = value.get("item").ok_or_else(|| {
                    ProtocolConversionError::new("OpenAI Responses item is missing.")
                })?;
                let provider_item_scope = Self::reasoning_wire_scope(&value);
                if let Some(replay_item) = replayable_provider_context_item(provider_item)
                    && is_replayable_codex_responses_item(&replay_item)
                    && self
                        .captured_provider_context_scopes
                        .insert(provider_item_scope)
                {
                    events.push(AiStreamEvent::ProviderContextItemCaptured { item: replay_item });
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
                let partial_image_index = value
                    .get("partial_image_index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        ProtocolConversionError::new(
                            "OpenAI Responses partial image index is missing or invalid.",
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
                events.push(AiStreamEvent::ImageGenerationPartial {
                    item_id: item_id.clone(),
                    partial_image_index,
                    partial_image_b64: encoded.to_string(),
                });
                events.push(AiStreamEvent::ResourceProgress {
                    resource_id: format!("{item_id}:resource"),
                    state: ResourceTransferState::Running {
                        bytes_processed,
                        bytes_total: None,
                    },
                });
            }
            "response.web_search_call.in_progress"
            | "response.web_search_call.searching"
            | "response.web_search_call.completed" => {}
            "response.content_part.added"
            | "response.content_part.done"
            | "response.output_text.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done" => {}
            // Official Codex ignores unknown non-terminal lifecycle events for forward compatibility.
            // 官方 Codex 会忽略未知的非终态生命周期事件，以保持前向兼容。
            _ => {}
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            return Ok(Vec::new());
        }
        Err(ProtocolConversionError::truncated_stream(
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
pub(crate) fn openai_usage(value: &Value) -> RequestUsage {
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
fn normalize_openai_error(
    error: &Value,
    outer_error_type: Option<&Value>,
    outer_http_status: Option<&Value>,
) -> AiExecutionError {
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .map(str::to_string);
    let error_type = outer_error_type
        .and_then(Value::as_str)
        .or_else(|| error.get("type").and_then(Value::as_str))
        .filter(|value| *value != "error")
        .map(str::to_string);
    // Official Codex overload codes take precedence over generic proxy error types.
    // 官方 Codex 过载代码优先于代理返回的通用错误类型。
    let discriminator = code
        .as_deref()
        .filter(|value| {
            matches!(
                *value,
                "server_is_overloaded"
                    | "slow_down"
                    | "context_length_exceeded"
                    | "max_tokens_exceeded"
                    | "token_limit_exceeded"
            )
        })
        .or(error_type.as_deref())
        .or(code.as_deref());
    let category = match discriminator {
        Some("authentication") | Some("authentication_error") | Some("invalid_api_key") => {
            AiErrorCategory::Authentication
        }
        Some("permission_denied") | Some("permission_error") => AiErrorCategory::PermissionDenied,
        Some("rate_limit_exceeded") | Some("rate_limit_error") => AiErrorCategory::RateLimited,
        Some("server_is_overloaded" | "slow_down" | "provider_overloaded" | "overloaded_error") => {
            AiErrorCategory::ProviderOverloaded
        }
        Some("provider_unavailable") => AiErrorCategory::ProviderUnavailable,
        Some("timeout") => AiErrorCategory::Timeout,
        Some("context_length_exceeded" | "max_tokens_exceeded" | "token_limit_exceeded") => {
            AiErrorCategory::ContextLengthExceeded
        }
        Some("invalid_request") | Some("invalid_request_error") | Some("invalid_prompt") => {
            AiErrorCategory::InvalidRequest
        }
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
        http_status: outer_http_status
            .and_then(Value::as_u64)
            .and_then(|status| u16::try_from(status).ok())
            .or_else(|| {
                error
                    .get("status")
                    .and_then(Value::as_u64)
                    .and_then(|status| u16::try_from(status).ok())
            }),
        transport: None,
        websocket_close_code: None,
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
    use crate::shared::ProtocolStreamDecoder;
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

    /// Verifies Chat reasoning summaries stay transient while opaque payloads remain hidden.
    /// 验证 Chat 思考总结保持临时状态，而不透明载荷继续隐藏。
    #[test]
    fn chat_reasoning_details_map_summaries_and_drop_opaque_payloads() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::new(),
        );
        let summary = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.summary","summary":"先拆分问题"}]} ,"finish_reason":null}]}"#,
            )
            .expect("reasoning summary should decode");
        let revised_summary = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.summary","summary":"，再核对协议"}]} ,"finish_reason":null}]}"#,
            )
            .expect("the next reasoning summary fragment should publish accumulated text");
        let encrypted = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.encrypted","data":"ciphertext","text":"must-not-leak"}]} ,"finish_reason":null}]}"#,
            )
            .expect("encrypted reasoning should be ignored safely");
        let signature_only = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.text","signature":"opaque-signature"}]} ,"finish_reason":null}]}"#,
            )
            .expect("signature-only reasoning should be ignored safely");

        assert!(summary.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "先拆分问题"
        )));
        assert!(!summary.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningStarted { .. } | AiStreamEvent::ReasoningDelta { .. }
        )));
        assert!(revised_summary.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "先拆分问题，再核对协议"
        )));
        assert!(!encrypted.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningStarted { .. } | AiStreamEvent::ReasoningDelta { .. }
        )));
        assert!(!signature_only.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningStarted { .. } | AiStreamEvent::ReasoningDelta { .. }
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
        assert!(matches!(
            terminal.as_slice(),
            [
                AiStreamEvent::ResponseItemCompleted { .. },
                AiStreamEvent::ResponseCompleted
            ]
        ));
    }

    /// Verifies a Luna-style summary stream cannot restart text after a choice finish reason.
    /// 验证 Luna 风格的摘要流不会在选择结束原因后重新启动文本项。
    #[test]
    fn chat_reasoning_summary_trailing_text_keeps_one_item_lifecycle() {
        let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
            String::from("execution"),
            BTreeSet::from([String::from("shell_command")]),
        );
        let first = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"content":"准备调用工具","reasoning_details":[{"type":"reasoning.summary","summary":"先检查文件"}]},"finish_reason":null}]}"#,
            )
            .expect("initial Luna summary and text should decode");
        let finish = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-shell","function":{"name":"shell_command","arguments":"{\"cmd\":\"dir\"}"}}]},"finish_reason":"tool_calls"}]}"#,
            )
            .expect("tool finish reason should remain non-terminal until transport completion");
        assert!(
            !finish
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ToolCallCompleted { .. }))
        );
        let trailing = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"content":"，等待结果"},"finish_reason":null}]}"#,
            )
            .expect("trailing Luna text should extend the existing text item");
        let terminal = decoder
            .push_sse(None, "[DONE]")
            .expect("transport terminal should close the text item once");
        let events = first
            .into_iter()
            .chain(finish)
            .chain(trailing)
            .chain(terminal)
            .collect::<Vec<_>>();

        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ResponseItemStarted { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ResponseItemCompleted { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ToolCallStarted { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ToolCallCompleted { .. }))
                .count(),
            1
        );
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "先检查文件"
        )));
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event {
                    AiStreamEvent::ResponseTextDelta { delta, .. } => Some(delta.as_str()),
                    _ => None,
                })
                .collect::<String>(),
            "准备调用工具，等待结果"
        );
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

    /// Verifies a buffered function call can complete from the official output_item.done fallback.
    /// 验证缓冲函数调用可通过官方 output_item.done 回退完成。
    #[test]
    fn responses_output_item_done_completes_buffered_function_call() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"lookup","arguments":"{\"path\":\"src\"}"}}"#,
            )
            .expect("buffered function call should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { call_id, tool_name, .. }
                if call_id == "call" && tool_name == "lookup"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.id == "call" && call.arguments == json!({"path": "src"})
        )));
    }

    /// Verifies a buffered assistant message can complete without output_text.delta events.
    /// 验证缓冲助手消息可在没有 output_text.delta 事件时完成。
    #[test]
    fn responses_output_item_done_emits_buffered_assistant_text() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"message","role":"assistant","content":[{"type":"output_text","text":"buffered answer"}]}}"#,
            )
            .expect("buffered assistant message should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTextDelta { item_id, delta }
                if item_id == "message" && delta == "buffered answer"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseItemCompleted { item_id } if item_id == "message"
        )));
    }

    /// Verifies summary-only Responses reasoning stays transient and never enters replay.
    /// 验证仅含摘要的 Responses 推理保持临时状态且绝不进入重放。
    #[test]
    fn responses_unsigned_summary_stays_out_of_provider_context() {
        // decoder receives the completed reasoning shape currently returned by DeepSeek Responses.
        // decoder 接收 DeepSeek Responses 当前返回的已完成推理结构。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let provider_item = json!({
            "type": "reasoning",
            "id": "reasoning-item",
            "status": "completed",
            "summary": [{"type": "summary_text", "text": "inspect the workspace"}],
        });
        let events = decoder
            .push_sse(
                Some("response.output_item.done"),
                serde_json::to_string(&json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": provider_item.clone(),
                }))
                .expect("reasoning event should serialize")
                .as_str(),
            )
            .expect("unsigned reasoning should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "inspect the workspace"
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ProviderContextItemCaptured { .. }
                | AiStreamEvent::ReasoningStarted { .. }
                | AiStreamEvent::ReasoningDelta { .. }
        )));
    }

    /// Verifies output-item completion emits only text missing from preceding deltas.
    /// 验证输出项完成事件只发送前序增量中缺失的文本。
    #[test]
    fn responses_output_item_done_completes_partial_text() {
        // decoder owns one partial message prefix with an official upstream item id.
        // decoder 持有一个带官方上游项目标识的部分消息前缀。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_text.delta"),
                r#"{"type":"response.output_text.delta","item_id":"message","output_index":0,"content_index":0,"delta":"buffer"}"#,
            )
            .expect("partial assistant text should decode");
        // completed contains the missing suffix and one exact item completion event.
        // completed 包含缺失后缀和一个精确项目完成事件。
        let completed = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"message","role":"assistant","content":[{"type":"output_text","text":"buffered answer"}]}}"#,
            )
            .expect("completed assistant text should extend the prefix");
        assert!(completed.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTextDelta { item_id, delta }
                if item_id == "message" && delta == "ed answer"
        )));
        assert_eq!(
            completed
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ResponseItemCompleted { .. }))
                .count(),
            1
        );

        // terminal must not repeat the item completion already emitted by output_item.done.
        // terminal 不得重复 output_item.done 已发送的项目完成事件。
        let terminal = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("partial text response should complete");
        assert!(
            terminal
                .iter()
                .all(|event| !matches!(event, AiStreamEvent::ResponseItemCompleted { .. }))
        );
    }

    /// Verifies a completed tool rejects later argument deltas.
    /// 验证已完成工具会拒绝后续参数增量。
    #[test]
    fn responses_tool_delta_after_completion_is_rejected() {
        // decoder opens and completes one official function-call output item.
        // decoder 打开并完成一个官方函数调用输出项。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"tool_item","call_id":"call","name":"lookup","arguments":""}}"#,
            )
            .expect("tool item should open");
        decoder
            .push_sse(
                Some("response.function_call_arguments.done"),
                r#"{"type":"response.function_call_arguments.done","item_id":"tool_item","output_index":0,"call_id":"call","arguments":"{}"}"#,
            )
            .expect("tool item should complete");
        // error proves malformed late deltas cannot mutate a terminal tool call.
        // error 证明格式错误的迟到增量无法修改终态工具调用。
        let error = decoder
            .push_sse(
                Some("response.function_call_arguments.delta"),
                r#"{"type":"response.function_call_arguments.delta","item_id":"tool_item","output_index":0,"call_id":"call","delta":"{}"}"#,
            )
            .expect_err("late tool delta must fail");
        assert!(error.to_string().contains("arguments after completion"));
    }

    /// Verifies tool completion cannot rebind one output index to a different call id.
    /// 验证工具完成事件不能把同一输出索引重新绑定到不同调用标识。
    #[test]
    fn responses_tool_completion_rejects_identity_mutation() {
        // decoder owns the immutable identity established by output_item.added.
        // decoder 持有由 output_item.added 建立的不可变身份。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"tool_item","call_id":"call_one","name":"lookup","arguments":""}}"#,
            )
            .expect("tool item should open");
        // error proves a mismatched completion cannot execute under the earlier tool identity.
        // error 证明不匹配的完成事件无法使用先前工具身份执行。
        let error = decoder
            .push_sse(
                Some("response.function_call_arguments.done"),
                r#"{"type":"response.function_call_arguments.done","item_id":"tool_item","output_index":0,"call_id":"call_two","arguments":"{}"}"#,
            )
            .expect_err("mutated tool call id must fail");
        assert!(error.to_string().contains("changed tool call identity"));
    }

    /// Verifies the public top-level Responses error event is normalized instead of rejected as unknown.
    /// 验证 Responses 公共顶层错误事件会被归一化，而不会被拒绝为未知事件。
    #[test]
    fn responses_top_level_error_event_is_registered() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("error"),
                r#"{"type":"error","code":"invalid_request_error","message":"bad input","param":"input"}"#,
            )
            .expect("top-level Responses error should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.category == AiErrorCategory::InvalidRequest
                    && error.upstream_code.as_deref() == Some("invalid_request_error")
                    && error.upstream_message.as_deref() == Some("bad input")
                    && error.upstream_parameter.as_deref() == Some("input")
                    && error.upstream_type.is_none()
        )));
        assert!(
            decoder
                .finish()
                .expect("terminal error should finish")
                .is_empty()
        );
    }

    /// Verifies Responses streaming preserves the exact context-window error category.
    /// 验证 Responses 流式处理保留精确上下文窗口错误类别。
    #[test]
    fn responses_context_length_error_is_typed() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("error"),
                r#"{"type":"error","code":"context_length_exceeded","message":"too large"}"#,
            )
            .expect("context-window error should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.category == AiErrorCategory::ContextLengthExceeded
        )));
    }

    /// Verifies the official Codex server overload code becomes a retryable provider overload.
    /// 验证官方 Codex 服务过载代码会被归类为可重试的供应商过载错误。
    #[test]
    fn responses_server_overloaded_error_is_retryable() {
        // decoder exercises the complete public Responses SSE error path.
        // decoder 覆盖完整的 Responses 公共 SSE 错误链路。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        // events contain the normalized structured failure consumed by the retry runtime.
        // events 包含供重试运行时消费的归一化结构化失败。
        let events = decoder
            .push_sse(
                Some("error"),
                r#"{"type":"error","code":"server_is_overloaded","message":"Our servers are currently overloaded. Please try again later."}"#,
            )
            .expect("server overload Responses error should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.category == AiErrorCategory::ProviderOverloaded
                    && error.retryable
                    && error.upstream_code.as_deref() == Some("server_is_overloaded")
                    && error.upstream_message.as_deref()
                        == Some("Our servers are currently overloaded. Please try again later.")
        )));
        assert!(
            decoder
                .finish()
                .expect("terminal overload error should finish")
                .is_empty()
        );
    }

    /// Verifies unknown lifecycle metadata follows official Codex forward-compatible handling.
    /// 验证未知生命周期元数据遵循官方 Codex 的前向兼容处理。
    #[test]
    fn responses_unknown_lifecycle_event_is_ignored_until_registered_terminal() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let metadata = decoder
            .push_sse(
                Some("response.proxy_metadata.updated"),
                r#"{"type":"response.proxy_metadata.updated","sequence_number":1}"#,
            )
            .expect("unknown non-terminal metadata should be ignored");
        assert!(
            metadata
                .iter()
                .all(|event| !matches!(event, AiStreamEvent::ResponseCompleted))
        );

        let terminal = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("registered terminal should still complete the response");
        assert!(
            terminal
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
        assert!(
            decoder
                .finish()
                .expect("registered terminal should finish")
                .is_empty()
        );
    }

    /// Verifies a compatibility proxy may append `[DONE]` after the registered Responses terminal event.
    /// 验证兼容代理可在已注册的 Responses 终态事件后追加 `[DONE]`。
    #[test]
    fn responses_trailing_done_sentinel_is_framing_only() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("registered terminal should decode");

        let trailing = decoder
            .push_sse(Some("done"), "[DONE]")
            .expect("trailing compatibility sentinel should be ignored");
        assert!(trailing.is_empty());
        assert!(
            decoder
                .finish()
                .expect("registered terminal should remain valid")
                .is_empty()
        );
    }

    /// Verifies `[DONE]` alone never fabricates a successful Responses terminal event.
    /// 验证单独的 `[DONE]` 绝不会伪造成功的 Responses 终态事件。
    #[test]
    fn responses_done_sentinel_without_terminal_is_rejected_at_finish() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(Some("done"), "[DONE]")
            .expect("compatibility sentinel should remain framing-only");
        assert!(events.is_empty());

        let error = decoder
            .finish()
            .expect_err("missing Responses terminal event must still fail");
        assert!(
            error
                .to_string()
                .contains("ended before a terminal response event")
        );
    }

    /// Verifies a JSON WebSocket Responses event reuses the registered decoder semantics.
    /// 验证 JSON WebSocket Responses 事件复用已注册解码器语义。
    #[test]
    fn responses_json_websocket_event_reuses_decoder() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_json(&json!({
                "type": "error",
                "code": "invalid_request_error",
                "message": "bad websocket input"
            }))
            .expect("JSON WebSocket error should decode");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.upstream_code.as_deref() == Some("invalid_request_error")
                    && error.upstream_message.as_deref() == Some("bad websocket input")
        )));
    }

    /// Verifies a proxy-wrapped Responses error preserves its HTTP status and nested payload.
    /// 验证代理嵌套 Responses 错误会保留 HTTP 状态码与内部载荷。
    #[test]
    fn responses_nested_error_preserves_status() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("error"),
                r#"{"type":"error","status":429,"error":{"code":"rate_limit_error","message":"slow down","param":null}}"#,
            )
            .expect("wrapped Responses error should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.category == AiErrorCategory::RateLimited
                    && error.http_status == Some(429)
                    && error.upstream_code.as_deref() == Some("rate_limit_error")
                    && error.upstream_message.as_deref() == Some("slow down")
        )));
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

    /// Verifies official full reasoning text still requires its exact content index.
    /// 验证官方完整思考文本仍然要求精确的内容索引。
    #[test]
    fn responses_reasoning_text_requires_index() {
        // decoder preserves the strict provider-neutral Responses contract.
        // decoder 保持严格且供应商无关的 Responses 契约。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        // error proves relay-specific adaptation cannot become a global zero-index fallback.
        // error 证明中转站专属适配不能退化为全局第零索引兜底。
        let error = decoder
            .push_sse(
                Some("response.reasoning_text.delta"),
                r#"{"type":"response.reasoning_text.delta","item_id":"rs_123","output_index":0,"delta":"thinking"}"#,
            )
            .expect_err("missing reasoning content index must fail");
        assert!(
            error
                .to_string()
                .contains("reasoning content index is missing")
        );
    }

    /// Verifies the official full reasoning-text delta is normalized into provider-neutral reasoning events.
    /// 验证官方完整思考文本增量会被规范化为供应商无关思考事件。
    #[test]
    fn responses_reasoning_text_delta_is_registered() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("response.reasoning_text.delta"),
                r#"{"type":"response.reasoning_text.delta","item_id":"rs_123","output_index":0,"content_index":0,"delta":"The","sequence_number":1}"#,
            )
            .expect("reasoning text delta should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningStarted { item_id }
                if item_id == "responses:execution:reasoning:0"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { item_id, delta }
                if item_id == "responses:execution:reasoning:0" && delta == "The"
        )));
    }

    /// Verifies a buffered official reasoning-summary completion supplies text when no delta arrived.
    /// 验证在没有增量到达时，官方缓冲推理摘要完成事件会补充文本。
    #[test]
    fn responses_reasoning_summary_done_emits_buffered_text() {
        // decoder owns the exact buffered-only reasoning lifecycle.
        // decoder 持有精确的仅缓冲推理生命周期。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        // events contains the semantic fallback synthesized from the complete done payload.
        // events 包含从完整 done 载荷合成的语义回退。
        let events = decoder
            .push_sse(
                Some("response.reasoning_summary_text.done"),
                r#"{"type":"response.reasoning_summary_text.done","item_id":"rs_123","output_index":0,"summary_index":0,"text":"buffered reasoning"}"#,
            )
            .expect("buffered reasoning summary should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "buffered reasoning"
        )));
        assert!(!events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningStarted { .. } | AiStreamEvent::ReasoningDelta { .. }
        )));
    }

    /// Verifies a reasoning done event never duplicates text already delivered by deltas.
    /// 验证推理 done 事件绝不会重复已经由增量交付的文本。
    #[test]
    fn responses_reasoning_done_does_not_duplicate_deltas() {
        // decoder owns one incremental reasoning item followed by its complete text event.
        // decoder 持有一项增量推理及其后续完整文本事件。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.reasoning_text.delta"),
                r#"{"type":"response.reasoning_text.delta","item_id":"rs_123","output_index":0,"content_index":0,"delta":"incremental"}"#,
            )
            .expect("reasoning delta should decode");
        // done contains the cumulative provider text but must not replay it to the public stream.
        // done 包含供应商累计文本，但不得将其重新播放到公共流。
        let done = decoder
            .push_sse(
                Some("response.reasoning_text.done"),
                r#"{"type":"response.reasoning_text.done","item_id":"rs_123","output_index":0,"content_index":0,"text":"incremental"}"#,
            )
            .expect("reasoning completion should decode");

        assert!(
            done.iter()
                .all(|event| !matches!(event, AiStreamEvent::ReasoningDelta { .. }))
        );
    }

    /// Verifies provider deltas arriving after the dedicated reasoning completion are rejected.
    /// 验证在专用推理完成事件之后到达的供应商增量会被拒绝。
    #[test]
    fn responses_reasoning_delta_after_done_is_rejected() {
        // decoder owns one completed buffered reasoning item.
        // decoder 持有一项已完成的缓冲推理项。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.reasoning_summary_text.done"),
                r#"{"type":"response.reasoning_summary_text.done","item_id":"rs_123","output_index":0,"summary_index":0,"text":"complete"}"#,
            )
            .expect("reasoning completion should decode");
        // error proves out-of-order replay cannot silently duplicate visible reasoning.
        // error 证明乱序重放无法静默复制可见推理内容。
        let error = decoder
            .push_sse(
                Some("response.reasoning_summary_text.delta"),
                r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_123","output_index":0,"summary_index":0,"delta":"late"}"#,
            )
            .expect_err("reasoning delta after completion must fail");
        assert!(error.to_string().contains("delta after completion"));
    }

    /// Verifies repeated local indexes remain isolated across reasoning items and event families.
    /// 验证重复的局部索引在不同推理项与事件族之间保持隔离。
    #[test]
    fn responses_reasoning_local_indexes_do_not_collide() {
        // decoder owns the complete lifecycle needed to verify stable cross-event identities.
        // decoder 持有验证跨事件稳定身份所需的完整生命周期。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        // events aggregates three deltas that intentionally reuse local index zero.
        // events 汇总三个刻意复用局部零索引的增量。
        let mut events = Vec::new();
        events.extend(
            decoder
                .push_sse(
                    Some("response.reasoning_text.delta"),
                    r#"{"type":"response.reasoning_text.delta","item_id":"rs_1","output_index":0,"content_index":0,"delta":"first"}"#,
                )
                .expect("first reasoning item should decode"),
        );
        events.extend(
            decoder
                .push_sse(
                    Some("response.reasoning_text.delta"),
                    r#"{"type":"response.reasoning_text.delta","item_id":"rs_2","output_index":1,"content_index":0,"delta":"second"}"#,
                )
                .expect("second reasoning item should decode"),
        );
        events.extend(
            decoder
                .push_sse(
                    Some("response.reasoning_summary_text.delta"),
                    r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":0,"delta":"summary"}"#,
                )
                .expect("reasoning summary should decode"),
        );

        // started_item_ids proves only full reasoning creates persistent semantic blocks.
        // started_item_ids 证明只有完整思考会创建持久化语义块。
        let started_item_ids = events
            .iter()
            .filter_map(|event| match event {
                AiStreamEvent::ReasoningStarted { item_id } => Some(item_id.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            started_item_ids,
            vec![
                "responses:execution:reasoning:0",
                "responses:execution:reasoning:1",
            ]
        );
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { item_id, delta }
                if item_id == "responses:execution:reasoning:0" && delta == "first"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { item_id, delta }
                if item_id == "responses:execution:reasoning:1" && delta == "second"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "summary"
        )));
    }

    /// Verifies encrypted Responses reasoning never enters semantic or replay state.
    /// 验证 Responses 加密推理绝不进入语义状态或重放状态。
    #[test]
    fn responses_discards_encrypted_reasoning_from_replay() {
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
        assert!(!events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningCompleted {
                signature: Some(_),
                ..
            } | AiStreamEvent::ProviderContextItemCaptured { .. }
        )));
    }

    /// Verifies output-item completion separates transient summaries from full reasoning.
    /// 验证输出项完成事件会分离临时摘要与完整思考。
    #[test]
    fn responses_output_item_done_emits_buffered_reasoning() {
        // decoder owns one reasoning item delivered without any preceding delta event.
        // decoder 持有一项没有任何前序增量事件的推理项。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        // buffered contains the complete summary and raw reasoning parts from the official item shape.
        // buffered 包含官方项目结构中的完整摘要与原始推理部分。
        let buffered = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_buffered","type":"reasoning","summary":[{"type":"summary_text","text":"summary"}],"content":[{"type":"reasoning_text","text":"raw"}],"encrypted_content":"opaque-buffered"}}"#,
            )
            .expect("buffered reasoning item should decode");
        assert!(buffered.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "summary"
        )));
        assert!(buffered.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { item_id, delta }
                if item_id == "responses:execution:reasoning:1" && delta == "raw"
        )));
        // captured_context is the replay-safe full-reasoning item after transient fields are scrubbed.
        // captured_context 是清除瞬态字段后的可安全重放完整思考项目。
        let captured_context = buffered.iter().find_map(|event| match event {
            AiStreamEvent::ProviderContextItemCaptured { item } => Some(item),
            _ => None,
        });
        assert_eq!(
            captured_context
                .and_then(|item| item.pointer("/content/0/text"))
                .and_then(Value::as_str),
            Some("raw")
        );
        assert!(captured_context.is_some_and(|item| {
            item.get("summary").is_none() && item.get("encrypted_content").is_none()
        }));

        // terminal completes only the full reasoning block without carrying encrypted payloads.
        // terminal 只完成完整思考块，且不会携带加密载荷。
        let terminal = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("buffered reasoning response should complete");
        let signatures = terminal
            .iter()
            .filter_map(|event| match event {
                AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                    Some((item_id.as_str(), signature.as_deref()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(signatures, vec![("responses:execution:reasoning:1", None)]);
    }

    /// Verifies a full reasoning completion emits only the suffix missing from streamed deltas.
    /// 验证完整推理完成事件只发送流式增量中缺失的后缀。
    #[test]
    fn responses_reasoning_done_completes_partial_delta() {
        // decoder owns one partial prefix followed by the provider's authoritative full value.
        // decoder 持有一个部分前缀及其后供应商提供的权威完整值。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let partial = decoder
            .push_sse(
                Some("response.reasoning_summary_text.delta"),
                r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_partial","summary_index":0,"delta":"part"}"#,
            )
            .expect("partial reasoning delta should decode");
        assert!(partial.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "part"
        )));
        // completed contains only the missing suffix in its public delta projection.
        // completed 的公共增量投影只包含缺失后缀。
        let completed = decoder
            .push_sse(
                Some("response.reasoning_summary_text.done"),
                r#"{"type":"response.reasoning_summary_text.done","item_id":"rs_partial","summary_index":0,"text":"partial"}"#,
            )
            .expect("full reasoning value should extend the prefix");
        assert!(completed.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text } if text == "partial"
        )));
    }

    /// Verifies encrypted reasoning from multiple upstream items never enters replay.
    /// 验证多个上游项目的加密推理绝不进入重放。
    #[test]
    fn responses_encrypted_reasoning_items_stay_out_of_replay() {
        // decoder owns two upstream reasoning items whose local indexes intentionally collide.
        // decoder 持有两个局部索引刻意冲突的上游推理项目。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        for (item_id, output_index, delta, signature) in [
            ("rs_one", 0, "one", "opaque-one"),
            ("rs_two", 1, "two", "opaque-two"),
        ] {
            decoder
                .push_sse(
                    Some("response.reasoning_summary_text.delta"),
                    format!(
                        r#"{{"type":"response.reasoning_summary_text.delta","item_id":"{item_id}","output_index":{output_index},"summary_index":0,"delta":"{delta}"}}"#
                    )
                    .as_str(),
                )
                .expect("scoped reasoning delta should decode");
            decoder
                .push_sse(
                    Some("response.output_item.done"),
                    format!(
                        r#"{{"type":"response.output_item.done","output_index":{output_index},"item":{{"id":"{item_id}","type":"reasoning","summary":[{{"type":"summary_text","text":"{delta}"}}],"encrypted_content":"{signature}"}}}}"#
                    )
                    .as_str(),
                )
                .expect("scoped reasoning completion should decode");
        }
        // completed_by_item proves summary and encrypted payloads created no persistent blocks.
        // completed_by_item 证明摘要与加密载荷没有创建持久化块。
        let completed_by_item = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("multi-reasoning response should complete")
            .into_iter()
            .filter_map(|event| match event {
                AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                    Some((item_id, signature))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(completed_by_item.is_empty());
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
                r#"{"type":"response.image_generation_call.partial_image","output_index":2,"item_id":"image-item","partial_image_index":0,"partial_image_b64":"aW1hZ2U="}"#,
            )
            .expect("partial image should decode");
        assert!(progress.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResourceProgress { resource_id, .. }
                if resource_id == "image-item:resource"
        )));
        assert!(progress.iter().any(|event| matches!(
            event,
            AiStreamEvent::ImageGenerationPartial {
                item_id,
                partial_image_index: 0,
                partial_image_b64,
            } if item_id == "image-item" && partial_image_b64 == "aW1hZ2U="
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
                r#"{"type":"response.image_generation_call.partial_image","output_index":2,"item_id":"image-item","partial_image_index":1,"partial_image_b64":"aW1hZ2U="}"#,
            )
            .expect_err("progress after image completion must fail");
        assert!(error.to_string().contains("progress after completion"));
    }

    /// Verifies Responses web-search lifecycle events remain provider-managed.
    /// 验证 Responses 网页搜索生命周期事件保持由供应商管理。
    #[test]
    fn responses_web_search_events_are_consumed_without_host_tool_events() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let started = decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"search-item","type":"web_search_call","status":"in_progress"}}"#,
            )
            .expect("web search item should decode");
        assert!(
            started
                .iter()
                .all(|event| !matches!(event, AiStreamEvent::ToolCallStarted { .. }))
        );
        for event_name in [
            "response.web_search_call.in_progress",
            "response.web_search_call.searching",
            "response.web_search_call.completed",
        ] {
            decoder
                .push_sse(
                    Some(event_name),
                    &format!(r#"{{"type":"{event_name}","output_index":0}}"#),
                )
                .expect("web search status should decode");
        }
        decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"search-item","type":"web_search_call","status":"completed"}}"#,
            )
            .expect("web search item should complete");
        let completed = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("response should complete");
        assert!(
            completed
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
    }
}
