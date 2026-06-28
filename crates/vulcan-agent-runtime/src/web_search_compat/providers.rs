//! Provider selection for configured web-search requests.
//! 配置化网页搜索请求的供应商选择逻辑。

use crate::provider_config::{PersistedWebSearchProviderEntry, WebSearchProviderKind};

use super::{
    http::{resolve_optional_secret_value, resolve_required_secret_value},
    llm_providers::{
        execute_gemini_search, execute_grok_search, execute_kimi_search, execute_minimax_search,
        execute_ollama_search, execute_perplexity_search,
    },
    structured_providers::{
        execute_brave_search, execute_duckduckgo_search, execute_exa_search,
        execute_firecrawl_search, execute_searxng_search, execute_serpapi_search,
        execute_tavily_search,
    },
    types::{WebSearchInvocation, WebSearchProviderResponse},
};

/// Executes one provider-specific search request.
/// 执行单个供应商专属搜索请求。
pub(super) async fn execute_single_provider_search(
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
