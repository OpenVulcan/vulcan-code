//! Canonical reasoning fields used by protocol conversion.
//! 协议转换使用的规范推理字段。

use serde_json::Value;

/// Copies one canonical Responses reasoning effort into the generic Chat field.
/// 将规范 Responses 推理档位复制到通用 Chat 字段。
///
/// Provider-specific switches such as DeepSeek `thinking` and Alibaba
/// `enable_thinking` are deliberately not handled here. They are projected by
/// the validated Vulcan route after structural protocol conversion.
/// DeepSeek 的 `thinking`、Alibaba 的 `enable_thinking` 等供应商专属开关不在此处处理；
/// 它们由经过验证的 Vulcan 路由在协议结构转换后负责投影。
pub(crate) fn apply_canonical_reasoning_option(result: &mut Value, body: &Value) {
    let Some(effort) = body
        .pointer("/reasoning/effort")
        .and_then(Value::as_str)
        .filter(|effort| !matches!(*effort, "none" | "off" | "disabled"))
    else {
        return;
    };
    result["reasoning_effort"] = Value::String(effort.to_string());
}

/// Validates one canonical effort before a protocol-specific converter uses it.
/// 在协议专属转换器使用前校验一个规范推理档位。
///
/// This function only recognizes the provider-neutral vocabulary. It does not
/// infer a provider, inspect a URL, or silently promote one effort to another.
/// 此函数只识别供应商无关词汇，不推断供应商、不检查 URL，也不会静默提升档位。
pub(crate) fn canonical_reasoning_effort(effort: &str) -> Option<&'static str> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "minimal" => Some("minimal"),
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "xhigh" => Some("xhigh"),
        "max" => Some("max"),
        _ => None,
    }
}
