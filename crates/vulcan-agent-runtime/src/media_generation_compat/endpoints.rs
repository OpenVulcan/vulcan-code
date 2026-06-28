//! MiniMax endpoint normalization helpers for media-generation capabilities.
//! 媒体生成能力使用的 MiniMax 端点归一化辅助能力。

use crate::provider_config::PersistedAiCapabilityVendorEntry;

/// Resolves the correct MiniMax endpoint from one shared vendor root and one capability-specific path.
/// 根据共享供应商根地址与能力专属路径解析正确的 MiniMax 端点。
pub(super) fn resolve_minimax_endpoint(
    vendor: &PersistedAiCapabilityVendorEntry,
    capability_path: &str,
    allow_regular_text_api: bool,
) -> Result<String, String> {
    let region = vendor.resolved_region();
    let host = if region.eq_ignore_ascii_case("cn") {
        "https://api.minimaxi.com"
    } else {
        "https://api.minimax.io"
    };
    let base_url = vendor.base_url.trim();
    if base_url.is_empty() {
        return Ok(format!("{host}{capability_path}"));
    }

    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| format!("MiniMax base URL is invalid: {error}"))?;
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if !allow_regular_text_api && minimax_path_looks_like_regular_text_api(&normalized_path) {
        return Err(String::from(
            "This MiniMax capability uses dedicated capability endpoints and does not support regular text-model API URLs. Provide a MiniMax root URL such as `https://api.minimax.io`, or leave the field empty to use the default.",
        ));
    }
    url.set_path(capability_path);
    url.set_query(None);
    Ok(url.to_string())
}

/// Resolves one MiniMax endpoint that appends query parameters to a capability root path.
/// 解析一个需要在能力根路径后附加查询参数的 MiniMax 端点。
pub(super) fn resolve_minimax_file_or_task_endpoint(
    vendor: &PersistedAiCapabilityVendorEntry,
    capability_path: &str,
    query_pairs: &[(&str, &str)],
) -> Result<String, String> {
    let endpoint = resolve_minimax_endpoint(vendor, capability_path, false)?;
    let mut url = reqwest::Url::parse(&endpoint)
        .map_err(|error| format!("MiniMax endpoint is invalid: {error}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query_pairs {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

/// Returns whether one MiniMax path obviously points at a regular text-generation endpoint.
/// 返回单个 MiniMax 路径是否明显指向普通文本生成接口。
fn minimax_path_looks_like_regular_text_api(path: &str) -> bool {
    let normalized = path.trim().to_ascii_lowercase();
    normalized.contains("chatcompletion")
        || normalized.contains("/text/")
        || normalized.contains("/anthropic")
        || normalized.contains("/openai")
        || normalized.contains("/responses")
        || normalized.contains("/messages")
}
