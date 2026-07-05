//! Response parsing helpers for structured and narrative web-search providers.
//! 结构化与叙述型网页搜索供应商的响应解析辅助能力。

use serde_json::Value;

use super::{
    helpers::{extract_string_array, first_string},
    types::{WebSearchCitation, WebSearchResult},
};

/// Parses one common structured result array into normalized result entries.
/// 将通用结构化结果数组解析为规范化结果条目。
pub(super) fn parse_structured_results_array(items: &[Value]) -> Vec<WebSearchResult> {
    items
        .iter()
        .filter_map(|item| {
            // Structured search providers use different documented field names for the same result concepts.
            // 结构化搜索供应商会对同一结果概念使用不同的已知字段名。
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

/// Extracts Perplexity citations from either top-level fields or annotations.
/// 从顶层字段或注解块中提取 Perplexity 引用。
pub(super) fn extract_perplexity_citations(payload: &Value) -> Vec<WebSearchCitation> {
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
                    // Perplexity-compatible payloads may expose URL citations as nested OpenAI annotations or flattened URL fields.
                    // Perplexity 兼容载荷可能以 OpenAI 嵌套注解或扁平 URL 字段暴露引用链接。
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

/// Extracts the first xAI output text block from a Responses payload.
/// 从 Responses 载荷中提取首个 xAI 输出文本块。
pub(super) fn extract_xai_output_text(payload: &Value) -> Option<String> {
    if let Some(output) = payload.get("output").and_then(Value::as_array) {
        for item in output {
            if item.get("type").and_then(Value::as_str) == Some("message")
                && let Some(content_items) = item.get("content").and_then(Value::as_array)
            {
                for block in content_items {
                    // xAI Responses message items expose visible text through output_text content blocks.
                    // xAI Responses 的 message 条目会通过 output_text 内容块暴露可见文本。
                    if block.get("type").and_then(Value::as_str) == Some("output_text")
                        && let Some(text) = block.get("text").and_then(Value::as_str)
                    {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
            // Some Responses-compatible payloads also emit standalone output_text items.
            // 部分 Responses 兼容载荷也会返回独立的 output_text 条目。
            if item.get("type").and_then(Value::as_str) == Some("output_text")
                && let Some(text) = item.get("text").and_then(Value::as_str)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    // xAI-compatible helpers may mirror final visible text into the top-level output_text field.
    // xAI 兼容辅助结构也可能把最终可见文本镜像到顶层 output_text 字段。
    payload
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

/// Extracts the most useful visible Kimi message text.
/// 提取 Kimi 消息中最有用的可见文本。
pub(super) fn extract_kimi_message_text(message: &Value) -> Option<String> {
    first_string(message, &["content", "reasoning_content"])
}

/// Extracts Kimi citation URLs from both top-level search results and tool-call arguments.
/// 从顶层搜索结果与工具调用参数中提取 Kimi 引用链接。
pub(super) fn extract_kimi_citation_urls(payload: &Value) -> Vec<String> {
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
pub(super) fn decode_html_entities(text: &str) -> String {
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
pub(super) fn strip_html(html: &str) -> String {
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
pub(super) fn is_duckduckgo_bot_challenge(html: &str) -> bool {
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
pub(super) fn read_href_attribute(tag_attributes: &str) -> Option<String> {
    let marker = "href=\"";
    let start = tag_attributes.find(marker)? + marker.len();
    let rest = &tag_attributes[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Decodes one DuckDuckGo redirect URL into the original result URL when possible.
/// 在可能的情况下将一条 DuckDuckGo 重定向链接解码为原始结果链接。
pub(super) fn decode_duckduckgo_url(raw_url: &str) -> String {
    let normalized = if raw_url.starts_with("//") {
        format!("https:{raw_url}")
    } else {
        raw_url.to_string()
    };
    if let Ok(parsed) = reqwest::Url::parse(&normalized)
        && let Some(uddg) = parsed
            .query_pairs()
            .find_map(|(key, value)| (key == "uddg").then(|| value.to_string()))
    {
        return uddg;
    }
    normalized
}

/// Parses DuckDuckGo HTML search results into normalized search entries.
/// 将 DuckDuckGo HTML 搜索结果解析为规范化搜索条目。
pub(super) fn parse_duckduckgo_html(html: &str) -> Vec<WebSearchResult> {
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
pub(super) fn extract_duckduckgo_snippet(trailing_html: &str) -> Option<String> {
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

/// Truncates provider error bodies before they are surfaced to the model or UI.
/// 在向模型或界面展示供应商错误正文前进行截断。
pub(super) fn truncate_error_text(text: &str, max_chars: usize) -> String {
    let mut truncated = text.trim().chars().take(max_chars).collect::<String>();
    if text.trim().chars().count() > max_chars {
        truncated.push_str("...");
    }
    truncated
}
