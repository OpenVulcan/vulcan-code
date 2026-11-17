//! Tool identities and readable arguments/results for context snapshots.
//! 上下文快照中的工具标识与可读参数、返回。

use super::{is_responses_tool_call, redact_context_text, render_raw_context};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One invocation or result with its producer-derived name and redacted readable payload.
/// 一条调用或返回，包含生产者提供的名称及已脱敏的可读载荷。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolNodePart {
    /// Stable tool name, absent when the corresponding invocation is outside this request.
    /// 稳定的工具名；对应调用在本请求之外时可缺省。
    name: Option<String>,
    /// Whether this block is a result rather than a call.
    /// 此块是否为工具返回。
    pub(super) result: bool,
    /// Complete redacted tool arguments or model-visible return value.
    /// 完整且已脱敏的工具参数或模型可见返回值。
    content: Option<String>,
}

/// Exact selected fields from one protocol tool block.
/// 从一个协议工具块选出的精确字段。
struct WireToolPart<'a> {
    /// Protocol call identity used to associate a return with its invocation.
    /// 用于将返回关联到调用的协议调用标识。
    id: Option<&'a str>,
    /// Tool name explicitly supplied by the source.
    /// 来源明确提供的工具名。
    name: Option<&'a str>,
    /// Original arguments or returned value.
    /// 原始参数或返回值。
    content: Option<&'a Value>,
    /// True for returned values.
    /// 返回值为 true。
    result: bool,
}

/// Returns exact wire or simulated tool blocks and registers names by call ID for later results.
/// 返回精确的实发或模拟工具块，并按调用标识登记工具名供后续返回使用。
/// `names` belongs to one snapshot and `credential` is masked in the returned blocks.
/// `names` 属于单份快照，返回的工具块会遮蔽 `credential`。
pub(super) fn project_tool_parts(
    value: &Value,
    names: &mut BTreeMap<String, String>,
    credential: &str,
) -> Vec<ToolNodePart> {
    // Preserve independent call identities even when a single model message issues multiple calls.
    // 即使单条模型消息发出多次调用，也保留独立的调用标识。
    let parts = wire_tool_parts(value);
    parts
        .into_iter()
        .map(|part| {
            if !part.result
                && let (Some(id), Some(name)) = (part.id, part.name)
            {
                names.insert(id.to_owned(), name.to_owned());
            }
            // Real tool output may be a JSON string carrying Vulcan's model-output envelope.
            // 实发工具返回可能是携带 Vulcan 模型输出信封的 JSON 字符串。
            let decoded = part
                .content
                .and_then(Value::as_str)
                .filter(|text| text.starts_with(['{', '[']))
                .and_then(|text| serde_json::from_str::<Value>(text).ok());
            // Simulated tool results retain the envelope as an object.
            // 模拟工具返回保留对象形式的信封。
            let envelope = decoded.as_ref().or(part.content).filter(|value| {
                part.result
                    && value.get("is_error").is_some_and(Value::is_boolean)
                    && value.get("tool_call_id").is_some()
                    && value.get("tool_name").and_then(Value::as_str).is_some()
            });
            // Use the paired name first; an incremental request may only carry the explicit result name.
            // 优先使用配对的工具名；增量请求可能只携带返回本身提供的名称。
            let name = part
                .name
                .or_else(|| part.id.and_then(|id| names.get(id).map(String::as_str)))
                .or_else(|| {
                    envelope.and_then(|value| value.get("tool_name").and_then(Value::as_str))
                })
                .map(|name| redact_context_text(name, credential).into_owned());
            // The result preview shows the value provided to the model rather than host diagnostics.
            // 返回预览显示提供给模型的值，宿主诊断保留在原始内容中。
            let content = envelope
                .and_then(|value| value.get("model_output"))
                .or(decoded.as_ref().filter(|_| !part.result))
                .or(part.content)
                .map(|value| render_tool_content(value, credential));
            ToolNodePart {
                name,
                result: part.result,
                content,
            }
        })
        .collect()
}

/// Selects exact fields defined by each encoder and the typed V1 projection.
/// 选取各编码器及有类型的 V1 投影定义的精确字段。
/// Returns the chronological blocks carried by one node.
/// 返回单个节点按时间顺序承载的工具块。
fn wire_tool_parts(value: &Value) -> Vec<WireToolPart<'_>> {
    // Simulated tool results already carry their own names and need no wire lookup.
    // 模拟工具返回已经携带工具名，无需查找传输标识。
    let kind = value.get("kind").and_then(Value::as_str);
    // Responses puts invocation and result blocks at the top level of its input array.
    // Responses 将调用及返回块放在输入数组顶层。
    let item_type = value.get("type").and_then(Value::as_str);
    if kind == Some("tool_result") {
        return vec![named_part(
            value,
            "provider_tool_call_id",
            "tool_name",
            "output",
            true,
        )];
    }
    if is_responses_tool_call(item_type) {
        return vec![named_part(
            value,
            "call_id",
            "name",
            if item_type == Some("custom_tool_call") {
                "input"
            } else {
                "arguments"
            },
            false,
        )];
    }
    if matches!(
        item_type,
        Some("function_call_output" | "custom_tool_call_output")
    ) {
        return vec![named_part(value, "call_id", "name", "output", true)];
    }
    if value.get("role").and_then(Value::as_str) == Some("tool") {
        return vec![named_part(value, "tool_call_id", "name", "content", true)];
    }
    if let Some(calls) = value.get("tool_calls").and_then(Value::as_array) {
        return calls
            .iter()
            .filter_map(|call| {
                if kind == Some("assistant_text") {
                    return Some(named_part(
                        call,
                        "provider_tool_call_id",
                        "tool_name",
                        "input",
                        false,
                    ));
                }
                // Chat function calls nest arguments under function while their identity stays outside.
                // Chat 函数调用把参数放在 function 内，调用标识留在外层。
                let function = call.get("function")?;
                Some(WireToolPart {
                    id: call.get("id").and_then(Value::as_str),
                    ..named_part(function, "id", "name", "arguments", false)
                })
            })
            .collect();
    }
    if let Some(parts) = value.get("parts").and_then(Value::as_array) {
        return parts
            .iter()
            .filter_map(|part| {
                if let Some(call) = part.get("functionCall") {
                    return Some(named_part(call, "id", "name", "args", false));
                }
                part.get("functionResponse")
                    .map(|result| named_part(result, "id", "name", "response", true))
            })
            .collect();
    }
    value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| match part.get("type").and_then(Value::as_str) {
            Some("tool_use") => Some(named_part(part, "id", "name", "input", false)),
            Some("tool_result") => Some(named_part(part, "tool_use_id", "name", "content", true)),
            _ => None,
        })
        .collect()
}

/// Reads the producer-specified keys for one call or result.
/// 读取一条调用或返回由生产者指定的键。
fn named_part<'a>(
    value: &'a Value,
    id_key: &str,
    name_key: &str,
    content_key: &str,
    result: bool,
) -> WireToolPart<'a> {
    WireToolPart {
        id: value.get(id_key).and_then(Value::as_str),
        name: value.get(name_key).and_then(Value::as_str),
        content: value.get(content_key),
        result,
    }
}

/// Projects strings as readable text and keeps structured payloads under the same redaction.
/// 将字符串投影为可读文本，并对结构化载荷应用相同脱敏。
fn render_tool_content(value: &Value, credential: &str) -> String {
    match value {
        Value::String(text) if text.starts_with("data:") => "{INLINE_MEDIA}".to_owned(),
        _ => render_raw_context(value, credential),
    }
}
