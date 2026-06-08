//! Shared prompt-based context-compaction runtime.
//! 共享提示词式上下文压缩运行时。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_preparation::prepare_execution;
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
    CompactionPlan, CompactionResult, build_compaction_request_prompt,
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
        let source_conversation_state =
            conversation_state_from_session_messages(&request.plan.source_messages);
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
                yield PromptCompactionStreamEvent::Finished(Err(build_prompt_compaction_failure_message(
                    request.model_label.as_deref(),
                    &report,
                )));
                return;
            }
        };

        let runtime_stream =
            drive_runtime_stream_launch(request.provider_configuration.clone(), launch, None);
        futures_util::pin_mut!(runtime_stream);
        while let Some(event) = runtime_stream.next().await {
            match event {
                ExecutionStreamEvent::Finished(report) => {
                    yield PromptCompactionStreamEvent::Finished(finalize_prompt_compaction_report(
                        &request.plan,
                        request.model_label.as_deref(),
                        report,
                    ));
                    return;
                }
                event => {
                    yield PromptCompactionStreamEvent::Stream(event);
                }
            }
        }

        yield PromptCompactionStreamEvent::Finished(Err(String::from(
            "Prompt compaction stream ended without a terminal execution report.",
        )));
    }
}

/// Streams one prompt-based compaction job through an already resolved runtime foundation.
/// 通过已解析运行时基础架构流式执行单次提示词式压缩任务。
pub fn stream_prompt_compaction_foundation_job(
    request: PromptCompactionFoundationJobRequest,
) -> impl Stream<Item = PromptCompactionStreamEvent> + Send + 'static {
    stream! {
        let source_conversation_state =
            conversation_state_from_session_messages(&request.plan.source_messages);
        let launch_request = build_prompt_compaction_launch_request(
            request.session_id,
            &request.session_title,
            &request.workspace_path,
            request.reasoning_effort,
            request.ai_tool_configuration.clone(),
            source_conversation_state,
        );

        let preparation = match prepare_execution(
            &request.foundation,
            request.luaskills_runtime.clone(),
            &request.provider_configuration,
            &launch_request,
        ) {
            Ok(preparation) => preparation,
            Err(error) => {
                yield PromptCompactionStreamEvent::Finished(Err(format!(
                    "Prompt compaction preparation failed: {error}"
                )));
                return;
            }
        };
        let bootstrap = bootstrap_runtime_stream(
            &request.foundation,
            request.luaskills_runtime.clone(),
            &launch_request,
            preparation,
        )
        .await;
        let launch = build_runtime_stream_launch_payload(&launch_request, bootstrap, None);

        let runtime_stream =
            drive_runtime_stream_launch(request.provider_configuration.clone(), launch, None);
        futures_util::pin_mut!(runtime_stream);
        while let Some(event) = runtime_stream.next().await {
            match event {
                ExecutionStreamEvent::Finished(report) => {
                    yield PromptCompactionStreamEvent::Finished(finalize_prompt_compaction_report(
                        &request.plan,
                        request.model_label.as_deref(),
                        report,
                    ));
                    return;
                }
                event => {
                    yield PromptCompactionStreamEvent::Stream(event);
                }
            }
        }

        yield PromptCompactionStreamEvent::Finished(Err(String::from(
            "Prompt compaction stream ended without a terminal execution report.",
        )));
    }
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
        return Ok(PromptCompactionExecution {
            compaction_result: CompactionResult::from_model_summary(plan, summary_text),
            fallback_used: false,
            provider_request_count: report.provider_request_count,
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
