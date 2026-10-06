// Tolerant readers over `serde_json::Value`.
// 基于 `serde_json::Value` 的宽容读取器。
//
// The TUI does not link the service crates, so wire DTOs are read field by field. A missing or
// mistyped field reads as empty instead of failing the whole payload, which keeps older and newer
// services usable; the fields that identify an entity are still checked by the mappers.
// TUI 不链接服务端 crate，因此按字段读取传输 DTO。缺失或类型不符的字段读为空值而不是让整个载荷
// 失败，使新旧服务都可用；标识实体的字段仍由映射函数检查。

use serde_json::Value;

/// Reads a dotted path such as `lifetime.main.requestCount`; array indices are numeric segments.
/// 读取点分路径，例如 `lifetime.main.requestCount`；数组下标为数字段。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `&'a Value`.
/// 返回 `&'a Value`。
pub fn at<'a>(value: &'a Value, path: &str) -> &'a Value {
    // Current.
    // 当前选中或已读取值。
    let mut current = value;
    for part in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(part).unwrap_or(&Value::Null),
            Value::Array(items) => part
                .parse::<usize>()
                .ok()
                .and_then(|i| items.get(i))
                .unwrap_or(&Value::Null),
            _ => &Value::Null,
        };
    }
    current
}

/// Reads a protocol text/number field at the exact dotted path, empty when absent.
/// 在准确点分路径读取协议文本或数值字段，缺失时为空。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `String`.
/// 返回 `String`。
pub fn str_at(value: &Value, path: &str) -> String {
    text(at(value, path)).unwrap_or_default()
}

/// Reads a nonempty optional text/number field at the exact dotted path.
/// 在准确点分路径读取非空的可选文本或数值字段。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `Option<String>`.
/// 返回 `Option<String>`。
pub fn opt_str(value: &Value, path: &str) -> Option<String> {
    text(at(value, path)).filter(|s| !s.is_empty())
}

/// Strings as is, numbers in decimal (ids such as `turn_id` are numbers on the wire).
/// 字符串原样返回，数字转为十进制（`turn_id` 等标识在传输中为数字）。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
pub fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Unsigned number; numeric strings (`"1790771610232"`) and negative numbers are accepted.
/// 无符号数；也接受数字字符串（`"1790771610232"`），负数按 0 处理。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `u64`.
/// 返回 `u64`。
pub fn u64_at(value: &Value, path: &str) -> u64 {
    num(at(value, path))
}

/// Converts declared wire numeric/string values to a nonnegative display count.
/// 将声明的传输数值或字符串转换为非负显示计数。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `u64`.
/// 返回 `u64`。
pub fn num(value: &Value) -> u64 {
    match value {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_i64().map(|i| i.max(0) as u64))
            .or_else(|| n.as_f64().map(|f| f.max(0.0) as u64))
            .unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// Reads an optional numeric field, distinguishing absence from zero.
/// 读取可选数值字段，区分缺失与零。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `Option<u64>`.
/// 返回 `Option<u64>`。
pub fn opt_u64(value: &Value, path: &str) -> Option<u64> {
    // Protocol field value.
    // 当前协议字段值。
    let v = at(value, path);
    (!v.is_null()).then(|| num(v))
}

/// Reads the declared fractional value at the exact dotted path.
/// 在准确点分路径读取已声明的小数值。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `f32`.
/// 返回 `f32`。
pub fn f32_at(value: &Value, path: &str) -> f32 {
    match at(value, path) {
        Value::Number(n) => n.as_f64().unwrap_or(0.0) as f32,
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Reads the boolean display field at the exact dotted path.
/// 在准确点分路径读取布尔显示字段。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `bool`.
/// 返回 `bool`。
pub fn bool_at(value: &Value, path: &str) -> bool {
    at(value, path).as_bool().unwrap_or(false)
}

/// Borrows the exact protocol array, returning an empty display slice while absent.
/// 借用准确协议数组，缺失时返回空显示切片。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `&'a [Value]`.
/// 返回 `&'a [Value]`。
pub fn arr<'a>(value: &'a Value, path: &str) -> &'a [Value] {
    at(value, path).as_array().map_or(&[], Vec::as_slice)
}

/// Lower-cased, with `-` and `_` removed, so `tool_running` and `ToolRunning` compare equal.
/// 转小写并去掉 `-` 与 `_`，使 `tool_running` 与 `ToolRunning` 相等。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `String`.
/// 返回 `String`。
pub fn normalized(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    /// Verifies reads paths numbers and strings with synthetic data and assertions.
    /// 使用合成数据及断言验证 reads_paths_numbers_and_strings。
    fn reads_paths_numbers_and_strings() {
        // Value.
        // 当前协议值。
        let value = json!({"a": {"b": [{"c": "17"}, {"c": 3}]}, "s": 5});
        assert_eq!(u64_at(&value, "a.b.0.c"), 17);
        assert_eq!(u64_at(&value, "a.b.1.c"), 3);
        assert_eq!(str_at(&value, "s"), "5");
        assert!(at(&value, "a.x.y").is_null());
        assert_eq!(normalized("Tool_Running"), "toolrunning");
    }
}
