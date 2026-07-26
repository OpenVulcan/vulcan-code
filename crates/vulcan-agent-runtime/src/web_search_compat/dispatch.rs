//! Provider dispatch entrypoint for configured web search.
//! 配置化网页搜索的供应商调度入口。

use std::time::Duration;

use serde_json::Value;

use crate::provider_config::{PersistedAiToolConfiguration, PersistedWebSearchProviderEntry};

use super::{
    invocation::parse_web_search_invocation,
    providers::execute_single_provider_search,
    render::{render_web_search_narrative, render_web_search_results},
    types::WebSearchProviderResponse,
};

/// Executes configured web search against the one provider frozen by the host configuration.
/// 使用宿主配置冻结的唯一供应商执行配置化网页搜索。
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
            "The host-compatible `web_search` tool has no bound provider. Bind exactly one provider before registering the tool.",
        ));
    }
    if providers.len() != 1 {
        return Err(format!(
            "The host-compatible `web_search` tool has {} usable providers configured. Bind exactly one provider; automatic routing and provider fallback are not supported.",
            providers.len()
        ));
    }
    let provider = &providers[0];

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Failed to build the web_search HTTP client: {error}"))?;
    match execute_single_provider_search(&client, provider, &invocation).await {
        Ok(WebSearchProviderResponse::ResultList(results)) => Ok(Value::String(
            render_web_search_results(provider, &invocation, &results),
        )),
        Ok(WebSearchProviderResponse::Narrative { content, citations }) => Ok(Value::String(
            render_web_search_narrative(provider, &invocation, &content, &citations),
        )),
        Err(error) => Err(format!(
            "The bound `web_search` provider `{}` failed: {error}",
            provider.qualified_target()
        )),
    }
}
