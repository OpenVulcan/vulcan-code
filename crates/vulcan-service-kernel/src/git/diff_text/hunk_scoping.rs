//! Git diff text hunk scoping helpers.
//! Git diff 文本 hunk scoping 辅助逻辑。

pub(crate) fn webui_diff_hunk_scoped_unified_diff(
    base_unified_diff: &str,
    expanded_unified_diff: &str,
    hunk_index: usize,
) -> Option<String> {
    let expanded_lines: Vec<&str> = expanded_unified_diff.lines().collect();
    let expanded_hunks = webui_diff_text_hunk_ranges(&expanded_lines);
    if expanded_hunks.is_empty() {
        return None;
    }
    let base_lines: Vec<&str> = base_unified_diff.lines().collect();
    let base_hunks = webui_diff_text_hunk_ranges(&base_lines);
    let target_hunk = base_hunks.get(hunk_index);
    let expanded_hunk = target_hunk
        .and_then(|target| {
            expanded_hunks
                .iter()
                .find(|candidate| webui_diff_text_hunk_contains_range(candidate, target))
        })
        .or_else(|| expanded_hunks.get(hunk_index))?;
    let header_end = expanded_hunks.first()?.start_line;
    let mut scoped_lines = Vec::new();
    scoped_lines.extend_from_slice(&expanded_lines[..header_end]);
    scoped_lines
        .extend_from_slice(&expanded_lines[expanded_hunk.start_line..expanded_hunk.end_line]);
    let mut scoped_diff = scoped_lines.join("\n");
    if expanded_unified_diff.ends_with('\n') {
        scoped_diff.push('\n');
    }
    Some(scoped_diff)
}

/// Text range and parsed old/new line span for one unified diff hunk.
/// 单个 unified diff hunk 的文本范围与解析出的新旧行跨度。
pub(crate) struct WebuiDiffTextHunkRange {
    /// Inclusive start line index in the diff text.
    /// diff 文本中的起始行下标，包含该行。
    start_line: usize,
    /// Exclusive end line index in the diff text.
    /// diff 文本中的结束行下标，不包含该行。
    end_line: usize,
    /// Old-side start line from the hunk header.
    /// hunk 头部中的旧侧起始行号。
    old_start: usize,
    /// Old-side line count from the hunk header.
    /// hunk 头部中的旧侧行数。
    old_len: usize,
    /// Old-side inclusive end line used for containment checks.
    /// 用于包含性检查的旧侧结束行号，包含该行。
    old_end: usize,
    /// New-side start line from the hunk header.
    /// hunk 头部中的新侧起始行号。
    new_start: usize,
    /// New-side line count from the hunk header.
    /// hunk 头部中的新侧行数。
    new_len: usize,
    /// New-side inclusive end line used for containment checks.
    /// 用于包含性检查的新侧结束行号，包含该行。
    new_end: usize,
}

/// Extracts hunk text ranges from one unified diff line list.
/// 从 unified diff 行列表中提取 hunk 文本范围。
pub(crate) fn webui_diff_text_hunk_ranges(lines: &[&str]) -> Vec<WebuiDiffTextHunkRange> {
    let mut hunks = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        if !line.starts_with("@@ ") {
            continue;
        }
        let Some((old_start, old_len, new_start, new_len)) =
            parse_webui_diff_hunk_header_span(line)
        else {
            continue;
        };
        let end_line = lines[line_index + 1..]
            .iter()
            .position(|next_line| next_line.starts_with("@@ "))
            .map(|offset| line_index + 1 + offset)
            .unwrap_or(lines.len());
        hunks.push(WebuiDiffTextHunkRange {
            start_line: line_index,
            end_line,
            old_start,
            old_len,
            old_end: webui_diff_hunk_inclusive_end(old_start, old_len),
            new_start,
            new_len,
            new_end: webui_diff_hunk_inclusive_end(new_start, new_len),
        });
    }
    hunks
}

/// Parses start and length values from a unified diff hunk header.
/// 从 unified diff hunk 头部解析起始行与长度。
pub(crate) fn parse_webui_diff_hunk_header_span(
    header: &str,
) -> Option<(usize, usize, usize, usize)> {
    let body = header.strip_prefix("@@ ")?;
    let mut parts = body.split_whitespace();
    let old_range = parts.next()?.strip_prefix('-')?;
    let new_range = parts.next()?.strip_prefix('+')?;
    let (old_start, old_len) = parse_webui_diff_hunk_range_span(old_range)?;
    let (new_start, new_len) = parse_webui_diff_hunk_range_span(new_range)?;
    Some((old_start, old_len, new_start, new_len))
}

/// Parses one hunk range in `start,count` or `start` form.
/// 解析 `start,count` 或 `start` 形式的单个 hunk 范围。
pub(crate) fn parse_webui_diff_hunk_range_span(range: &str) -> Option<(usize, usize)> {
    let mut parts = range.split(',');
    let start = parts.next()?.parse::<usize>().ok()?;
    let len = parts
        .next()
        .map(|value| value.parse::<usize>().ok())
        .unwrap_or(Some(1))?;
    Some((start, len))
}

/// Converts a hunk start and length into an inclusive end for containment checks.
/// 将 hunk 起始行与长度转换为用于包含性检查的包含式结束行。
pub(crate) fn webui_diff_hunk_inclusive_end(start: usize, len: usize) -> usize {
    start + len.saturating_sub(1)
}

/// Returns whether an expanded hunk contains the original target hunk range.
/// 返回扩展 hunk 是否包含原始目标 hunk 范围。
pub(crate) fn webui_diff_text_hunk_contains_range(
    expanded: &WebuiDiffTextHunkRange,
    target: &WebuiDiffTextHunkRange,
) -> bool {
    let old_side_contains = target.old_len == 0
        || (expanded.old_start <= target.old_start && expanded.old_end >= target.old_end);
    let new_side_contains = target.new_len == 0
        || (expanded.new_start <= target.new_start && expanded.new_end >= target.new_end);
    old_side_contains && new_side_contains
}
