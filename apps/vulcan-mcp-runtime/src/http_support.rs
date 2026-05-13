//! Shared HTTP helper functions reused by MCP transports built on top of reqwest and rmcp.
//! 供基于 reqwest 与 rmcp 的 MCP 传输复用的共享 HTTP 辅助函数。

use std::collections::HashMap;

use http::{HeaderMap, HeaderName, HeaderValue};

use crate::error::McpRuntimeError;

/// Parse custom string headers into a typed `HeaderMap`.
/// 将自定义字符串请求头解析为类型化 `HeaderMap`。
pub(crate) fn parse_http_headers_to_header_map(
    custom_headers: HashMap<String, String>,
) -> Result<HeaderMap, McpRuntimeError> {
    let mut parsed = HeaderMap::with_capacity(custom_headers.len());

    for (raw_name, raw_value) in custom_headers {
        let header_name = HeaderName::from_bytes(raw_name.trim().as_bytes()).map_err(|error| {
            McpRuntimeError::invalid_argument(format!(
                "invalid MCP HTTP header name `{}`: {}",
                raw_name, error
            ))
        })?;
        let header_value = HeaderValue::from_str(raw_value.trim()).map_err(|error| {
            McpRuntimeError::invalid_argument(format!(
                "invalid MCP HTTP header value for `{}`: {}",
                raw_name, error
            ))
        })?;
        parsed.append(header_name, header_value);
    }

    Ok(parsed)
}

/// Parse custom string headers into the `HashMap` shape required by rmcp streamable HTTP configuration.
/// 将自定义字符串请求头解析为 rmcp streamable HTTP 配置要求的 `HashMap` 形态。
pub(crate) fn parse_http_headers_to_hash_map(
    custom_headers: HashMap<String, String>,
) -> Result<HashMap<HeaderName, HeaderValue>, McpRuntimeError> {
    Ok(parse_http_headers_to_header_map(custom_headers)?
        .into_iter()
        .filter_map(|(name, value)| name.map(|header_name| (header_name, value)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{parse_http_headers_to_hash_map, parse_http_headers_to_header_map};
    use std::collections::HashMap;

    #[test]
    fn header_map_parser_keeps_trimmed_name_and_value() {
        let parsed = parse_http_headers_to_header_map(HashMap::from([(
            String::from(" X-Test "),
            String::from(" value "),
        )]))
        .expect("header map parsing should succeed");

        assert_eq!(parsed["x-test"], "value");
    }

    #[test]
    fn hash_map_parser_keeps_trimmed_name_and_value() {
        let parsed = parse_http_headers_to_hash_map(HashMap::from([(
            String::from(" X-Test "),
            String::from(" value "),
        )]))
        .expect("hash-map parsing should succeed");

        let header_name = http::HeaderName::from_static("x-test");
        assert_eq!(parsed[&header_name], "value");
    }
}
