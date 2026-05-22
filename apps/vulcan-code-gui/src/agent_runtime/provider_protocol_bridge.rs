//! Shared provider-response bridge helpers reused by the executor, transport adapters, and regression tests.
//! 供执行器、传输适配器与回归测试复用的共享 Provider 响应桥接辅助逻辑。

use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use serde_json::{Map as JsonMap, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{
    ProviderContinuation as CoreProviderContinuation, ProviderTurnDelta as CoreProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Builds one synthetic host-owned tool-result identifier that stays stable within the current execution turn.
/// 构建一个在当前执行回合内保持稳定的宿主自有合成工具结果标识。
pub(crate) fn build_host_tool_result_id(tool_call_id: &str) -> String {
    format!("tool-result-{tool_call_id}")
}

/// Scopes one persisted continuation cursor to the currently active provider/model pair so cross-protocol session switches do not reuse a foreign upstream cursor.
/// 将一条已持久化 continuation 游标限制到当前激活的 provider/model 组合，避免跨协议切换时误复用外部上游游标。
pub(crate) fn scope_continuation_to_active_provider(
    continuation: &CoreProviderContinuation,
    provider_id: &str,
    model_id: &str,
) -> CoreProviderContinuation {
    let Some(saved_provider_id) = continuation.provider_id.as_deref() else {
        return CoreProviderContinuation::default();
    };
    if saved_provider_id != provider_id {
        return CoreProviderContinuation::default();
    }
    if continuation
        .model_id
        .as_deref()
        .is_some_and(|saved_model_id| saved_model_id != model_id)
    {
        return CoreProviderContinuation::default();
    }

    CoreProviderContinuation {
        provider_id: Some(provider_id.to_string()),
        model_id: Some(model_id.to_string()),
        last_provider_message_id: continuation
            .last_provider_message_id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Injects `previous_response_id` into one additional-params payload when the current continuation snapshot already carries a usable upstream response identifier.
/// 当当前 continuation 快照已经携带可用的上游响应标识时，将 `previous_response_id` 注入到附加参数载荷中。
pub(crate) fn inject_previous_response_id_into_additional_params(
    additional_params: Option<Value>,
    continuation: &CoreProviderContinuation,
) -> Option<Value> {
    let Some(previous_response_id) = continuation
        .last_provider_message_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
    else {
        return additional_params;
    };

    let mut object = match additional_params {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = JsonMap::new();
            object.insert(String::from("host_additional_params"), other);
            object
        }
        None => JsonMap::new(),
    };

    object
        .entry(String::from("previous_response_id"))
        .or_insert_with(|| Value::String(previous_response_id));
    Some(Value::Object(object))
}

/// Attaches one tool name to runtime-event metadata so display-layer fallbacks can still recover the semantic tool title.
/// 将工具名称附着到运行时事件元数据中，以便显示层在回退路径下仍能恢复语义化工具标题。
pub(crate) fn attach_tool_name_to_runtime_metadata(
    metadata: Option<Value>,
    tool_name: &str,
) -> Option<Value> {
    let mut object = match metadata {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = JsonMap::new();
            object.insert(String::from("host_metadata"), other);
            object
        }
        None => JsonMap::new(),
    };
    object.insert(
        String::from("tool_name"),
        Value::String(tool_name.to_string()),
    );
    Some(Value::Object(object))
}

/// Formats one host-side tool result into the full Markdown payload shown by the inline transcript and modal.
/// 将单次宿主侧工具结果格式化为内联转录与弹窗使用的完整 Markdown 载荷。
pub(crate) fn format_host_tool_result_runtime_detail(
    tool_result_id: &str,
    model_output: &str,
) -> String {
    let normalized_output = if model_output.trim().is_empty() {
        String::from("(empty result)")
    } else {
        model_output.trim().to_string()
    };
    format!("## Result ID\n\n`{tool_result_id}`\n\n## Tool Result\n\n{normalized_output}")
}

/// Converts one `vulcan-core` provider-turn delta into the GUI-facing runtime delta shape without changing the semantic payload.
/// 将一条 `vulcan-core` Provider 回合增量转换为面向 GUI 的运行时增量形状，同时不改变其语义载荷。
pub(crate) fn core_provider_turn_delta_to_runtime_delta(
    delta: CoreProviderTurnDelta,
) -> ProviderStreamDelta {
    match delta {
        CoreProviderTurnDelta::Thinking { delta } => ProviderStreamDelta::Thinking(delta),
        CoreProviderTurnDelta::Output { delta } => ProviderStreamDelta::Output(delta),
    }
}

/// Builds one normalized `vulcan-core` single-turn response from the final provider assistant content list and provider message identifier.
/// 根据最终 Provider 助手内容列表与 Provider 消息标识构建一份规范化的 `vulcan-core` 单回合响应。
pub(crate) fn build_openai_compatible_turn_response(
    provider_id: &str,
    model_id: &str,
    provider_message_id: Option<String>,
    assistant_content: &[ProviderAssistantContent],
    usage: Option<CoreProviderUsageSnapshot>,
) -> CoreProviderTurnResponse {
    let assistant_text = assistant_content
        .iter()
        .filter_map(|item| match item {
            ProviderAssistantContent::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    let assistant_reasoning = assistant_content
        .iter()
        .filter_map(|item| match item {
            ProviderAssistantContent::Reasoning(reasoning) => {
                let flattened = flatten_reasoning_content(reasoning);
                (!flattened.is_empty()).then_some(flattened)
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tool_calls = assistant_content
        .iter()
        .filter_map(|item| match item {
            ProviderAssistantContent::ToolCall(tool_call) => Some(tool_call.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    CoreProviderTurnResponse {
        assistant_text,
        assistant_reasoning,
        provider_message_id: provider_message_id.clone(),
        provider_message_state: None,
        usage,
        tool_calls,
        continuation: Some(CoreProviderContinuation {
            provider_id: Some(provider_id.to_string()),
            model_id: Some(model_id.to_string()),
            last_provider_message_id: provider_message_id,
            updated_at: Some(current_unix_millis_text()),
        }),
    }
}

/// Formats one tool-call detail body into the full Markdown payload shown by the inline transcript and modal.
/// 将单次工具调用详情正文格式化为内联转录与弹窗使用的完整 Markdown 载荷。
pub(crate) fn format_tool_call_runtime_detail(tool_name: &str, arguments: &Value) -> String {
    let pretty_arguments =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string());
    format!("## Tool Call\n\n`{tool_name}`\n\n## Arguments\n\n```json\n{pretty_arguments}\n```")
}

/// Builds one structured runtime-event metadata payload for a completed tool result while preserving host-side metadata fields.
/// 在保留宿主侧元数据字段的前提下，为已完成工具结果构建一份结构化运行时事件元数据载荷。
pub(crate) fn build_tool_result_runtime_metadata(
    tool_call_id: &str,
    tool_result_id: &str,
    host_metadata: Option<&Value>,
) -> Option<Value> {
    let mut metadata = JsonMap::new();
    metadata.insert(
        String::from("tool_call_id"),
        Value::String(tool_call_id.to_string()),
    );
    metadata.insert(
        String::from("tool_result_id"),
        Value::String(tool_result_id.to_string()),
    );

    if let Some(Value::Object(object)) = host_metadata {
        metadata.insert(String::from("host_result"), Value::Object(object.clone()));
        metadata.extend(object.clone());
    } else if let Some(other) = host_metadata {
        metadata.insert(String::from("host_metadata"), other.clone());
    }

    Some(Value::Object(metadata))
}

/// Flattens structured reasoning content into one readable text block for the transcript thinking section.
/// 将结构化推理内容压平成一段适合转录思考区展示的可读文本。
fn flatten_reasoning_content(content: &[ProviderReasoningPart]) -> String {
    content
        .iter()
        .filter_map(|part| match part {
            ProviderReasoningPart::Text(text) => Some(text.trim().to_string()),
            ProviderReasoningPart::Summary(summary) => Some(summary.trim().to_string()),
            ProviderReasoningPart::Opaque => None,
        })
        .filter(|part: &String| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the current Unix timestamp in milliseconds as one text value for protocol-state refreshes inside the shared provider bridge helpers.
/// 以文本形式返回当前 Unix 毫秒时间戳，供共享 Provider 桥接辅助逻辑刷新协议状态时使用。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::build_tool_result_runtime_metadata;
    use serde_json::json;

    /// Verifies completed tool-result runtime metadata preserves one nested `host_result` snapshot while still exposing the flattened host fields for existing task-card callers.
    /// 验证完成态工具结果运行时元数据会保留一份嵌套 `host_result` 快照，同时继续暴露扁平化宿主字段以兼容既有 task 卡片调用方。
    #[test]
    fn tool_result_runtime_metadata_keeps_nested_host_result_snapshot() {
        let metadata = build_tool_result_runtime_metadata(
            "call-1",
            "tool-result-call-1",
            Some(&json!({
                "kind": "generated_attachments",
                "payload": {
                    "outputs": [{
                        "kind": "image",
                        "file_name": "demo.png",
                        "local_path": r"C:\demo.png",
                    }],
                },
            })),
        )
        .expect("metadata should exist");

        assert_eq!(
            metadata
                .get("tool_call_id")
                .and_then(|value| value.as_str()),
            Some("call-1")
        );
        assert_eq!(
            metadata.get("kind").and_then(|value| value.as_str()),
            Some("generated_attachments")
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|value| value.get("kind"))
                .and_then(|value| value.as_str()),
            Some("generated_attachments")
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|value| value.get("payload"))
                .and_then(|value| value.get("outputs"))
                .and_then(|value| value.as_array())
                .map(Vec::len),
            Some(1)
        );
    }
}
