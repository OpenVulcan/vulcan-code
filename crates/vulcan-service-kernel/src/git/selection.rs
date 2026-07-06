//! Shared WebUI Git diff selection resolution.
//! WebUI Git diff 选择项解析共享逻辑。

use std::path::Path;

use super::diff::{
    load_webui_current_git_branch, load_webui_default_git_branch, load_webui_recent_commits,
};
use super::model::{
    WebuiGitCommitSummary, WebuiWorkspaceDiffRequest, default_webui_workspace_diff_request,
};

/// Resolved WebUI diff request and its derived selectable defaults.
/// 已解析的 WebUI diff 请求及其派生选择默认值。
pub(crate) struct WebuiWorkspaceDiffSelection {
    /// Effective WebUI diff request after applying the default request.
    /// 应用默认请求后的有效 WebUI diff 请求。
    pub(crate) request: WebuiWorkspaceDiffRequest,
    /// Current Git branch when the repository is on a named branch.
    /// 仓库位于具名分支时的当前 Git 分支。
    pub(crate) current_branch: Option<String>,
    /// Default Git branch resolved for comparison selectors.
    /// 为对比选择器解析出的默认 Git 分支。
    pub(crate) default_branch: Option<String>,
    /// Recent commits used by commit mode selectors.
    /// 提交模式选择器使用的最近提交列表。
    pub(crate) recent_commits: Vec<WebuiGitCommitSummary>,
    /// Selected base branch after trimming request text and applying defaults.
    /// 裁剪请求文本并应用默认值后的选中基准分支。
    pub(crate) selected_base_branch: Option<String>,
    /// Selected compare branch after trimming request text and applying defaults.
    /// 裁剪请求文本并应用默认值后的选中对比分支。
    pub(crate) selected_compare_branch: Option<String>,
    /// Selected commit hash after trimming request text and applying defaults.
    /// 裁剪请求文本并应用默认值后的选中提交哈希。
    pub(crate) selected_commit_hash: Option<String>,
}

/// Loads the shared WebUI diff selection context from a repository.
/// 从仓库加载共享的 WebUI diff 选择上下文。
pub(crate) fn load_webui_workspace_diff_selection(
    repository_root: &Path,
    request: Option<WebuiWorkspaceDiffRequest>,
) -> Result<WebuiWorkspaceDiffSelection, String> {
    // Normalize the optional caller request once so every diff entrypoint uses the same defaults.
    // 只标准化一次调用方可选请求，确保所有 diff 入口使用同一套默认值。
    let request = request.unwrap_or_else(default_webui_workspace_diff_request);
    // Load branch and commit selector sources from Git before deriving selected values.
    // 先从 Git 加载分支与提交选择器来源，再派生选中值。
    let current_branch = load_webui_current_git_branch(repository_root)?;
    // Load the comparison default branch used when the request omits compare_branch.
    // 加载请求省略 compare_branch 时使用的默认对比分支。
    let default_branch = load_webui_default_git_branch(repository_root)?;
    // Load recent commits used as the commit-mode fallback.
    // 加载提交模式兜底使用的最近提交。
    let recent_commits = load_webui_recent_commits(repository_root)?;
    Ok(webui_workspace_diff_selection(
        request,
        current_branch,
        default_branch,
        recent_commits,
    ))
}

/// Resolves selected WebUI diff values from already-loaded selector sources.
/// 基于已加载的选择器来源解析 WebUI diff 选中值。
pub(crate) fn webui_workspace_diff_selection(
    request: WebuiWorkspaceDiffRequest,
    current_branch: Option<String>,
    default_branch: Option<String>,
    recent_commits: Vec<WebuiGitCommitSummary>,
) -> WebuiWorkspaceDiffSelection {
    // Prefer a non-empty request base branch, otherwise fall back to the current branch.
    // 优先使用请求中的非空基准分支，否则回退到当前分支。
    let selected_base_branch = trimmed_webui_diff_request_value(request.base_branch.as_deref())
        .or_else(|| current_branch.clone());
    // Prefer a non-empty request compare branch, otherwise fall back to the default branch.
    // 优先使用请求中的非空对比分支，否则回退到默认分支。
    let selected_compare_branch =
        trimmed_webui_diff_request_value(request.compare_branch.as_deref())
            .or_else(|| default_branch.clone());
    // Prefer a non-empty request commit hash, otherwise use the first recent commit.
    // 优先使用请求中的非空提交哈希，否则使用最近提交中的第一项。
    let selected_commit_hash = trimmed_webui_diff_request_value(request.commit_hash.as_deref())
        .or_else(|| recent_commits.first().map(|commit| commit.hash.clone()));
    WebuiWorkspaceDiffSelection {
        request,
        current_branch,
        default_branch,
        recent_commits,
        selected_base_branch,
        selected_compare_branch,
        selected_commit_hash,
    }
}

/// Trims an optional WebUI diff request selector and drops empty text.
/// 裁剪可选 WebUI diff 请求选择值并丢弃空文本。
fn trimmed_webui_diff_request_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::super::model::WebuiWorkspaceDiffMode;
    use super::*;

    /// Verifies request selector text is trimmed and empty text is ignored.
    /// 验证请求选择文本会被裁剪且空文本会被忽略。
    #[test]
    fn trimmed_webui_diff_request_value_discards_empty_text() {
        assert_eq!(
            trimmed_webui_diff_request_value(Some("  main  ")),
            Some(String::from("main"))
        );
        assert_eq!(trimmed_webui_diff_request_value(Some("   ")), None);
        assert_eq!(trimmed_webui_diff_request_value(None), None);
    }

    /// Verifies branch and commit selections use loaded defaults when request values are empty.
    /// 验证请求值为空时分支与提交选择会使用已加载默认值。
    #[test]
    fn webui_workspace_diff_selection_uses_loaded_defaults() {
        let selection = webui_workspace_diff_selection(
            WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Commit,
                base_branch: Some(String::from(" ")),
                compare_branch: None,
                commit_hash: None,
            },
            Some(String::from("feature/source")),
            Some(String::from("main")),
            vec![WebuiGitCommitSummary {
                hash: String::from("abc123"),
                short_hash: String::from("abc123"),
                subject: String::from("Initial commit"),
                relative_time: String::from("1 minute ago"),
            }],
        );

        assert_eq!(
            selection.selected_base_branch.as_deref(),
            Some("feature/source")
        );
        assert_eq!(selection.selected_compare_branch.as_deref(), Some("main"));
        assert_eq!(selection.selected_commit_hash.as_deref(), Some("abc123"));
    }
}
