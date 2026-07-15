//! Git diff search input helpers.
//! Git diff 搜索 input 辅助逻辑。

use super::*;
use crate::git::diff::load_webui_selected_workspace_unified_diff;
use crate::git::selection::WebuiWorkspaceDiffSelection;

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

/// Loads the complete diff input using the selected WebUI diff request context.
/// 使用已解析的 WebUI diff 选择上下文加载完整 diff 输入。
pub(crate) fn load_webui_workspace_diff_search_input_for_selection(
    repository_root: &Path,
    selection: &WebuiWorkspaceDiffSelection,
) -> Result<WebuiDiffSearchInput, String> {
    load_webui_workspace_diff_search_input(
        repository_root,
        selection.request.mode,
        selection.selected_base_branch.as_deref(),
        selection.selected_compare_branch.as_deref(),
        selection.selected_commit_hash.as_deref(),
    )
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
    let loaded_diff = load_webui_selected_workspace_unified_diff(
        repository_root,
        mode,
        selected_base_branch,
        selected_compare_branch,
        selected_commit_hash,
        "当前分支没有可搜索的未推送提交。",
    )?;
    let unified_diff = loaded_diff.unified_diff;
    let summaries = webui_unified_diff_file_summaries(&unified_diff);
    let indexed_files = webui_unified_diff_search_index(&unified_diff, &summaries);
    let raw_line_count = webui_unified_diff_line_count(&unified_diff);
    Ok(WebuiDiffSearchInput {
        unified_diff,
        indexed_files,
        raw_line_count,
        notices: loaded_diff.notices,
    })
}
