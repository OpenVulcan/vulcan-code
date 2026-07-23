//! Git diff text line counts helpers.
//! Git diff 文本 line counts 辅助逻辑。

use super::super::selection::WebuiWorkspaceDiffSelection;
use super::*;
use vulcan_host_tools::output_decode::decode_shell_output_bytes;

/// Annotates file summaries using the selected WebUI diff request context.
/// 使用已解析的 WebUI diff 选择上下文标注文件摘要行数。
pub(crate) fn annotate_webui_diff_file_line_counts_for_selection(
    repository_root: &Path,
    selection: &WebuiWorkspaceDiffSelection,
    summaries: &mut [WebuiWorkspaceDiffFileSummary],
) {
    annotate_webui_diff_file_line_counts(
        repository_root,
        selection.request.mode,
        selection.selected_base_branch.as_deref(),
        selection.selected_compare_branch.as_deref(),
        selection.selected_commit_hash.as_deref(),
        summaries,
    );
}

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
pub(crate) fn webui_diff_summary_side_paths(
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
pub(crate) fn webui_diff_summary_side_path(
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
pub(crate) fn webui_diff_summary_side_line_count_from_map(
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
pub(crate) fn webui_git_merge_base(
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
    Ok(decode_shell_output_bytes(&output.stdout).trim().to_string())
}

/// Number of pathspecs passed to one line-count metadata command.
/// 单次行数元数据命令传入的 pathspec 数量。
const WEBUI_DIFF_LINE_COUNT_PATH_CHUNK_SIZE: usize = 256;

/// Loads blob ids from one chunked Git metadata command.
/// 通过单个分块 Git 元数据命令加载 blob id。
fn webui_git_blob_ids_from_metadata_command(
    repository_root: &Path,
    paths: &[String],
    command_args_before_paths: &[&str],
    metadata_blob_id: fn(&str) -> Option<&str>,
) -> BTreeMap<String, String> {
    let mut blob_ids = BTreeMap::new();
    for chunk in paths.chunks(WEBUI_DIFF_LINE_COUNT_PATH_CHUNK_SIZE) {
        let mut args = command_args_before_paths
            .iter()
            .map(|argument| (*argument).to_string())
            .collect::<Vec<_>>();
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
            let text = decode_shell_output_bytes(entry);
            let Some((metadata, path)) = text.split_once('\t') else {
                continue;
            };
            let Some(oid) = metadata_blob_id(metadata) else {
                continue;
            };
            blob_ids.insert(path.to_string(), oid.to_string());
        }
    }
    blob_ids
}

/// Extracts a stage-zero index blob id from `git ls-files -s` metadata.
/// 从 `git ls-files -s` 元数据中提取 stage-zero 索引 blob id。
fn webui_git_index_metadata_blob_id(metadata: &str) -> Option<&str> {
    let mut metadata_parts = metadata.split_whitespace();
    let _mode = metadata_parts.next()?;
    let oid = metadata_parts.next()?;
    let stage = metadata_parts.next()?;
    if stage == "0" { Some(oid) } else { None }
}

/// Extracts a blob id from `git ls-tree -r` metadata.
/// 从 `git ls-tree -r` 元数据中提取 blob id。
fn webui_git_revision_metadata_blob_id(metadata: &str) -> Option<&str> {
    let mut metadata_parts = metadata.split_whitespace();
    let _mode = metadata_parts.next()?;
    let kind = metadata_parts.next()?;
    let oid = metadata_parts.next()?;
    if kind == "blob" { Some(oid) } else { None }
}

/// Counts worktree lines for many paths without spawning Git per file.
/// 在不为每个文件启动 Git 的情况下批量统计工作区行数。
pub(crate) fn webui_worktree_line_counts(
    repository_root: &Path,
    paths: &[String],
) -> BTreeMap<String, usize> {
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
pub(crate) fn webui_git_index_line_counts(
    repository_root: &Path,
    paths: &[String],
) -> BTreeMap<String, usize> {
    let blob_ids = webui_git_index_blob_ids(repository_root, paths);
    webui_git_blob_line_counts(repository_root, blob_ids)
}

/// Counts revision blob lines for many paths with batched Git object reads.
/// 使用批量 Git 对象读取统计指定修订中的多个 blob 行数。
pub(crate) fn webui_git_revision_line_counts(
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
pub(crate) fn webui_git_index_blob_ids(
    repository_root: &Path,
    paths: &[String],
) -> BTreeMap<String, String> {
    webui_git_blob_ids_from_metadata_command(
        repository_root,
        paths,
        &["ls-files", "-s", "-z", "--"],
        webui_git_index_metadata_blob_id,
    )
}

/// Loads revision blob ids for many paths.
/// 批量加载指定修订中多个路径对应的 blob id。
pub(crate) fn webui_git_revision_blob_ids(
    repository_root: &Path,
    revision: &str,
    paths: &[String],
) -> BTreeMap<String, String> {
    webui_git_blob_ids_from_metadata_command(
        repository_root,
        paths,
        &["ls-tree", "-r", "-z", revision, "--"],
        webui_git_revision_metadata_blob_id,
    )
}

/// Counts lines for path-to-blob mappings through one batched cat-file call.
/// 通过一次批量 cat-file 调用统计路径到 blob 映射的行数。
pub(crate) fn webui_git_blob_line_counts(
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
pub(crate) fn parse_webui_git_cat_file_line_counts(output: &[u8]) -> BTreeMap<String, usize> {
    let mut line_counts = BTreeMap::new();
    let mut cursor = 0usize;
    while cursor < output.len() {
        let Some(header_end_offset) = output[cursor..].iter().position(|byte| *byte == b'\n')
        else {
            break;
        };
        let header_end = cursor + header_end_offset;
        let header = decode_shell_output_bytes(&output[cursor..header_end]);
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
pub(crate) fn webui_line_count_from_bytes(bytes: &[u8]) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies index metadata only accepts stage-zero blobs.
    /// 验证索引元数据只接受 stage-zero blob。
    #[test]
    fn webui_git_index_metadata_blob_id_accepts_stage_zero_only() {
        assert_eq!(
            webui_git_index_metadata_blob_id("100644 abcdef123456 0"),
            Some("abcdef123456")
        );
        assert_eq!(
            webui_git_index_metadata_blob_id("100644 abcdef123456 1"),
            None
        );
        assert_eq!(
            webui_git_index_metadata_blob_id("100644 abcdef123456"),
            None
        );
    }

    /// Verifies revision metadata only accepts tree blob entries.
    /// 验证修订元数据只接受树中的 blob 条目。
    #[test]
    fn webui_git_revision_metadata_blob_id_accepts_blob_entries_only() {
        assert_eq!(
            webui_git_revision_metadata_blob_id("100644 blob fedcba654321"),
            Some("fedcba654321")
        );
        assert_eq!(
            webui_git_revision_metadata_blob_id("040000 tree fedcba654321"),
            None
        );
        assert_eq!(
            webui_git_revision_metadata_blob_id("100644 fedcba654321"),
            None
        );
    }

    /// Verifies cat-file batch parsing counts only blob payload lines.
    /// 验证 cat-file 批量输出解析只统计 blob 载荷行数。
    #[test]
    fn parse_webui_git_cat_file_line_counts_counts_blob_payloads() {
        let output = concat!(
            "111111 blob 7\n",
            "one\ntwo\n",
            "222222 tree 4\n",
            "skip\n",
            "333333 blob 5\n",
            "last\n"
        );

        let line_counts = parse_webui_git_cat_file_line_counts(output.as_bytes());

        assert_eq!(line_counts.get("111111"), Some(&2));
        assert_eq!(line_counts.get("222222"), None);
        assert_eq!(line_counts.get("333333"), Some(&1));
    }
}
