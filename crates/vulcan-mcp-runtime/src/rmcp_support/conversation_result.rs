//! Conversation MCP projection keeps ordered reading content and native supported media.
//! 会话 MCP 投影保留有序阅读内容与受支持的原生媒体。

use super::*;
use rmcp::model::{ContentBlock, ResourceContents};
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Projects all blocks while deduplicating only provably identical structured/text JSON.
/// 投影全部内容块，仅去重能够证明相同的结构化与文本 JSON。
/// Takes the complete MCP result; returns readable content, a native image envelope, or explicit error.
/// 接受完整 MCP 结果；返回可读内容、原生图片封套或明确错误。
pub(super) fn project(mut result: CallToolResult) -> Result<Value, McpRuntimeError> {
    // A single text block that decodes exactly to structured_content is the MCP compatibility
    // duplicate. Other text/structure combinations are opaque and both must be preserved.
    // 单个文本块精确解码为 structured_content 时属于 MCP 兼容副本；其他组合不透明，必须两者都保留。
    if let [ContentBlock::Text(text)] = result.content.as_slice()
        && let Some(structured) = &result.structured_content
        && serde_json::from_str::<Value>(&text.text)
            .as_ref()
            .is_ok_and(|value| value == structured)
    {
        result.structured_content = None;
    }
    let mut content = Vec::with_capacity(result.content.len());
    let mut text_parts = Vec::new();
    let mut has_image = false;
    for block in result.content {
        let text = match block {
            ContentBlock::Text(text) => text.text,
            ContentBlock::Image(image) => {
                validate_mcp_image_content(&image.data, &image.mime_type)
                    .map_err(McpRuntimeError::transport)?;
                content.push(serde_json::json!({"type":"image","data":image.data,"mimeType":image.mime_type}));
                has_image = true;
                continue;
            }
            ContentBlock::ResourceLink(resource) => {
                let mut text = format!(
                    "Resource {}: {}",
                    inline_code(&resource.name),
                    inline_code(&resource.uri)
                );
                if let Some(title) = resource.title {
                    text.push_str(&format!("\nTitle: {title}"));
                }
                if let Some(description) = resource.description {
                    text.push_str(&format!("\n{description}"));
                }
                if let Some(mime) = resource.mime_type {
                    text.push_str(&format!("\nMIME: {}", inline_code(&mime)));
                }
                if let Some(size) = resource.size {
                    text.push_str(&format!("\nBytes: {size}"));
                }
                text
            }
            ContentBlock::Resource(resource) => match resource.resource {
                ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => {
                    let mime = mime_type
                        .map(|mime| format!("\nMIME: {}", inline_code(&mime)))
                        .unwrap_or_default();
                    format!(
                        "Resource: {}{mime}\n\n{}",
                        inline_code(&uri),
                        fenced_code(&text)
                    )
                }
                // Binary resources have no durable native carrier in the current tool contract.
                // Reject explicitly rather than report success while silently dropping their bytes.
                // 当前工具契约没有二进制资源的持久原生载体；明确拒绝，避免丢弃字节却报告成功。
                ResourceContents::BlobResourceContents { .. } => {
                    return Err(unsupported("binary embedded resource"));
                }
                _ => return Err(unsupported("unknown embedded resource type")),
            },
            ContentBlock::Audio(_) => return Err(unsupported("audio")),
            _ => return Err(unsupported("unknown content type")),
        };
        text_parts.push(text.clone());
        content.push(serde_json::json!({"type":"text","text":text}));
    }
    if result.is_error.unwrap_or(false) {
        if let Some(structured) = result.structured_content {
            text_parts.push(fenced_code(
                &serde_json::to_string_pretty(&structured)
                    .map_err(|error| McpRuntimeError::transport(error.to_string()))?,
            ));
        }
        return Err(McpRuntimeError::transport(format!(
            "MCP tool reported failure after invocation. Do not assume retry is safe.\n\n{}",
            text_parts.join("\n\n")
        )));
    }
    if has_image {
        let mut envelope = Map::new();
        envelope.insert("content".into(), Value::Array(content));
        if let Some(structured) = result.structured_content {
            envelope.insert("structured_content".into(), structured);
        }
        return Ok(Value::Object(envelope));
    }
    if let Some(structured) = result.structured_content {
        if text_parts.is_empty() {
            // Unknown structured-only tools remain raw business data, not guessed Markdown fields.
            // 未知纯结构工具保留原始业务数据，不猜测 Markdown 字段。
            return Ok(structured);
        }
        text_parts.push(format!(
            "## Structured result\n\n{}",
            fenced_code(
                &serde_json::to_string_pretty(&structured)
                    .map_err(|error| McpRuntimeError::transport(error.to_string()))?
            )
        ));
    }
    Ok(Value::String(text_parts.join("\n\n")))
}

/// Reports an unsupported native content type after dispatch without claiming retry safety.
/// 在派发后报告不支持的原生内容类型，不宣称可安全重试。
/// Takes the type name and returns the explicit transport diagnostic.
/// 接受类型名称，返回明确的传输诊断。
fn unsupported(kind: &str) -> McpRuntimeError {
    McpRuntimeError::transport(format!(
        "MCP tool completed but returned unsupported conversation {kind}; its side effects may already have occurred. Do not repeat the operation automatically."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Native images retain ordering and one encoded copy, while failures remain explicit.
    /// 原生图片保留顺序和唯一编码副本，同时明确报告失败。
    #[test]
    fn conversation_preserves_native_images_and_reports_post_dispatch_failure() {
        let response: CallToolResult = serde_json::from_value(json!({"content":[
            {"type":"text","text":"before"},{"type":"image","data":"iVBORw0KGgo=","mimeType":"image/png"},
            {"type":"text","text":"after"}]})).unwrap();
        let value = project(response).unwrap();
        assert_eq!(value["content"][0]["text"], "before");
        assert_eq!(value["content"][1]["type"], "image");
        assert_eq!(value["content"][2]["text"], "after");
        assert_eq!(value.to_string().matches("iVBORw0KGgo=").count(), 1);
        let failed: CallToolResult = serde_json::from_value(json!({"isError":true,"content":[
            {"type":"text","text":"first failure"},{"type":"text","text":"partial effects"}]}))
        .unwrap();
        let error = project(failed).unwrap_err().to_string();
        assert!(
            error.contains("first failure")
                && error.contains("partial effects")
                && error.contains("Do not assume retry is safe")
        );
        let unsupported: CallToolResult = serde_json::from_value(json!({"content":[
            {"type":"audio","data":"YQ==","mimeType":"audio/wav"}]}))
        .unwrap();
        assert!(
            project(unsupported)
                .unwrap_err()
                .to_string()
                .contains("side effects may already have occurred")
        );
    }

    /// Ensures every text block, literal JSON and embedded resource survives in source order.
    /// 确保全部文本块、原始 JSON 与内嵌资源按来源顺序保留。
    #[test]
    fn conversation_preserves_multiple_blocks_and_does_not_parse_plain_json() {
        let response: CallToolResult = serde_json::from_value(json!({"content":[{"type":"text","text":"first"},{"type":"text","text":"{\"literal\":true}"},{"type":"resource","resource":{"uri":"fixture://read","text":"```\n正文\n```"}}]})).expect("MCP response");
        let output = project(response).expect("projection");
        let text = output.as_str().expect("text");
        assert!(text.starts_with("first\n\n{\"literal\":true}"));
        assert!(text.contains("fixture://read"));
        assert!(text.contains("````"));
        let json_only: CallToolResult = serde_json::from_value(
            json!({"content":[{"type":"text","text":"{\"literal\":true}"}]}),
        )
        .expect("MCP response");
        assert!(project(json_only).expect("projection").is_string());
    }

    /// Ensures only equality-proven JSON compatibility copies disappear, preserving independent facts.
    /// 确保只删除等值证明成立的 JSON 兼容副本，保留独立事实。
    #[test]
    fn conversation_deduplicates_only_exact_structured_copy() {
        let response:CallToolResult=serde_json::from_value(json!({"content":[{"type":"text","text":"{\"body\":\"UNIQUE_BODY\"}"}],"structuredContent":{"body":"UNIQUE_BODY"}})).expect("MCP response");
        assert_eq!(
            project(response)
                .expect("projection")
                .to_string()
                .matches("UNIQUE_BODY")
                .count(),
            1
        );
        let independent:CallToolResult=serde_json::from_value(json!({"content":[{"type":"text","text":"summary"}],"structuredContent":{"cursor":"NEXT"}})).expect("MCP response");
        let output = project(independent).expect("projection");
        assert!(output.as_str().expect("text").contains("NEXT"));
    }
}
