//! Shared utility helpers for Google Gemini transport modules.
//! Google Gemini 传输子模块共享的辅助逻辑。

use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Deep-merges two JSON values while allowing overlay branches to replace non-object leaves.
/// 深度合并两个 JSON 值，并允许覆盖分支替换非对象叶子节点。
pub(super) fn merge_json_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_object), Value::Object(overlay_object)) => {
            for (key, overlay_value) in overlay_object {
                let merged_value = match base_object.remove(&key) {
                    Some(base_value) => merge_json_values(base_value, overlay_value),
                    None => overlay_value,
                };
                base_object.insert(key, merged_value);
            }
            Value::Object(base_object)
        }
        (_, overlay) => overlay,
    }
}

/// Returns the current Unix timestamp in milliseconds as one text value for Gemini continuation refreshes.
/// 以文本形式返回当前 Unix 毫秒时间戳，供 Gemini continuation 刷新使用。
pub(super) fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
