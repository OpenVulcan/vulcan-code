//! Git diff text stats helpers.
//! Git diff 文本 stats 辅助逻辑。

/// Lightweight counters computed directly from one unified diff string.
/// 直接从 unified diff 字符串计算出的轻量计数。
pub(crate) struct WebuiUnifiedDiffStats {
    /// Number of changed files in the diff.
    /// diff 中的变更文件数量。
    pub(crate) files: usize,
    /// Number of added lines in the diff.
    /// diff 中的新增行数。
    pub(crate) additions: usize,
    /// Number of deleted lines in the diff.
    /// diff 中的删除行数。
    pub(crate) deletions: usize,
    /// Number of changed lines counted as additions plus deletions.
    /// 以新增行加删除行计算的变更行数。
    pub(crate) total_changed_lines: usize,
    /// Estimated byte size of added and deleted line bodies.
    /// 新增与删除行正文的估算字节体积。
    pub(crate) total_changed_bytes: usize,
}

/// Computes lightweight counters directly from one unified diff string.
/// 直接从 unified diff 字符串计算轻量计数。
pub(crate) fn webui_unified_diff_stats(unified_diff: &str) -> WebuiUnifiedDiffStats {
    let mut files = 0usize;
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut total_changed_bytes = 0usize;
    for_each_webui_unified_diff_line(unified_diff, |line| {
        if line.starts_with("diff --git ") {
            files += 1;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
            total_changed_bytes += line[1..].len();
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
            total_changed_bytes += line[1..].len();
        }
    });
    WebuiUnifiedDiffStats {
        files,
        additions,
        deletions,
        total_changed_lines: additions + deletions,
        total_changed_bytes,
    }
}

/// Scans one unified diff line by line without allocating an intermediate line vector.
/// 逐行扫描 unified diff，避免分配中间行数组。
pub(crate) fn for_each_webui_unified_diff_line(
    unified_diff: &str,
    mut consume_line: impl FnMut(&str),
) {
    let bytes = unified_diff.as_bytes();
    let mut line_start = 0usize;
    for index in 0..bytes.len() {
        if bytes[index] != b'\n' {
            continue;
        }
        let line_end = if index > line_start && bytes[index - 1] == b'\r' {
            index - 1
        } else {
            index
        };
        consume_line(&unified_diff[line_start..line_end]);
        line_start = index + 1;
    }
    if line_start < unified_diff.len() {
        consume_line(&unified_diff[line_start..]);
    }
}

/// Counts unified diff lines using the same streaming scanner as the summary and search index.
/// 使用与摘要和搜索索引一致的流式扫描器统计 unified diff 行数。
pub(crate) fn webui_unified_diff_line_count(unified_diff: &str) -> usize {
    let mut count = 0usize;
    for_each_webui_unified_diff_line(unified_diff, |_| {
        count += 1;
    });
    count
}

/// Returns the last token separated by ASCII whitespace without allocating token vectors.
/// 返回按 ASCII 空白分隔的最后一个 token，避免分配 token 向量。
pub(crate) fn last_webui_ascii_whitespace_token(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    Some(&value[start..end])
}

/// Truncates a string slice without cutting through a UTF-8 code point.
/// 在不截断 UTF-8 字符边界的情况下截断字符串切片。
pub(crate) fn truncate_string_to_char_boundary(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = 0usize;
    for (index, _) in value.char_indices() {
        if index > max_bytes {
            break;
        }
        boundary = index;
    }
    &value[..boundary]
}
