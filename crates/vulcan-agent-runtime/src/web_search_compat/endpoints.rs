//! Endpoint normalization helpers for configured web-search providers.
//! 配置化网页搜索供应商的端点归一化辅助能力。

use crate::provider_config::PersistedWebSearchProviderEntry;

/// Resolves the Gemini endpoint from the configured base URL and model.
/// 根据配置基址与模型解析 Gemini 端点。
pub(super) fn resolve_gemini_endpoint(provider: &PersistedWebSearchProviderEntry) -> String {
    let base_url = provider.resolved_base_url();
    if base_url.contains(":generateContent") {
        return base_url;
    }
    let base_url = base_url.trim_end_matches('/');
    format!(
        "{base_url}/models/{}:generateContent",
        provider.resolved_model()
    )
}

/// Resolves the xAI Responses endpoint from the configured base URL.
/// 根据配置基址解析 xAI Responses 端点。
pub(super) fn resolve_xai_responses_endpoint(provider: &PersistedWebSearchProviderEntry) -> String {
    let base_url = provider.resolved_base_url();
    if base_url.ends_with("/responses") {
        return base_url;
    }
    format!("{}/responses", base_url.trim_end_matches('/'))
}

/// Resolves one chat-completions endpoint from the configured base URL.
/// 根据配置基址解析 chat-completions 端点。
pub(super) fn resolve_chat_completions_endpoint(
    provider: &PersistedWebSearchProviderEntry,
) -> String {
    let base_url = provider.resolved_base_url();
    if base_url.ends_with("/chat/completions") {
        return base_url;
    }
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

/// Resolves the MiniMax Search endpoint by combining base URL overrides with region defaults while rejecting obvious regular text-API endpoints.
/// 在拒绝明显普通文本接口地址的前提下，结合基址覆盖与区域默认值解析 MiniMax Search 端点。
pub(super) fn resolve_minimax_endpoint(
    provider: &PersistedWebSearchProviderEntry,
) -> Result<String, String> {
    if provider.base_url.trim().is_empty() {
        return if provider.resolved_region().eq_ignore_ascii_case("cn") {
            Ok(String::from(
                "https://api.minimaxi.com/v1/coding_plan/search",
            ))
        } else {
            Ok(provider.resolved_base_url())
        };
    }

    let mut url = reqwest::Url::parse(&provider.resolved_base_url())
        .map_err(|error| format!("MiniMax Search Base URL 无效：{error}"))?;
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if minimax_path_looks_like_regular_text_api(&normalized_path) {
        return Err(String::from(
            "当前 MiniMax 第三方搜索使用 MiniMax Search 接口，不支持普通文本模型 API 地址。请填写搜索端点（当前官方路径为 `/v1/coding_plan/search`）或留空使用默认值。",
        ));
    }
    if normalized_path.is_empty() || normalized_path == "/" || normalized_path == "/v1" {
        url.set_path("/v1/coding_plan/search");
    }
    Ok(url.to_string())
}

/// Returns whether one MiniMax path obviously points at a regular text-generation endpoint instead of the dedicated MiniMax Search endpoint.
/// 返回单个 MiniMax 路径是否明显指向普通文本生成接口而非专用的 MiniMax Search 端点。
fn minimax_path_looks_like_regular_text_api(path: &str) -> bool {
    let normalized = path.trim().to_ascii_lowercase();
    normalized.contains("chatcompletion")
        || normalized.contains("/text/")
        || normalized.contains("/anthropic")
        || normalized.contains("/openai")
        || normalized.contains("/responses")
}

/// Builds the ordered Ollama endpoint attempts used by the local-first runtime flow.
/// 构建本地优先运行流程使用的 Ollama 有序端点尝试列表。
pub(super) fn build_ollama_attempts(
    provider: &PersistedWebSearchProviderEntry,
    api_key: Option<&str>,
) -> Vec<(String, Option<String>)> {
    let base_url = provider.resolved_base_url();
    if base_url.contains("/api/web_search") || base_url.contains("/api/experimental/web_search") {
        return vec![(base_url, api_key.map(ToOwned::to_owned))];
    }

    let trimmed = base_url.trim_end_matches('/');
    let mut attempts = vec![
        (
            format!("{trimmed}/api/experimental/web_search"),
            api_key.map(ToOwned::to_owned),
        ),
        (
            format!("{trimmed}/api/web_search"),
            api_key.map(ToOwned::to_owned),
        ),
    ];
    if api_key.is_some() && !trimmed.contains("ollama.com") {
        attempts.push((
            String::from("https://ollama.com/api/web_search"),
            api_key.map(ToOwned::to_owned),
        ));
    }
    attempts
}

/// Returns whether the configured Perplexity endpoint already points at the structured search API.
/// 返回配置的 Perplexity 端点是否已经指向结构化搜索 API。
pub(super) fn is_probably_perplexity_search_endpoint(base_url: &str) -> bool {
    let normalized = base_url.trim().trim_end_matches('/').to_ascii_lowercase();
    normalized.ends_with("/search")
}

/// Resolves the request model for Perplexity direct chat endpoints.
/// 解析 Perplexity 直连 chat 端点使用的请求模型。
pub(super) fn resolve_perplexity_request_model(
    provider: &PersistedWebSearchProviderEntry,
) -> String {
    let model = provider.resolved_model();
    if provider.resolved_base_url().contains("api.perplexity.ai")
        && model.starts_with("perplexity/")
    {
        model.trim_start_matches("perplexity/").to_string()
    } else {
        model
    }
}

/// Builds the SearXNG `/search` endpoint from a configured base URL.
/// 从配置的基址构建 SearXNG `/search` 端点。
pub(super) fn build_searxng_search_endpoint(
    provider: &PersistedWebSearchProviderEntry,
) -> Result<String, String> {
    let mut url = reqwest::Url::parse(&provider.resolved_base_url())
        .map_err(|error| format!("SearXNG Base URL 无效：{error}"))?;
    let pathname = if url.path().ends_with('/') {
        format!("{}search", url.path())
    } else {
        format!("{}/search", url.path())
    };
    url.set_path(&pathname);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}
