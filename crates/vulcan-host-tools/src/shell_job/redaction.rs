//! Mask known values in raw shell job windows before line and display projection.
//! 在逻辑行和显示投影之前，遮蔽 shell 任务原始窗口中的登记值。

use std::io;
use std::ops::Range;

use super::text::decode_log_slice;

/// Snapshot of registered exact values and their display replacements for one log read.
/// 一次日志读取使用的已登记精确值及显示替换文本的快照。
#[derive(Default)]
pub struct ShellJobLogRedaction {
    /// Values in registry order, with a longer value taking priority at the same start.
    /// 按注册表顺序保存的值，同一起点处优先匹配较长值。
    values: Vec<(String, String)>,
    /// Bytes of adjacent raw context needed for matches that contain newlines.
    /// 含换行符的匹配需要的相邻原始上下文字节数。
    overlap: u64,
}

impl ShellJobLogRedaction {
    /// Takes ownership of exact nonempty values and the labels that will replace them.
    /// 接管非空精确值以及用于替换它们的标签。
    pub fn new(values: impl IntoIterator<Item = (String, String)>) -> Self {
        let values: Vec<_> = values
            .into_iter()
            .filter(|(value, _)| !value.is_empty())
            .collect();
        let overlap = values
            .iter()
            .filter(|(value, _)| value.contains('\n'))
            .map(|(value, _)| value.len().saturating_sub(1) as u64)
            .max()
            .unwrap_or_default();
        Self { values, overlap }
    }

    /// Reports whether raw output can be projected without scanning for secrets.
    /// 报告是否可以在不扫描密钥的情况下投影原始输出。
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the context margin in bytes.
    /// 返回上下文边距的字节数。
    pub(crate) fn overlap(&self) -> u64 {
        self.overlap
    }

    /// Records all protected byte ranges in the window, including overlapping occurrences.
    /// 记录窗口内所有受保护的字节范围，包括互相重叠的匹配。
    /// The byte offsets stay aligned with the original log even when a match crosses a newline.
    /// 即使匹配跨过换行，字节偏移仍与原始日志对齐。
    pub(crate) fn find<'a>(&'a self, raw: &[u8]) -> io::Result<Vec<LogRedactionSpan<'a>>> {
        if self.is_empty() {
            return Ok(Vec::new());
        }
        // A window can start or end in the middle of a UTF-8 character.
        // 窗口可能在 UTF-8 字符中间开始或结束。
        let start = raw
            .iter()
            .take_while(|byte| (**byte & 0xc0) == 0x80)
            .count();
        let text = match std::str::from_utf8(&raw[start..]) {
            Ok(text) => text,
            Err(error) if error.error_len().is_none() => {
                std::str::from_utf8(&raw[start..start + error.valid_up_to()])
                    .map_err(io::Error::other)?
            }
            Err(error) => return Err(io::Error::other(error)),
        };
        let mut spans = Vec::new();
        for (value, replacement) in &self.values {
            let mut offset = 0;
            while let Some(found) = text[offset..].find(value) {
                let at = offset + found;
                spans.push(LogRedactionSpan {
                    bytes: start + at..start + at + value.len(),
                    replacement,
                });
                offset = at
                    + text[at..]
                        .chars()
                        .next()
                        .expect("nonempty match")
                        .len_utf8();
            }
        }
        spans.sort_unstable_by_key(|span| (span.bytes.start, std::cmp::Reverse(span.bytes.end)));
        let mut merged: Vec<LogRedactionSpan<'_>> = Vec::new();
        for span in spans {
            if let Some(previous) = merged.last_mut()
                && span.bytes.start < previous.bytes.end
            {
                previous.bytes.end = previous.bytes.end.max(span.bytes.end);
            } else {
                merged.push(span);
            }
        }
        Ok(merged)
    }
}

/// A protected byte range and the safe text used for its displayed portion.
/// 一段受保护的字节范围及其可见部分使用的安全文本。
pub(crate) struct LogRedactionSpan<'a> {
    /// Offsets relative to the raw window, exclusive at the end.
    /// 相对于原始窗口的偏移，不含末尾。
    pub(crate) bytes: Range<usize>,
    /// Trusted replacement taken from the registry.
    /// 来自注册表的可信替换文本。
    replacement: &'a str,
}

/// Projects a logical line from raw bytes, replacing every intersecting protected span.
/// 从原始字节投影逻辑行，替换与其相交的每一段受保护范围。
/// The caller divides the original bytes into lines, so replacements preserve original line numbers.
/// 调用方按原始字节分行，因此替换保留原始行号。
pub(crate) fn project_log_line(
    raw: &[u8],
    visible: Range<usize>,
    spans: &[LogRedactionSpan<'_>],
) -> String {
    if visible.is_empty() {
        return String::new();
    }
    let mut result = String::new();
    let mut cursor = visible.start;
    let first = spans.partition_point(|span| span.bytes.end <= visible.start);
    for span in &spans[first..] {
        if span.bytes.start >= visible.end {
            break;
        }
        let begin = span.bytes.start.max(visible.start);
        result.push_str(&decode_log_slice(&raw[cursor..begin]));
        result.push_str(span.replacement);
        cursor = span.bytes.end.min(visible.end);
    }
    result.push_str(&decode_log_slice(&raw[cursor..visible.end]));
    result
}
