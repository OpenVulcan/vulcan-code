//! Rolling job log: decoded output text kept in a fixed head segment plus rolling body segments.
//! 滚动任务日志：解码后的输出文本保存在固定的开头分段与滚动的正文分段中。
//!
//! Each segment file is named `<start offset>-<line breaks before it>.seg`, so a log directory describes
//! itself after a restart. The head segment starts at offset 0 and is never dropped; body segments are
//! dropped oldest first once the retained body exceeds the log limit, which leaves at most one gap right
//! after the head.
//! 每个分段文件命名为 `<起始偏移>-<之前的换行数>.seg`，因此日志目录在重启后可以自我描述。开头分段从
//! 偏移 0 开始且永不丢弃；保留的正文超过日志上限后按从旧到新丢弃正文分段，因此最多只在开头之后出现
//! 一个缺口。

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Serialize;

use super::text::{
    ShellJobLineRow, ShellJobTail, decode_log_slice, keep_newest_rows_within, wrap_line,
};
use super::{SHELL_JOB_LOG_HEAD_BYTES, SHELL_JOB_LOG_LIMIT_BYTES, SHELL_JOB_LOG_SEGMENT_BYTES};

/// Longest part of one line read for display or matching; the rest is skipped and reported.
/// 为显示或匹配读取的单行最长部分；其余部分跳过并报告。
const LOG_LINE_READ_CAP_BYTES: usize = 64 * 1024;

/// File extension of one log segment.
/// 单个日志分段的文件扩展名。
const SEGMENT_EXTENSION: &str = "seg";

/// One log segment file.
/// 一个日志分段文件。
#[derive(Debug, Clone)]
struct LogSegment {
    /// Stream offset of the first byte.
    /// 第一个字节的流偏移。
    start: u64,
    /// Bytes in the segment.
    /// 分段中的字节数。
    len: u64,
    /// Line breaks in the stream before the segment.
    /// 流中位于该分段之前的换行数。
    breaks_before: u64,
    /// Line breaks inside the segment.
    /// 分段内的换行数。
    breaks: u64,
    /// Segment file path.
    /// 分段文件路径。
    path: PathBuf,
}

impl LogSegment {
    /// Stream offset just past the last byte.
    /// 紧跟最后一个字节之后的流偏移。
    fn end(&self) -> u64 {
        self.start + self.len
    }
}

/// Size limits of one log.
/// 单个日志的大小上限。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LogLimits {
    /// Bytes of the head segment.
    /// 开头分段的字节数。
    pub(crate) head: u64,
    /// Bytes of one body segment.
    /// 单个正文分段的字节数。
    pub(crate) segment: u64,
    /// Bytes kept in total.
    /// 总共保留的字节数。
    pub(crate) limit: u64,
}

impl LogLimits {
    /// The product limits.
    /// 产品使用的上限。
    pub(crate) const PRODUCT: Self = Self {
        head: SHELL_JOB_LOG_HEAD_BYTES,
        segment: SHELL_JOB_LOG_SEGMENT_BYTES,
        limit: SHELL_JOB_LOG_LIMIT_BYTES,
    };

    /// Byte capacity of one segment: the head holds the head size, body segments the segment size.
    /// 单个分段的字节容量：开头分段为开头大小，正文分段为分段大小。
    fn capacity(&self, segment: &LogSegment) -> u64 {
        if segment.start == 0 {
            self.head
        } else {
            self.segment
        }
    }
}

/// Appends decoded output to one job log.
/// 向一个任务日志追加解码后的输出。
#[derive(Debug)]
pub(crate) struct ShellJobLogWriter {
    /// Job log directory.
    /// 任务日志目录。
    dir: PathBuf,
    /// Retained segments in stream order; the first is the head.
    /// 按流顺序保留的分段；第一个是开头分段。
    segments: Vec<LogSegment>,
    /// Open file of the last segment.
    /// 最后一个分段的已打开文件。
    current: File,
    /// Total stream bytes written.
    /// 已写入的流字节总数。
    total_bytes: u64,
    /// Total line breaks written.
    /// 已写入的换行总数。
    line_breaks: u64,
    /// Whether the last written byte is a line break.
    /// 最后写入的字节是否为换行。
    ends_with_newline: bool,
    /// Bytes dropped by the log limit.
    /// 因日志上限被丢弃的字节数。
    dropped_bytes: u64,
    /// Size limits.
    /// 大小上限。
    limits: LogLimits,
}

impl ShellJobLogWriter {
    /// Creates an empty log with the product limits in `dir`, which must not hold a log yet.
    /// 在 `dir` 中以产品上限创建空日志；该目录不得已有日志。
    pub(crate) fn create(dir: &Path) -> io::Result<Self> {
        Self::create_with_limits(dir, LogLimits::PRODUCT)
    }

    /// Creates an empty log with explicit limits in `dir`.
    /// 在 `dir` 中以显式上限创建空日志。
    pub(crate) fn create_with_limits(dir: &Path, limits: LogLimits) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = segment_path(dir, 0, 0);
        let current = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            segments: vec![LogSegment {
                start: 0,
                len: 0,
                breaks_before: 0,
                breaks: 0,
                path,
            }],
            current,
            total_bytes: 0,
            line_breaks: 0,
            ends_with_newline: false,
            dropped_bytes: 0,
            limits,
        })
    }

    /// Appends one decoded text chunk and drops the oldest body segments beyond the log limit.
    /// 追加一段解码文本，并丢弃超出日志上限的最早正文分段。
    pub(crate) fn append(&mut self, text: &str) -> io::Result<()> {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            let last = self
                .segments
                .last()
                .expect("a log always keeps its head segment");
            let capacity = self.limits.capacity(last);
            if last.len >= capacity {
                let path = segment_path(&self.dir, self.total_bytes, self.line_breaks);
                self.current = OpenOptions::new()
                    .create_new(true)
                    .append(true)
                    .open(&path)?;
                self.segments.push(LogSegment {
                    start: self.total_bytes,
                    len: 0,
                    breaks_before: self.line_breaks,
                    breaks: 0,
                    path,
                });
                continue;
            }
            let take = usize::try_from(capacity - last.len)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            let (written, rest) = bytes.split_at(take);
            self.current.write_all(written)?;
            let breaks = count_line_breaks(written);
            let last = self
                .segments
                .last_mut()
                .expect("a log always keeps its head segment");
            last.len += take as u64;
            last.breaks += breaks;
            self.total_bytes += take as u64;
            self.line_breaks += breaks;
            self.ends_with_newline = written.last() == Some(&b'\n');
            bytes = rest;
        }
        self.drop_oldest_body_segments()
    }

    /// Drops the oldest sealed body segments while the body exceeds the limit, preserving the head and open segment.
    /// 正文超限时丢弃最早的已封口正文分段，保留头段及正在写入的分段。
    fn drop_oldest_body_segments(&mut self) -> io::Result<()> {
        let body_limit = self.limits.limit.saturating_sub(self.limits.head);
        let mut retained_body: u64 = self.segments[1..].iter().map(|segment| segment.len).sum();
        // The last segment is the open one and is never dropped.
        // 最后一个分段是正在写入的分段，永不丢弃。
        while retained_body > body_limit && self.segments.len() > 2 {
            let dropped = self.segments.remove(1);
            retained_body -= dropped.len;
            self.dropped_bytes += dropped.len;
            match fs::remove_file(&dropped.path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Returns a reader over a snapshot of the log as written so far.
    /// 返回一个基于迄今已写日志快照的读取器。
    pub(crate) fn reader(&self) -> ShellJobLogReader {
        ShellJobLogReader {
            segments: self.segments.clone(),
            total_bytes: self.total_bytes,
            line_breaks: self.line_breaks,
            ends_with_newline: self.ends_with_newline,
            dropped_bytes: self.dropped_bytes,
        }
    }
}

/// Size facts of one job log.
/// 一个任务日志的大小信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ShellJobLogStats {
    /// Output bytes written.
    /// 已写入的输出字节数。
    pub total_bytes: u64,
    /// Output lines written.
    /// 已写入的输出行数。
    pub total_lines: u64,
    /// Bytes dropped by the log limit.
    /// 因日志上限被丢弃的字节数。
    pub dropped_bytes: u64,
}

/// Result of reading a line range.
/// 读取一个行范围的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobLineRange {
    /// Rows in output order.
    /// 按输出顺序排列的显示行。
    pub rows: Vec<ShellJobLineRow>,
    /// One-based line the next read should start at, or none at the end of the output.
    /// 下一次读取应开始的从 1 开始行号；已到输出末尾时为 none。
    pub next_line: Option<u64>,
    /// Output lines so far.
    /// 迄今的输出行数。
    pub total_lines: u64,
}

/// Result of a log search.
/// 一次日志搜索的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobSearchMatch {
    /// Matching rows with context, groups separated by separator rows.
    /// 带上下文的匹配显示行，各组之间以分隔行隔开。
    pub rows: Vec<ShellJobLineRow>,
    /// Number of matching lines returned.
    /// 返回的匹配行数。
    pub match_count: usize,
    /// True when more matches exist than were returned, or the byte budget cut the rows.
    /// 存在未返回的更多匹配，或字节预算截断了显示行时为 true。
    pub truncated: bool,
}

/// One line item while walking the log.
/// 遍历日志时的一个行条目。
enum LogLineItem {
    /// One line read from retained output.
    /// 从保留输出中读取的一行。
    Text {
        /// Zero-based line index.
        /// 从 0 开始的行索引。
        line: u64,
        /// Line text without the line break, up to the read cap.
        /// 不含换行符的行文本，最多到读取上限。
        text: String,
        /// Bytes of the line skipped beyond the read cap.
        /// 超出读取上限而跳过的该行字节数。
        skipped_bytes: u64,
    },
    /// Output dropped by the log limit.
    /// 因日志上限被丢弃的输出。
    Gap {
        /// Zero-based first line index affected.
        /// 受影响的第一个从 0 开始的行索引。
        line: u64,
        /// Dropped bytes.
        /// 被丢弃的字节数。
        bytes: u64,
    },
}

/// Where one line starts.
/// 一行开始的位置。
enum LineLocation {
    /// Stream offset inside the retained segment with this index.
    /// 位于该索引保留分段中的流偏移。
    Start {
        /// Segment index.
        /// 分段索引。
        segment: usize,
        /// Stream offset.
        /// 流偏移。
        offset: u64,
    },
    /// The line starts in dropped output.
    /// 该行开始于被丢弃的输出中。
    InGap,
    /// The line does not exist yet.
    /// 该行尚不存在。
    PastEnd,
}

/// Reads one job log as of one snapshot; files only grow, so a snapshot stays readable.
/// 按某一快照读取一个任务日志；文件只会增长，因此快照始终可读。
#[derive(Debug, Clone)]
pub struct ShellJobLogReader {
    /// Retained segments in stream order.
    /// 按流顺序保留的分段。
    segments: Vec<LogSegment>,
    /// Stream bytes in the snapshot.
    /// 快照中的流字节数。
    total_bytes: u64,
    /// Line breaks in the snapshot.
    /// 快照中的换行数。
    line_breaks: u64,
    /// Whether the snapshot ends with a line break.
    /// 快照是否以换行结尾。
    ends_with_newline: bool,
    /// Bytes dropped by the log limit.
    /// 因日志上限被丢弃的字节数。
    dropped_bytes: u64,
}

impl ShellJobLogReader {
    /// Opens a finished log from its directory.
    /// 从目录打开一个已完成的日志。
    pub fn open(dir: &Path) -> io::Result<Self> {
        let mut segments = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some(SEGMENT_EXTENSION)
            {
                continue;
            }
            let Some((start, breaks_before)) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(parse_segment_stem)
            else {
                continue;
            };
            let mut bytes = Vec::new();
            File::open(&path)?.read_to_end(&mut bytes)?;
            segments.push((
                LogSegment {
                    start,
                    len: bytes.len() as u64,
                    breaks_before,
                    breaks: count_line_breaks(&bytes),
                    path,
                },
                bytes.last() == Some(&b'\n'),
            ));
        }
        segments.sort_by_key(|(segment, _)| segment.start);
        if segments
            .first()
            .is_none_or(|(segment, _)| segment.start != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("The job log in {} has no head segment.", dir.display()),
            ));
        }
        let ends_with_newline = segments
            .iter()
            .rev()
            .find(|(segment, _)| segment.len > 0)
            .is_some_and(|(_, ends)| *ends);
        let segments: Vec<LogSegment> = segments.into_iter().map(|(segment, _)| segment).collect();
        let last = segments.last().expect("checked above");
        let dropped_bytes = segments
            .windows(2)
            .map(|pair| pair[1].start.saturating_sub(pair[0].end()))
            .sum();
        Ok(Self {
            total_bytes: last.end(),
            line_breaks: last.breaks_before + last.breaks,
            ends_with_newline,
            dropped_bytes,
            segments,
        })
    }

    /// Returns the size facts.
    /// 返回大小信息。
    pub fn stats(&self) -> ShellJobLogStats {
        ShellJobLogStats {
            total_bytes: self.total_bytes,
            total_lines: self.total_lines(),
            dropped_bytes: self.dropped_bytes,
        }
    }

    /// Number of output lines; a trailing line without a break counts.
    /// 输出行数；末尾没有换行的行也计入。
    fn total_lines(&self) -> u64 {
        if self.total_bytes > 0 && !self.ends_with_newline {
            self.line_breaks + 1
        } else {
            self.line_breaks
        }
    }

    /// Returns the newest rows fitting `budget` bytes; shorter output returns in full.
    /// 返回在 `budget` 字节内的最新显示行；更短的输出完整返回。
    pub fn tail(&self, budget: usize) -> io::Result<ShellJobTail> {
        let mut tail = ShellJobTail {
            rows: Vec::new(),
            total_lines: self.total_lines(),
            total_bytes: self.total_bytes,
            truncated: false,
        };
        if self.total_bytes == 0 {
            return Ok(tail);
        }
        // run_start is where the last contiguous run of retained output begins.
        // run_start 是最后一段连续保留输出开始的位置。
        let run_index = self.last_run_start_index();
        let run_start = self.segments[run_index].start;
        let start = self
            .total_bytes
            .saturating_sub(budget as u64)
            .max(run_start);
        let bytes = self.read_range(start, self.total_bytes)?;
        // starts_at_line is true when the first byte begins a line.
        // starts_at_line 在第一个字节是行首时为 true。
        let starts_at_line = start == 0 || (start == run_start && run_index == 0);
        let starts_at_line = starts_at_line || self.byte_at(start - 1)? == Some(b'\n');
        let body = if self.ends_with_newline {
            &bytes[..bytes.len().saturating_sub(1)]
        } else {
            &bytes[..]
        };
        let mut pieces: Vec<&[u8]> = body.split(|byte| *byte == b'\n').collect();
        let mut first_partial = !starts_at_line;
        if first_partial && pieces.len() > 1 {
            // The first piece is the end of a line that started before the read window.
            // 第一段是读取窗口之前开始的某行的结尾。
            pieces.remove(0);
            first_partial = false;
        }
        let last_line = self.total_lines();
        let first_line = last_line + 1 - pieces.len() as u64;
        let mut rows = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            let text = decode_log_slice(piece);
            rows.extend(wrap_line(
                first_line + index as u64,
                &text,
                index == 0 && first_partial,
                false,
            ));
        }
        let cut = keep_newest_rows_within(&mut rows, budget);
        tail.truncated = cut || start > 0 || first_partial;
        tail.rows = rows;
        Ok(tail)
    }

    /// Returns at most `max_rows` of the newest rows, for compact live views.
    /// 返回至多 `max_rows` 个最新显示行，用于紧凑的实时视图。
    pub fn tail_rows(&self, max_rows: usize, budget: usize) -> io::Result<Vec<ShellJobLineRow>> {
        let mut rows = self.tail(budget)?.rows;
        if rows.len() > max_rows {
            rows.drain(..rows.len() - max_rows);
        }
        Ok(rows)
    }

    /// Reads rows starting at one-based `first_line`, at most `max_lines` lines and `budget` bytes.
    /// 从从 1 开始的 `first_line` 读取显示行，至多 `max_lines` 行、`budget` 字节。
    pub fn read_lines(
        &self,
        first_line: u64,
        max_lines: usize,
        budget: usize,
    ) -> io::Result<ShellJobLineRange> {
        let mut rows: Vec<ShellJobLineRow> = Vec::new();
        let mut used = 0;
        let mut lines = 0;
        let mut next_line = None;
        self.walk_lines(first_line.saturating_sub(1), |item| {
            if lines >= max_lines {
                next_line = Some(item_line(&item) + 1);
                return ControlFlow::Break(());
            }
            let item_rows = item_rows(&item, false);
            let size: usize = item_rows.iter().map(ShellJobLineRow::rendered_len).sum();
            if used + size > budget && !rows.is_empty() {
                next_line = Some(item_line(&item) + 1);
                return ControlFlow::Break(());
            }
            used += size;
            lines += 1;
            rows.extend(item_rows);
            if used >= budget {
                next_line = Some(item_line(&item) + 2);
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        })?;
        let total_lines = self.total_lines();
        Ok(ShellJobLineRange {
            rows,
            next_line: next_line.filter(|line| *line <= total_lines),
            total_lines,
        })
    }

    /// Finds lines matching `pattern`, with `context` lines around each, within `budget` bytes.
    /// 查找匹配 `pattern` 的行，每处带 `context` 行上下文，总量在 `budget` 字节内。
    pub fn search(
        &self,
        pattern: &Regex,
        context: usize,
        max_matches: usize,
        budget: usize,
    ) -> io::Result<ShellJobSearchMatch> {
        let mut result = ShellJobSearchMatch {
            rows: Vec::new(),
            match_count: 0,
            truncated: false,
        };
        let mut used = 0;
        // recent keeps the lines before a possible match.
        // recent 保存可能匹配之前的若干行。
        let mut recent: VecDeque<(u64, String)> = VecDeque::new();
        let mut after_remaining = 0;
        let mut last_emitted: Option<u64> = None;
        let push_rows = |result: &mut ShellJobSearchMatch,
                         used: &mut usize,
                         last_emitted: &mut Option<u64>,
                         line: u64,
                         text: &str,
                         matched: bool|
         -> bool {
            let mut rows = Vec::new();
            if last_emitted.is_some_and(|last| line > last + 1) {
                rows.push(ShellJobLineRow::separator());
            }
            rows.extend(wrap_line(line + 1, text, false, matched));
            let size: usize = rows.iter().map(ShellJobLineRow::rendered_len).sum();
            if *used + size > budget {
                result.truncated = true;
                return false;
            }
            *used += size;
            result.rows.extend(rows);
            *last_emitted = Some(line);
            true
        };
        self.walk_lines(0, |item| {
            let LogLineItem::Text { line, text, .. } = item else {
                recent.clear();
                return ControlFlow::Continue(());
            };
            if pattern.is_match(&text) {
                if result.match_count >= max_matches {
                    result.truncated = true;
                    return ControlFlow::Break(());
                }
                for (before_line, before_text) in recent.drain(..) {
                    if last_emitted.is_some_and(|last| before_line <= last) {
                        continue;
                    }
                    if !push_rows(
                        &mut result,
                        &mut used,
                        &mut last_emitted,
                        before_line,
                        &before_text,
                        false,
                    ) {
                        return ControlFlow::Break(());
                    }
                }
                if !push_rows(&mut result, &mut used, &mut last_emitted, line, &text, true) {
                    return ControlFlow::Break(());
                }
                result.match_count += 1;
                after_remaining = context;
            } else if after_remaining > 0 {
                after_remaining -= 1;
                if !push_rows(
                    &mut result,
                    &mut used,
                    &mut last_emitted,
                    line,
                    &text,
                    false,
                ) {
                    return ControlFlow::Break(());
                }
            } else if context > 0 {
                recent.push_back((line, text));
                if recent.len() > context {
                    recent.pop_front();
                }
            }
            ControlFlow::Continue(())
        })?;
        Ok(result)
    }

    /// Walks lines from zero-based `from`, reporting the dropped region once as a gap item.
    /// 从从 0 开始的 `from` 遍历各行，被丢弃的区域作为一个缺口条目报告一次。
    fn walk_lines(
        &self,
        from: u64,
        mut visit: impl FnMut(LogLineItem) -> ControlFlow<()>,
    ) -> io::Result<()> {
        let mut line = from;
        loop {
            match self.locate_line(line)? {
                LineLocation::PastEnd => return Ok(()),
                LineLocation::InGap => {
                    let (bytes, resume) = self.gap_after_head();
                    if visit(LogLineItem::Gap { line, bytes }).is_break() {
                        return Ok(());
                    }
                    line = resume.max(line + 1);
                }
                LineLocation::Start { segment, offset } => {
                    let run = RunReader::new(self, segment, offset);
                    let run_end = run.run_end();
                    let mut reader = BufReader::new(run);
                    while let Some((bytes, newline, skipped)) =
                        read_capped_line(&mut reader, LOG_LINE_READ_CAP_BYTES)?
                    {
                        let item = LogLineItem::Text {
                            line,
                            text: decode_log_slice(&bytes),
                            skipped_bytes: skipped,
                        };
                        if visit(item).is_break() {
                            return Ok(());
                        }
                        line += 1;
                        if !newline {
                            break;
                        }
                    }
                    if run_end >= self.total_bytes {
                        return Ok(());
                    }
                    // The run stopped at the gap after the head; the next line is reported as dropped.
                    // 连续段止于开头之后的缺口；下一行作为被丢弃输出报告。
                    let (bytes, resume) = self.gap_after_head();
                    if visit(LogLineItem::Gap { line, bytes }).is_break() {
                        return Ok(());
                    }
                    line = resume.max(line);
                }
            }
        }
    }

    /// Returns the dropped bytes after the head and the first line index readable after them.
    /// 返回开头之后被丢弃的字节数，以及其后第一个可读行的索引。
    fn gap_after_head(&self) -> (u64, u64) {
        match self.segments.get(1) {
            Some(first_body) => (
                first_body.start.saturating_sub(self.segments[0].end()),
                first_body.breaks_before + 1,
            ),
            None => (0, self.line_breaks + 1),
        }
    }

    /// Locates where zero-based `line` starts.
    /// 定位从 0 开始的 `line` 的起始位置。
    fn locate_line(&self, line: u64) -> io::Result<LineLocation> {
        if line >= self.total_lines() {
            return Ok(LineLocation::PastEnd);
        }
        if line == 0 {
            return Ok(LineLocation::Start {
                segment: 0,
                offset: 0,
            });
        }
        // Line `line` starts right after line break number `line` (one-based).
        // 第 `line` 行紧接在第 `line` 个换行（从 1 计）之后开始。
        let Some((index, segment)) = self.segments.iter().enumerate().find(|(_, segment)| {
            segment.breaks_before < line && line <= segment.breaks_before + segment.breaks
        }) else {
            return Ok(LineLocation::InGap);
        };
        let wanted = line - segment.breaks_before;
        let mut file = File::open(&segment.path)?;
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut seen = 0;
        let mut position = 0_u64;
        loop {
            let limit = usize::try_from(segment.len - position)
                .unwrap_or(usize::MAX)
                .min(buffer.len());
            if limit == 0 {
                return Ok(LineLocation::InGap);
            }
            let read = file.read(&mut buffer[..limit])?;
            if read == 0 {
                return Ok(LineLocation::InGap);
            }
            for (at, byte) in buffer[..read].iter().enumerate() {
                if *byte == b'\n' {
                    seen += 1;
                    if seen == wanted {
                        let offset = segment.start + position + at as u64 + 1;
                        // A break that is the segment's last byte means the line starts in the next segment.
                        // 换行是分段最后一个字节时，该行从下一个分段开始。
                        if offset == segment.end() {
                            return Ok(match self.segments.get(index + 1) {
                                Some(next) if next.start == offset => LineLocation::Start {
                                    segment: index + 1,
                                    offset,
                                },
                                Some(_) => LineLocation::InGap,
                                None => LineLocation::PastEnd,
                            });
                        }
                        return Ok(LineLocation::Start {
                            segment: index,
                            offset,
                        });
                    }
                }
            }
            position += read as u64;
        }
    }

    /// Index of the segment where the last contiguous run of retained output starts.
    /// 最后一段连续保留输出开始所在分段的索引。
    fn last_run_start_index(&self) -> usize {
        let mut index = self.segments.len() - 1;
        while index > 0 && self.segments[index - 1].end() == self.segments[index].start {
            index -= 1;
        }
        index
    }

    /// Reads retained bytes `[from, to)`, which must lie in one contiguous run.
    /// 读取保留字节 `[from, to)`，该范围必须位于一段连续输出中。
    fn read_range(&self, from: u64, to: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(usize::try_from(to - from).unwrap_or(0));
        for segment in &self.segments {
            if segment.end() <= from || segment.start >= to {
                continue;
            }
            let begin = from.max(segment.start);
            let end = to.min(segment.end());
            let mut file = File::open(&segment.path)?;
            file.seek(SeekFrom::Start(begin - segment.start))?;
            let mut part = vec![0_u8; usize::try_from(end - begin).unwrap_or(0)];
            file.read_exact(&mut part)?;
            bytes.extend_from_slice(&part);
        }
        Ok(bytes)
    }

    /// Reads one retained byte, or none when it was dropped.
    /// 读取一个保留字节；已被丢弃时返回 none。
    fn byte_at(&self, offset: u64) -> io::Result<Option<u8>> {
        if !self
            .segments
            .iter()
            .any(|segment| segment.start <= offset && offset < segment.end())
        {
            return Ok(None);
        }
        Ok(self.read_range(offset, offset + 1)?.first().copied())
    }
}

/// Line index of one walk item.
/// 一个遍历条目的行索引。
fn item_line(item: &LogLineItem) -> u64 {
    match item {
        LogLineItem::Text { line, .. } | LogLineItem::Gap { line, .. } => *line,
    }
}

/// Rows of one walk item; a skipped line end is reported in a trailing row.
/// 一个遍历条目的显示行；被跳过的行尾在末尾显示行中说明。
fn item_rows(item: &LogLineItem, matched: bool) -> Vec<ShellJobLineRow> {
    match item {
        LogLineItem::Gap { line, bytes } => vec![ShellJobLineRow::gap(line + 1, *bytes)],
        LogLineItem::Text {
            line,
            text,
            skipped_bytes,
        } => {
            let mut rows = wrap_line(line + 1, text, false, matched);
            if *skipped_bytes > 0 {
                let mut marker = wrap_line(
                    line + 1,
                    &format!("[{skipped_bytes} more bytes of this line not shown]"),
                    true,
                    matched,
                );
                rows.append(&mut marker);
            }
            rows
        }
    }
}

/// Reads one line of at most `cap` bytes, skipping and counting the rest of a longer line.
/// 读取至多 `cap` 字节的一行，更长行的其余部分跳过并计数。
///
/// # Returns
/// 返回值
/// Returns `None` at the end of the run, otherwise the bytes, whether a line break ended the line, and the
/// skipped byte count.
/// 到达连续段末尾时返回 `None`，否则返回字节、该行是否以换行结束，以及跳过的字节数。
fn read_capped_line(
    reader: &mut impl BufRead,
    cap: usize,
) -> io::Result<Option<(Vec<u8>, bool, u64)>> {
    let mut bytes = Vec::new();
    let mut skipped = 0_u64;
    let mut any = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(any.then_some((bytes, false, skipped)));
        }
        any = true;
        let (used, newline) = match available.iter().position(|byte| *byte == b'\n') {
            Some(at) => (at + 1, true),
            None => (available.len(), false),
        };
        let content = if newline {
            &available[..used - 1]
        } else {
            &available[..used]
        };
        let room = cap.saturating_sub(bytes.len());
        let kept = room.min(content.len());
        bytes.extend_from_slice(&content[..kept]);
        skipped += (content.len() - kept) as u64;
        reader.consume(used);
        if newline {
            return Ok(Some((bytes, true, skipped)));
        }
    }
}

/// Reads the retained output from one offset to the end of its contiguous run.
/// 从某一偏移读取保留输出，直至其所在连续段的末尾。
struct RunReader<'a> {
    /// Log snapshot.
    /// 日志快照。
    log: &'a ShellJobLogReader,
    /// Current segment index.
    /// 当前分段索引。
    segment: usize,
    /// Current stream offset.
    /// 当前流偏移。
    position: u64,
    /// Open file of the current segment.
    /// 当前分段的已打开文件。
    file: Option<File>,
}

impl<'a> RunReader<'a> {
    /// Starts reading at `offset` inside segment `segment`.
    /// 从分段 `segment` 内的 `offset` 开始读取。
    fn new(log: &'a ShellJobLogReader, segment: usize, offset: u64) -> Self {
        Self {
            log,
            segment,
            position: offset,
            file: None,
        }
    }

    /// Stream offset where the contiguous run containing the start ends.
    /// 包含起点的连续段结束处的流偏移。
    fn run_end(&self) -> u64 {
        let mut index = self.segment;
        while let Some(next) = self.log.segments.get(index + 1)
            && next.start == self.log.segments[index].end()
        {
            index += 1;
        }
        self.log.segments[index].end()
    }
}

impl Read for RunReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            let Some(segment) = self.log.segments.get(self.segment) else {
                return Ok(0);
            };
            if self.position >= segment.end() {
                match self.log.segments.get(self.segment + 1) {
                    Some(next) if next.start == segment.end() => {
                        self.segment += 1;
                        self.file = None;
                        continue;
                    }
                    _ => return Ok(0),
                }
            }
            if self.file.is_none() {
                let mut file = File::open(&segment.path)?;
                file.seek(SeekFrom::Start(self.position - segment.start))?;
                self.file = Some(file);
            }
            let limit = usize::try_from(segment.end() - self.position)
                .unwrap_or(usize::MAX)
                .min(buffer.len());
            let read = self
                .file
                .as_mut()
                .expect("opened above")
                .read(&mut buffer[..limit])?;
            if read == 0 {
                // The snapshot length is authoritative; a shorter file reads as the end of the run.
                // 以快照长度为准；更短的文件按连续段结束处理。
                return Ok(0);
            }
            self.position += read as u64;
            return Ok(read);
        }
    }
}

/// Path of one segment file.
/// 一个分段文件的路径。
fn segment_path(dir: &Path, start: u64, breaks_before: u64) -> PathBuf {
    dir.join(format!(
        "{start:020}-{breaks_before:020}.{SEGMENT_EXTENSION}"
    ))
}

/// Parses a segment file stem into its start offset and preceding line breaks.
/// 把分段文件名主体解析为起始偏移与之前的换行数。
fn parse_segment_stem(stem: &str) -> Option<(u64, u64)> {
    let (start, breaks_before) = stem.split_once('-')?;
    Some((start.parse().ok()?, breaks_before.parse().ok()?))
}

/// Counts line breaks in one byte slice.
/// 统计一段字节中的换行数。
fn count_line_breaks(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|byte| **byte == b'\n').count() as u64
}
