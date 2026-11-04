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
    let actual = actual.trim().as_bytes();
    let expected = expected.as_bytes();
    // Every byte is folded, so the comparison time does not reveal how long a matching prefix is.
    // 折叠每一个字节，使比较耗时不会泄露匹配前缀的长度。
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
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

#[cfg(test)]
mod tests {
    use super::webui_service_token_matches;

    /// Token matching accepts only the exact token, ignoring request whitespace.
    /// 令牌匹配只接受精确令牌，并忽略请求空白。
    #[test]
    fn token_matching_accepts_only_the_exact_token() {
        assert!(webui_service_token_matches(
            " secret-token\n",
            "secret-token"
        ));
        for candidate in ["secret-toke", "secret-token-", "secret-tokEn", ""] {
            assert!(
                !webui_service_token_matches(candidate, "secret-token"),
                "{candidate:?}"
            );
        }
    }
}
