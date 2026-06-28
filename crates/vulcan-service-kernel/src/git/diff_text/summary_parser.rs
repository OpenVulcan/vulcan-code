//! Git diff text summary parser helpers.
//! Git diff 文本 summary parser 辅助逻辑。

use super::*;

/// Mutable summary accumulator used while scanning one file patch.
/// 扫描单个文件 patch 时使用的可变摘要累加器。
pub(crate) struct WebuiWorkspaceDiffFileSummaryBuilder {
    /// Previous file path.
    /// 旧文件路径。
    old_path: String,
    /// Current file path.
    /// 当前文件路径。
    new_path: String,
    /// File-level change status.
    /// 文件级变更状态。
    status: String,
    /// Number of added lines in this file.
    /// 该文件的新增行数。
    additions: usize,
    /// Number of deleted lines in this file.
    /// 该文件的删除行数。
    deletions: usize,
    /// Whether this file is binary-only in the diff.
    /// 该文件是否在 diff 中仅表现为二进制变更。
    is_binary: bool,
    /// Whether this file is a gitlink or submodule entry.
    /// 该文件是否为 gitlink 或子模块条目。
    is_gitlink: bool,
    /// File mode parsed from diff metadata when available.
    /// 从 diff 元数据解析出的可选文件模式。
    file_mode: Option<String>,
    /// Estimated UTF-8 byte size of changed lines in this file.
    /// 该文件变更行的估算 UTF-8 字节体积。
    changed_bytes: usize,
    /// Largest estimated UTF-8 byte size among changed lines.
    /// 变更行中的最大估算 UTF-8 字节体积。
    max_changed_line_bytes: usize,
    /// Estimated visual row count including hunk headers.
    /// 包含 hunk 头部的估算可视行数。
    row_count: usize,
    /// Number of parsed hunk headers in this file diff.
    /// 该文件 diff 中解析到的 hunk 头部数量。
    hunk_count: usize,
    /// Whether the current scan position is inside a hunk body.
    /// 当前扫描位置是否位于 hunk 正文内。
    in_hunk: bool,
}

/// Computes lightweight file summaries directly from one unified diff string.
/// 直接从 unified diff 字符串计算轻量文件摘要。
pub(crate) fn webui_unified_diff_file_summaries(
    unified_diff: &str,
) -> Vec<WebuiWorkspaceDiffFileSummary> {
    let mut summaries = Vec::new();
    let mut current: Option<WebuiWorkspaceDiffFileSummaryBuilder> = None;
    for_each_webui_unified_diff_line(unified_diff, |line| {
        if line.starts_with("diff --git ") {
            flush_webui_diff_file_summary(&mut summaries, current.take());
            let (old_path, new_path) = parse_webui_diff_git_header(line);
            current = Some(WebuiWorkspaceDiffFileSummaryBuilder {
                old_path,
                new_path,
                status: String::from("modified"),
                additions: 0,
                deletions: 0,
                is_binary: false,
                is_gitlink: false,
                file_mode: None,
                changed_bytes: 0,
                max_changed_line_bytes: 0,
                row_count: 0,
                hunk_count: 0,
                in_hunk: false,
            });
            return;
        }
        let Some(summary) = current.as_mut() else {
            return;
        };
        if let Some(mode) = line.strip_prefix("new file mode ") {
            summary.status = String::from("added");
            apply_webui_diff_file_mode(summary, mode.trim());
        } else if let Some(mode) = line.strip_prefix("deleted file mode ") {
            summary.status = String::from("deleted");
            apply_webui_diff_file_mode(summary, mode.trim());
        } else if let Some(path) = line.strip_prefix("rename from ") {
            summary.old_path = trim_webui_diff_path(path);
            summary.status = String::from("renamed");
        } else if let Some(path) = line.strip_prefix("rename to ") {
            summary.new_path = trim_webui_diff_path(path);
            summary.status = String::from("renamed");
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            summary.is_binary = true;
            summary.status = String::from("binary");
        } else if line.starts_with("old mode ") || line.starts_with("new mode ") {
            if let Some(mode) = last_webui_ascii_whitespace_token(line) {
                apply_webui_diff_file_mode(summary, mode);
            }
        } else if line.starts_with("index ") {
            if let Some(mode) =
                last_webui_ascii_whitespace_token(line).filter(|mode| mode.len() == 6)
            {
                apply_webui_diff_file_mode(summary, mode);
            }
        } else if line.starts_with("@@ ") {
            summary.row_count += 1;
            summary.hunk_count += 1;
            summary.in_hunk = true;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            if summary.in_hunk {
                summary.row_count += 1;
            }
            summary.additions += 1;
            apply_webui_diff_changed_line(summary, &line[1..]);
        } else if line.starts_with('-') && !line.starts_with("---") {
            if summary.in_hunk {
                summary.row_count += 1;
            }
            summary.deletions += 1;
            apply_webui_diff_changed_line(summary, &line[1..]);
        } else if summary.in_hunk {
            summary.row_count += 1;
        }
    });
    flush_webui_diff_file_summary(&mut summaries, current);
    summaries
}

/// Appends one completed file summary to the output list.
/// 将一个已完成文件摘要追加到输出列表。
pub(crate) fn flush_webui_diff_file_summary(
    summaries: &mut Vec<WebuiWorkspaceDiffFileSummary>,
    summary: Option<WebuiWorkspaceDiffFileSummaryBuilder>,
) {
    let Some(summary) = summary else {
        return;
    };
    let key = format!(
        "{}->{}:{}",
        summary.old_path,
        summary.new_path,
        summaries.len()
    );
    let status = final_webui_diff_file_summary_status(&summary);
    let changed_lines = summary.additions + summary.deletions;
    let too_large_to_display = changed_lines > WEBUI_DIFF_FILE_TRANSMIT_CHANGED_LINES_LIMIT
        || summary.max_changed_line_bytes > WEBUI_DIFF_FILE_TRANSMIT_MAX_CHANGED_LINE_BYTES_LIMIT;
    summaries.push(WebuiWorkspaceDiffFileSummary {
        key,
        old_path: summary.old_path,
        new_path: summary.new_path,
        status,
        additions: summary.additions,
        deletions: summary.deletions,
        is_binary: summary.is_binary,
        is_gitlink: summary.is_gitlink,
        file_mode: summary.file_mode,
        changed_lines,
        too_large_to_display,
        changed_bytes: summary.changed_bytes,
        max_changed_line_bytes: summary.max_changed_line_bytes,
        row_count: summary.row_count,
        hunk_count: summary.hunk_count,
        old_file_line_count: None,
        new_file_line_count: None,
    });
}

pub(crate) fn parse_webui_diff_git_header(header: &str) -> (String, String) {
    let body = header.trim_start_matches("diff --git ").trim();
    let tokens = tokenize_webui_diff_header_paths(body);
    let old_path = trim_webui_diff_path(tokens.first().map(String::as_str).unwrap_or_default());
    let new_path = trim_webui_diff_path(tokens.get(1).map(String::as_str).unwrap_or(&old_path));
    (old_path, new_path)
}

/// Splits a diff header path list while preserving quoted spaces.
/// 拆分 diff 头部路径列表，同时保留带引号路径中的空格。
pub(crate) fn tokenize_webui_diff_header_paths(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push('\\');
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            continue;
        }
        if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(character);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Trims Git's a/ and b/ prefixes from one diff path.
/// 从单个 diff 路径中移除 Git 的 a/ 与 b/ 前缀。
pub(crate) fn trim_webui_diff_path(path: &str) -> String {
    let normalized = unescape_webui_git_quoted_path(path.trim().trim_matches('"'));
    normalized
        .strip_prefix("a/")
        .or_else(|| normalized.strip_prefix("b/"))
        .unwrap_or(&normalized)
        .to_string()
}

/// Decodes Git's C-style quoted path escapes into UTF-8 text.
/// 将 Git 的 C 风格路径转义解码为 UTF-8 文本。
pub(crate) fn unescape_webui_git_quoted_path(path: &str) -> String {
    let mut bytes = Vec::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            let mut encoded = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            continue;
        }
        let Some(escaped) = chars.next() else {
            bytes.push(b'\\');
            break;
        };
        if ('0'..='7').contains(&escaped) {
            let mut value = escaped.to_digit(8).unwrap_or(0);
            for _ in 0..2 {
                let Some(next) = chars.peek().copied() else {
                    break;
                };
                let Some(digit) = next.to_digit(8) else {
                    break;
                };
                chars.next();
                value = value * 8 + digit;
            }
            bytes.push(value.min(u8::MAX as u32) as u8);
            continue;
        }
        match escaped {
            'a' => bytes.push(0x07),
            'b' => bytes.push(0x08),
            'f' => bytes.push(0x0c),
            'n' => bytes.push(b'\n'),
            'r' => bytes.push(b'\r'),
            't' => bytes.push(b'\t'),
            'v' => bytes.push(0x0b),
            '\\' => bytes.push(b'\\'),
            '"' => bytes.push(b'"'),
            other => {
                let mut encoded = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut encoded).as_bytes());
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| path.replace("\\\"", "\"").replace("\\\\", "\\"))
}

/// Stores one parsed file mode and detects gitlinks.
/// 保存一个解析出的文件模式并识别 gitlink。
pub(crate) fn apply_webui_diff_file_mode(
    summary: &mut WebuiWorkspaceDiffFileSummaryBuilder,
    mode: &str,
) {
    if mode.is_empty() {
        return;
    }
    summary.file_mode = Some(mode.to_string());
    if mode == "160000" {
        summary.is_gitlink = true;
    }
}

/// Updates changed-line byte metrics for one file summary.
/// 更新单个文件摘要的变更行字节指标。
pub(crate) fn apply_webui_diff_changed_line(
    summary: &mut WebuiWorkspaceDiffFileSummaryBuilder,
    line: &str,
) {
    let line_bytes = line.len();
    summary.changed_bytes += line_bytes;
    summary.max_changed_line_bytes = summary.max_changed_line_bytes.max(line_bytes);
}

/// Resolves the final summary status after all metadata and counters are known.
/// 在所有元数据与计数已知后解析最终摘要状态。
pub(crate) fn final_webui_diff_file_summary_status(
    summary: &WebuiWorkspaceDiffFileSummaryBuilder,
) -> String {
    if summary.status != "modified" {
        return summary.status.clone();
    }
    if summary.old_path == "/dev/null" {
        return String::from("added");
    }
    if summary.new_path == "/dev/null" {
        return String::from("deleted");
    }
    String::from("modified")
}
