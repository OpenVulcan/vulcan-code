//! Git commit and push action service logic.
//! Git 提交与推送操作服务逻辑。

use std::path::Path;

use vulcan_workbench_model::SessionId;

use super::command::{
    format_webui_git_error, run_webui_git_command, run_webui_git_mutating_command,
    run_webui_git_mutating_command_with_stdin,
};
use super::diff::{
    ensure_webui_git_ref_text, load_webui_current_branch_unpushed_commit_count,
    load_webui_current_git_branch, load_webui_head_commit_identity,
    resolve_webui_repository_root_from_workspace_text, webui_current_branch_has_upstream,
    webui_git_ref_exists,
};
use super::model::{GitCommitActionRequest, GitCommitActionResponse, GitCommitActionTargetRequest};
use crate::{SharedServiceBridgeState, workspace_root};

/// Runs the requested Git commit and optional push operation.
/// 执行请求的 Git 提交与可选推送操作。
pub(crate) async fn service_run_git_commit_action(
    state: SharedServiceBridgeState,
    request: GitCommitActionRequest,
) -> Result<GitCommitActionResponse, String> {
    let session_id = request
        .conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("会话标识无效，无法执行提交：{error}"))?;
    validate_git_commit_action_session_exists(&state, session_id)?;
    let repository_root =
        resolve_webui_repository_root_from_workspace_text(&request.workspace_path)?;
    let commit_message = request.message.trim();
    if commit_message.is_empty() {
        if request.include_unstaged {
            return Err(String::from("只推送时不能暂存工作区变更。"));
        }
        if matches!(request.target, GitCommitActionTargetRequest::New { .. }) {
            return Err(String::from("创建新分支需要同时创建提交。"));
        }
        if request.push && load_webui_current_branch_unpushed_commit_count(&repository_root)? == 0 {
            return Err(String::from("当前分支没有已提交但未推送的内容。"));
        }
    }
    let mut committed = false;
    let mut pushed = false;
    let mut summary_rows = Vec::new();

    match request.target {
        GitCommitActionTargetRequest::Current => {}
        GitCommitActionTargetRequest::New { branch_name } => {
            let branch_name = normalize_git_commit_action_branch_name(&branch_name)?;
            ensure_git_commit_action_new_branch_available(&repository_root, &branch_name)?;
            let output = run_webui_git_mutating_command(
                &repository_root,
                &["checkout", "-b", &branch_name],
            )?;
            ensure_git_commit_action_command_succeeded("git checkout -b", output)?;
            summary_rows.push(format!("已创建并切换到分支 `{branch_name}`。"));
        }
    }

    if request.include_unstaged {
        let output = run_webui_git_mutating_command(&repository_root, &["add", "-A"])?;
        ensure_git_commit_action_command_succeeded("git add -A", output)?;
        summary_rows.push(String::from("已暂存工作区变更。"));
    }

    if !commit_message.is_empty() {
        ensure_git_commit_action_has_staged_changes(&repository_root)?;
        let output = run_webui_git_mutating_command_with_stdin(
            &repository_root,
            &["commit", "-F", "-"],
            commit_message.as_bytes(),
        )?;
        ensure_git_commit_action_command_succeeded("git commit", output)?;
        committed = true;
        summary_rows.push(String::from("已创建 Git 提交。"));
    }

    if request.push {
        let branch_name = load_webui_current_git_branch(&repository_root)?
            .ok_or_else(|| String::from("当前处于 detached HEAD，无法推送分支。"))?;
        let push_args = if webui_current_branch_has_upstream(&repository_root) {
            vec!["push"]
        } else {
            vec!["push", "-u", "origin", branch_name.as_str()]
        };
        let output = run_webui_git_mutating_command(&repository_root, &push_args)?;
        ensure_git_commit_action_command_succeeded("git push", output)?;
        pushed = true;
        summary_rows.push(String::from("已推送当前分支。"));
    }

    let branch_name = load_webui_current_git_branch(&repository_root)?;
    let head_commit = if committed || pushed {
        load_webui_head_commit_identity(&repository_root)?
    } else {
        None
    };
    if summary_rows.is_empty() {
        summary_rows.push(String::from("没有执行提交或推送操作。"));
    }
    Ok(GitCommitActionResponse {
        summary: summary_rows.join(" "),
        committed,
        pushed,
        branch_name,
        commit_hash: head_commit.as_ref().map(|commit| commit.hash.clone()),
        commit_message: head_commit.map(|commit| commit.subject),
    })
}

/// Ensures one Git commit action command completed successfully.
/// 确保单个 Git 提交动作命令已成功完成。
/// Parameters: label is the stable error label, and output is the completed Git process output.
/// 参数：label 是稳定错误标签，output 是已完成的 Git 进程输出。
/// Returns: `Ok(())` on success, or the formatted Git error on failure.
/// 返回：成功时返回 `Ok(())`，失败时返回格式化后的 Git 错误。
fn ensure_git_commit_action_command_succeeded(
    label: &str,
    output: std::process::Output,
) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    Err(format_webui_git_error(label, &output))
}

/// Validates that a Git commit action is attached to a known session.
/// 校验 Git 提交动作附着在已知会话上。
fn validate_git_commit_action_session_exists(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    if workspace_snapshot
        .session_runtime_state
        .contains_key(&session_id)
    {
        Ok(())
    } else {
        Err(String::from("无法找到提交动作所属会话。"))
    }
}

/// Normalizes and validates a branch name requested by the commit dialog.
/// 归一化并校验提交弹窗请求的分支名称。
fn normalize_git_commit_action_branch_name(branch_name: &str) -> Result<String, String> {
    let branch_name = branch_name.trim().to_string();
    ensure_webui_git_ref_text(&branch_name)?;
    let output = run_webui_git_mutating_command(
        &workspace_root(),
        &["check-ref-format", "--branch", branch_name.as_str()],
    )?;
    ensure_git_commit_action_command_succeeded("git check-ref-format --branch", output)?;
    Ok(branch_name)
}

/// Ensures the requested new branch does not already exist locally.
/// 确保请求的新分支在本地尚不存在。
fn ensure_git_commit_action_new_branch_available(
    repository_root: &Path,
    branch_name: &str,
) -> Result<(), String> {
    if webui_git_ref_exists(repository_root, &format!("refs/heads/{branch_name}"))? {
        return Err(format!("本地分支 `{branch_name}` 已存在。"));
    }
    Ok(())
}

/// Ensures the Git index contains staged changes before committing.
/// 在提交前确保 Git 索引中存在已暂存变更。
fn ensure_git_commit_action_has_staged_changes(repository_root: &Path) -> Result<(), String> {
    let output = run_webui_git_command(repository_root, &["diff", "--cached", "--quiet"])?;
    if output.status.success() {
        return Err(String::from("当前没有已暂存变更，无法提交。"));
    }
    match output.status.code() {
        Some(1) => Ok(()),
        _ => Err(format_webui_git_error("git diff --cached --quiet", &output)),
    }
}
