//! Prompt-based session compaction executor reused by manual and automatic compaction entrypoints.
//! 供手动与自动压缩入口复用的提示词式会话压缩执行器。

use super::model::PendingPromptModelReference;
use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::runtime_stream_driver::drive_runtime_stream_launch;
use crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::conversation_protocol::conversation_state_from_session_messages;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeAccess;
use crate::provider_config::{PersistedAiToolConfiguration, ProviderConfigurationFile, ReasoningEffortLevel};
use crate::skills::session::{
    CompactionPlan, CompactionSkillResult, build_compaction_request_prompt,
};
use crate::tooling::CommandSessionRuntime;
use futures_util::StreamExt;
use std::sync::Arc;

/// Static system-prompt suffix that forces compaction requests into a plain-text, no-tools summarization mode.
/// 强制压缩请求进入纯文本无工具总结模式的固定系统提示词后缀。
const PROMPT_COMPACTION_SYSTEM_PREAMBLE_SUFFIX: &str = "\n\
\n\
Internal host task: summarize earlier conversation context for continuation.\n\
- Do not call tools.\n\
- Do not delegate.\n\
- Do not ask questions.\n\
- Return plain text only.";

/// Immutable request payload required to execute one prompt-based compaction job.
/// 执行单次提示词式压缩任务所需的不可变请求载荷。
#[derive(Clone)]
pub(super) struct PromptCompactionJobRequest {
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
    /// Optional locked provider/model reference kept only for diagnostics.
    /// 仅供诊断使用的可选锁定供应商/模型引用。
    pub model_reference: Option<PendingPromptModelReference>,
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
}

/// Terminal result returned by one prompt-based compaction job after the internal summary request settles.
/// 单次提示词式压缩任务在内部总结请求收敛后返回的终态结果。
#[derive(Debug, Clone)]
pub(super) struct PromptCompactionExecution {
    /// Final prompt-shaped compaction result ready to be applied to the session state.
    /// 准备好应用到会话状态的最终提示词形态压缩结果。
    pub compaction_result: CompactionSkillResult,
    /// Whether the model request failed and the legacy host-side summary had to be used as a fallback.
    /// 是否因模型请求失败而不得不使用旧版宿主侧总结作为回退。
    pub fallback_used: bool,
    /// Number of provider requests dispatched while producing the summary.
    /// 产出该总结期间发出的 Provider 请求次数。
    pub provider_request_count: usize,
}

/// Executes one prompt-based compaction job and falls back to the legacy host-side summary only when the internal model request cannot complete successfully.
/// 执行单次提示词式压缩任务，并且仅在内部模型请求无法成功完成时回退到旧版宿主侧总结。
pub(super) async fn execute_prompt_compaction_job(
    request: PromptCompactionJobRequest,
) -> Result<PromptCompactionExecution, String> {
    let source_conversation_state = conversation_state_from_session_messages(&request.plan.source_messages);
    let launch_request = ExecutionPreparationRequest {
        session_id: request.session_id,
        session_title: request.session_title,
        workspace_path: request.workspace_path,
        prompt: build_compaction_request_prompt(),
        conversation_state: source_conversation_state,
        execution_mode: String::from("Compaction"),
        temperature: None,
        reasoning_effort: request.reasoning_effort,
        ai_tool_configuration: request.ai_tool_configuration,
    };

    let launch = prepare_runtime_stream_launch(
        &request.provider_configuration,
        &request.agent_profile_catalog,
        request.luaskills_runtime,
        request.mcp_runtime_access,
        request.task_runtime,
        request.question_runtime,
        request.command_session_runtime,
        launch_request,
    )
    .await;

    let mut launch = match launch {
        Ok(launch) => launch,
        Err(report) => {
            return Ok(fallback_execution_from_report(&request.plan, &report));
        }
    };

    launch.system_preamble = build_prompt_compaction_system_preamble(&launch.system_preamble);
    launch.resolved_tool_definitions.clear();
    launch.preparation.tool_descriptors.clear();
    if let Some(active_agent) = launch.active_agent.as_mut() {
        active_agent.max_turns = Some(1);
    }

    let runtime_stream = drive_runtime_stream_launch(request.provider_configuration, launch);
    futures_util::pin_mut!(runtime_stream);
    while let Some(event) = runtime_stream.next().await {
        if let crate::agent_runtime::ExecutionStreamEvent::Finished(report) = event {
            return finalize_prompt_compaction_report(&request.plan, request.model_reference.as_ref(), report);
        }
    }

    Ok(PromptCompactionExecution {
        compaction_result: CompactionSkillResult::from_legacy_plan(&request.plan),
        fallback_used: true,
        provider_request_count: 0,
    })
}

/// Builds the effective system preamble for one internal prompt-compaction request by extending the normal runtime preamble with no-tools summarization instructions.
/// 通过在常规运行时前导语后附加无工具总结指令，为内部提示词压缩请求构建生效系统前导语。
fn build_prompt_compaction_system_preamble(base_preamble: &str) -> String {
    let trimmed = base_preamble.trim();
    if trimmed.is_empty() {
        PROMPT_COMPACTION_SYSTEM_PREAMBLE_SUFFIX.trim().to_string()
    } else {
        format!("{trimmed}{PROMPT_COMPACTION_SYSTEM_PREAMBLE_SUFFIX}")
    }
}

/// Finalizes one terminal execution report returned by the internal summary run into the prompt-based compaction result consumed by the workbench.
/// 将内部总结运行返回的终态执行报告整理为工作台消费的提示词式压缩结果。
fn finalize_prompt_compaction_report(
    plan: &CompactionPlan,
    model_reference: Option<&PendingPromptModelReference>,
    report: ExecutionRunReport,
) -> Result<PromptCompactionExecution, String> {
    if report.is_success() {
        let summary_text = report
            .assistant_output
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| build_prompt_compaction_failure_message(model_reference, &report))?;
        return Ok(PromptCompactionExecution {
            compaction_result: CompactionSkillResult::from_model_summary(plan, summary_text),
            fallback_used: false,
            provider_request_count: report.provider_request_count,
        });
    }

    Ok(fallback_execution_from_report(plan, &report))
}

/// Builds one fallback prompt-compaction execution from a failed model report while preserving the report's provider request count for diagnostics.
/// 根据失败的模型报告构建一份回退提示词压缩执行结果，并保留报告中的 Provider 请求次数用于诊断。
fn fallback_execution_from_report(
    plan: &CompactionPlan,
    report: &ExecutionRunReport,
) -> PromptCompactionExecution {
    PromptCompactionExecution {
        compaction_result: CompactionSkillResult::from_legacy_plan(plan),
        fallback_used: true,
        provider_request_count: report.provider_request_count,
    }
}

/// Builds one human-readable failure message for prompt-based compaction when even the successful report did not contain usable plain-text summary output.
/// 当提示词式压缩即便拿到了成功报告却仍未包含可用纯文本总结时，构建一条人类可读失败信息。
fn build_prompt_compaction_failure_message(
    model_reference: Option<&PendingPromptModelReference>,
    report: &ExecutionRunReport,
) -> String {
    let model_label = model_reference
        .map(|reference| format!("{}/{}", reference.provider_id, reference.model_id))
        .unwrap_or_else(|| String::from("unknown-model"));
    let report_error = report
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("the model did not return a usable plain-text summary");
    format!("Prompt compaction on {model_label} did not produce a usable summary: {report_error}.")
}
