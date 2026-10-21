//! WebUI Git diff snapshot assembly.
//! WebUI Git diff 快照组装逻辑。

use super::diff::{
    load_webui_current_branch_unpushed_commit_count, load_webui_current_git_branch,
    load_webui_default_git_branch, load_webui_git_branches,
    load_webui_selected_workspace_unified_diff, load_webui_unpushed_commits,
    resolve_webui_workspace_git_resolution,
};
use super::diff_text::{
    annotate_webui_diff_file_line_counts_for_selection, webui_large_diff_omitted_file_count,
    webui_unified_diff_file_summaries, webui_unified_diff_stats,
    webui_unified_diff_without_large_files,
};
use super::model::{
    WebuiWorkspaceDiffRequest, WebuiWorkspaceDiffSnapshot, default_webui_workspace_diff_request,
};
use super::repository::resolve_webui_repository_root_for_request;
use super::selection::webui_workspace_diff_selection;
use crate::normalized_workspace_path_text_from_path;

/// Loads a complete Git diff snapshot for the requested workspace path.
/// 为请求的工作区路径加载完整 Git diff 快照。
pub(crate) fn load_webui_workspace_diff_snapshot(
    workspace_path: &str,
    request: Option<WebuiWorkspaceDiffRequest>,
) -> Result<WebuiWorkspaceDiffSnapshot, String> {
    let request = request.unwrap_or_else(default_webui_workspace_diff_request);
    let resolution = resolve_webui_workspace_git_resolution(workspace_path)?;
    let repository_root = resolve_webui_repository_root_for_request(
        &resolution.workspace_path,
        request.repository_key.as_deref(),
        resolution.repository_root,
    )?;
    let Some(repository_root) = repository_root else {
        return Ok(empty_webui_diff_snapshot(
            normalized_workspace_path_text_from_path(&resolution.workspace_path),
            String::from(
                "The current workspace is not a Git repository; the code review diff cannot be generated.",
            ),
        ));
    };

    // Normalize the optional request before combining it with Git selector sources.
    // 先标准化可选请求，再与 Git 选择器来源组合。
    // The branch and commit queries are independent Git processes, so they run side by side.
    // 分支与提交查询是互不依赖的 Git 进程，因此并行执行。
    let root = repository_root.as_path();
    let (branch_facts, current_branch_unpushed_commits, default_branch, recent_commits) =
        std::thread::scope(|scope| {
            let branch_facts = scope.spawn(|| {
                let current_branch = load_webui_current_git_branch(root)?;
                let branches = load_webui_git_branches(root, current_branch.as_deref())?;
                Ok::<_, String>((current_branch, branches))
            });
            let unpushed_count =
                scope.spawn(|| load_webui_current_branch_unpushed_commit_count(root));
            let default_branch = scope.spawn(|| load_webui_default_git_branch(root));
            // Load only current-branch commits that have not reached the relevant remote view.
            // 仅加载当前分支中尚未进入相关远端视图的提交。
            let recent_commits = scope.spawn(|| load_webui_unpushed_commits(root));
            (
                joined(branch_facts),
                joined(unpushed_count),
                joined(default_branch),
                joined(recent_commits),
            )
        });
    let (current_branch, branches) = branch_facts?;
    let current_branch_unpushed_commits = current_branch_unpushed_commits?;
    let default_branch = default_branch?;
    let recent_commits = recent_commits?;
    let selection =
        webui_workspace_diff_selection(request, current_branch, default_branch, recent_commits);
    let loaded_diff = load_webui_selected_workspace_unified_diff(
        &repository_root,
        selection.request.mode,
        selection.selected_base_branch.as_deref(),
        selection.selected_compare_branch.as_deref(),
        selection.selected_commit_hash.as_deref(),
        "The current branch has no unpushed commits to show.",
    )?;
    let mut notices = loaded_diff.notices;
    let complete_unified_diff = loaded_diff.unified_diff;
    let stats = webui_unified_diff_stats(&complete_unified_diff);
    let mut file_summaries = webui_unified_diff_file_summaries(&complete_unified_diff);
    annotate_webui_diff_file_line_counts_for_selection(
        &repository_root,
        &selection,
        &mut file_summaries,
    );
    let omitted_large_file_count = webui_large_diff_omitted_file_count(&file_summaries);
    if omitted_large_file_count > 0 {
        notices.push(format!(
            "Skipped the bodies of {omitted_large_file_count} files exceeding the WebUI rendering threshold; expand a file and open its location to view it."
        ));
    }
    let unified_diff = if omitted_large_file_count > 0 {
        webui_unified_diff_without_large_files(&complete_unified_diff, &file_summaries)
    } else {
        complete_unified_diff
    };
    let notice = if stats.files == 0 {
        Some(String::from(
            "The current workspace has no Git changes to show.",
        ))
    } else if notices.is_empty() {
        None
    } else {
        Some(notices.join(" "))
    };

    Ok(WebuiWorkspaceDiffSnapshot {
        git_repository: true,
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
        current_branch: selection.current_branch,
        current_branch_unpushed_commits,
        default_branch: selection.default_branch,
        selected_base_branch: selection.selected_base_branch,
        selected_compare_branch: selection.selected_compare_branch,
        selected_commit_hash: selection.selected_commit_hash,
        branches,
        recent_commits: selection.recent_commits,
    })
}

/// Builds an empty WebUI diff snapshot with one user-facing notice.
/// 构建带有单条用户提示的空 WebUI diff 快照。
fn empty_webui_diff_snapshot(
    repository_root: String,
    notice: String,
) -> WebuiWorkspaceDiffSnapshot {
    WebuiWorkspaceDiffSnapshot {
        git_repository: false,
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

/// Joins one scoped Git query thread, re-raising its panic.
/// 汇合一个作用域 Git 查询线程，并重新抛出其 panic。
fn joined<R>(handle: std::thread::ScopedJoinHandle<'_, R>) -> R {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}
