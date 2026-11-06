//! Display rows of job output: line wrapping, tails and text rendering.
//! 任务输出的显示行：换行切分、尾部与文本渲染。

use serde::Serialize;

use super::SHELL_OUTPUT_LINE_WRAP_BYTES;

/// What one display row holds.
/// 一个显示行承载的内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellJobRowKind {
    /// Output text of one line or one wrapped piece of it.
    /// 一行输出文本，或其中被切分出的一段。
    Text,
    /// Marker for output dropped by the log size limit.
    /// 因日志大小上限被丢弃的输出标记。
    Gap,
    /// Marker between two non-adjacent groups of search results.
    /// 两组不相邻搜索结果之间的分隔标记。
    Separator,
}

/// One display row of job output.
/// 任务输出的一个显示行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobLineRow {
    /// One-based output line the row belongs to; for a gap, the first dropped line.
    /// 该行所属的从 1 开始的输出行号；对缺口为第一条被丢弃的行。
    pub line: u64,
    /// Row kind.
    /// 行种类。
    pub kind: ShellJobRowKind,
    /// True when the row continues a line split by the wrap width, or starts after the line's beginning.
    /// 该行延续一条按换行宽度切分的行，或从行首之后开始时为 true。
    pub continuation: bool,
    /// True when the row belongs to a search match line.
    /// 该行属于搜索匹配行时为 true。
    pub matched: bool,
    /// Row text without the line break.
    /// 不含换行符的行文本。
    pub text: String,
}

impl ShellJobLineRow {
    /// Builds one gap marker row.
    /// 构建一个缺口标记行。
    pub(crate) fn gap(line: u64, dropped_bytes: u64) -> Self {
        Self {
            line,
            kind: ShellJobRowKind::Gap,
            continuation: false,
            matched: false,
            text: format!("[{dropped_bytes} bytes of output were dropped by the log size limit]"),
        }
    }

    /// Builds one separator row.
    /// 构建一个分隔行。
    pub(crate) fn separator() -> Self {
        Self {
            line: 0,
            kind: ShellJobRowKind::Separator,
            continuation: false,
            matched: false,
            text: String::from("--"),
        }
    }

    /// Bytes the row occupies in rendered text, including its line break.
    /// 该行在渲染文本中占用的字节数，含换行符。
    pub(crate) fn rendered_len(&self) -> usize {
        self.text.len() + 1
    }
}

/// Bounded tail of one job's output.
/// 一个任务输出的有界尾部。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobTail {
    /// Rows of the tail in output order.
    /// 按输出顺序排列的尾部显示行。
    pub rows: Vec<ShellJobLineRow>,
    /// Number of output lines so far.
    /// 迄今的输出行数。
    pub total_lines: u64,
    /// Number of output bytes so far.
    /// 迄今的输出字节数。
    pub total_bytes: u64,
    /// True when earlier output exists that the tail does not show.
    /// 存在尾部未显示的更早输出时为 true。
    pub truncated: bool,
}

/// Splits one line into rows no longer than the wrap width, cutting only at character boundaries.
/// 把一行切分为不超过换行宽度的多个显示行，只在字符边界处切分。
///
/// `starts_mid_line` marks the first row as a continuation when the text does not begin at the line start.
/// `starts_mid_line` 在文本不从行首开始时把第一行标记为续行。
pub(crate) fn wrap_line(
    line: u64,
    text: &str,
    starts_mid_line: bool,
    matched: bool,
) -> Vec<ShellJobLineRow> {
    let text = text.strip_suffix('\r').unwrap_or(text);
    let mut rows = Vec::new();
    let mut rest = text;
    let mut continuation = starts_mid_line;
    loop {
        let cut = if rest.len() <= SHELL_OUTPUT_LINE_WRAP_BYTES {
            rest.len()
        } else {
            floor_char_boundary(rest, SHELL_OUTPUT_LINE_WRAP_BYTES)
        };
        rows.push(ShellJobLineRow {
            line,
            kind: ShellJobRowKind::Text,
            continuation,
            matched,
            text: rest[..cut].to_string(),
        });
        rest = &rest[cut..];
        if rest.is_empty() {
            return rows;
        }
        continuation = true;
    }
}

/// Returns the largest character boundary of `text` not after `index`, never zero for non-empty text.
/// 返回 `text` 中不晚于 `index` 的最大字符边界；对非空文本绝不返回 0。
pub(crate) fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut cut = index;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    if cut == 0 {
        // A single character wider than the limit still forms one row.
        // 比上限更宽的单个字符仍单独成行。
        cut = text
            .char_indices()
            .nth(1)
            .map_or(text.len(), |(index, _)| index);
    }
    cut
}

/// Decodes one log byte slice, dropping an incomplete character cut at either end.
/// 解码一段日志字节，丢弃两端被截断的不完整字符。
pub(crate) fn decode_log_slice(bytes: &[u8]) -> String {
    let mut start = 0;
    // UTF-8 continuation bytes at the front belong to a character cut by the slice start.
    // 开头的 UTF-8 后续字节属于被切片起点截断的字符。
    while start < bytes.len() && start < 4 && (bytes[start] & 0xC0) == 0x80 {
        start += 1;
    }
    let bytes = &bytes[start..];
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned()
        }
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Keeps the newest rows whose rendered size fits the byte budget.
/// 保留渲染大小在字节预算内的最新显示行。
///
/// # Returns
/// 返回值
/// Returns true when rows were removed.
/// 删除了显示行时返回 true。
pub(crate) fn keep_newest_rows_within(rows: &mut Vec<ShellJobLineRow>, budget: usize) -> bool {
    let mut used = 0;
    let mut keep_from = rows.len();
    while keep_from > 0 {
        let next = rows[keep_from - 1].rendered_len();
        if used + next > budget {
            break;
        }
        used += next;
        keep_from -= 1;
    }
    if keep_from == 0 {
        return false;
    }
    rows.drain(..keep_from);
    true
}

/// Renders rows as plain output text; a row starting after its line's beginning is prefixed with `…`.
/// 把显示行渲染为纯输出文本；从行首之后开始的首行以 `…` 开头。
pub fn render_plain_rows(rows: &[ShellJobLineRow]) -> String {
    let mut rendered = String::new();
    let mut previous_line = None;
    for row in rows {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        if row.kind == ShellJobRowKind::Text && row.continuation && previous_line != Some(row.line)
        {
            rendered.push('…');
        }
        rendered.push_str(&row.text);
        previous_line = Some(row.line);
    }
    rendered
}

/// Renders rows with their line numbers: `12|text`, `12+|wrapped piece`, `--` between search groups.
/// 带行号渲染显示行：`12|text`、`12+|切分段`，搜索结果组之间为 `--`。
pub fn render_numbered_rows(rows: &[ShellJobLineRow]) -> String {
    let mut rendered = String::new();
    for row in rows {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        match row.kind {
            ShellJobRowKind::Separator => rendered.push_str("--"),
            ShellJobRowKind::Gap => {
                rendered.push_str(&format!("{}|{}", row.line, row.text));
            }
            ShellJobRowKind::Text => {
                let marker = match (row.continuation, row.matched) {
                    (false, false) => "|",
                    (true, false) => "+|",
                    (false, true) => ">|",
                    (true, true) => ">+|",
                };
                rendered.push_str(&format!("{}{marker}{}", row.line, row.text));
            }
        }
    }
    rendered
}
