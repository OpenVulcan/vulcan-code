//! Reading projections for conversation provider tools, separate from provider wire contracts.
//! 会话供应商工具的阅读投影，与供应商协议契约分离。

use vulcan_provider_adapters::{
    ProviderToolCompositeOutput, ProviderToolExtractOutput, ProviderToolSearchOutput,
};
use vulcan_tools_manager::result_markdown::{inline_code, source_link};

pub(super) mod extension_parameters;

/// Wraps a provider answer as one Markdown result, retaining its content exactly once.
/// 将供应商回答包装为单份 Markdown 结果，正文只保留一次。
/// The input is the normalized answer; the return value is shared by the model and WebUI.
/// 输入是归一化回答；返回值由模型与前端共用。
pub(super) fn text(answer: &str) -> String {
    if answer.is_empty() {
        String::from("# Result\n\nNo content returned.")
    } else {
        format!("# Result\n\n{answer}")
    }
}

/// Renders ordered search evidence with clickable sources and no duplicated provider envelope.
/// 将有序检索证据渲染为可点击来源，不重复供应商封装。
/// Accepts normalized search output and returns all supplied snippets with their source identities.
/// 接受归一化检索结果，返回全部摘要及其来源身份。
pub(super) fn search(search: &ProviderToolSearchOutput) -> String {
    // The actual query disambiguates multiple searches inside a composite result.
    // 实际查询用于区分复合结果中的多个检索。
    let mut output = format!("# Search: {}\n", inline_code(&search.query));
    if search.results.is_empty() {
        output.push_str("\nNo results.\n");
    }
    for hit in &search.results {
        output.push_str(&format!(
            "\n{}. {}\n\n{}\n",
            hit.rank,
            source_link(&hit.title, &hit.url),
            hit.snippet
        ));
    }
    output
}

/// Renders extracted pages and failures without copying their bodies into host metadata.
/// 渲染提取网页及失败，不将正文复制到宿主元数据。
/// Accepts successful pages and per-source failures; returns a complete readable result.
/// 接受成功网页及逐来源失败；返回完整可读结果。
pub(super) fn extract(extract: &ProviderToolExtractOutput) -> String {
    // A source boundary keeps successful content separate from failure and continuation facts.
    // 来源边界将成功正文与失败及继续操作信息区分。
    let mut output = String::from("# Extracted pages\n");
    if extract.results.is_empty() {
        output.push_str("\nNo pages extracted.\n");
    }
    for page in &extract.results {
        output.push_str(&format!(
            "\n## {}\n\n{}\n",
            source_link(&page.url, &page.url),
            page.content
        ));
        // Provider images may contain evidence absent from the text; favicons are presentation-only.
        // 供应商图片可能含正文没有的证据；站点图标仅用于展示。
        if !page.images.is_empty() {
            output.push_str("\nImages:\n");
            for image in &page.images {
                output.push_str(&format!("- {}\n", source_link(image, image)));
            }
        }
    }
    if !extract.failures.is_empty() {
        output.push_str("\n## Failed sources\n");
        for failure in &extract.failures {
            output.push_str(&format!(
                "\n- {}: {}\n",
                source_link(&failure.url, &failure.url),
                failure.error
            ));
        }
    }
    output
}

/// Combines only reading-oriented composite members in their declared collection order.
/// 仅将复合结果中的阅读成员按声明集合顺序组合。
/// The caller excludes resource, vector and rerank members; the result contains each text once.
/// 调用方排除资源、向量和重排成员；结果包含每份文本一次。
pub(super) fn reading_composite(composite: &ProviderToolCompositeOutput) -> String {
    // Each constituent formatter retains its source boundary without a second serialized object.
    // 每个成员格式化函数保留来源边界，不再产生第二个序列化对象。
    let mut sections = composite
        .text
        .iter()
        .map(|answer| text(answer))
        .collect::<Vec<_>>();
    sections.extend(composite.searches.iter().map(search));
    sections.extend(composite.extracts.iter().map(extract));
    if sections.is_empty() {
        text("")
    } else {
        sections.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::tool_runtime_bridge::assert_result_payload_cost;
    use crate::provider_tool_bridge::render_provider_tool_result;
    use vulcan_provider_adapters::{
        ProviderToolExecutionResult, ProviderToolExtractFailure, ProviderToolExtractResult,
        ProviderToolNormalizedOutput, ProviderToolSearchResult,
    };
    use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};

    /// Measures actual provider projections against their former duplicated normalized envelopes.
    /// 对照原先重复的归一化封套，计量实际供应商投影。
    #[tokio::test]
    async fn provider_payload_cost_samples() {
        let search = ProviderToolSearchOutput {
            query: "fixture query".into(),
            results: vec![ProviderToolSearchResult {
                rank: 1,
                title: "Reference".into(),
                url: "https://example.test/read".into(),
                source_domain: "example.test".into(),
                snippet: format!("UNIQUE_SEARCH_BODY {}", "source evidence ".repeat(64)),
            }],
        };
        let extract = ProviderToolExtractOutput {
            results: vec![ProviderToolExtractResult {
                url: "https://example.test/page".into(),
                content: format!("UNIQUE_EXTRACT_BODY {}", "readable paragraph\n".repeat(256)),
                images: vec![],
                favicon: Some("https://example.test/favicon.ico".into()),
            }],
            failures: vec![],
        };
        for (tool, marker, normalized, payload) in [
            (
                "search",
                "UNIQUE_SEARCH_BODY",
                ProviderToolNormalizedOutput::Search(search.clone()),
                serde_json::json!({"kind":"search","search":search,"upstream_response_id":"internal-trace"}),
            ),
            (
                "extract",
                "UNIQUE_EXTRACT_BODY",
                ProviderToolNormalizedOutput::Extract(extract.clone()),
                serde_json::json!({"kind":"extract","extract":extract,"upstream_response_id":"internal-trace"}),
            ),
        ] {
            let before = VulcanToolInvocationResult::from_model_output(payload.clone())
                .with_host_result(VulcanToolHostResult {
                    kind: "provider_tool_output".into(),
                    payload,
                });
            let after = render_provider_tool_result(
                None,
                tool,
                ProviderToolExecutionResult {
                    upstream_response_id: Some("internal-trace".into()),
                    output: normalized,
                },
            )
            .await
            .unwrap();
            assert_result_payload_cost(tool, before, after, marker);
        }
    }

    /// Verifies a conversation search keeps sources and snippets while removing the duplicate host body.
    /// 验证会话搜索保留来源及摘要，同时删除宿主正文副本。
    #[tokio::test]
    async fn search_result_is_single_markdown_body() {
        // The source fixture includes syntax that must remain a label rather than a second link.
        // 来源夹具包含必须作为标签而不是第二个链接的语法。
        let result = render_provider_tool_result(
            None,
            "search-test",
            ProviderToolExecutionResult {
                upstream_response_id: Some(String::from("internal-trace")),
                output: ProviderToolNormalizedOutput::Search(ProviderToolSearchOutput {
                    query: String::from("query"),
                    results: vec![ProviderToolSearchResult {
                        rank: 1,
                        title: String::from("[title]"),
                        url: String::from("https://example.test/page"),
                        source_domain: String::from("example.test"),
                        snippet: String::from("unique-search-evidence"),
                    }],
                }),
            },
        )
        .await
        .expect("conversation search projection");
        // The model-facing body is the sole persisted source of the readable evidence.
        // 模型正文是可读证据唯一的持久化来源。
        let body = result.model_output.as_str().expect("Markdown result");
        assert_eq!(body.matches("unique-search-evidence").count(), 1);
        assert!(body.contains("https://example.test/page"));
        assert!(!body.contains("internal-trace"));
        assert!(result.host_result.is_none());
    }

    /// Verifies partial extraction retains errors and evidence images without host-only favicon noise.
    /// 验证部分提取保留错误和证据图片，不包含仅用于宿主展示的图标噪音。
    #[test]
    fn extract_retains_partial_failure_and_image_references() {
        // A mixed result must not become an unqualified success or lose its failed source.
        // 混合结果不能变为无条件成功，也不能丢失失败来源。
        let output = extract(&ProviderToolExtractOutput {
            results: vec![ProviderToolExtractResult {
                url: String::from("https://example.test/page"),
                content: String::from("unique-page-content"),
                images: vec![String::from("https://example.test/evidence.png")],
                favicon: Some(String::from("https://example.test/favicon.ico")),
            }],
            failures: vec![ProviderToolExtractFailure {
                url: String::from("https://example.test/failed"),
                error: String::from("Access denied"),
            }],
        });
        assert_eq!(output.matches("unique-page-content").count(), 1);
        assert!(output.contains("evidence.png"));
        assert!(!output.contains("favicon.ico"));
        assert!(output.contains("Failed sources"));
        assert!(output.contains("Access denied"));
    }
}
