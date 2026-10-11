//! WebUI Git file action service logic.
//! WebUI Git 文件级操作服务逻辑。

use std::{collections::BTreeSet, path::Path};

use super::command::{format_webui_git_error, run_webui_git_command};
use super::diff_text::ensure_webui_git_pathspec_text;
use super::model::{
    WebuiWorkspaceGitFileAction, WebuiWorkspaceGitFileActionFile,
    WebuiWorkspaceGitFileActionRequest, WebuiWorkspaceGitFileActionResult,
};
use super::repository::resolve_webui_repository_root_from_workspace_text_with_key;
use super::search::clear_webui_diff_search_caches;
use crate::normalized_workspace_path_text_from_path;

/// Executes one WebUI file-level Git operation inside a resolved repository.
/// 在解析后的仓库中执行单个 WebUI 文件级 Git 操作。
pub(crate) fn run_webui_workspace_git_file_action(
    workspace_path: &str,
    request: WebuiWorkspaceGitFileActionRequest,
) -> Result<WebuiWorkspaceGitFileActionResult, String> {
    let repository_root = resolve_webui_repository_root_from_workspace_text_with_key(
        workspace_path,
        request.repository_key.as_deref(),
    )?;
    let affected_path_count = match request.action {
        WebuiWorkspaceGitFileAction::StageAll => {
            run_webui_git_action_command(&repository_root, &["add", "-A", "--", "."])?;
            0
        }
        WebuiWorkspaceGitFileAction::RestoreAll => {
            run_webui_git_action_command(&repository_root, &["restore", "--worktree", "--", "."])?;
            run_webui_git_action_command(&repository_root, &["clean", "-fd", "--", "."])?;
            0
        }
        WebuiWorkspaceGitFileAction::StageFile => {
            let pathspecs = webui_git_file_action_pathspecs(&request.files)?;
            run_webui_git_file_action_command(&repository_root, &["add", "--"], &pathspecs)?;
            pathspecs.len()
        }
        WebuiWorkspaceGitFileAction::RestoreFile => {
            let pathspecs = webui_git_file_action_pathspecs(&request.files)?;
            run_webui_restore_file_action(&repository_root, &request.files, &pathspecs)?;
            pathspecs.len()
        }
        WebuiWorkspaceGitFileAction::UnstageAll => {
            run_webui_git_action_command(&repository_root, &["restore", "--staged", "--", "."])?;
            0
        }
        WebuiWorkspaceGitFileAction::UnstageFile => {
            let pathspecs = webui_git_file_action_pathspecs(&request.files)?;
            run_webui_git_file_action_command(
                &repository_root,
                &["restore", "--staged", "--"],
                &pathspecs,
            )?;
            pathspecs.len()
        }
    };
    clear_webui_diff_search_caches();
    Ok(WebuiWorkspaceGitFileActionResult {
        repository_root: normalized_workspace_path_text_from_path(&repository_root),
        action: webui_git_file_action_name(request.action).to_string(),
        affected_path_count,
        summary: webui_git_file_action_summary(request.action, affected_path_count),
    })
}

/// Executes a repository-wide Git action and reports failures with context.
/// 执行仓库级 Git 操作并带上下文报告失败。
fn run_webui_git_action_command(repository_root: &Path, args: &[&str]) -> Result<(), String> {
    let output = run_webui_git_command(repository_root, args)?;
    if output.status.success() {
        return Ok(());
    }
    Err(format_webui_git_error("git file action", &output))
}

/// Executes one file-scoped Git action after appending safe pathspecs.
/// 追加安全路径规格后执行单个文件级 Git 操作。
fn run_webui_git_file_action_command(
    repository_root: &Path,
    prefix_args: &[&str],
    pathspecs: &[String],
) -> Result<(), String> {
    if pathspecs.is_empty() {
        return Err(String::from("Select the files to operate on."));
    }
    let mut args = prefix_args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    args.extend(pathspecs.iter().cloned());
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_webui_git_action_command(repository_root, &arg_refs)
}

/// Discards file-scoped unstaged changes while removing untracked new files.
/// 丢弃文件级未暂存变更，并移除未跟踪的新文件。
fn run_webui_restore_file_action(
    repository_root: &Path,
    files: &[WebuiWorkspaceGitFileActionFile],
    pathspecs: &[String],
) -> Result<(), String> {
    if pathspecs.is_empty() {
        return Err(String::from("Select the files to restore."));
    }
    let mut tracked_pathspecs = BTreeSet::new();
    let mut untracked_pathspecs = BTreeSet::new();
    for file in files {
        let target_set = if file.status.trim() == "added" {
            &mut untracked_pathspecs
        } else {
            &mut tracked_pathspecs
        };
        for pathspec in webui_git_file_action_file_pathspecs(file)? {
            target_set.insert(pathspec);
        }
    }
    let tracked = tracked_pathspecs.into_iter().collect::<Vec<_>>();
    if !tracked.is_empty() {
        run_webui_git_file_action_command(
            repository_root,
            &["restore", "--worktree", "--"],
            &tracked,
        )?;
    }
    let untracked = untracked_pathspecs.into_iter().collect::<Vec<_>>();
    if !untracked.is_empty() {
        run_webui_git_file_action_command(repository_root, &["clean", "-f", "--"], &untracked)?;
    }
    Ok(())
}

/// Builds safe Git pathspecs for a batch of WebUI file action targets.
/// 为一批 WebUI 文件操作目标构建安全 Git 路径规格。
fn webui_git_file_action_pathspecs(
    files: &[WebuiWorkspaceGitFileActionFile],
) -> Result<Vec<String>, String> {
    let mut pathspecs = BTreeSet::new();
    for file in files {
        for pathspec in webui_git_file_action_file_pathspecs(file)? {
            pathspecs.insert(pathspec);
        }
    }
    Ok(pathspecs.into_iter().collect())
}

/// Builds safe Git pathspecs for one WebUI file action target.
/// 为单个 WebUI 文件操作目标构建安全 Git 路径规格。
fn webui_git_file_action_file_pathspecs(
    file: &WebuiWorkspaceGitFileActionFile,
) -> Result<Vec<String>, String> {
    let old_path = normalized_webui_git_action_pathspec(file.old_path.trim())?;
    let new_path = normalized_webui_git_action_pathspec(file.new_path.trim())?;
    let mut pathspecs = Vec::new();
    match file.status.trim() {
        "deleted" => {
            if let Some(pathspec) = old_path {
                pathspecs.push(pathspec);
            }
        }
        "renamed" => {
            if let Some(pathspec) = old_path {
                pathspecs.push(pathspec);
            }
            if let Some(pathspec) = new_path {
                pathspecs.push(pathspec);
            }
        }
        _ => {
            if let Some(pathspec) = new_path.or(old_path) {
                pathspecs.push(pathspec);
            }
        }
    }
    if pathspecs.is_empty() {
        return Err(String::from(
            "Invalid file path; the Git file operation cannot run.",
        ));
    }
    Ok(pathspecs)
}

/// Normalizes an optional WebUI Git action pathspec.
/// 规范化可选的 WebUI Git 操作路径规格。
fn normalized_webui_git_action_pathspec(pathspec: &str) -> Result<Option<String>, String> {
    if pathspec.is_empty() || pathspec == "/dev/null" {
        return Ok(None);
    }
    ensure_webui_git_pathspec_text(pathspec)?;
    Ok(Some(pathspec.replace('\\', "/")))
}

/// Returns the stable action name used in WebUI Git action results.
/// 返回 WebUI Git 操作结果使用的稳定操作名称。
fn webui_git_file_action_name(action: WebuiWorkspaceGitFileAction) -> &'static str {
    match action {
        WebuiWorkspaceGitFileAction::StageAll => "stageAll",
        WebuiWorkspaceGitFileAction::RestoreAll => "restoreAll",
        WebuiWorkspaceGitFileAction::StageFile => "stageFile",
        WebuiWorkspaceGitFileAction::RestoreFile => "restoreFile",
        WebuiWorkspaceGitFileAction::UnstageAll => "unstageAll",
        WebuiWorkspaceGitFileAction::UnstageFile => "unstageFile",
    }
}

/// Builds a compact user-facing summary for one completed Git file action.
/// 为已完成的 Git 文件操作构建紧凑的用户摘要。
fn webui_git_file_action_summary(action: WebuiWorkspaceGitFileAction, path_count: usize) -> String {
    match action {
        WebuiWorkspaceGitFileAction::StageAll => String::from("Staged all changes."),
        WebuiWorkspaceGitFileAction::RestoreAll => String::from("Restored all unstaged changes."),
        WebuiWorkspaceGitFileAction::StageFile => format!("Staged {path_count} file paths."),
        WebuiWorkspaceGitFileAction::RestoreFile => {
            format!("Restored the unstaged changes of {path_count} file paths.")
        }
        WebuiWorkspaceGitFileAction::UnstageAll => String::from("Unstaged all changes."),
        WebuiWorkspaceGitFileAction::UnstageFile => format!("Unstaged {path_count} file paths."),
    }
}
