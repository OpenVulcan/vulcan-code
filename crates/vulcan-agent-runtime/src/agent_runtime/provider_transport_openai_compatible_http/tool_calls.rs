use super::*;

/// Applies ordered OpenAI-compatible tool-call chunks and emits request-stream changes as soon as an exact public name resolves.
/// 应用有序 OpenAI 兼容工具调用碎片，并在精确公开名称解析后立即发出请求流变化。
///
/// # Parameters
/// 参数
/// `pending_tool_calls` owns protocol-level call assembly by provider slot.
/// `pending_tool_calls` 按 Provider 槽位持有协议级调用组装状态。
/// `finalized_tool_calls` collects calls displaced by a distinct later call in the same slot.
/// `finalized_tool_calls` 收集同槽位中被后续不同调用替换的调用。
/// `active_tool_request_streams` correlates visible requests to provider slots.
/// `active_tool_request_streams` 将可见请求关联到 Provider 槽位。
/// `provider_tool_definitions` is the frozen exact public tool catalog.
/// `provider_tool_definitions` 是冻结的精确公开工具目录。
/// `chunks` contains the next ordered protocol fragments.
/// `chunks` 包含下一批有序协议碎片。
///
/// # Returns
/// 返回值
/// Ordered normalized request fragments that are safe for the runtime coordinator.
/// 返回可安全交给运行时协调器的有序规范化请求碎片。
pub(super) fn process_compatible_tool_call_chunks(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
    active_tool_request_streams: &mut HashMap<usize, String>,
    provider_tool_definitions: &[ProviderToolDefinition],
    chunks: Vec<CompatibleToolCallChunk>,
) -> Vec<ToolRequestFragment> {
    // fragments contains only normalized request changes safe for the runtime coordinator.
    // fragments 仅包含可安全交给运行时协调器的规范化请求变化。
    let mut fragments = Vec::new();
    for chunk in chunks {
        if let Some(existing) = pending_tool_calls.get(&chunk.index)
            && should_evict_distinct_named_tool_call(existing, &chunk)
        {
            let request_stream_id = active_tool_request_streams.remove(&chunk.index);
            let finalized = pending_tool_calls
                .remove(&chunk.index)
                .and_then(finalize_pending_tool_call)
                .map(compatible_tool_call_to_core_tool_call);
            if let Some(request_stream_id) = request_stream_id {
                let canonical_tool_call_id = finalized
                    .as_ref()
                    .and_then(|tool_call| tool_call.tool_call_id.clone())
                    .filter(|tool_call_id| tool_call_id != &request_stream_id);
                fragments.push(ToolRequestFragment::Completed {
                    request_stream_id,
                    canonical_tool_call_id,
                    final_arguments: None,
                });
            }
            if let Some(finalized) = finalized {
                finalized_tool_calls.push(finalized);
            }
        }

        // streamedArguments preserves the exact incoming fragment for progress accounting.
        // streamedArguments 为进度统计保留精确的输入碎片。
        let streamed_arguments = chunk.arguments.clone().filter(|value| !value.is_empty());
        // requestWasActive identifies whether this fragment belongs to an already visible request.
        // requestWasActive 标识当前碎片是否属于已可见请求。
        let request_was_active = active_tool_request_streams.get(&chunk.index).cloned();
        let pending = pending_tool_calls
            .entry(chunk.index)
            .or_insert_with(CompatibleAccumulatedToolCall::empty);

        if let Some(id) = chunk.id.as_ref()
            && !id.is_empty()
        {
            pending.id = id.clone();
        }

        if let Some(name_fragment) = chunk.name.as_ref()
            && !name_fragment.is_empty()
        {
            pending.name = merge_compatible_tool_name_fragment(
                &pending.name,
                name_fragment,
                provider_tool_definitions,
            );
        }

        if let Some(arguments) = chunk.arguments.as_ref()
            && !arguments.is_empty()
        {
            append_tool_call_arguments(pending, arguments);
        }
        if let (Some(request_stream_id), Some(delta)) =
            (request_was_active, streamed_arguments.clone())
        {
            fragments.push(ToolRequestFragment::ArgumentsDelta {
                request_stream_id,
                delta,
            });
            continue;
        }
        if active_tool_request_streams.contains_key(&chunk.index)
            || !provider_tool_definitions
                .iter()
                .any(|definition| definition.name == pending.name)
        {
            continue;
        }
        if pending.id.trim().is_empty() {
            pending.id = format!("call_{}", Uuid::new_v4().simple());
        }
        // requestStreamId uses the same stable id that the final core tool call will carry.
        // requestStreamId 使用最终 Core 工具调用将携带的同一稳定标识。
        let request_stream_id = pending.id.clone();
        // initialArguments includes every argument fragment accumulated before the name resolved.
        // initialArguments 包含工具名称解析前已累计的全部参数碎片。
        let argument_assembler = pending.arguments.clone();
        let initial_arguments = argument_assembler.snapshot();
        active_tool_request_streams.insert(chunk.index, request_stream_id.clone());
        fragments.push(ToolRequestFragment::Detected {
            request_stream_id,
            tool_call_id: pending.id.clone(),
            tool_name: pending.name.clone(),
            initial_arguments,
            argument_assembler: Some(argument_assembler),
        });
    }
    fragments
}

/// Merges one streamed function-name fragment only when the frozen public catalog proves the concatenation.
/// 仅当冻结公开目录能证明拼接关系时，合并一段流式函数名称碎片。
///
/// # Parameters
/// 参数
/// `current_name` is the name text accumulated for the current provider slot.
/// `current_name` 是当前 Provider 槽位已累计的名称文本。
/// `incoming_fragment` is the exact non-empty name fragment from the next chunk.
/// `incoming_fragment` 是下一块携带的精确非空名称碎片。
/// `provider_tool_definitions` is the frozen catalog that establishes valid prefixes and exact names.
/// `provider_tool_definitions` 是用于确定有效前缀与精确名称的冻结目录。
///
/// # Returns
/// 返回值
/// The deterministic accumulated name without fuzzy matching or alias guessing.
/// 返回不进行模糊匹配或别名猜测的确定性累计名称。
fn merge_compatible_tool_name_fragment(
    current_name: &str,
    incoming_fragment: &str,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> String {
    if current_name.is_empty() || current_name == incoming_fragment {
        return incoming_fragment.to_string();
    }
    if provider_tool_definitions
        .iter()
        .any(|definition| definition.name == incoming_fragment)
    {
        return incoming_fragment.to_string();
    }
    let joined = format!("{current_name}{incoming_fragment}");
    if provider_tool_definitions
        .iter()
        .any(|definition| definition.name.starts_with(&joined))
    {
        return joined;
    }
    incoming_fragment.to_string()
}

/// Flushes every pending OpenAI-compatible tool call into finalized canonical tool-call entries.
/// 将所有待定 OpenAI 兼容工具调用冲刷为已完成的规范工具调用项。
pub(super) fn flush_pending_compatible_tool_calls(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
    active_tool_request_streams: &mut HashMap<usize, String>,
) -> Vec<ToolRequestFragment> {
    // fragments closes visible requests in the same stable slot order as canonical calls.
    // fragments 按规范调用相同的稳定槽位顺序关闭可见请求。
    let mut fragments = Vec::new();
    let mut indices = pending_tool_calls.keys().copied().collect::<Vec<_>>();
    indices.sort_unstable();

    for index in indices {
        let request_stream_id = active_tool_request_streams.remove(&index);
        let Some(pending) = pending_tool_calls.remove(&index) else {
            continue;
        };
        let finalized = finalize_pending_tool_call(finalize_completed_streaming_tool_call(pending))
            .map(compatible_tool_call_to_core_tool_call);
        if let Some(request_stream_id) = request_stream_id {
            let canonical_tool_call_id = finalized
                .as_ref()
                .and_then(|tool_call| tool_call.tool_call_id.clone())
                .filter(|tool_call_id| tool_call_id != &request_stream_id);
            fragments.push(ToolRequestFragment::Completed {
                request_stream_id,
                canonical_tool_call_id,
                final_arguments: None,
            });
        }
        if let Some(finalized) = finalized {
            finalized_tool_calls.push(finalized);
        }
    }
    fragments
}

/// Converts one accumulated compatible tool call into the canonical `vulcan-core` tool-call shape.
/// 将一条累计完成的兼容工具调用转换为规范 `vulcan-core` 工具调用形状。
pub(super) fn compatible_tool_call_to_core_tool_call(
    tool_call: CompatibleAccumulatedToolCall,
) -> CoreConversationToolCall {
    let tool_call_id = (!tool_call.id.is_empty()).then_some(tool_call.id.clone());
    let arguments = tool_call
        .arguments
        .read(|arguments| serde_json::from_str(arguments))
        .expect("finalized compatible tool arguments must remain valid JSON");
    CoreConversationToolCall {
        id: tool_call.id,
        tool_call_id,
        tool_name: tool_call.name,
        arguments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::tool_request_argument_assembler::ToolCallDraftAssembler;
    use serde_json::json;

    /// Builds one exact provider tool definition for focused streaming-name tests.
    /// 为聚焦的流式名称测试构建一条精确 Provider 工具定义。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the exact public function name exposed to the provider.
    /// `name` 是暴露给 Provider 的精确公开函数名称。
    ///
    /// # Returns
    /// 返回值
    /// A minimal valid provider definition.
    /// 返回一条最小有效 Provider 定义。
    fn tool_definition(name: &str) -> ProviderToolDefinition {
        ProviderToolDefinition {
            name: name.to_string(),
            description: String::from("Test tool"),
            parameters: json!({"type": "object"}),
        }
    }

    /// Verifies a tool card starts only after multiple name fragments form one exact catalog entry.
    /// 验证只有多段名称碎片组成精确目录项后才启动工具卡片。
    #[test]
    fn split_tool_name_starts_request_after_exact_resolution() {
        let definitions = vec![tool_definition("write_file")];
        let mut pending = HashMap::new();
        let mut finalized = Vec::new();
        let mut active = HashMap::new();

        let first = process_compatible_tool_call_chunks(
            &mut pending,
            &mut finalized,
            &mut active,
            &definitions,
            vec![CompatibleToolCallChunk {
                index: 0,
                id: Some(String::from("call-1")),
                name: Some(String::from("write_")),
                arguments: Some(String::from("{\"path\":")),
            }],
        );
        assert!(first.is_empty());

        let second = process_compatible_tool_call_chunks(
            &mut pending,
            &mut finalized,
            &mut active,
            &definitions,
            vec![CompatibleToolCallChunk {
                index: 0,
                id: None,
                name: Some(String::from("file")),
                arguments: Some(String::from("\"a.txt\"}")),
            }],
        );

        assert_eq!(
            second,
            vec![ToolRequestFragment::Detected {
                request_stream_id: String::from("call-1"),
                tool_call_id: String::from("call-1"),
                tool_name: String::from("write_file"),
                initial_arguments: String::from("{\"path\":\"a.txt\"}"),
                argument_assembler: Some(ToolCallDraftAssembler::new(String::from(
                    "{\"path\":\"a.txt\"}",
                ))),
            }]
        );
    }

    /// Verifies an active compatible request emits later argument deltas and one matching completion boundary.
    /// 验证活动的兼容请求会发出后续参数增量以及一个匹配的完成边界。
    #[test]
    fn active_tool_request_emits_delta_then_single_completion() {
        // maps preserve exact slot ownership from provider chunks through canonical call finalization.
        // 映射会从 Provider 碎片到规范调用最终化全程保留精确槽位所有权。
        let definitions = vec![tool_definition("submit_plan")];
        let mut pending = HashMap::new();
        let mut finalized = Vec::new();
        let mut active = HashMap::new();
        let detected = process_compatible_tool_call_chunks(
            &mut pending,
            &mut finalized,
            &mut active,
            &definitions,
            vec![CompatibleToolCallChunk {
                index: 4,
                id: Some(String::from("compatible-call-4")),
                name: Some(String::from("submit_plan")),
                arguments: Some(String::from("{\"name\":")),
            }],
        );
        assert!(matches!(
            detected.as_slice(),
            [ToolRequestFragment::Detected { request_stream_id, .. }]
                if request_stream_id == "compatible-call-4"
        ));
        let delta = process_compatible_tool_call_chunks(
            &mut pending,
            &mut finalized,
            &mut active,
            &definitions,
            vec![CompatibleToolCallChunk {
                index: 4,
                id: None,
                name: None,
                arguments: Some(String::from("\"demo\"}")),
            }],
        );
        assert_eq!(
            delta,
            vec![ToolRequestFragment::ArgumentsDelta {
                request_stream_id: String::from("compatible-call-4"),
                delta: String::from("\"demo\"}"),
            }]
        );
        let completed =
            flush_pending_compatible_tool_calls(&mut pending, &mut finalized, &mut active);
        assert_eq!(
            completed,
            vec![ToolRequestFragment::Completed {
                request_stream_id: String::from("compatible-call-4"),
                canonical_tool_call_id: None,
                final_arguments: None,
            }]
        );
        assert_eq!(finalized.len(), 1);
        assert_eq!(
            finalized[0].tool_call_id.as_deref(),
            Some("compatible-call-4")
        );
        assert_eq!(finalized[0].arguments, json!({ "name": "demo" }));
        assert!(pending.is_empty());
        assert!(active.is_empty());
    }

    /// Verifies a provider id arriving after detection is promoted at the completion boundary.
    /// 验证在检测后才到达的 Provider 标识会在完成边界晋升为规范标识。
    #[test]
    fn late_provider_call_id_is_promoted_on_completion() {
        // maps model a provider that resolves the public name before emitting its canonical call id.
        // 映射模拟先解析公开工具名、后发出规范调用标识的 Provider。
        let definitions = vec![tool_definition("submit_plan")];
        let mut pending = HashMap::new();
        let mut finalized = Vec::new();
        let mut active = HashMap::new();
        let detected = process_compatible_tool_call_chunks(
            &mut pending,
            &mut finalized,
            &mut active,
            &definitions,
            vec![CompatibleToolCallChunk {
                index: 7,
                id: None,
                name: Some(String::from("submit_plan")),
                arguments: Some(String::from("{\"name\":")),
            }],
        );
        let temporary_request_stream_id = match detected.as_slice() {
            [
                ToolRequestFragment::Detected {
                    request_stream_id, ..
                },
            ] => request_stream_id.clone(),
            fragments => panic!("expected one detected fragment, got {fragments:?}"),
        };
        process_compatible_tool_call_chunks(
            &mut pending,
            &mut finalized,
            &mut active,
            &definitions,
            vec![CompatibleToolCallChunk {
                index: 7,
                id: Some(String::from("provider-call-7")),
                name: None,
                arguments: Some(String::from("\"demo\"}")),
            }],
        );

        let completed =
            flush_pending_compatible_tool_calls(&mut pending, &mut finalized, &mut active);

        assert_eq!(
            completed,
            vec![ToolRequestFragment::Completed {
                request_stream_id: temporary_request_stream_id,
                canonical_tool_call_id: Some(String::from("provider-call-7")),
                final_arguments: None,
            }]
        );
        assert_eq!(
            finalized
                .first()
                .and_then(|tool_call| tool_call.tool_call_id.as_deref()),
            Some("provider-call-7")
        );
    }
}
