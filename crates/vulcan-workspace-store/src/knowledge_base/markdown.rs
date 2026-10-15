//! Deterministic Markdown title extraction and source-preserving chunking.
//! 确定性的 Markdown 标题提取与保留来源位置的切片逻辑。

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use sha2::{Digest, Sha256};
use std::ops::Range;
use vulcan_context::estimate_mixed_language_text_tokens;

/// Preferred mixed-language token count for one chunk.
/// 单个切片首选的中英文混合 token 数量。
pub const TARGET_CHUNK_TOKENS: usize = 800;
/// Hard mixed-language token ceiling for ordinary chunk construction.
/// 普通切片构建使用的中英文混合 token 硬上限。
pub const HARD_CHUNK_TOKENS: usize = 1_200;
/// Approximate overlap retained between adjacent oversized section chunks.
/// 超大章节相邻切片之间保留的近似重叠 token 数量。
pub const CHUNK_OVERLAP_TOKENS: usize = 80;

/// One source-preserving Markdown chunk before persistence assigns an identifier.
/// 持久化分配标识符之前的一个保留来源位置的 Markdown 切片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMarkdownChunk {
    /// Zero-based deterministic ordinal.
    /// 从零开始的确定性序号。
    pub ordinal: usize,
    /// Slash-separated heading ancestry.
    /// 以斜杠分隔的标题层级。
    pub heading_path: String,
    /// One-based inclusive first source line.
    /// 从一开始且包含边界的来源起始行。
    pub start_line: usize,
    /// One-based inclusive last source line.
    /// 从一开始且包含边界的来源结束行。
    pub end_line: usize,
    /// Exact newline-normalized Markdown slice.
    /// 精确的换行规范化 Markdown 切片。
    pub content: String,
    /// Mixed-language estimated token count.
    /// 中英文混合估算 token 数量。
    pub estimated_tokens: usize,
    /// SHA-256 digest of the chunk content.
    /// 切片内容的 SHA-256 摘要。
    pub content_sha256: String,
}

/// One heading boundary extracted from parser offsets.
/// 从解析器偏移量提取的一条标题边界。
#[derive(Debug, Clone, PartialEq, Eq)]
struct HeadingBoundary {
    /// Heading byte offset.
    /// 标题字节偏移。
    offset: usize,
    /// Numeric heading level in the range one through six.
    /// 一到六范围内的数字标题级别。
    level: usize,
    /// Plain heading text assembled from inline events.
    /// 从行内事件拼接得到的纯标题文本。
    title: String,
}

/// Normalizes source newlines while preserving all other Markdown bytes.
/// 在保留其他 Markdown 字节的同时规范化来源换行。
pub fn normalize_markdown_newlines(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

/// Exact body-line iterator recognizing LF, CRLF and CR without rewriting source bytes.
/// 识别 LF、CRLF 和 CR 且不改写来源字节的精确正文行迭代器。
pub struct KnowledgeBodyLines<'a> {
    /// Unconsumed exact UTF-8 suffix.
    /// 尚未消费的精确 UTF-8 后缀。
    remaining: &'a str,
}

impl<'a> Iterator for KnowledgeBodyLines<'a> {
    type Item = &'a str;

    /// Returns the next line including its original separator, or None after all bytes are consumed.
    /// 返回包含原始分隔符的下一行，全部字节消费后返回空值。
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        // ASCII separator positions are always valid UTF-8 slicing boundaries.
        // ASCII 分隔符位置始终是合法 UTF-8 切片边界。
        let bytes = self.remaining.as_bytes();
        // CRLF is one logical separator, matching the Markdown chunker's normalization authority.
        // CRLF 是一个逻辑分隔符，与 Markdown 切块规范化权威一致。
        let end = match bytes.iter().position(|byte| matches!(byte, b'\r' | b'\n')) {
            Some(position)
                if bytes[position] == b'\r' && bytes.get(position + 1) == Some(&b'\n') =>
            {
                position + 2
            }
            Some(position) => position + 1,
            None => bytes.len(),
        };
        // Keep original separators in the yielded slice rather than materializing normalized text.
        // 在产出切片中保留原分隔符，而非物化规范化文本。
        let (line, remaining) = self.remaining.split_at(end);
        self.remaining = remaining;
        Some(line)
    }
}

/// Iterates exact body lines with the chunker's logical line semantics; returns a zero-allocation iterator.
/// 按切块器逻辑行语义遍历精确正文行；返回零分配迭代器。
pub fn knowledge_body_lines(content: &str) -> KnowledgeBodyLines<'_> {
    KnowledgeBodyLines { remaining: content }
}

/// Extracts the first level-one heading or falls back to the file stem.
/// 提取第一个一级标题，若不存在则回退到文件主名。
pub fn extract_document_title(content: &str, file_stem: &str) -> String {
    extract_heading_boundaries(content)
        .into_iter()
        .find(|heading| heading.level == 1 && !heading.title.trim().is_empty())
        .map(|heading| heading.title.trim().to_string())
        .unwrap_or_else(|| file_stem.to_string())
}

/// Extracts only parsed Markdown headings from `content` within the remaining `byte_budget`.
/// 在剩余 `byte_budget` 内仅提取 `content` 中已解析的 Markdown 标题。
/// Returns bounded headings and whether any were omitted; fenced-code text never becomes an outline.
/// 返回有界标题及是否省略了条目；围栏代码文本不会成为大纲。
pub fn extract_document_outline(content: &str, byte_budget: usize) -> (Vec<String>, bool) {
    // Normalization follows the same heading parser used by source chunking.
    // 规范化沿用来源切块使用的相同标题解析器。
    let normalized = normalize_markdown_newlines(content);
    // Only actual heading events contribute text to the model-visible receipt.
    // 仅实际标题事件向模型可见回执贡献文本。
    let headings = extract_heading_boundaries(&normalized);
    // The output is bounded by both item count and the caller's aggregate byte budget.
    // 输出同时受条目数及调用方累计字节预算约束。
    let mut output = Vec::new();
    // Count exact heading bytes rather than body bytes or estimated tokens.
    // 计算精确标题字节，而非正文字节或估算 token。
    let mut bytes = 0usize;
    for heading in headings {
        if output.len() >= super::MAX_KNOWLEDGE_OUTLINE_LINES
            || heading.title.len() > byte_budget.saturating_sub(bytes)
        {
            return (output, true);
        }
        bytes += heading.title.len();
        output.push(heading.title);
    }
    (output, false)
}

/// Parsed outlines exclude code and bound both Unicode bytes and heading count.
/// 已解析大纲排除代码，并同时约束 Unicode 字节及标题数量。
#[cfg(test)]
#[test]
fn knowledge_outline_has_no_body_or_fenced_headings() {
    // Only real heading events can become a transfer receipt.
    // 仅真实标题事件可成为搬运回执。
    let unicode_heading = "中文";
    // Heading text comes from this source input rather than an independently maintained wording assertion.
    // 标题文本来自此来源输入，而非独立维护的措辞断言。
    let content =
        format!("# Title\r\nprivate-body\r\n```md\r\n# Fake\r\n```\r\n## {unicode_heading}\r\n");
    assert_eq!(
        extract_document_outline(&content, usize::MAX),
        (vec!["Title".into(), unicode_heading.into()], false)
    );
    assert_eq!(
        extract_document_outline(&content, "Title".len()),
        (vec!["Title".into()], true)
    );
    assert_eq!(extract_document_outline(&content, 0), (Vec::new(), true));
    // A source with many headings retains only the shared heading-count limit.
    // 标题很多的来源仅保留共享标题数量上限。
    let many = "# Heading\n".repeat(super::MAX_KNOWLEDGE_OUTLINE_LINES + 1);
    assert_eq!(
        extract_document_outline(&many, usize::MAX).0.len(),
        super::MAX_KNOWLEDGE_OUTLINE_LINES
    );
    assert!(extract_document_outline(&many, usize::MAX).1);
}

/// Splits normalized Markdown into deterministic source-citable chunks.
/// 将规范化 Markdown 拆分为确定性且可引用来源的切片。
pub fn chunk_markdown(content: &str) -> Vec<ParsedMarkdownChunk> {
    let normalized = normalize_markdown_newlines(content);
    if normalized.trim().is_empty() {
        return Vec::new();
    }

    let headings = extract_heading_boundaries(&normalized);
    let mut sections = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();

    if headings.first().is_some_and(|heading| heading.offset > 0) || headings.is_empty() {
        let end = headings
            .first()
            .map_or(normalized.len(), |heading| heading.offset);
        sections.push((0..end, String::new()));
    }

    for (index, heading) in headings.iter().enumerate() {
        heading_stack.truncate(heading.level.saturating_sub(1));
        while heading_stack.len() < heading.level.saturating_sub(1) {
            heading_stack.push(String::new());
        }
        heading_stack.push(heading.title.clone());
        let end = headings
            .get(index + 1)
            .map_or(normalized.len(), |next| next.offset);
        sections.push((heading.offset..end, compact_heading_path(&heading_stack)));
    }

    let line_starts = build_line_starts(&normalized);
    let mut chunks = Vec::new();
    for (range, heading_path) in sections {
        append_section_chunks(&normalized, range, &heading_path, &line_starts, &mut chunks);
    }
    chunks
}

/// Computes a lowercase hexadecimal SHA-256 digest.
/// 计算小写十六进制 SHA-256 摘要。
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Extracts heading offsets, levels, and inline text with pulldown-cmark.
/// 使用 pulldown-cmark 提取标题偏移、级别与行内文本。
fn extract_heading_boundaries(content: &str) -> Vec<HeadingBoundary> {
    let mut headings = Vec::new();
    let mut current: Option<(usize, usize, String)> = None;

    for (event, range) in Parser::new(content).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((range.start, heading_level_number(level), String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, title)) = current.as_mut() {
                    title.push_str(&text);
                }
            }
            Event::End(pulldown_cmark::TagEnd::Heading(_)) => {
                if let Some((offset, level, title)) = current.take() {
                    headings.push(HeadingBoundary {
                        offset,
                        level,
                        title: title.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    headings
}

/// Converts one pulldown-cmark heading level into a stable integer.
/// 将 pulldown-cmark 标题级别转换为稳定整数。
fn heading_level_number(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Removes empty synthetic ancestors and joins a heading stack.
/// 移除空的合成祖先并拼接标题栈。
fn compact_heading_path(stack: &[String]) -> String {
    stack
        .iter()
        .filter(|part| !part.trim().is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Builds byte offsets for each one-based source line.
/// 为每个从一开始的来源行构建字节偏移。
fn build_line_starts(content: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' && index + 1 < content.len() {
            starts.push(index + 1);
        }
    }
    starts
}

/// Appends one natural section, splitting only when its size requires it.
/// 追加一个自然章节，仅在大小确有需要时继续拆分。
fn append_section_chunks(
    content: &str,
    range: Range<usize>,
    heading_path: &str,
    line_starts: &[usize],
    output: &mut Vec<ParsedMarkdownChunk>,
) {
    let section = content[range.clone()].trim_matches('\n');
    if section.trim().is_empty() {
        return;
    }
    let trimmed_start = range.start + content[range].find(section).unwrap_or(0);
    let tokens = estimate_mixed_language_text_tokens(section).max(1);
    if tokens <= HARD_CHUNK_TOKENS {
        push_chunk(
            content,
            trimmed_start..trimmed_start + section.len(),
            heading_path,
            line_starts,
            output,
        );
        return;
    }

    let section_end = trimmed_start + section.len();
    let mut cursor = trimmed_start;
    while cursor < section_end {
        let mut end = cursor;
        for line in content[cursor..section_end].split_inclusive('\n') {
            let next = end + line.len();
            let next_estimated = estimate_mixed_language_text_tokens(&content[cursor..next]).max(1);
            if next_estimated > HARD_CHUNK_TOKENS {
                if end == cursor {
                    end =
                        largest_prefix_within_token_limit(content, cursor, next, HARD_CHUNK_TOKENS);
                }
                break;
            }
            end = next;
            if next_estimated >= TARGET_CHUNK_TOKENS {
                break;
            }
        }
        if end <= cursor {
            end = next_char_boundary(content, cursor, section_end);
        }
        push_chunk(content, cursor..end, heading_path, line_starts, output);
        if end >= section_end {
            break;
        }
        cursor = overlap_start(content, cursor, end);
    }
}

/// Finds the longest UTF-8 prefix whose estimated size does not exceed one hard token limit.
/// 查找估算大小不超过指定 token 硬上限的最长 UTF-8 前缀。
fn largest_prefix_within_token_limit(
    content: &str,
    start: usize,
    end: usize,
    token_limit: usize,
) -> usize {
    let mut boundaries = content[start..end]
        .char_indices()
        .map(|(offset, _)| start + offset)
        .collect::<Vec<_>>();
    boundaries.push(end);
    let mut lower = 1usize;
    let mut upper = boundaries.len() - 1;
    while lower < upper {
        let middle = (lower + upper).div_ceil(2);
        let candidate = boundaries[middle];
        if estimate_mixed_language_text_tokens(&content[start..candidate]).max(1) <= token_limit {
            lower = middle;
        } else {
            upper = middle - 1;
        }
    }
    boundaries[lower]
}

/// Pushes one non-empty chunk with stable source lines and digest.
/// 追加一个具有稳定来源行和摘要的非空切片。
fn push_chunk(
    source: &str,
    range: Range<usize>,
    heading_path: &str,
    line_starts: &[usize],
    output: &mut Vec<ParsedMarkdownChunk>,
) {
    let raw = &source[range.clone()];
    let content = raw.trim_matches('\n');
    if content.trim().is_empty() {
        return;
    }
    let leading = raw.find(content).unwrap_or(0);
    let start = range.start + leading;
    let end = start + content.len();
    output.push(ParsedMarkdownChunk {
        ordinal: output.len(),
        heading_path: heading_path.to_string(),
        start_line: line_number_at(line_starts, start),
        end_line: line_number_at(line_starts, end.saturating_sub(1)),
        content: content.to_string(),
        estimated_tokens: estimate_mixed_language_text_tokens(content).max(1),
        content_sha256: sha256_hex(content.as_bytes()),
    });
}

/// Finds the one-based line containing a byte offset.
/// 查找包含某个字节偏移的从一开始行号。
fn line_number_at(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|start| *start <= offset).max(1)
}

/// Finds a UTF-8 boundary for the emergency single-character progress path.
/// 为紧急单字符推进路径查找 UTF-8 边界。
fn next_char_boundary(content: &str, start: usize, limit: usize) -> usize {
    content[start..limit]
        .char_indices()
        .nth(1)
        .map_or(limit, |(offset, _)| start + offset)
}

/// Finds a newline-aligned overlap start without crossing the current chunk start.
/// 查找不越过当前切片起点且按换行对齐的重叠起点。
fn overlap_start(content: &str, chunk_start: usize, chunk_end: usize) -> usize {
    let mut cursor = chunk_end;
    while cursor > chunk_start {
        let candidate = content[..cursor]
            .rfind('\n')
            .map_or(chunk_start, |index| index + 1);
        if candidate <= chunk_start {
            return chunk_end;
        }
        if estimate_mixed_language_text_tokens(&content[candidate..chunk_end])
            >= CHUNK_OVERLAP_TOKENS
        {
            return candidate;
        }
        cursor = candidate.saturating_sub(1);
    }
    chunk_end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies Chinese headings, CRLF normalization, and source lines remain deterministic.
    /// 验证中文标题、CRLF 规范化与来源行号保持确定性。
    #[test]
    fn chunks_chinese_markdown_with_stable_lines() {
        let markdown = "# 总览\r\n\r\n第一段。\r\n\r\n## 接口\r\n\r\n`search_api` 说明。\r\n";
        let normalized = normalize_markdown_newlines(markdown);
        let chunks = chunk_markdown(&normalized);

        assert_eq!(extract_document_title(&normalized, "fallback"), "总览");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading_path, "总览");
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[1].heading_path, "总览 / 接口");
        assert_eq!(chunks[1].start_line, 5);
    }

    /// Verifies blank documents do not create meaningless FTS rows.
    /// 验证空白文档不会创建无意义的 FTS 记录。
    #[test]
    fn blank_markdown_has_no_chunks() {
        assert!(chunk_markdown(" \r\n\t").is_empty());
    }

    /// Verifies large sections split while preserving overlap and the hard target.
    /// 验证大章节会在保留重叠的同时按硬目标拆分。
    #[test]
    fn oversized_section_splits_deterministically() {
        let markdown = format!("# 大章节\n\n{}", "一段较长的中文内容。\n".repeat(2_000));
        let chunks = chunk_markdown(&markdown);

        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.estimated_tokens <= HARD_CHUNK_TOKENS)
        );
        assert!(chunks.iter().all(|chunk| chunk.heading_path == "大章节"));
    }

    /// Verifies one oversized line cannot bypass the hard chunk ceiling.
    /// 验证单个超长行无法绕过切片硬上限。
    #[test]
    fn oversized_single_line_respects_hard_limit() {
        let markdown = format!("# 单行\n{}", "超长内容".repeat(4_000));
        let chunks = chunk_markdown(&markdown);

        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.estimated_tokens <= HARD_CHUNK_TOKENS)
        );
    }
}
