//! Extension-aware Markdown parsing helpers for agent-view documents.
//! 面向 agent-view 文档的扩展感知 Markdown 解析辅助逻辑。

use crate::document::{
    AgentViewAttributes, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument,
};

/// Parses one mixed Markdown and directive source string into the structured agent-view document.
/// 将一段混合 Markdown 与扩展指令的源码解析为结构化 agent-view 文档。
pub fn parse_agent_view_document(source: &str) -> AgentViewDocument {
    let mut document = AgentViewDocument::new();
    let mut markdown_buffer = String::new();
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let mut index = 0usize;

    while index < lines.len() {
        let current_line = lines[index];

        if let Some(header) = parse_directive_header(current_line) {
            let mut raw_source = String::from(current_line);
            let mut body = String::new();
            let mut closing_index = None;
            let mut look_ahead_index = index + 1;

            while look_ahead_index < lines.len() {
                let line = lines[look_ahead_index];
                raw_source.push_str(line);

                if line.trim() == ":::" {
                    closing_index = Some(look_ahead_index);
                    break;
                }

                body.push_str(line);
                look_ahead_index += 1;
            }

            if let Some(found_closing_index) = closing_index {
                if !markdown_buffer.is_empty() {
                    document.push_markdown(std::mem::take(&mut markdown_buffer));
                }
                document.push_directive(AgentViewDirectiveBlock::new(
                    header.kind,
                    header.attributes,
                    body,
                    raw_source,
                    header.collapsed_by_default,
                ));
                index = found_closing_index + 1;
                continue;
            }

            markdown_buffer.push_str(&raw_source);
            index = lines.len();
            continue;
        }

        markdown_buffer.push_str(current_line);
        index += 1;
    }

    if !markdown_buffer.is_empty() {
        document.push_markdown(markdown_buffer);
    }

    document
}

/// Parsed directive header parts extracted from one opening fence line.
/// 从单条起始围栏行中提取出的指令头部信息。
struct ParsedDirectiveHeader {
    /// Semantic directive kind resolved from the first token after the fence marker.
    /// 从围栏标记后的首个标记解析得到的语义指令类型。
    kind: AgentViewDirectiveKind,
    /// Parsed key-value attributes attached to the directive header line.
    /// 附着在指令头部行上的键值属性集合。
    attributes: AgentViewAttributes,
    /// Whether the directive should be collapsed by default according to its attributes.
    /// 根据属性判断该指令是否应默认折叠。
    collapsed_by_default: bool,
}

/// Parses one opening directive fence line when it matches the supported extension format.
/// 当单条起始指令围栏行符合支持的扩展格式时对其进行解析。
fn parse_directive_header(line: &str) -> Option<ParsedDirectiveHeader> {
    let trimmed = line.trim();
    let header_body = trimmed.strip_prefix(":::")?.trim();
    if header_body.is_empty() {
        return None;
    }

    let tokens = tokenize_header_body(header_body);
    let directive_name = tokens.first()?.to_string();
    let attributes = parse_header_attributes(&tokens[1..]);
    let collapsed_by_default = attributes
        .get("collapsed")
        .and_then(|value| parse_bool_flag(value))
        .unwrap_or(false);

    Some(ParsedDirectiveHeader {
        kind: directive_kind_from_name(&directive_name),
        attributes,
        collapsed_by_default,
    })
}

/// Tokenizes one directive header body while preserving quoted attribute values containing spaces.
/// 对单条指令头部正文进行分词，并保留包含空格的带引号属性值。
fn tokenize_header_body(header_body: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote_marker = None;

    for character in header_body.chars() {
        match quote_marker {
            Some(active_quote) if character == active_quote => {
                quote_marker = None;
            }
            Some(_) => current.push(character),
            None if character == '"' || character == '\'' => {
                quote_marker = Some(character);
            }
            None if character.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(character),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Parses one token slice into key-value directive attributes.
/// 将一组标记切片解析为键值形式的指令属性。
fn parse_header_attributes(tokens: &[String]) -> AgentViewAttributes {
    let mut attributes = AgentViewAttributes::new();

    for token in tokens {
        if let Some((key, value)) = token.split_once('=') {
            attributes.insert(key.to_string(), value.to_string());
        } else {
            attributes.insert(token.to_string(), String::from("true"));
        }
    }

    attributes
}

/// Converts one directive label into the matching semantic directive kind.
/// 将单条指令标签转换为对应的语义指令类型。
fn directive_kind_from_name(value: &str) -> AgentViewDirectiveKind {
    match value {
        "user-prompt" => AgentViewDirectiveKind::UserPrompt,
        "thinking" => AgentViewDirectiveKind::Thinking,
        "tool-call" => AgentViewDirectiveKind::ToolCall,
        "tool-result" => AgentViewDirectiveKind::ToolResult,
        "diff" => AgentViewDirectiveKind::Diff,
        "notice" => AgentViewDirectiveKind::Notice,
        custom => AgentViewDirectiveKind::Custom(custom.to_string()),
    }
}

/// Parses one loosely formatted boolean attribute value used by directive headers.
/// 解析指令头部使用的宽松布尔属性值。
fn parse_bool_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Targeted parser regression tests for the first-generation agent-view document pipeline.
    //! 第一代 agent-view 文档管线使用的定向解析回归测试。

    use super::parse_agent_view_document;
    use crate::document::{AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind};

    /// Verifies that plain Markdown without directive fences stays as a single Markdown block.
    /// 验证不含扩展指令围栏的纯 Markdown 会保持为单个 Markdown 区块。
    #[test]
    fn parses_plain_markdown_as_single_markdown_block() {
        let document = parse_agent_view_document("# Title\n\nHello **world**.\n");

        assert_eq!(document.blocks.len(), 1);
        assert!(matches!(
            document.blocks.first(),
            Some(AgentViewBlock::Markdown(_))
        ));
        assert_eq!(document.plain_text(), "Title\nHello world.");
    }

    /// Verifies that extension directive fences split the source into surrounding Markdown and directive blocks.
    /// 验证扩展指令围栏会将源码切分为前后 Markdown 与指令区块。
    #[test]
    fn parses_mixed_markdown_and_thinking_directive() {
        let source =
            "Intro paragraph.\n\n:::thinking collapsed=true\nStep 1\nStep 2\n:::\n\nAfterwards.\n";
        let document = parse_agent_view_document(source);

        assert_eq!(document.blocks.len(), 3);
        assert!(matches!(document.blocks[0], AgentViewBlock::Markdown(_)));
        assert!(matches!(document.blocks[2], AgentViewBlock::Markdown(_)));

        match &document.blocks[1] {
            AgentViewBlock::Directive(block) => {
                assert_eq!(block.kind, AgentViewDirectiveKind::Thinking);
                assert!(block.collapsed_by_default);
                assert_eq!(block.body, "Step 1\nStep 2\n");
            }
            AgentViewBlock::Markdown(_) => panic!("expected directive block in the middle"),
        }
    }

    /// Verifies that unclosed directive fences gracefully fall back to plain Markdown.
    /// 验证未闭合的指令围栏会优雅回退为普通 Markdown。
    #[test]
    fn treats_unclosed_directive_as_plain_markdown() {
        let source = "Before\n:::tool-call id=run-1\ncargo check\n";
        let document = parse_agent_view_document(source);

        assert_eq!(document.blocks.len(), 1);
        assert!(matches!(document.blocks[0], AgentViewBlock::Markdown(_)));
    }

    /// Verifies that runtime-owned directive constructors keep semantic kind and raw fence source aligned.
    /// 验证运行时直接构造的指令区块能够保持语义类型与围栏源码一致。
    #[test]
    fn builds_runtime_owned_thinking_directive_source() {
        let block = AgentViewDirectiveBlock::thinking("Investigating output layout.", true);

        assert_eq!(block.kind, AgentViewDirectiveKind::Thinking);
        assert!(block.raw_source.starts_with(":::thinking"));
        assert!(block.raw_source.contains("collapsed=true"));
        assert!(block.raw_source.contains("Investigating output layout."));
    }
}
