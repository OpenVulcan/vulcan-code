//! Git diff search input helpers.
//! Git diff 搜索 input 辅助逻辑。

use super::*;

#[derive(Clone)]
pub(crate) struct WebuiDiffSearchInput {
    /// Complete unified diff text used by the backend scanner.
    /// 后端扫描器使用的完整 unified diff 文本。
    pub(crate) unified_diff: String,
    /// File-level candidate index precomputed from the complete diff text.
    /// 从完整 diff 文本预先计算出的文件级候选索引。
    pub(crate) indexed_files: Vec<WebuiDiffSearchIndexedFile>,
    /// Number of raw diff lines in the complete diff text.
    /// 完整 diff 文本中的原始 diff 行数。
    pub(crate) raw_line_count: usize,
    /// User-facing notices produced while loading the diff input.
    /// 加载 diff 输入时产生的面向用户提示。
    pub(crate) notices: Vec<String>,
}

/// Loads the complete diff input used by backend summary search.
/// 加载后端摘要搜索使用的完整 diff 输入。
pub(crate) fn load_webui_workspace_diff_search_input(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
) -> Result<WebuiDiffSearchInput, String> {
    let mut notices = Vec::new();
    let mut sections = Vec::new();
    match mode {
        WebuiWorkspaceDiffMode::Unstaged => {
            let unstaged_diff = load_webui_unstaged_git_diff(repository_root)?;
            if !unstaged_diff.trim().is_empty() {
                sections.push(unstaged_diff);
            }
            let untracked_diff = load_webui_untracked_git_diff(repository_root, &mut notices)?;
            if !untracked_diff.trim().is_empty() {
                sections.push(untracked_diff);
            }
        }
        WebuiWorkspaceDiffMode::Staged => {
            let staged_diff = load_webui_staged_git_diff(repository_root)?;
            if !staged_diff.trim().is_empty() {
                sections.push(staged_diff);
            }
        }
        WebuiWorkspaceDiffMode::Commit => {
            if let Some(commit_hash) = selected_commit_hash {
                let commit_diff = load_webui_commit_git_diff(repository_root, commit_hash)?;
                if !commit_diff.trim().is_empty() {
                    sections.push(commit_diff);
                }
            } else {
                notices.push(String::from("当前仓库没有可搜索的提交记录。"));
            }
        }
        WebuiWorkspaceDiffMode::Branch => match (selected_base_branch, selected_compare_branch) {
            (Some(base_branch), Some(compare_branch)) => {
                let branch_diff =
                    load_webui_branch_git_diff(repository_root, compare_branch, base_branch)?;
                if !branch_diff.trim().is_empty() {
                    sections.push(branch_diff);
                }
            }
            _ => notices.push(String::from("当前仓库无法解析用于分支对比的分支。")),
        },
    }

    let unified_diff = sections.join("\n");
    let summaries = webui_unified_diff_file_summaries(&unified_diff);
    let indexed_files = webui_unified_diff_search_index(&unified_diff, &summaries);
    let raw_line_count = webui_unified_diff_line_count(&unified_diff);
    Ok(WebuiDiffSearchInput {
        unified_diff,
        indexed_files,
        raw_line_count,
        notices,
    })
}
