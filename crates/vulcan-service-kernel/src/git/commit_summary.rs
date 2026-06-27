//! Git commit-message generation service logic.
//! Git 提交信息生成服务逻辑。

use std::{path::Path, sync::Arc};

use futures_util::StreamExt;
use vulcan_agent_runtime::{
    ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport, ExecutionStreamEvent,
    conversation_protocol::SessionConversationState, provider_config::ReasoningEffortLevel,
    stream_runtime_run_with_intervention,
};
use vulcan_context::estimate_mixed_language_text_tokens;
use vulcan_session_core::{ExecutionRuntimePolicy, ExecutionScope, SessionNodeId};
use vulcan_workbench_model::SessionId;

use super::command::{format_webui_git_error, run_webui_git_command};
use super::diff::{
    load_webui_staged_git_diff, load_webui_unstaged_git_diff, load_webui_untracked_git_diff,
    resolve_webui_repository_root_from_workspace_text,
};
use super::model::{
    GitCommitSummaryGenerateRequest, GitCommitSummaryGenerateResponse, GitCommitSummaryInput,
};
use super::settings::resolve_git_commit_summary_model;
use crate::{AgentRuntimeBridge, SharedServiceBridgeState, active_reasoning_selection_effort};

/// Generates a Git commit message through the configured internal summary model.
/// 通过已配置的内部总结模型生成 Git 提交信息。
pub(crate) async fn service_generate_git_commit_message(
    state: SharedServiceBridgeState,
    request: GitCommitSummaryGenerateRequest,
) -> Result<GitCommitSummaryGenerateResponse, String> {
    let session_id = request
        .conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("会话标识无效，无法生成提交信息：{error}"))?;
    let repository_root =
        resolve_webui_repository_root_from_workspace_text(&request.workspace_path)?;
    let (agent_runtime, provider_id, model_id, reasoning_effort, extra_prompt) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let mut agent_runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
            .clone();
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| String::from("无法找到提交所属会话的运行时状态。"))?;
        let (provider_id, model_id) = resolve_git_commit_summary_model(
            &agent_runtime
                .provider_configuration
                .git_commit
                .summary_model,
            runtime_state,
            &agent_runtime.provider_configuration,
        )?;
        let mut summary_provider_configuration = agent_runtime.provider_configuration.clone();
        summary_provider_configuration.model = Some(format!("{provider_id}/{model_id}"));
        let reasoning_effort = runtime_state
            .active_reasoning_effort
            .or_else(|| active_reasoning_selection_effort(&summary_provider_configuration))
            .unwrap_or(ReasoningEffortLevel::Medium);
        let extra_prompt = summary_provider_configuration
            .git_commit
            .summary_extra_prompt
            .trim()
            .to_string();
        agent_runtime.provider_configuration = summary_provider_configuration;
        (
            agent_runtime,
            provider_id,
            model_id,
            reasoning_effort,
            extra_prompt,
        )
    };
    let summary_input = build_git_commit_summary_input(
        &repository_root,
        request.include_unstaged,
        &model_id,
        agent_runtime.token_budget_runtime.estimator(),
    )?;
    let prompt = build_git_commit_summary_prompt(&summary_input, &extra_prompt);
    let estimated_input_tokens = estimate_git_commit_prompt_tokens(
        &model_id,
        &prompt,
        agent_runtime.token_budget_runtime.estimator(),
    );
    let report = run_git_commit_summary_model(
        agent_runtime,
        session_id,
        &request.workspace_path,
        reasoning_effort,
        prompt,
    )
    .await?;
    let message = report
        .assistant_output
        .as_deref()
        .map(clean_generated_git_commit_message)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("提交信息生成结果为空。"))?;
    Ok(GitCommitSummaryGenerateResponse {
        message,
        provider_id,
        model_id,
        provider_request_count: report.provider_request_count,
        input_was_truncated: summary_input.truncated,
        estimated_input_tokens,
        provider_usage: report.last_successful_provider_usage,
    })
}

/// Maximum token budget reserved for Git commit-message diff input.
/// Git 提交信息 diff 输入预留的最大 Token 预算。
const GIT_COMMIT_SUMMARY_DIFF_TOKEN_BUDGET: usize = 100_000;

/// Builds the repository change input for Git commit-message generation.
/// 构建 Git 提交信息生成使用的仓库变更输入。
fn build_git_commit_summary_input(
    repository_root: &Path,
    include_unstaged: bool,
    model_id: &str,
    estimator: Option<&dyn vulcan_context::ContextTokenEstimator>,
) -> Result<GitCommitSummaryInput, String> {
    let mut notices = Vec::new();
    let mut sections = Vec::new();
    let staged_diff = load_webui_staged_git_diff(repository_root)?;
    if !staged_diff.trim().is_empty() {
        sections.push(format!("## Staged changes\n\n{staged_diff}"));
    }
    if include_unstaged {
        let unstaged_diff = load_webui_unstaged_git_diff(repository_root)?;
        if !unstaged_diff.trim().is_empty() {
            sections.push(format!("## Unstaged tracked changes\n\n{unstaged_diff}"));
        }
        let untracked_diff = load_webui_untracked_git_diff(repository_root, &mut notices)?;
        if !untracked_diff.trim().is_empty() {
            sections.push(format!("## Untracked files\n\n{untracked_diff}"));
        }
    }
    if sections.is_empty() {
        return Err(String::from("当前没有可用于生成提交信息的变更。"));
    }
    if !notices.is_empty() {
        sections.insert(0, format!("## Notices\n\n{}", notices.join("\n")));
    }
    let full_content = sections.join("\n\n");
    let token_estimate = estimate_git_commit_prompt_tokens(model_id, &full_content, estimator);
    if token_estimate <= GIT_COMMIT_SUMMARY_DIFF_TOKEN_BUDGET {
        return Ok(GitCommitSummaryInput {
            content: full_content,
            truncated: false,
        });
    }
    Ok(GitCommitSummaryInput {
        content: build_git_commit_summary_metadata_input(repository_root, include_unstaged)?,
        truncated: true,
    })
}

/// Builds a compact change-metadata fallback when the full diff is too large.
/// 在完整 diff 过大时构建紧凑的变更元数据回退输入。
fn build_git_commit_summary_metadata_input(
    repository_root: &Path,
    include_unstaged: bool,
) -> Result<String, String> {
    let mut sections = Vec::new();
    sections.push(load_git_commit_summary_command_section(
        repository_root,
        "Status",
        &["status", "--short", "--branch"],
    )?);
    sections.push(load_git_commit_summary_command_section(
        repository_root,
        "Staged diff stat",
        &["diff", "--cached", "--stat", "--no-color"],
    )?);
    sections.push(load_git_commit_summary_command_section(
        repository_root,
        "Staged file status",
        &["diff", "--cached", "--name-status", "--no-color"],
    )?);
    if include_unstaged {
        sections.push(load_git_commit_summary_command_section(
            repository_root,
            "Unstaged diff stat",
            &["diff", "--stat", "--no-color"],
        )?);
        sections.push(load_git_commit_summary_command_section(
            repository_root,
            "Unstaged file status",
            &["diff", "--name-status", "--no-color"],
        )?);
    }
    Ok(sections.join("\n\n"))
}

/// Runs one read-only Git metadata command and formats its output as a prompt section.
/// 执行单条只读 Git 元数据命令并将输出格式化为提示词片段。
fn load_git_commit_summary_command_section(
    repository_root: &Path,
    title: &str,
    args: &[&str],
) -> Result<String, String> {
    let output = run_webui_git_command(repository_root, args)?;
    if !output.status.success() {
        return Err(format_webui_git_error(
            &format!("git {}", args.join(" ")),
            &output,
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let body = if stdout.is_empty() {
        "(none)".to_string()
    } else {
        stdout
    };
    Ok(format!("## {title}\n\n```text\n{body}\n```"))
}

/// Builds the fixed English prompt used by the commit-message summary model.
/// 构建提交信息总结模型使用的固定英文提示词。
fn build_git_commit_summary_prompt(input: &GitCommitSummaryInput, extra_prompt: &str) -> String {
    let mut prompt = String::from(
        "Generate a Git commit message for the repository changes below.\n\
         Requirements:\n\
         - Output only the commit message text.\n\
         - Do not wrap the result in Markdown or a code fence.\n\
         - Use a concise subject line followed by an optional blank line and body when useful.\n\
         - Base the message only on the provided change data.\n",
    );
    if input.truncated {
        prompt.push_str(
            "- The full diff exceeded the input budget, so only status, file lists, and diff stats are provided. Produce a higher-level message from that metadata.\n",
        );
    }
    if !extra_prompt.trim().is_empty() {
        prompt.push_str("\nAdditional user preferences:\n");
        prompt.push_str(extra_prompt.trim());
        prompt.push('\n');
    }
    prompt.push_str("\nRepository change data:\n");
    prompt.push_str(&input.content);
    prompt
}

/// Estimates text tokens for Git commit-message prompt budgeting.
/// 为 Git 提交信息提示预算估算文本 Token 数。
fn estimate_git_commit_prompt_tokens(
    model_id: &str,
    text: &str,
    estimator: Option<&dyn vulcan_context::ContextTokenEstimator>,
) -> usize {
    estimator
        .and_then(|estimator| estimator.estimate_text_tokens(model_id, text))
        .unwrap_or_else(|| estimate_mixed_language_text_tokens(text))
        .max(1usize)
}

/// Runs the internal provider request that generates a Git commit message.
/// 执行生成 Git 提交信息的内部 Provider 请求。
async fn run_git_commit_summary_model(
    agent_runtime: AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    reasoning_effort: ReasoningEffortLevel,
    prompt: String,
) -> Result<ExecutionRunReport, String> {
    let request = ExecutionPreparationRequest {
        session_id,
        execution_scope: ExecutionScope::ContextCompaction {
            node_id: SessionNodeId::from(session_id),
        },
        runtime_policy: ExecutionRuntimePolicy::context_compaction(),
        execution_agent_id: None,
        agent_system_prompt_override: None,
        task_session_store_root: None,
        session_title: String::from("Git commit summary"),
        workspace_path: workspace_path.to_string(),
        prompt,
        conversation_state: SessionConversationState::default(),
        execution_mode: String::from("GitCommitSummary"),
        temperature: None,
        reasoning_effort,
        ai_tool_configuration: agent_runtime.ai_tool_configuration.clone(),
        tool_exposure_override: Default::default(),
        prompt_purpose: ExecutionPromptPurpose::GitCommitSummary,
        project_prompt_skills: Default::default(),
        session_prompt_skill_overrides: Default::default(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: Default::default(),
        session_vmm: Default::default(),
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    };
    let mut stream = Box::pin(stream_runtime_run_with_intervention(
        agent_runtime.provider_configuration.clone(),
        agent_runtime.agent_profile_catalog.clone(),
        agent_runtime.luaskills_runtime.clone(),
        agent_runtime.mcp_runtime.clone(),
        Arc::clone(&agent_runtime.task_runtime),
        Arc::clone(&agent_runtime.question_runtime),
        Arc::clone(&agent_runtime.command_session_runtime),
        Arc::clone(&agent_runtime.token_budget_runtime),
        request,
        None,
    ));
    while let Some(event) = stream.next().await {
        if let ExecutionStreamEvent::Finished(report) = event {
            if let Some(error) = report.error.as_ref() {
                return Err(error.clone());
            }
            return Ok(report);
        }
    }
    Err(String::from("提交信息生成请求未返回终态报告。"))
}

/// Cleans model-generated commit message text before it reaches `git commit`.
/// 在生成文本进入 `git commit` 前清理提交信息。
fn clean_generated_git_commit_message(message: &str) -> String {
    message
        .trim()
        .trim_start_matches("```text")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}
