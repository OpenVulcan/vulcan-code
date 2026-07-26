//! Argument parsing for configured web-search calls.
//! 配置化网页搜索调用的参数解析。

use serde_json::Value;

use super::types::WebSearchInvocation;

/// Parses model-provided JSON arguments into a stable search invocation.
/// 将模型提供的 JSON 参数解析为稳定的搜索调用。
pub(super) fn parse_web_search_invocation(args: Value) -> Result<WebSearchInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("web_search 参数必须是 JSON 对象。"))?;
    let query = object
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("web_search 缺少必填参数 `query`。"))?
        .to_string();
    let max_results = object
        .get("max_results")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .map(|value| value.clamp(1, 20));

    Ok(WebSearchInvocation { query, max_results })
}
