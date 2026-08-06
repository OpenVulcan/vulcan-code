//! Lossless reasoning transport between OpenAI Responses and Anthropic Messages.
//! OpenAI Responses 与 Anthropic Messages 之间的无损思考传输。

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

/// Prefixes an encoded OpenAI reasoning item with a versioned bridge marker.
/// 为编码后的 OpenAI reasoning item 添加带版本的桥接标记。
const OPENAI_REASONING_ITEM_PREFIX: &str = "ccswitch-openai-reasoning-v1:";

/// Extracts the visible summary text from one Responses reasoning item.
/// 从一项 Responses reasoning item 中提取可见摘要文本。
pub(crate) fn reasoning_summary_text(item: &Value) -> String {
    item.get("summary")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| {
            matches!(
                part.get("type").and_then(Value::as_str),
                Some("summary_text" | "reasoning_text")
            )
            .then(|| part.get("text").and_then(Value::as_str))
            .flatten()
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Encodes one complete Responses reasoning item for an Anthropic signature field.
/// 将完整的 Responses reasoning item 编码为 Anthropic signature 字段。
pub(crate) fn encode_openai_reasoning_item(item: &Value) -> Option<String> {
    if item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return None;
    }
    let bytes = serde_json::to_vec(item).ok()?;
    Some(format!(
        "{OPENAI_REASONING_ITEM_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(bytes)
    ))
}

/// Decodes one bridge signature back into the exact original Responses reasoning item.
/// 将桥接 signature 解码回原始的 Responses reasoning item。
pub fn decode_openai_reasoning_item(encoded: &str) -> Option<Value> {
    let payload = encoded.strip_prefix(OPENAI_REASONING_ITEM_PREFIX)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let item: Value = serde_json::from_slice(&bytes).ok()?;
    (item.get("type").and_then(Value::as_str) == Some("reasoning")).then_some(item)
}

/// Converts one Responses reasoning item into a valid Anthropic thinking block.
/// 将一项 Responses reasoning item 转换为有效的 Anthropic thinking block。
pub(crate) fn anthropic_block_from_openai_reasoning_item(item: &Value) -> Option<Value> {
    if item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return None;
    }
    let text = reasoning_summary_text(item);
    let has_encrypted_content = item
        .get("encrypted_content")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    if has_encrypted_content {
        let envelope = encode_openai_reasoning_item(item)?;
        if text.is_empty() {
            return Some(json!({
                "type": "redacted_thinking",
                "data": envelope,
            }));
        }
        return Some(json!({
            "type": "thinking",
            "thinking": text,
            "signature": envelope,
        }));
    }
    (!text.is_empty()).then(|| json!({"type":"thinking","thinking":text}))
}

/// Restores one Responses reasoning item from an Anthropic thinking block.
/// 从 Anthropic thinking block 恢复一项 Responses reasoning item。
pub fn openai_reasoning_item_from_anthropic_block(block: &Value) -> Option<Value> {
    match block.get("type").and_then(Value::as_str) {
        Some("thinking") => block
            .get("signature")
            .and_then(Value::as_str)
            .and_then(decode_openai_reasoning_item),
        Some("redacted_thinking") => block
            .get("data")
            .and_then(Value::as_str)
            .and_then(decode_openai_reasoning_item),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an encrypted Responses reasoning item round-trips through Anthropic thinking.
    /// 验证带加密内容的 Responses reasoning item 可以经 Anthropic thinking 无损往返。
    #[test]
    fn encrypted_reasoning_round_trips_through_thinking() {
        let item = json!({
            "id": "rs_1",
            "type": "reasoning",
            "summary": [{"type":"summary_text","text":"Need a tool."}],
            "encrypted_content": "opaque",
        });
        let block = anthropic_block_from_openai_reasoning_item(&item).expect("thinking block");
        assert_eq!(block["type"], "thinking");
        assert_eq!(
            openai_reasoning_item_from_anthropic_block(&block),
            Some(item)
        );
    }

    /// Verifies an encrypted item without summary uses redacted thinking.
    /// 验证没有摘要的加密 item 使用 redacted thinking。
    #[test]
    fn encrypted_reasoning_without_summary_uses_redacted_thinking() {
        let item = json!({
            "id": "rs_2",
            "type": "reasoning",
            "summary": [],
            "encrypted_content": "opaque",
        });
        let block = anthropic_block_from_openai_reasoning_item(&item).expect("redacted block");
        assert_eq!(block["type"], "redacted_thinking");
        assert_eq!(
            openai_reasoning_item_from_anthropic_block(&block),
            Some(item)
        );
    }
}
