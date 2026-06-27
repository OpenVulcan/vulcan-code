//! Git diff loading and repository metadata helpers for WebUI review flows.
//! WebUI 审查流程使用的 Git diff 加载与仓库元数据辅助函数。

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use super::command::{format_webui_git_error, run_webui_git_command};
use super::model::{WebuiGitBranchSummary, WebuiGitCommitIdentity, WebuiGitCommitSummary};
use crate::{WEBUI_DIFF_DEFAULT_CONTEXT_LINES, workspace_root};

/// Resolves a WebUI workspace path into a concrete Git repository root.
/// 将 WebUI 工作区路径解析为明确的 Git 仓库根目录。
pub(crate) fn resolve_webui_repository_root_from_workspace_text(
    workspace_path: &str,
) -> Result<PathBuf, String> {
    let requested_path = PathBuf::from(workspace_path.trim());
    let workspace_path = if requested_path.as_os_str().is_empty() {
        workspace_root()
    } else {
        requested_path
    };
    let workspace_path = fs::canonicalize(&workspace_path).map_err(|error| {
        format!("failed to resolve workspace path `{workspace_path:?}`: {error}")
    })?;
    resolve_webui_git_repository_root(&workspace_path)?
        .ok_or_else(|| String::from("当前工作区不是 Git 仓库，无法执行 Git 提交操作。"))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git diff", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git diff file", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git diff --cached", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git diff --cached file", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git show", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git show file", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error("git diff branch range", &output))
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
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(format_webui_git_error(
        "git diff branch range file",
        &output,
    ))
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

/// Loads untracked text files as synthesized new-file hunks.
/// 将未跟踪文本文件加载为合成的新文件 hunk。
pub(crate) fn load_webui_untracked_git_diff(
    repository_root: &Path,
    notices: &mut Vec<String>,
) -> Result<String, String> {
    let output = run_webui_git_command(
        repository_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    if !output.status.success() {
        return Err(format_webui_git_error("git ls-files", &output));
    }

    let mut sections = Vec::new();
    let mut skipped_binary = 0usize;
    for raw_path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        let relative_path = String::from_utf8_lossy(raw_path).replace('\\', "/");
        let file_path = repository_root.join(&relative_path);
        if !is_path_within_root(repository_root, &file_path) || !file_path.is_file() {
            continue;
        }
        let bytes = fs::read(&file_path)
            .map_err(|error| format!("failed to read untracked file `{relative_path}`: {error}"))?;
        if bytes.contains(&0) {
            skipped_binary += 1;
            sections.push(synthesize_webui_binary_new_file_diff(&relative_path));
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            skipped_binary += 1;
            sections.push(synthesize_webui_binary_new_file_diff(&relative_path));
            continue;
        };
        sections.push(synthesize_webui_text_new_file_diff(&relative_path, &text));
    }

    if skipped_binary > 0 {
        notices.push(format!(
            "已将 {skipped_binary} 个未跟踪二进制文件按二进制变更展示。"
        ));
    }

    Ok(sections.join("\n"))
}

/// Loads one untracked text file as a synthesized new-file hunk.
/// 将单个未跟踪文本文件加载为合成的新文件 hunk。
pub(crate) fn load_webui_untracked_git_diff_for_path(
    repository_root: &Path,
    pathspec: &str,
    notices: &mut Vec<String>,
) -> Result<String, String> {
    let output = run_webui_git_command(
        repository_root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            pathspec,
        ],
    )?;
    if !output.status.success() {
        return Err(format_webui_git_error("git ls-files file", &output));
    }
    let Some(raw_path) = output
        .stdout
        .split(|byte| *byte == 0)
        .find(|part| !part.is_empty())
    else {
        return Ok(String::new());
    };
    let relative_path = String::from_utf8_lossy(raw_path).replace('\\', "/");
    let file_path = repository_root.join(&relative_path);
    if !is_path_within_root(repository_root, &file_path) || !file_path.is_file() {
        return Ok(String::new());
    }
    let bytes = fs::read(&file_path)
        .map_err(|error| format!("failed to read untracked file `{relative_path}`: {error}"))?;
    if bytes.contains(&0) {
        notices.push(String::from("该未跟踪文件是二进制文件，按二进制变更展示。"));
        return Ok(synthesize_webui_binary_new_file_diff(&relative_path));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        notices.push(String::from(
            "该未跟踪文件不是 UTF-8 文本，按二进制变更展示。",
        ));
        return Ok(synthesize_webui_binary_new_file_diff(&relative_path));
    };
    Ok(synthesize_webui_text_new_file_diff(&relative_path, &text))
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
