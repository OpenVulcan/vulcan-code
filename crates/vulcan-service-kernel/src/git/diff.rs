//! Git diff loading and repository metadata helpers for WebUI review flows.
//! WebUI 审查流程使用的 Git diff 加载与仓库元数据辅助函数。

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use super::command::{format_webui_git_error, run_webui_git_command};
use super::model::{
    WebuiGitBranchSummary, WebuiGitCommitIdentity, WebuiGitCommitSummary, WebuiWorkspaceDiffMode,
};
use crate::{WEBUI_DIFF_DEFAULT_CONTEXT_LINES, workspace_root};

/// Resolved workspace path and optional Git repository root for WebUI Git flows.
/// WebUI Git 流程使用的已解析工作区路径与可选 Git 仓库根目录。
pub(crate) struct WebuiWorkspaceGitResolution {
    /// Canonical workspace path resolved from user-provided text.
    /// 从用户提供文本解析出的规范化工作区路径。
    pub(crate) workspace_path: PathBuf,
    /// Canonical Git repository root when the workspace belongs to a Git repository.
    /// 工作区属于 Git 仓库时的规范化 Git 仓库根目录。
    pub(crate) repository_root: Option<PathBuf>,
}

/// Loaded unified diff text and notices for one selected WebUI diff source.
/// 单个已选 WebUI diff 来源加载出的 unified diff 文本与提示信息。
/// Fields: unified_diff stores the joined visible diff sections, notices stores loader notices.
/// 字段：unified_diff 保存拼接后的可见 diff 区块，notices 保存加载阶段提示。
pub(crate) struct WebuiLoadedUnifiedDiff {
    /// Complete unified diff text assembled from all non-empty sections.
    /// 由全部非空区块组装出的完整 unified diff 文本。
    pub(crate) unified_diff: String,
    /// User-facing notices produced while loading the selected diff source.
    /// 加载所选 diff 来源过程中产生的用户可见提示。
    pub(crate) notices: Vec<String>,
}

/// Scope used when loading a selected WebUI diff source.
/// 加载已选 WebUI diff 来源时使用的范围。
#[derive(Clone, Copy)]
enum WebuiSelectedDiffScope<'a> {
    /// Load the complete selected diff without path filtering.
    /// 加载不按路径过滤的完整已选 diff。
    Workspace,
    /// Load the selected diff for one Git pathspec with explicit context lines.
    /// 按单个 Git 路径规格与显式上下文行加载已选 diff。
    Path {
        /// Git pathspec used to constrain the loaded diff.
        /// 用于限定已加载 diff 的 Git 路径规格。
        pathspec: &'a str,
        /// Number of context lines requested from Git for file-scoped diffs.
        /// 单文件 diff 请求 Git 返回的上下文行数。
        context_lines: usize,
    },
}

/// Reason why an untracked file is represented as a binary diff marker.
/// 未跟踪文件以二进制 diff 标记展示的原因。
#[derive(Clone, Copy)]
enum WebuiUntrackedBinaryReason {
    /// The file bytes contain a null byte.
    /// 文件字节包含空字节。
    ContainsNull,
    /// The file bytes are not valid UTF-8 text.
    /// 文件字节不是合法 UTF-8 文本。
    NonUtf8,
}

/// Synthesized diff section for one untracked file.
/// 单个未跟踪文件合成出的 diff 区块。
struct WebuiUntrackedDiffSection {
    /// Unified diff text synthesized for the untracked file.
    /// 为未跟踪文件合成出的 unified diff 文本。
    unified_diff: String,
    /// Binary rendering reason when the file could not be shown as UTF-8 text.
    /// 文件无法以 UTF-8 文本展示时的二进制展示原因。
    binary_reason: Option<WebuiUntrackedBinaryReason>,
}

/// Resolves a WebUI workspace path into a concrete Git repository root.
/// 将 WebUI 工作区路径解析为明确的 Git 仓库根目录。
pub(crate) fn resolve_webui_repository_root_from_workspace_text(
    workspace_path: &str,
) -> Result<PathBuf, String> {
    let resolution = resolve_webui_workspace_git_resolution(workspace_path)?;
    resolution
        .repository_root
        .ok_or_else(|| String::from("当前工作区不是 Git 仓库，无法执行 Git 提交操作。"))
}

/// Resolves WebUI workspace text into a canonical path and optional Git repository root.
/// 将 WebUI 工作区文本解析为规范化路径与可选 Git 仓库根目录。
pub(crate) fn resolve_webui_workspace_git_resolution(
    workspace_path: &str,
) -> Result<WebuiWorkspaceGitResolution, String> {
    let workspace_path = resolve_webui_workspace_path_from_text(workspace_path)?;
    let repository_root = resolve_webui_git_repository_root(&workspace_path)?;
    Ok(WebuiWorkspaceGitResolution {
        workspace_path,
        repository_root,
    })
}

/// Resolves WebUI workspace text into a canonical filesystem path.
/// 将 WebUI 工作区文本解析为规范化文件系统路径。
pub(crate) fn resolve_webui_workspace_path_from_text(
    workspace_path: &str,
) -> Result<PathBuf, String> {
    let requested_path = PathBuf::from(workspace_path.trim());
    let workspace_path = if requested_path.as_os_str().is_empty() {
        workspace_root()
    } else {
        requested_path
    };
    fs::canonicalize(&workspace_path)
        .map_err(|error| format!("failed to resolve workspace path `{workspace_path:?}`: {error}"))
}

/// Builds the Git unified-context argument for one normalized context line count.
/// 为规范化后的上下文行数构建 Git unified-context 参数。
fn webui_diff_context_arg(context_lines: usize) -> String {
    format!(
        "--unified={}",
        context_lines.max(WEBUI_DIFF_DEFAULT_CONTEXT_LINES)
    )
}

/// Resolves the Git repository root for one workspace path.
/// 解析单个工作区路径对应的 Git 仓库根目录。
pub(crate) fn resolve_webui_git_repository_root(
    workspace_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let output = run_webui_git_command(workspace_path, &["rev-parse", "--show-toplevel"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Ok(None);
    }
    fs::canonicalize(&root)
        .map(Some)
        .map_err(|error| format!("failed to resolve git repository root `{root}`: {error}"))
}

/// Decodes successful Git stdout or formats the failed Git command output.
/// 解码成功 Git stdout，或格式化失败的 Git 命令输出。
/// Parameters: label is the stable error label, and output is the completed Git process output.
/// 参数：label 是稳定错误标签，output 是已完成的 Git 进程输出。
/// Returns: raw stdout text on success, or the formatted Git error on failure.
/// 返回：成功时返回原始 stdout 文本，失败时返回格式化后的 Git 错误。
fn webui_git_stdout_or_error(label: &str, output: std::process::Output) -> Result<String, String> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error(label, &output))
}

/// Loads unstaged tracked changes as one unified diff.
/// 以 unified diff 形式加载未暂存的已跟踪变更。
pub(crate) fn load_webui_unstaged_git_diff(repository_root: &Path) -> Result<String, String> {
    let output = run_webui_git_command(
        repository_root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            "--",
        ],
    )?;
    webui_git_stdout_or_error("git diff", output)
}

/// Loads unstaged tracked changes for one file pathspec.
/// 为单个文件路径规格加载未暂存的已跟踪变更。
pub(crate) fn load_webui_unstaged_git_diff_for_path(
    repository_root: &Path,
    pathspec: &str,
    context_lines: usize,
) -> Result<String, String> {
    let context_arg = webui_diff_context_arg(context_lines);
    let output = run_webui_git_command(
        repository_root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            context_arg.as_str(),
            "--",
            pathspec,
        ],
    )?;
    webui_git_stdout_or_error("git diff file", output)
}

/// Loads staged changes as one unified diff.
/// 以 unified diff 形式加载已暂存变更。
pub(crate) fn load_webui_staged_git_diff(repository_root: &Path) -> Result<String, String> {
    let output = run_webui_git_command(
        repository_root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            "--cached",
            "--",
        ],
    )?;
    webui_git_stdout_or_error("git diff --cached", output)
}

/// Loads staged changes for one file pathspec.
/// 为单个文件路径规格加载已暂存变更。
pub(crate) fn load_webui_staged_git_diff_for_path(
    repository_root: &Path,
    pathspec: &str,
    context_lines: usize,
) -> Result<String, String> {
    let context_arg = webui_diff_context_arg(context_lines);
    let output = run_webui_git_command(
        repository_root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            "--cached",
            context_arg.as_str(),
            "--",
            pathspec,
        ],
    )?;
    webui_git_stdout_or_error("git diff --cached file", output)
}

/// Loads one selected commit patch.
/// 加载单个选中提交的 patch。
pub(crate) fn load_webui_commit_git_diff(
    repository_root: &Path,
    commit_hash: &str,
) -> Result<String, String> {
    ensure_webui_git_ref_text(commit_hash)?;
    let output = run_webui_git_command(
        repository_root,
        &[
            "show",
            "--format=",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            "--patch",
            commit_hash,
            "--",
        ],
    )?;
    webui_git_stdout_or_error("git show", output)
}

/// Loads one selected commit patch for one file pathspec.
/// 为单个文件路径规格加载选中提交的 patch。
pub(crate) fn load_webui_commit_git_diff_for_path(
    repository_root: &Path,
    commit_hash: &str,
    pathspec: &str,
    context_lines: usize,
) -> Result<String, String> {
    ensure_webui_git_ref_text(commit_hash)?;
    let context_arg = webui_diff_context_arg(context_lines);
    let output = run_webui_git_command(
        repository_root,
        &[
            "show",
            "--format=",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            "--patch",
            context_arg.as_str(),
            commit_hash,
            "--",
            pathspec,
        ],
    )?;
    webui_git_stdout_or_error("git show file", output)
}

/// Loads a branch-to-branch merge-base diff.
/// 加载两个分支之间基于合并基点的 diff。
pub(crate) fn load_webui_branch_git_diff(
    repository_root: &Path,
    base_branch: &str,
    compare_branch: &str,
) -> Result<String, String> {
    ensure_webui_git_ref_text(base_branch)?;
    ensure_webui_git_ref_text(compare_branch)?;
    let range = format!("{base_branch}...{compare_branch}");
    let output = run_webui_git_command(
        repository_root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            &range,
            "--",
        ],
    )?;
    webui_git_stdout_or_error("git diff branch range", output)
}

/// Loads a branch-to-branch merge-base diff for one file pathspec.
/// 为单个文件路径规格加载两个分支之间基于合并基点的 diff。
pub(crate) fn load_webui_branch_git_diff_for_path(
    repository_root: &Path,
    base_branch: &str,
    compare_branch: &str,
    pathspec: &str,
    context_lines: usize,
) -> Result<String, String> {
    ensure_webui_git_ref_text(base_branch)?;
    ensure_webui_git_ref_text(compare_branch)?;
    let range = format!("{base_branch}...{compare_branch}");
    let context_arg = webui_diff_context_arg(context_lines);
    let output = run_webui_git_command(
        repository_root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--submodule=short",
            context_arg.as_str(),
            &range,
            "--",
            pathspec,
        ],
    )?;
    webui_git_stdout_or_error("git diff branch range file", output)
}

/// Loads the complete unified diff for one selected WebUI diff mode.
/// 按单个已选 WebUI diff 模式加载完整 unified diff。
/// Parameters: repository_root is the Git root, selection fields identify the requested source, and missing_commit_notice preserves caller wording.
/// 参数：repository_root 是 Git 根目录，选择字段标识请求来源，missing_commit_notice 保留调用方文案。
/// Returns: loaded diff text plus notices without applying UI statistics, search indexing, or large-file filtering.
/// 返回：加载出的 diff 文本和提示，不执行 UI 统计、搜索索引或大文件过滤。
pub(crate) fn load_webui_selected_workspace_unified_diff(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
    missing_commit_notice: &str,
) -> Result<WebuiLoadedUnifiedDiff, String> {
    load_webui_selected_workspace_unified_diff_for_scope(
        repository_root,
        mode,
        selected_base_branch,
        selected_compare_branch,
        selected_commit_hash,
        WebuiSelectedDiffScope::Workspace,
        missing_commit_notice,
    )
}

/// Loads the file-scoped unified diff for one selected WebUI diff mode.
/// 按单个已选 WebUI diff 模式加载单文件范围 unified diff。
/// Parameters: repository_root is the Git root, selection fields identify the requested source, and pathspec/context_lines constrain the file diff.
/// 参数：repository_root 是 Git 根目录，选择字段标识请求来源，pathspec/context_lines 限定单文件 diff。
/// Returns: loaded file-scoped diff text plus notices with the caller-specific missing commit message.
/// 返回：加载出的单文件 diff 文本和提示，并保留调用方指定的缺失提交文案。
pub(crate) fn load_webui_selected_workspace_unified_diff_for_path(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
    pathspec: &str,
    context_lines: usize,
    missing_commit_notice: &str,
) -> Result<WebuiLoadedUnifiedDiff, String> {
    load_webui_selected_workspace_unified_diff_for_scope(
        repository_root,
        mode,
        selected_base_branch,
        selected_compare_branch,
        selected_commit_hash,
        WebuiSelectedDiffScope::Path {
            pathspec,
            context_lines,
        },
        missing_commit_notice,
    )
}

/// Loads the selected WebUI diff source for the requested workspace or file scope.
/// 按请求的工作区或单文件范围加载已选 WebUI diff 来源。
/// Parameters: repository_root is the Git root, selection fields identify the diff source, scope selects workspace or path loading, and missing_commit_notice preserves caller wording.
/// 参数：repository_root 是 Git 根目录，选择字段标识 diff 来源，scope 选择工作区或路径加载，missing_commit_notice 保留调用方文案。
/// Returns: loaded diff text plus user-facing notices.
/// 返回：加载出的 diff 文本和用户可见提示。
fn load_webui_selected_workspace_unified_diff_for_scope(
    repository_root: &Path,
    mode: WebuiWorkspaceDiffMode,
    selected_base_branch: Option<&str>,
    selected_compare_branch: Option<&str>,
    selected_commit_hash: Option<&str>,
    scope: WebuiSelectedDiffScope<'_>,
    missing_commit_notice: &str,
) -> Result<WebuiLoadedUnifiedDiff, String> {
    let mut notices = Vec::new();
    let mut sections = Vec::new();
    match mode {
        WebuiWorkspaceDiffMode::Unstaged => {
            push_webui_non_empty_diff_section(
                &mut sections,
                load_webui_unstaged_git_diff_for_scope(repository_root, scope)?,
            );
            push_webui_non_empty_diff_section(
                &mut sections,
                load_webui_untracked_git_diff_for_scope(repository_root, scope, &mut notices)?,
            );
        }
        WebuiWorkspaceDiffMode::Staged => {
            push_webui_non_empty_diff_section(
                &mut sections,
                load_webui_staged_git_diff_for_scope(repository_root, scope)?,
            );
        }
        WebuiWorkspaceDiffMode::Commit => {
            if let Some(commit_hash) = selected_commit_hash {
                push_webui_non_empty_diff_section(
                    &mut sections,
                    load_webui_commit_git_diff_for_scope(repository_root, commit_hash, scope)?,
                );
            } else {
                notices.push(String::from(missing_commit_notice));
            }
        }
        WebuiWorkspaceDiffMode::Branch => match (selected_base_branch, selected_compare_branch) {
            (Some(base_branch), Some(compare_branch)) => {
                push_webui_non_empty_diff_section(
                    &mut sections,
                    load_webui_branch_git_diff_for_scope(
                        repository_root,
                        compare_branch,
                        base_branch,
                        scope,
                    )?,
                );
            }
            _ => notices.push(String::from("当前仓库无法解析用于分支对比的分支。")),
        },
    }
    Ok(WebuiLoadedUnifiedDiff {
        unified_diff: sections.join("\n"),
        notices,
    })
}

/// Loads unstaged tracked changes for one selected diff scope.
/// 按单个已选 diff 范围加载未暂存的已跟踪变更。
/// Parameters: repository_root is the Git root, and scope selects workspace or path-constrained loading.
/// 参数：repository_root 是 Git 根目录，scope 选择工作区或路径限定加载。
/// Returns: raw Git diff text for unstaged tracked changes.
/// 返回：未暂存已跟踪变更的原始 Git diff 文本。
fn load_webui_unstaged_git_diff_for_scope(
    repository_root: &Path,
    scope: WebuiSelectedDiffScope<'_>,
) -> Result<String, String> {
    match scope {
        WebuiSelectedDiffScope::Workspace => load_webui_unstaged_git_diff(repository_root),
        WebuiSelectedDiffScope::Path {
            pathspec,
            context_lines,
        } => load_webui_unstaged_git_diff_for_path(repository_root, pathspec, context_lines),
    }
}

/// Loads untracked changes for one selected diff scope.
/// 按单个已选 diff 范围加载未跟踪变更。
/// Parameters: repository_root is the Git root, scope selects workspace or path-constrained loading, and notices receives loader notices.
/// 参数：repository_root 是 Git 根目录，scope 选择工作区或路径限定加载，notices 接收加载提示。
/// Returns: synthesized unified diff text for untracked files.
/// 返回：未跟踪文件合成出的 unified diff 文本。
fn load_webui_untracked_git_diff_for_scope(
    repository_root: &Path,
    scope: WebuiSelectedDiffScope<'_>,
    notices: &mut Vec<String>,
) -> Result<String, String> {
    match scope {
        WebuiSelectedDiffScope::Workspace => {
            load_webui_untracked_git_diff(repository_root, notices)
        }
        WebuiSelectedDiffScope::Path { pathspec, .. } => {
            load_webui_untracked_git_diff_for_path(repository_root, pathspec, notices)
        }
    }
}

/// Loads staged changes for one selected diff scope.
/// 按单个已选 diff 范围加载已暂存变更。
/// Parameters: repository_root is the Git root, and scope selects workspace or path-constrained loading.
/// 参数：repository_root 是 Git 根目录，scope 选择工作区或路径限定加载。
/// Returns: raw Git diff text for staged changes.
/// 返回：已暂存变更的原始 Git diff 文本。
fn load_webui_staged_git_diff_for_scope(
    repository_root: &Path,
    scope: WebuiSelectedDiffScope<'_>,
) -> Result<String, String> {
    match scope {
        WebuiSelectedDiffScope::Workspace => load_webui_staged_git_diff(repository_root),
        WebuiSelectedDiffScope::Path {
            pathspec,
            context_lines,
        } => load_webui_staged_git_diff_for_path(repository_root, pathspec, context_lines),
    }
}

/// Loads one selected commit diff for one selected diff scope.
/// 按单个已选 diff 范围加载单个选中提交 diff。
/// Parameters: repository_root is the Git root, commit_hash identifies the selected commit, and scope selects workspace or path-constrained loading.
/// 参数：repository_root 是 Git 根目录，commit_hash 标识选中提交，scope 选择工作区或路径限定加载。
/// Returns: raw Git show diff text for the selected commit.
/// 返回：选中提交的原始 Git show diff 文本。
fn load_webui_commit_git_diff_for_scope(
    repository_root: &Path,
    commit_hash: &str,
    scope: WebuiSelectedDiffScope<'_>,
) -> Result<String, String> {
    match scope {
        WebuiSelectedDiffScope::Workspace => {
            load_webui_commit_git_diff(repository_root, commit_hash)
        }
        WebuiSelectedDiffScope::Path {
            pathspec,
            context_lines,
        } => load_webui_commit_git_diff_for_path(
            repository_root,
            commit_hash,
            pathspec,
            context_lines,
        ),
    }
}

/// Loads one branch comparison diff for one selected diff scope.
/// 按单个已选 diff 范围加载单个分支对比 diff。
/// Parameters: repository_root is the Git root, base_branch and compare_branch form the Git range, and scope selects workspace or path-constrained loading.
/// 参数：repository_root 是 Git 根目录，base_branch 与 compare_branch 组成 Git range，scope 选择工作区或路径限定加载。
/// Returns: raw Git diff text for the selected branch comparison.
/// 返回：选中分支对比的原始 Git diff 文本。
fn load_webui_branch_git_diff_for_scope(
    repository_root: &Path,
    base_branch: &str,
    compare_branch: &str,
    scope: WebuiSelectedDiffScope<'_>,
) -> Result<String, String> {
    match scope {
        WebuiSelectedDiffScope::Workspace => {
            load_webui_branch_git_diff(repository_root, base_branch, compare_branch)
        }
        WebuiSelectedDiffScope::Path {
            pathspec,
            context_lines,
        } => load_webui_branch_git_diff_for_path(
            repository_root,
            base_branch,
            compare_branch,
            pathspec,
            context_lines,
        ),
    }
}

/// Appends one diff section only when Git returned visible unified diff text.
/// 仅当 Git 返回可见 unified diff 文本时追加单个 diff 区块。
/// Parameters: sections receives the retained diff blocks, and section is the raw Git output to inspect.
/// 参数：sections 接收保留的 diff 区块，section 是待检查的原始 Git 输出。
/// Returns: nothing; the sections vector is mutated only when the raw output is non-empty after trimming.
/// 返回：无返回值；仅当原始输出裁剪后非空时修改 sections 向量。
fn push_webui_non_empty_diff_section(sections: &mut Vec<String>, section: String) {
    if !section.trim().is_empty() {
        sections.push(section);
    }
}

/// Loads the current branch name when the repository is not detached.
/// 在仓库不是游离 HEAD 时加载当前分支名称。
pub(crate) fn load_webui_current_git_branch(
    repository_root: &Path,
) -> Result<Option<String>, String> {
    let output = run_webui_git_command(repository_root, &["branch", "--show-current"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!branch.is_empty()).then_some(branch))
}

/// Loads the default mainline branch used as the branch comparison target.
/// 加载作为分支对比目标的默认主线分支。
pub(crate) fn load_webui_default_git_branch(
    repository_root: &Path,
) -> Result<Option<String>, String> {
    let origin_head = run_webui_git_command(
        repository_root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )?;
    if origin_head.status.success() {
        let branch = String::from_utf8_lossy(&origin_head.stdout)
            .trim()
            .to_string();
        if !branch.is_empty() {
            return Ok(Some(branch));
        }
    }
    for candidate in ["origin/main", "origin/master", "main", "master"] {
        if webui_git_ref_exists(repository_root, candidate)? {
            return Ok(Some(candidate.to_string()));
        }
    }
    Ok(None)
}

/// Loads selectable local and remote branch names.
/// 加载可选择的本地与远端分支名称。
pub(crate) fn load_webui_git_branches(
    repository_root: &Path,
    current_branch: Option<&str>,
) -> Result<Vec<WebuiGitBranchSummary>, String> {
    let output = run_webui_git_command(
        repository_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let mut seen = BTreeSet::new();
    let mut branches = Vec::new();
    for raw_line in String::from_utf8_lossy(&output.stdout).lines() {
        let branch = raw_line.trim();
        if branch.is_empty() || branch.ends_with("/HEAD") || !seen.insert(branch.to_string()) {
            continue;
        }
        branches.push(WebuiGitBranchSummary {
            name: branch.to_string(),
            current: current_branch == Some(branch),
            remote: branch.contains('/'),
        });
    }
    Ok(branches)
}

/// Loads the latest commits used by commit review mode.
/// 加载提交审核模式使用的最近提交。
pub(crate) fn load_webui_recent_commits(
    repository_root: &Path,
) -> Result<Vec<WebuiGitCommitSummary>, String> {
    let output = run_webui_git_command(
        repository_root,
        &[
            "log",
            "-n",
            "20",
            "--pretty=format:%H%x1f%h%x1f%s%x1f%cr%x1e",
        ],
    )?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let mut commits = Vec::new();
    for raw_record in String::from_utf8_lossy(&output.stdout).split('\x1e') {
        let fields = raw_record
            .trim_matches('\n')
            .split('\x1f')
            .collect::<Vec<_>>();
        if fields.len() < 4 || fields[0].trim().is_empty() {
            continue;
        }
        commits.push(WebuiGitCommitSummary {
            hash: fields[0].trim().to_string(),
            short_hash: fields[1].trim().to_string(),
            subject: fields[2].trim().to_string(),
            relative_time: fields[3].trim().to_string(),
        });
    }
    Ok(commits)
}

/// Loads the current HEAD commit identity for Git action completion notices.
/// 加载当前 HEAD 提交身份信息，用于 Git 操作完成提示。
pub(crate) fn load_webui_head_commit_identity(
    repository_root: &Path,
) -> Result<Option<WebuiGitCommitIdentity>, String> {
    let output =
        run_webui_git_command(repository_root, &["log", "-1", "--pretty=format:%h%x1f%s"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let fields = text.trim().split('\x1f').collect::<Vec<_>>();
    if fields.len() < 2 || fields[0].trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(WebuiGitCommitIdentity {
        hash: fields[0].trim().to_string(),
        subject: fields[1].trim().to_string(),
    }))
}

/// Checks whether one Git ref exists.
/// 检查单个 Git 引用是否存在。
pub(crate) fn webui_git_ref_exists(repository_root: &Path, git_ref: &str) -> Result<bool, String> {
    ensure_webui_git_ref_text(git_ref)?;
    let output = run_webui_git_command(
        repository_root,
        &["rev-parse", "--verify", "--quiet", git_ref],
    )?;
    Ok(output.status.success())
}

/// Rejects ref text that could be interpreted as an option or shell-like payload.
/// 拒绝可能被解释为选项或类 shell 载荷的引用文本。
pub(crate) fn ensure_webui_git_ref_text(git_ref: &str) -> Result<(), String> {
    let value = git_ref.trim();
    if value.is_empty()
        || value.starts_with('-')
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(String::from("invalid Git reference for WebUI diff request"));
    }
    Ok(())
}

/// Loads untracked files as synthesized new-file hunks.
/// 将未跟踪文件加载为合成的新文件 hunk。
pub(crate) fn load_webui_untracked_git_diff(
    repository_root: &Path,
    notices: &mut Vec<String>,
) -> Result<String, String> {
    let sections = load_webui_untracked_git_diff_sections(repository_root, None)?;
    let skipped_binary = sections
        .iter()
        .filter(|section| section.binary_reason.is_some())
        .count();
    if skipped_binary > 0 {
        notices.push(format!(
            "已将 {skipped_binary} 个未跟踪二进制文件按二进制变更展示。"
        ));
    }

    Ok(sections
        .into_iter()
        .map(|section| section.unified_diff)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Loads one untracked file as a synthesized new-file hunk.
/// 将单个未跟踪文件加载为合成的新文件 hunk。
pub(crate) fn load_webui_untracked_git_diff_for_path(
    repository_root: &Path,
    pathspec: &str,
    notices: &mut Vec<String>,
) -> Result<String, String> {
    let Some(section) = load_webui_untracked_git_diff_sections(repository_root, Some(pathspec))?
        .into_iter()
        .next()
    else {
        return Ok(String::new());
    };
    match section.binary_reason {
        Some(WebuiUntrackedBinaryReason::ContainsNull) => {
            notices.push(String::from("该未跟踪文件是二进制文件，按二进制变更展示。"));
        }
        Some(WebuiUntrackedBinaryReason::NonUtf8) => {
            notices.push(String::from(
                "该未跟踪文件不是 UTF-8 文本，按二进制变更展示。",
            ));
        }
        None => {}
    }
    Ok(section.unified_diff)
}

/// Loads synthesized diff sections for untracked files, optionally constrained by one pathspec.
/// 加载未跟踪文件的合成 diff 区块，并可选按单个路径规格限定。
/// Parameters: repository_root is the Git root, and pathspec limits `git ls-files` when present.
/// 参数：repository_root 是 Git 根目录，pathspec 存在时限定 `git ls-files`。
/// Returns: synthesized untracked diff sections in Git output order.
/// 返回：按 Git 输出顺序排列的未跟踪文件合成 diff 区块。
fn load_webui_untracked_git_diff_sections(
    repository_root: &Path,
    pathspec: Option<&str>,
) -> Result<Vec<WebuiUntrackedDiffSection>, String> {
    let output = if let Some(pathspec) = pathspec {
        run_webui_git_command(
            repository_root,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                pathspec,
            ],
        )?
    } else {
        run_webui_git_command(
            repository_root,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )?
    };
    if !output.status.success() {
        let label = if pathspec.is_some() {
            "git ls-files file"
        } else {
            "git ls-files"
        };
        return Err(format_webui_git_error(label, &output));
    }

    let mut sections = Vec::new();
    for raw_path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        let relative_path = String::from_utf8_lossy(raw_path).replace('\\', "/");
        if let Some(section) =
            load_webui_untracked_file_diff_section(repository_root, &relative_path)?
        {
            sections.push(section);
        }
    }
    Ok(sections)
}

/// Loads one synthesized diff section for an untracked file path from Git output.
/// 根据 Git 输出中的未跟踪文件路径加载单个合成 diff 区块。
/// Parameters: repository_root is the Git root, and relative_path is the normalized Git path.
/// 参数：repository_root 是 Git 根目录，relative_path 是规范化后的 Git 路径。
/// Returns: a synthesized section when the path is a safe file inside the repository.
/// 返回：当路径是仓库内安全文件时返回合成区块。
fn load_webui_untracked_file_diff_section(
    repository_root: &Path,
    relative_path: &str,
) -> Result<Option<WebuiUntrackedDiffSection>, String> {
    let file_path = repository_root.join(relative_path);
    if !is_path_within_root(repository_root, &file_path) || !file_path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&file_path)
        .map_err(|error| format!("failed to read untracked file `{relative_path}`: {error}"))?;
    if bytes.contains(&0) {
        return Ok(Some(WebuiUntrackedDiffSection {
            unified_diff: synthesize_webui_binary_new_file_diff(relative_path),
            binary_reason: Some(WebuiUntrackedBinaryReason::ContainsNull),
        }));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(Some(WebuiUntrackedDiffSection {
            unified_diff: synthesize_webui_binary_new_file_diff(relative_path),
            binary_reason: Some(WebuiUntrackedBinaryReason::NonUtf8),
        }));
    };
    Ok(Some(WebuiUntrackedDiffSection {
        unified_diff: synthesize_webui_text_new_file_diff(relative_path, &text),
        binary_reason: None,
    }))
}

/// Returns whether the current branch already has an upstream branch.
/// 返回当前分支是否已经配置上游分支。
pub(crate) fn webui_current_branch_has_upstream(repository_root: &Path) -> bool {
    run_webui_git_command(
        repository_root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .map(|output| output.status.success())
    .unwrap_or(false)
}

/// Counts commits on the current branch that have not reached the relevant remote view.
/// 统计当前分支中尚未进入相关远端视图的提交数量。
pub(crate) fn load_webui_current_branch_unpushed_commit_count(
    repository_root: &Path,
) -> Result<usize, String> {
    if load_webui_current_git_branch(repository_root)?.is_none() {
        return Ok(0);
    }
    if webui_current_branch_has_upstream(repository_root) {
        return load_webui_git_revision_count(repository_root, &["@{u}..HEAD"]);
    }
    load_webui_git_revision_count(repository_root, &["HEAD", "--not", "--remotes"])
}

/// Loads one Git revision count from `git rev-list --count`.
/// 通过 `git rev-list --count` 读取一个 Git 修订范围数量。
fn load_webui_git_revision_count(
    repository_root: &Path,
    revision_args: &[&str],
) -> Result<usize, String> {
    let mut args = vec!["rev-list", "--count"];
    args.extend_from_slice(revision_args);
    let output = run_webui_git_command(repository_root, &args)?;
    if !output.status.success() {
        return Err(format_webui_git_error("git rev-list --count", &output));
    }
    let count_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    count_text
        .parse::<usize>()
        .map_err(|error| format!("failed to parse git revision count `{count_text}`: {error}"))
}

/// Returns whether one candidate path stays inside the repository root.
/// 返回候选路径是否仍位于仓库根目录内。
pub(crate) fn is_path_within_root(repository_root: &Path, candidate: &Path) -> bool {
    let Ok(repository_root) = fs::canonicalize(repository_root) else {
        return false;
    };
    let Ok(candidate) = fs::canonicalize(candidate) else {
        return false;
    };
    candidate.starts_with(repository_root)
}

/// Synthesizes a unified diff section for one untracked text file.
/// 为单个未跟踪文本文件合成 unified diff 片段。
fn synthesize_webui_text_new_file_diff(relative_path: &str, text: &str) -> String {
    let old_path = git_diff_path(&format!("a/{relative_path}"));
    let new_path = git_diff_path(&format!("b/{relative_path}"));
    let line_count = text.lines().count();
    let mut section = format!(
        "diff --git {old_path} {new_path}\nnew file mode 100644\n--- /dev/null\n+++ {new_path}\n@@ -0,0 +1,{line_count} @@\n"
    );
    for line in text.lines() {
        section.push('+');
        section.push_str(line);
        section.push('\n');
    }
    if !text.ends_with('\n') {
        section.push_str("\\ No newline at end of file\n");
    }
    section
}

/// Synthesizes a binary diff marker for one untracked binary file.
/// 为单个未跟踪二进制文件合成二进制 diff 标记。
fn synthesize_webui_binary_new_file_diff(relative_path: &str) -> String {
    let old_path = git_diff_path(&format!("a/{relative_path}"));
    let new_path = git_diff_path(&format!("b/{relative_path}"));
    format!(
        "diff --git {old_path} {new_path}\nnew file mode 100644\nBinary files /dev/null and {new_path} differ\n"
    )
}

/// Quotes one Git diff path when whitespace or escapes require it.
/// 当路径包含空白或转义字符时为 Git diff 路径加引号。
fn git_diff_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized
        .chars()
        .any(|character| character.is_whitespace() || character == '"' || character == '\\')
    {
        format!(
            "\"{}\"",
            normalized.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates one unique temporary directory for Git diff helper tests.
    /// 为 Git diff 辅助函数测试创建唯一临时目录。
    fn create_webui_git_diff_temp_dir(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vulcan-webui-git-diff-{label}-{}-{now}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary git diff directory should be created");
        path
    }

    /// Verifies selected diff loading preserves caller-specific missing commit notices.
    /// 验证已选 diff 加载会保留调用方指定的缺失提交提示。
    #[test]
    fn load_webui_selected_workspace_unified_diff_uses_caller_missing_commit_notice() {
        let path = create_webui_git_diff_temp_dir("missing-commit");

        let loaded_diff = load_webui_selected_workspace_unified_diff(
            &path,
            WebuiWorkspaceDiffMode::Commit,
            None,
            None,
            None,
            "调用方缺失提交提示。",
        )
        .expect("missing commit diff should load without git io");

        assert!(loaded_diff.unified_diff.is_empty());
        assert_eq!(loaded_diff.notices, vec!["调用方缺失提交提示。"]);
        let _ = fs::remove_dir_all(path);
    }

    /// Verifies file-scoped selected diff loading preserves caller-specific missing commit notices.
    /// 验证单文件范围已选 diff 加载会保留调用方指定的缺失提交提示。
    #[test]
    fn load_webui_selected_workspace_unified_diff_for_path_uses_caller_missing_commit_notice() {
        let path = create_webui_git_diff_temp_dir("missing-commit-path");

        let loaded_diff = load_webui_selected_workspace_unified_diff_for_path(
            &path,
            WebuiWorkspaceDiffMode::Commit,
            None,
            None,
            None,
            "src/lib.rs",
            WEBUI_DIFF_DEFAULT_CONTEXT_LINES,
            "调用方单文件缺失提交提示。",
        )
        .expect("missing commit file diff should load without git io");

        assert!(loaded_diff.unified_diff.is_empty());
        assert_eq!(loaded_diff.notices, vec!["调用方单文件缺失提交提示。"]);
        let _ = fs::remove_dir_all(path);
    }

    /// Verifies untracked file section loading preserves binary reasons for caller notices.
    /// 验证未跟踪文件区块加载会保留用于调用方提示的二进制原因。
    #[test]
    fn load_webui_untracked_file_diff_section_marks_binary_reasons() {
        let path = create_webui_git_diff_temp_dir("untracked-binary-reasons");
        fs::write(path.join("null.bin"), b"a\0b")
            .expect("null-byte binary fixture should be written");
        fs::write(path.join("non_utf8.bin"), [0xff, 0xfe])
            .expect("non-utf8 binary fixture should be written");

        let null_section = load_webui_untracked_file_diff_section(&path, "null.bin")
            .expect("null-byte section should load")
            .expect("null-byte section should exist");
        let non_utf8_section = load_webui_untracked_file_diff_section(&path, "non_utf8.bin")
            .expect("non-utf8 section should load")
            .expect("non-utf8 section should exist");

        assert!(matches!(
            null_section.binary_reason,
            Some(WebuiUntrackedBinaryReason::ContainsNull)
        ));
        assert!(matches!(
            non_utf8_section.binary_reason,
            Some(WebuiUntrackedBinaryReason::NonUtf8)
        ));
        assert!(null_section.unified_diff.contains("Binary files"));
        assert!(non_utf8_section.unified_diff.contains("Binary files"));
        let _ = fs::remove_dir_all(path);
    }

    /// Verifies workspace text resolution trims and canonicalizes explicit paths.
    /// 验证工作区文本解析会裁剪并规范化显式路径。
    #[test]
    fn resolve_webui_workspace_path_from_text_canonicalizes_explicit_path() {
        let path = create_webui_git_diff_temp_dir("canonical");
        let padded_path = format!("  {}  ", path.display());

        let resolved =
            resolve_webui_workspace_path_from_text(&padded_path).expect("path should resolve");

        assert_eq!(
            resolved,
            fs::canonicalize(&path).expect("path should canonicalize")
        );
        let _ = fs::remove_dir_all(path);
    }

    /// Verifies Git resolution keeps the workspace path when no repository exists.
    /// 验证不存在 Git 仓库时解析结果仍保留工作区路径。
    #[test]
    fn resolve_webui_workspace_git_resolution_keeps_non_repository_workspace_path() {
        let path = create_webui_git_diff_temp_dir("nonrepo");

        let resolution = resolve_webui_workspace_git_resolution(&path.to_string_lossy())
            .expect("workspace git resolution should load");

        assert_eq!(
            resolution.workspace_path,
            fs::canonicalize(&path).expect("path should canonicalize")
        );
        assert_eq!(resolution.repository_root, None);
        let _ = fs::remove_dir_all(path);
    }
}
