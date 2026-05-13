#![allow(dead_code)]

//! Anthropic client-side protocol helpers owned by the host runtime.
//! 宿主管理的 Anthropic 客户端侧协议辅助逻辑。

/// Normalizes one Anthropic base URL into the canonical API root used by the client builder.
/// 将一个 Anthropic 基础地址规范化为客户端构造器使用的标准 API 根地址。
pub(crate) fn normalize_anthropic_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');

    if let Some(stripped) = trimmed.strip_suffix("/v1/messages") {
        stripped.to_string()
    } else if let Some(stripped) = trimmed.strip_suffix("/messages") {
        stripped.to_string()
    } else if let Some(stripped) = trimmed.strip_suffix("/v1") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_anthropic_base_url;

    /// Verifies that all Anthropic message endpoint variants collapse to the same canonical base URL.
    /// 验证 Anthropic 的各类 messages 端点都会收敛成同一个标准基础地址。
    #[test]
    fn normalizes_message_base_urls() {
        assert_eq!(
            normalize_anthropic_base_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com"
        );
        assert_eq!(
            normalize_anthropic_base_url("https://api.anthropic.com/messages"),
            "https://api.anthropic.com"
        );
        assert_eq!(
            normalize_anthropic_base_url("https://api.anthropic.com/v1/"),
            "https://api.anthropic.com"
        );
    }
}
