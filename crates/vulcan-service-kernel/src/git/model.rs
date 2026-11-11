//! Git-facing WebUI request and response models for the service kernel.
//! 服务内核面向 Git WebUI 的请求与响应模型。

use serde::{Deserialize, Serialize};
use vulcan_core::types::ProviderUsageSnapshot;

/// Maximum file-level search hits returned for one WebUI diff search.
/// 单次 WebUI diff 搜索返回的最大文件级命中数。
pub(crate) const WEBUI_DIFF_SEARCH_MAX_RESULTS: usize = 200;

/// Git commit-message generation request sent by the WebUI commit dialog.
/// WebUI 提交弹窗发送的 Git 提交信息生成请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitCommitSummaryGenerateRequest {
    /// Conversation id whose active model may be used for generation.
    /// 可用于生成提交信息的会话标识。
    pub(crate) conversation_id: String,
    /// Workspace path used to resolve the Git repository.
    /// 用于解析 Git 仓库的工作区路径。
    pub(crate) workspace_path: String,
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
    /// Whether unstaged and untracked changes are included in the summary input.
    /// 是否将未暂存与未跟踪变更纳入总结输入。
    pub(crate) include_unstaged: bool,
}

/// Git commit-message generation response returned to the WebUI commit dialog.
/// 返回给 WebUI 提交弹窗的 Git 提交信息生成响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitCommitSummaryGenerateResponse {
    /// Generated commit message text.
    /// 生成的提交信息文本。
    pub(crate) message: String,
    /// Provider id used for the internal summary request.
    /// 内部总结请求使用的 Provider 标识。
    pub(crate) provider_id: String,
    /// Model id used for the internal summary request.
    /// 内部总结请求使用的模型标识。
    pub(crate) model_id: String,
    /// Real provider request count consumed by the internal summary request.
    /// 内部总结请求消耗的真实 Provider 请求次数。
    pub(crate) provider_request_count: usize,
    /// Whether the full diff exceeded the configured prompt budget and was replaced by summaries.
    /// 完整 diff 是否超过提示预算并被替换为摘要。
    pub(crate) input_was_truncated: bool,
    /// Input-token estimate for the final summary prompt.
    /// 最终总结提示词的输入 Token 估算值。
    pub(crate) estimated_input_tokens: usize,
    /// Provider-reported usage from the successful generation request when available.
    /// 成功生成请求返回的 Provider 用量；不可用时为空。
    pub(crate) provider_usage: Option<ProviderUsageSnapshot>,
}

/// Git commit action request sent by the WebUI commit dialog.
/// WebUI 提交弹窗发送的 Git 提交操作请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitCommitActionRequest {
    /// Conversation id that owns the commit action.
    /// 拥有本次提交动作的会话标识。
    pub(crate) conversation_id: String,
    /// Workspace path used to resolve the Git repository.
    /// 用于解析 Git 仓库的工作区路径。
    pub(crate) workspace_path: String,
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
    /// Target branch selection for the commit.
    /// 本次提交的目标分支选择。
    pub(crate) target: GitCommitActionTargetRequest,
    /// Whether unstaged and untracked changes should be staged before committing.
    /// 是否在提交前暂存未暂存与未跟踪变更。
    pub(crate) include_unstaged: bool,
    /// Commit message text passed to `git commit`.
    /// 传递给 `git commit` 的提交信息文本。
    pub(crate) message: String,
    /// Whether the resulting branch should be pushed after commit.
    /// 提交完成后是否推送当前分支。
    pub(crate) push: bool,
}

/// Git commit target selected by the WebUI commit dialog.
/// WebUI 提交弹窗选择的 Git 提交目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum GitCommitActionTargetRequest {
    /// Commit directly on the current branch.
    /// 直接提交到当前分支。
    Current,
    /// Create a new branch before committing.
    /// 提交前创建新分支。
    New {
        /// New branch name requested by the user.
        /// 用户请求的新分支名称。
        branch_name: String,
    },
}

/// Git commit action response returned to the WebUI commit dialog.
/// 返回给 WebUI 提交弹窗的 Git 提交操作响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitCommitActionResponse {
    /// Human-readable operation summary.
    /// 面向用户的操作摘要。
    pub(crate) summary: String,
    /// Whether a new commit was created.
    /// 是否创建了新提交。
    pub(crate) committed: bool,
    /// Whether a push command was executed.
    /// 是否执行了推送命令。
    pub(crate) pushed: bool,
    /// Current branch name after the operation.
    /// 操作完成后的当前分支名称。
    pub(crate) branch_name: Option<String>,
    /// Short hash for the HEAD commit affected by the operation.
    /// 本次操作涉及的 HEAD 提交短哈希。
    pub(crate) commit_hash: Option<String>,
    /// Commit message subject for the HEAD commit affected by the operation.
    /// 本次操作涉及的 HEAD 提交信息标题。
    pub(crate) commit_message: Option<String>,
}

/// Git pull request sent by the session tools commit menu.
/// 会话工具提交菜单发送的 Git 拉取请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitPullActionRequest {
    /// Conversation id that owns the pull action.
    /// 拥有本次拉取动作的会话标识。
    pub(crate) conversation_id: String,
    /// Workspace path used to resolve the Git repository.
    /// 用于解析 Git 仓库的工作区路径。
    pub(crate) workspace_path: String,
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
}

/// Git pull response returned to the session tools commit menu.
/// 返回给会话工具提交菜单的 Git 拉取响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitPullActionResponse {
    /// Human-readable operation summary.
    /// 面向用户的操作摘要。
    pub(crate) summary: String,
    /// Whether HEAD moved; false means the branch was already up to date.
    /// HEAD 是否移动；为 false 表示分支已是最新。
    pub(crate) updated: bool,
    /// Current branch name after the pull.
    /// 拉取完成后的当前分支名称。
    pub(crate) branch_name: Option<String>,
    /// Short hash of HEAD after the pull.
    /// 拉取后 HEAD 的短哈希。
    pub(crate) commit_hash: Option<String>,
    /// Commit message subject of HEAD after the pull.
    /// 拉取后 HEAD 的提交信息标题。
    pub(crate) commit_message: Option<String>,
}

/// WebUI workspace Git diff snapshot returned by the service bridge.
/// 服务桥接层返回给 WebUI 的工作区 Git diff 快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffSnapshot {
    /// Whether the workspace resolved to a Git repository; false means every Git field is empty.
    /// 工作区是否解析到 Git 仓库；为 false 时所有 Git 字段均为空。
    pub(crate) git_repository: bool,
    /// Repository root used for the Git command.
    /// Git 命令使用的仓库根目录。
    pub(crate) repository_root: String,
    /// Complete combined unified diff for tracked and untracked text changes.
    /// 已跟踪变更与未跟踪文本变更组合后的完整 unified diff。
    pub(crate) unified_diff: String,
    /// Number of files represented by the combined diff.
    /// 组合 diff 中包含的文件数量。
    pub(crate) changed_files: usize,
    /// Complete file summaries computed from the returned diff.
    /// 基于返回 diff 计算出的完整文件摘要。
    pub(crate) files: Vec<WebuiWorkspaceDiffFileSummary>,
    /// Number of added lines estimated from the combined diff.
    /// 从组合 diff 中估算出的新增行数。
    pub(crate) additions: usize,
    /// Number of deleted lines estimated from the combined diff.
    /// 从组合 diff 中估算出的删除行数。
    pub(crate) deletions: usize,
    /// Number of changed lines estimated from the complete combined diff.
    /// 从完整组合 diff 中估算出的变更行数。
    pub(crate) total_changed_lines: usize,
    /// Number of changed bytes estimated from the complete combined diff.
    /// 从完整组合 diff 中估算出的变更字节数。
    pub(crate) total_changed_bytes: usize,
    /// Compatibility flag for legacy clients; normal WebUI diff payloads are returned complete.
    /// 面向旧客户端的兼容标记；正常 WebUI diff 载荷会完整返回。
    pub(crate) truncated: bool,
    /// Optional user-facing note for empty or partial snapshots.
    /// 空快照或局部快照使用的可选用户提示。
    pub(crate) notice: Option<String>,
    /// Current Git branch name when available.
    /// 可用时的当前 Git 分支名称。
    pub(crate) current_branch: Option<String>,
    /// Number of current-branch commits that are not present on the configured remote view.
    /// 当前分支中尚未出现在远端视图中的提交数量。
    pub(crate) current_branch_unpushed_commits: usize,
    /// Default mainline branch resolved by the backend.
    /// 后端解析出的默认主线分支。
    pub(crate) default_branch: Option<String>,
    /// Branch selected as the comparison base.
    /// 当前作为对比基准的分支。
    pub(crate) selected_base_branch: Option<String>,
    /// Branch selected as the comparison target.
    /// 当前作为对比目标的分支。
    pub(crate) selected_compare_branch: Option<String>,
    /// Commit selected for commit review mode.
    /// 提交审核模式当前选择的提交。
    pub(crate) selected_commit_hash: Option<String>,
    /// Branches available for branch comparison.
    /// 可用于分支对比的分支列表。
    pub(crate) branches: Vec<WebuiGitBranchSummary>,
    /// Unpushed commits available for commit review mode.
    /// 可用于提交审核模式的未推送提交列表。
    pub(crate) recent_commits: Vec<WebuiGitCommitSummary>,
}

/// WebUI workspace Git diff file summary returned with the snapshot.
/// 随快照返回给 WebUI 的工作区 Git diff 文件摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffFileSummary {
    /// Stable summary key used by the WebUI.
    /// WebUI 使用的稳定摘要键。
    pub(crate) key: String,
    /// Previous file path.
    /// 旧文件路径。
    pub(crate) old_path: String,
    /// Current file path.
    /// 当前文件路径。
    pub(crate) new_path: String,
    /// File-level change status.
    /// 文件级变更状态。
    pub(crate) status: String,
    /// Number of added lines in this file.
    /// 该文件的新增行数。
    pub(crate) additions: usize,
    /// Number of deleted lines in this file.
    /// 该文件的删除行数。
    pub(crate) deletions: usize,
    /// Whether this file is binary-only in the diff.
    /// 该文件是否在 diff 中仅表现为二进制变更。
    pub(crate) is_binary: bool,
    /// Whether this file is a gitlink or submodule entry.
    /// 该文件是否为 gitlink 或子模块条目。
    pub(crate) is_gitlink: bool,
    /// File mode parsed from diff metadata when available.
    /// 从 diff 元数据解析出的可选文件模式。
    pub(crate) file_mode: Option<String>,
    /// Number of changed lines counted as additions plus deletions.
    /// 以新增行加删除行计算的变更行数。
    pub(crate) changed_lines: usize,
    /// Whether this file body is intentionally omitted from WebUI rendering.
    /// 该文件正文是否被有意从 WebUI 渲染载荷中省略。
    pub(crate) too_large_to_display: bool,
    /// Estimated UTF-8 byte size of changed lines in this file.
    /// 该文件变更行的估算 UTF-8 字节体积。
    pub(crate) changed_bytes: usize,
    /// Largest estimated UTF-8 byte size among changed lines.
    /// 变更行中的最大估算 UTF-8 字节体积。
    pub(crate) max_changed_line_bytes: usize,
    /// Estimated visual row count including hunk headers.
    /// 包含 hunk 头部的估算可视行数。
    pub(crate) row_count: usize,
    /// Number of parsed hunk headers in this file diff.
    /// 该文件 diff 中解析到的 hunk 头部数量。
    pub(crate) hunk_count: usize,
    /// Complete old-side source file line count when available.
    /// 可用时旧侧源文件的完整行数。
    pub(crate) old_file_line_count: Option<usize>,
    /// Complete new-side source file line count when available.
    /// 可用时新侧源文件的完整行数。
    pub(crate) new_file_line_count: Option<usize>,
}

/// WebUI workspace Git diff file selector for lazy detail loading.
/// WebUI 工作区 Git diff 懒加载详情使用的文件选择器。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffFileDetailRequest {
    /// Stable file key from the summary snapshot.
    /// 摘要快照中的稳定文件键。
    pub(crate) key: String,
    /// Previous file path.
    /// 旧文件路径。
    pub(crate) old_path: String,
    /// Current file path.
    /// 当前文件路径。
    pub(crate) new_path: String,
    /// Optional unified-diff context line count requested for hunk expansion.
    /// hunk 展开请求使用的可选 unified-diff 上下文行数。
    pub(crate) context_lines: Option<usize>,
    /// Optional zero-based hunk index requested for targeted expansion.
    /// 定向展开请求使用的可选零基 hunk 索引。
    pub(crate) hunk_index: Option<usize>,
    /// Optional hunk expansion direction requested by the WebUI.
    /// WebUI 请求的可选 hunk 展开方向。
    pub(crate) hunk_expansion_direction: Option<String>,
}

/// WebUI workspace Git diff detail returned for one file.
/// 返回给 WebUI 的单文件工作区 Git diff 详情。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffFileDetail {
    /// Repository root used for the Git command.
    /// Git 命令使用的仓库根目录。
    pub(crate) repository_root: String,
    /// File summary computed from the single-file diff.
    /// 从单文件 diff 计算出的文件摘要。
    pub(crate) file: Option<WebuiWorkspaceDiffFileSummary>,
    /// Unified diff for the requested file only.
    /// 仅包含请求文件的 unified diff。
    pub(crate) unified_diff: String,
    /// Compatibility flag for legacy clients; normal file diff payloads are returned complete.
    /// 面向旧客户端的兼容标记；正常单文件 diff 载荷会完整返回。
    pub(crate) truncated: bool,
    /// Optional user-facing note for empty or partial details.
    /// 空详情或局部详情使用的可选用户提示。
    pub(crate) notice: Option<String>,
    /// Unified-diff context line count used to produce this detail payload.
    /// 生成当前详情载荷时使用的 unified-diff 上下文行数。
    pub(crate) context_lines: usize,
    /// Optional zero-based hunk index echoed for targeted expansion.
    /// 定向展开响应回传的可选零基 hunk 索引。
    pub(crate) hunk_index: Option<usize>,
    /// Optional normalized hunk expansion direction echoed to the WebUI.
    /// 回传给 WebUI 的可选规范化 hunk 展开方向。
    pub(crate) hunk_expansion_direction: Option<String>,
}

/// WebUI workspace Git diff search options.
/// WebUI 工作区 Git diff 搜索选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffSearchRequest {
    /// Search query matched against full backend diff text.
    /// 与后端完整 diff 文本匹配的搜索词。
    pub(crate) query: String,
    /// Maximum number of matching files returned to the WebUI.
    /// 返回给 WebUI 的最大命中文件数量。
    #[serde(default = "default_webui_diff_search_limit")]
    pub(crate) limit: usize,
}

/// WebUI workspace Git diff search response.
/// WebUI 工作区 Git diff 搜索响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffSearchResponse {
    /// Repository root used for the Git command.
    /// Git 命令使用的仓库根目录。
    pub(crate) repository_root: String,
    /// Original query after backend trimming.
    /// 后端裁剪空白后的原始搜索词。
    pub(crate) query: String,
    /// File-level matches ordered by diff appearance.
    /// 按 diff 出现顺序排列的文件级命中。
    pub(crate) matches: Vec<WebuiWorkspaceDiffSearchFileMatch>,
    /// Whether additional matching files were omitted by the result limit.
    /// 是否因结果数量限制省略了更多命中文件。
    pub(crate) truncated: bool,
    /// Number of files scanned in the complete backend diff.
    /// 后端完整 diff 中已扫描的文件数量。
    pub(crate) scanned_files: usize,
    /// Number of diff lines scanned in the complete backend diff.
    /// 后端完整 diff 中已扫描的 diff 行数量。
    pub(crate) scanned_lines: usize,
    /// Optional user-facing note for empty, partial, or skipped searches.
    /// 空搜索、局部搜索或跳过搜索使用的可选用户提示。
    pub(crate) notice: Option<String>,
}

/// WebUI workspace Git diff file-level search match.
/// WebUI 工作区 Git diff 文件级搜索命中。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffSearchFileMatch {
    /// Stable summary key used by the WebUI.
    /// WebUI 使用的稳定摘要键。
    pub(crate) key: String,
    /// Previous file path.
    /// 旧文件路径。
    pub(crate) old_path: String,
    /// Current file path.
    /// 当前文件路径。
    pub(crate) new_path: String,
    /// File-level change status.
    /// 文件级变更状态。
    pub(crate) status: String,
    /// Number of matched diff lines in this file.
    /// 该文件中命中的 diff 行数量。
    pub(crate) match_count: usize,
    /// First compact matching line preview.
    /// 首条紧凑命中行预览。
    pub(crate) first_preview: String,
    /// Bounded compact matching line previews for summary-only display.
    /// 用于摘要态展示的受限紧凑命中行预览。
    pub(crate) previews: Vec<String>,
    /// Hunk index for the first matching diff row when available.
    /// 可用时首条命中 diff 行所在的 hunk 索引。
    pub(crate) first_hunk_index: Option<usize>,
    /// Old-file line number for the first matching diff row when available.
    /// 可用时首条命中 diff 行对应的旧文件行号。
    pub(crate) first_old_line: Option<usize>,
    /// New-file line number for the first matching diff row when available.
    /// 可用时首条命中 diff 行对应的新文件行号。
    pub(crate) first_new_line: Option<usize>,
    /// Diff row kind for the first matching diff row when available.
    /// 可用时首条命中 diff 行对应的行类型。
    pub(crate) first_line_kind: Option<String>,
}

/// WebUI workspace Git file action request.
/// WebUI 工作区 Git 文件级操作请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceGitFileActionRequest {
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
    /// File action to execute against the Git index or worktree.
    /// 要对 Git 索引或工作区执行的文件操作。
    pub(crate) action: WebuiWorkspaceGitFileAction,
    /// Files targeted by file-scoped actions.
    /// 文件级操作的目标文件列表。
    #[serde(default)]
    pub(crate) files: Vec<WebuiWorkspaceGitFileActionFile>,
}

/// WebUI workspace Git file action kind.
/// WebUI 工作区 Git 文件级操作类型。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WebuiWorkspaceGitFileAction {
    /// Stage every worktree change, including untracked files.
    /// 暂存全部工作区变更，包括未跟踪文件。
    StageAll,
    /// Discard every unstaged worktree change and remove untracked files.
    /// 丢弃全部未暂存工作区变更并移除未跟踪文件。
    RestoreAll,
    /// Stage the provided file paths.
    /// 暂存传入的文件路径。
    StageFile,
    /// Discard the provided unstaged file paths.
    /// 丢弃传入文件路径的未暂存变更。
    RestoreFile,
    /// Unstage every indexed change.
    /// 取消暂存全部索引变更。
    UnstageAll,
    /// Unstage the provided file paths.
    /// 取消暂存传入的文件路径。
    UnstageFile,
}

/// WebUI workspace Git file action target.
/// WebUI 工作区 Git 文件级操作目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceGitFileActionFile {
    /// Previous file path from the current diff summary.
    /// 当前 diff 摘要中的旧文件路径。
    pub(crate) old_path: String,
    /// Current file path from the current diff summary.
    /// 当前 diff 摘要中的新文件路径。
    pub(crate) new_path: String,
    /// File-level status from the current diff summary.
    /// 当前 diff 摘要中的文件级状态。
    pub(crate) status: String,
}

/// WebUI workspace Git file action result.
/// WebUI 工作区 Git 文件级操作结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceGitFileActionResult {
    /// Repository root used by the Git operation.
    /// Git 操作使用的仓库根目录。
    pub(crate) repository_root: String,
    /// Executed action name.
    /// 已执行的操作名称。
    pub(crate) action: String,
    /// Number of file pathspecs passed to Git.
    /// 传递给 Git 的文件路径规格数量。
    pub(crate) affected_path_count: usize,
    /// User-facing operation summary.
    /// 面向用户的操作摘要。
    pub(crate) summary: String,
}

/// WebUI Git diff request options.
/// WebUI Git diff 请求选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiWorkspaceDiffRequest {
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
    /// Selected review mode.
    /// 当前选择的审核模式。
    pub(crate) mode: WebuiWorkspaceDiffMode,
    /// Optional base branch for branch comparisons.
    /// 分支对比使用的可选基准分支。
    pub(crate) base_branch: Option<String>,
    /// Optional compare branch for branch comparisons.
    /// 分支对比使用的可选目标分支。
    pub(crate) compare_branch: Option<String>,
    /// Optional commit hash for commit review mode.
    /// 提交审核模式使用的可选提交哈希。
    pub(crate) commit_hash: Option<String>,
}

/// Builds the default WebUI diff request.
/// 构建默认 WebUI diff 请求。
pub(crate) fn default_webui_workspace_diff_request() -> WebuiWorkspaceDiffRequest {
    WebuiWorkspaceDiffRequest {
        mode: WebuiWorkspaceDiffMode::Unstaged,
        repository_key: None,
        base_branch: None,
        compare_branch: None,
        commit_hash: None,
    }
}

/// One discovered Git repository exposed to the WebUI review surface.
/// 暴露给 WebUI 审查界面的单个已发现 Git 仓库。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitRepositorySummary {
    /// Stable project-scoped repository key, including explicitly attached roots.
    /// 稳定的项目范围仓库键，涵盖显式登记的附属根目录。
    pub(crate) repository_key: String,
    /// Canonical repository root used by backend Git commands.
    /// 后端 Git 命令使用的规范化仓库根目录。
    pub(crate) repository_root: String,
    /// Repository directory name shown in compact selectors.
    /// 紧凑选择器中显示的仓库目录名称。
    pub(crate) name: String,
    /// Display location: relative to the project for its own repositories, absolute for attached ones.
    /// 显示位置：项目自身的仓库为项目相对路径，附属仓库为绝对路径。
    pub(crate) relative_path: String,
    /// Whether this repository is the deterministic primary repository.
    /// 该仓库是否为确定性的主要仓库。
    pub(crate) is_primary: bool,
    /// Whether this repository is enabled for live review monitoring.
    /// 该仓库是否启用实时审查监听。
    pub(crate) monitoring_enabled: bool,
}

/// Catalog of Git repositories discovered below one WebUI project.
/// 单个 WebUI 项目下发现的 Git 仓库目录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitRepositoryCatalog {
    /// Canonical project root used for discovery.
    /// 发现流程使用的规范化项目根目录。
    pub(crate) project_root: String,
    /// Deterministically ordered discovered repositories.
    /// 按确定性顺序排列的已发现仓库。
    pub(crate) repositories: Vec<WebuiGitRepositorySummary>,
}

/// Request that toggles live monitoring for one discovered repository.
/// 切换单个已发现仓库实时监听状态的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitRepositoryMonitoringRequest {
    /// Stable project-relative repository key to update.
    /// 需要更新的稳定项目相对仓库键。
    pub(crate) repository_key: String,
    /// Desired monitoring state.
    /// 需要设置的监听状态。
    pub(crate) enabled: bool,
}

/// WebUI Git diff source mode.
/// WebUI Git diff 来源模式。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WebuiWorkspaceDiffMode {
    /// Unstaged working tree changes.
    /// 未暂存的工作区变更。
    Unstaged,
    /// Staged index changes.
    /// 已暂存的索引变更。
    Staged,
    /// Single commit patch.
    /// 单次提交 patch。
    Commit,
    /// Branch-to-branch comparison.
    /// 分支到分支对比。
    Branch,
}

/// WebUI Git branch summary.
/// WebUI Git 分支摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitBranchSummary {
    /// Displayable branch name.
    /// 可展示的分支名称。
    pub(crate) name: String,
    /// Whether this is the current branch.
    /// 是否为当前分支。
    pub(crate) current: bool,
    /// Whether this branch is remote-tracking.
    /// 是否为远端跟踪分支。
    pub(crate) remote: bool,
}

/// WebUI Git commit summary.
/// WebUI Git 提交摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitCommitSummary {
    /// Full commit hash.
    /// 完整提交哈希。
    pub(crate) hash: String,
    /// Short commit hash.
    /// 短提交哈希。
    pub(crate) short_hash: String,
    /// Commit subject.
    /// 提交标题。
    pub(crate) subject: String,
    /// Committer time in Unix milliseconds; clients name it in their own language.
    /// Unix 毫秒提交者时间；客户端以各自语言显示。
    pub(crate) committed_at_ms: i64,
}

/// Compact identity for one Git commit used by action completion notices.
/// Git 操作完成提示使用的紧凑提交身份信息。
#[derive(Debug, Clone)]
pub(crate) struct WebuiGitCommitIdentity {
    /// Short commit hash shown to users.
    /// 展示给用户的短提交哈希。
    pub(crate) hash: String,
    /// First-line commit message subject.
    /// 提交信息首行标题。
    pub(crate) subject: String,
}

/// Resolved Git commit-summary input passed into the internal summary prompt.
/// 传入内部总结提示词的 Git 提交总结输入。
pub(crate) struct GitCommitSummaryInput {
    /// Full or summarized repository change text.
    /// 完整或摘要化的仓库变更文本。
    pub(crate) content: String,
    /// Whether the full diff was replaced by summarized change metadata.
    /// 完整 diff 是否被摘要化变更元数据替换。
    pub(crate) truncated: bool,
}

/// Returns the default WebUI diff search result limit.
/// 返回默认 WebUI diff 搜索结果上限。
fn default_webui_diff_search_limit() -> usize {
    WEBUI_DIFF_SEARCH_MAX_RESULTS
}

/// Commits returned by one WebUI Git history page when the request omits a limit.
/// WebUI Git 历史请求未指定上限时单页返回的提交数量。
pub(crate) const WEBUI_GIT_HISTORY_PAGE_SIZE: usize = 30;

/// Upper bound for one WebUI Git history page so a caller cannot request an unbounded log.
/// WebUI Git 历史单页上限，避免调用方请求无界日志。
pub(crate) const WEBUI_GIT_HISTORY_MAX_PAGE_SIZE: usize = 200;

/// Request that targets one discovered repository without extra options.
/// 仅定位单个已发现仓库、不带额外选项的请求。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitRepositoryTargetRequest {
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
}

/// Local branches of one repository, ordered by most recent commit first.
/// 单个仓库的本地分支，按最近提交时间倒序排列。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitLocalBranchList {
    /// Current branch name, or `None` when HEAD is detached.
    /// 当前分支名称；游离 HEAD 时为 `None`。
    pub(crate) current_branch: Option<String>,
    /// Local branch short names.
    /// 本地分支短名称。
    pub(crate) branches: Vec<String>,
}

/// Request that switches the worktree to a local branch, optionally creating it from HEAD.
/// 将工作区切换到本地分支的请求，可选择基于 HEAD 新建。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitSwitchBranchRequest {
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
    /// Target local branch name.
    /// 目标本地分支名称。
    pub(crate) branch_name: String,
    /// Whether the branch is created from the current HEAD before switching.
    /// 是否在切换前基于当前 HEAD 新建该分支。
    #[serde(default)]
    pub(crate) create: bool,
}

/// Result of one WebUI branch switch.
/// 单次 WebUI 分支切换的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitSwitchBranchResult {
    /// Repository root used by the Git operation.
    /// Git 操作使用的仓库根目录。
    pub(crate) repository_root: String,
    /// Branch checked out after the operation.
    /// 操作完成后检出的分支。
    pub(crate) branch_name: String,
    /// Whether the branch was newly created.
    /// 该分支是否为新建。
    pub(crate) created: bool,
}

/// Request for one page of repository history.
/// 请求仓库历史的单页数据。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitHistoryRequest {
    /// Stable project-relative repository key; `.` selects the project root repository.
    /// 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
    #[serde(default)]
    pub(crate) repository_key: Option<String>,
    /// Number of commits already loaded by the caller.
    /// 调用方已加载的提交数量。
    #[serde(default)]
    pub(crate) skip: usize,
    /// Page size; omitted values use `WEBUI_GIT_HISTORY_PAGE_SIZE`.
    /// 单页数量；省略时使用 `WEBUI_GIT_HISTORY_PAGE_SIZE`。
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

/// One page of repository history in graph order.
/// 按图谱顺序排列的单页仓库历史。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitHistoryPage {
    /// Commits in this page, children before parents.
    /// 本页提交，子提交排在父提交之前。
    pub(crate) commits: Vec<WebuiGitHistoryCommit>,
    /// Whether older commits remain after this page.
    /// 本页之后是否仍有更早的提交。
    pub(crate) has_more: bool,
}

/// One commit row rendered by the WebUI Git graph.
/// WebUI Git 图谱渲染的单行提交。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitHistoryCommit {
    /// Full commit hash.
    /// 完整提交哈希。
    pub(crate) hash: String,
    /// Abbreviated commit hash.
    /// 缩写提交哈希。
    pub(crate) short_hash: String,
    /// Full parent hashes in Git order.
    /// 按 Git 顺序排列的完整父提交哈希。
    pub(crate) parents: Vec<String>,
    /// Author display name.
    /// 作者显示名称。
    pub(crate) author_name: String,
    /// Author email address.
    /// 作者邮箱地址。
    pub(crate) author_email: String,
    /// Author date in strict ISO 8601 format.
    /// 严格 ISO 8601 格式的作者日期。
    pub(crate) author_date: String,
    /// First-line commit subject.
    /// 提交信息首行标题。
    pub(crate) subject: String,
    /// Commit message body without the subject line.
    /// 不含标题行的提交信息正文。
    pub(crate) body: String,
    /// Refs decorating this commit.
    /// 指向该提交的引用。
    pub(crate) refs: Vec<WebuiGitHistoryRef>,
}

/// One ref decoration shown on a Git graph row.
/// Git 图谱行上展示的单个引用标记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WebuiGitHistoryRef {
    /// Short ref name.
    /// 引用短名称。
    pub(crate) name: String,
    /// Ref category.
    /// 引用类别。
    pub(crate) kind: WebuiGitHistoryRefKind,
    /// Whether HEAD points at this ref, or this is a detached HEAD marker.
    /// HEAD 是否指向该引用，或该项为游离 HEAD 标记。
    pub(crate) head: bool,
}

/// Category of one Git graph ref decoration.
/// Git 图谱引用标记的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WebuiGitHistoryRefKind {
    /// Detached HEAD.
    /// 游离 HEAD。
    Head,
    /// Local branch under `refs/heads`.
    /// `refs/heads` 下的本地分支。
    Branch,
    /// Remote-tracking branch under `refs/remotes`.
    /// `refs/remotes` 下的远端跟踪分支。
    Remote,
    /// Tag under `refs/tags`.
    /// `refs/tags` 下的标签。
    Tag,
}
