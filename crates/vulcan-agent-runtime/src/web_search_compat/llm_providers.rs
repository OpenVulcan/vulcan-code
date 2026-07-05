//! Narrative and LLM-backed web-search provider request implementations.
//! 叙述型与 LLM 承载的网页搜索供应商请求实现。

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};

use crate::provider_config::PersistedWebSearchProviderEntry;

use super::{
    endpoints::{
        build_ollama_attempts, is_probably_perplexity_search_endpoint,
        resolve_chat_completions_endpoint, resolve_gemini_endpoint, resolve_minimax_endpoint,
        resolve_perplexity_request_model, resolve_xai_responses_endpoint,
    },
    helpers::{extract_string_array, first_string},
    http::{fetch_json, send_request},
    parsing::{
        extract_kimi_citation_urls, extract_kimi_message_text, extract_perplexity_citations,
        extract_xai_output_text, parse_structured_results_array, truncate_error_text,
    },
    types::{WebSearchCitation, WebSearchInvocation, WebSearchProviderResponse, WebSearchResult},
};

/// Executes a Gemini grounding search request.
/// 执行 Gemini grounding 搜索请求。
pub(super) async fn execute_gemini_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
) -> Result<WebSearchProviderResponse, String> {
    let endpoint = resolve_gemini_endpoint(provider);
    let payload = fetch_json(
        client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header("x-goog-api-key", api_key)
            .json(&json!({
                "contents": [{
                    "parts": [{
                        "text": invocation.query.as_str()
                    }]
                }],
                "tools": [{
                    "google_search": {}
                }]
            })),
    )
    .await?;

    if let Some(error_message) = payload
        .get("error")
        .and_then(|error| first_string(error, &["message", "status"]))
    {
        return Err(error_message);
    }

    let content = payload
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| String::from("No response"));

    let citations = payload
        .pointer("/candidates/0/groundingMetadata/groundingChunks")
        .and_then(Value::as_array)
        .map(|chunks| {
            chunks
                .iter()
                .filter_map(|chunk| {
                    let web = chunk.get("web")?;
                    let url = first_string(web, &["uri"])?;
                    Some(WebSearchCitation {
                        title: first_string(web, &["title"]).unwrap_or_default(),
                        url,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(WebSearchProviderResponse::Narrative { content, citations })
}

/// Executes an xAI Responses web-search request.
/// 执行 xAI Responses 联网搜索请求。
pub(super) async fn execute_grok_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
) -> Result<WebSearchProviderResponse, String> {
    let endpoint = resolve_xai_responses_endpoint(provider);
    let model = provider.resolved_model();
    let payload = fetch_json(
        client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .json(&json!({
                "model": model,
                "input": [{
                    "role": "user",
                    "content": invocation.query.as_str()
                }],
                "tools": [{
                    "type": "web_search"
                }]
            })),
    )
    .await?;

    let content = extract_xai_output_text(&payload).unwrap_or_else(|| String::from("No response"));
    let citations = extract_string_array(&payload, "citations")
        .into_iter()
        .map(|url| WebSearchCitation {
            title: String::new(),
            url,
        })
        .collect::<Vec<_>>();

    Ok(WebSearchProviderResponse::Narrative { content, citations })
}

/// Executes a Moonshot Kimi web-search request by replaying tool turns until a final answer is produced.
/// 通过回放工具轮次执行 Moonshot Kimi 联网搜索，直到产出最终答案。
pub(super) async fn execute_kimi_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
) -> Result<WebSearchProviderResponse, String> {
    let endpoint = resolve_chat_completions_endpoint(provider);
    let model = provider.resolved_model();
    let mut messages = vec![json!({
        "role": "user",
        "content": invocation.query.as_str()
    })];
    let mut citations = Vec::<String>::new();

    for _ in 0..3 {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "tools": [{
                "type": "builtin_function",
                "function": { "name": "$web_search" }
            }]
        });
        if matches!(model.as_str(), "kimi-k2.6" | "kimi-k2.5") {
            body["thinking"] = json!({ "type": "disabled" });
        }

        let payload = fetch_json(
            client
                .post(&endpoint)
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .json(&body),
        )
        .await?;

        for url in extract_kimi_citation_urls(&payload) {
            if !citations.contains(&url) {
                citations.push(url);
            }
        }

        let choice = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .ok_or_else(|| String::from("Kimi 未返回有效 choices。"))?;
        let finish_reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));
        let content =
            extract_kimi_message_text(&message).unwrap_or_else(|| String::from("No response"));
        let tool_calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if finish_reason != "tool_calls" || tool_calls.is_empty() {
            return Ok(WebSearchProviderResponse::Narrative {
                content,
                citations: citations
                    .into_iter()
                    .map(|url| WebSearchCitation {
                        title: String::new(),
                        url,
                    })
                    .collect(),
            });
        }

        messages.push(json!({
            "role": "assistant",
            "content": message.get("content").and_then(Value::as_str).unwrap_or_default(),
            "reasoning_content": message.get("reasoning_content").and_then(Value::as_str).unwrap_or_default(),
            "tool_calls": tool_calls
        }));

        let mut pushed_tool_result = false;
        for tool_call in tool_calls {
            let tool_call_id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let tool_name = tool_call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let tool_content = tool_call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let (Some(tool_call_id), Some(tool_name), Some(tool_content)) =
                (tool_call_id, tool_name, tool_content)
            else {
                continue;
            };
            pushed_tool_result = true;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "name": tool_name,
                "content": tool_content
            }));
        }

        if !pushed_tool_result {
            return Ok(WebSearchProviderResponse::Narrative {
                content,
                citations: citations
                    .into_iter()
                    .map(|url| WebSearchCitation {
                        title: String::new(),
                        url,
                    })
                    .collect(),
            });
        }
    }

    Ok(WebSearchProviderResponse::Narrative {
        content: String::from("Search completed but no final answer was produced."),
        citations: citations
            .into_iter()
            .map(|url| WebSearchCitation {
                title: String::new(),
                url,
            })
            .collect(),
    })
}

/// Executes one MiniMax Search request backed by the Token Plan key and credits system.
/// 执行由 Token Plan Key 与 Credits 体系承载的 MiniMax 搜索请求。
pub(super) async fn execute_minimax_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let endpoint = resolve_minimax_endpoint(provider)?;
    let payload = fetch_json(
        client
            .post(endpoint)
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&json!({ "q": invocation.query.as_str() })),
    )
    .await?;

    if let Some(status_code) = payload
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        && status_code != 0
    {
        let message = payload
            .pointer("/base_resp/status_msg")
            .and_then(Value::as_str)
            .unwrap_or("MiniMax Search 返回错误");
        return Err(message.to_string());
    }

    let results = payload
        .get("organic")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(max_results)
                .filter_map(|item| {
                    let title = first_string(item, &["title"])?;
                    // MiniMax Search organic results use link in current responses and url in compatible search-shaped payloads.
                    // MiniMax Search organic 结果当前使用 link，兼容搜索形态载荷中也可能使用 url。
                    let url = first_string(item, &["link", "url"])?;
                    // MiniMax Search exposes result text as snippet and some compatible result shapes expose description.
                    // MiniMax Search 使用 snippet 暴露结果文本，部分兼容结果形态使用 description。
                    let snippet =
                        first_string(item, &["snippet", "description"]).unwrap_or_default();
                    Some(WebSearchResult {
                        title,
                        url,
                        snippet,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(WebSearchProviderResponse::ResultList(results))
}

/// Executes an Ollama web-search request with local-first endpoint probing.
/// 以本地优先端点探测方式执行 Ollama 联网搜索请求。
pub(super) async fn execute_ollama_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: Option<&str>,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let attempts = build_ollama_attempts(provider, api_key);
    let request_body = json!({
        "query": invocation.query.as_str(),
        "max_results": max_results
    });
    let mut failures = Vec::new();

    for (endpoint, endpoint_api_key) in attempts {
        let mut builder = client
            .post(&endpoint)
            .header(CONTENT_TYPE, "application/json")
            .json(&request_body);
        if let Some(key) = endpoint_api_key.as_deref() {
            builder = builder.header(AUTHORIZATION, format!("Bearer {key}"));
        }

        let (status, text) = send_request(builder).await?;
        if status == reqwest::StatusCode::NOT_FOUND {
            failures.push(format!("{endpoint}：HTTP 状态 404"));
            continue;
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            failures.push(format!(
                "{endpoint}：认证失败，请先执行 `ollama signin` 或检查 API Key。"
            ));
            continue;
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            failures.push(format!("{endpoint}：当前 Ollama 宿主未启用联网搜索。"));
            continue;
        }
        if !status.is_success() {
            failures.push(format!(
                "{endpoint}：HTTP 状态 {status}：{}",
                truncate_error_text(&text, 240)
            ));
            continue;
        }

        let payload: Value = serde_json::from_str(&text)
            .map_err(|error| format!("解析 Ollama JSON 响应失败：{error}"))?;
        let results = payload
            .get("results")
            .and_then(Value::as_array)
            .map(|items| parse_structured_results_array(items))
            .unwrap_or_default();
        return Ok(WebSearchProviderResponse::ResultList(results));
    }

    Err(format!(
        "Ollama 搜索端点均不可用：{}。",
        failures.join("；")
    ))
}

/// Executes a Perplexity structured search request or chat-completions fallback according to the configured endpoint.
/// 根据配置端点执行 Perplexity 结构化搜索请求或 chat-completions 回退请求。
pub(super) async fn execute_perplexity_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    api_key: &str,
    max_results: usize,
) -> Result<WebSearchProviderResponse, String> {
    let base_url = provider.resolved_base_url();
    if is_probably_perplexity_search_endpoint(&base_url) {
        let payload = fetch_json(
            client
                .post(base_url)
                .header(CONTENT_TYPE, "application/json")
                .header(ACCEPT, "application/json")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("HTTP-Referer", "https://openvulcan.local")
                .header("X-Title", "Vulcan Code Web Search")
                .json(&json!({
                    "query": invocation.query.as_str(),
                    "max_results": max_results
                })),
        )
        .await?;

        return Ok(WebSearchProviderResponse::ResultList(
            payload
                .get("results")
                .and_then(Value::as_array)
                .map(|items| parse_structured_results_array(items))
                .unwrap_or_default(),
        ));
    }

    let endpoint = resolve_chat_completions_endpoint(provider);
    let model = resolve_perplexity_request_model(provider);
    let payload = fetch_json(
        client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .header("HTTP-Referer", "https://openvulcan.local")
            .header("X-Title", "Vulcan Code Web Search")
            .json(&json!({
                "model": model,
                "messages": [{
                    "role": "user",
                    "content": invocation.query.as_str()
                }]
            })),
    )
    .await?;

    let content = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| String::from("No response"));
    let citations = extract_perplexity_citations(&payload);

    Ok(WebSearchProviderResponse::Narrative { content, citations })
}
