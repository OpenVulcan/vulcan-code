//! OpenAI Chat Completions and Responses stream decoders.
//! OpenAI Chat Completions 与 Responses 流解码器。

use crate::{
    ProtocolConversionError, ProtocolStreamDecoder, merge_streamed_tool_name,
    shared::append_bounded_tool_arguments,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiContinuationUpdate, AiErrorCategory, AiExecutionError, AiMessagePhase, AiStreamEvent,
    ContentItem, FreeformToolArgumentEncoding, RequestUsage, ResourceKind, ResourceReference,
    ResourceTransferState, ToolCall, ToolCallProgress, ToolDefinition, ToolInputFormat, UsageValue,
    UsageValueSource, derive_uncached_input, is_replayable_codex_responses_item,
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
    /// Frozen registered input format for this exact tool name.
    /// 当前精确工具名称的冻结注册输入格式。
    input_format: ToolInputFormat,
    /// Accumulated exact custom-tool text.
    /// 累积的精确自定义工具正文。
    custom_text: String,
    /// Whether the output item opened with a non-empty input snapshot, which deltas cannot extend.
    /// 输出项是否以非空输入快照开启；增量不能在其后续接。
    custom_snapshot_opened: bool,
    /// Whether the public start event was emitted.
    /// 是否已经发送公共开始事件。
    started: bool,
    /// Whether the public terminal tool event was emitted.
    /// 是否已经发送公共工具终态事件。
    terminal: bool,
    /// Whether `response.function_call_arguments.done` supplied a complete candidate snapshot.
    /// `response.function_call_arguments.done` 是否已提供完整候选快照。
    arguments_done_seen: bool,
}

/// Carries the three semantically distinct projections decoded from one reasoning-details chunk.
/// 携带从单个 reasoning-details 分片解码出的三类语义不同投影。
#[derive(Debug, Default)]
struct OpenAiReasoningDetailsDelta {
    /// Newly observed durable reasoning suffix.
    /// 新观测到的持久化推理后缀。
    detail_reasoning: Option<String>,
    /// Provider-indexed cumulative summary replacements.
    /// 按供应商索引组织的累计摘要替换值。
    summary_updates: Vec<(String, String)>,
    /// Raw summary fragment used only to suppress an identical mirrored reasoning field.
    /// 仅用于抑制同值镜像 reasoning 字段的原始摘要片段。
    raw_summary_delta: Option<String>,
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

/// Builds one exact replay-safe provider context item for private route-bound persistence.
/// 构建一项用于私有路由绑定持久化的精确可安全重放供应商上下文项目。
///
/// # Parameters
/// 参数
/// `item` is one completed provider-owned Responses output item.
/// `item` 是一个已完成且由供应商持有的 Responses 输出项目。
///
/// # Returns
/// 返回值
/// A cloned replay item, or `None` when a reasoning item carries no reconstructable context.
/// 返回克隆后的重放项目；推理项不携带任何可重建上下文时返回 `None`。
fn replayable_provider_context_item(item: &Value) -> Option<Value> {
    if item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return Some(item.clone());
    }
    // hasFullReasoning admits provider content that can reconstruct durable reasoning.
    // hasFullReasoning 允许能够重建持久化完整思考的供应商正文。
    let has_full_reasoning = item
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|content| !content.is_empty());
    // hasEncryptedContext retains the opaque continuation exactly; callers keep this item inside
    // the versioned route-bound replay envelope and never project it to visible reasoning.
    // hasEncryptedContext 精确保留不透明延续；调用方只把该项放入版本化路由绑定重放封套，绝不投影为可见思考。
    let has_encrypted_context = item
        .get("encrypted_content")
        .and_then(Value::as_str)
        .is_some_and(|encrypted_content| !encrypted_content.is_empty());
    (has_full_reasoning || has_encrypted_context).then(|| item.clone())
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
    /// Cumulative transient `reasoning.summary` text keyed by provider-owned detail index.
    /// 按供应商持有的详情索引保存的累计瞬态 `reasoning.summary` 文本。
    reasoning_summary_text: BTreeMap<u32, String>,
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
            reasoning_summary_text: BTreeMap::new(),
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
        // detailReasoning and summaries keep durable thinking separate from typed summary nodes.
        // detailReasoning 与 summaries 将持久化思考和类型化摘要节点分离。
        let OpenAiReasoningDetailsDelta {
            detail_reasoning,
            summary_updates: summaries,
            raw_summary_delta: summary_delta,
        } = self.decode_reasoning_details_delta(delta)?;
        // reasoningContent is an explicit durable field and therefore outranks every mirror.
        // reasoningContent 是显式持久化字段，因此优先于任何镜像字段。
        let reasoning_content = delta
            .get("reasoning_content")
            .and_then(Value::as_str)
            .map(str::to_string);
        // mirroredReasoning is omitted only when the typed summary proves byte-identical content.
        // mirroredReasoning 仅在类型化摘要证明内容逐字节相同时才被排除。
        let mirrored_reasoning = delta
            .get("reasoning")
            .and_then(Value::as_str)
            .filter(|reasoning| summary_delta.as_deref() != Some(*reasoning))
            .map(str::to_string);
        // reasoning prefers explicit direct durable fields while avoiding duplicate detail content.
        // reasoning 优先采用显式直接持久化字段，同时避免重复详情内容。
        let reasoning = reasoning_content
            .or(mirrored_reasoning)
            .or(detail_reasoning);
        // An empty reasoning field, which relays send on every chunk of a turn that did not
        // think, is no reasoning: opening an item for it would record a reasoned turn whose text
        // a declared replay contract then rejects as lost.
        // 空推理字段（中转在未思考回合的每个分片都会发送）不是推理：为它开启推理项会记录一个
        // 「已推理」回合，其文本随后会被已声明的回放契约当作丢失而拒绝。
        if let Some(reasoning) = reasoning.filter(|reasoning| !reasoning.is_empty()) {
            let item_id = format!("openai:{}:reasoning", self.execution_id);
            if !self.reasoning_open {
                self.reasoning_open = true;
                events.push(AiStreamEvent::ReasoningStarted {
                    item_id: item_id.clone(),
                });
            }
            events.push(AiStreamEvent::ReasoningDelta {
                item_id,
                delta: reasoning,
            });
        }
        for (item_id, text) in summaries {
            events.push(AiStreamEvent::ReasoningSummaryUpdated { item_id, text });
        }
        // One wire chunk may contain both the last reasoning token and the first answer token.
        // 一个线上分片可能同时包含最后一个思考 token 和第一个回答 token。
        // Emit reasoning first: the host seals it when answer streaming begins.
        // 必须先发出思考：宿主在回答流开始时会封口思考。
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            // item_id preserves the physical response's stable text identity.
            // item_id 保留真实响应的稳定正文身份。
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
    /// Newly observed full-reasoning suffix, indexed summary replacements, and raw summary delta.
    /// 返回新观测到的完整推理后缀、带索引的摘要替换值与原始摘要增量。
    fn decode_reasoning_details_delta(
        &mut self,
        delta: &Value,
    ) -> Result<OpenAiReasoningDetailsDelta, ProtocolConversionError> {
        // details is the exact provider-owned typed reasoning array for this chunk.
        // details 是当前分片中由供应商持有的精确类型化推理数组。
        let Some(details) = delta.get("reasoning_details").and_then(Value::as_array) else {
            return Ok(OpenAiReasoningDetailsDelta::default());
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
        // summaryUpdates preserve every provider summary node instead of flattening unrelated indexes.
        // summaryUpdates 保留每个供应商摘要节点，而不是压平无关索引。
        let mut summary_updates = Vec::new();
        // rawSummaryDelta proves whether an untyped top-level reasoning field mirrors this chunk.
        // rawSummaryDelta 用于证明无类型顶层 reasoning 字段是否镜像当前分片。
        let mut raw_summary_delta = String::new();
        for (array_index, detail) in details.iter().enumerate() {
            if detail.get("type").and_then(Value::as_str) != Some("reasoning.summary") {
                continue;
            }
            let Some(summary) = detail.get("summary").and_then(Value::as_str) else {
                continue;
            };
            let fallback_index = u32::try_from(array_index).map_err(|_| {
                ProtocolConversionError::new("OpenAI Chat reasoning detail index exceeds u32.")
            })?;
            let index = match detail.get("index") {
                Some(Value::Number(index)) => index
                    .as_u64()
                    .and_then(|index| u32::try_from(index).ok())
                    .ok_or_else(|| {
                        ProtocolConversionError::new(
                            "OpenAI Chat reasoning detail index is invalid or exceeds u32.",
                        )
                    })?,
                Some(_) => {
                    return Err(ProtocolConversionError::new(
                        "OpenAI Chat reasoning detail index is invalid.",
                    ));
                }
                None => fallback_index,
            };
            raw_summary_delta.push_str(summary);
            let accumulated = self.reasoning_summary_text.entry(index).or_default();
            accumulated.push_str(summary);
            summary_updates.push((
                format!("openai:{}:reasoning-summary:{index}", self.execution_id),
                accumulated.clone(),
            ));
        }
        Ok(OpenAiReasoningDetailsDelta {
            detail_reasoning: cumulative_suffix(&mut self.reasoning_details_text, reasoning),
            summary_updates,
            raw_summary_delta: (!raw_summary_delta.is_empty()).then_some(raw_summary_delta),
        })
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

    /// Finalizes buffered calls at the transport boundary, preserving unknown names for host rejection.
    /// 在传输边界完成缓存调用，保留未知工具名以交由宿主拒绝。
    /// No parameters; returns ordered lifecycle events, or an error for missing wire identity.
    /// 无参数；返回有序生命周期事件，或在传输身份缺失时返回错误。
    fn complete_tools(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        // Validate all identities before mutating any sibling's terminal state.
        // 修改任何同批调用的终态前，先校验全部身份。
        for state in self.tools.values().filter(|state| !state.terminal) {
            if state.call_id.is_empty() || state.tool_name.is_empty() {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Chat tool item `{}` ended with an incomplete identity (missing call id: {}, missing tool name: {}).",
                    state.item_id,
                    state.call_id.is_empty(),
                    state.tool_name.is_empty(),
                )));
            }
        }
        let mut events = Vec::new();
        for (index, state) in self.tools.iter_mut().filter(|(_, state)| !state.terminal) {
            state.terminal = true;
            if !state.started {
                // The boundary makes the accumulated name final even when it is not registered.
                // 边界使累积名称成为最终名称，即使该名称未注册。
                // Registration is an execution check; skipping start here breaks the public lifecycle.
                // 注册校验属于执行层；在此跳过开始事件会破坏公共生命周期。
                state.started = true;
                events.push(AiStreamEvent::ToolCallStarted {
                    item_id: state.item_id.clone(),
                    call_id: state.call_id.clone(),
                    tool_name: state.tool_name.clone(),
                    index: *index,
                });
                if !state.arguments.is_empty() {
                    events.push(tool_argument_delta(state, state.arguments.as_str())?);
                }
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
        Ok(events)
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
            events.extend(self.complete_tools()?);
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
            events.extend(self.complete_tools()?);
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
    /// Frozen input formats keyed by the exact model-visible tool name.
    /// 按精确模型可见工具名称保存的冻结输入格式。
    registered_tool_inputs: BTreeMap<String, ToolInputFormat>,
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
    #[cfg(test)]
    pub(crate) fn new(execution_id: String) -> Self {
        Self::with_registered_tools(execution_id, &[])
    }

    /// Creates one Responses decoder with the immutable request tool registry.
    /// 使用不可变请求工具注册表创建一个 Responses 解码器。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the stable execution identifier and `tools` is the exact request-visible
    /// tool set that owns custom-input interpretation.
    /// `execution_id` 是稳定执行标识，`tools` 是拥有自定义输入解释权的精确请求可见工具集合。
    ///
    /// # Returns
    /// 返回值
    /// A fresh decoder whose custom-tool events cannot change the registered input format.
    /// 返回一个自定义工具事件不能修改注册输入格式的新解码器。
    pub(crate) fn with_registered_tools(execution_id: String, tools: &[ToolDefinition]) -> Self {
        // registeredToolInputs freezes name-to-format ownership before any provider bytes arrive.
        // registeredToolInputs 在任何供应商字节到达前冻结名称到格式的归属。
        let registered_tool_inputs = tools
            .iter()
            .map(|tool| (tool.name.clone(), tool.input_format.clone()))
            .collect();
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
            registered_tool_inputs,
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
        let item_type = item.get("type").and_then(Value::as_str);
        let custom_input = item_type == Some("custom_tool_call");
        if item_type != Some("function_call") && !custom_input {
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
        // inputFormat is owned by the frozen request registry, never inferred from provider syntax.
        // inputFormat 由冻结请求注册表持有，绝不根据供应商语法推断。
        let input_format = self
            .registered_tool_inputs
            .get(tool_name.as_str())
            .cloned()
            .unwrap_or_default();
        if custom_input && matches!(input_format, ToolInputFormat::JsonSchema) {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses emitted custom input for JSON-schema tool `{tool_name}`."
            )));
        }
        if !custom_input && matches!(input_format, ToolInputFormat::Freeform { .. }) {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses emitted function arguments for freeform tool `{tool_name}`."
            )));
        }
        let provider_item_id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|item_id| !item_id.is_empty())
            .map(str::to_string);
        let item_id = provider_item_id
            .clone()
            .unwrap_or_else(|| format!("responses:{}:tool:{index}", self.execution_id));
        let custom_text = item
            .get("input")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.tools.insert(
            index,
            OpenAiToolState {
                item_id: item_id.clone(),
                provider_item_id,
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                arguments: if custom_input {
                    String::new()
                } else {
                    item.get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                },
                input_format,
                custom_snapshot_opened: !custom_text.is_empty(),
                custom_text,
                started: true,
                terminal: false,
                arguments_done_seen: false,
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
        // reportedItemId gives an authoritative completed nested item precedence over compatibility-only top-level fields.
        // reportedItemId 让权威的已完成嵌套项目优先于仅用于兼容的顶层字段。
        let reported_item_id = value
            .pointer("/item/id")
            .and_then(Value::as_str)
            .or_else(|| value.get("item_id").and_then(Value::as_str))
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
            .pointer("/item/call_id")
            .and_then(Value::as_str)
            .or_else(|| value.get("call_id").and_then(Value::as_str));
        if reported_call_id.is_some_and(|reported| reported != state.call_id) {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses changed tool call identity.",
            ));
        }
        // reportedToolName prevents completion from changing the registered tool target.
        // reportedToolName 防止完成事件修改已注册工具目标。
        let reported_tool_name = value
            .pointer("/item/name")
            .and_then(Value::as_str)
            .or_else(|| value.get("name").and_then(Value::as_str));
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
        if state.terminal || state.arguments_done_seen {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses tool output index `{index}` emitted arguments after completion."
            )));
        }
        Self::validate_tool_identity(state, value)?;
        append_bounded_tool_arguments(&mut state.arguments, delta, "OpenAI Responses")?;
        Ok(vec![tool_argument_delta(state, delta)?])
    }

    /// Decodes one raw custom-tool input delta without changing its registered representation.
    /// 解码一段原始自定义工具输入增量，且不改变其注册表示。
    fn custom_tool_input_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = Self::output_index(value)?;
        let delta = value.get("delta").and_then(Value::as_str).ok_or_else(|| {
            ProtocolConversionError::new("OpenAI Responses custom-tool delta is missing.")
        })?;
        let state = self.tools.get_mut(&index).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "OpenAI Responses custom-tool delta references unopened index `{index}`."
            ))
        })?;
        if !matches!(state.input_format, ToolInputFormat::Freeform { .. }) {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses emitted custom-tool input for a function tool.",
            ));
        }
        if state.terminal || state.arguments_done_seen {
            return Err(ProtocolConversionError::new(format!(
                "OpenAI Responses custom-tool index `{index}` emitted input after completion."
            )));
        }
        // Only an opening snapshot excludes deltas; text accumulated from earlier deltas does not.
        // 只有开启时的快照排斥增量；由先前增量累积的正文不受影响。
        if state.custom_snapshot_opened {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses custom-tool input delta followed a non-empty initial input snapshot.",
            ));
        }
        Self::validate_tool_identity(state, value)?;
        append_bounded_tool_arguments(
            &mut state.custom_text,
            delta,
            "OpenAI Responses custom tool",
        )?;
        Ok(vec![tool_argument_delta(state, delta)?])
    }

    /// Records authoritative raw custom-tool input without terminating the tool early.
    /// 记录权威原始自定义工具输入且不提前终止工具。
    fn record_custom_tool_input_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = Self::output_index(value)?;
        let state = self.tools.get_mut(&index).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "OpenAI Responses custom-tool completion references unopened index `{index}`."
            ))
        })?;
        Self::validate_tool_identity(state, value)?;
        if !matches!(state.input_format, ToolInputFormat::Freeform { .. }) {
            return Err(ProtocolConversionError::new(
                "OpenAI Responses emitted custom-tool completion for a function tool.",
            ));
        }
        if state.terminal || state.arguments_done_seen {
            return Ok(Vec::new());
        }
        if let Some(input) = value.get("input").and_then(Value::as_str) {
            state.custom_text = input.to_string();
        }
        state.arguments_done_seen = true;
        Ok(Vec::new())
    }

    /// Records one non-authoritative function-argument completion candidate.
    /// 记录一个非权威函数参数完成候选值。
    ///
    /// # Parameters
    /// 参数
    /// `value` is `response.function_call_arguments.done`; its arguments may still be corrected by
    /// the later authoritative `response.output_item.done` item.
    /// `value` 是 `response.function_call_arguments.done`；其参数仍可能被后续权威
    /// `response.output_item.done` 项修正。
    ///
    /// # Returns
    /// 返回值
    /// No semantic terminal event after the candidate is validated and retained.
    /// 候选值通过校验并保留后不返回语义终态事件。
    fn record_tool_arguments_done(
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
        if state.terminal || state.arguments_done_seen {
            return Ok(Vec::new());
        }
        if let Some(arguments) = value.get("arguments").and_then(Value::as_str) {
            state.arguments.clear();
            append_bounded_tool_arguments(&mut state.arguments, arguments, "OpenAI Responses")?;
        }
        state.arguments_done_seen = true;
        Ok(Vec::new())
    }

    /// Builds the one public terminal event for an authoritative tool-argument snapshot.
    /// 为权威工具参数快照构建唯一公共终态事件。
    ///
    /// # Parameters
    /// 参数
    /// `state` owns the immutable tool identity and exact final argument text.
    /// `state` 持有不可变工具身份与精确最终参数文本。
    ///
    /// # Returns
    /// 返回值
    /// A completed tool call for valid JSON, otherwise a content-free abort diagnostic.
    /// JSON 合法时返回已完成工具调用，否则返回不含正文的中止诊断。
    fn finalize_tool_state(state: &mut OpenAiToolState) -> AiStreamEvent {
        state.terminal = true;
        let arguments = match &state.input_format {
            ToolInputFormat::JsonSchema => serde_json::from_str::<Value>(state.arguments.as_str()),
            ToolInputFormat::Freeform {
                argument_encoding, ..
            } => Ok(match argument_encoding {
                FreeformToolArgumentEncoding::RawText => Value::String(state.custom_text.clone()),
                FreeformToolArgumentEncoding::ApplyPatchObject => {
                    serde_json::json!({"patch": state.custom_text})
                }
            }),
        };
        match arguments {
            Ok(arguments) => AiStreamEvent::ToolCallCompleted {
                item_id: state.item_id.clone(),
                call: ToolCall {
                    id: state.call_id.clone(),
                    name: state.tool_name.clone(),
                    arguments,
                    signature: None,
                },
            },
            Err(error) => AiStreamEvent::ToolCallAborted {
                item_id: state.item_id.clone(),
                message: format!(
                    "OpenAI Responses tool input ended before valid JSON was complete: {error}"
                ),
            },
        }
    }

    /// Completes one response function call from an authoritative output item.
    /// 根据权威输出项完成一个响应函数调用。
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
        if matches!(state.input_format, ToolInputFormat::Freeform { .. }) {
            if let Some(input) = value
                .pointer("/item/input")
                .or_else(|| value.get("input"))
                .and_then(Value::as_str)
            {
                state.custom_text = input.to_string();
            }
        } else if let Some(arguments) = value
            .pointer("/item/arguments")
            .or_else(|| value.get("arguments"))
            .and_then(Value::as_str)
        {
            state.arguments.clear();
            append_bounded_tool_arguments(&mut state.arguments, arguments, "OpenAI Responses")?;
        }
        Ok(vec![Self::finalize_tool_state(state)])
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
        // phase is closed by the Codex Responses contract; unknown strings must not silently
        // become final answers in the host transcript.
        // phase 由 Codex Responses 契约封闭；未知字符串不得在宿主转录中静默变成最终答复。
        let phase = match item.get("phase") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value == "commentary" => Some(AiMessagePhase::Commentary),
            Some(Value::String(value)) if value == "final_answer" => {
                Some(AiMessagePhase::FinalAnswer)
            }
            Some(Value::String(value)) => {
                return Err(ProtocolConversionError::new(format!(
                    "OpenAI Responses message phase `{value}` is not registered."
                )));
            }
            Some(_) => {
                return Err(ProtocolConversionError::new(
                    "OpenAI Responses message phase must be a string when present.",
                ));
            }
        };
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
        if let Some(phase) = phase {
            events.push(AiStreamEvent::ResponseMessagePhaseUpdated {
                item_id: item_id.clone(),
                phase,
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
                    item_id: item_id.to_string(),
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

    /// Finalizes done-candidate tools when a successful response omits output-item completion.
    /// 在成功响应省略输出项完成事件时收口已收到 done 候选值的工具。
    ///
    /// # Parameters
    /// 参数
    /// `terminal_event` identifies the successful provider boundary closing the response.
    /// `terminal_event` 标识关闭响应的成功 Provider 边界。
    ///
    /// # Returns
    /// 返回值
    /// Completed or malformed-argument events for candidates, plus deterministic aborts for tools
    /// that never supplied any completion candidate.
    /// 为候选值返回完成或参数非法事件，并为从未提供完成候选值的工具返回确定性中止事件。
    fn finalize_tools_at_response_completion(
        &mut self,
        terminal_event: &str,
    ) -> Vec<AiStreamEvent> {
        let mut events = Vec::new();
        for state in self.tools.values_mut().filter(|state| !state.terminal) {
            if state.arguments_done_seen {
                events.push(Self::finalize_tool_state(state));
            } else {
                state.terminal = true;
                events.push(AiStreamEvent::ToolCallAborted {
                    item_id: state.item_id.clone(),
                    message: format!(
                        "OpenAI Responses emitted `{terminal_event}` before the tool input completion event."
                    ),
                });
            }
        }
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
                        resource_path: None,
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
                        item_id,
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
                events.extend(self.record_tool_arguments_done(&value)?);
            }
            "response.custom_tool_call_input.delta" => {
                events.extend(self.custom_tool_input_delta(&value)?);
            }
            "response.custom_tool_call_input.done" => {
                events.extend(self.record_custom_tool_input_done(&value)?);
            }
            "response.completed" => {
                events.extend(self.capture_response_id(&value)?);
                events.extend(self.close_semantic_items());
                events.extend(self.finalize_tools_at_response_completion("response.completed"));
                events.extend(self.fail_incomplete_images("response.completed"));
                if let Some(usage) = value
                    .pointer("/response/usage")
                    .or_else(|| value.get("usage"))
                {
                    events.push(AiStreamEvent::UsageUpdated {
                        usage: Box::new(openai_usage(usage)),
                    });
                }
                if let Some(end_turn) = value
                    .pointer("/response/end_turn")
                    .or_else(|| value.get("end_turn"))
                    .filter(|value| !value.is_null())
                {
                    let end_turn = end_turn.as_bool().ok_or_else(|| {
                        ProtocolConversionError::new(
                            "OpenAI Responses end_turn must be a boolean when present.",
                        )
                    })?;
                    events.push(AiStreamEvent::ResponseTurnBoundary { end_turn });
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
                    Some("function_call" | "custom_tool_call") => {
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
    // receivedInput follows the frozen tool format so progress counts exact provider text without
    // pretending that custom input is function-call JSON.
    // receivedInput 遵循冻结工具格式，使进度精确统计供应商正文，且不把自定义输入伪装成函数 JSON。
    let received_input = match state.input_format {
        ToolInputFormat::JsonSchema => state.arguments.as_str(),
        ToolInputFormat::Freeform { .. } => state.custom_text.as_str(),
    };
    Ok(AiStreamEvent::ToolCallArgumentsDelta {
        item_id: state.item_id.clone(),
        delta: delta.to_string(),
        progress: ToolCallProgress {
            call_id: Some(state.call_id.clone()),
            tool_name: Some(state.tool_name.clone()),
            bytes_received: u64::try_from(received_input.len()).map_err(|_| {
                ProtocolConversionError::new("OpenAI tool argument byte count overflowed.")
            })?,
            characters_received: u64::try_from(received_input.chars().count()).map_err(|_| {
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
        // OpenAI reports an exhausted plan or balance as `insufficient_quota`; retrying cannot succeed.
        // OpenAI 以 `insufficient_quota` 报告套餐或余额耗尽；重试不可能成功。
        Some("insufficient_quota") => AiErrorCategory::PaymentRequired,
        Some("rate_limit_exceeded") | Some("rate_limit_error") => AiErrorCategory::RateLimited,
        Some("server_is_overloaded" | "slow_down" | "provider_overloaded" | "overloaded_error") => {
            AiErrorCategory::ProviderOverloaded
        }
        Some("provider_unavailable") => AiErrorCategory::ProviderUnavailable,
        Some("STREAM_ERROR") => AiErrorCategory::ProviderUnavailable,
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

    /// Builds one explicitly registered freeform tool for decoder tests.
    /// 为解码器测试构建一个显式注册的自由文本工具。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the exact model-visible name and `argument_encoding` selects the host value.
    /// `name` 是精确模型可见名称，`argument_encoding` 选择宿主值。
    ///
    /// # Returns
    /// 返回值
    /// A grammar-constrained freeform definition.
    /// 返回受 grammar 约束的自由文本定义。
    fn freeform_tool(
        name: &str,
        argument_encoding: FreeformToolArgumentEncoding,
    ) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: String::from("Consumes exact freeform text."),
            input_schema: json!({"type": "object"}),
            input_format: ToolInputFormat::Freeform {
                format: vulcan_ai_contract::FreeformToolFormat::Grammar {
                    syntax: vulcan_ai_contract::FreeformToolSyntax::Lark,
                    definition: String::from("start: /[\\s\\S]+/"),
                },
                argument_encoding,
            },
            strict: false,
        }
    }

    /// Verifies empty reasoning fields on a turn that did not think open no reasoning item.
    /// 验证未思考回合中的空推理字段不会开启推理项。
    #[test]
    fn chat_empty_reasoning_fields_open_no_reasoning_item() {
        for reasoning_field in [json!({"reasoning_content":""}), json!({"reasoning":""})] {
            let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
                String::from("empty-reasoning"),
                BTreeSet::new(),
            );
            let mut delta = reasoning_field;
            delta["content"] = json!("t");
            let events = decoder
                .push_sse(None, &json!({"choices":[{"delta":delta}]}).to_string())
                .expect("a chunk with an empty reasoning field must decode");
            assert!(
                !events.iter().any(|event| matches!(
                    event,
                    AiStreamEvent::ReasoningStarted { .. } | AiStreamEvent::ReasoningDelta { .. }
                )),
                "an empty reasoning field must not open a reasoning item: {events:?}"
            );
        }
    }

    /// Verifies co-packed reasoning and answer fragments preserve semantic order through completion.
    /// 验证同片思考与回答片段直到完成均保持语义顺序。
    /// No inputs or return value; covers explicit, mirrored and cumulative reasoning contracts.
    /// 无参数或返回值；覆盖显式、镜像与累计思考契约。
    #[test]
    fn chat_mixed_reasoning_and_text_chunk_emits_reasoning_before_answer() {
        for reasoning_field in [
            json!({"reasoning_content":".", "reasoning":"."}),
            json!({"reasoning":"."}),
            json!({"reasoning_details":[{"type":"reasoning.text","text":"."}]}),
        ] {
            // decoder represents a fresh physical request, independently of its provider name.
            // decoder 表示一个与供应商名称无关的全新物理请求。
            let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
                String::from("mixed-chunk"),
                BTreeSet::new(),
            );
            // delta recreates the observed boundary: one final reasoning token and first answer token.
            // delta 重现观测到的边界：一个最终思考 token 与首个回答 token。
            let mut delta = reasoning_field;
            delta["content"] = json!("t");
            // events must not depend on JSON object key order.
            // events 不得依赖 JSON 对象键顺序。
            let events = decoder
                .push_sse(None, &json!({"choices":[{"delta":delta}]}).to_string())
                .expect("mixed semantic chunk must decode");
            // semantics retains only visible deltas, so lifecycle additions cannot shift test indexes.
            // semantics 仅保留可见增量，生命周期新增不会改变测试索引。
            let semantics = events
                .iter()
                .filter_map(|event| match event {
                    AiStreamEvent::ReasoningDelta { delta, .. } => {
                        Some(("reasoning", delta.as_str()))
                    }
                    AiStreamEvent::ResponseTextDelta { delta, .. } => {
                        Some(("text", delta.as_str()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                semantics,
                [("reasoning", "."), ("text", "t")],
                "co-packed reasoning must precede text and mirrored fields must not duplicate it"
            );
            decoder
                .push_sse(
                    None,
                    r#"{"choices":[{"delta":{"content":"est"},"finish_reason":"stop"}]}"#,
                )
                .expect("answer continuation must decode");
            assert!(
                decoder
                    .push_sse(None, "[DONE]")
                    .expect("terminal must decode")
                    .iter()
                    .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
            );
        }
    }

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
                r#"{"choices":[{"delta":{"reasoning":"先拆分问题","reasoning_details":[{"type":"reasoning.summary","index":0,"summary":"先拆分问题"}]} ,"finish_reason":null}]}"#,
            )
            .expect("mirrored reasoning summary should decode once");
        let revised_summary = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.summary","index":0,"summary":"，再核对协议"}]} ,"finish_reason":null}]}"#,
            )
            .expect("the next reasoning summary fragment should publish accumulated text");
        let next_summary = decoder
            .push_sse(
                None,
                r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.summary","index":1,"summary":"检查工具结果"}]} ,"finish_reason":null}]}"#,
            )
            .expect("a new reasoning summary index should open an independent node");
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
            AiStreamEvent::ReasoningSummaryUpdated { item_id, text }
                if item_id == "openai:execution:reasoning-summary:0" && text == "先拆分问题"
        )));
        assert!(!summary.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningStarted { .. } | AiStreamEvent::ReasoningDelta { .. }
        )));
        assert!(revised_summary.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "先拆分问题，再核对协议"
        )));
        assert!(next_summary.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { item_id, text }
                if item_id == "openai:execution:reasoning-summary:1" && text == "检查工具结果"
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
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "先检查文件"
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

    /// Checks known and unknown calls against the real lifecycle through both Chat terminal boundaries.
    /// 使用真实生命周期校验已知和未知调用，覆盖两种 Chat 终态边界。
    /// No parameters or return value; unknown tools remain model-correctable host errors.
    /// 无参数或返回值；未知工具仍可通过宿主错误回执让模型纠正。
    #[test]
    fn chat_unknown_tool_keeps_valid_lifecycle_with_registered_siblings() {
        for eof in [false, true] {
            for arguments in ["{}", "{"] {
                // Decoder receives three registered siblings and a fragmented unknown fourth name.
                // 解码器接收三个已注册同批调用，以及第四个未知工具名的片段。
                let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
                    String::from("unknown-tool"),
                    BTreeSet::from([String::from("file")]),
                );
                // Machine validates exactly the event sequence consumed by the public provider layer.
                // 状态机校验公共供应商层实际消费的事件序列。
                let mut machine = vulcan_ai_contract::AiStreamStateMachine::default();
                let mut calls = (0..3)
                    .map(|index| {
                        json!({
                            "index": index, "id": format!("call-{index}"),
                            "function": {"name": "file", "arguments": "{}"},
                        })
                    })
                    .collect::<Vec<_>>();
                calls.push(json!({"index":3,"id":"call-3","function":{"name":"invented_","arguments":arguments}}));
                let mut events = decoder
                    .push_sse(
                        None,
                        &json!({"choices":[{"delta":{"tool_calls":calls}}]}).to_string(),
                    )
                    .unwrap();
                events.extend(decoder.push_sse(None, r#"{"choices":[{"delta":{"tool_calls":[{"index":3,"function":{"name":"tool"}}]},"finish_reason":"tool_calls"}]}"#).unwrap());
                // StepFun keeps empty identity fields in argument-only deltas; these must preserve the unknown name.
                // StepFun 在仅含参数的增量中保留空身份字段；这些字段必须保留已收到的未知工具名。
                events.extend(decoder.push_sse(None, r#"{"choices":[{"delta":{"tool_calls":[{"index":3,"id":"","function":{"name":"","arguments":""}}]}}]}"#).unwrap());
                events.extend(if eof {
                    decoder.finish().unwrap()
                } else {
                    decoder.push_sse(None, "[DONE]").unwrap()
                });
                for event in &events {
                    machine
                        .apply(event)
                        .expect("all starts, deltas and terminals must balance");
                }
                assert!(events.iter().any(|event| matches!(event,
                    AiStreamEvent::ToolCallStarted { tool_name, call_id, index: 3, .. }
                    if tool_name == "invented_tool" && call_id == "call-3"
                )));
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, AiStreamEvent::ToolCallStarted { .. }))
                        .count(),
                    4
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, AiStreamEvent::ToolCallCompleted { .. }))
                        .count(),
                    if arguments == "{}" { 4 } else { 3 }
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, AiStreamEvent::ToolCallAborted { .. }))
                        .count(),
                    usize::from(arguments != "{}")
                );
                assert!(matches!(
                    events.last(),
                    Some(AiStreamEvent::ResponseCompleted)
                ));
            }
        }
    }

    /// Rejects genuinely absent identities without inventing call ids or emitting an unstarted abort.
    /// 拒绝真正缺失的身份，不伪造调用标识，也不发送未开始调用的中止事件。
    /// No parameters or return value; both transport boundaries must expose the original protocol defect.
    /// 无参数或返回值；两种传输边界都必须暴露原始协议缺陷。
    #[test]
    fn chat_missing_identity_reports_the_missing_fields() {
        for eof in [false, true] {
            for (id, name) in [("", "file"), ("call", ""), ("", "")] {
                let mut decoder = OpenAiChatStreamDecoder::with_registered_tools(
                    String::from("missing-identity"),
                    BTreeSet::from([String::from("file")]),
                );
                decoder
                    .push_sse(
                        None,
                        &json!({"choices":[{"delta":{"tool_calls":[{
                    "index":3,"id":id,"function":{"name":name,"arguments":"{}"}
                }]},"finish_reason":"tool_calls"}]})
                        .to_string(),
                    )
                    .unwrap();
                let error = if eof {
                    decoder.finish()
                } else {
                    decoder.push_sse(None, "[DONE]")
                }
                .unwrap_err();
                let message = error.to_string();
                assert!(message.contains("incomplete identity"));
                assert!(message.contains(&format!("missing call id: {}", id.is_empty())));
                assert!(message.contains(&format!("missing tool name: {}", name.is_empty())));
                assert!(!message.contains("was not started"));
            }
        }
    }

    /// Verifies malformed arguments.done waits for an authoritative valid output item.
    /// 验证非法 arguments.done 会等待权威且合法的输出项。
    #[test]
    fn responses_malformed_arguments_done_is_repaired_by_output_item_done() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"edit"}}"#,
            )
            .expect("tool should start");
        // candidateEvents must not abort the turn before output_item.done can repair the snapshot.
        // candidateEvents 不得在 output_item.done 能够修正快照前中止回合。
        let candidate_events = decoder
            .push_sse(
                Some("response.function_call_arguments.done"),
                r#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"text\":\""}"#,
            )
            .expect("done should normalize");
        assert!(candidate_events.iter().all(|event| !matches!(
            event,
            AiStreamEvent::ToolCallAborted { .. } | AiStreamEvent::ToolCallCompleted { .. }
        )));

        // completedEvents proves the later output item is authoritative over the malformed candidate.
        // completedEvents 证明后续输出项相对于非法候选值具有权威性。
        let completed_events = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"arguments":"{\"text\":\"stale\"}","item":{"type":"function_call","id":"item","call_id":"call","name":"edit","arguments":"{\"text\":\"fixed\"}"}}"#,
            )
            .expect("authoritative output item should repair the candidate");
        assert!(completed_events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.arguments == json!({"text": "fixed"})
        )));
        assert!(
            completed_events
                .iter()
                .all(|event| !matches!(event, AiStreamEvent::ToolCallAborted { .. }))
        );
    }

    /// Verifies a valid arguments.done candidate completes at response completion when no item follows.
    /// 验证未跟随输出项时，合法 arguments.done 候选值会在响应完成时收口。
    #[test]
    fn responses_arguments_done_candidate_completes_at_response_terminal() {
        // decoder opens one tool whose compatible provider emits no output_item.done event.
        // decoder 打开一个兼容 Provider 不发送 output_item.done 事件的工具。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"edit"}}"#,
            )
            .expect("tool should start");
        // candidateEvents remain non-terminal until either the authoritative item or response boundary.
        // candidateEvents 在权威输出项或响应边界到来前保持非终态。
        let candidate_events = decoder
            .push_sse(
                Some("response.function_call_arguments.done"),
                r#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"text\":\"candidate\"}"}"#,
            )
            .expect("valid candidate should be retained");
        assert!(candidate_events.is_empty());

        // terminalEvents use the retained candidate only because no authoritative output item arrived.
        // terminalEvents 仅因未收到权威输出项而使用已保留候选值。
        let terminal_events = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"usage":{}}}"#,
            )
            .expect("response completion should finalize the candidate");
        assert!(terminal_events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.arguments == json!({"text": "candidate"})
        )));
        assert!(
            terminal_events
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
    }

    /// Verifies malformed authoritative output-item arguments still abort exactly once.
    /// 验证权威输出项参数非法时仍会恰好中止一次。
    #[test]
    fn responses_malformed_output_item_done_aborts_tool_once() {
        // decoder first receives one malformed non-authoritative candidate.
        // decoder 首先收到一个非法的非权威候选值。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"edit"}}"#,
            )
            .expect("tool should start");
        decoder
            .push_sse(
                Some("response.function_call_arguments.done"),
                r#"{"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"text\":\""}"#,
            )
            .expect("malformed candidate should remain recoverable");

        // abortedEvents prove only the authoritative malformed item closes the tool as invalid.
        // abortedEvents 证明只有权威非法输出项才会将工具关闭为无效。
        let aborted_events = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"edit","arguments":"{\"text\":\""}}"#,
            )
            .expect("malformed authoritative item should normalize to an abort");
        assert_eq!(
            aborted_events
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ToolCallAborted { .. }))
                .count(),
            1
        );
        assert!(aborted_events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallAborted { message, .. }
                if message.contains("line 1 column")
        )));
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

    /// Verifies custom Apply Patch text streams through portable JSON and completes authoritatively.
    /// 验证自定义 Apply Patch 文本通过可移植 JSON 流式传递并按权威值完成。
    #[test]
    fn responses_custom_tool_input_streams_and_completes_as_patch_argument() {
        // applyPatchTool owns the legacy object representation explicitly instead of relying on its name.
        // applyPatchTool 显式持有既有对象表示，不依赖工具名称猜测。
        let apply_patch_tool = freeform_tool(
            "apply_patch",
            FreeformToolArgumentEncoding::ApplyPatchObject,
        );
        let mut decoder = OpenAiResponsesStreamDecoder::with_registered_tools(
            String::from("execution"),
            &[apply_patch_tool],
        );
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"custom_tool_call","id":"item","call_id":"call","name":"apply_patch","input":""}}"#,
            )
            .expect("custom tool should start");
        let first = decoder
            .push_sse(
                Some("response.custom_tool_call_input.delta"),
                r#"{"type":"response.custom_tool_call_input.delta","output_index":0,"item_id":"item","call_id":"call","delta":"*** Begin Patch\n"}"#,
            )
            .expect("custom input delta should decode");
        assert!(first.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallArgumentsDelta { delta, progress, .. }
                if delta == "*** Begin Patch\n"
                    && progress.characters_received == "*** Begin Patch\n".chars().count() as u64
        )));

        let complete_patch = "*** Begin Patch\n*** Add File: note.txt\n+done\n*** End Patch";
        let candidate = decoder
            .push_sse(
                Some("response.custom_tool_call_input.done"),
                serde_json::to_string(&json!({
                    "type": "response.custom_tool_call_input.done",
                    "output_index": 0,
                    "item_id": "item",
                    "call_id": "call",
                    "input": complete_patch,
                }))
                .expect("custom completion should serialize")
                .as_str(),
            )
            .expect("custom input completion should decode");
        assert!(candidate.is_empty());

        let completed = decoder
            .push_sse(
                Some("response.output_item.done"),
                serde_json::to_string(&json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": {
                        "type": "custom_tool_call",
                        "id": "item",
                        "call_id": "call",
                        "name": "apply_patch",
                        "input": complete_patch,
                    },
                }))
                .expect("custom output item should serialize")
                .as_str(),
            )
            .expect("custom output item should complete");
        assert!(completed.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.name == "apply_patch" && call.arguments == json!({"patch": complete_patch})
        )));
    }

    /// Verifies custom input may span many deltas while an opening snapshot still excludes them.
    /// 验证自定义输入可跨越多个增量，而开启快照仍排斥增量。
    #[test]
    fn responses_custom_tool_input_accepts_many_deltas_but_not_after_opening_snapshot() {
        let exec_tool = freeform_tool("exec", FreeformToolArgumentEncoding::RawText);
        let delta = |text: &str| {
            json!({
                "type": "response.custom_tool_call_input.delta",
                "output_index": 0,
                "item_id": "item",
                "call_id": "call",
                "delta": text,
            })
            .to_string()
        };
        let mut decoder = OpenAiResponsesStreamDecoder::with_registered_tools(
            String::from("execution"),
            std::slice::from_ref(&exec_tool),
        );
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"custom_tool_call","id":"item","call_id":"call","name":"exec","input":""}}"#,
            )
            .expect("custom tool should start");
        for fragment in ["const a = 1;\n", "const b = 2;\n", "text(a + b);"] {
            decoder
                .push_sse(
                    Some("response.custom_tool_call_input.delta"),
                    delta(fragment).as_str(),
                )
                .expect("every streamed custom input delta should decode");
        }

        let mut decoder = OpenAiResponsesStreamDecoder::with_registered_tools(
            String::from("execution"),
            &[exec_tool],
        );
        decoder
            .push_sse(
                Some("response.output_item.added"),
                r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"custom_tool_call","id":"item","call_id":"call","name":"exec","input":"text(1);"}}"#,
            )
            .expect("custom tool with an opening snapshot should start");
        decoder
            .push_sse(
                Some("response.custom_tool_call_input.delta"),
                delta("text(2);").as_str(),
            )
            .expect_err("a delta must not extend an opening input snapshot");
    }

    /// Verifies standard function deltas produce the exact Apply Patch JSON object without custom wrapping.
    /// 验证标准函数增量生成精确 Apply Patch JSON 对象，不进行 custom 封装。
    #[test]
    fn responses_apply_patch_function_stream_preserves_json_arguments() {
        // decoder consumes standard Responses function events, as returned for the new tool declaration.
        // decoder 消费新工具声明对应的标准 Responses 函数事件。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder.push_sse(Some("response.output_item.added"),
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"apply_patch","arguments":""}}"#
        ).expect("function should start");
        // arguments deliberately include characters whose escaping must survive streamed JSON decoding.
        // arguments 特意包含必须经过流式 JSON 解码保留转义的字符。
        let arguments = json!({"patch":"*** Begin Patch\n*** Add File: note.txt\n+你好 \"quoted\" C:\\workspace\n*** End Patch"});
        let argument_text = arguments.to_string();
        let deltas = decoder.push_sse(Some("response.function_call_arguments.delta"),
            &json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"item","call_id":"call","delta":argument_text}).to_string()
        ).expect("standard argument delta");
        assert!(deltas.iter().any(|event| matches!(event,
            AiStreamEvent::ToolCallArgumentsDelta { delta, .. } if delta == &argument_text
        )));
        let completed = decoder.push_sse(Some("response.output_item.done"),
            &json!({"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"apply_patch","arguments":argument_text}}).to_string()
        ).expect("standard function completion");
        assert_eq!(
            completed
                .iter()
                .filter(|event| matches!(event, AiStreamEvent::ToolCallCompleted { .. }))
                .count(),
            1
        );
        assert!(completed.iter().any(|event| matches!(event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.id == "call" && call.name == "apply_patch" && call.arguments == arguments
        )));
    }

    /// Verifies a buffered custom tool can complete without input delta events.
    /// 验证缓冲自定义工具可在没有输入增量事件时完成。
    #[test]
    fn responses_output_item_done_completes_buffered_custom_tool() {
        let patch = "*** Begin Patch\n*** Delete File: old.txt\n*** End Patch";
        // applyPatchTool freezes the exact buffered custom-call representation.
        // applyPatchTool 冻结精确的缓冲自定义调用表示。
        let apply_patch_tool = freeform_tool(
            "apply_patch",
            FreeformToolArgumentEncoding::ApplyPatchObject,
        );
        let mut decoder = OpenAiResponsesStreamDecoder::with_registered_tools(
            String::from("execution"),
            &[apply_patch_tool],
        );
        let events = decoder
            .push_sse(
                Some("response.output_item.done"),
                serde_json::to_string(&json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": {
                        "type": "custom_tool_call",
                        "id": "item",
                        "call_id": "call",
                        "name": "apply_patch",
                        "input": patch,
                    },
                }))
                .expect("buffered custom item should serialize")
                .as_str(),
            )
            .expect("buffered custom tool should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. } if tool_name == "apply_patch"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.arguments == json!({"patch": patch})
        )));
    }

    /// Verifies a non-patch custom tool retains exact text as a string argument.
    /// 验证非补丁自定义工具将精确正文保留为字符串参数。
    #[test]
    fn responses_freeform_custom_tool_preserves_raw_text_argument() {
        // execTool declares raw-string host representation independently from its tool name.
        // execTool 独立于工具名称声明原始字符串宿主表示。
        let exec_tool = freeform_tool("exec", FreeformToolArgumentEncoding::RawText);
        let mut decoder = OpenAiResponsesStreamDecoder::with_registered_tools(
            String::from("execution"),
            &[exec_tool],
        );
        let source = "const result = await tools.lookup({ id: 1 });\ntext(result);";
        let events = decoder
            .push_sse(
                Some("response.output_item.done"),
                serde_json::to_string(&json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": {
                        "type": "custom_tool_call",
                        "id": "item",
                        "call_id": "call",
                        "name": "exec",
                        "input": source,
                    },
                }))
                .expect("raw custom item should serialize")
                .as_str(),
            )
            .expect("raw custom item should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.name == "exec" && call.arguments == Value::String(source.to_string())
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

    /// Verifies Codex message phase and logical-turn completion remain distinct typed events.
    /// 验证 Codex 消息阶段与逻辑回合完成保持为两个独立的类型化事件。
    #[test]
    fn responses_preserves_message_phase_and_end_turn_false() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let message_events = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"still working"}]}}"#,
            )
            .expect("phased assistant message should decode");
        assert!(message_events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseMessagePhaseUpdated { item_id, phase }
                if item_id == "message" && *phase == AiMessagePhase::Commentary
        )));

        let terminal_events = decoder
            .push_sse(
                Some("response.completed"),
                r#"{"type":"response.completed","response":{"id":"response","end_turn":false,"usage":{}}}"#,
            )
            .expect("explicit continuation boundary should decode");
        assert!(terminal_events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTurnBoundary { end_turn: false }
        )));
        assert!(
            terminal_events
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
    }

    /// Verifies unknown Codex message phases fail closed before host projection.
    /// 验证未知 Codex 消息阶段在进入宿主投影前封闭失败。
    #[test]
    fn responses_rejects_unknown_message_phase() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let error = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"message","role":"assistant","phase":"background","content":[{"type":"output_text","text":"text"}]}}"#,
            )
            .expect_err("unknown message phase must fail closed");
        assert!(error.to_string().contains("not registered"));
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
                    "item": provider_item,
                }))
                .expect("reasoning event should serialize")
                .as_str(),
            )
            .expect("unsigned reasoning should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "inspect the workspace"
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

    /// Verifies an in-stream exhausted quota is a terminal payment failure, not an unclassified one.
    /// 验证流内额度耗尽是终止的付费失败，而不是未分类失败。
    #[test]
    fn responses_insufficient_quota_error_is_payment_required() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("error"),
                r#"{"type":"error","error":{"type":"insufficient_quota","code":"insufficient_quota","message":"quota exhausted"}}"#,
            )
            .expect("insufficient quota Responses error should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.category == AiErrorCategory::PaymentRequired && !error.retryable
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

    /// Verifies the Responses stream transport error is normalized as a retryable provider failure.
    /// 验证 Responses 流传输错误会被归一化为可重试的供应商失败。
    #[test]
    fn responses_stream_error_is_retryable() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let events = decoder
            .push_sse(
                Some("error"),
                r#"{"type":"error","code":"STREAM_ERROR","message":"Responses stream call failed"}"#,
            )
            .expect("Responses stream error should decode");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseFailed { error }
                if error.category == AiErrorCategory::ProviderUnavailable
                    && error.retryable
                    && error.upstream_code.as_deref() == Some("STREAM_ERROR")
                    && error.upstream_message.as_deref() == Some("Responses stream call failed")
        )));
        assert!(
            decoder
                .finish()
                .expect("terminal stream error should finish")
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
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "buffered reasoning"
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
        // events aggregates four deltas that intentionally reuse local index zero.
        // events 汇总四个刻意复用局部零索引的增量。
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
        events.extend(
            decoder
                .push_sse(
                    Some("response.reasoning_summary_text.delta"),
                    r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_2","output_index":1,"summary_index":0,"delta":"other summary"}"#,
                )
                .expect("second reasoning summary item should decode"),
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
            AiStreamEvent::ReasoningSummaryUpdated { item_id, text }
                if item_id == "responses:execution:reasoning:2" && text == "summary"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningSummaryUpdated { item_id, text }
                if item_id == "responses:execution:reasoning:3" && text == "other summary"
        )));
    }

    /// Verifies encrypted Responses reasoning stays private while remaining replayable.
    /// 验证 Responses 加密推理保持私有，同时仍可重放。
    #[test]
    fn responses_preserves_encrypted_reasoning_for_private_replay() {
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        decoder
            .push_sse(
                Some("response.reasoning_summary_text.delta"),
                r#"{"type":"response.reasoning_summary_text.delta","summary_index":0,"delta":"summary"}"#,
            )
            .expect("reasoning summary should decode");
        let captured = decoder
            .push_sse(
                Some("response.output_item.done"),
                r#"{"type":"response.output_item.done","item":{"type":"reasoning","encrypted_content":"encrypted-replay"}}"#,
            )
            .expect("reasoning signature should decode");
        assert!(captured.iter().any(|event| matches!(
            event,
            AiStreamEvent::ProviderContextItemCaptured { item }
                if item.get("encrypted_content").and_then(Value::as_str) == Some("encrypted-replay")
        )));
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
            }
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
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "summary"
        )));
        assert!(buffered.iter().any(|event| matches!(
            event,
            AiStreamEvent::ReasoningDelta { item_id, delta }
                if item_id == "responses:execution:reasoning:1" && delta == "raw"
        )));
        // captured_context is the exact private replay item; public reasoning remains separately normalized.
        // captured_context 是精确的私有重放项目；公共推理仍单独规范化。
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
        assert_eq!(
            captured_context
                .and_then(|item| item.get("encrypted_content"))
                .and_then(Value::as_str),
            Some("opaque-buffered")
        );
        assert_eq!(
            captured_context
                .and_then(|item| item.pointer("/summary/0/text"))
                .and_then(Value::as_str),
            Some("summary")
        );

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
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "part"
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
            AiStreamEvent::ReasoningSummaryUpdated { text, .. } if text == "partial"
        )));
    }

    /// Verifies encrypted reasoning from multiple upstream items keeps exact private identity.
    /// 验证多个上游项目的加密推理保持精确私有身份。
    #[test]
    fn responses_encrypted_reasoning_items_preserve_private_replay_order() {
        // decoder owns two upstream reasoning items whose local indexes intentionally collide.
        // decoder 持有两个局部索引刻意冲突的上游推理项目。
        let mut decoder = OpenAiResponsesStreamDecoder::new(String::from("execution"));
        let mut captured_items = Vec::new();
        for (item_id, output_index, delta, signature) in [
            ("rs_one", 0, "one", "opaque-one"),
            ("rs_two", 1, "two", "opaque-two"),
        ] {
            captured_items.extend(
                decoder
                .push_sse(
                    Some("response.reasoning_summary_text.delta"),
                    format!(
                        r#"{{"type":"response.reasoning_summary_text.delta","item_id":"{item_id}","output_index":{output_index},"summary_index":0,"delta":"{delta}"}}"#
                    )
                    .as_str(),
                )
                .expect("scoped reasoning delta should decode"),
            );
            captured_items.extend(
                decoder
                    .push_sse(
                        Some("response.output_item.done"),
                        format!(
                            r#"{{"type":"response.output_item.done","output_index":{output_index},"item":{{"id":"{item_id}","type":"reasoning","summary":[{{"type":"summary_text","text":"{delta}"}}],"encrypted_content":"{signature}"}}}}"#
                        )
                        .as_str(),
                    )
                    .expect("scoped reasoning completion should decode"),
            );
        }
        let private_items = captured_items
            .iter()
            .filter_map(|event| match event {
                AiStreamEvent::ProviderContextItemCaptured { item } => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(private_items.len(), 2);
        assert_eq!(
            private_items[0].get("id").and_then(Value::as_str),
            Some("rs_one")
        );
        assert_eq!(
            private_items[1].get("id").and_then(Value::as_str),
            Some("rs_two")
        );
        assert_eq!(
            private_items[0]
                .get("encrypted_content")
                .and_then(Value::as_str),
            Some("opaque-one")
        );
        // completedByItem proves private payloads never become visible reasoning completions.
        // completedByItem 证明私有载荷绝不会成为可见推理完成事件。
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
