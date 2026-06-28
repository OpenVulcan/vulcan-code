//! Provider dispatch entrypoint for configured web search.
//! 配置化网页搜索的供应商调度入口。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::Value;

use crate::provider_config::{
    PersistedAiToolConfiguration, PersistedWebSearchProviderEntry, WebSearchProviderDispatchMode,
};

use super::{
    invocation::parse_web_search_invocation,
    providers::execute_single_provider_search,
    render::{render_web_search_narrative, render_web_search_results},
    types::WebSearchProviderResponse,
};

/// Shared rotating cursor used by balanced provider dispatch.
/// 均衡调度模式使用的共享轮转游标。
static WEB_SEARCH_BALANCED_CURSOR: AtomicUsize = AtomicUsize::new(0);

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
pub(super) fn resolve_provider_attempt_order_indices(
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
pub(super) fn build_rotated_indices(provider_count: usize, start_index: usize) -> Vec<usize> {
    if provider_count == 0 {
        return Vec::new();
    }

    (0..provider_count)
        .map(|offset| (start_index + offset) % provider_count)
        .collect()
}
