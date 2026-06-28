//! JSON field extraction helpers shared by web-search providers.
//! 网页搜索供应商共享的 JSON 字段提取辅助能力。

use serde_json::Value;

/// Returns the first non-empty string field from one JSON object.
/// 从单个 JSON 对象返回第一个非空字符串字段。
pub(super) fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .next()
}

/// Returns the first non-empty string from a JSON array field.
/// 从 JSON 数组字段返回第一个非空字符串。
pub(super) fn first_array_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_array).and_then(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
            .next()
    })
}

/// Extracts one string array from the given JSON field.
/// 从给定 JSON 字段中提取字符串数组。
pub(super) fn extract_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
