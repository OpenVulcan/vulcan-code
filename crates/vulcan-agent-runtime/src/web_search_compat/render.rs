//! Markdown rendering helpers for web-search results.
//! 网页搜索结果的 Markdown 渲染辅助能力。

use crate::provider_config::PersistedWebSearchProviderEntry;

use super::types::{WebSearchCitation, WebSearchInvocation, WebSearchResult};

/// Renders normalized search results into compact model-visible Markdown.
/// 将规范化搜索结果渲染为紧凑的模型可见 Markdown。
pub(super) fn render_web_search_results(
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
pub(super) fn render_web_search_narrative(
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
