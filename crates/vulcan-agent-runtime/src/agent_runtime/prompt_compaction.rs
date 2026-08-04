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
use crate::provider_config::{ProviderConfigurationFile, ReasoningEffortLevel};
use crate::tooling::CommandSessionRuntime;
use async_stream::stream;
use futures_util::{StreamExt, stream::Stream};
use std::sync::Arc;
use vulcan_context::SessionConversationState;
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionResult, ContextCompactionRequestPromptMode, build_compaction_request_prompt_for_mode,
    extract_compaction_model_summary,
};
use vulcan_core::types::ProviderUsageSnapshot;

mod pruning;

#[cfg(test)]
mod tests;

use pruning::{
    append_prune_report_to_error, compaction_attempt_conversation_state,
    compaction_prompt_mode_for_prune_mode, initial_compaction_prune_mode,
    input_prune_report_for_execution, next_compaction_retry_mode,
    prepared_compaction_source_for_attempt,
};

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
    /// Full runtime conversation cloned before any visible compaction status rows are inserted.
    /// 插入任何可见压缩状态行之前克隆的完整运行时会话。
    pub full_context_state: SessionConversationState,
    /// Optional target input budget used to decide clone-only pruning for window downgrades.
    /// 用于判断窗口降级时是否执行仅克隆剪枝的可选目标输入预算。
    pub input_budget: Option<CompactionInputBudget>,
    /// Latest observed provider input tokens used before local estimates when deciding downgrade.
    /// 判断是否降级时优先于本地估算使用的最近一次 Provider 实测输入 Token。
    pub observed_input_tokens: Option<usize>,
    /// Latest live budget input estimate used when provider feedback is unavailable.
    /// Provider 反馈不可用时使用的最近一次实时预算输入估算。
    pub estimated_input_tokens: Option<usize>,
    /// Provider configuration already adjusted to the intended provider/model snapshot.
    /// 已经调整到目标供应商/模型快照的 Provider 配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Optional provider/model label kept only for diagnostics.
    /// 仅供诊断使用的可选供应商/模型标签。
    pub model_label: Option<String>,
    /// Effective reasoning preset that should be forwarded into the internal summary call.
    /// 应转发到内部总结调用中的生效推理档位。
    pub reasoning_effort: ReasoningEffortLevel,
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
    /// Full runtime conversation cloned before any visible compaction status rows are inserted.
    /// 插入任何可见压缩状态行之前克隆的完整运行时会话。
    pub full_context_state: SessionConversationState,
    /// Optional target input budget used to decide clone-only pruning for window downgrades.
    /// 用于判断窗口降级时是否执行仅克隆剪枝的可选目标输入预算。
    pub input_budget: Option<CompactionInputBudget>,
    /// Latest observed provider input tokens used before local estimates when deciding downgrade.
    /// 判断是否降级时优先于本地估算使用的最近一次 Provider 实测输入 Token。
    pub observed_input_tokens: Option<usize>,
    /// Latest live budget input estimate used when provider feedback is unavailable.
    /// Provider 反馈不可用时使用的最近一次实时预算输入估算。
    pub estimated_input_tokens: Option<usize>,
    /// Provider configuration already adjusted to the intended provider/model snapshot.
    /// 已经调整到目标供应商/模型快照的 Provider 配置。
    pub provider_configuration: ProviderConfigurationFile,
    /// Optional provider/model label kept only for diagnostics.
    /// 仅供诊断使用的可选供应商/模型标签。
    pub model_label: Option<String>,
    /// Effective reasoning preset that should be forwarded into the internal summary call.
    /// 应转发到内部总结调用中的生效推理档位。
    pub reasoning_effort: ReasoningEffortLevel,
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
    /// Latest successful provider usage captured while producing the summary.
    /// 产出该总结期间捕获到的最近一次成功 Provider 用量。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Provider identifier used by the successful compaction request.
    /// 成功压缩请求使用的 Provider 标识。
    pub provider_id: Option<String>,
    /// Model identifier used by the successful compaction request.
    /// 成功压缩请求使用的模型标识。
    pub model_id: Option<String>,
    /// Execution profile used by the successful compaction request.
    /// 成功压缩请求使用的执行配置。
    pub execution_profile: PromptCompactionExecutionProfile,
    /// Optional clone-only pruning report for the successful compaction attempt.
    /// 成功压缩尝试对应的可选仅克隆剪枝报告。
    pub input_prune_report: Option<CompactionInputPruneReport>,
}

/// Provider-facing launch profile selected for one prompt-compaction attempt.
/// 单次提示词压缩尝试选择的 Provider 可见启动配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCompactionExecutionProfile {
    /// Full-fidelity cache-preserving launch that keeps normal prompt topology and tool schemas.
    /// 保持完整保真缓存的启动方式，保留普通提示词拓扑与工具 schema。
    CachePreservingFullFidelity,
    /// Minimal pruned launch that strips tool schemas and normal assistant prompt overhead.
    /// 极简裁剪启动方式，移除工具 schema 与普通助手提示词开销。
    PrunedMinimal,
}

impl PromptCompactionExecutionProfile {
    /// Returns the stable persisted label for this execution profile.
    /// 返回该执行配置的稳定持久化标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::CachePreservingFullFidelity => "cache_preserving_full_fidelity",
            Self::PrunedMinimal => "pruned_minimal",
        }
    }
}

/// Lightweight progress update emitted before buffered model output is committed.
/// 在缓冲的模型输出提交前发出的轻量进度更新。
#[derive(Debug, Clone)]
pub struct PromptCompactionProgressEvent {
    /// Stable machine-readable progress stage.
    /// 稳定的机器可读进度阶段。
    pub stage: String,
}

/// Incremental event emitted while one prompt-compaction request is running.
/// 单次提示词压缩请求运行期间发出的增量事件。
#[derive(Debug, Clone)]
pub enum PromptCompactionStreamEvent {
    /// Progress update that is safe to show even if the current attempt later retries.
    /// 即使当前尝试随后重试也可以安全展示的进度更新。
    Progress(PromptCompactionProgressEvent),
    /// A normal execution stream event that should be mirrored into transcript UI or task logs.
    /// 应镜像到转录界面或任务日志中的普通执行流事件。
    Stream(ExecutionStreamEvent),
    /// Terminal compaction result after the internal summary request settles.
    /// 内部总结请求收敛后的终态压缩结果。
    Finished(Result<PromptCompactionExecution, String>),
}

/// Builds a prompt-compaction progress stream item.
/// 构建一条提示词压缩进度流事件。
fn prompt_compaction_progress(stage: impl Into<String>) -> PromptCompactionStreamEvent {
    PromptCompactionStreamEvent::Progress(PromptCompactionProgressEvent {
        stage: stage.into(),
    })
}

/// Projects safe runtime lifecycle events into compaction progress updates.
/// 将安全的运行时生命周期事件投影为压缩进度更新。
fn prompt_compaction_progress_for_runtime_event(
    event: &ExecutionStreamEvent,
) -> Option<PromptCompactionStreamEvent> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Some(prompt_compaction_progress("prepared")),
        ExecutionStreamEvent::RequestReady { .. } => {
            Some(prompt_compaction_progress("request_ready"))
        }
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => {
            Some(prompt_compaction_progress("provider_checkpoint"))
        }
        ExecutionStreamEvent::ProviderLoweringRepair { .. } => {
            Some(prompt_compaction_progress("provider_lowering_repair"))
        }
        ExecutionStreamEvent::WireRequestReady { .. } => {
            Some(prompt_compaction_progress("wire_request_ready"))
        }
        ExecutionStreamEvent::RequestStarted { .. } => {
            Some(prompt_compaction_progress("request_started"))
        }
        ExecutionStreamEvent::ThinkingDelta { .. } => {
            Some(prompt_compaction_progress("thinking_delta"))
        }
        ExecutionStreamEvent::OutputDelta { .. } => {
            Some(prompt_compaction_progress("output_delta"))
        }
        ExecutionStreamEvent::ProviderRequestTelemetry(_)
        | ExecutionStreamEvent::RuntimeEvent { .. }
        | ExecutionStreamEvent::ConversationCheckpoint { .. }
        | ExecutionStreamEvent::ThinkingCompleted { .. }
        | ExecutionStreamEvent::Finished(_) => None,
    }
}

/// Streams one prompt-based compaction job and exposes the model's thinking/output before applying the final compaction result.
/// 以流式方式执行单次提示词压缩任务，并在应用最终压缩结果前暴露模型思考与输出。
pub fn stream_prompt_compaction_job(
    request: PromptCompactionJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    stream! {
        let mut prune_mode = initial_compaction_prune_mode(
            &request.full_context_state,
            request.input_budget,
            request.observed_input_tokens,
            request.estimated_input_tokens,
        );
        let mut full_fidelity_attempts = 0usize;
        let mut pruned_attempts = 0usize;
        let mut provider_request_count = 0usize;
        loop {
            if prune_mode == CompactionInputPruneMode::None {
                full_fidelity_attempts = full_fidelity_attempts.saturating_add(1);
            } else {
                pruned_attempts = pruned_attempts.saturating_add(1);
            }
            yield prompt_compaction_progress("prepare_input");
            let prepared_source = prepared_compaction_source_for_attempt(
                &request.plan,
                &request.full_context_state,
                request.input_budget,
                prune_mode,
            );
            yield prompt_compaction_progress("build_request");
            let source_conversation_state = match compaction_attempt_conversation_state(
                &request.full_context_state,
                &prepared_source.source_messages,
                prune_mode,
            ) {
                Ok(source_conversation_state) => source_conversation_state,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            };
            let prompt_mode =
                compaction_prompt_mode_for_prune_mode(prepared_source.prune_report.mode);
            let launch_request = build_prompt_compaction_launch_request(
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.reasoning_effort,
                source_conversation_state,
                prompt_mode,
            );

            yield prompt_compaction_progress("prepare_runtime");
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
                Ok(launch) => {
                    yield prompt_compaction_progress("runtime_ready");
                    launch
                }
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
                        if let Some(progress) = prompt_compaction_progress_for_runtime_event(&event)
                        {
                            yield progress;
                        }
                        yield PromptCompactionStreamEvent::Stream(event);
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
        let mut prune_mode = initial_compaction_prune_mode(
            &request.full_context_state,
            request.input_budget,
            request.observed_input_tokens,
            request.estimated_input_tokens,
        );
        let mut full_fidelity_attempts = 0usize;
        let mut pruned_attempts = 0usize;
        let mut provider_request_count = 0usize;
        loop {
            if prune_mode == CompactionInputPruneMode::None {
                full_fidelity_attempts = full_fidelity_attempts.saturating_add(1);
            } else {
                pruned_attempts = pruned_attempts.saturating_add(1);
            }
            yield prompt_compaction_progress("prepare_input");
            let prepared_source = prepared_compaction_source_for_attempt(
                &request.plan,
                &request.full_context_state,
                request.input_budget,
                prune_mode,
            );
            yield prompt_compaction_progress("build_request");
            let source_conversation_state = match compaction_attempt_conversation_state(
                &request.full_context_state,
                &prepared_source.source_messages,
                prune_mode,
            ) {
                Ok(source_conversation_state) => source_conversation_state,
                Err(error) => {
                    yield PromptCompactionStreamEvent::Finished(Err(error));
                    return;
                }
            };
            let prompt_mode =
                compaction_prompt_mode_for_prune_mode(prepared_source.prune_report.mode);
            let launch_request = build_prompt_compaction_launch_request(
                request.session_id,
                &request.session_title,
                &request.workspace_path,
                request.reasoning_effort,
                source_conversation_state,
                prompt_mode,
            );

            yield prompt_compaction_progress("prepare_execution");
            let prepared = match prepare_execution_with_tool_snapshot(
                &request.foundation,
                request.luaskills_runtime.clone(),
                &request.provider_configuration,
                &launch_request,
            ) {
                Ok(prepared) => {
                    yield prompt_compaction_progress("execution_ready");
                    prepared
                }
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
            yield prompt_compaction_progress("bootstrap_runtime");
            let bootstrap = bootstrap_runtime_stream(
                &request.foundation,
                request.luaskills_runtime.clone(),
                &launch_request,
                prepared.preparation,
                prepared.tool_snapshot,
            )
            .await;
            let bootstrap = match bootstrap {
                Ok(bootstrap) => {
                    yield prompt_compaction_progress("runtime_ready");
                    bootstrap
                }
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
                        if let Some(progress) = prompt_compaction_progress_for_runtime_event(&event)
                        {
                            yield progress;
                        }
                        yield PromptCompactionStreamEvent::Stream(event);
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

/// Builds the shared execution-preparation request for prompt-based context compaction.
/// 构建提示词式上下文压缩使用的共享执行准备请求。
///
/// # Parameters
/// # 参数
/// `prompt_mode` selects the prompt variant that matches the prepared compaction input.
/// `prompt_mode` 选择与已准备压缩输入匹配的提示词变体。
fn build_prompt_compaction_launch_request(
    session_id: crate::model::SessionId,
    session_title: &str,
    workspace_path: &str,
    reasoning_effort: ReasoningEffortLevel,
    conversation_state: vulcan_context::SessionConversationState,
    prompt_mode: ContextCompactionRequestPromptMode,
) -> ExecutionPreparationRequest {
    let prompt_purpose = match prompt_mode {
        ContextCompactionRequestPromptMode::FullFidelityMerge => {
            ExecutionPromptPurpose::ContextCompactionCachePreserving
        }
        ContextCompactionRequestPromptMode::PrunedFallback => {
            ExecutionPromptPurpose::ContextCompaction
        }
    };
    ExecutionPreparationRequest {
        session_id,
        managed_codebase_memory: None,
        execution_scope: vulcan_session_core::ExecutionScope::ContextCompaction {
            node_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::context_compaction(),
        conversation_hook_turn_id: None,
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        provider_tool_selections_override: Default::default(),
        subagent_prompt_context: None,
        task_session_store_root: None,
        session_title: session_title.to_string(),
        workspace_path: workspace_path.to_string(),
        prompt: build_compaction_request_prompt_for_mode(prompt_mode),
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        conversation_state,
        context_compaction_policy: None,
        context_budget_override: None,
        execution_mode: String::from("Compaction"),
        temperature: None,
        reasoning_effort,
        tool_exposure_override: Default::default(),
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose,
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
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
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
        let provider_id = report
            .preparation
            .as_ref()
            .map(|preparation| preparation.model_blueprint.provider_id.clone());
        let model_id = report
            .preparation
            .as_ref()
            .map(|preparation| preparation.model_blueprint.model_id.clone());
        let execution_profile = match report
            .preparation
            .as_ref()
            .map(|preparation| preparation.prompt_purpose)
        {
            Some(ExecutionPromptPurpose::ContextCompactionCachePreserving) => {
                PromptCompactionExecutionProfile::CachePreservingFullFidelity
            }
            _ => PromptCompactionExecutionProfile::PrunedMinimal,
        };
        let last_successful_provider_usage = report.last_successful_provider_usage.clone();
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
            last_successful_provider_usage,
            provider_id,
            model_id,
            execution_profile,
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
