//! Shared prompt-based context-compaction runtime.
//! 共享提示词式上下文压缩运行时。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_preparation::prepare_execution_with_tool_snapshot;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_driver::drive_runtime_stream_launch;
use crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch;
use crate::agent_runtime::runtime_stream_launch_payload::build_runtime_stream_launch_payload;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::agent_runtime::{
    ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport, ExecutionStreamEvent,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeAccess;
use crate::provider_config::{
    PersistedAiToolConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
};
use crate::tooling::CommandSessionRuntime;
use async_stream::stream;
use futures_util::{StreamExt, stream::Stream};
use std::sync::Arc;
use vulcan_context::conversation_state_from_session_messages;
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionResult, build_compaction_request_prompt, estimate_compaction_source_tokens,
    extract_compaction_model_summary, prepare_compaction_source_for_target_budget,
};

/// Maximum full-fidelity prompt-compaction attempts before clone-only downgrade is allowed.
/// 允许仅克隆降级前的最大完整保真提示词压缩尝试次数。
const MAX_FULL_FIDELITY_COMPACTION_ATTEMPTS: usize = 3;

/// Immutable request payload required to execute one prompt-based compaction job.
/// 执行单次提示词式压缩任务所需的不可变请求载荷。
#[derive(Clone)]
pub struct PromptCompactionJobRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: crate::model::SessionId,
    /// Target session title used by preparation diagnostics.
    /// 供准备阶段诊断使用的目标会话标题。
    pub session_title: String,
    /// Effective workspace path associated with the session.
    /// 与该会话关联的生效工作区路径。
    pub workspace_path: String,
    /// Planned compaction window whose source messages should be summarized.
    /// 其源消息需要被总结的已规划压缩窗口。
    pub plan: CompactionPlan,
    /// Optional target input budget used to decide clone-only pruning for window downgrades.
    /// 用于判断窗口降级时是否执行仅克隆剪枝的可选目标输入预算。
    pub input_budget: Option<CompactionInputBudget>,
    /// Provider configuration already adjusted to the intended provider/model snapshot.
    /// 已经调整到目标供应商/模型快照的 Provider 配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Optional provider/model label kept only for diagnostics.
    /// 仅供诊断使用的可选供应商/模型标签。
    pub model_label: Option<String>,
    /// Effective reasoning preset that should be forwarded into the internal summary call.
    /// 应转发到内部总结调用中的生效推理档位。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Effective AI-tool configuration captured for the internal summary call.
    /// 为内部总结调用捕获的生效 AI 工具配置。
    pub ai_tool_configuration: PersistedAiToolConfiguration,
    /// Shared agent-profile catalog used to rebuild one fresh runtime foundation.
    /// 用于重建新运行时基础架构的共享代理画像目录。
    pub agent_profile_catalog: AgentProfileCatalog,
    /// Optional LuaSkills runtime handle reused by the preparation pipeline.
    /// 被准备链路复用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Stable MCP runtime access surface reused by the preparation pipeline.
    /// 被准备链路复用的稳定 MCP 运行时访问面。
    pub mcp_runtime_access: McpRuntimeAccess,
    /// Shared resumable task runtime reused by host-controlled transport execution.
    /// 供宿主控制传输执行复用的共享可恢复任务运行时。
    pub task_runtime: Arc<TaskRuntime>,
    /// Shared question runtime reused by the execution foundation.
    /// 供执行基础架构复用的共享提问运行时。
    pub question_runtime: Arc<crate::agent_runtime::question_runtime::QuestionRuntime>,
    /// Shared command-session runtime reused by the execution foundation.
    /// 供执行基础架构复用的共享命令会话运行时。
    pub command_session_runtime: Arc<CommandSessionRuntime>,
    /// Shared token budget runtime reused by the execution foundation.
    /// 供执行基础架构复用的共享 Token 预算运行时。
    pub token_budget_runtime: Arc<TokenBudgetRuntime>,
}

/// Immutable request payload required to execute one prompt-based compaction job from an already resolved runtime foundation.
/// 从已解析运行时基础架构执行单次提示词式压缩任务所需的不可变请求载荷。
#[derive(Clone)]
pub struct PromptCompactionFoundationJobRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: crate::model::SessionId,
    /// Target session title used by preparation diagnostics.
    /// 供准备阶段诊断使用的目标会话标题。
    pub session_title: String,
    /// Effective workspace path associated with the session.
    /// 与该会话关联的生效工作区路径。
    pub workspace_path: String,
    /// Planned compaction window whose source messages should be summarized.
    /// 其源消息需要被总结的已规划压缩窗口。
    pub plan: CompactionPlan,
    /// Optional target input budget used to decide clone-only pruning for window downgrades.
    /// 用于判断窗口降级时是否执行仅克隆剪枝的可选目标输入预算。
    pub input_budget: Option<CompactionInputBudget>,
    /// Provider configuration already adjusted to the intended provider/model snapshot.
    /// 已经调整到目标供应商/模型快照的 Provider 配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Optional provider/model label kept only for diagnostics.
    /// 仅供诊断使用的可选供应商/模型标签。
    pub model_label: Option<String>,
    /// Effective reasoning preset that should be forwarded into the internal summary call.
    /// 应转发到内部总结调用中的生效推理档位。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Effective AI-tool configuration captured for the internal summary call.
    /// 为内部总结调用捕获的生效 AI 工具配置。
    pub ai_tool_configuration: PersistedAiToolConfiguration,
    /// Runtime foundation that already owns resolved agents, tools, LuaSkills, MCP, and shared runtimes.
    /// 已持有解析后代理、工具、LuaSkills、MCP 与共享运行时的运行时基础架构。
    pub foundation: Arc<RuntimeFoundation>,
    /// Optional LuaSkills runtime handle reused by the preparation pipeline.
    /// 被准备链路复用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
}

/// Terminal result returned by one prompt-based compaction job after the internal summary request settles.
/// 单次提示词式压缩任务在内部总结请求收敛后返回的终态结果。
#[derive(Debug, Clone)]
pub struct PromptCompactionExecution {
    /// Final prompt-shaped compaction result ready to be applied to the session state.
    /// 准备好应用到会话状态的最终提示词形态压缩结果。
    pub compaction_result: CompactionResult,
    /// Whether the model request fell back; prompt compaction now treats this as an invalid terminal state.
    /// 模型请求是否发生回退；提示词压缩现在将其视为无效终态。
    pub fallback_used: bool,
    /// Number of provider requests dispatched while producing the summary.
    /// 产出该总结期间发出的 Provider 请求次数。
    pub provider_request_count: usize,
    /// Optional clone-only pruning report for the successful compaction attempt.
    /// 成功压缩尝试对应的可选仅克隆剪枝报告。
    pub input_prune_report: Option<CompactionInputPruneReport>,
}

/// Incremental event emitted while one prompt-compaction request is running.
/// 单次提示词压缩请求运行期间发出的增量事件。
#[derive(Debug, Clone)]
pub enum PromptCompactionStreamEvent {
    /// A normal execution stream event that should be mirrored into transcript UI or task logs.
    /// 应镜像到转录界面或任务日志中的普通执行流事件。
    Stream(ExecutionStreamEvent),
    /// Terminal compaction result after the internal summary request settles.
    /// 内部总结请求收敛后的终态压缩结果。
    Finished(Result<PromptCompactionExecution, String>),
}

/// Streams one prompt-based compaction job and exposes the model's thinking/output before applying the final compaction result.
/// 以流式方式执行单次提示词压缩任务，并在应用最终压缩结果前暴露模型思考与输出。
pub fn stream_prompt_compaction_job(
    request: PromptCompactionJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    stream! {
        let mut prune_mode = initial_compaction_prune_mode(&request.plan, request.input_budget);
        let mut full_fidelity_attempts = 0usize;
        let mut pruned_attempts = 0usize;
        let mut provider_request_count = 0usize;
        loop {
            if prune_mode == CompactionInputPruneMode::None {
                full_fidelity_attempts = full_fidelity_attempts.saturating_add(1);
            } else {
                pruned_attempts = pruned_attempts.saturating_add(1);
            }
            let prepared_source = prepared_compaction_source_for_attempt(
                &request.plan,
                request.input_budget,
                prune_mode,
            );
            let source_conversation_state =
                conversation_state_from_session_messages(&prepared_source.source_messages);
            let launch_request = build_prompt_compaction_launch_request(
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.reasoning_effort,
                request.ai_tool_configuration.clone(),
                source_conversation_state,
            );

            let launch = prepare_runtime_stream_launch(
                &request.provider_configuration,
                &request.agent_profile_catalog,
                request.luaskills_runtime.clone(),
                request.mcp_runtime_access.clone(),
                request.task_runtime.clone(),
                request.question_runtime.clone(),
                request.command_session_runtime.clone(),
                request.token_budget_runtime.clone(),
                launch_request,
            )
            .await;

            let launch = match launch {
                Ok(launch) => launch,
                Err(report) => {
                    let error = build_prompt_compaction_failure_message(
                        request.model_label.as_deref(),
                        &report,
                    );
                    match next_compaction_retry_mode(
                        &error,
                        prune_mode,
                        full_fidelity_attempts,
                        pruned_attempts,
                        request.input_budget,
                    ) {
                        Some(next_mode) => {
                            prune_mode = next_mode;
                            continue;
                        }
                        None => {
                            yield PromptCompactionStreamEvent::Finished(Err(
                                append_prune_report_to_error(error, &prepared_source.prune_report),
                            ));
                            return;
                        }
                    }
                }
            };

            let runtime_stream =
                drive_runtime_stream_launch(request.provider_configuration.clone(), launch, None);
            futures_util::pin_mut!(runtime_stream);
            let mut terminal_error = None;
            let mut attempt_events = Vec::new();
            while let Some(event) = runtime_stream.next().await {
                match event {
                    ExecutionStreamEvent::Finished(report) => {
                        provider_request_count =
                            provider_request_count.saturating_add(report.provider_request_count);
                        match finalize_prompt_compaction_report(
                            &request.plan,
                            request.model_label.as_deref(),
                            report,
                        ) {
                            Ok(mut execution) => {
                                execution.provider_request_count = provider_request_count;
                                execution.input_prune_report = input_prune_report_for_execution(
                                    &prepared_source.prune_report,
                                );
                                for event in attempt_events {
                                    yield PromptCompactionStreamEvent::Stream(event);
                                }
                                yield PromptCompactionStreamEvent::Finished(Ok(execution));
                                return;
                            }
                            Err(error) => {
                                terminal_error = Some(error);
                                break;
                            }
                        }
                    }
                    event => {
                        attempt_events.push(event);
                    }
                }
            }

            let error = terminal_error.unwrap_or_else(|| {
                String::from("Prompt compaction stream ended without a terminal execution report.")
            });
            match next_compaction_retry_mode(
                &error,
                prune_mode,
                full_fidelity_attempts,
                pruned_attempts,
                request.input_budget,
            ) {
                Some(next_mode) => {
                    prune_mode = next_mode;
                }
                None => {
                    yield PromptCompactionStreamEvent::Finished(Err(
                        append_prune_report_to_error(error, &prepared_source.prune_report),
                    ));
                    return;
                }
            }
        }
    }
}

/// Streams one prompt-based compaction job through an already resolved runtime foundation.
/// 通过已解析运行时基础架构流式执行单次提示词式压缩任务。
pub fn stream_prompt_compaction_foundation_job(
    request: PromptCompactionFoundationJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    stream! {
        let mut prune_mode = initial_compaction_prune_mode(&request.plan, request.input_budget);
        let mut full_fidelity_attempts = 0usize;
        let mut pruned_attempts = 0usize;
        let mut provider_request_count = 0usize;
        loop {
            if prune_mode == CompactionInputPruneMode::None {
                full_fidelity_attempts = full_fidelity_attempts.saturating_add(1);
            } else {
                pruned_attempts = pruned_attempts.saturating_add(1);
            }
            let prepared_source = prepared_compaction_source_for_attempt(
                &request.plan,
                request.input_budget,
                prune_mode,
            );
            let source_conversation_state =
                conversation_state_from_session_messages(&prepared_source.source_messages);
            let launch_request = build_prompt_compaction_launch_request(
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.reasoning_effort,
                request.ai_tool_configuration.clone(),
                source_conversation_state,
            );

            let prepared = match prepare_execution_with_tool_snapshot(
                &request.foundation,
                request.luaskills_runtime.clone(),
                &request.provider_configuration,
                &launch_request,
            ) {
                Ok(prepared) => prepared,
                Err(error) => {
                    let error = format!("Prompt compaction preparation failed: {error}");
                    match next_compaction_retry_mode(
                        &error,
                        prune_mode,
                        full_fidelity_attempts,
                        pruned_attempts,
                        request.input_budget,
                    ) {
                        Some(next_mode) => {
                            prune_mode = next_mode;
                            continue;
                        }
                        None => {
                            yield PromptCompactionStreamEvent::Finished(Err(
                                append_prune_report_to_error(error, &prepared_source.prune_report),
                            ));
                            return;
                        }
                    }
                }
            };
            let bootstrap = bootstrap_runtime_stream(
                &request.foundation,
                request.luaskills_runtime.clone(),
                &launch_request,
                prepared.preparation,
                prepared.tool_snapshot,
            )
            .await;
            let bootstrap = match bootstrap {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    let error = format!("Prompt compaction bootstrap failed: {error}");
                    match next_compaction_retry_mode(
                        &error,
                        prune_mode,
                        full_fidelity_attempts,
                        pruned_attempts,
                        request.input_budget,
                    ) {
                        Some(next_mode) => {
                            prune_mode = next_mode;
                            continue;
                        }
                        None => {
                            yield PromptCompactionStreamEvent::Finished(Err(
                                append_prune_report_to_error(error, &prepared_source.prune_report),
                            ));
                            return;
                        }
                    }
                }
            };
            let launch = build_runtime_stream_launch_payload(&launch_request, bootstrap, None);

            let runtime_stream =
                drive_runtime_stream_launch(request.provider_configuration.clone(), launch, None);
            futures_util::pin_mut!(runtime_stream);
            let mut terminal_error = None;
            let mut attempt_events = Vec::new();
            while let Some(event) = runtime_stream.next().await {
                match event {
                    ExecutionStreamEvent::Finished(report) => {
                        provider_request_count =
                            provider_request_count.saturating_add(report.provider_request_count);
                        match finalize_prompt_compaction_report(
                            &request.plan,
                            request.model_label.as_deref(),
                            report,
                        ) {
                            Ok(mut execution) => {
                                execution.provider_request_count = provider_request_count;
                                execution.input_prune_report = input_prune_report_for_execution(
                                    &prepared_source.prune_report,
                                );
                                for event in attempt_events {
                                    yield PromptCompactionStreamEvent::Stream(event);
                                }
                                yield PromptCompactionStreamEvent::Finished(Ok(execution));
                                return;
                            }
                            Err(error) => {
                                terminal_error = Some(error);
                                break;
                            }
                        }
                    }
                    event => {
                        attempt_events.push(event);
                    }
                }
            }

            let error = terminal_error.unwrap_or_else(|| {
                String::from("Prompt compaction stream ended without a terminal execution report.")
            });
            match next_compaction_retry_mode(
                &error,
                prune_mode,
                full_fidelity_attempts,
                pruned_attempts,
                request.input_budget,
            ) {
                Some(next_mode) => {
                    prune_mode = next_mode;
                }
                None => {
                    yield PromptCompactionStreamEvent::Finished(Err(
                        append_prune_report_to_error(error, &prepared_source.prune_report),
                    ));
                    return;
                }
            }
        }
    }
}

/// Selects the initial pruning mode from the target compaction input budget.
/// 根据目标压缩输入预算选择初始剪枝模式。
///
/// # Parameters
/// # 参数
/// `plan` is the immutable compaction plan whose source size is checked.
/// `plan` 是需要检查源消息大小的不可变压缩计划。
/// `input_budget` is the optional target input budget for the selected model window.
/// `input_budget` 是所选模型窗口对应的可选目标输入预算。
///
/// # Returns
/// # 返回值
/// Returns a pruning mode that either keeps full input or requires clone-only window-downgrade pruning.
/// 返回保留完整输入或要求仅克隆窗口降级剪枝的剪枝模式。
fn initial_compaction_prune_mode(
    plan: &CompactionPlan,
    input_budget: Option<CompactionInputBudget>,
) -> CompactionInputPruneMode {
    let Some(input_budget) = input_budget else {
        return CompactionInputPruneMode::None;
    };
    let source_token_estimate = estimate_compaction_source_tokens(&plan.source_messages);
    if source_token_estimate > input_budget.target_input_budget_tokens {
        CompactionInputPruneMode::RequiredForWindowDowngrade
    } else {
        CompactionInputPruneMode::None
    }
}

/// Builds the source message list for one prompt-compaction attempt.
/// 构建单次提示词压缩尝试使用的源消息列表。
///
/// # Parameters
/// # 参数
/// `plan` is the immutable compaction plan for the current attempt.
/// `plan` 是当前尝试使用的不可变压缩计划。
/// `input_budget` is the optional target model input budget.
/// `input_budget` 是可选的目标模型输入预算。
/// `prune_mode` is the mode selected for this attempt.
/// `prune_mode` 是本次尝试选定的剪枝模式。
///
/// # Returns
/// # 返回值
/// Returns an owned prepared source and its pruning report without mutating the plan.
/// 返回拥有所有权的已准备源消息及其剪枝报告，不会修改计划。
fn prepared_compaction_source_for_attempt(
    plan: &CompactionPlan,
    input_budget: Option<CompactionInputBudget>,
    prune_mode: CompactionInputPruneMode,
) -> vulcan_context_compaction::PreparedCompactionSource {
    let Some(input_budget) = input_budget else {
        let source_token_estimate = estimate_compaction_source_tokens(&plan.source_messages);
        return vulcan_context_compaction::PreparedCompactionSource {
            source_messages: plan.source_messages.clone(),
            prune_report: CompactionInputPruneReport {
                mode: CompactionInputPruneMode::None,
                target_input_budget_tokens: 0,
                safety_reserve_tokens: 0,
                original_token_estimate: source_token_estimate,
                pruned_token_estimate: source_token_estimate,
                pruned_tool_output_count: 0,
                preserved_tool_output_count: 0,
                dropped_source_message_count: 0,
                truncated_source_message_count: 0,
                still_exceeds_budget: false,
            },
        };
    };
    prepare_compaction_source_for_target_budget(
        &plan.source_messages,
        input_budget,
        plan.policy,
        prune_mode,
    )
}

/// Selects the next retry mode after a prompt-compaction attempt fails.
/// 在一次提示词压缩尝试失败后选择下一次重试模式。
///
/// # Parameters
/// # 参数
/// `error` is the terminal error text produced by the failed attempt.
/// `error` 是失败尝试产出的终态错误文本。
/// `current_mode` is the pruning mode used by the failed attempt.
/// `current_mode` 是失败尝试使用的剪枝模式。
/// `full_fidelity_attempts` is the number of full-input attempts already made.
/// `full_fidelity_attempts` 是已经执行过的完整输入尝试次数。
/// `pruned_attempts` is the number of clone-pruned attempts already made.
/// `pruned_attempts` 是已经执行过的仅克隆剪枝尝试次数。
/// `input_budget` is the optional target input budget required for downgrade pruning.
/// `input_budget` 是降级剪枝所需的可选目标输入预算。
///
/// # Returns
/// # 返回值
/// Returns the next mode to retry, or `None` when no safe retry remains.
/// 返回下一次重试模式；当不存在安全重试时返回 `None`。
fn next_compaction_retry_mode(
    error: &str,
    current_mode: CompactionInputPruneMode,
    full_fidelity_attempts: usize,
    pruned_attempts: usize,
    input_budget: Option<CompactionInputBudget>,
) -> Option<CompactionInputPruneMode> {
    if current_mode != CompactionInputPruneMode::None {
        return (pruned_attempts < MAX_FULL_FIDELITY_COMPACTION_ATTEMPTS).then_some(current_mode);
    }
    if prompt_compaction_error_is_context_overflow(error) {
        return input_budget.map(|_| CompactionInputPruneMode::RetryAfterFailure);
    }
    if full_fidelity_attempts < MAX_FULL_FIDELITY_COMPACTION_ATTEMPTS {
        return Some(CompactionInputPruneMode::None);
    }
    input_budget.map(|_| CompactionInputPruneMode::RetryAfterFailure)
}

/// Returns the clone-only pruning report only when the successful attempt used pruning.
/// 仅在成功尝试使用剪枝时返回仅克隆剪枝报告。
///
/// # Parameters
/// # 参数
/// `prune_report` is the report produced for the successful model-facing input.
/// `prune_report` 是成功的模型侧输入对应的剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns `Some` only when clone-only pruning was used; otherwise returns `None`.
/// 仅当使用了仅克隆剪枝时返回 `Some`，否则返回 `None`。
fn input_prune_report_for_execution(
    prune_report: &CompactionInputPruneReport,
) -> Option<CompactionInputPruneReport> {
    (prune_report.mode != CompactionInputPruneMode::None).then(|| prune_report.clone())
}

/// Returns whether one provider or preparation error clearly indicates context overflow.
/// 判断 provider 或准备错误是否明确表示上下文溢出。
///
/// # Parameters
/// # 参数
/// `error` is the provider, bootstrap, preparation, or summary extraction error text.
/// `error` 是 provider、bootstrap、准备阶段或摘要提取阶段的错误文本。
///
/// # Returns
/// # 返回值
/// Returns `true` only for known context-window overflow phrases.
/// 仅在匹配已知上下文窗口溢出短语时返回 `true`。
fn prompt_compaction_error_is_context_overflow(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    [
        "prompt too long",
        "context length exceeded",
        "maximum context length",
        "max context length",
        "input too long",
        "context window",
        "too many tokens",
        "token limit",
        "tokens exceed",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

/// Appends clone-only pruning diagnostics to a terminal compaction failure.
/// 将仅克隆剪枝诊断追加到终态压缩失败信息中。
///
/// # Parameters
/// # 参数
/// `error` is the original terminal error text.
/// `error` 是原始终态错误文本。
/// `prune_report` is the report produced for the failed attempt input.
/// `prune_report` 是失败尝试输入对应的剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns the original error or the error with clone-only pruning diagnostics appended.
/// 返回原始错误，或追加了仅克隆剪枝诊断的错误。
fn append_prune_report_to_error(
    error: String,
    prune_report: &CompactionInputPruneReport,
) -> String {
    if prune_report.mode == CompactionInputPruneMode::None {
        return error;
    }
    format!(
        "{error}\n\nCompaction input pruning: mode={:?}, target_budget={}, safety_reserve={}, original_tokens={}, pruned_tokens={}, pruned_tool_outputs={}, preserved_tool_outputs={}, dropped_source_messages={}, truncated_source_messages={}, still_exceeds_budget={}.",
        prune_report.mode,
        prune_report.target_input_budget_tokens,
        prune_report.safety_reserve_tokens,
        prune_report.original_token_estimate,
        prune_report.pruned_token_estimate,
        prune_report.pruned_tool_output_count,
        prune_report.preserved_tool_output_count,
        prune_report.dropped_source_message_count,
        prune_report.truncated_source_message_count,
        prune_report.still_exceeds_budget
    )
}

/// Builds the shared execution-preparation request for prompt-based context compaction.
/// 构建提示词式上下文压缩使用的共享执行准备请求。
fn build_prompt_compaction_launch_request(
    session_id: crate::model::SessionId,
    session_title: &str,
    workspace_path: &str,
    reasoning_effort: ReasoningEffortLevel,
    ai_tool_configuration: PersistedAiToolConfiguration,
    conversation_state: vulcan_context::SessionConversationState,
) -> ExecutionPreparationRequest {
    ExecutionPreparationRequest {
        session_id,
        execution_scope: vulcan_session_core::ExecutionScope::ContextCompaction {
            node_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::context_compaction(),
        execution_agent_id: None,
        agent_system_prompt_override: None,
        task_session_store_root: None,
        session_title: session_title.to_string(),
        workspace_path: workspace_path.to_string(),
        prompt: build_compaction_request_prompt(),
        conversation_state,
        execution_mode: String::from("Compaction"),
        temperature: None,
        reasoning_effort,
        ai_tool_configuration,
        tool_exposure_override: Default::default(),
        prompt_purpose: ExecutionPromptPurpose::ContextCompaction,
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
    }
}

/// Finalizes one terminal execution report returned by the internal summary run into the prompt-based compaction result.
/// 将内部总结运行返回的终态执行报告整理为提示词式压缩结果。
fn finalize_prompt_compaction_report(
    plan: &CompactionPlan,
    model_label: Option<&str>,
    report: ExecutionRunReport,
) -> Result<PromptCompactionExecution, String> {
    if report.is_success() {
        let summary_text = report
            .assistant_output
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| build_prompt_compaction_failure_message(model_label, &report))?;
        let summary_text = extract_compaction_model_summary(summary_text)
            .ok_or_else(|| build_prompt_compaction_failure_message(model_label, &report))?;
        return Ok(PromptCompactionExecution {
            compaction_result: CompactionResult::from_model_summary(plan, summary_text),
            fallback_used: false,
            provider_request_count: report.provider_request_count,
            input_prune_report: None,
        });
    }

    Err(build_prompt_compaction_failure_message(
        model_label,
        &report,
    ))
}

/// Builds one human-readable failure message for prompt-based compaction without usable plain-text summary output.
/// 当提示词式压缩没有可用纯文本总结输出时构建人类可读失败信息。
fn build_prompt_compaction_failure_message(
    model_label: Option<&str>,
    report: &ExecutionRunReport,
) -> String {
    let model_label = model_label.unwrap_or("unknown-model");
    let report_error = report
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("the model did not return a usable plain-text summary");
    format!("Prompt compaction on {model_label} did not produce a usable summary: {report_error}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_budget() -> CompactionInputBudget {
        CompactionInputBudget {
            target_input_budget_tokens: 100,
            safety_reserve_tokens: 30_000,
        }
    }

    /// Verifies explicit context-overflow failures skip full-fidelity retries.
    /// 验证明示上下文溢出失败会跳过完整保真重试。
    #[test]
    fn context_overflow_error_immediately_downgrades() {
        assert!(prompt_compaction_error_is_context_overflow(
            "maximum context length exceeded"
        ));
        assert_eq!(
            next_compaction_retry_mode(
                "maximum context length exceeded",
                CompactionInputPruneMode::None,
                1,
                0,
                Some(test_budget()),
            ),
            Some(CompactionInputPruneMode::RetryAfterFailure)
        );
    }

    /// Verifies non-context failures keep retrying full-fidelity input before downgrade.
    /// 验证非上下文失败会先继续重试完整保真输入。
    #[test]
    fn non_context_error_retries_full_fidelity_before_downgrade() {
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::None,
                1,
                0,
                Some(test_budget()),
            ),
            Some(CompactionInputPruneMode::None)
        );
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::None,
                2,
                0,
                Some(test_budget()),
            ),
            Some(CompactionInputPruneMode::None)
        );
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::None,
                3,
                0,
                Some(test_budget()),
            ),
            Some(CompactionInputPruneMode::RetryAfterFailure)
        );
    }

    /// Verifies missing input budget allows full retries but never clone-pruned downgrade.
    /// 验证缺少输入预算时允许完整重试但不会进入克隆剪枝降级。
    #[test]
    fn missing_budget_never_downgrades_to_pruning() {
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::None,
                1,
                0,
                None,
            ),
            Some(CompactionInputPruneMode::None)
        );
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::None,
                3,
                0,
                None,
            ),
            None
        );
    }

    /// Verifies clone-pruned attempts still retry transient non-context failures.
    /// 验证仅克隆剪枝尝试遇到非上下文临时失败时仍会重试。
    #[test]
    fn pruned_attempt_retries_non_context_failures_before_stopping() {
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::RequiredForWindowDowngrade,
                0,
                1,
                Some(test_budget()),
            ),
            Some(CompactionInputPruneMode::RequiredForWindowDowngrade)
        );
        assert_eq!(
            next_compaction_retry_mode(
                "temporary provider connection failed",
                CompactionInputPruneMode::RequiredForWindowDowngrade,
                0,
                3,
                Some(test_budget()),
            ),
            None
        );
    }
}
