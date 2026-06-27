//! WebUI Git diff snapshot assembly.
//! WebUI Git diff 快照组装逻辑。

use std::{fs, path::PathBuf};

use super::diff::{
    load_webui_branch_git_diff, load_webui_commit_git_diff,
    load_webui_current_branch_unpushed_commit_count, load_webui_current_git_branch,
    load_webui_default_git_branch, load_webui_git_branches, load_webui_recent_commits,
    load_webui_staged_git_diff, load_webui_unstaged_git_diff, load_webui_untracked_git_diff,
    resolve_webui_git_repository_root,
};
use super::diff_text::{
    annotate_webui_diff_file_line_counts, webui_large_diff_omitted_file_count,
    webui_unified_diff_file_summaries, webui_unified_diff_stats,
    webui_unified_diff_without_large_files,
};
use super::model::{
    WebuiWorkspaceDiffMode, WebuiWorkspaceDiffRequest, WebuiWorkspaceDiffSnapshot,
    default_webui_workspace_diff_request,
};
use crate::{normalized_workspace_path_text_from_path, workspace_root};

/// Loads a complete Git diff snapshot for the requested workspace path.
/// 为请求的工作区路径加载完整 Git diff 快照。
pub(crate) fn load_webui_workspace_diff_snapshot(
    workspace_path: &str,
    request: Option<WebuiWorkspaceDiffRequest>,
) -> Result<WebuiWorkspaceDiffSnapshot, String> {
    let requested_path = PathBuf::from(workspace_path.trim());
    let workspace_path = if requested_path.as_os_str().is_empty() {
        workspace_root()
    } else {
        requested_path
    };
    let workspace_path = fs::canonicalize(&workspace_path).map_err(|error| {
        format!("failed to resolve workspace path `{workspace_path:?}`: {error}")
    })?;
    let Some(repository_root) = resolve_webui_git_repository_root(&workspace_path)? else {
        return Ok(empty_webui_diff_snapshot(
            normalized_workspace_path_text_from_path(&workspace_path),
            String::from("当前工作区不是 Git 仓库，无法生成代码审查 diff。"),
        ));
    };

    let request = request.unwrap_or_else(default_webui_workspace_diff_request);
    let current_branch = load_webui_current_git_branch(&repository_root)?;
    let current_branch_unpushed_commits =
        load_webui_current_branch_unpushed_commit_count(&repository_root)?;
    let default_branch = load_webui_default_git_branch(&repository_root)?;
    let branches = load_webui_git_branches(&repository_root, current_branch.as_deref())?;
    let recent_commits = load_webui_recent_commits(&repository_root)?;
    let selected_base_branch = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| current_branch.clone());
    let selected_compare_branch = request
        .compare_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| default_branch.clone());
    let selected_commit_hash = request
        .commit_hash
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| recent_commits.first().map(|commit| commit.hash.clone()));
    let mut notices = Vec::new();
    let mut sections = Vec::new();
    match request.mode {
        WebuiWorkspaceDiffMode::Unstaged => {
            let unstaged_diff = load_webui_unstaged_git_diff(&repository_root)?;
            if !unstaged_diff.trim().is_empty() {
                sections.push(unstaged_diff);
            }
            let untracked_diff = load_webui_untracked_git_diff(&repository_root, &mut notices)?;
            if !untracked_diff.trim().is_empty() {
                sections.push(untracked_diff);
            }
        }
        WebuiWorkspaceDiffMode::Staged => {
            let staged_diff = load_webui_staged_git_diff(&repository_root)?;
            if !staged_diff.trim().is_empty() {
                sections.push(staged_diff);
            }
        }
        WebuiWorkspaceDiffMode::Commit => {
            if let Some(commit_hash) = selected_commit_hash.as_deref() {
                let commit_diff = load_webui_commit_git_diff(&repository_root, commit_hash)?;
                if !commit_diff.trim().is_empty() {
                    sections.push(commit_diff);
                }
            } else {
                notices.push(String::from("当前仓库没有可展示的提交记录。"));
            }
        }
        WebuiWorkspaceDiffMode::Branch => {
            match (
                selected_base_branch.as_deref(),
                selected_compare_branch.as_deref(),
            ) {
                (Some(base_branch), Some(compare_branch)) => {
                    let branch_diff =
                        load_webui_branch_git_diff(&repository_root, compare_branch, base_branch)?;
                    if !branch_diff.trim().is_empty() {
                        sections.push(branch_diff);
                    }
                }
                _ => notices.push(String::from("当前仓库无法解析用于分支对比的分支。")),
            }
        }
    }

    let complete_unified_diff = sections.join("\n");
    let stats = webui_unified_diff_stats(&complete_unified_diff);
    let mut file_summaries = webui_unified_diff_file_summaries(&complete_unified_diff);
    annotate_webui_diff_file_line_counts(
        &repository_root,
        request.mode,
        selected_base_branch.as_deref(),
        selected_compare_branch.as_deref(),
        selected_commit_hash.as_deref(),
        &mut file_summaries,
    );
    let omitted_large_file_count = webui_large_diff_omitted_file_count(&file_summaries);
    if omitted_large_file_count > 0 {
        notices.push(format!(
            "已跳过 {omitted_large_file_count} 个超过 WebUI 渲染阈值的文件正文，可展开文件后打开所在位置查看。"
        ));
    }
    let unified_diff = if omitted_large_file_count > 0 {
        webui_unified_diff_without_large_files(&complete_unified_diff, &file_summaries)
    } else {
        complete_unified_diff
    };
    let notice = if stats.files == 0 {
        Some(String::from("当前工作区没有可展示的 Git 变更。"))
    } else if notices.is_empty() {
        None
    } else {
        Some(notices.join(" "))
    };

    Ok(WebuiWorkspaceDiffSnapshot {
        repository_root: normalized_workspace_path_text_from_path(&repository_root),
        unified_diff,
        changed_files: stats.files,
        files: file_summaries,
        additions: stats.additions,
        deletions: stats.deletions,
        total_changed_lines: stats.total_changed_lines,
        total_changed_bytes: stats.total_changed_bytes,
        truncated: omitted_large_file_count > 0,
        notice,
        current_branch,
        current_branch_unpushed_commits,
        default_branch,
        selected_base_branch,
        selected_compare_branch,
        selected_commit_hash,
        branches,
        recent_commits,
    })
}

/// Builds an empty WebUI diff snapshot with one user-facing notice.
/// 构建带有单条用户提示的空 WebUI diff 快照。
fn empty_webui_diff_snapshot(
    repository_root: String,
    notice: String,
) -> WebuiWorkspaceDiffSnapshot {
    WebuiWorkspaceDiffSnapshot {
        repository_root,
        unified_diff: String::new(),
        changed_files: 0,
        files: Vec::new(),
        additions: 0,
        deletions: 0,
        total_changed_lines: 0,
        total_changed_bytes: 0,
        truncated: false,
        notice: Some(notice),
        current_branch: None,
        current_branch_unpushed_commits: 0,
        default_branch: None,
        selected_base_branch: None,
        selected_compare_branch: None,
        selected_commit_hash: None,
        branches: Vec::new(),
        recent_commits: Vec::new(),
    }
}
