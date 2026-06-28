//! Structured-result web-search provider request implementations.
//! 结构化结果网页搜索供应商请求实现。

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::{Value, json};

use crate::provider_config::PersistedWebSearchProviderEntry;

use super::{
    endpoints::build_searxng_search_endpoint,
    helpers::{first_array_string, first_string},
    http::{fetch_json, fetch_text},
    parsing::{is_duckduckgo_bot_challenge, parse_duckduckgo_html, parse_structured_results_array},
    types::{WebSearchInvocation, WebSearchProviderResponse, WebSearchResult},
};

/// Executes a DuckDuckGo HTML search request.
/// 执行 DuckDuckGo HTML 搜索请求。
pub(super) async fn execute_duckduckgo_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let region = provider.resolved_region();
    let html = fetch_text(
        client
            .get(provider.resolved_base_url())
            .header(
                USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
            )
            .query(&[
                ("q", invocation.query.as_str()),
                ("kl", region.as_str()),
                ("kp", "-1"),
            ]),
    )
    .await?;

    if is_duckduckgo_bot_challenge(&html) {
        return Err(String::from("DuckDuckGo 返回了机器人验证页面。"));
    }

    Ok(WebSearchProviderResponse::ResultList(
        parse_duckduckgo_html(&html)
            .into_iter()
            .take(max_results)
            .collect(),
    ))
}

/// Executes a Brave Search API request.
/// 执行 Brave Search API 请求。
pub(super) async fn execute_brave_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let count = max_results.to_string();
    let payload = fetch_json(
        client
            .get(provider.resolved_base_url())
            .header(ACCEPT, "application/json")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", invocation.query.as_str()), ("count", count.as_str())]),
    )
    .await?;

    Ok(WebSearchProviderResponse::ResultList(
        payload
            .pointer("/web/results")
            .and_then(Value::as_array)
            .map(|items| parse_structured_results_array(items))
            .unwrap_or_default(),
    ))
}

/// Executes a Tavily Search API request.
/// 执行 Tavily Search API 请求。
pub(super) async fn execute_tavily_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let payload = fetch_json(client.post(provider.resolved_base_url()).json(&json!({
        "api_key": api_key,
        "query": invocation.query.as_str(),
        "max_results": max_results,
        "search_depth": "basic",
        "include_answer": false,
        "include_raw_content": false
    })))
    .await?;

    Ok(WebSearchProviderResponse::ResultList(
        payload
            .get("results")
            .and_then(Value::as_array)
            .map(|items| parse_structured_results_array(items))
            .unwrap_or_default(),
    ))
}

/// Executes an Exa Search API request.
/// 执行 Exa Search API 请求。
pub(super) async fn execute_exa_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let payload = fetch_json(
        client
            .post(provider.resolved_base_url())
            .header("x-api-key", api_key)
            .json(&json!({
                "query": invocation.query.as_str(),
                "numResults": max_results
            })),
    )
    .await?;

    Ok(WebSearchProviderResponse::ResultList(
        payload
            .get("results")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let title = first_string(item, &["title"])
                            .unwrap_or_else(|| String::from("未命名结果"));
                        let url = first_string(item, &["url"])?;
                        // Exa result payloads expose body previews as text, summary, snippet, or highlight arrays depending on endpoint options.
                        // Exa 结果载荷会随端点选项以 text、summary、snippet 或 highlights 数组暴露正文预览。
                        let snippet = first_string(item, &["text", "summary", "snippet"])
                            .or_else(|| first_array_string(item, "highlights"))
                            .unwrap_or_default();
                        Some(WebSearchResult {
                            title,
                            url,
                            snippet,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    ))
}

/// Executes a Firecrawl Search API request.
/// 执行 Firecrawl Search API 请求。
pub(super) async fn execute_firecrawl_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let payload = fetch_json(
        client
            .post(provider.resolved_base_url())
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "query": invocation.query.as_str(),
                "limit": max_results
            })),
    )
    .await?;

    // Firecrawl-compatible search responses have appeared as top-level results and nested data arrays.
    // Firecrawl 兼容搜索响应已知会以顶层 results 或嵌套 data 数组形式返回。
    let results = payload
        .get("results")
        .or_else(|| payload.pointer("/data/results"))
        .or_else(|| payload.pointer("/data/data"))
        .and_then(Value::as_array)
        .map(|items| parse_structured_results_array(items))
        .unwrap_or_default();

    Ok(WebSearchProviderResponse::ResultList(results))
}

/// Executes a SearXNG search request.
/// 执行 SearXNG 搜索请求。
pub(super) async fn execute_searxng_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let endpoint = build_searxng_search_endpoint(provider)?;
    let payload = fetch_json(
        client
            .get(endpoint)
            .header(ACCEPT, "application/json")
            .query(&[("q", invocation.query.as_str()), ("format", "json")]),
    )
    .await?;

    Ok(WebSearchProviderResponse::ResultList(
        payload
            .get("results")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .take(max_results)
                    .filter_map(|item| {
                        let title = first_string(item, &["title"])?;
                        let url = first_string(item, &["url"])?;
                        // SearXNG engines normalize snippets under content, snippet, or description depending on source engine.
                        // SearXNG 会根据来源搜索引擎把摘要归一到 content、snippet 或 description 字段。
                        let snippet = first_string(item, &["content", "snippet", "description"])
                            .unwrap_or_default();
                        Some(WebSearchResult {
                            title,
                            url,
                            snippet,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    ))
}

/// Executes a SerpAPI Google search request.
/// 执行 SerpAPI Google 搜索请求。
pub(super) async fn execute_serpapi_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let count = max_results.to_string();
    let payload = fetch_json(client.get(provider.resolved_base_url()).query(&[
        ("engine", "google"),
        ("q", invocation.query.as_str()),
        ("api_key", api_key),
        ("num", count.as_str()),
    ]))
    .await?;

    Ok(WebSearchProviderResponse::ResultList(
        payload
            .get("organic_results")
            .and_then(Value::as_array)
            .map(|items| parse_structured_results_array(items))
            .unwrap_or_default(),
    ))
}
