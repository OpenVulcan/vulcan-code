//! Git commit-message generation service logic.
//! Git 提交信息生成服务逻辑。

#[cfg(test)]
#[path = "commit_summary/live_acceptance_tests.rs"]
mod live_acceptance_tests;

use std::{path::Path, sync::Arc};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_agent_runtime::{
    ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport, ExecutionStreamEvent,
    agent_runtime::{
        conversation_hooks::HOOK_GIT_COMMIT_SUMMARY_CONTEXT,
        provider_v1_bridge::ProviderV1LifecycleFacts,
        vulcan_plugin_hooks::dispatch_vulcan_plugin_hook,
    },
    conversation_protocol::SessionConversationState,
    provider_config::ReasoningEffortLevel,
    stream_runtime_run_with_intervention_plugins_and_public_ai,
};
use vulcan_context::estimate_mixed_language_text_tokens;
use vulcan_hooks::{HookActor, HookCapabilitySet, HookContext, HookScope};
use vulcan_host_tools::output_decode::decode_shell_output_bytes;
use vulcan_session_core::{ExecutionRuntimePolicy, ExecutionScope, SessionNodeId};
use vulcan_workbench_model::SessionId;

use super::command::{format_webui_git_error, run_webui_git_command};
use super::diff::{
    load_webui_staged_git_diff, load_webui_unstaged_git_diff, load_webui_untracked_git_diff,
};
use super::model::{
    GitCommitSummaryGenerateRequest, GitCommitSummaryGenerateResponse, GitCommitSummaryInput,
};
use super::repository::resolve_webui_repository_root_from_workspace_text_with_key;
use super::settings::resolve_git_commit_summary_model;
use crate::{
    AgentRuntimeBridge, ProviderTelemetryHookGuard, SharedServiceBridgeState,
    active_reasoning_selection_effort,
};

/// Generates a Git commit message through the configured internal summary model.
/// 通过已配置的内部总结模型生成 Git 提交信息。
pub(crate) async fn service_generate_git_commit_message(
    state: SharedServiceBridgeState,
    request: GitCommitSummaryGenerateRequest,
) -> Result<GitCommitSummaryGenerateResponse, String> {
    let session_id = request
        .conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("Invalid session identifier; the commit message cannot be generated: {error}")
        })?;
    let resolved_workspace_path = crate::webui_diff_workspace_path_from_state(
        &state,
        None,
        Some(&request.conversation_id),
        Some(&request.workspace_path),
    )?;
    let repository_root = resolve_webui_repository_root_from_workspace_text_with_key(
        &resolved_workspace_path,
        request.repository_key.as_deref(),
    )?;
    let (agent_runtime, provider_id, model_id, reasoning_effort, extra_prompt, plugin_runtime) = {
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
            .ok_or_else(|| {
                String::from("The runtime state of the commit's session was not found.")
            })?;
        let public_ai_configuration = agent_runtime.public_ai_runtime.configuration_snapshot()?;
        let (provider_id, model_id) = resolve_git_commit_summary_model(
            &agent_runtime
                .provider_configuration
                .git_commit
                .summary_model,
            runtime_state,
            &agent_runtime.public_ai_runtime,
            &public_ai_configuration,
        )?;
        let summary_provider_configuration = agent_runtime.provider_configuration.clone();
        let reasoning_effort = runtime_state
            .active_reasoning_effort
            .or_else(|| active_reasoning_selection_effort(&summary_provider_configuration))
            .unwrap_or(ReasoningEffortLevel::Off);
        let extra_prompt = summary_provider_configuration
            .git_commit
            .summary_extra_prompt
            .trim()
            .to_string();
        let plugin_runtime = Some(agent_runtime.vulcan_plugin_runtime.clone());
        agent_runtime.provider_configuration = summary_provider_configuration;
        (
            agent_runtime,
            provider_id,
            model_id,
            reasoning_effort,
            extra_prompt,
            plugin_runtime,
        )
    };
    let summary_input = build_git_commit_summary_input(
        &repository_root,
        request.include_unstaged,
        plugin_runtime.as_ref(),
        session_id,
    )
    .await?;
    let prompt = build_git_commit_summary_prompt(&summary_input, &extra_prompt);
    let estimated_input_tokens = estimate_git_commit_prompt_tokens(&prompt);
    // repository_root_text keeps the owned repository path alive across the async model call.
    // repository_root_text 在异步模型调用期间保持仓库路径所有权有效。
    let repository_root_text = repository_root.to_string_lossy().into_owned();
    let report = run_git_commit_summary_model(
        state.clone(),
        agent_runtime,
        session_id,
        &repository_root_text,
        &provider_id,
        &model_id,
        reasoning_effort,
        prompt,
    )
    .await?;
    let message = report
        .assistant_output
        .as_deref()
        .map(clean_generated_git_commit_message)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("The generated commit message is empty."))?;
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

/// One exact repository section carried through the generic Git context transform hook.
/// 一段通过通用 Git 上下文转换 Hook 传递的精确仓库区块。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GitCommitContextSection {
    /// Stable host or plugin-owned section identifier.
    /// 稳定的宿主或插件自有区块标识。
    id: String,
    /// Human-readable Markdown section title.
    /// 人类可读的 Markdown 区块标题。
    title: String,
    /// Exact section body.
    /// 精确区块正文。
    body: String,
    /// Whether the host must wrap the body in a plain-text code fence.
    /// 宿主是否必须把正文包裹在纯文本代码围栏中。
    #[serde(default)]
    code_fence: bool,
}

/// One generic user-visible contribution returned by a Git context hook.
/// 一条由 Git 上下文 Hook 返回的通用用户可见贡献。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GitCommitContextContribution {
    /// Short stable contribution label.
    /// 简短稳定的贡献标签。
    label: String,
    /// Human-readable contribution value.
    /// 人类可读的贡献值。
    value: String,
}

/// Exact transform payload for `git.commit_summary.context`.
/// `git.commit_summary.context` 使用的精确转换载荷。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GitCommitContextHookPayload {
    /// Protocol schema version.
    /// 协议 Schema 版本。
    schema_version: u32,
    /// Immutable canonical repository root.
    /// 不可变的规范化仓库根目录。
    repository_root: String,
    /// Immutable unstaged-change inclusion policy.
    /// 不可变的未暂存变更包含策略。
    include_unstaged: bool,
    /// Ordered repository context sections.
    /// 有序仓库上下文区块。
    sections: Vec<GitCommitContextSection>,
    /// Ordered host notices.
    /// 有序宿主提示。
    notices: Vec<String>,
    /// Generic display contributions.
    /// 通用展示贡献。
    #[serde(default)]
    contributions: Vec<GitCommitContextContribution>,
}

/// Builds the repository change input for Git commit-message generation.
/// 构建 Git 提交信息生成使用的仓库变更输入。
async fn build_git_commit_summary_input(
    repository_root: &Path,
    include_unstaged: bool,
    plugin_runtime: Option<&vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    session_id: SessionId,
) -> Result<GitCommitSummaryInput, String> {
    let mut notices = Vec::new();
    let mut sections = Vec::new();
    let staged_diff = load_webui_staged_git_diff(repository_root)?;
    append_git_commit_context_section(&mut sections, "staged", "Staged changes", &staged_diff);
    if include_unstaged {
        let unstaged_diff = load_webui_unstaged_git_diff(repository_root)?;
        append_git_commit_context_section(
            &mut sections,
            "unstaged",
            "Unstaged tracked changes",
            &unstaged_diff,
        );
        let untracked_diff = load_webui_untracked_git_diff(repository_root, &mut notices)?;
        append_git_commit_context_section(
            &mut sections,
            "untracked",
            "Untracked files",
            &untracked_diff,
        );
    }
    if sections.is_empty() {
        return Err(String::from(
            "There are no changes available to generate a commit message.",
        ));
    }
    let repository_root_text =
        vulcan_runtime_assets::native_host_path_text_from_path(repository_root);
    let initial_payload = GitCommitContextHookPayload {
        schema_version: 1,
        repository_root: repository_root_text.clone(),
        include_unstaged,
        sections,
        notices,
        contributions: Vec::new(),
    };
    let transformed_payload = apply_git_commit_context_hooks(
        initial_payload,
        plugin_runtime,
        repository_root,
        session_id,
    )
    .await?;
    let full_content = render_git_commit_context_payload(&transformed_payload);
    let token_estimate = estimate_git_commit_prompt_tokens(&full_content);
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

/// Appends one raw Git context section when its body contains useful content.
/// 当正文包含有效内容时追加一段原始 Git 上下文区块。
///
/// # Parameters
/// 参数
/// `sections` receives the structured section and `id` is its stable protocol identity.
/// `sections` 接收结构化区块，`id` 是其稳定协议身份。
/// `title` is the Markdown heading text without leading hashes.
/// `title` 是不包含井号前缀的 Markdown 标题文本。
/// `body` is the raw Git diff body to preserve in the prompt.
/// `body` 是需要在提示词中保留的原始 Git diff 正文。
///
/// # Returns
/// 返回值
/// Returns whether a section was appended.
/// 返回是否实际追加了区块。
fn append_git_commit_context_section(
    sections: &mut Vec<GitCommitContextSection>,
    id: &str,
    title: &str,
    body: &str,
) -> bool {
    if body.trim().is_empty() {
        return false;
    }
    sections.push(GitCommitContextSection {
        id: id.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        code_fence: false,
    });
    true
}

/// Applies the generic plugin transform chain to one exact Git context payload.
/// 对一份精确 Git 上下文载荷应用通用插件转换链。
///
/// # Parameters
/// 参数
/// The payload is host-owned; runtime, repository root, and session provide dispatch context.
/// 载荷由宿主持有；运行时、仓库根与会话提供调度上下文。
///
/// # Returns
/// 返回值
/// Returns a strictly validated payload while preserving immutable host fields.
/// 返回经过严格校验且保留宿主不可变字段的载荷。
async fn apply_git_commit_context_hooks(
    payload: GitCommitContextHookPayload,
    plugin_runtime: Option<&vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    repository_root: &Path,
    session_id: SessionId,
) -> Result<GitCommitContextHookPayload, String> {
    let Some(plugin_runtime) = plugin_runtime else {
        return Ok(payload);
    };
    let original_repository_root = payload.repository_root.clone();
    let original_include_unstaged = payload.include_unstaged;
    let mut context = HookContext::new(
        uuid::Uuid::new_v4().to_string(),
        HOOK_GIT_COMMIT_SUMMARY_CONTEXT,
    );
    context.scope = HookScope::Workspace;
    context.actor = HookActor::new("service/git_commit_summary");
    context.workspace_id = Some(vulcan_runtime_assets::native_host_path_text_from_path(
        repository_root,
    ));
    context.metadata.insert(
        String::from("workspace_root"),
        Value::String(vulcan_runtime_assets::native_host_path_text_from_path(
            repository_root,
        )),
    );
    context.session_id = Some(session_id.to_string());
    context.conversation_id = context.session_id.clone();
    context.capabilities = HookCapabilitySet::default();
    let report = dispatch_vulcan_plugin_hook(
        plugin_runtime.clone(),
        context,
        serde_json::to_value(payload)
            .map_err(|error| format!("Failed to encode Git context hook payload: {error}"))?,
    )
    .await
    .map_err(|error| format!("Git context hook dispatch failed: {error}"))?;
    let transformed: GitCommitContextHookPayload = serde_json::from_value(report.payload.value)
        .map_err(|error| format!("Git context hook returned an invalid payload: {error}"))?;
    if transformed.schema_version != 1 {
        return Err(format!(
            "Git context hook returned unsupported schema_version `{}`.",
            transformed.schema_version
        ));
    }
    if transformed.repository_root != original_repository_root
        || transformed.include_unstaged != original_include_unstaged
    {
        return Err(String::from(
            "Git context hook must preserve repository_root and include_unstaged.",
        ));
    }
    if transformed.sections.is_empty()
        || transformed.sections.iter().any(|section| {
            section.id.trim().is_empty()
                || section.title.trim().is_empty()
                || section.body.trim().is_empty()
        })
    {
        return Err(String::from(
            "Git context hook must return non-empty identified sections.",
        ));
    }
    Ok(transformed)
}

/// Renders one validated Git context hook payload into the model prompt source.
/// 把一份通过校验的 Git 上下文 Hook 载荷渲染为模型提示词来源。
///
/// # Parameters
/// 参数
/// `payload` is the exact post-hook protocol value.
/// `payload` 是精确的 Hook 后协议值。
///
/// # Returns
/// 返回值
/// Returns ordered Markdown preserving notices, contributions, and section bodies.
/// 返回保留提示、贡献与区块正文顺序的 Markdown。
fn render_git_commit_context_payload(payload: &GitCommitContextHookPayload) -> String {
    let mut rendered = Vec::new();
    if !payload.notices.is_empty() {
        rendered.push(format!("## Notices\n\n{}", payload.notices.join("\n")));
    }
    if !payload.contributions.is_empty() {
        rendered.push(format!(
            "## Context transforms\n\n{}",
            payload
                .contributions
                .iter()
                .map(|contribution| format!("- {}: {}", contribution.label, contribution.value))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    for section in &payload.sections {
        if section.body.trim() == "(none)" && section.id != "status" {
            continue;
        }
        if section.code_fence {
            rendered.push(format!(
                "## {}\n\n```text\n{}\n```",
                section.title, section.body
            ));
        } else {
            rendered.push(format!("## {}\n\n{}", section.title, section.body));
        }
    }
    rendered.join("\n\n")
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
    let stdout = decode_shell_output_bytes(&output.stdout).trim().to_string();
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
fn estimate_git_commit_prompt_tokens(text: &str) -> usize {
    estimate_mixed_language_text_tokens(text).max(1usize)
}

/// Runs the internal provider request that generates a Git commit message.
/// 执行生成 Git 提交信息的内部 Provider 请求。
// The summary route needs its own provider, model and reasoning selection.
// 总结路由需要自身的供应商、模型和推理选择。
#[expect(
    clippy::too_many_arguments,
    reason = "The summary route needs its own provider, model and reasoning selection."
)]
async fn run_git_commit_summary_model(
    state: SharedServiceBridgeState,
    agent_runtime: AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    // Validated summary provider and model, independent from the conversation's route.
    // 已校验的总结供应商和模型，独立于会话路由。
    provider_id: &str,
    model_id: &str,
    reasoning_effort: ReasoningEffortLevel,
    prompt: String,
) -> Result<ExecutionRunReport, String> {
    // request preserves the owner and carries the same target returned by the service response.
    // request 保留所属会话，并携带与服务响应一致的目标。
    let request = build_git_commit_summary_execution_request(
        session_id,
        workspace_path,
        provider_id,
        model_id,
        reasoning_effort,
        prompt,
    );
    run_git_commit_summary_stream(state, agent_runtime, request).await
}

/// Builds the tool-free summary request from the exact validated service target.
/// 使用服务校验后的精确目标构建不含工具的总结请求。
///
/// Takes the owning session, repository, provider/model, reasoning and prepared prompt.
/// 接收所属会话、仓库、供应商和模型、思考档位以及已准备的提示词。
/// Returns an excluded non-session request retaining the original accounting owner.
/// 返回排除于会话账本之外、同时保留原统计归属的请求。
fn build_git_commit_summary_execution_request(
    session_id: SessionId,
    workspace_path: &str,
    provider_id: &str,
    model_id: &str,
    reasoning_effort: ReasoningEffortLevel,
    prompt: String,
) -> ExecutionPreparationRequest {
    ExecutionPreparationRequest {
        session_id,
        execution_scope: ExecutionScope::ContextCompaction {
            node_id: SessionNodeId::from(session_id),
        },
        runtime_policy: ExecutionRuntimePolicy::context_compaction(),
        conversation_hook_turn_id: None,
        continuation_action_id: None,
        provider_request_continuation: None,
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        dedicated_session: None,
        provider_id_override: Some(provider_id.to_string()),
        model_id_override: Some(model_id.to_string()),
        connection_id_override: None,
        provider_route_lease_identity: None,
        provider_tool_selections_override: Default::default(),
        provider_tool_media_understanding_selections_override: Default::default(),
        provider_tool_enabled_capabilities_override: Default::default(),
        provider_extension_target_sets_override: Default::default(),
        subagent_model_grants: None,
        subagent_prompt_context: None,
        session_storage_directory: None,
        session_title: String::from("Git commit summary"),
        workspace_path: workspace_path.to_string(),
        project_id: None,
        prompt,
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: None,
        conversation_state: SessionConversationState::default(),
        context_compaction_policy: None,
        context_compaction_window_node_id: None,
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: String::from("GitCommitSummary"),
        temperature: None,
        reasoning_effort,
        tool_exposure_override: Default::default(),
        captured_child_tool_admission: None,
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose: ExecutionPromptPurpose::GitCommitSummary,
        prompt_skill_policy_override: Default::default(),
        project_prompt_skills: Default::default(),
        session_prompt_skill_overrides: Default::default(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: Default::default(),
        session_vmm: Default::default(),
        vmm_runtime_ready: false,
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        native_goal_runtime: None,
        native_browser_runtime: None,
        native_agent_collaboration_runtime: None,
        supplemental_mcp_tools: Vec::new(),
        media_resource_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    }
}

/// Executes the frozen summary request and forwards its usage through the existing owner pipeline.
/// 执行冻结的总结请求，通过已有归属链转发用量。
///
/// Takes shared service state, runtime handles and the validated summary request.
/// 接收共享服务状态、运行时句柄与已校验的总结请求。
/// Returns the terminal report or explicit execution/persistence failure.
/// 返回终态报告或明确的执行、持久化失败。
async fn run_git_commit_summary_stream(
    state: SharedServiceBridgeState,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
) -> Result<ExecutionRunReport, String> {
    let mut stream = Box::pin(stream_runtime_run_with_intervention_plugins_and_public_ai(
        agent_runtime.provider_configuration.clone(),
        agent_runtime.agent_profile_catalog.clone(),
        agent_runtime.luaskills_runtime.clone(),
        Some(agent_runtime.vulcan_plugin_runtime.clone()),
        Some(Arc::clone(&agent_runtime.extension_generation)),
        Arc::clone(&agent_runtime.task_runtime),
        Arc::clone(&agent_runtime.question_runtime),
        Arc::clone(&agent_runtime.command_session_runtime),
        Some(Arc::clone(&agent_runtime.public_ai_runtime)),
        request,
        None,
    ));
    // providerTelemetryGuard forwards statistics and closes an active request if this future is dropped.
    // providerTelemetryGuard 转发统计，并在当前 Future 被丢弃时关闭活动请求。
    let controller_runtime = crate::controller_runtime_from_state(&state)?;
    let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
        state.clone(),
        Some(controller_runtime.clone()),
    );
    while let Some(event) = stream.next().await {
        match event {
            ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
                provider_telemetry_guard.forward(event);
            }
            ExecutionStreamEvent::ProviderRequestPreparationBoundary {
                event,
                candidate,
                completion,
            } => {
                let result = match event.v1_lifecycle {
                    Some(ProviderV1LifecycleFacts::ExcludedNonSession) => {
                        candidate.into_excluded_non_session()
                    }
                    _ => Err(String::from(
                        "Git commit summary Provider preparation is not marked as excluded from the session ledger.",
                    )),
                };
                let response = result.as_ref().map(|_| ()).map_err(Clone::clone);
                let _ = completion.complete(result);
                response?;
            }
            ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
                if !matches!(
                    event.v1_lifecycle,
                    Some(ProviderV1LifecycleFacts::ExcludedNonSession)
                ) {
                    let error = String::from(
                        "Git commit summary Provider boundary is not marked as excluded from the session ledger.",
                    );
                    let _ = completion.complete(Err(error.clone()));
                    return Err(error);
                }
                provider_telemetry_guard.observe_durable_boundary(&event);
                provider_telemetry_guard.forward(event);
                let result = Ok(());
                let _ = completion.complete(result.clone());
                result?;
            }
            ExecutionStreamEvent::Finished(report) => {
                if let Some(event) = provider_telemetry_guard.take_cancelled_event() {
                    provider_telemetry_guard.forward(event);
                }
                if let Some(error) = report.error.as_ref() {
                    return Err(error.clone());
                }
                return Ok(report);
            }
            _ => {}
        }
    }
    if let Some(event) = provider_telemetry_guard.take_cancelled_event() {
        provider_telemetry_guard.forward(event);
    }
    Err(String::from(
        "git commit summary stream ended without a terminal report.",
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a summary request preserves ownership and carries the selected model into execution.
    /// 验证总结请求保留所属会话，并将选中模型传入执行链。
    /// Returns normally only when no global/session target can substitute for the validated pair.
    /// 仅当已校验的目标组合不会被全局或会话默认替换时正常返回。
    #[test]
    fn git_summary_request_preserves_explicit_target_and_owner() {
        // owner identifies the real conversation without creating a synthetic accounting session.
        // owner 标识真实会话，不创建虚构的统计会话。
        let owner = SessionId::new_v4();
        // request is built by the same function used immediately before stream execution.
        // request 由实际流执行前使用的同一个函数构建。
        let request = build_git_commit_summary_execution_request(
            owner,
            "D:/acceptance",
            "summary-provider",
            "summary-model",
            ReasoningEffortLevel::Off,
            String::from("Summarize the reviewed diff."),
        );
        assert_eq!(request.session_id, owner);
        assert_eq!(
            request.provider_id_override.as_deref(),
            Some("summary-provider")
        );
        assert_eq!(request.model_id_override.as_deref(), Some("summary-model"));
        assert_eq!(
            request.prompt_purpose,
            ExecutionPromptPurpose::GitCommitSummary
        );
        assert!(request.provider_route_lease_identity.is_none());
        assert!(request.connection_id_override.is_none());
        assert!(request.conversation_hook_turn_id.is_none());
        assert!(
            matches!(request.execution_scope, ExecutionScope::ContextCompaction { node_id } if node_id == SessionNodeId::from(owner))
        );
    }

    /// Verifies blank Git context bodies do not create empty protocol sections.
    /// 验证空白 Git 上下文正文不会创建空协议区块。
    #[test]
    fn git_commit_context_section_skips_blank_body() {
        let mut sections = Vec::new();

        assert!(!append_git_commit_context_section(
            &mut sections,
            "staged_diff",
            "Staged changes",
            " \n\t"
        ));
        assert!(sections.is_empty());

        assert!(append_git_commit_context_section(
            &mut sections,
            "staged_diff",
            "Staged changes",
            "diff --git a/lib.rs b/lib.rs"
        ));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].id, "staged_diff");
        assert_eq!(sections[0].title, "Staged changes");
        assert_eq!(sections[0].body, "diff --git a/lib.rs b/lib.rs");
    }

    /// Verifies generated commit messages discard surrounding Markdown fences.
    /// 验证生成的提交信息会丢弃外围 Markdown 代码围栏。
    #[test]
    fn generated_commit_message_removes_markdown_fence() {
        assert_eq!(
            clean_generated_git_commit_message("```text\n实现插件迁移\n```"),
            "实现插件迁移"
        );
    }
}
