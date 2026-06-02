//! Host-owned OpenAI-compatible streaming protocol primitives.
//! 宿主自有的 OpenAI 兼容流式协议基础原语。

use serde_json::{Map as JsonMap, Value};
use vulcan_core::types::ProviderUsageSnapshot as CoreProviderUsageSnapshot;

/// Finish reasons normalized for OpenAI Chat Completions-compatible provider streams.
/// 为 OpenAI Chat Completions 兼容 Provider 流规范化的结束原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibleFinishReason {
    /// The provider indicates that tool calls are complete and should be flushed immediately.
    /// Provider 表示工具调用已经结束并应立即冲刷输出。
    ToolCalls,
    /// Any non-tool-calls finish reason that does not need special stream cleanup semantics.
    /// 任何不需要特殊流式清理语义的非工具调用结束原因。
    Other,
}

/// One streamed tool-call fragment normalized from an OpenAI-compatible delta payload.
/// 从 OpenAI 兼容增量载荷中规范化得到的一段工具调用碎片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompatibleToolCallChunk {
    /// Stable positional index used to merge multiple deltas that belong to the same logical tool call.
    /// 用于合并同一逻辑工具调用多段增量的稳定位置索引。
    pub(crate) index: usize,
    /// Optional upstream tool-call identifier carried by the provider delta.
    /// Provider 增量携带的可选上游工具调用标识。
    pub(crate) id: Option<String>,
    /// Optional tool name fragment carried by the provider delta.
    /// Provider 增量携带的可选工具名称碎片。
    pub(crate) name: Option<String>,
    /// Optional tool-arguments fragment carried by the provider delta.
    /// Provider 增量携带的可选工具参数碎片。
    pub(crate) arguments: Option<String>,
}

impl CompatibleToolCallChunk {
    /// Returns whether the current chunk carries one non-empty tool name fragment.
    /// 返回当前碎片是否携带非空工具名称片段。
    fn has_nonempty_name(&self) -> bool {
        self.name.as_ref().is_some_and(|name| !name.is_empty())
    }

    /// Returns whether the current chunk semantically starts one new tool call.
    /// 返回当前碎片在语义上是否开启了一个新的工具调用。
    fn starts_new_tool_call(&self) -> bool {
        self.has_nonempty_name()
            && self
                .arguments
                .as_ref()
                .map(|arguments| arguments.is_empty())
                .unwrap_or(true)
    }
}

/// One normalized choice payload extracted from the first compatible provider choice.
/// 从首个兼容 Provider choice 提取出的规范化 choice 载荷。
#[derive(Debug, Clone)]
pub(crate) struct CompatibleChoice {
    /// Stream finish reason normalized to the compatibility layer.
    /// 已规范化到兼容层的流式结束原因。
    pub(crate) finish_reason: CompatibleFinishReason,
    /// Optional assistant text delta emitted by the provider.
    /// Provider 发出的可选助手文本增量。
    pub(crate) text: Option<String>,
    /// Optional assistant reasoning delta emitted by the provider.
    /// Provider 发出的可选助手推理增量。
    pub(crate) reasoning: Option<String>,
    /// Additional provider-visible assistant fields that must survive raw interleaved replay.
    /// 必须在原生交错回放中保留的额外 Provider 可见助手字段。
    pub(crate) provider_visible_assistant_fields: JsonMap<String, Value>,
    /// Streamed tool-call fragments emitted by the provider.
    /// Provider 发出的工具调用碎片集合。
    pub(crate) tool_calls: Vec<CompatibleToolCallChunk>,
}

/// One intermediate choice representation used before tool-call chunks are converted into the canonical compatible shape.
/// 在工具调用碎片转换成标准兼容形态之前使用的中间 choice 表示。
#[derive(Debug, Clone)]
pub(crate) struct CompatibleChoiceData<T> {
    /// Stream finish reason normalized to the compatibility layer.
    /// 已规范化到兼容层的流式结束原因。
    pub(crate) finish_reason: CompatibleFinishReason,
    /// Optional assistant text delta emitted by the provider.
    /// Provider 发出的可选助手文本增量。
    pub(crate) text: Option<String>,
    /// Optional assistant reasoning delta emitted by the provider.
    /// Provider 发出的可选助手推理增量。
    pub(crate) reasoning: Option<String>,
    /// Additional provider-visible assistant fields that must survive raw interleaved replay.
    /// 必须在原生交错回放中保留的额外 Provider 可见助手字段。
    pub(crate) provider_visible_assistant_fields: JsonMap<String, Value>,
    /// Provider-specific tool-call fragments that can be converted into the canonical compatible tool-call chunk.
    /// 可转换为标准兼容工具调用碎片的 Provider 专有工具调用片段。
    pub(crate) tool_calls: Vec<T>,
}

/// One normalized stream chunk extracted from a provider event.
/// 从一条 Provider 事件中提取出的规范化流式块。
#[derive(Debug, Clone)]
pub(crate) struct CompatibleChunk {
    /// Optional upstream response identifier carried by the current event.
    /// 当前事件携带的可选上游响应标识。
    pub(crate) response_id: Option<String>,
    /// Optional normalized provider usage snapshot carried by the current event.
    /// 当前事件携带的可选规范化 Provider 用量快照。
    pub(crate) usage: Option<CoreProviderUsageSnapshot>,
    /// Optional first normalized choice extracted from the current event.
    /// 从当前事件提取出的可选首个规范化 choice。
    pub(crate) choice: Option<CompatibleChoice>,
}

/// One accumulated tool call assembled from multiple streamed delta fragments.
/// 由多段流式增量碎片组装而成的一次累计工具调用。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompatibleAccumulatedToolCall {
    /// Stable upstream tool-call identifier, if already known.
    /// 若已知则保存稳定的上游工具调用标识。
    pub(crate) id: String,
    /// Stable resolved tool name, if already known.
    /// 若已知则保存稳定的已解析工具名称。
    pub(crate) name: String,
    /// Incrementally merged tool argument payload.
    /// 增量合并后的工具参数载荷。
    pub(crate) arguments: Value,
}

impl CompatibleAccumulatedToolCall {
    /// Creates one empty accumulated tool call that can be filled by later stream fragments.
    /// 创建一个可被后续流式碎片填充的空累计工具调用。
    pub(crate) fn empty() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            arguments: Value::Null,
        }
    }
}

impl<T> From<CompatibleChoiceData<T>> for CompatibleChoice
where
    T: Into<CompatibleToolCallChunk>,
{
    /// Converts one provider-specific intermediate choice into the canonical compatible choice shape.
    /// 将一份 Provider 专用中间 choice 转换为标准兼容 choice 形状。
    fn from(value: CompatibleChoiceData<T>) -> Self {
        Self {
            finish_reason: value.finish_reason,
            text: value.text,
            reasoning: value.reasoning,
            provider_visible_assistant_fields: value.provider_visible_assistant_fields,
            tool_calls: value.tool_calls.into_iter().map(Into::into).collect(),
        }
    }
}

/// Normalizes the first provider choice into the shared compatible chunk shape while preserving response metadata and usage.
/// 在保留响应元数据与用量的同时，将首个 Provider choice 规范化为共享兼容块形状。
pub(crate) fn normalize_first_choice_chunk<Choice, ToolCall, F>(
    response_id: Option<String>,
    usage: Option<CoreProviderUsageSnapshot>,
    choices: &[Choice],
    map_choice: F,
) -> CompatibleChunk
where
    ToolCall: Into<CompatibleToolCallChunk>,
    F: FnOnce(&Choice) -> CompatibleChoiceData<ToolCall>,
{
    let choice = choices.first().map(|choice| map_choice(choice).into());

    CompatibleChunk {
        response_id,
        usage,
        choice,
    }
}

/// Converts one provider-specific tool-call list into canonical compatible tool-call chunks.
/// 将一组 Provider 专用工具调用列表转换为标准兼容工具调用碎片。
pub(crate) fn tool_call_chunks<T>(tool_calls: &[T]) -> Vec<CompatibleToolCallChunk>
where
    for<'a> CompatibleToolCallChunk: From<&'a T>,
{
    tool_calls
        .iter()
        .map(CompatibleToolCallChunk::from)
        .collect()
}

/// Returns whether one incoming tool-call fragment should evict the existing accumulated entry because it starts a distinct named call.
/// 返回当前传入的工具调用碎片是否应淘汰现有累计条目，因为它开启了一个不同的具名调用。
pub(crate) fn should_evict_distinct_named_tool_call(
    existing: &CompatibleAccumulatedToolCall,
    incoming: &CompatibleToolCallChunk,
) -> bool {
    if let Some(new_id) = &incoming.id
        && !new_id.is_empty()
        && let Some(new_name) = &incoming.name
        && incoming.has_nonempty_name()
        && !existing.id.is_empty()
        && existing.id != *new_id
        && !existing.name.is_empty()
    {
        return existing.name != *new_name || incoming.starts_new_tool_call();
    }

    false
}

/// Appends one streamed argument fragment into the accumulated tool call while preserving partially assembled JSON payloads.
/// 将一段流式参数碎片追加到累计工具调用中，同时保留尚未闭合的 JSON 载荷。
pub(crate) fn append_tool_call_arguments(
    tool_call: &mut CompatibleAccumulatedToolCall,
    chunk: &str,
) {
    let current_args = match &tool_call.arguments {
        Value::Null => String::new(),
        Value::String(existing) => {
            if existing.trim() == "null" && !chunk.trim().is_empty() {
                String::new()
            } else {
                existing.clone()
            }
        }
        value => value.to_string(),
    };

    let combined = format!("{current_args}{chunk}");

    if combined.trim_start().starts_with('{') && combined.trim_end().ends_with('}') {
        match serde_json::from_str(&combined) {
            Ok(parsed) => tool_call.arguments = parsed,
            Err(_) => tool_call.arguments = Value::String(combined),
        }
    } else {
        tool_call.arguments = Value::String(combined);
    }
}

/// Finalizes one already completed accumulated tool call by normalizing missing arguments into the canonical empty-object payload.
/// 通过把缺失参数规范化为空对象载荷，完成一次已完成累计工具调用的收尾。
pub(crate) fn finalize_completed_streaming_tool_call(
    mut tool_call: CompatibleAccumulatedToolCall,
) -> CompatibleAccumulatedToolCall {
    if tool_call.arguments.is_null() {
        tool_call.arguments = Value::Object(JsonMap::new());
    }

    tool_call
}

/// Finalizes one pending accumulated tool call and drops nameless or syntactically incomplete entries.
/// 完成一次待定累计工具调用的收尾，并丢弃无名称或语法不完整的条目。
pub(crate) fn finalize_pending_tool_call(
    mut tool_call: CompatibleAccumulatedToolCall,
) -> Option<CompatibleAccumulatedToolCall> {
    if tool_call.name.is_empty() {
        return None;
    }

    match &tool_call.arguments {
        Value::Null => Some(finalize_completed_streaming_tool_call(tool_call)),
        Value::String(arguments) => {
            if arguments.trim().is_empty() {
                tool_call.arguments = Value::Object(JsonMap::new());
                return Some(tool_call);
            }

            let parsed = serde_json::from_str(arguments).ok()?;
            tool_call.arguments = parsed;
            Some(tool_call)
        }
        _ => Some(tool_call),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompatibleAccumulatedToolCall, CompatibleToolCallChunk, append_tool_call_arguments,
        finalize_pending_tool_call, should_evict_distinct_named_tool_call,
    };
    use serde_json::json;

    /// Verifies that one parameterless pending tool call is normalized into an empty JSON object instead of being dropped.
    /// 验证无参数的待定工具调用会被规范化为空 JSON 对象，而不是被丢弃。
    #[test]
    fn pending_parameterless_tool_call_is_preserved() {
        let tool_call = CompatibleAccumulatedToolCall {
            id: String::from("call_123"),
            name: String::from("ping"),
            arguments: serde_json::Value::Null,
        };

        let finalized =
            finalize_pending_tool_call(tool_call).expect("tool call should be preserved");

        assert_eq!(finalized.id, "call_123");
        assert_eq!(finalized.name, "ping");
        assert_eq!(finalized.arguments, json!({}));
    }

    /// Verifies that incomplete JSON fragments are intentionally dropped so host-owned execution does not run syntactically broken tool calls.
    /// 验证语法不完整的 JSON 碎片会被刻意丢弃，以避免宿主执行语法损坏的工具调用。
    #[test]
    fn partial_argument_payload_is_dropped() {
        let tool_call = CompatibleAccumulatedToolCall {
            id: String::from("call_123"),
            name: String::from("ping"),
            arguments: serde_json::Value::String(String::from("{\"x\":")),
        };

        assert!(finalize_pending_tool_call(tool_call).is_none());
    }

    /// Verifies that literal `null` placeholders are treated as empty state once a real JSON fragment starts streaming afterwards.
    /// 验证一旦后续开始流出真实 JSON 片段，字面量 `null` 占位符会被当作空状态处理。
    #[test]
    fn null_placeholder_is_replaced_by_following_json_fragments() {
        let mut tool_call = CompatibleAccumulatedToolCall {
            id: String::from("call_123"),
            name: String::from("web_search"),
            arguments: serde_json::Value::String(String::from("null")),
        };

        append_tool_call_arguments(&mut tool_call, "{\"query\": \"META");
        append_tool_call_arguments(&mut tool_call, " Platforms news\"}");

        let finalized =
            finalize_pending_tool_call(tool_call).expect("tool call should be preserved");

        assert_eq!(finalized.arguments, json!({"query": "META Platforms news"}));
    }

    /// Verifies that a new named tool-call fragment with a different upstream identifier evicts the existing accumulated entry.
    /// 验证当新的具名工具调用碎片携带不同上游标识时，会淘汰现有累计条目。
    #[test]
    fn distinct_named_tool_calls_are_evicted_by_identifier() {
        let existing = CompatibleAccumulatedToolCall {
            id: String::from("call_aaa"),
            name: String::from("search"),
            arguments: json!({"query": "one"}),
        };
        let incoming = CompatibleToolCallChunk {
            index: 0,
            id: Some(String::from("call_bbb")),
            name: Some(String::from("search")),
            arguments: Some(String::new()),
        };

        assert!(should_evict_distinct_named_tool_call(&existing, &incoming));
    }
}
