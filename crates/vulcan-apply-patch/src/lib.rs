//! Provider-neutral Apply Patch grammar and streaming parser.
//! 供应商无关的 Apply Patch 语法与流式解析器。

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Canonical Lark grammar exposed to grammar-capable model protocols.
/// 暴露给支持 grammar 的模型协议的规范 Lark 语法。
pub const APPLY_PATCH_LARK_GRAMMAR: &str = r#"start: begin_patch hunk+ end_patch
begin_patch: "*** Begin Patch" LF
end_patch: "*** End Patch" LF?
hunk: add_hunk | delete_hunk | update_hunk
add_hunk: "*** Add File: " filename LF add_line+
delete_hunk: "*** Delete File: " filename LF
update_hunk: "*** Update File: " filename LF change_move? change?
filename: /(.+)/
add_line: "+" /(.*)/ LF -> line
change_move: "*** Move to: " filename LF
change: (change_context | change_line)+ eof_line?
change_context: ("@@" | "@@ " /(.+)/) LF
change_line: ("+" | "-" | " ") /(.*)/ LF
eof_line: "*** End of File" LF
%import common.LF"#;

const BEGIN_PATCH_MARKER: &str = "*** Begin Patch";
const END_PATCH_MARKER: &str = "*** End Patch";
const ADD_FILE_MARKER: &str = "*** Add File: ";
const DELETE_FILE_MARKER: &str = "*** Delete File: ";
const UPDATE_FILE_MARKER: &str = "*** Update File: ";
const MOVE_TO_MARKER: &str = "*** Move to: ";
const EOF_MARKER: &str = "*** End of File";
const CHANGE_CONTEXT_MARKER: &str = "@@ ";
const EMPTY_CHANGE_CONTEXT_MARKER: &str = "@@";

/// One fully parsed file operation in the Apply Patch language.
/// Apply Patch 语言中一个完整解析的文件操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchFile {
    /// Creates one file from ordered logical lines.
    /// 使用有序逻辑行创建一个文件。
    Add {
        /// Requested workspace path.
        /// 请求的工作区路径。
        path: String,
        /// New logical lines without line-ending bytes.
        /// 不含行尾字节的新逻辑行。
        lines: Vec<String>,
    },
    /// Deletes one existing file.
    /// 删除一个已有文件。
    Delete {
        /// Requested workspace path.
        /// 请求的工作区路径。
        path: String,
    },
    /// Updates and optionally moves one existing file.
    /// 更新并可选移动一个已有文件。
    Update {
        /// Requested source path.
        /// 请求的源路径。
        path: String,
        /// Optional move destination.
        /// 可选的移动目标路径。
        move_to: Option<String>,
        /// Ordered update chunks.
        /// 有序更新块。
        hunks: Vec<PatchHunk>,
    },
}

impl PatchFile {
    /// Returns the source or creation path named by the operation.
    /// 返回操作指定的源路径或创建路径。
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Delete { path } | Self::Update { path, .. } => path,
        }
    }
}

/// One ordered update chunk with an optional semantic anchor.
/// 一个带可选语义锚点的有序更新块。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatchHunk {
    /// Optional exact logical-line anchor from an `@@ name` header.
    /// 来自 `@@ name` 头的可选精确逻辑行锚点。
    pub context: Option<String>,
    /// Original logical lines, including unchanged context lines.
    /// 原始逻辑行，包括未改变的上下文行。
    pub old_lines: Vec<String>,
    /// Replacement logical lines, including unchanged context lines.
    /// 替换逻辑行，包括未改变的上下文行。
    pub new_lines: Vec<String>,
    /// Whether this chunk must end at the logical end of the file.
    /// 此更新块是否必须结束于文件逻辑末尾。
    pub end_of_file: bool,
}

impl PatchHunk {
    /// Adds one unchanged context line to both sides of the chunk.
    /// 向更新块两侧添加一条未改变的上下文行。
    fn push_context_line(&mut self, line: String) {
        self.old_lines.push(line.clone());
        self.new_lines.push(line);
    }

    /// Returns whether the chunk has no old or new logical lines.
    /// 返回更新块是否不含任何新旧逻辑行。
    fn is_empty(&self) -> bool {
        self.old_lines.is_empty() && self.new_lines.is_empty()
    }
}

/// A bounded syntax failure produced while parsing Apply Patch text.
/// 解析 Apply Patch 文本时产生的有界语法错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchParseError {
    /// One-based input line when the failure is line-specific.
    /// 错误与具体行相关时的一基输入行号。
    pub line_number: Option<usize>,
    /// Model-facing correction message.
    /// 面向模型的修正消息。
    pub message: String,
}

impl PatchParseError {
    /// Creates one parser error with optional line context.
    /// 创建一条带可选行上下文的解析错误。
    fn new(line_number: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            line_number,
            message: message.into(),
        }
    }
}

impl Display for PatchParseError {
    /// Formats the parser error without embedding patch contents.
    /// 在不嵌入 patch 正文的情况下格式化解析错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self.line_number {
            Some(line_number) => write!(
                formatter,
                "invalid hunk at line {line_number}: {}",
                self.message
            ),
            None => write!(formatter, "invalid patch: {}", self.message),
        }
    }
}

impl Error for PatchParseError {}

/// Parses one complete Apply Patch document.
/// 解析一份完整的 Apply Patch 文档。
pub fn parse_apply_patch(patch: &str) -> Result<Vec<PatchFile>, PatchParseError> {
    let mut parser = StreamingPatchParser::default();
    let _ = parser.push_delta(patch)?;
    parser.finish()
}

/// Incrementally parses complete logical lines from a streamed patch argument.
/// 从流式 patch 参数中增量解析完整逻辑行。
#[derive(Debug, Default, Clone)]
pub struct StreamingPatchParser {
    line_buffer: String,
    state: StreamingParserState,
    line_number: usize,
    paired_newline_tail: Option<char>,
}

/// Mutable semantic state retained by the streaming parser.
/// 流式解析器保留的可变语义状态。
#[derive(Debug, Default, Clone)]
struct StreamingParserState {
    mode: StreamingParserMode,
    files: Vec<PatchFile>,
}

/// Current grammar section consumed by the streaming parser.
/// 流式解析器当前消费的语法区段。
#[derive(Debug, Default, Clone, Copy)]
enum StreamingParserMode {
    /// No begin marker has been consumed.
    /// 尚未消费开始标记。
    #[default]
    NotStarted,
    /// The parser accepts a file-operation header.
    /// 解析器正在等待文件操作头。
    StartedPatch,
    /// The parser is collecting add-file lines.
    /// 解析器正在收集新增文件行。
    AddFile,
    /// The parser requires the next line to be another header.
    /// 解析器要求下一行必须是另一个操作头。
    DeleteFile,
    /// The parser is collecting update hunks.
    /// 解析器正在收集更新块。
    UpdateFile { header_line: usize },
    /// The end marker has been consumed.
    /// 已消费结束标记。
    EndedPatch,
}

impl StreamingPatchParser {
    /// Pushes one arbitrary argument fragment and returns the parsed snapshot.
    /// 推入一个任意参数分片并返回当前已解析快照。
    pub fn push_delta(&mut self, delta: &str) -> Result<Vec<PatchFile>, PatchParseError> {
        for character in delta.chars() {
            if self.paired_newline_tail == Some(character) {
                self.paired_newline_tail = None;
                continue;
            }
            self.paired_newline_tail = None;
            if matches!(character, '\r' | '\n') {
                let line = std::mem::take(&mut self.line_buffer);
                self.line_number = self.line_number.saturating_add(1);
                self.process_line(&line)?;
                self.paired_newline_tail = Some(if character == '\r' { '\n' } else { '\r' });
            } else {
                self.line_buffer.push(character);
            }
        }
        Ok(self.state.files.clone())
    }

    /// Finishes parsing and requires a complete end marker and at least one operation.
    /// 完成解析，并要求完整结束标记及至少一个操作。
    pub fn finish(&mut self) -> Result<Vec<PatchFile>, PatchParseError> {
        self.paired_newline_tail = None;
        if !self.line_buffer.is_empty() {
            let line = std::mem::take(&mut self.line_buffer);
            self.line_number = self.line_number.saturating_add(1);
            self.process_line(&line)?;
        }
        if !matches!(self.state.mode, StreamingParserMode::EndedPatch) {
            return Err(PatchParseError::new(
                None,
                "The last line of the patch must be '*** End Patch'.",
            ));
        }
        if self.state.files.is_empty() {
            return Err(PatchParseError::new(
                None,
                "The patch must contain at least one file operation.",
            ));
        }
        Ok(self.state.files.clone())
    }

    /// Processes one complete logical line according to the active grammar state.
    /// 根据当前语法状态处理一条完整逻辑行。
    fn process_line(&mut self, line: &str) -> Result<(), PatchParseError> {
        let trimmed = line.trim();
        match self.state.mode {
            StreamingParserMode::NotStarted => {
                if trimmed == BEGIN_PATCH_MARKER {
                    self.state.mode = StreamingParserMode::StartedPatch;
                    Ok(())
                } else {
                    Err(PatchParseError::new(
                        Some(self.line_number),
                        "The first line of the patch must be '*** Begin Patch'.",
                    ))
                }
            }
            StreamingParserMode::StartedPatch => {
                if self.handle_file_header_or_end(trimmed)? {
                    Ok(())
                } else {
                    Err(self.invalid_header(trimmed))
                }
            }
            StreamingParserMode::AddFile => {
                if self.handle_file_header_or_end(trimmed)? {
                    return Ok(());
                }
                let added = line
                    .strip_prefix('+')
                    .ok_or_else(|| self.invalid_header(trimmed))?;
                let Some(PatchFile::Add { lines, .. }) = self.state.files.last_mut() else {
                    return Err(PatchParseError::new(
                        None,
                        "Internal add-file parser state is invalid.",
                    ));
                };
                lines.push(added.to_string());
                Ok(())
            }
            StreamingParserMode::DeleteFile => {
                if self.handle_file_header_or_end(trimmed)? {
                    Ok(())
                } else {
                    Err(self.invalid_header(trimmed))
                }
            }
            StreamingParserMode::UpdateFile { header_line } => {
                self.process_update_line(line, header_line)
            }
            StreamingParserMode::EndedPatch => {
                if trimmed.is_empty() {
                    Ok(())
                } else {
                    Err(PatchParseError::new(
                        Some(self.line_number),
                        "No content may follow '*** End Patch'.",
                    ))
                }
            }
        }
    }

    /// Handles a file header or the final patch marker.
    /// 处理文件操作头或最终 patch 标记。
    fn handle_file_header_or_end(&mut self, line: &str) -> Result<bool, PatchParseError> {
        if line == END_PATCH_MARKER {
            self.validate_active_operation()?;
            self.state.mode = StreamingParserMode::EndedPatch;
            return Ok(true);
        }
        if let Some(path) = line.strip_prefix(ADD_FILE_MARKER) {
            self.validate_active_operation()?;
            require_path(path, self.line_number)?;
            self.state.files.push(PatchFile::Add {
                path: path.to_string(),
                lines: Vec::new(),
            });
            self.state.mode = StreamingParserMode::AddFile;
            return Ok(true);
        }
        if let Some(path) = line.strip_prefix(DELETE_FILE_MARKER) {
            self.validate_active_operation()?;
            require_path(path, self.line_number)?;
            self.state.files.push(PatchFile::Delete {
                path: path.to_string(),
            });
            self.state.mode = StreamingParserMode::DeleteFile;
            return Ok(true);
        }
        if let Some(path) = line.strip_prefix(UPDATE_FILE_MARKER) {
            self.validate_active_operation()?;
            require_path(path, self.line_number)?;
            self.state.files.push(PatchFile::Update {
                path: path.to_string(),
                move_to: None,
                hunks: Vec::new(),
            });
            self.state.mode = StreamingParserMode::UpdateFile {
                header_line: self.line_number,
            };
            return Ok(true);
        }
        Ok(false)
    }

    /// Validates the operation being left before accepting another header.
    /// 在接受另一个操作头之前验证即将结束的操作。
    fn validate_active_operation(&self) -> Result<(), PatchParseError> {
        match self.state.files.last() {
            Some(PatchFile::Add { lines, .. }) if lines.is_empty() => Err(PatchParseError::new(
                Some(self.line_number),
                "Add File requires at least one '+' line.",
            )),
            Some(PatchFile::Update { move_to, hunks, .. })
                if move_to.is_none() && hunks.is_empty() =>
            {
                Err(PatchParseError::new(
                    Some(self.line_number),
                    "Update File requires at least one change hunk or Move to header.",
                ))
            }
            Some(PatchFile::Update { hunks, .. })
                if hunks.last().is_some_and(PatchHunk::is_empty) =>
            {
                Err(PatchParseError::new(
                    Some(self.line_number),
                    "Update hunk does not contain any lines.",
                ))
            }
            _ => Ok(()),
        }
    }

    /// Parses one line in an active update-file operation.
    /// 解析活动更新文件操作中的一行。
    fn process_update_line(
        &mut self,
        line: &str,
        header_line: usize,
    ) -> Result<(), PatchParseError> {
        let update_line = line.trim_end();
        if self.handle_file_header_or_end(update_line)? {
            return Ok(());
        }
        let Some(PatchFile::Update { move_to, hunks, .. }) = self.state.files.last_mut() else {
            return Err(PatchParseError::new(
                None,
                "Internal update parser state is invalid.",
            ));
        };
        if hunks.last().is_some_and(|hunk| hunk.end_of_file) {
            return Err(PatchParseError::new(
                Some(self.line_number),
                "No change lines or hunk headers may follow '*** End of File'.",
            ));
        }
        if hunks.is_empty()
            && move_to.is_none()
            && let Some(destination) = update_line.strip_prefix(MOVE_TO_MARKER)
        {
            require_path(destination, self.line_number)?;
            *move_to = Some(destination.to_string());
            self.state.mode = StreamingParserMode::UpdateFile { header_line };
            return Ok(());
        }
        if update_line == EMPTY_CHANGE_CONTEXT_MARKER
            || update_line.starts_with(CHANGE_CONTEXT_MARKER)
        {
            if hunks.last().is_some_and(PatchHunk::is_empty) {
                return Err(PatchParseError::new(
                    Some(self.line_number),
                    "Update hunk does not contain any lines.",
                ));
            }
            let context = update_line
                .strip_prefix(CHANGE_CONTEXT_MARKER)
                .map(str::to_string);
            hunks.push(PatchHunk {
                context,
                ..PatchHunk::default()
            });
            self.state.mode = StreamingParserMode::UpdateFile { header_line };
            return Ok(());
        }
        if update_line == EOF_MARKER {
            let hunk = hunks.last_mut().ok_or_else(|| {
                PatchParseError::new(
                    Some(self.line_number),
                    "'*** End of File' must follow a non-empty update hunk.",
                )
            })?;
            if hunk.is_empty() || hunk.end_of_file {
                return Err(PatchParseError::new(
                    Some(self.line_number),
                    "'*** End of File' must follow one non-empty hunk and may appear once.",
                ));
            }
            hunk.end_of_file = true;
            self.state.mode = StreamingParserMode::UpdateFile { header_line };
            return Ok(());
        }
        if hunks.is_empty() {
            hunks.push(PatchHunk::default());
        }
        let Some(hunk) = hunks.last_mut() else {
            return Err(PatchParseError::new(
                None,
                "Internal update hunk state is invalid.",
            ));
        };
        if hunk.end_of_file {
            return Err(PatchParseError::new(
                Some(self.line_number),
                "No change lines may follow '*** End of File'.",
            ));
        }
        if line.is_empty() {
            hunk.push_context_line(String::new());
        } else if let Some(context) = line.strip_prefix(' ') {
            hunk.push_context_line(context.to_string());
        } else if let Some(added) = line.strip_prefix('+') {
            hunk.new_lines.push(added.to_string());
        } else if let Some(deleted) = line.strip_prefix('-') {
            hunk.old_lines.push(deleted.to_string());
        } else {
            return Err(PatchParseError::new(
                Some(self.line_number),
                "Every update line must start with a space, '+', or '-'.",
            ));
        }
        self.state.mode = StreamingParserMode::UpdateFile { header_line };
        Ok(())
    }

    /// Builds the standard correction message for an unknown operation header.
    /// 为未知操作头构造标准修正消息。
    fn invalid_header(&self, line: &str) -> PatchParseError {
        PatchParseError::new(
            Some(self.line_number),
            format!(
                "'{line}' is not a valid file header; use Add File, Delete File, or Update File."
            ),
        )
    }
}

/// Rejects an empty path in an operation or move header.
/// 拒绝操作头或移动头中的空路径。
fn require_path(path: &str, line_number: usize) -> Result<(), PatchParseError> {
    if path.trim().is_empty() {
        Err(PatchParseError::new(
            Some(line_number),
            "Patch file paths must not be empty.",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PatchFile, StreamingPatchParser, parse_apply_patch};

    /// Verifies all four supported logical line endings parse identically.
    /// 验证四种受支持的逻辑行尾会得到相同解析结果。
    #[test]
    fn complete_parser_accepts_all_line_endings() {
        for separator in ["\n", "\r\n", "\n\r", "\r"] {
            let patch = [
                "*** Begin Patch",
                "*** Update File: sample.txt",
                "@@ function greet()",
                "-old",
                "+new",
                "*** End Patch",
            ]
            .join(separator);
            let files = parse_apply_patch(&patch).expect("patch should parse");
            assert_eq!(files.len(), 1);
        }
    }

    /// Verifies character-at-a-time streaming produces stable multi-file operations.
    /// 验证逐字符流式输入会产生稳定的多文件操作。
    #[test]
    fn streaming_parser_accepts_character_fragments() {
        for separator in ["\n", "\r\n", "\n\r", "\r"] {
            let patch = [
                "*** Begin Patch",
                "*** Add File: one.txt",
                "+one",
                "*** Update File: two.txt",
                "@@ fn two()",
                "-old",
                "+new",
                "*** Delete File: three.txt",
                "*** End Patch",
            ]
            .join(separator);
            let mut parser = StreamingPatchParser::default();
            for character in patch.chars() {
                let _ = parser
                    .push_delta(&character.to_string())
                    .expect("stream fragment should parse");
            }
            let files = parser.finish().expect("stream should finish");
            assert_eq!(files.len(), 3);
            assert!(matches!(files[0], PatchFile::Add { .. }));
            assert!(matches!(files[1], PatchFile::Update { .. }));
            assert!(matches!(files[2], PatchFile::Delete { .. }));
        }
    }

    /// Verifies indented marker text remains an update context line.
    /// 验证带缩进的标记文本仍被视为更新上下文行。
    #[test]
    fn indented_file_marker_remains_context() {
        let patch = "*** Begin Patch\n*** Update File: sample.txt\n@@\n-old\n+new\n *** Update File: literal.txt\n*** End Patch";
        let files = parse_apply_patch(patch).expect("patch should parse");
        let PatchFile::Update { hunks, .. } = &files[0] else {
            panic!("expected update operation");
        };
        assert_eq!(
            hunks[0].old_lines.last().map(String::as_str),
            Some("*** Update File: literal.txt")
        );
        assert_eq!(
            hunks[0].new_lines.last().map(String::as_str),
            Some("*** Update File: literal.txt")
        );
    }

    /// Verifies trailing change lines after an end-of-file marker are rejected.
    /// 验证文件末尾标记后的额外变更行会被拒绝。
    #[test]
    fn end_of_file_rejects_following_change_lines() {
        let patch = "*** Begin Patch\n*** Update File: sample.txt\n@@\n-old\n+new\n*** End of File\n+later\n*** End Patch";
        let error = parse_apply_patch(patch).expect_err("trailing change should fail");
        assert!(error.message.contains("End of File"));
    }

    /// Verifies a new hunk or blank context cannot follow an end-of-file marker.
    /// 验证文件末尾标记后不能继续出现新 hunk 或空白上下文。
    #[test]
    fn end_of_file_rejects_following_hunks_and_blank_context() {
        for trailing in ["@@ next\n-old\n+new", ""] {
            let patch = format!(
                "*** Begin Patch\n*** Update File: sample.txt\n@@\n-old\n+new\n*** End of File\n{trailing}\n*** End Patch"
            );
            let error = parse_apply_patch(&patch).expect_err("content after EOF should fail");
            assert!(error.message.contains("End of File"));
        }
    }
}
