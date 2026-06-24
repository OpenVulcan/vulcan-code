//! Host-side web-search compatibility tool backed by user-configured third-party providers.
//! 由用户配置的第三方供应商驱动的宿主侧网页搜索兼容工具。

use crate::provider_config::{
    PersistedAiToolConfiguration, PersistedWebSearchProviderEntry, WebSearchProviderDispatchMode,
    WebSearchProviderKind,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Shared rotating cursor used by balanced provider dispatch.
/// 均衡调度模式使用的共享轮转游标。
static WEB_SEARCH_BALANCED_CURSOR: AtomicUsize = AtomicUsize::new(0);

/// Parsed invocation arguments accepted by the semantic `web_search` tool.
/// 语义 `web_search` 工具接受的已解析调用参数。
#[derive(Debug, Clone)]
struct WebSearchInvocation {
    /// Optional semantic target selector such as `auto` or `instance/target`.
    /// 可选的语义目标选择器，例如 `auto` 或 `实例/目标`。
    target: Option<String>,
    /// Search query text.
    /// 搜索查询文本。
    query: String,
    /// Optional caller-requested result count.
    /// 调用方请求的可选结果数量。
    max_results: Option<usize>,
}

/// Normalized search result returned by structured providers.
/// 由结构化供应商返回的规范化搜索结果。
#[derive(Debug, Clone)]
struct WebSearchResult {
    /// Result title.
    /// 结果标题。
    title: String,
    /// Result URL.
    /// 结果链接。
    url: String,
    /// Result snippet or summary.
    /// 结果摘要或片段。
    snippet: String,
}

/// Normalized citation returned by narrative providers.
/// 由叙述型供应商返回的规范化引用。
#[derive(Debug, Clone)]
struct WebSearchCitation {
    /// Citation URL.
    /// 引用链接。
    url: String,
    /// Optional citation title.
    /// 可选引用标题。
    title: String,
}

/// Provider response normalized into either a structured result list or one narrative answer.
/// 将供应商响应规范化为结构化结果列表或单段叙述答案。
#[derive(Debug, Clone)]
enum WebSearchProviderResponse {
    /// Structured result list.
    /// 结构化结果列表。
    ResultList(Vec<WebSearchResult>),
    /// Narrative answer with optional citations.
    /// 携带可选引用的叙述型答案。
    Narrative {
        /// Human-readable answer content.
        /// 面向人类可读的答案正文。
        content: String,
        /// Structured citations collected from the provider response.
        /// 从供应商响应中收集到的结构化引用。
        citations: Vec<WebSearchCitation>,
    },
}

/// Executes configured web search by trying usable providers in configured dispatch order.
/// 按配置化调度顺序尝试可用供应商来执行配置化网页搜索。
pub async fn execute_configured_web_search(
    args: Value,
    configuration: PersistedAiToolConfiguration,
) -> Result<Value, String> {
    let invocation = parse_web_search_invocation(args)?;
    let providers = configuration
        .projected_web_search_providers()
        .into_iter()
        .filter(PersistedWebSearchProviderEntry::is_usable)
        .collect::<Vec<_>>();

    if providers.is_empty() {
        return Err(String::from(
            "web_search is routed through the host-compatible path, but no usable third-party search provider is configured.",
        ));
    }

    let available_targets = providers
        .iter()
        .map(PersistedWebSearchProviderEntry::qualified_target)
        .collect::<Vec<_>>();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Failed to build the web_search HTTP client: {error}"))?;
    let mut failures = Vec::new();

    if let Some(explicit_target) = invocation
        .target
        .as_deref()
        .map(str::trim)
        .filter(|target| !target.is_empty() && !target.eq_ignore_ascii_case("auto"))
    {
        let provider = providers
            .iter()
            .find(|provider| provider.matches_target(explicit_target))
            .ok_or_else(|| {
                format!(
                    "The requested web_search target `{explicit_target}` does not exist or is not available. Available targets: {}.",
                    available_targets.join(", ")
                )
            })?;

        return match execute_single_provider_search(&client, provider, &invocation).await {
            Ok(WebSearchProviderResponse::ResultList(results)) => Ok(Value::String(
                render_web_search_results(provider, &invocation, &results),
            )),
            Ok(WebSearchProviderResponse::Narrative { content, citations }) => Ok(Value::String(
                render_web_search_narrative(provider, &invocation, &content, &citations),
            )),
            Err(error) => Err(format!(
                "The requested web_search target `{}` failed: {error}",
                provider.qualified_target()
            )),
        };
    }

    for provider_index in resolve_provider_attempt_order_indices(
        providers.len(),
        configuration.web_search.dispatch_mode,
    ) {
        let provider = &providers[provider_index];
        match execute_single_provider_search(&client, provider, &invocation).await {
            Ok(WebSearchProviderResponse::ResultList(results)) => {
                return Ok(Value::String(render_web_search_results(
                    provider,
                    &invocation,
                    &results,
                )));
            }
            Ok(WebSearchProviderResponse::Narrative { content, citations }) => {
                return Ok(Value::String(render_web_search_narrative(
                    provider,
                    &invocation,
                    &content,
                    &citations,
                )));
            }
            Err(error) => failures.push(format!("{}: {error}", provider.name)),
        }
    }

    Err(format!(
        "All configured web_search providers failed: {}.",
        failures.join("; ")
    ))
}

/// Resolves the provider attempt order for one run according to the configured dispatch mode.
/// 根据配置的调度模式解析单次运行使用的供应商尝试顺序。
fn resolve_provider_attempt_order_indices(
    provider_count: usize,
    dispatch_mode: WebSearchProviderDispatchMode,
) -> Vec<usize> {
    if provider_count == 0 {
        return Vec::new();
    }

    let start_index = match dispatch_mode {
        WebSearchProviderDispatchMode::PriorityFailover => 0usize,
        WebSearchProviderDispatchMode::Balanced => {
            WEB_SEARCH_BALANCED_CURSOR.fetch_add(1, Ordering::Relaxed) % provider_count
        }
    };
    build_rotated_indices(provider_count, start_index)
}

/// Builds one rotated index list used by balanced provider dispatch.
/// 构建均衡供应商调度使用的轮转索引列表。
fn build_rotated_indices(provider_count: usize, start_index: usize) -> Vec<usize> {
    if provider_count == 0 {
        return Vec::new();
    }

    (0..provider_count)
        .map(|offset| (start_index + offset) % provider_count)
        .collect()
}

/// Parses model-provided JSON arguments into a stable search invocation.
/// 将模型提供的 JSON 参数解析为稳定的搜索调用。
fn parse_web_search_invocation(args: Value) -> Result<WebSearchInvocation, String> {
    let object = args
        .as_object()
        .ok_or_else(|| String::from("web_search 参数必须是 JSON 对象。"))?;
    let target = object
        .get("target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
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

    Ok(WebSearchInvocation {
        target,
        query,
        max_results,
    })
}

/// Executes one provider-specific search request.
/// 执行单个供应商专属搜索请求。
async fn execute_single_provider_search(
    client: &reqwest::Client,
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
) -> Result<WebSearchProviderResponse, String> {
    let max_results = invocation
        .max_results
        .unwrap_or(provider.max_results)
        .clamp(1, provider.kind.max_results_limit());

    match provider.kind {
        WebSearchProviderKind::DuckDuckGo => {
            execute_duckduckgo_search(client, provider, invocation, max_results).await
        }
        WebSearchProviderKind::Brave => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_brave_search(client, provider, invocation, &api_key, max_results).await
        }
        WebSearchProviderKind::Tavily => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_tavily_search(client, provider, invocation, &api_key, max_results).await
        }
        WebSearchProviderKind::Exa => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_exa_search(client, provider, invocation, &api_key, max_results).await
        }
        WebSearchProviderKind::Firecrawl => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_firecrawl_search(client, provider, invocation, &api_key, max_results).await
        }
        WebSearchProviderKind::Gemini => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_gemini_search(client, provider, invocation, &api_key).await
        }
        WebSearchProviderKind::Grok => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_grok_search(client, provider, invocation, &api_key).await
        }
        WebSearchProviderKind::Kimi => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_kimi_search(client, provider, invocation, &api_key).await
        }
        WebSearchProviderKind::MiniMax => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_minimax_search(client, provider, invocation, &api_key, max_results).await
        }
        WebSearchProviderKind::Ollama => {
            let api_key = resolve_optional_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_ollama_search(
                client,
                provider,
                invocation,
                api_key.as_deref(),
                max_results,
            )
            .await
        }
        WebSearchProviderKind::Perplexity => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_perplexity_search(client, provider, invocation, &api_key, max_results).await
        }
        WebSearchProviderKind::Searxng => {
            execute_searxng_search(client, provider, invocation, max_results).await
        }
        WebSearchProviderKind::SerpApi => {
            let api_key = resolve_required_secret_value(
                &provider.api_key,
                &format!("{} API Key", provider.name),
            )?;
            execute_serpapi_search(client, provider, invocation, &api_key, max_results).await
        }
    }
}

/// Executes a DuckDuckGo HTML search request.
/// 执行 DuckDuckGo HTML 搜索请求。
async fn execute_duckduckgo_search(
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
async fn execute_brave_search(
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
async fn execute_tavily_search(
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
async fn execute_exa_search(
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
async fn execute_firecrawl_search(
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

    let results = payload
        .get("results")
        .or_else(|| payload.pointer("/data/results"))
        .or_else(|| payload.pointer("/data/data"))
        .and_then(Value::as_array)
        .map(|items| parse_structured_results_array(items))
        .unwrap_or_default();

    Ok(WebSearchProviderResponse::ResultList(results))
}

/// Executes a Gemini grounding search request.
/// 执行 Gemini grounding 搜索请求。
async fn execute_gemini_search(
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
async fn execute_grok_search(
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
async fn execute_kimi_search(
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
async fn execute_minimax_search(
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
    {
        if status_code != 0 {
            let message = payload
                .pointer("/base_resp/status_msg")
                .and_then(Value::as_str)
                .unwrap_or("MiniMax Search 返回错误");
            return Err(message.to_string());
        }
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
                    let url = first_string(item, &["link", "url"])?;
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
async fn execute_ollama_search(
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
async fn execute_perplexity_search(
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

/// Executes a SearXNG search request.
/// 执行 SearXNG 搜索请求。
async fn execute_searxng_search(
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
async fn execute_serpapi_search(
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

/// Parses one common structured result array into normalized result entries.
/// 将通用结构化结果数组解析为规范化结果条目。
fn parse_structured_results_array(items: &[Value]) -> Vec<WebSearchResult> {
    items
        .iter()
        .filter_map(|item| {
            let title = first_string(item, &["title", "name"])
                .unwrap_or_else(|| String::from("未命名结果"));
            let url = first_string(item, &["url", "link", "href"])?;
            let snippet = first_string(item, &["snippet", "description", "content", "text"])
                .unwrap_or_default();
            Some(WebSearchResult {
                title,
                url,
                snippet,
            })
        })
        .collect()
}

/// Sends one HTTP request and returns the status code together with the response body text.
/// 发送单个 HTTP 请求，并返回状态码与响应正文文本。
async fn send_request(
    builder: reqwest::RequestBuilder,
) -> Result<(reqwest::StatusCode, String), String> {
    let response = builder
        .send()
        .await
        .map_err(|error| format!("HTTP 请求失败：{error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取 HTTP 响应失败：{error}"))?;
    Ok((status, text))
}

/// Sends one HTTP request and parses a successful JSON response.
/// 发送单个 HTTP 请求并解析成功 JSON 响应。
async fn fetch_json(builder: reqwest::RequestBuilder) -> Result<Value, String> {
    let (status, text) = send_request(builder).await?;
    if !status.is_success() {
        return Err(format!(
            "HTTP 状态 {status}：{}",
            truncate_error_text(&text, 240)
        ));
    }

    serde_json::from_str(&text).map_err(|error| format!("解析 JSON 响应失败：{error}"))
}

/// Sends one HTTP request and returns the successful response text.
/// 发送单个 HTTP 请求并返回成功响应文本。
async fn fetch_text(builder: reqwest::RequestBuilder) -> Result<String, String> {
    let (status, text) = send_request(builder).await?;
    if !status.is_success() {
        return Err(format!(
            "HTTP 状态 {status}：{}",
            truncate_error_text(&text, 240)
        ));
    }
    Ok(text)
}

/// Resolves a secret value that may reference a `{env:NAME}` environment placeholder.
/// 解析可能引用 `{env:NAME}` 环境变量占位符的密钥值。
fn resolve_required_secret_value(raw_value: &str, field_label: &str) -> Result<String, String> {
    resolve_optional_secret_value(raw_value, field_label)?
        .ok_or_else(|| format!("{field_label} is empty."))
}

/// Resolves an optional secret value that may reference a `{env:NAME}` environment placeholder.
/// 解析可能引用 `{env:NAME}` 环境变量占位符的可选密钥值。
fn resolve_optional_secret_value(
    raw_value: &str,
    field_label: &str,
) -> Result<Option<String>, String> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Some(env_name) = trimmed
        .strip_prefix("{env:")
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Ok(Some(trimmed.to_string()));
    };
    let env_name = env_name.trim();
    if env_name.is_empty() {
        return Err(format!(
            "{field_label} uses an invalid environment-variable placeholder format."
        ));
    }

    std::env::var(env_name)
        .map(|value| value.trim().to_string())
        .map_err(|_| {
            format!(
                "{field_label} references environment variable `{env_name}`, but it is not defined in the current process."
            )
        })
        .and_then(|value| {
            if value.is_empty() {
                Err(format!(
                    "{field_label} references environment variable `{env_name}`, but that variable is empty."
                ))
            } else {
                Ok(Some(value))
            }
        })
}

/// Returns the first non-empty string field from one JSON object.
/// 从单个 JSON 对象返回第一个非空字符串字段。
fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .next()
}

/// Returns the first non-empty string from a JSON array field.
/// 从 JSON 数组字段返回第一个非空字符串。
fn first_array_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_array).and_then(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
            .next()
    })
}

/// Extracts one string array from the given JSON field.
/// 从给定 JSON 字段中提取字符串数组。
fn extract_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Resolves the Gemini endpoint from the configured base URL and model.
/// 根据配置基址与模型解析 Gemini 端点。
fn resolve_gemini_endpoint(provider: &PersistedWebSearchProviderEntry) -> String {
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
fn resolve_xai_responses_endpoint(provider: &PersistedWebSearchProviderEntry) -> String {
    let base_url = provider.resolved_base_url();
    if base_url.ends_with("/responses") {
        return base_url;
    }
    format!("{}/responses", base_url.trim_end_matches('/'))
}

/// Resolves one chat-completions endpoint from the configured base URL.
/// 根据配置基址解析 chat-completions 端点。
fn resolve_chat_completions_endpoint(provider: &PersistedWebSearchProviderEntry) -> String {
    let base_url = provider.resolved_base_url();
    if base_url.ends_with("/chat/completions") {
        return base_url;
    }
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

/// Resolves the MiniMax Search endpoint by combining base URL overrides with region defaults while rejecting obvious regular text-API endpoints.
/// 在拒绝明显普通文本接口地址的前提下，结合基址覆盖与区域默认值解析 MiniMax Search 端点。
fn resolve_minimax_endpoint(provider: &PersistedWebSearchProviderEntry) -> Result<String, String> {
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
fn build_ollama_attempts(
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
fn is_probably_perplexity_search_endpoint(base_url: &str) -> bool {
    let normalized = base_url.trim().trim_end_matches('/').to_ascii_lowercase();
    normalized.ends_with("/search")
}

/// Resolves the request model for Perplexity direct chat endpoints.
/// 解析 Perplexity 直连 chat 端点使用的请求模型。
fn resolve_perplexity_request_model(provider: &PersistedWebSearchProviderEntry) -> String {
    let model = provider.resolved_model();
    if provider.resolved_base_url().contains("api.perplexity.ai")
        && model.starts_with("perplexity/")
    {
        model.trim_start_matches("perplexity/").to_string()
    } else {
        model
    }
}

/// Extracts Perplexity citations from either top-level fields or annotations.
/// 从顶层字段或注解块中提取 Perplexity 引用。
fn extract_perplexity_citations(payload: &Value) -> Vec<WebSearchCitation> {
    let top_level = extract_string_array(payload, "citations");
    if !top_level.is_empty() {
        return top_level
            .into_iter()
            .map(|url| WebSearchCitation {
                title: String::new(),
                url,
            })
            .collect();
    }

    payload
        .pointer("/choices/0/message/annotations")
        .and_then(Value::as_array)
        .map(|annotations| {
            annotations
                .iter()
                .filter_map(|annotation| {
                    if annotation.get("type").and_then(Value::as_str) != Some("url_citation") {
                        return None;
                    }
                    let url = annotation
                        .pointer("/url_citation/url")
                        .and_then(Value::as_str)
                        .or_else(|| annotation.get("url").and_then(Value::as_str))
                        .map(str::trim)
                        .filter(|text| !text.is_empty())?;
                    Some(WebSearchCitation {
                        title: String::new(),
                        url: url.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Builds the SearXNG `/search` endpoint from a configured base URL.
/// 从配置的基址构建 SearXNG `/search` 端点。
fn build_searxng_search_endpoint(
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

/// Extracts the first xAI output text block from a Responses payload.
/// 从 Responses 载荷中提取首个 xAI 输出文本块。
fn extract_xai_output_text(payload: &Value) -> Option<String> {
    if let Some(output) = payload.get("output").and_then(Value::as_array) {
        for item in output {
            if item.get("type").and_then(Value::as_str) == Some("message") {
                if let Some(content_items) = item.get("content").and_then(Value::as_array) {
                    for block in content_items {
                        if block.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    return Some(trimmed.to_string());
                                }
                            }
                        }
                    }
                }
            }
            if item.get("type").and_then(Value::as_str) == Some("output_text") {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    payload
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

/// Extracts the most useful visible Kimi message text.
/// 提取 Kimi 消息中最有用的可见文本。
fn extract_kimi_message_text(message: &Value) -> Option<String> {
    first_string(message, &["content", "reasoning_content"])
}

/// Extracts Kimi citation URLs from both top-level search results and tool-call arguments.
/// 从顶层搜索结果与工具调用参数中提取 Kimi 引用链接。
fn extract_kimi_citation_urls(payload: &Value) -> Vec<String> {
    let mut citations = payload
        .get("search_results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|entry| first_string(entry, &["url"]))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let tool_calls = payload
        .pointer("/choices/0/message/tool_calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for tool_call in tool_calls {
        let Some(raw_arguments) = tool_call
            .pointer("/function/arguments")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Ok(arguments) = serde_json::from_str::<Value>(raw_arguments) else {
            continue;
        };
        if let Some(url) = first_string(&arguments, &["url"]) {
            citations.push(url);
        }
        if let Some(search_results) = arguments.get("search_results").and_then(Value::as_array) {
            for search_result in search_results {
                if let Some(url) = first_string(search_result, &["url"]) {
                    citations.push(url);
                }
            }
        }
    }

    citations.dedup();
    citations
}

/// Decodes common HTML entities emitted by HTML-based search providers.
/// 解码 HTML 类搜索供应商返回的常见 HTML 实体。
fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&nbsp;", " ")
        .replace("&ndash;", "-")
        .replace("&mdash;", "--")
        .replace("&hellip;", "...")
}

/// Strips HTML tags and squashes repeated whitespace.
/// 去除 HTML 标签并压缩重复空白字符。
fn strip_html(html: &str) -> String {
    let mut plain = String::with_capacity(html.len());
    let mut inside_tag = false;
    let mut previous_was_whitespace = false;

    for ch in html.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                if !previous_was_whitespace {
                    plain.push(' ');
                    previous_was_whitespace = true;
                }
            }
            _ if inside_tag => {}
            _ if ch.is_whitespace() => {
                if !previous_was_whitespace {
                    plain.push(' ');
                    previous_was_whitespace = true;
                }
            }
            _ => {
                plain.push(ch);
                previous_was_whitespace = false;
            }
        }
    }

    plain.trim().to_string()
}

/// Returns whether one DuckDuckGo HTML response is actually a bot challenge page.
/// 返回单个 DuckDuckGo HTML 响应是否实际是机器人验证页面。
fn is_duckduckgo_bot_challenge(html: &str) -> bool {
    if html.contains("result__a") {
        return false;
    }
    let lower = html.to_ascii_lowercase();
    lower.contains("g-recaptcha")
        || lower.contains("are you a human")
        || lower.contains("id=\"challenge-form\"")
        || lower.contains("name=\"challenge\"")
}

/// Reads the first `href` attribute value from one HTML tag attribute segment.
/// 从单个 HTML 标签属性片段中读取首个 `href` 属性值。
fn read_href_attribute(tag_attributes: &str) -> Option<String> {
    let marker = "href=\"";
    let start = tag_attributes.find(marker)? + marker.len();
    let rest = &tag_attributes[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Decodes one DuckDuckGo redirect URL into the original result URL when possible.
/// 在可能的情况下将一条 DuckDuckGo 重定向链接解码为原始结果链接。
fn decode_duckduckgo_url(raw_url: &str) -> String {
    let normalized = if raw_url.starts_with("//") {
        format!("https:{raw_url}")
    } else {
        raw_url.to_string()
    };
    if let Ok(parsed) = reqwest::Url::parse(&normalized) {
        if let Some(uddg) = parsed
            .query_pairs()
            .find_map(|(key, value)| (key == "uddg").then(|| value.to_string()))
        {
            return uddg;
        }
    }
    normalized
}

/// Parses DuckDuckGo HTML search results into normalized search entries.
/// 将 DuckDuckGo HTML 搜索结果解析为规范化搜索条目。
fn parse_duckduckgo_html(html: &str) -> Vec<WebSearchResult> {
    let mut results = Vec::new();
    let mut search_offset = 0usize;

    while let Some(relative_start) = html[search_offset..].find("result__a") {
        let class_position = search_offset + relative_start;
        let Some(tag_start) = html[..class_position].rfind("<a") else {
            search_offset = class_position + "result__a".len();
            continue;
        };
        let Some(tag_end_relative) = html[class_position..].find("</a>") else {
            break;
        };
        let tag_end = class_position + tag_end_relative + "</a>".len();
        let anchor_html = &html[tag_start..tag_end];
        let Some(anchor_close) = anchor_html.find('>') else {
            search_offset = tag_end;
            continue;
        };
        let raw_attributes = &anchor_html[..anchor_close];
        let raw_title = &anchor_html[anchor_close + 1..anchor_html.len() - "</a>".len()];
        let raw_url = read_href_attribute(raw_attributes).unwrap_or_default();

        let trailing_html = &html[tag_end..];
        let next_result_index = trailing_html
            .find("result__a")
            .unwrap_or(trailing_html.len());
        let scoped_trailing_html = &trailing_html[..next_result_index];
        let snippet = extract_duckduckgo_snippet(scoped_trailing_html).unwrap_or_default();

        let title = strip_html(&decode_html_entities(raw_title));
        let url = decode_duckduckgo_url(&decode_html_entities(&raw_url));

        if !title.is_empty() && !url.is_empty() {
            results.push(WebSearchResult {
                title,
                url,
                snippet,
            });
        }

        search_offset = tag_end;
    }

    results
}

/// Extracts the first DuckDuckGo snippet block after one result anchor.
/// 提取单个结果锚点之后的首个 DuckDuckGo 摘要块。
fn extract_duckduckgo_snippet(trailing_html: &str) -> Option<String> {
    let class_marker = "result__snippet";
    let class_position = trailing_html.find(class_marker)?;
    let tag_start = trailing_html[..class_position].rfind("<a")?;
    let tag_end_relative = trailing_html[class_position..].find("</a>")?;
    let tag_end = class_position + tag_end_relative + "</a>".len();
    let snippet_anchor = &trailing_html[tag_start..tag_end];
    let anchor_close = snippet_anchor.find('>')?;
    let raw_snippet = &snippet_anchor[anchor_close + 1..snippet_anchor.len() - "</a>".len()];
    let snippet = strip_html(&decode_html_entities(raw_snippet));
    (!snippet.is_empty()).then_some(snippet)
}

/// Renders normalized search results into compact model-visible Markdown.
/// 将规范化搜索结果渲染为紧凑的模型可见 Markdown。
fn render_web_search_results(
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    results: &[WebSearchResult],
) -> String {
    let mut lines = vec![format!(
        "web_search completed via `{}`.\nQuery: {}",
        provider.name, invocation.query
    )];

    if results.is_empty() {
        lines.push(String::from("No usable search results were returned."));
        return lines.join("\n");
    }

    for (index, result) in results.iter().enumerate() {
        lines.push(format!(
            "{}. {}\n   URL: {}\n   Snippet: {}",
            index + 1,
            result.title,
            result.url,
            if result.snippet.trim().is_empty() {
                "No snippet"
            } else {
                result.snippet.trim()
            }
        ));
    }

    lines.join("\n\n")
}

/// Renders one narrative search answer into compact model-visible Markdown.
/// 将单段叙述型搜索答案渲染为紧凑的模型可见 Markdown。
fn render_web_search_narrative(
    provider: &PersistedWebSearchProviderEntry,
    invocation: &WebSearchInvocation,
    content: &str,
    citations: &[WebSearchCitation],
) -> String {
    let mut lines = vec![format!(
        "web_search completed via `{}`.\nQuery: {}",
        provider.name, invocation.query
    )];

    let trimmed_content = content.trim();
    if trimmed_content.is_empty() {
        lines.push(String::from("No usable narrative answer was returned."));
    } else {
        lines.push(trimmed_content.to_string());
    }

    if !citations.is_empty() {
        let citation_lines = citations
            .iter()
            .enumerate()
            .map(|(index, citation)| {
                let label = if citation.title.trim().is_empty() {
                    citation.url.as_str()
                } else {
                    citation.title.trim()
                };
                format!("{}. {} ({})", index + 1, label, citation.url)
            })
            .collect::<Vec<_>>();
        lines.push(format!("Sources:\n{}", citation_lines.join("\n")));
    }

    lines.join("\n\n")
}

/// Truncates provider error bodies before they are surfaced to the model or UI.
/// 在向模型或界面展示供应商错误正文前进行截断。
fn truncate_error_text(text: &str, max_chars: usize) -> String {
    let mut truncated = text.trim().chars().take(max_chars).collect::<String>();
    if text.trim().chars().count() > max_chars {
        truncated.push_str("...");
    }
    truncated
}

/// Builds the JSON schema used by the configured web-search fallback tool.
/// 构建配置化网页搜索回退工具使用的 JSON Schema。
pub fn configured_web_search_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(crate::provider_config::AiToolKind::WebSearch))
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "enum": target_enum,
                "default": "auto",
                "description": "Search target selector. Use `auto` to follow the configured default order, or choose one explicit instance/target pair."
            },
            "query": {
                "type": "string",
                "description": "Search query text."
            },
            "max_results": {
                "type": "integer",
                "minimum": 1,
                "maximum": 20,
                "description": "Optional maximum result count."
            }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    //! Host-side web-search compatibility tests.
    //! 宿主侧网页搜索兼容测试。

    use super::{
        WebSearchResult, build_rotated_indices, configured_web_search_parameters_schema,
        decode_duckduckgo_url, parse_duckduckgo_html, parse_web_search_invocation,
        render_web_search_results, resolve_minimax_endpoint,
    };
    use crate::provider_config::{PersistedWebSearchProviderEntry, WebSearchProviderKind};
    use serde_json::json;

    /// Parses the schema-declared web-search argument names.
    /// 解析 Schema 声明的网页搜索参数名称。
    #[test]
    fn parse_web_search_invocation_accepts_schema_argument_names() {
        let invocation = parse_web_search_invocation(json!({
            "target": "minimax/minimax",
            "query": "vulcan code",
            "max_results": 8
        }))
        .expect("arguments should parse");

        assert_eq!(invocation.target.as_deref(), Some("minimax/minimax"));
        assert_eq!(invocation.query, "vulcan code");
        assert_eq!(invocation.max_results, Some(8));
    }

    /// Rejects alias-only web-search arguments that are absent from the schema.
    /// 拒绝未出现在 Schema 中的网页搜索别名参数。
    #[test]
    fn parse_web_search_invocation_rejects_alias_only_query() {
        let error = parse_web_search_invocation(json!({
            "q": "vulcan code"
        }))
        .expect_err("alias-only query should be rejected");

        assert!(error.contains("query"));
    }

    /// Exposes a strict schema with the stable `query` field required.
    /// 暴露一份要求稳定 `query` 字段的严格参数 Schema。
    #[test]
    fn configured_web_search_schema_requires_query() {
        let configuration = crate::provider_config::PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("duckduckgo"),
                vendor_id: String::from("duckduckgo"),
                name: String::from("DuckDuckGo"),
                enabled: true,
                api_key: String::new(),
                connection_mode: None,
                base_url: String::new(),
                region: String::new(),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        crate::provider_config::AiToolKind::WebSearch,
                        "duckduckgo",
                        "duckduckgo",
                    ),
                ],
            }],
            web_search: crate::provider_config::PersistedWebSearchConfiguration {
                target_order: vec![String::from("duckduckgo/duckduckgo")],
                ..crate::provider_config::PersistedWebSearchConfiguration::default()
            },
            ..crate::provider_config::PersistedAiToolConfiguration::default()
        };
        let schema = configured_web_search_parameters_schema(&configuration);

        assert_eq!(schema["required"][0], "query");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["target"]["enum"][0], "auto");
        assert_eq!(
            schema["properties"]["target"]["enum"][1],
            "duckduckgo/duckduckgo"
        );
    }

    /// Rotates provider indices from the requested start position without losing any candidate.
    /// 从指定起点轮转供应商索引，同时不会丢失任何候选条目。
    #[test]
    fn build_rotated_indices_rotates_without_duplicates() {
        assert_eq!(build_rotated_indices(4, 0), vec![0, 1, 2, 3]);
        assert_eq!(build_rotated_indices(4, 2), vec![2, 3, 0, 1]);
    }

    /// Decodes DuckDuckGo redirect links back to the original target URL.
    /// 将 DuckDuckGo 重定向链接解码回原始目标链接。
    #[test]
    fn decode_duckduckgo_url_restores_uddg_target() {
        let decoded = decode_duckduckgo_url(
            "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fdocs",
        );
        assert_eq!(decoded, "https://example.com/docs");
    }

    /// Parses one minimal DuckDuckGo HTML payload into normalized search results.
    /// 将最小 DuckDuckGo HTML 载荷解析为规范化搜索结果。
    #[test]
    fn parse_duckduckgo_html_extracts_results() {
        let html = r#"
        <div>
          <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com">
            Example &amp; Title
          </a>
          <a class="result__snippet">A short &lt;b&gt;snippet&lt;/b&gt;.</a>
        </div>
        "#;

        let results = parse_duckduckgo_html(html);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example & Title");
        assert_eq!(results[0].url, "https://example.com");
        assert_eq!(results[0].snippet, "A short snippet .");
    }

    /// Renders structured result lists with the provider header and numbered entries.
    /// 使用供应商头部与编号条目渲染结构化结果列表。
    #[test]
    fn render_web_search_results_includes_provider_name_and_items() {
        let provider =
            PersistedWebSearchProviderEntry::new("duckduckgo", WebSearchProviderKind::DuckDuckGo);
        let markdown = render_web_search_results(
            &provider,
            &super::WebSearchInvocation {
                target: None,
                query: String::from("vulcan"),
                max_results: Some(5),
            },
            &[WebSearchResult {
                title: String::from("Example"),
                url: String::from("https://example.com"),
                snippet: String::from("Example snippet"),
            }],
        );

        assert!(markdown.contains("DuckDuckGo Search"));
        assert!(markdown.contains("https://example.com"));
    }

    /// Auto-completes the MiniMax Search path when MiniMax only receives a bare host or `/v1` base.
    /// 当 MiniMax 只收到裸主机或 `/v1` 基址时，会自动补全 MiniMax Search 路径。
    #[test]
    fn resolve_minimax_endpoint_completes_token_plan_path_for_bare_base_url() {
        let mut provider =
            PersistedWebSearchProviderEntry::new("minimax", WebSearchProviderKind::MiniMax);
        provider.base_url = String::from("https://api.minimax.io");

        let endpoint = resolve_minimax_endpoint(&provider).expect("endpoint should resolve");

        assert_eq!(endpoint, "https://api.minimax.io/v1/coding_plan/search");
    }

    /// Rejects obvious regular MiniMax text endpoints so users do not mistake them for the dedicated MiniMax Search endpoint.
    /// 拒绝明显的 MiniMax 普通文本端点，避免用户把它们误认为专用的 MiniMax Search 端点。
    #[test]
    fn resolve_minimax_endpoint_rejects_regular_text_api_paths() {
        let mut provider =
            PersistedWebSearchProviderEntry::new("minimax", WebSearchProviderKind::MiniMax);
        provider.base_url = String::from("https://api.minimaxi.com/v1/text/chatcompletion_v2");

        let error = resolve_minimax_endpoint(&provider).expect_err("regular text api should fail");

        assert!(error.contains("MiniMax Search 接口"));
    }
}
