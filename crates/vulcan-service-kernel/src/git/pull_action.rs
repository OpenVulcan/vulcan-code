//! Git pull action service logic.
//! Git 拉取操作服务逻辑。

use std::path::Path;

use vulcan_service_api::{WebuiLocalizedMessage, WebuiServiceRpcError};
use vulcan_workbench_model::SessionId;

use super::command::{run_webui_git_command, run_webui_git_mutating_command};
use super::commit_action::{
    ensure_git_commit_action_command_succeeded, validate_git_commit_action_session_exists,
};
use super::diff::{
    load_webui_current_git_branch, load_webui_head_commit_identity,
    webui_current_branch_has_upstream,
};
use super::model::{GitPullActionRequest, GitPullActionResponse};
use crate::SharedServiceBridgeState;

/// Pulls the current branch from its upstream, fast-forward only.
/// 从上游拉取当前分支，仅允许快进。
///
/// A diverged branch is reported with its commit counts instead of being merged or rebased behind the user's back.
/// 已分叉的分支连同两侧提交数报告给用户，不会在用户不知情时被合并或变基。
pub(crate) async fn service_run_git_pull_action(
    state: SharedServiceBridgeState,
    request: GitPullActionRequest,
) -> Result<GitPullActionResponse, WebuiServiceRpcError> {
    let session_id = request
        .conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            WebuiLocalizedMessage::new("serviceError.gitCommit.invalidSession")
                .with("detail", error.to_string())
        })?;
    validate_git_commit_action_session_exists(&state, session_id)?;
    let repository_root = crate::resolve_webui_git_request_repository_root(
        &state,
        Some(&request.workspace_path),
        None,
        Some(&request.conversation_id),
        request.repository_key.clone(),
    )
    .await?;
    // operation_guard prevents concurrent sessions from mutating the same repository index or refs.
    // operation_guard 防止多个会话并发修改同一个仓库的索引或引用。
    let _operation_guard = crate::begin_webui_git_repository_operation(&repository_root).await?;
    crate::ensure_webui_git_repository_monitoring_enabled(
        state,
        Some(request.workspace_path.clone()),
        None,
        Some(request.conversation_id.clone()),
        request.repository_key.as_deref(),
    )
    .await?;
    pull_webui_current_branch(&repository_root)
}

/// Fast-forwards the current branch of one repository to its upstream and reports whether HEAD moved.
/// 将单个仓库的当前分支快进到其上游，并报告 HEAD 是否移动。
fn pull_webui_current_branch(
    repository_root: &Path,
) -> Result<GitPullActionResponse, WebuiServiceRpcError> {
    let branch_name = load_webui_current_git_branch(repository_root)?
        .ok_or_else(|| WebuiLocalizedMessage::new("serviceError.gitPull.detachedHead"))?;
    if !webui_current_branch_has_upstream(repository_root) {
        return Err(
            WebuiLocalizedMessage::new("serviceError.gitPull.noUpstream")
                .with("branch", branch_name)
                .into(),
        );
    }
    let before = load_webui_head_object(repository_root)?;
    let output = run_webui_git_mutating_command(repository_root, &["fetch"])?;
    ensure_git_commit_action_command_succeeded("git fetch", output)?;
    // A branch with commits on both sides cannot fast-forward; say so plainly instead of passing on
    // Git's hints, and leave merging or rebasing to the user.
    // 两侧都有新提交的分支无法快进；直接说明情况而不是转述 Git 的提示，合并或变基交给用户。
    let (ahead, behind) = load_webui_upstream_divergence(repository_root)?;
    if ahead > 0 && behind > 0 {
        return Err(WebuiLocalizedMessage::new("serviceError.gitPull.diverged")
            .with("branch", branch_name)
            .with("ahead", ahead.to_string())
            .with("behind", behind.to_string())
            .into());
    }
    if behind > 0 {
        let output = run_webui_git_mutating_command(
            repository_root,
            &["merge", "--ff-only", "@{upstream}"],
        )?;
        ensure_git_commit_action_command_succeeded("git merge --ff-only", output)?;
    }
    let updated = load_webui_head_object(repository_root)? != before;
    let head_commit = load_webui_head_commit_identity(repository_root)?;
    Ok(GitPullActionResponse {
        summary: String::from(if updated {
            "Fast-forwarded the current branch to its upstream."
        } else {
            "The current branch is already up to date."
        }),
        updated,
        branch_name: Some(branch_name),
        commit_hash: head_commit.as_ref().map(|commit| commit.hash.clone()),
        commit_message: head_commit.map(|commit| commit.subject),
    })
}

/// Returns how many commits the current branch has that its upstream lacks, and the reverse.
/// 返回当前分支有而上游没有的提交数，以及相反方向的提交数。
fn load_webui_upstream_divergence(repository_root: &Path) -> Result<(u64, u64), String> {
    let output = run_webui_git_command(
        repository_root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )?;
    ensure_git_commit_action_command_succeeded(
        "git rev-list --left-right --count",
        output.clone(),
    )?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut counts = text.split_whitespace().map(str::parse::<u64>);
    match (counts.next(), counts.next(), counts.next()) {
        (Some(Ok(ahead)), Some(Ok(behind)), None) => Ok((ahead, behind)),
        _ => Err(format!(
            "unexpected git rev-list --count output: {}",
            text.trim()
        )),
    }
}

/// Returns the full object id HEAD points to, or `None` for a branch without commits.
/// 返回 HEAD 指向的完整对象标识；尚无提交的分支返回 `None`。
fn load_webui_head_object(repository_root: &Path) -> Result<Option<String>, String> {
    let output = run_webui_git_command(repository_root, &["rev-parse", "--verify", "-q", "HEAD"])?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one Git command in a test repository and requires success.
    /// 在测试仓库中运行一条 Git 命令并要求成功。
    fn git(repository_root: &Path, args: &[&str]) {
        let output = run_webui_git_command(repository_root, args).expect("git runs");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Creates a clone of `remote` with a committer identity.
    /// 创建 `remote` 的克隆并配置提交者身份。
    fn clone(remote: &Path, root: &Path, name: &str) -> std::path::PathBuf {
        git(
            root,
            &["clone", "-q", remote.to_str().expect("utf-8 path"), name],
        );
        let path = root.join(name);
        git(&path, &["config", "user.email", "pull@example.test"]);
        git(&path, &["config", "user.name", "Pull Test"]);
        path
    }

    /// Commits one file change in `repository_root`.
    /// 在 `repository_root` 中提交一次文件变更。
    fn commit(repository_root: &Path, body: &str) {
        std::fs::write(repository_root.join("pull.txt"), body).expect("write test file");
        git(repository_root, &["add", "pull.txt"]);
        git(repository_root, &["commit", "-q", "-m", body]);
    }

    /// Verifies pull fast-forwards to new upstream commits, reports an up-to-date branch, refuses a diverged branch and one
    /// without an upstream.
    /// 验证拉取会快进到上游新提交、报告已是最新的分支，并拒绝已分叉的分支和没有上游的分支。
    #[test]
    fn pull_fast_forwards_reports_up_to_date_and_refuses_diverged_or_untracked_branches() {
        let root = tempfile::tempdir().expect("temp dir");
        let remote = root.path().join("remote.git");
        std::fs::create_dir_all(&remote).expect("remote dir");
        git(&remote, &["init", "-q", "--bare"]);
        let author = clone(&remote, root.path(), "author");
        commit(&author, "first");
        git(&author, &["push", "-q", "-u", "origin", "HEAD"]);
        let reader = clone(&remote, root.path(), "reader");
        commit(&author, "second");
        git(&author, &["push", "-q"]);

        let pulled = pull_webui_current_branch(&reader).expect("pull");
        assert!(pulled.updated);
        assert_eq!(pulled.commit_message.as_deref(), Some("second"));
        assert!(
            !pull_webui_current_branch(&reader)
                .expect("second pull")
                .updated
        );

        commit(&reader, "local");
        commit(&author, "remote");
        git(&author, &["push", "-q"]);
        match pull_webui_current_branch(&reader) {
            Err(WebuiServiceRpcError::Localized(message)) => {
                assert_eq!(message.key, "serviceError.gitPull.diverged");
            }
            other => panic!("expected the diverged message, got {other:?}"),
        }

        git(&reader, &["checkout", "-q", "-b", "local-only"]);
        match pull_webui_current_branch(&reader) {
            Err(WebuiServiceRpcError::Localized(message)) => {
                assert_eq!(message.key, "serviceError.gitPull.noUpstream");
            }
            other => panic!("expected the no-upstream message, got {other:?}"),
        }
    }
}
