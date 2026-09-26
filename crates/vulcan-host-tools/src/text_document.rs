//! Line-ending-aware text document model shared by native file tools.
//! 原生文件工具共享的换行感知文本文档模型。

use crate::text_encoding::{
    DecodedText, TextBom, TextCodecError, TextEncoding, TextEncodingSource, encode_text,
};
use std::fmt::{Display, Formatter};

/// Maximum source ranges returned for an ambiguous edit; shared with the file error projection.
/// 歧义编辑返回的源范围上限，与文件错误投影共享。
pub const MAX_ERROR_CANDIDATE_RANGES: usize = 32;

/// One recognized logical line terminator, including the uncommon LFCR pair.
/// 一种已识别的逻辑行终止符，包括不常见的 LFCR 组合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineEnding {
    /// Line feed.
    /// 换行符 LF。
    Lf,
    /// Carriage return followed by line feed.
    /// 回车换行 CRLF。
    CrLf,
    /// Carriage return.
    /// 回车符 CR。
    Cr,
    /// Line feed followed by carriage return.
    /// 换行回车 LFCR。
    LfCr,
}

impl LineEnding {
    /// Returns the exact terminator text.
    /// 返回精确的终止符文本。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
            Self::LfCr => "\n\r",
        }
    }

    /// Returns the stable metadata label.
    /// 返回稳定的元数据标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "lf",
            Self::CrLf => "crlf",
            Self::Cr => "cr",
            Self::LfCr => "lfcr",
        }
    }

    /// Parses one public line-ending label.
    /// 解析一个公开换行标签。
    pub fn parse(label: &str) -> Option<Self> {
        match label.trim().to_ascii_lowercase().as_str() {
            "lf" => Some(Self::Lf),
            "crlf" => Some(Self::CrLf),
            "cr" => Some(Self::Cr),
            "lfcr" => Some(Self::LfCr),
            _ => None,
        }
    }
}

impl Display for LineEnding {
    /// Renders the stable metadata label.
    /// 渲染稳定的元数据标签。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One logical line and its original terminator.
/// 一条逻辑行及其原始终止符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLine {
    /// Line content without a terminator.
    /// 不含终止符的行正文。
    pub content: String,
    /// Original terminator, or none for an unterminated final line.
    /// 原始终止符；未终止的最后一行为无。
    pub ending: Option<LineEnding>,
}

/// One decoded document preserving encoding, BOM, and per-line endings.
/// 一份保留编码、BOM 与逐行换行符的已解码文档。
#[derive(Debug, Clone)]
pub struct TextDocument {
    /// Original decoded Unicode text.
    /// 原始解码后的 Unicode 文本。
    pub text: String,
    /// Exact source encoding.
    /// 精确源编码。
    pub encoding: TextEncoding,
    /// Exact source BOM.
    /// 精确源 BOM。
    pub bom: TextBom,
    /// Encoding-selection source.
    /// 编码选择来源。
    pub encoding_source: TextEncodingSource,
    /// Whether the encoding selection is safe for write-back.
    /// 编码选择是否可安全写回。
    pub encoding_certain: bool,
    /// Logical lines with exact terminators.
    /// 带精确终止符的逻辑行。
    pub lines: Vec<DocumentLine>,
    /// Preferred terminator for newly inserted text.
    /// 新插入文本使用的首选终止符。
    pub preferred_line_ending: LineEnding,
    /// Whether multiple terminator kinds occur in the source.
    /// 源文本是否包含多种终止符。
    pub mixed_line_endings: bool,
}

impl TextDocument {
    /// Builds a line-aware document from one strictly decoded payload.
    /// 从一份严格解码载荷构建换行感知文档。
    pub fn from_decoded(decoded: DecodedText) -> Self {
        // The parser consumes paired endings longest-first so CRLF and LFCR each form one boundary.
        // 解析器按最长配对优先消费换行，因此 CRLF 与 LFCR 各自形成一个边界。
        let lines = parse_document_lines(&decoded.text);
        let (preferred_line_ending, mixed_line_endings) = line_ending_profile(&lines);
        Self {
            text: decoded.text,
            encoding: decoded.encoding,
            bom: decoded.bom,
            encoding_source: decoded.source,
            encoding_certain: decoded.certain,
            lines,
            preferred_line_ending,
            mixed_line_endings,
        }
    }

    /// Builds a UTF-8 document from newly supplied text.
    /// 从新提供的文本构建 UTF-8 文档。
    pub fn new_utf8(text: String) -> Self {
        Self::from_decoded(DecodedText {
            text,
            encoding: TextEncoding::Utf8,
            bom: TextBom::None,
            source: TextEncodingSource::Explicit,
            certain: true,
        })
    }

    /// Returns logical line content without textual line-number prefixes.
    /// 返回不含文本行号前缀的逻辑行正文。
    pub fn logical_lines(&self) -> Vec<String> {
        self.lines.iter().map(|line| line.content.clone()).collect()
    }

    /// Returns whether the text ends with any recognized terminator.
    /// 返回文本是否以任一已识别终止符结尾。
    pub fn final_newline(&self) -> bool {
        self.lines.last().is_some_and(|line| line.ending.is_some())
    }

    /// Returns one stable line-ending summary label.
    /// 返回稳定的换行摘要标签。
    pub fn line_ending_label(&self) -> &'static str {
        if self.mixed_line_endings {
            "mixed"
        } else {
            self.preferred_line_ending.label()
        }
    }

    /// Encodes the current text with its exact source encoding and BOM.
    /// 使用精确源编码与 BOM 编码当前文本。
    pub fn encode(&self) -> Result<Vec<u8>, TextCodecError> {
        encode_text(&self.text, self.encoding, self.bom)
    }

    /// Returns a copy with every logical terminator normalized to one style.
    /// 返回将全部逻辑终止符归一化为一种样式的副本。
    pub fn with_uniform_line_ending(&self, ending: LineEnding) -> Self {
        let text = rewrite_line_endings(&self.text, ending);
        let mut document = Self::from_decoded(DecodedText {
            text,
            encoding: self.encoding,
            bom: self.bom,
            source: self.encoding_source,
            certain: self.encoding_certain,
        });
        document.preferred_line_ending = ending;
        document
    }

    /// Replaces one exact normalized substring while preserving untouched terminators.
    /// 替换一个精确归一化子串，同时保留未触及的终止符。
    pub fn replace_exact(
        &self,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<TextReplacement, TextDocumentError> {
        let normalized_old = normalize_line_endings(old_string);
        if normalized_old.is_empty() {
            return Err(TextDocumentError::new(
                "invalid_arguments",
                "old_string must not be empty.",
            ));
        }
        let normalized_new = normalize_line_endings(new_string);
        let (normalized_source, boundaries) = normalized_text_with_boundaries(&self.text);
        // match_indices deliberately uses non-overlapping matches, matching common AI edit-tool semantics.
        // match_indices 故意使用非重叠匹配，与常见 AI 编辑工具语义一致。
        let matches = normalized_source
            .match_indices(&normalized_old)
            .map(|(start, _)| (start, start + normalized_old.len()))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(TextDocumentError::new(
                "match_not_found",
                "old_string was not found in the decoded file. Read the current file and retry with an exact substring.",
            ));
        }
        if matches.len() > 1 && !replace_all {
            return Err(TextDocumentError::with_candidates(
                "ambiguous_match",
                "old_string matches more than once. Include more surrounding text or set replace_all=true.",
                matches.len(),
                matches
                    .iter()
                    .take(MAX_ERROR_CANDIDATE_RANGES)
                    .map(|(start, end)| {
                        // Match offsets address normalized Unicode text, so CRLF and multibyte text retain source lines.
                        // 匹配偏移指向归一化 Unicode 文本，因此 CRLF 和多字节文本保留源行坐标。
                        let first = line_number_at_offset(&normalized_source, *start) - 1;
                        (
                            first,
                            first + logical_span_line_count(&normalized_source[*start..*end]),
                        )
                    })
                    .collect(),
            ));
        }
        let selected = if replace_all {
            matches
        } else {
            matches.into_iter().take(1).collect()
        };
        let mut original_ranges = selected
            .iter()
            .map(|(start, end)| {
                Ok((
                    original_offset_for_boundary(&boundaries, *start)?,
                    original_offset_for_boundary(&boundaries, *end)?,
                ))
            })
            .collect::<Result<Vec<_>, TextDocumentError>>()?;
        let mut updated = self.text.clone();
        for (start, end) in original_ranges.iter().rev().copied() {
            let local_ending =
                local_line_ending(&self.text, start, end).unwrap_or(self.preferred_line_ending);
            let replacement = rewrite_replacement_line_endings(
                &normalized_new,
                &self.text[start..end],
                local_ending,
            );
            updated.replace_range(start..end, &replacement);
        }
        original_ranges.sort_unstable();
        let decoded = DecodedText {
            text: updated,
            encoding: self.encoding,
            bom: self.bom,
            source: self.encoding_source,
            certain: self.encoding_certain,
        };
        let document = Self::from_decoded(decoded);
        let ranges = selected
            .iter()
            .map(|(start, end)| TextReplacementRange {
                original_start_line: line_number_at_offset(&normalized_source, *start),
                original_line_count: logical_span_line_count(&normalized_source[*start..*end]),
                final_line_count: logical_span_line_count(&normalized_new),
            })
            .collect::<Vec<_>>();
        Ok(TextReplacement {
            document,
            match_count: selected.len(),
            ranges,
        })
    }
}

/// One applied exact-text replacement result.
/// 一份已应用的精确文本替换结果。
#[derive(Debug, Clone)]
pub struct TextReplacement {
    /// Final line-aware document.
    /// 最终换行感知文档。
    pub document: TextDocument,
    /// Number of replaced non-overlapping matches.
    /// 已替换的非重叠匹配数量。
    pub match_count: usize,
    /// Stable ranges for result projection.
    /// 用于结果投影的稳定范围。
    pub ranges: Vec<TextReplacementRange>,
}

/// One logical range changed by an exact-text replacement.
/// 一段由精确文本替换改变的逻辑范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextReplacementRange {
    /// One-based original start line.
    /// 一基原始起始行。
    pub original_start_line: usize,
    /// Number of original logical lines covered by the substring.
    /// 子串覆盖的原始逻辑行数。
    pub original_line_count: usize,
    /// Number of final logical lines introduced by the substring.
    /// 子串引入的最终逻辑行数。
    pub final_line_count: usize,
}

/// One line-document operation failure.
/// 一条行文档操作失败。
#[derive(Debug, Clone)]
pub struct TextDocumentError {
    /// Stable machine-readable code.
    /// 稳定的机器可读错误码。
    pub code: &'static str,
    /// English model-facing message.
    /// 面向模型的英文消息。
    pub message: String,
    /// Optional match count for ambiguity diagnostics.
    /// 用于歧义诊断的可选匹配数量。
    pub match_count: Option<usize>,
    /// Bounded zero-based source ranges; None means candidate discovery was not performed.
    /// 有界的零基源范围；None 表示未进行候选发现。
    pub candidate_ranges: Option<Vec<(usize, usize)>>,
}

impl TextDocumentError {
    /// Creates one document error without a match count.
    /// 创建一条不含匹配数量的文档错误。
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            match_count: None,
            candidate_ranges: None,
        }
    }

    /// Creates an ambiguity error from the total count and bounded source ranges; returns its diagnostic value.
    /// 从总匹配数与有界源范围创建歧义错误，返回诊断值。
    fn with_candidates(
        code: &'static str,
        message: impl Into<String>,
        count: usize,
        ranges: Vec<(usize, usize)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            match_count: Some(count),
            candidate_ranges: Some(ranges),
        }
    }
}

impl Display for TextDocumentError {
    /// Renders one stable document error.
    /// 渲染一条稳定文档错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for TextDocumentError {}

/// Parses logical lines and consumes paired terminators longest-first.
/// 解析逻辑行并按最长优先消费配对终止符。
pub fn parse_document_lines(text: &str) -> Vec<DocumentLine> {
    if text.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let ending = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => Some((LineEnding::CrLf, 2)),
            b'\n' if bytes.get(index + 1) == Some(&b'\r') => Some((LineEnding::LfCr, 2)),
            b'\r' => Some((LineEnding::Cr, 1)),
            b'\n' => Some((LineEnding::Lf, 1)),
            _ => None,
        };
        if let Some((line_ending, width)) = ending {
            lines.push(DocumentLine {
                content: text[line_start..index].to_string(),
                ending: Some(line_ending),
            });
            index += width;
            line_start = index;
        } else {
            // Advancing by a Unicode scalar keeps every slice boundary valid UTF-8.
            // 按 Unicode 标量推进，确保每个切片边界都是有效 UTF-8。
            index += text[index..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
        }
    }
    if line_start < text.len() {
        lines.push(DocumentLine {
            content: text[line_start..].to_string(),
            ending: None,
        });
    }
    lines
}

/// Normalizes CRLF, LFCR, CR, and LF to one LF token.
/// 将 CRLF、LFCR、CR 与 LF 归一化为一个 LF 标记。
pub fn normalize_line_endings(text: &str) -> String {
    rewrite_line_endings(text, LineEnding::Lf)
}

/// Rewrites every recognized logical terminator to one requested style.
/// 将每个已识别逻辑终止符改写为请求的样式。
pub fn rewrite_line_endings(text: &str, ending: LineEnding) -> String {
    let lines = parse_document_lines(text);
    let mut output = String::with_capacity(text.len());
    for line in lines {
        output.push_str(&line.content);
        if line.ending.is_some() {
            output.push_str(ending.as_str());
        }
    }
    output
}

/// Rewrites normalized replacement text while inheriting corresponding source line endings.
/// 重写已归一化的替换文本，并逐行继承对应源片段的行尾。
pub(crate) fn rewrite_replacement_line_endings(
    normalized_replacement: &str,
    original_slice: &str,
    fallback_ending: LineEnding,
) -> String {
    let original_lines = parse_document_lines(original_slice);
    let replacement_lines = parse_document_lines(normalized_replacement);
    let mut inherited_indexes = vec![None; replacement_lines.len()];
    let mut original_used = vec![false; original_lines.len()];
    let mut original_cursor = 0usize;
    for (replacement_index, replacement) in replacement_lines.iter().enumerate() {
        let matching_index = original_lines
            .iter()
            .enumerate()
            .skip(original_cursor)
            .find_map(|(index, original)| {
                (original.content == replacement.content).then_some(index)
            });
        if let Some(original_index) = matching_index {
            inherited_indexes[replacement_index] = Some(original_index);
            original_used[original_index] = true;
            original_cursor = original_index.saturating_add(1);
        }
    }
    let remaining_original = original_used
        .iter()
        .enumerate()
        .filter_map(|(index, used)| (!used).then_some(index))
        .collect::<Vec<_>>();
    let mut remaining_original = remaining_original.into_iter();
    for inherited_index in &mut inherited_indexes {
        if inherited_index.is_none() {
            *inherited_index = remaining_original.next();
        }
    }
    let mut output = String::with_capacity(normalized_replacement.len());
    for (index, line) in replacement_lines.into_iter().enumerate() {
        output.push_str(&line.content);
        if line.ending.is_some() {
            let inherited = inherited_indexes[index]
                .and_then(|original_index| original_lines.get(original_index))
                .and_then(|original| original.ending)
                .unwrap_or(fallback_ending);
            output.push_str(inherited.as_str());
        }
    }
    output
}

/// Computes the preferred ending and whether the source is mixed.
/// 计算首选换行符以及源文本是否混合换行。
fn line_ending_profile(lines: &[DocumentLine]) -> (LineEnding, bool) {
    let endings = lines
        .iter()
        .filter_map(|line| line.ending)
        .collect::<Vec<_>>();
    if endings.is_empty() {
        return (LineEnding::Lf, false);
    }
    let candidates = [
        LineEnding::CrLf,
        LineEnding::LfCr,
        LineEnding::Lf,
        LineEnding::Cr,
    ];
    // A strict-greater fold preserves the declared priority when counts tie.
    // 严格大于折叠会在计数相同时保留声明的优先顺序。
    let preferred = candidates
        .into_iter()
        .reduce(|preferred, candidate| {
            let preferred_count = endings
                .iter()
                .filter(|ending| **ending == preferred)
                .count();
            let candidate_count = endings
                .iter()
                .filter(|ending| **ending == candidate)
                .count();
            if candidate_count > preferred_count {
                candidate
            } else {
                preferred
            }
        })
        .unwrap_or(LineEnding::Lf);
    let mixed = endings.iter().any(|ending| *ending != preferred);
    (preferred, mixed)
}

/// Builds normalized text and exact normalized-to-original boundary mappings.
/// 构建归一化文本及精确的归一化到原文边界映射。
fn normalized_text_with_boundaries(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut normalized = String::with_capacity(text.len());
    let mut boundaries = vec![(0usize, 0usize)];
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let ending_width = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => Some(2),
            b'\n' if bytes.get(index + 1) == Some(&b'\r') => Some(2),
            b'\r' | b'\n' => Some(1),
            _ => None,
        };
        if let Some(width) = ending_width {
            normalized.push('\n');
            index += width;
            boundaries.push((normalized.len(), index));
        } else if let Some(character) = text[index..].chars().next() {
            normalized.push(character);
            index += character.len_utf8();
            boundaries.push((normalized.len(), index));
        } else {
            break;
        }
    }
    (normalized, boundaries)
}

/// Resolves one normalized boundary to its exact original UTF-8 byte offset.
/// 将一个归一化边界解析为精确的原始 UTF-8 字节偏移。
fn original_offset_for_boundary(
    boundaries: &[(usize, usize)],
    normalized_offset: usize,
) -> Result<usize, TextDocumentError> {
    boundaries
        .binary_search_by_key(&normalized_offset, |(normalized, _)| *normalized)
        .ok()
        .map(|index| boundaries[index].1)
        .ok_or_else(|| {
            TextDocumentError::new(
                "invalid_arguments",
                "The normalized edit boundary was not a Unicode scalar boundary.",
            )
        })
}

/// Chooses the closest existing terminator for inserted text at one range.
/// 为一个范围内的插入文本选择最近的现有终止符。
fn local_line_ending(text: &str, start: usize, end: usize) -> Option<LineEnding> {
    first_line_ending(&text[start..end])
        .or_else(|| last_line_ending(&text[..start]))
        .or_else(|| first_line_ending(&text[end..]))
}

/// Returns the first recognized terminator in text.
/// 返回文本中的第一个已识别终止符。
fn first_line_ending(text: &str) -> Option<LineEnding> {
    parse_document_lines(text)
        .into_iter()
        .find_map(|line| line.ending)
}

/// Returns the last recognized terminator in text.
/// 返回文本中的最后一个已识别终止符。
fn last_line_ending(text: &str) -> Option<LineEnding> {
    parse_document_lines(text)
        .into_iter()
        .filter_map(|line| line.ending)
        .next_back()
}

/// Returns the one-based logical line containing a normalized byte offset.
/// 返回包含归一化字节偏移的一基逻辑行。
fn line_number_at_offset(normalized: &str, offset: usize) -> usize {
    normalized[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        .saturating_add(1)
}

/// Returns the logical-line span represented by normalized text.
/// 返回归一化文本表示的逻辑行跨度。
fn logical_span_line_count(normalized: &str) -> usize {
    if normalized.is_empty() {
        0
    } else {
        normalized
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            .saturating_add(usize::from(!normalized.ends_with('\n')))
    }
}

#[cfg(test)]
mod tests {
    use super::{LineEnding, TextDocument, normalize_line_endings, parse_document_lines};

    /// Verifies every supported terminator, including LFCR, forms exactly one logical boundary.
    /// 验证包括 LFCR 在内的每种终止符都只形成一个逻辑边界。
    #[test]
    fn parses_all_line_endings_longest_first() {
        let lines = parse_document_lines("a\r\nb\n\rc\rd\ne");
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].ending, Some(LineEnding::CrLf));
        assert_eq!(lines[1].ending, Some(LineEnding::LfCr));
        assert_eq!(lines[2].ending, Some(LineEnding::Cr));
        assert_eq!(lines[3].ending, Some(LineEnding::Lf));
        assert_eq!(lines[4].ending, None);
        assert_eq!(normalize_line_endings("a\r\nb\n\rc\rd\ne"), "a\nb\nc\nd\ne");
    }

    /// Verifies equal ending counts use the declared stable priority instead of iterator tie behavior.
    /// 验证换行数量相同时使用声明的稳定优先级，而不依赖迭代器并列行为。
    #[test]
    fn preferred_line_ending_uses_stable_priority_for_ties() {
        let document = TextDocument::new_utf8(String::from("a\rb\nc\n\rd\r\n"));

        assert_eq!(document.preferred_line_ending, LineEnding::CrLf);
        assert!(document.mixed_line_endings);
    }

    /// Verifies localized replacement preserves untouched mixed terminators.
    /// 验证局部替换保留未触及的混合终止符。
    #[test]
    fn exact_replacement_preserves_unaffected_mixed_endings() {
        let document = TextDocument::new_utf8(String::from("a\r\nb\n\rc\rd"));
        let replacement = document
            .replace_exact("b\nc", "B\nC", false)
            .expect("the normalized block should match");
        assert_eq!(replacement.document.text, "a\r\nB\n\rC\rd");
        assert_eq!(replacement.match_count, 1);
    }

    /// Verifies unchanged context inside a replacement inherits each corresponding mixed ending.
    /// 验证替换片段内部的未改上下文会逐行继承各自对应的混合行尾。
    #[test]
    fn exact_replacement_inherits_mixed_endings_line_by_line() {
        let document = TextDocument::new_utf8(String::from("a\r\nb\n\rc\rd"));
        let replacement = document
            .replace_exact("a\nb\nc", "A\nb\nc", false)
            .expect("the normalized mixed-ending block should match");

        assert_eq!(replacement.document.text, "A\r\nb\n\rc\rd");
    }

    /// Verifies newly inserted logical lines use the local ending after inherited boundaries run out.
    /// 验证对应继承边界用尽后，新插入的逻辑行会使用局部行尾。
    #[test]
    fn exact_replacement_uses_local_ending_for_extra_lines() {
        let document = TextDocument::new_utf8(String::from("a\r\nb\rd"));
        let replacement = document
            .replace_exact("a\nb", "A\nB\nC", false)
            .expect("the normalized source block should match");

        assert_eq!(replacement.document.text, "A\r\nB\r\nC\rd");
    }

    /// Verifies inserted lines do not shift unchanged context away from its original endings.
    /// 验证插入行不会使未变化上下文偏离各自原有行尾。
    #[test]
    fn exact_replacement_aligns_context_before_inheriting_endings() {
        let document = TextDocument::new_utf8(String::from("a\r\nb\n\rc\rd"));
        let replacement = document
            .replace_exact("a\nb\nc", "inserted\na\nb\nc", false)
            .expect("the normalized source block should match");

        assert_eq!(replacement.document.text, "inserted\r\na\r\nb\n\rc\rd");
    }

    /// Verifies duplicate exact matches require explicit replace_all.
    /// 验证重复精确匹配要求显式 replace_all。
    #[test]
    fn duplicate_match_requires_replace_all() {
        let document = TextDocument::new_utf8(String::from("same\nsame"));
        let error = document
            .replace_exact("same", "next", false)
            .expect_err("duplicate matches must be rejected");
        assert_eq!(error.code, "ambiguous_match");
        assert_eq!(error.match_count, Some(2));
    }
}
