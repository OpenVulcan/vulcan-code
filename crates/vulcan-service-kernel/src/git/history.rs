//! WebUI Git branch switching and paged history service logic.
//! WebUI Git 分支切换与分页历史服务逻辑。

use std::path::Path;

use vulcan_host_tools::output_decode::decode_shell_output_bytes;

use super::command::{
    format_webui_git_error, run_webui_git_command, run_webui_git_mutating_command,
};
use super::diff::{ensure_webui_git_ref_text, load_webui_current_git_branch, webui_git_ref_exists};
use super::model::{
    WEBUI_GIT_HISTORY_MAX_PAGE_SIZE, WEBUI_GIT_HISTORY_PAGE_SIZE, WebuiGitHistoryCommit,
    WebuiGitHistoryPage, WebuiGitHistoryRef, WebuiGitHistoryRefKind, WebuiGitHistoryRequest,
    WebuiGitLocalBranchList, WebuiGitSwitchBranchRequest, WebuiGitSwitchBranchResult,
};
use super::search::clear_webui_diff_search_caches;
use crate::normalized_workspace_path_text_from_path;

/// Git pretty format for one history record; fields use US and records use RS separators.
/// 单条历史记录的 Git pretty 格式；字段使用 US 分隔，记录使用 RS 分隔。
const WEBUI_GIT_HISTORY_FORMAT: &str =
    "--pretty=format:%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%D%x1f%s%x1f%b%x1e";

/// Loads local branches ordered by their latest commit, newest first.
/// 按最新提交时间倒序加载本地分支。
pub(crate) fn load_webui_git_local_branches(
    repository_root: &Path,
) -> Result<WebuiGitLocalBranchList, String> {
    let output = run_webui_git_command(
        repository_root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )?;
    if !output.status.success() {
        return Err(format_webui_git_error("git for-each-ref", &output));
    }
    let branches = decode_shell_output_bytes(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    Ok(WebuiGitLocalBranchList {
        current_branch: load_webui_current_git_branch(repository_root)?,
        branches,
    })
}

/// Switches the worktree to a local branch, creating it from HEAD when requested.
/// 将工作区切换到本地分支，按请求基于 HEAD 新建。
///
/// # Parameters
/// 参数
/// `repository_root` is the resolved repository root; the caller holds its write lane.
/// `repository_root` 是已解析的仓库根目录；调用方持有其写入通道。
/// `request` names the target branch and whether it must be created first.
/// `request` 指定目标分支以及是否需要先新建。
///
/// # Returns
/// 返回值
/// The checked-out branch, or a validation or Git diagnostic.
/// 返回已检出的分支，或校验与 Git 诊断信息。
pub(crate) fn switch_webui_git_branch(
    repository_root: &Path,
    request: &WebuiGitSwitchBranchRequest,
) -> Result<WebuiGitSwitchBranchResult, String> {
    let branch_name = request.branch_name.trim().to_string();
    ensure_webui_git_ref_text(&branch_name)?;
    let format_output = run_webui_git_command(
        repository_root,
        &["check-ref-format", "--branch", branch_name.as_str()],
    )?;
    if !format_output.status.success() {
        return Err(format!(
            "Branch name `{branch_name}` is not a valid Git branch name."
        ));
    }
    let exists = webui_git_ref_exists(repository_root, &format!("refs/heads/{branch_name}"))?;
    let args: &[&str] = if request.create {
        if exists {
            return Err(format!("Local branch `{branch_name}` already exists."));
        }
        &["switch", "-c", branch_name.as_str()]
    } else {
        if !exists {
            return Err(format!("Local branch `{branch_name}` does not exist."));
        }
        &["switch", branch_name.as_str()]
    };
    let output = run_webui_git_mutating_command(repository_root, args)?;
    if !output.status.success() {
        return Err(format_webui_git_error("git switch", &output));
    }
    clear_webui_diff_search_caches();
    Ok(WebuiGitSwitchBranchResult {
        repository_root: normalized_workspace_path_text_from_path(repository_root),
        branch_name,
        created: request.create,
    })
}

/// Loads one page of history across local branches, remote branches, tags, and HEAD.
/// 加载覆盖本地分支、远端分支、标签与 HEAD 的单页历史。
///
/// # Parameters
/// 参数
/// `repository_root` is the resolved repository root.
/// `repository_root` 是已解析的仓库根目录。
/// `request` carries the number of commits to skip and the optional page size.
/// `request` 携带需要跳过的提交数量与可选的单页数量。
///
/// # Returns
/// 返回值
/// Commits in date order with children before parents, plus whether more remain.
/// 返回子提交先于父提交的按日期排序提交，以及是否仍有更多提交。
pub(crate) fn load_webui_git_history_page(
    repository_root: &Path,
    request: &WebuiGitHistoryRequest,
) -> Result<WebuiGitHistoryPage, String> {
    let limit = request
        .limit
        .unwrap_or(WEBUI_GIT_HISTORY_PAGE_SIZE)
        .clamp(1, WEBUI_GIT_HISTORY_MAX_PAGE_SIZE);
    if load_webui_head_is_unborn(repository_root)? {
        return Ok(WebuiGitHistoryPage {
            commits: Vec::new(),
            has_more: false,
        });
    }
    let skip_arg = format!("--skip={}", request.skip);
    // One extra record reveals whether another page exists without a second Git call.
    // 多取一条记录即可判断是否还有下一页，无需额外调用 Git。
    let max_count_arg = format!("--max-count={}", limit + 1);
    let output = run_webui_git_command(
        repository_root,
        &[
            "log",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
            "--date-order",
            "--decorate=full",
            "--no-show-signature",
            skip_arg.as_str(),
            max_count_arg.as_str(),
            WEBUI_GIT_HISTORY_FORMAT,
            "--",
        ],
    )?;
    if !output.status.success() {
        return Err(format_webui_git_error("git log", &output));
    }
    let mut commits = parse_webui_git_history_records(&decode_shell_output_bytes(&output.stdout));
    let has_more = commits.len() > limit;
    commits.truncate(limit);
    Ok(WebuiGitHistoryPage { commits, has_more })
}

/// Reports whether HEAD has no commit yet, as in a freshly initialized repository.
/// 判断 HEAD 是否尚无提交，例如刚初始化的仓库。
fn load_webui_head_is_unborn(repository_root: &Path) -> Result<bool, String> {
    let output = run_webui_git_command(
        repository_root,
        &["rev-parse", "--verify", "--quiet", "HEAD"],
    )?;
    Ok(!output.status.success())
}

/// Parses RS-separated history records produced by `WEBUI_GIT_HISTORY_FORMAT`.
/// 解析由 `WEBUI_GIT_HISTORY_FORMAT` 生成、以 RS 分隔的历史记录。
fn parse_webui_git_history_records(text: &str) -> Vec<WebuiGitHistoryCommit> {
    text.split('\x1e')
        .filter_map(|record| {
            let fields = record
                .trim_start_matches('\n')
                .splitn(9, '\x1f')
                .collect::<Vec<_>>();
            if fields.len() < 9 || fields[0].trim().is_empty() {
                return None;
            }
            Some(WebuiGitHistoryCommit {
                hash: fields[0].trim().to_string(),
                short_hash: fields[1].trim().to_string(),
                parents: fields[2].split_whitespace().map(str::to_string).collect(),
                author_name: fields[3].trim().to_string(),
                author_email: fields[4].trim().to_string(),
                author_date: fields[5].trim().to_string(),
                refs: parse_webui_git_history_refs(fields[6]),
                subject: fields[7].trim().to_string(),
                body: fields[8].trim().to_string(),
            })
        })
        .collect()
}

/// Parses one full-name `%D` decoration list into graph ref chips.
/// 将单个完整名称的 `%D` 装饰列表解析为图谱引用标签。
fn parse_webui_git_history_refs(decoration: &str) -> Vec<WebuiGitHistoryRef> {
    let mut refs = Vec::new();
    for item in decoration
        .split(", ")
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if item == "HEAD" {
            refs.push(WebuiGitHistoryRef {
                name: String::from("HEAD"),
                kind: WebuiGitHistoryRefKind::Head,
                head: true,
            });
            continue;
        }
        let (head, full_name) = match item.strip_prefix("HEAD -> ") {
            Some(target) => (true, target),
            None => (false, item),
        };
        let full_name = full_name.strip_prefix("tag: ").unwrap_or(full_name);
        let (kind, name) = if let Some(name) = full_name.strip_prefix("refs/heads/") {
            (WebuiGitHistoryRefKind::Branch, name)
        } else if let Some(name) = full_name.strip_prefix("refs/remotes/") {
            // Remote HEAD aliases duplicate the remote default branch chip.
            // 远端 HEAD 别名与远端默认分支标签重复。
            if name.ends_with("/HEAD") {
                continue;
            }
            (WebuiGitHistoryRefKind::Remote, name)
        } else if let Some(name) = full_name.strip_prefix("refs/tags/") {
            (WebuiGitHistoryRefKind::Tag, name)
        } else {
            continue;
        };
        refs.push(WebuiGitHistoryRef {
            name: name.to_string(),
            kind,
            head,
        });
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_refs_classify_head_branches_remotes_and_tags() {
        let refs = parse_webui_git_history_refs(
            "HEAD -> refs/heads/main, refs/remotes/origin/main, refs/remotes/origin/HEAD, tag: refs/tags/v1.0, refs/stash",
        );
        assert_eq!(
            refs,
            vec![
                WebuiGitHistoryRef {
                    name: String::from("main"),
                    kind: WebuiGitHistoryRefKind::Branch,
                    head: true,
                },
                WebuiGitHistoryRef {
                    name: String::from("origin/main"),
                    kind: WebuiGitHistoryRefKind::Remote,
                    head: false,
                },
                WebuiGitHistoryRef {
                    name: String::from("v1.0"),
                    kind: WebuiGitHistoryRefKind::Tag,
                    head: false,
                },
            ]
        );
    }

    #[test]
    fn history_refs_mark_detached_head() {
        let refs = parse_webui_git_history_refs("HEAD, refs/heads/feature/x");
        assert_eq!(refs[0].kind, WebuiGitHistoryRefKind::Head);
        assert!(refs[0].head);
        assert_eq!(refs[1].name, "feature/x");
        assert!(!refs[1].head);
    }

    #[test]
    fn history_records_keep_multiline_bodies_and_merge_parents() {
        let text = "aaa\x1fa\x1fp1 p2\x1fAda\x1fada@example.com\x1f2026-01-02T03:04:05+08:00\x1f\x1fMerge\x1fline one\nline two\n\x1e\nbbb\x1fb\x1f\x1fBob\x1fbob@example.com\x1f2026-01-01T00:00:00+00:00\x1f\x1fRoot\x1f\x1e";
        let commits = parse_webui_git_history_records(text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents, vec!["p1", "p2"]);
        assert_eq!(commits[0].body, "line one\nline two");
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[1].subject, "Root");
    }

    #[test]
    fn history_page_reports_more_commits_and_honors_skip() {
        let repository = tempfile::tempdir().expect("temporary repository should be created");
        let root = repository.path();
        let git = |args: &[&str]| {
            let output = run_webui_git_mutating_command(root, args).expect("git should run");
            assert!(
                output.status.success(),
                "{}",
                format_webui_git_error("git", &output)
            );
        };
        git(&["init", "--quiet", "--initial-branch=main"]);
        git(&["config", "user.name", "Vulcan Test"]);
        git(&["config", "user.email", "vulcan@example.com"]);
        for index in 0..3 {
            git(&[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &format!("commit {index}"),
            ]);
        }
        let first = load_webui_git_history_page(
            root,
            &WebuiGitHistoryRequest {
                repository_key: None,
                skip: 0,
                limit: Some(2),
            },
        )
        .expect("first history page should load");
        assert_eq!(first.commits.len(), 2);
        assert!(first.has_more);
        assert_eq!(first.commits[0].subject, "commit 2");
        assert!(
            first.commits[0]
                .refs
                .iter()
                .any(|item| item.head && item.name == "main")
        );
        let second = load_webui_git_history_page(
            root,
            &WebuiGitHistoryRequest {
                repository_key: None,
                skip: 2,
                limit: Some(2),
            },
        )
        .expect("second history page should load");
        assert_eq!(second.commits.len(), 1);
        assert!(!second.has_more);
        assert_eq!(second.commits[0].subject, "commit 0");
    }

    #[test]
    fn switch_branch_creates_then_returns_to_existing_branch() {
        let repository = tempfile::tempdir().expect("temporary repository should be created");
        let root = repository.path();
        let git = |args: &[&str]| {
            let output = run_webui_git_mutating_command(root, args).expect("git should run");
            assert!(
                output.status.success(),
                "{}",
                format_webui_git_error("git", &output)
            );
        };
        git(&["init", "--quiet", "--initial-branch=main"]);
        git(&["config", "user.name", "Vulcan Test"]);
        git(&["config", "user.email", "vulcan@example.com"]);
        git(&["commit", "--quiet", "--allow-empty", "-m", "root"]);
        let switch = |branch_name: &str, create: bool| {
            switch_webui_git_branch(
                root,
                &WebuiGitSwitchBranchRequest {
                    repository_key: None,
                    branch_name: branch_name.to_string(),
                    create,
                },
            )
        };
        switch("vulcan/topic", true).expect("new branch should be created");
        let branches = load_webui_git_local_branches(root).expect("branches should load");
        assert_eq!(branches.current_branch.as_deref(), Some("vulcan/topic"));
        assert!(branches.branches.iter().any(|name| name == "main"));
        assert!(
            switch("vulcan/topic", true).is_err(),
            "duplicate branch must be rejected"
        );
        assert!(
            switch("missing", false).is_err(),
            "unknown branch must be rejected"
        );
        assert!(
            switch("bad..name", true).is_err(),
            "invalid ref format must be rejected"
        );
        switch("main", false).expect("existing branch should be checked out");
        let branches = load_webui_git_local_branches(root).expect("branches should load");
        assert_eq!(branches.current_branch.as_deref(), Some("main"));
    }
}
