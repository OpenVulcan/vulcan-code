//! Host-side web-search compatibility tool backed by user-configured third-party providers.
//! 由用户配置的第三方供应商驱动的宿主侧网页搜索兼容工具。

mod dispatch;
mod endpoints;
mod helpers;
mod http;
mod invocation;
mod llm_providers;
mod parsing;
mod providers;
mod render;
mod schema;
mod structured_providers;
mod types;

pub use dispatch::execute_configured_web_search;
pub use schema::configured_web_search_parameters_schema;

#[cfg(test)]
mod tests {
    //! Host-side web-search compatibility tests.
    //! 宿主侧网页搜索兼容测试。

    use super::{
        endpoints::resolve_minimax_endpoint,
        invocation::parse_web_search_invocation,
        parsing::{decode_duckduckgo_url, parse_duckduckgo_html},
        render::render_web_search_results,
        schema::configured_web_search_parameters_schema,
        types::{WebSearchInvocation, WebSearchResult},
    };
    use crate::provider_config::{PersistedWebSearchProviderEntry, WebSearchProviderKind};
    use serde_json::json;

    /// Parses the schema-declared web-search argument names.
    /// 解析 Schema 声明的网页搜索参数名称。
    #[test]
    fn parse_web_search_invocation_accepts_schema_argument_names() {
        let invocation = parse_web_search_invocation(json!({
            "query": "vulcan code",
            "max_results": 8
        }))
        .expect("arguments should parse");

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
                selected_target: Some(String::from("duckduckgo/duckduckgo")),
                ..crate::provider_config::PersistedWebSearchConfiguration::default()
            },
            ..crate::provider_config::PersistedAiToolConfiguration::default()
        };
        let schema = configured_web_search_parameters_schema(&configuration);

        assert_eq!(schema["required"][0], "query");
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"].get("target").is_none());
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
            &WebSearchInvocation {
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
