use super::*;

pub(super) fn process_compatible_tool_call_chunks(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
    chunks: Vec<CompatibleToolCallChunk>,
) {
    for chunk in chunks {
        if let Some(existing) = pending_tool_calls.get(&chunk.index)
            && should_evict_distinct_named_tool_call(existing, &chunk)
            && let Some(evicted) = pending_tool_calls.remove(&chunk.index)
            && let Some(finalized) = finalize_pending_tool_call(evicted)
        {
            finalized_tool_calls.push(compatible_tool_call_to_core_tool_call(finalized));
        }

        let pending = pending_tool_calls
            .entry(chunk.index)
            .or_insert_with(CompatibleAccumulatedToolCall::empty);

        if let Some(id) = chunk.id.as_ref()
            && !id.is_empty()
        {
            pending.id = id.clone();
        }

        if let Some(name) = chunk.name.as_ref()
            && !name.is_empty()
        {
            pending.name = name.clone();
        }

        if let Some(arguments) = chunk.arguments.as_ref()
            && !arguments.is_empty()
        {
            append_tool_call_arguments(pending, arguments);
        }
    }
}

/// Flushes every pending OpenAI-compatible tool call into finalized canonical tool-call entries.
/// 将所有待定 OpenAI 兼容工具调用冲刷为已完成的规范工具调用项。
pub(super) fn flush_pending_compatible_tool_calls(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
) {
    let mut indices = pending_tool_calls.keys().copied().collect::<Vec<_>>();
    indices.sort_unstable();

    for index in indices {
        let Some(pending) = pending_tool_calls.remove(&index) else {
            continue;
        };
        let finalized = finalize_pending_tool_call(finalize_completed_streaming_tool_call(pending));
        if let Some(tool_call) = finalized {
            finalized_tool_calls.push(compatible_tool_call_to_core_tool_call(tool_call));
        }
    }
}

/// Converts one accumulated compatible tool call into the canonical `vulcan-core` tool-call shape.
/// 将一条累计完成的兼容工具调用转换为规范 `vulcan-core` 工具调用形状。
pub(super) fn compatible_tool_call_to_core_tool_call(
    tool_call: CompatibleAccumulatedToolCall,
) -> CoreConversationToolCall {
    let tool_call_id = (!tool_call.id.is_empty()).then_some(tool_call.id.clone());
    CoreConversationToolCall {
        id: tool_call.id,
        tool_call_id,
        tool_name: tool_call.name,
        arguments: tool_call.arguments,
    }
}
