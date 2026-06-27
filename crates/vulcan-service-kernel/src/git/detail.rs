//! WebUI Git diff file-detail assembly.
//! WebUI Git diff 文件详情组装逻辑。

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::diff::{
    load_webui_branch_git_diff_for_path, load_webui_commit_git_diff_for_path,
    load_webui_current_git_branch, load_webui_default_git_branch, load_webui_recent_commits,
    load_webui_staged_git_diff_for_path, load_webui_unstaged_git_diff_for_path,
    load_webui_untracked_git_diff_for_path, resolve_webui_git_repository_root,
};
use super::diff_text::{
    annotate_webui_diff_file_line_counts, ensure_webui_git_pathspec_text,
    should_omit_webui_large_diff_file, webui_diff_hunk_scoped_unified_diff,
    webui_unified_diff_file_summaries,
};
use super::model::{
    WebuiWorkspaceDiffFileDetail, WebuiWorkspaceDiffFileDetailRequest, WebuiWorkspaceDiffMode,
    WebuiWorkspaceDiffRequest, default_webui_workspace_diff_request,
};
use crate::{
    WEBUI_DIFF_DEFAULT_CONTEXT_LINES, normalized_workspace_path_text_from_path, workspace_root,
};

/// Loads a complete Git diff detail for one requested file.
/// 为请求的单个文件加载完整 Git diff 详情。
pub(crate) fn load_webui_workspace_diff_file_detail(
    workspace_path: &str,
    request: Option<WebuiWorkspaceDiffRequest>,
    file: WebuiWorkspaceDiffFileDetailRequest,
) -> Result<WebuiWorkspaceDiffFileDetail, String> {
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
        return Ok(WebuiWorkspaceDiffFileDetail {
            repository_root: normalized_workspace_path_text_from_path(&workspace_path),
            file: None,
            unified_diff: String::new(),
            truncated: false,
            notice: Some(String::from("当前工作区不是 Git 仓库，无法加载文件 diff。")),
            context_lines: WEBUI_DIFF_DEFAULT_CONTEXT_LINES,
            hunk_index: file.hunk_index,
            hunk_expansion_direction: normalized_webui_diff_hunk_expansion_direction(
                file.hunk_expansion_direction.as_deref(),
            ),
        });
    };

    let request = request.unwrap_or_else(default_webui_workspace_diff_request);
    let pathspec = webui_diff_file_detail_pathspec(&file)?;
    let context_lines = normalized_webui_diff_context_lines(file.context_lines);
    let hunk_index = file.hunk_index;
    let hunk_expansion_direction =
        normalized_webui_diff_hunk_expansion_direction(file.hunk_expansion_direction.as_deref());
    let current_branch = load_webui_current_git_branch(&repository_root)?;
    let default_branch = load_webui_default_git_branch(&repository_root)?;
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
    let mut unified_diff = load_webui_workspace_diff_file_detail_unified_diff(
        &repository_root,
        request.mode,
        selected_base_branch.as_deref(),
        selected_compare_branch.as_deref(),
        selected_commit_hash.as_deref(),
        &pathspec,
        context_lines,
        &mut notices,
    )?;
    let mut unfiltered_file_summaries = webui_unified_diff_file_summaries(&unified_diff);
    annotate_webui_diff_file_line_counts(
        &repository_root,
        request.mode,
        selected_base_branch.as_deref(),
        selected_compare_branch.as_deref(),
        selected_commit_hash.as_deref(),
        &mut unfiltered_file_summaries,
    );
    let unfiltered_file_summary = unfiltered_file_summaries.into_iter().next();
    if unfiltered_file_summary
        .as_ref()
        .is_some_and(should_omit_webui_large_diff_file)
    {
        return Ok(WebuiWorkspaceDiffFileDetail {
            repository_root: normalized_workspace_path_text_from_path(&repository_root),
            file: unfiltered_file_summary,
            unified_diff: String::new(),
            truncated: true,
            notice: Some(String::from(
                "该文件超过 WebUI 渲染阈值，后端未传输文件 diff 正文。可打开文件所在位置查看完整内容。",
            )),
            context_lines,
            hunk_index,
            hunk_expansion_direction,
        });
    }
    if let Some(scoped_diff) = webui_diff_detail_hunk_scoped_unified_diff(
        &repository_root,
        request.mode,
        selected_base_branch.as_deref(),
        selected_compare_branch.as_deref(),
        selected_commit_hash.as_deref(),
        &pathspec,
        context_lines,
        hunk_index,
        hunk_expansion_direction.as_deref(),
        &unified_diff,
    )? {
        unified_diff = scoped_diff;
    }
    let mut file_summaries = webui_unified_diff_file_summaries(&unified_diff);
    annotate_webui_diff_file_line_counts(
        &repository_root,
        request.mode,
        selected_base_branch.as_deref(),
        selected_compare_branch.as_deref(),
        selected_commit_hash.as_deref(),
        &mut file_summaries,
    );
    let file_summary = file_summaries.into_iter().next();
    let notice = if unified_diff.trim().is_empty() {
        Some(String::from("该文件当前没有可加载的 Git diff 详情。"))
    } else if notices.is_empty() {
        None
    } else {
        Some(notices.join(" "))
    };

    Ok(WebuiWorkspaceDiffFileDetail {
        repository_root: normalized_workspace_path_text_from_path(&repository_root),
        file: file_summary,
        unified_diff,
        truncated: false,
        notice,
        context_lines,
        hunk_index,
        hunk_expansion_direction,
    })
}

/// Loads one file-detail unified diff for a selected WebUI diff mode.
/// 按选中的 WebUI diff 模式加载单文件详情 unified diff。
fn load_webui_workspace_diff_file_detail_unified_diff(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
    pathspec: &str,
    context_lines: usize,
    notices: &mut Vec<String>,
) -> Result<String, String> {
    let mut sections = Vec::new();
    match mode {
        WebuiWorkspaceDiffMode::Unstaged => {
            let unstaged_diff =
                load_webui_unstaged_git_diff_for_path(repository_root, pathspec, context_lines)?;
            if !unstaged_diff.trim().is_empty() {
                sections.push(unstaged_diff);
            }
            let untracked_diff =
                load_webui_untracked_git_diff_for_path(repository_root, pathspec, notices)?;
            if !untracked_diff.trim().is_empty() {
                sections.push(untracked_diff);
            }
        }
        WebuiWorkspaceDiffMode::Staged => {
            let staged_diff =
                load_webui_staged_git_diff_for_path(repository_root, pathspec, context_lines)?;
            if !staged_diff.trim().is_empty() {
                sections.push(staged_diff);
            }
        }
        WebuiWorkspaceDiffMode::Commit => {
            if let Some(commit_hash) = selected_commit_hash {
                let commit_diff = load_webui_commit_git_diff_for_path(
                    repository_root,
                    commit_hash,
                    pathspec,
                    context_lines,
                )?;
                if !commit_diff.trim().is_empty() {
                    sections.push(commit_diff);
                }
            } else {
                notices.push(String::from("当前仓库没有可展示的提交记录。"));
            }
        }
        WebuiWorkspaceDiffMode::Branch => match (selected_base_branch, selected_compare_branch) {
            (Some(base_branch), Some(compare_branch)) => {
                let branch_diff = load_webui_branch_git_diff_for_path(
                    repository_root,
                    compare_branch,
                    base_branch,
                    pathspec,
                    context_lines,
                )?;
                if !branch_diff.trim().is_empty() {
                    sections.push(branch_diff);
                }
            }
            _ => notices.push(String::from("当前仓库无法解析用于分支对比的分支。")),
        },
    }
    Ok(sections.join("\n"))
}

/// Builds a hunk-scoped expanded diff payload for targeted WebUI hunk expansion.
/// 为定向 WebUI hunk 展开构建仅包含目标 hunk 的扩展 diff 载荷。
fn webui_diff_detail_hunk_scoped_unified_diff(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
    pathspec: &str,
    context_lines: usize,
    hunk_index: Option<usize>,
    hunk_expansion_direction: Option<&str>,
    expanded_unified_diff: &str,
) -> Result<Option<String>, String> {
    if hunk_index.is_none()
        || hunk_expansion_direction.is_none()
        || context_lines <= WEBUI_DIFF_DEFAULT_CONTEXT_LINES
        || expanded_unified_diff.trim().is_empty()
    {
        return Ok(None);
    }
    let mut base_notices = Vec::new();
    let base_unified_diff = load_webui_workspace_diff_file_detail_unified_diff(
        repository_root,
        mode,
        selected_base_branch,
        selected_compare_branch,
        selected_commit_hash,
        pathspec,
        WEBUI_DIFF_DEFAULT_CONTEXT_LINES,
        &mut base_notices,
    )?;
    Ok(webui_diff_hunk_scoped_unified_diff(
        &base_unified_diff,
        expanded_unified_diff,
        hunk_index.unwrap_or_default(),
    ))
}

/// Normalizes an optional WebUI diff context-line request into the supported range.
/// 将可选 WebUI diff 上下文行请求规范化到受支持范围内。
fn normalized_webui_diff_context_lines(context_lines: Option<usize>) -> usize {
    context_lines
        .unwrap_or(WEBUI_DIFF_DEFAULT_CONTEXT_LINES)
        .max(WEBUI_DIFF_DEFAULT_CONTEXT_LINES)
}

/// Normalizes an optional WebUI hunk expansion direction.
/// 规范化可选的 WebUI hunk 展开方向。
fn normalized_webui_diff_hunk_expansion_direction(direction: Option<&str>) -> Option<String> {
    match direction.map(str::trim) {
        Some("up") => Some(String::from("up")),
        Some("down") => Some(String::from("down")),
        Some("both") => Some(String::from("both")),
        _ => None,
    }
}

/// Returns the safe Git pathspec for one requested file detail.
/// 返回单个文件详情请求对应的安全 Git 路径规格。
fn webui_diff_file_detail_pathspec(
    file: &WebuiWorkspaceDiffFileDetailRequest,
) -> Result<String, String> {
    if file.key.contains('\0') || file.key.contains('\n') || file.key.contains('\r') {
        return Err(String::from("invalid WebUI diff file key"));
    }
    let candidate = if !file.new_path.trim().is_empty() && file.new_path != "/dev/null" {
        file.new_path.trim()
    } else {
        file.old_path.trim()
    };
    ensure_webui_git_pathspec_text(candidate)?;
    Ok(candidate.replace('\\', "/"))
}
