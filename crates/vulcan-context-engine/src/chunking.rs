//! Deterministic source-preserving fallback chunking.
//! 保留源码文本和行号的确定性回退切块。

use crate::identity::normalize_relative_path;
use crate::model::{Chunk, ChunkError, ChunkKind};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Configuration for the recursive fallback chunker.
/// 递归回退切块器的配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursiveChunkerConfig {
    /// Maximum Unicode scalar values in a normal Chunk.
    /// 普通代码块允许的最大 Unicode 标量值数量。
    pub max_chars: usize,
    /// Minimum Unicode scalar values for an emitted Chunk.
    /// 产出代码块允许的最小 Unicode 标量值数量。
    pub min_chars: usize,
}

impl Default for RecursiveChunkerConfig {
    /// Return the documented OCE-inspired default chunk budget.
    /// 返回 OCE 思路中记录的默认代码块预算。
    fn default() -> Self {
        Self {
            max_chars: 6_000,
            min_chars: 1,
        }
    }
}

/// A source-preserving recursive fallback chunker.
/// 一个保留源码文本的递归回退切块器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursiveChunker {
    /// Explicit chunking limits used by this instance.
    /// 当前实例使用的明确切块限制。
    pub config: RecursiveChunkerConfig,
}

impl RecursiveChunker {
    /// Construct a chunker after validating its character budgets.
    /// 校验字符预算后创建切块器。
    pub fn new(config: RecursiveChunkerConfig) -> Result<Self, ChunkerError> {
        if config.max_chars == 0 || config.min_chars == 0 || config.min_chars > config.max_chars {
            return Err(ChunkerError::InvalidConfig);
        }
        Ok(Self { config })
    }

    /// Chunk one Blob while retaining exact source text and one-based line ranges.
    /// 对一个 Blob 进行切块，同时保留精确源码文本和从一开始的行号范围。
    pub fn chunk(
        &self,
        blob_name: impl Into<String>,
        path: &str,
        source: &str,
    ) -> Result<Vec<Chunk>, ChunkerError> {
        let normalized_path = normalize_relative_path(path).map_err(ChunkerError::InvalidPath)?;
        if source.is_empty() || !contains_alphanumeric(source) {
            return Ok(Vec::new());
        }

        let blob_name = blob_name.into();
        let lines = line_spans(source);
        let mut chunks = Vec::new();
        let mut line_index = 0usize;
        while line_index < lines.len() {
            let current = lines[line_index];
            let current_chars = char_count(&source[current.start_byte..current.end_byte]);
            if current_chars > self.config.max_chars {
                self.emit_long_line(&mut chunks, &blob_name, &normalized_path, source, current)?;
                line_index += 1;
                continue;
            }

            let range_end = self.choose_line_end(source, &lines, line_index);
            let first = lines[line_index];
            let last = lines[range_end - 1];
            let content = source[first.start_byte..last.end_byte].to_owned();
            if self.is_emittable(&content) {
                chunks.push(
                    Chunk::new(
                        blob_name.clone(),
                        &normalized_path,
                        content,
                        first.line_number,
                        last.line_number,
                        ChunkKind::Recursive,
                    )
                    .map_err(ChunkerError::InvalidChunk)?,
                );
            }
            line_index = range_end;
        }
        Ok(chunks)
    }

    /// Choose the largest readable line boundary under the configured character budget.
    /// 在字符预算内选择最大的可读行边界。
    fn choose_line_end(&self, source: &str, lines: &[LineSpan], start: usize) -> usize {
        let mut end = start;
        let mut total_chars = 0usize;
        let mut last_blank_boundary = None;
        while end < lines.len() {
            let line = lines[end];
            let line_chars = char_count(&source[line.start_byte..line.end_byte]);
            if end > start && total_chars + line_chars > self.config.max_chars {
                break;
            }
            if end == start && line_chars > self.config.max_chars {
                break;
            }
            total_chars += line_chars;
            end += 1;
            if source[line.start_byte..line.content_end_byte]
                .trim()
                .is_empty()
            {
                last_blank_boundary = Some(end);
            }
        }

        let selected = last_blank_boundary
            .filter(|boundary| *boundary > start)
            .unwrap_or(end);
        selected.max(start + 1)
    }

    /// Split one overlong source line into exact character-aligned source fragments.
    /// 将超长源码行切分为精确字符边界的源码片段。
    fn emit_long_line(
        &self,
        chunks: &mut Vec<Chunk>,
        blob_name: &str,
        path: &str,
        source: &str,
        line: LineSpan,
    ) -> Result<(), ChunkerError> {
        let line_content = &source[line.start_byte..line.end_byte];
        let positions = line_content.char_indices().collect::<Vec<_>>();
        let char_total = positions.len();
        let mut start_char = 0usize;
        while start_char < char_total {
            let end_char = start_char
                .saturating_add(self.config.max_chars)
                .min(char_total);
            let start_byte = line.start_byte + positions[start_char].0;
            let end_byte = if end_char == char_total {
                line.end_byte
            } else {
                line.start_byte + positions[end_char].0
            };
            let content = source[start_byte..end_byte].to_owned();
            if self.is_emittable(&content) {
                let chunk = Chunk::new_with_start_char_offset(
                    blob_name.to_owned(),
                    start_char,
                    path,
                    content,
                    line.line_number,
                    line.line_number,
                    ChunkKind::Recursive,
                )
                .map_err(ChunkerError::InvalidChunk)?;
                chunks.push(chunk);
            }
            start_char = end_char;
        }
        Ok(())
    }

    /// Check whether a Chunk has enough meaningful text to enter retrieval.
    /// 判断代码块是否包含足够有意义的文本以进入检索。
    fn is_emittable(&self, content: &str) -> bool {
        char_count(content) >= self.config.min_chars && contains_alphanumeric(content)
    }
}

/// Errors raised by the source-preserving chunker.
/// 保留源码切块器产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkerError {
    /// The chunk character configuration is invalid.
    /// 切块字符配置无效。
    InvalidConfig,
    /// The input path is not a workspace-relative path.
    /// 输入路径不是工作区相对路径。
    InvalidPath(crate::identity::PathError),
    /// A generated Chunk could not be constructed.
    /// 生成的代码块无法构建。
    InvalidChunk(ChunkError),
}

impl Display for ChunkerError {
    /// Formats a chunking error for deterministic diagnostics.
    /// 将切块错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig => {
                formatter.write_str("recursive chunker configuration is invalid")
            }
            Self::InvalidPath(error) => write!(formatter, "invalid chunker path: {error}"),
            Self::InvalidChunk(error) => write!(formatter, "generated Chunk is invalid: {error}"),
        }
    }
}

impl Error for ChunkerError {}

/// Store byte ranges for source lines without copying source content.
/// 保存源码行的字节范围且不复制源码内容。
#[derive(Debug, Clone, Copy)]
struct LineSpan {
    /// One-based line number.
    /// 从一开始的行号。
    line_number: usize,
    /// Byte offset including the line start.
    /// 包含行起点的字节偏移。
    start_byte: usize,
    /// Byte offset after the line terminator when present.
    /// 存在行结束符时包含行结束符之后的字节偏移。
    end_byte: usize,
    /// Byte offset before the line terminator.
    /// 行结束符之前的字节偏移。
    content_end_byte: usize,
}

/// Build source line spans with exact UTF-8 byte boundaries.
/// 按精确 UTF-8 字节边界构建源码行范围。
fn line_spans(source: &str) -> Vec<LineSpan> {
    let mut spans = Vec::new();
    let mut start_byte = 0usize;
    let mut line_number = 1usize;
    for (byte_index, character) in source.char_indices() {
        if character == '\n' {
            spans.push(LineSpan {
                line_number,
                start_byte,
                end_byte: byte_index + character.len_utf8(),
                content_end_byte: byte_index,
            });
            start_byte = byte_index + character.len_utf8();
            line_number += 1;
        }
    }
    if start_byte < source.len() {
        spans.push(LineSpan {
            line_number,
            start_byte,
            end_byte: source.len(),
            content_end_byte: source.len(),
        });
    }
    spans
}

/// Count Unicode scalar values without treating bytes as characters.
/// 统计 Unicode 标量值数量，而不是把字节当作字符。
fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// Return whether text contains at least one Unicode letter or number.
/// 判断文本是否至少包含一个 Unicode 字母或数字。
fn contains_alphanumeric(text: &str) -> bool {
    text.chars().any(|character| character.is_alphanumeric())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Verify emitted Chunk text is a verbatim source slice with correct lines.
    /// 验证产出的代码块文本是原始源码切片且行号正确。
    #[test]
    fn preserves_source_text_and_line_ranges() {
        let chunker = RecursiveChunker::new(RecursiveChunkerConfig {
            max_chars: 30,
            min_chars: 1,
        })
        .unwrap();
        let source = "fn first() {}\n\nfn second() {\n    true\n}\n";
        let chunks = chunker.chunk("blob", "src/lib.rs", source).unwrap();
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].range.start_line, 1);
        assert!(source.contains(&chunks[0].content));
    }

    /// Verify overlong lines are split without invalid UTF-8 boundaries.
    /// 验证超长行会被切分且不会破坏 UTF-8 字符边界。
    #[test]
    fn splits_unicode_long_lines_safely() {
        let chunker = RecursiveChunker::new(RecursiveChunkerConfig {
            max_chars: 3,
            min_chars: 1,
        })
        .unwrap();
        let chunks = chunker.chunk("blob", "src/lib.rs", "你好世界").unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content, "你好世");
        assert_eq!(chunks[1].content, "界");
        assert_eq!(chunks[0].range.start_line, chunks[1].range.start_line);
    }

    /// Verify an unbounded configured maximum cannot overflow later fragment offsets.
    /// 验证无界配置上限不会使后续片段偏移发生溢出。
    #[test]
    fn supports_maximum_character_budget_without_overflow() {
        let chunker = RecursiveChunker::new(RecursiveChunkerConfig {
            max_chars: usize::MAX,
            min_chars: 1,
        })
        .unwrap();
        let chunks = chunker.chunk("blob", "src/lib.rs", "fn main() {}").unwrap();

        assert_eq!(chunks.len(), 1);
    }

    /// Verify line terminators consume the same hard character budget as source text.
    /// 验证行终止符与源码正文共同占用同一个硬字符预算。
    #[test]
    fn line_terminators_respect_chunk_character_budget() {
        let chunker = RecursiveChunker::new(RecursiveChunkerConfig {
            max_chars: 3,
            min_chars: 1,
        })
        .unwrap();
        let source = "abcde\nxy\n";
        let chunks = chunker.chunk("blob", "src/lib.rs", source).unwrap();

        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.content.chars().count() <= 3)
        );
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.content.as_str())
                .collect::<String>(),
            source
        );
    }

    /// Verify repeated fragments on one overlong line do not collide during indexing.
    /// 验证超长单行上的重复片段不会在索引阶段产生身份碰撞。
    #[test]
    fn distinguishes_repeated_long_line_fragments() {
        let chunker = RecursiveChunker::new(RecursiveChunkerConfig {
            max_chars: 4,
            min_chars: 1,
        })
        .unwrap();
        let chunks = chunker.chunk("blob", "src/lib.rs", "abc1abc1").unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content, "abc1");
        assert_eq!(chunks[1].content, "abc1");
        assert_eq!(chunks[0].start_char_offset, 0);
        assert_eq!(chunks[1].start_char_offset, 4);
        assert_ne!(chunks[0].chunk_id, chunks[1].chunk_id);
    }

    /// Verify empty and punctuation-only sources do not create retrieval chunks.
    /// 验证空内容和纯标点内容不会产生检索代码块。
    #[test]
    fn skips_non_semantic_content() {
        let chunker = RecursiveChunker::new(RecursiveChunkerConfig::default()).unwrap();
        assert!(
            chunker
                .chunk("blob", "src/lib.rs", "   \n{}[]")
                .unwrap()
                .is_empty()
        );
    }
}
