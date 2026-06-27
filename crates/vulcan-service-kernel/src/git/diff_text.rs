//! Shared WebUI Git diff text parsing helpers.
//! WebUI Git diff 文本解析共享辅助逻辑。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use super::command::{
    format_webui_git_error, run_webui_git_command, run_webui_git_command_with_stdin,
};
use super::diff::{ensure_webui_git_ref_text, is_path_within_root};
use super::model::{WebuiWorkspaceDiffFileSummary, WebuiWorkspaceDiffMode};

/// Maximum per-file changed lines transmitted to the WebUI diff renderer.
/// 允许传输给 WebUI diff 渲染器的单文件最大变更行数。
const WEBUI_DIFF_FILE_TRANSMIT_CHANGED_LINES_LIMIT: usize = 20_000;

/// Maximum single changed-line bytes transmitted to the WebUI diff renderer.
/// 允许传输给 WebUI diff 渲染器的单条变更行最大字节数。
const WEBUI_DIFF_FILE_TRANSMIT_MAX_CHANGED_LINE_BYTES_LIMIT: usize = 1024 * 1024;

/// Selects the expanded hunk that contains the original target hunk range.
/// 选择包含原始目标 hunk 范围的扩展 hunk。
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
struct WebuiDiffTextHunkRange {
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
fn webui_diff_text_hunk_ranges(lines: &[&str]) -> Vec<WebuiDiffTextHunkRange> {
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
fn parse_webui_diff_hunk_header_span(header: &str) -> Option<(usize, usize, usize, usize)> {
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
fn parse_webui_diff_hunk_range_span(range: &str) -> Option<(usize, usize)> {
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
fn webui_diff_hunk_inclusive_end(start: usize, len: usize) -> usize {
    start + len.saturating_sub(1)
}

/// Returns whether an expanded hunk contains the original target hunk range.
/// 返回扩展 hunk 是否包含原始目标 hunk 范围。
fn webui_diff_text_hunk_contains_range(
    expanded: &WebuiDiffTextHunkRange,
    target: &WebuiDiffTextHunkRange,
) -> bool {
    let old_side_contains = target.old_len == 0
        || (expanded.old_start <= target.old_start && expanded.old_end >= target.old_end);
    let new_side_contains = target.new_len == 0
        || (expanded.new_start <= target.new_start && expanded.new_end >= target.new_end);
    old_side_contains && new_side_contains
}

/// Rejects file pathspec text that could escape the repository or break argument boundaries.
/// 拒绝可能逃逸仓库或破坏参数边界的文件路径规格文本。
pub(crate) fn ensure_webui_git_pathspec_text(pathspec: &str) -> Result<(), String> {
    let value = pathspec.trim();
    if value.is_empty()
        || value == "/dev/null"
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || value.starts_with('/')
        || value.starts_with('\\')
    {
        return Err(String::from(
            "invalid file path for WebUI diff detail request",
        ));
    }
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(String::from(
            "invalid file path for WebUI diff detail request",
        ));
    }
    Ok(())
}

/// Mutable summary accumulator used while scanning one file patch.
/// 扫描单个文件 patch 时使用的可变摘要累加器。
struct WebuiWorkspaceDiffFileSummaryBuilder {
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
fn flush_webui_diff_file_summary(
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

/// Annotates file summaries with complete old/new source line counts.
/// 为文件摘要补充旧/新两侧源文件完整行数。
pub(crate) fn annotate_webui_diff_file_line_counts(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
    summaries: &mut [WebuiWorkspaceDiffFileSummary],
) {
    let branch_merge_base = if mode == WebuiWorkspaceDiffMode::Branch {
        selected_base_branch.zip(selected_compare_branch).and_then(
            |(base_branch, compare_branch)| {
                webui_git_merge_base(repository_root, base_branch, compare_branch).ok()
            },
        )
    } else {
        None
    };
    let old_paths = webui_diff_summary_side_paths(summaries, false);
    let new_paths = webui_diff_summary_side_paths(summaries, true);
    let old_counts = match mode {
        WebuiWorkspaceDiffMode::Unstaged => {
            webui_git_index_line_counts(repository_root, &old_paths)
        }
        WebuiWorkspaceDiffMode::Staged => {
            webui_git_revision_line_counts(repository_root, "HEAD", &old_paths)
        }
        WebuiWorkspaceDiffMode::Commit => selected_commit_hash
            .map(|commit_hash| format!("{commit_hash}^"))
            .map(|parent_revision| {
                webui_git_revision_line_counts(repository_root, &parent_revision, &old_paths)
            })
            .unwrap_or_default(),
        WebuiWorkspaceDiffMode::Branch => branch_merge_base
            .as_deref()
            .map(|base_revision| {
                webui_git_revision_line_counts(repository_root, base_revision, &old_paths)
            })
            .unwrap_or_default(),
    };
    let new_counts = match mode {
        WebuiWorkspaceDiffMode::Unstaged => webui_worktree_line_counts(repository_root, &new_paths),
        WebuiWorkspaceDiffMode::Staged => webui_git_index_line_counts(repository_root, &new_paths),
        WebuiWorkspaceDiffMode::Commit => selected_commit_hash
            .map(|commit_hash| {
                webui_git_revision_line_counts(repository_root, commit_hash, &new_paths)
            })
            .unwrap_or_default(),
        WebuiWorkspaceDiffMode::Branch => selected_base_branch
            .map(|base_branch| {
                webui_git_revision_line_counts(repository_root, base_branch, &new_paths)
            })
            .unwrap_or_default(),
    };
    for summary in summaries {
        summary.old_file_line_count =
            webui_diff_summary_side_line_count_from_map(summary, false, &old_counts);
        summary.new_file_line_count =
            webui_diff_summary_side_line_count_from_map(summary, true, &new_counts);
    }
}

/// Collects unique safe paths for one old/new side of file summaries.
/// 收集文件摘要旧/新一侧的唯一安全路径。
fn webui_diff_summary_side_paths(
    summaries: &[WebuiWorkspaceDiffFileSummary],
    new_side: bool,
) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for summary in summaries {
        let Some(path) = webui_diff_summary_side_path(summary, new_side) else {
            continue;
        };
        paths.insert(path.to_string());
    }
    paths.into_iter().collect()
}

/// Returns one safe side path from a file summary.
/// 从文件摘要返回单侧安全路径。
fn webui_diff_summary_side_path(
    summary: &WebuiWorkspaceDiffFileSummary,
    new_side: bool,
) -> Option<&str> {
    if summary.is_binary || summary.is_gitlink {
        return None;
    }
    let path = if new_side {
        summary.new_path.as_str()
    } else {
        summary.old_path.as_str()
    };
    if path.is_empty()
        || path == "/dev/null"
        || (new_side && summary.status == "deleted")
        || (!new_side && summary.status == "added")
        || ensure_webui_git_pathspec_text(path).is_err()
    {
        return None;
    }
    Some(path)
}

/// Looks up one side line count from a precomputed path map.
/// 从预计算路径映射中查询单侧行数。
fn webui_diff_summary_side_line_count_from_map(
    summary: &WebuiWorkspaceDiffFileSummary,
    new_side: bool,
    line_counts: &BTreeMap<String, usize>,
) -> Option<usize> {
    if (new_side && summary.status == "deleted") || (!new_side && summary.status == "added") {
        return Some(0);
    }
    let path = webui_diff_summary_side_path(summary, new_side)?;
    line_counts.get(path).copied()
}

/// Loads the merge-base hash used by a branch comparison.
/// 加载分支对比使用的 merge-base 哈希。
fn webui_git_merge_base(
    repository_root: &Path,
    base_branch: &str,
    compare_branch: &str,
) -> Result<String, String> {
    ensure_webui_git_ref_text(base_branch)?;
    ensure_webui_git_ref_text(compare_branch)?;
    let output = run_webui_git_command(
        repository_root,
        &["merge-base", base_branch, compare_branch],
    )?;
    if !output.status.success() {
        return Err(format_webui_git_error("git merge-base", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Number of pathspecs passed to one line-count metadata command.
/// 单次行数元数据命令传入的 pathspec 数量。
const WEBUI_DIFF_LINE_COUNT_PATH_CHUNK_SIZE: usize = 256;

/// Counts worktree lines for many paths without spawning Git per file.
/// 在不为每个文件启动 Git 的情况下批量统计工作区行数。
fn webui_worktree_line_counts(repository_root: &Path, paths: &[String]) -> BTreeMap<String, usize> {
    let mut line_counts = BTreeMap::new();
    for path in paths {
        let candidate = repository_root.join(path);
        if !is_path_within_root(repository_root, &candidate) || !candidate.is_file() {
            line_counts.insert(path.clone(), 0);
            continue;
        }
        if let Ok(bytes) = fs::read(candidate) {
            line_counts.insert(path.clone(), webui_line_count_from_bytes(&bytes));
        }
    }
    line_counts
}

/// Counts index blob lines for many paths with batched Git object reads.
/// 使用批量 Git 对象读取统计多个索引 blob 行数。
fn webui_git_index_line_counts(
    repository_root: &Path,
    paths: &[String],
) -> BTreeMap<String, usize> {
    let blob_ids = webui_git_index_blob_ids(repository_root, paths);
    webui_git_blob_line_counts(repository_root, blob_ids)
}

/// Counts revision blob lines for many paths with batched Git object reads.
/// 使用批量 Git 对象读取统计指定修订中的多个 blob 行数。
fn webui_git_revision_line_counts(
    repository_root: &Path,
    revision: &str,
    paths: &[String],
) -> BTreeMap<String, usize> {
    if ensure_webui_git_ref_text(revision).is_err() {
        return BTreeMap::new();
    }
    let blob_ids = webui_git_revision_blob_ids(repository_root, revision, paths);
    webui_git_blob_line_counts(repository_root, blob_ids)
}

/// Loads index blob ids for many paths.
/// 批量加载多个路径对应的索引 blob id。
fn webui_git_index_blob_ids(repository_root: &Path, paths: &[String]) -> BTreeMap<String, String> {
    let mut blob_ids = BTreeMap::new();
    for chunk in paths.chunks(WEBUI_DIFF_LINE_COUNT_PATH_CHUNK_SIZE) {
        let mut args = vec![
            String::from("ls-files"),
            String::from("-s"),
            String::from("-z"),
            String::from("--"),
        ];
        args.extend(chunk.iter().cloned());
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let Ok(output) = run_webui_git_command(repository_root, &arg_refs) else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        for entry in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let text = String::from_utf8_lossy(entry);
            let Some((metadata, path)) = text.split_once('\t') else {
                continue;
            };
            let mut metadata_parts = metadata.split_whitespace();
            let _mode = metadata_parts.next();
            let Some(oid) = metadata_parts.next() else {
                continue;
            };
            let Some(stage) = metadata_parts.next() else {
                continue;
            };
            if stage != "0" {
                continue;
            }
            blob_ids.insert(path.to_string(), oid.to_string());
        }
    }
    blob_ids
}

/// Loads revision blob ids for many paths.
/// 批量加载指定修订中多个路径对应的 blob id。
fn webui_git_revision_blob_ids(
    repository_root: &Path,
    revision: &str,
    paths: &[String],
) -> BTreeMap<String, String> {
    let mut blob_ids = BTreeMap::new();
    for chunk in paths.chunks(WEBUI_DIFF_LINE_COUNT_PATH_CHUNK_SIZE) {
        let mut args = vec![
            String::from("ls-tree"),
            String::from("-r"),
            String::from("-z"),
            revision.to_string(),
            String::from("--"),
        ];
        args.extend(chunk.iter().cloned());
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let Ok(output) = run_webui_git_command(repository_root, &arg_refs) else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        for entry in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let text = String::from_utf8_lossy(entry);
            let Some((metadata, path)) = text.split_once('\t') else {
                continue;
            };
            let mut metadata_parts = metadata.split_whitespace();
            let _mode = metadata_parts.next();
            let Some(kind) = metadata_parts.next() else {
                continue;
            };
            let Some(oid) = metadata_parts.next() else {
                continue;
            };
            if kind != "blob" {
                continue;
            }
            blob_ids.insert(path.to_string(), oid.to_string());
        }
    }
    blob_ids
}

/// Counts lines for path-to-blob mappings through one batched cat-file call.
/// 通过一次批量 cat-file 调用统计路径到 blob 映射的行数。
fn webui_git_blob_line_counts(
    repository_root: &Path,
    blob_ids_by_path: BTreeMap<String, String>,
) -> BTreeMap<String, usize> {
    if blob_ids_by_path.is_empty() {
        return BTreeMap::new();
    }
    let unique_blob_ids = blob_ids_by_path.values().cloned().collect::<BTreeSet<_>>();
    let mut stdin = Vec::new();
    for oid in &unique_blob_ids {
        stdin.extend_from_slice(oid.as_bytes());
        stdin.push(b'\n');
    }
    let Ok(output) =
        run_webui_git_command_with_stdin(repository_root, &["cat-file", "--batch"], &stdin)
    else {
        return BTreeMap::new();
    };
    if !output.status.success() {
        return BTreeMap::new();
    }
    let blob_counts = parse_webui_git_cat_file_line_counts(&output.stdout);
    let mut line_counts = BTreeMap::new();
    for (path, oid) in blob_ids_by_path {
        if let Some(count) = blob_counts.get(&oid) {
            line_counts.insert(path, *count);
        }
    }
    line_counts
}

/// Parses `git cat-file --batch` output into blob line counts.
/// 将 `git cat-file --batch` 输出解析为 blob 行数。
fn parse_webui_git_cat_file_line_counts(output: &[u8]) -> BTreeMap<String, usize> {
    let mut line_counts = BTreeMap::new();
    let mut cursor = 0usize;
    while cursor < output.len() {
        let Some(header_end_offset) = output[cursor..].iter().position(|byte| *byte == b'\n')
        else {
            break;
        };
        let header_end = cursor + header_end_offset;
        let header = String::from_utf8_lossy(&output[cursor..header_end]);
        let mut parts = header.split_whitespace();
        let Some(oid) = parts.next() else {
            break;
        };
        let Some(kind) = parts.next() else {
            break;
        };
        let Some(size_text) = parts.next() else {
            break;
        };
        let Ok(size) = size_text.parse::<usize>() else {
            break;
        };
        let content_start = header_end + 1;
        let content_end = content_start.saturating_add(size);
        if content_end > output.len() {
            break;
        }
        if kind == "blob" {
            line_counts.insert(
                oid.to_string(),
                webui_line_count_from_bytes(&output[content_start..content_end]),
            );
        }
        cursor = content_end;
        if cursor < output.len() && output[cursor] == b'\n' {
            cursor += 1;
        }
    }
    line_counts
}

/// Counts text lines from raw bytes using newline boundaries.
/// 基于换行边界从原始字节统计文本行数。
fn webui_line_count_from_bytes(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let newline_count = bytes.iter().filter(|byte| **byte == b'\n').count();
    if bytes.last().copied() == Some(b'\n') {
        newline_count
    } else {
        newline_count + 1
    }
}

/// Counts files whose diff body should not be transmitted to the WebUI renderer.
/// 统计不应传输给 WebUI 渲染器的文件 diff 正文数量。
pub(crate) fn webui_large_diff_omitted_file_count(
    // Summaries computed from the complete backend diff.
    // 从后端完整 diff 计算出的文件摘要。
    summaries: &[WebuiWorkspaceDiffFileSummary],
) -> usize {
    summaries
        .iter()
        .filter(|summary| should_omit_webui_large_diff_file(summary))
        .count()
}

/// Returns whether one file summary crosses the backend body-transmission gate.
/// 返回单个文件摘要是否超过后端正文传输阈值。
pub(crate) fn should_omit_webui_large_diff_file(
    // Summary for one complete file diff section.
    // 单个完整文件 diff 段落对应的摘要。
    summary: &WebuiWorkspaceDiffFileSummary,
) -> bool {
    summary.too_large_to_display
}

/// Removes oversized file sections while keeping the complete summary list available separately.
/// 移除超大文件段落，同时完整摘要列表仍通过独立字段返回。
pub(crate) fn webui_unified_diff_without_large_files(
    // Complete unified diff generated by Git or synthesized for untracked files.
    // Git 生成或为未跟踪文件合成的完整 unified diff。
    unified_diff: &str,
    // Summaries ordered exactly like the complete diff file sections.
    // 与完整 diff 文件段顺序一致的摘要列表。
    summaries: &[WebuiWorkspaceDiffFileSummary],
) -> String {
    let mut filtered = String::new();
    let mut current_section = String::new();
    let mut current_summary_index: Option<usize> = None;
    let mut next_summary_index = 0usize;

    for_each_webui_unified_diff_line(unified_diff, |line| {
        if line.starts_with("diff --git ") {
            flush_webui_filtered_diff_section(
                &mut filtered,
                &mut current_section,
                current_summary_index,
                summaries,
            );
            current_summary_index = Some(next_summary_index);
            next_summary_index += 1;
        }
        current_section.push_str(line);
        current_section.push('\n');
    });
    flush_webui_filtered_diff_section(
        &mut filtered,
        &mut current_section,
        current_summary_index,
        summaries,
    );
    filtered
}

/// Appends one accumulated diff section when its summary does not cross the omit gate.
/// 当累积的 diff 段未超过省略阈值时将其追加到过滤结果。
fn flush_webui_filtered_diff_section(
    // Filtered unified diff text being built.
    // 正在构建的过滤后 unified diff 文本。
    filtered: &mut String,
    // Accumulated section text for the current file.
    // 当前文件已经累积的 diff 段文本。
    current_section: &mut String,
    // Summary index corresponding to the current section.
    // 当前 diff 段对应的摘要下标。
    current_summary_index: Option<usize>,
    // Complete summaries ordered like the original diff sections.
    // 与原始 diff 段顺序一致的完整摘要列表。
    summaries: &[WebuiWorkspaceDiffFileSummary],
) {
    if current_section.is_empty() {
        return;
    }
    let omit = current_summary_index
        .and_then(|index| summaries.get(index))
        .is_some_and(should_omit_webui_large_diff_file);
    if !omit {
        if !filtered.is_empty() && !filtered.ends_with('\n') {
            filtered.push('\n');
        }
        filtered.push_str(current_section);
    }
    current_section.clear();
}

/// Parses a standard `diff --git` header into old and new paths.
/// 将标准 `diff --git` 头解析为旧路径和新路径。
fn parse_webui_diff_git_header(header: &str) -> (String, String) {
    let body = header.trim_start_matches("diff --git ").trim();
    let tokens = tokenize_webui_diff_header_paths(body);
    let old_path = trim_webui_diff_path(tokens.first().map(String::as_str).unwrap_or_default());
    let new_path = trim_webui_diff_path(tokens.get(1).map(String::as_str).unwrap_or(&old_path));
    (old_path, new_path)
}

/// Splits a diff header path list while preserving quoted spaces.
/// 拆分 diff 头部路径列表，同时保留带引号路径中的空格。
fn tokenize_webui_diff_header_paths(value: &str) -> Vec<String> {
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
fn trim_webui_diff_path(path: &str) -> String {
    let normalized = unescape_webui_git_quoted_path(path.trim().trim_matches('"'));
    normalized
        .strip_prefix("a/")
        .or_else(|| normalized.strip_prefix("b/"))
        .unwrap_or(&normalized)
        .to_string()
}

/// Decodes Git's C-style quoted path escapes into UTF-8 text.
/// 将 Git 的 C 风格路径转义解码为 UTF-8 文本。
fn unescape_webui_git_quoted_path(path: &str) -> String {
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
fn apply_webui_diff_file_mode(summary: &mut WebuiWorkspaceDiffFileSummaryBuilder, mode: &str) {
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
fn apply_webui_diff_changed_line(summary: &mut WebuiWorkspaceDiffFileSummaryBuilder, line: &str) {
    let line_bytes = line.len();
    summary.changed_bytes += line_bytes;
    summary.max_changed_line_bytes = summary.max_changed_line_bytes.max(line_bytes);
}

/// Resolves the final summary status after all metadata and counters are known.
/// 在所有元数据与计数已知后解析最终摘要状态。
fn final_webui_diff_file_summary_status(summary: &WebuiWorkspaceDiffFileSummaryBuilder) -> String {
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
fn last_webui_ascii_whitespace_token(value: &str) -> Option<&str> {
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
