//! Ordered canonical replay for a declared plaintext Responses target.
//! 声明明文契约的 Responses 目标所使用的有序规范回放。

use crate::LlmCoreError;
use serde_json::{Value, json};
use vulcan_ai_contract::{ContentItem, ReplaySidecar};

/// Restores canonical reasoning in `body` using exact message ranges from `sidecar`.
/// 使用 `sidecar` 的精确消息区间恢复 `body` 中的规范推理。
/// Returns the target body, or an error for invalid attribution or unsupported assistant content.
/// 返回目标正文；归属无效或助手内容不受支持时返回错误。
pub(crate) fn project_plaintext_reasoning(
    mut body: Value,
    sidecar: &ReplaySidecar,
) -> Result<Value, LlmCoreError> {
    // Input ranges refer to the original array; build a separate array so insertions never shift them.
    // 输入区间指向原始数组；另建数组使插入操作不会改变这些区间。
    let input = body
        .get("input")
        .and_then(Value::as_array)
        .ok_or_else(|| LlmCoreError::new("Plaintext Responses replay requires an input array."))?;
    // Replayed collects exact native prefixes and reconstructed canonical suffixes in message order.
    // replayed 按消息顺序收集精确原生前缀及重建的规范后缀。
    let mut replayed = Vec::new();
    // Cursor stays in the original input's coordinate space, including empty message ranges.
    // cursor 始终位于原始输入的坐标空间中，包括空消息区间。
    let mut cursor = 0;
    for entry in sidecar.assistant_messages().filter(|entry| {
        entry
            .content
            .iter()
            .any(|item| matches!(item, ContentItem::Reasoning { .. }))
    }) {
        // The generic encoder emits one item per durable provider item before canonical content.
        // 通用编码器在规范内容之前为每个持久化供应商项输出一个输入项。
        let range = entry.input_range.clone();
        if range.start < cursor
            || range.end < range.start
            || range.end > input.len()
            || entry.provider_context_item_count > range.len()
        {
            return Err(LlmCoreError::new(
                "Plaintext Responses replay has invalid message attribution.",
            ));
        }
        replayed.extend_from_slice(&input[cursor..range.start + entry.provider_context_item_count]);
        append_canonical_items(&entry.content, &mut replayed)?;
        cursor = range.end;
    }
    replayed.extend_from_slice(&input[cursor..]);
    body["input"] = Value::Array(replayed);
    Ok(body)
}

/// Appends ordered `content` to `input` without copying route-bound signatures or IDs.
/// 向 `input` 追加有序 `content`，不复制绑定路由的签名或标识。
/// Returns an error when a canonical item cannot be represented by this plaintext contract.
/// 规范项无法由本明文契约表达时返回错误。
fn append_canonical_items(
    content: &[ContentItem],
    input: &mut Vec<Value>,
) -> Result<(), LlmCoreError> {
    // Text is accumulated exactly as in the baseline encoder and flushed at every semantic boundary.
    // text 与基线编码器相同地累计，并在每个语义边界输出。
    let mut text = String::new();
    for item in content {
        match item {
            ContentItem::Text { text: part } | ContentItem::Refusal { text: part } => {
                text.push_str(part)
            }
            ContentItem::StructuredJson { value } => {
                if let Some(part) = value.as_str() {
                    text.push_str(part);
                } else {
                    text.push_str(&value.to_string());
                }
            }
            ContentItem::Reasoning {
                text: reasoning, ..
            } => {
                flush_text(input, &mut text);
                if reasoning.trim().is_empty() {
                    return Err(LlmCoreError::new(
                        "Plaintext Responses replay requires non-empty reasoning text.",
                    ));
                }
                input.push(json!({"type": "reasoning", "content": [{"type": "reasoning_text", "text": reasoning}]}));
            }
            ContentItem::ToolCall { call } => {
                flush_text(input, &mut text);
                input.push(json!({
                    "type": "function_call", "call_id": call.id, "name": call.name,
                    "arguments": serde_json::to_string(&call.arguments).map_err(|error| LlmCoreError::new(format!("Plaintext Responses tool arguments could not be encoded: {error}")))?,
                }));
            }
            ContentItem::Resource { .. }
            | ContentItem::Citation { .. }
            | ContentItem::ToolResult { .. } => {
                return Err(LlmCoreError::new(
                    "Plaintext Responses replay cannot represent this assistant content item.",
                ));
            }
        }
    }
    flush_text(input, &mut text);
    Ok(())
}

/// Moves accumulated assistant `text` into `input`, preserving the baseline empty-text behavior.
/// 将累计的助手 `text` 移入 `input`，保持基线对空文本的处理方式。
fn flush_text(input: &mut Vec<Value>, text: &mut String) {
    if !text.is_empty() {
        input.push(json!({"role": "assistant", "content": [{"type": "output_text", "text": std::mem::take(text)}]}));
    }
}
