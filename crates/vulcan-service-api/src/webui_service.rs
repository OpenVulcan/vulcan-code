use super::*;

/// Returns whether browser service token locations contain the current instance token.
/// 返回浏览器服务令牌位置是否包含当前实例令牌。
///
/// # Parameters
/// 参数
/// `header_token` is the optional token header, `query` is the raw query string, and `expected_token` is the generated instance token.
/// `header_token` 是可选令牌请求头，`query` 是原始查询字符串，`expected_token` 是生成的实例令牌。
///
/// # Returns
/// 返回值
/// `true` when either header or query token matches the expected token.
/// 当 header 或 query 中的令牌与期望令牌匹配时返回 `true`。
pub fn webui_service_request_token_values_are_valid(
    header_token: Option<&str>,
    query: Option<&str>,
    expected_token: &str,
) -> bool {
    if expected_token.trim().is_empty() {
        return false;
    }
    if header_token.is_some_and(|token| webui_service_token_matches(token, expected_token)) {
        return true;
    }
    query
        .and_then(webui_service_token_from_query)
        .is_some_and(|token| webui_service_token_matches(&token, expected_token))
}

/// Extracts the service token value from a URL query string.
/// 从 URL 查询字符串中提取服务令牌值。
///
/// # Parameters
/// 参数
/// `query` is the raw query string without the leading question mark.
/// `query` 是不带问号前缀的原始查询字符串。
///
/// # Returns
/// 返回值
/// The first decoded `token` or `serviceToken` query parameter when present.
/// 存在时返回首个解码后的 `token` 或 `serviceToken` 查询参数。
pub fn webui_service_token_from_query(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key != "token" && key != "serviceToken" {
            continue;
        }
        return percent_decode_query_component(value);
    }
    None
}

/// Compares two browser service tokens.
/// 比较两个浏览器服务令牌。
///
/// # Parameters
/// 参数
/// `actual` is the token supplied by the request, and `expected` is the generated token.
/// `actual` 是请求提供的令牌，`expected` 是生成的令牌。
///
/// # Returns
/// 返回值
/// `true` when both token strings are exactly equal after trimming request whitespace.
/// 当两个令牌字符串在裁剪请求空白后完全一致时返回 `true`。
pub fn webui_service_token_matches(actual: &str, expected: &str) -> bool {
    actual.trim() == expected
}

/// Resolves candidate browser-facing local service ports from an optional override value.
/// 根据可选覆盖值解析浏览器侧本地服务候选端口。
///
/// # Parameters
/// 参数
/// `port_override` is the raw configured port value, usually read from `WEBUI_SERVICE_PORT_ENV`.
/// `port_override` 是原始配置端口值，通常来自 `WEBUI_SERVICE_PORT_ENV`。
///
/// # Returns
/// 返回值
/// A single override port when valid, otherwise the default discovery range.
/// 覆盖值有效时返回单个端口，否则返回默认发现端口范围。
pub fn webui_service_http_bind_ports_from_override(port_override: Option<&str>) -> Vec<u16> {
    if let Some(port) = port_override
        .map(str::trim)
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
    {
        return vec![port];
    }
    webui_service_http_default_bind_ports()
}

/// Returns the default browser-facing local service discovery port range.
/// 返回浏览器侧本地服务默认发现端口范围。
pub fn webui_service_http_default_bind_ports() -> Vec<u16> {
    (WEBUI_SERVICE_HTTP_PORT..=WEBUI_SERVICE_HTTP_PORT_END).collect()
}

/// Resolves the shared browser service discovery file path.
/// 解析共享浏览器服务发现文件路径。
///
/// # Parameters
/// 参数
/// `home_dir` is the optional user home directory and `temp_dir` is the fallback temporary directory.
/// `home_dir` 是可选用户主目录，`temp_dir` 是回退临时目录。
///
/// # Returns
/// 返回值
/// The path where browser hosts can discover the current service endpoint.
/// 返回浏览器宿主可发现当前服务端点的路径。
pub fn webui_service_discovery_snapshot_path(
    home_dir: Option<PathBuf>,
    temp_dir: PathBuf,
) -> PathBuf {
    let root = home_dir.unwrap_or(temp_dir);
    vulcan_runtime_assets::user_vulcan_code_root_for_home(&root).join("webui-service.json")
}

/// Decodes one URL query component enough for opaque service tokens.
/// 对单个 URL 查询组件进行足以支持不透明服务令牌的解码。
fn percent_decode_query_component(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut index = 0usize;
    let raw = value.as_bytes();
    while index < raw.len() {
        match raw[index] {
            b'+' => {
                bytes.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < raw.len() => {
                let high = hex_digit_value(raw[index + 1])?;
                let low = hex_digit_value(raw[index + 2])?;
                bytes.push((high << 4) | low);
                index += 3;
            }
            byte => {
                bytes.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(bytes).ok()
}

/// Converts one ASCII hex digit into its numeric value.
/// 将单个 ASCII 十六进制字符转换为数值。
fn hex_digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
