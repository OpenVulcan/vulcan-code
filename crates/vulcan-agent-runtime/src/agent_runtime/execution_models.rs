//! Shared execution-model types reused by the executor, runtime events, and module exports.
//! 供执行器、运行时事件与模块导出复用的共享执行模型类型。

use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::context_budget::ContextBudgetSnapshot;
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use crate::provider_config::{PersistedAiToolConfiguration, ReasoningEffortLevel};
use crate::provider_config::{
    PersistedPromptConfiguration, PersistedPromptPersonalitySelection,
    ProjectPromptSkillsConfiguration,
};
use serde_json::Value;
use std::path::PathBuf;
use vulcan_context::{ContextAssembly, ContextBaseline, ContextRequestMode};
use vulcan_core::types::{
    ConversationAttachment, CoreEvent as CoreControllerEvent,
    ProviderContextInjection as CoreProviderContextInjection, ProviderUsageSnapshot,
};
use vulcan_hooks::SystemPromptHookBlock;
use vulcan_luaskills_prompt::LuaSkillPromptEntry;
use vulcan_prompt_skills::ResolvedPromptSkill;
use vulcan_tools_manager::{VulcanToolDescriptor, VulcanToolSourceKind};
use vulcan_vmm::VmmConfigLayer;

/// Minimal execution request captured from the current GUI state.
/// 从当前 GUI 状态捕获的最小执行请求。
#[derive(Debug, Clone)]
pub struct ExecutionPreparationRequest {
    /// Target session identifier.
    /// 目标会话标识。
    pub session_id: SessionId,
    /// Target session title.
    /// 目标会话标题。
    pub session_title: String,
    /// Current workspace path.
    /// 当前工作区路径。
    pub workspace_path: String,
    /// Current composer prompt.
    /// 当前提示词输入区内容。
    pub prompt: String,
    /// Canonical conversation state reconstructed from the session protocol state.
    /// 从会话协议状态重建出的规范会话状态。
    pub conversation_state: SessionConversationState,
    /// Current execution-mode label.
    /// 当前执行模式标签。
    pub execution_mode: String,
    /// Optional sampling temperature override from the run-time controls.
    /// 来自运行参数控制区的可选采样温度覆盖值。
    pub temperature: Option<f32>,
    /// Shared reasoning-effort preset selected by the current workbench run controls.
    /// 当前工作台运行控制区选择的共享推理档位预设。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Effective AI-tool configuration snapshot locked for the current run.
    /// 为当前运行锁定的生效 AI 工具配置快照。
    pub ai_tool_configuration: PersistedAiToolConfiguration,
    /// Prompt purpose used to choose runtime prompt assembly and tool exposure.
    /// 用于选择运行时提示词装配与工具暴露策略的提示词用途。
    pub prompt_purpose: ExecutionPromptPurpose,
    /// Project-level prompt-skill overrides captured for the current workspace.
    /// 为当前工作区捕获的项目级提示词技能覆盖。
    pub project_prompt_skills: ProjectPromptSkillsConfiguration,
    /// Session-level prompt-skill overrides captured for the current conversation.
    /// 为当前会话捕获的会话级提示词技能覆盖。
    pub session_prompt_skill_overrides:
        std::collections::BTreeMap<String, crate::provider_config::PersistedPromptSkillState>,
    /// Provider-facing context-injection overlay captured for the current run.
    /// 为当前运行捕获的 Provider 可见上下文注入覆盖层。
    pub context_injection: Option<CoreProviderContextInjection>,
    /// Request-scoped system-prompt hook blocks captured for the current run.
    /// 为当前运行捕获的请求级系统提示词 hook 块。
    pub system_prompt_hook_blocks: Vec<SystemPromptHookBlock>,
    /// Project-level VMM configuration captured for the current workspace.
    /// 为当前工作区捕获的项目级 VMM 配置。
    pub project_vmm: VmmConfigLayer,
    /// Session-level VMM configuration captured for the current conversation.
    /// 为当前会话捕获的会话级 VMM 配置。
    pub session_vmm: VmmConfigLayer,
}

/// Runtime purpose attached to one provider-facing prompt execution.
/// 附着在一次面向 Provider 的提示词执行上的运行时用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPromptPurpose {
    /// Normal user-authored conversation turn.
    /// 普通用户发起的会话回合。
    UserTurn,
    /// Internal context-compaction summary request.
    /// 内部上下文压缩摘要请求。
    ContextCompaction,
}

impl Default for ExecutionPromptPurpose {
    /// Returns the default prompt purpose used by normal conversation turns.
    /// 返回普通会话回合使用的默认提示词用途。
    fn default() -> Self {
        Self::UserTurn
    }
}

impl ExecutionPromptPurpose {
    /// Returns whether this purpose must hide provider-visible tools.
    /// 返回该用途是否必须隐藏 Provider 可见工具。
    pub const fn disables_provider_tools(self) -> bool {
        matches!(self, Self::ContextCompaction)
    }

    /// Returns the maximum provider-turn override required by this purpose.
    /// 返回该用途要求的最大 Provider 回合数覆盖值。
    pub const fn max_turns_override(self) -> Option<usize> {
        match self {
            Self::UserTurn => None,
            Self::ContextCompaction => Some(1),
        }
    }

    /// Returns a compact diagnostics label for this prompt purpose.
    /// 返回该提示词用途的紧凑诊断标签。
    pub const fn diagnostics_label(self) -> &'static str {
        match self {
            Self::UserTurn => "user-turn",
            Self::ContextCompaction => "context-compaction",
        }
    }
}

/// Prepared execution plan rendered by the GUI before real LLM execution begins.
/// 在真实 LLM 执行开始前由 GUI 渲染的执行准备计划。
#[derive(Debug, Clone)]
pub struct ExecutionPreparation {
    /// Active session identifier.
    /// 当前激活会话标识。
    pub session_id: SessionId,
    /// Active session title.
    /// 当前激活会话标题。
    pub session_title: String,
    /// Active agent identifier.
    /// 当前激活代理标识。
    pub agent_id: String,
    /// Active agent display name.
    /// 当前激活代理显示名称。
    pub agent_name: String,
    /// Active agent kind label.
    /// 当前激活代理类型标签。
    pub agent_kind: String,
    /// Validated provider/model runtime blueprint.
    /// 通过校验的 Provider/Model 运行时蓝图。
    pub model_blueprint: ProviderRuntimeBlueprint,
    /// Prompt preview shortened for activity logs.
    /// 为活动日志截断后的提示词预览。
    pub prompt_preview: String,
    /// Workspace path captured for the run.
    /// 为本次运行捕获的工作区路径。
    pub workspace_path: String,
    /// Latest host-visible attachment catalog captured from the canonical conversation window for this run.
    /// 为本次运行从规范会话窗口捕获的最新宿主可见附件目录。
    pub conversation_attachments: Vec<ConversationAttachment>,
    /// Number of history rows that will be forwarded into the real model call.
    /// 将被转发到真实模型调用中的历史行数量。
    pub context_history_count: usize,
    /// Execution-mode label captured for the run.
    /// 为本次运行捕获的执行模式标签。
    pub execution_mode: String,
    /// Optional sampling temperature captured for the run.
    /// 为本次运行捕获的可选采样温度。
    pub temperature: Option<f32>,
    /// Prompt-composer configuration captured for this run.
    /// 为本次运行捕获的提示词组合配置。
    pub prompt_configuration: PersistedPromptConfiguration,
    /// Prompt purpose captured for this run.
    /// 为本次运行捕获的提示词用途。
    pub prompt_purpose: ExecutionPromptPurpose,
    /// Prompt skills resolved for this run after global/project/session overrides.
    /// 为本次运行按全局、项目与会话覆盖解析出的提示词技能集合。
    pub prompt_skills: Vec<ResolvedPromptSkill>,
    /// User prompt-skill root captured for this run.
    /// 为本次运行捕获的用户提示词技能根目录。
    pub prompt_skill_user_root: Option<PathBuf>,
    /// Prompt entries for LuaSkills enabled through implicit host dispatch in this run.
    /// 本次运行中通过隐式宿主调度启用的 LuaSkills 提示词入口集合。
    pub implicit_luaskill_prompt_entries: Vec<LuaSkillPromptEntry>,
    /// Optional developer prompt segment advertising the enabled prompt skills.
    /// 向模型说明已启用提示词技能与隐式 LuaSkills 的可选开发者提示词片段。
    pub prompt_skill_developer_segment: Option<String>,
    /// Resolved skill names attached to the active agent.
    /// 附着在当前代理上的技能名称集合。
    pub skill_names: Vec<String>,
    /// Unified AI-visible tool descriptors resolved for the active mode.
    /// 为当前模式解析出的统一 AI 可见工具描述集合。
    pub tool_descriptors: Vec<VulcanToolDescriptor>,
    /// Delegate agent identifiers exposed to the active agent.
    /// 暴露给当前代理的委派代理标识集合。
    pub delegate_agents: Vec<String>,
    /// Enabled MCP server summaries visible to the runtime foundation.
    /// 对运行时基础架构可见的已启用 MCP 服务器摘要集合。
    pub enabled_mcp_servers: Vec<String>,
    /// Non-fatal preparation warnings.
    /// 非致命的准备阶段警告集合。
    pub warnings: Vec<String>,
    /// Full provider-request preview captured right before the real API call starts.
    /// 在真实 API 调用开始前捕获的完整 Provider 请求预览。
    pub provider_request_preview_markdown: String,
    /// Structured provider-request snapshot JSON captured by `vulcan-core` right before dispatch.
    /// 由 `vulcan-core` 在分发前捕获的结构化 Provider 请求快照 JSON。
    pub provider_request_snapshot_json: Option<Value>,
    /// Final provider-specific HTTP request JSON captured immediately before network dispatch.
    /// 在网络发送前捕获到的最终 Provider 专用 HTTP 请求 JSON。
    pub provider_wire_request_json: Option<Value>,
    /// Optional context-budget snapshot captured from the final provider-facing request shape.
    /// 从最终面向 Provider 的请求形态捕获到的可选上下文预算快照。
    pub context_budget: Option<ContextBudgetSnapshot>,
    /// Optional layered context assembly captured from the final provider-facing request shape.
    /// 从最终面向 Provider 的请求形态捕获到的可选分层上下文装配结果。
    pub context_assembly: Option<ContextAssembly>,
    /// Optional compact baseline snapshot derived from the current layered context assembly.
    /// 从当前分层上下文装配结果派生出的可选紧凑基线快照。
    pub context_baseline: Option<ContextBaseline>,
    /// Optional context request send-mode diagnostic captured from the final request shape.
    /// 从最终请求形态捕获的可选上下文请求发送模式诊断。
    pub context_request_mode: Option<ContextRequestMode>,
    /// Provider-facing context-injection overlay captured for the current run.
    /// 为当前运行捕获的 Provider 可见上下文注入覆盖层。
    pub context_injection: Option<CoreProviderContextInjection>,
    /// Request-scoped system-prompt hook blocks captured for the current run.
    /// 为当前运行捕获的请求级系统提示词 hook 块。
    pub system_prompt_hook_blocks: Vec<SystemPromptHookBlock>,
    /// Effective project-level VMM configuration captured for this run.
    /// 为本次运行捕获的有效项目级 VMM 配置。
    pub project_vmm: VmmConfigLayer,
    /// Effective session-level VMM configuration captured for this run.
    /// 为本次运行捕获的有效会话级 VMM 配置。
    pub session_vmm: VmmConfigLayer,
}

/// Convert a persisted personality selection into prompt-composer input.
/// 将持久化人格选择转换为提示词组合器输入。
pub(crate) fn persisted_personality_to_prompt_selection(
    selection: &PersistedPromptPersonalitySelection,
) -> vulcan_prompt_composer::PersonalitySelection {
    match selection {
        PersistedPromptPersonalitySelection::Default => {
            vulcan_prompt_composer::PersonalitySelection::Default
        }
        PersistedPromptPersonalitySelection::Named(id) => {
            vulcan_prompt_composer::PersonalitySelection::Named(id.clone())
        }
        PersistedPromptPersonalitySelection::Disabled => {
            vulcan_prompt_composer::PersonalitySelection::Disabled
        }
    }
}

impl ExecutionPreparation {
    /// Returns a compact list of human-readable summary rows for UI cards and logs.
    /// 返回供界面卡片与日志使用的紧凑人类可读摘要行列表。
    pub fn summary_rows(&self) -> Vec<String> {
        let mut rows = vec![
            format!("会话：#{} · {}", self.session_id, self.session_title),
            format!(
                "代理：{}（{}，id={})",
                self.agent_name, self.agent_kind, self.agent_id
            ),
            format!(
                "模型：{} / {} · 运行时 {}",
                self.model_blueprint.provider_name,
                self.model_blueprint.model_name,
                self.model_blueprint.runtime_label
            ),
            format!(
                "路由：{}/{} · kind={}",
                self.model_blueprint.provider_id,
                self.model_blueprint.model_id,
                self.model_blueprint.runtime_kind_key()
            ),
            format!(
                "接口族：{} · family={}",
                self.model_blueprint.api_family_label(),
                self.model_blueprint.api_family_key()
            ),
            format!("接口：{}", self.model_blueprint.endpoint_summary()),
            format!("凭据：{}", self.model_blueprint.credential_source),
            format!(
                "生命周期：{}",
                self.model_blueprint.capabilities.lifecycle_summary()
            ),
            format!(
                "模型发现：{}",
                if self.model_blueprint.supports_remote_model_refresh {
                    "支持远端刷新"
                } else {
                    "使用静态目录"
                }
            ),
            format!(
                "模式：{} · 温度 {}",
                self.execution_mode,
                self.temperature
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| String::from("使用模型默认值"))
            ),
            self.model_blueprint.reasoning.summary_label(),
            format!("工作区：{}", self.workspace_path),
            format!("上下文：{} 条历史消息", self.context_history_count),
            format!(
                "能力：工具调用 {} · 温度覆盖 {}",
                if self.model_blueprint.supports_tool_call() {
                    "支持"
                } else {
                    "不支持"
                },
                if self.model_blueprint.supports_temperature() {
                    "支持"
                } else {
                    "不支持"
                }
            ),
            format!(
                "模态：{} · attachment 声明 {}",
                if self.model_blueprint.capabilities.is_text_only() {
                    String::from("text")
                } else {
                    self.model_blueprint.capabilities.input_modalities.join("/")
                },
                if self.model_blueprint.capabilities.supports_attachment {
                    "已声明"
                } else {
                    "未声明"
                }
            ),
        ];
        if !self.model_blueprint.capabilities.ai_tools.is_empty() {
            rows.push(format!(
                "AI 专用工具：{}",
                self.model_blueprint
                    .capabilities
                    .ai_tools
                    .iter()
                    .map(|tool| format!("{}({:?})", tool.kind.key(), tool.policy))
                    .collect::<Vec<_>>()
                    .join("、")
            ));
        }

        if let Some(context_budget) = self.context_budget.as_ref() {
            rows.push(format!("预算：{}", context_budget.summary_note()));
            rows.push(format!("预算指纹：{}", context_budget.fingerprint));
        }

        if let Some(context_assembly) = self.context_assembly.as_ref() {
            rows.push(format!("上下文层：{}", context_assembly.summary_note()));
        }

        if let Some(context_baseline) = self.context_baseline.as_ref() {
            rows.push(format!(
                "上下文基线：{} 层 · {}",
                context_baseline.layer_count, context_baseline.fingerprint
            ));
        }

        if let Some(context_request_mode) = self.context_request_mode.as_ref() {
            rows.push(format!(
                "上下文发送：{}",
                context_request_mode.summary_note()
            ));
        }

        if !self.skill_names.is_empty() {
            rows.push(format!("技能：{}", self.skill_names.join("、")));
        }

        let enabled_prompt_skills = self
            .prompt_skills
            .iter()
            .filter(|skill| skill.is_enabled())
            .map(|skill| skill.skill.name.as_str())
            .collect::<Vec<_>>();
        if !enabled_prompt_skills.is_empty() {
            rows.push(format!("提示词技能：{}", enabled_prompt_skills.join("、")));
        }

        if !self.tool_descriptors.is_empty() {
            rows.push(format!(
                "工具：{} 个已直接暴露",
                self.tool_descriptors.len()
            ));
        }

        let native_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Native)
            .count();
        if native_tool_count > 0 {
            rows.push(format!("原生工具：{} 个", native_tool_count));
        }

        let luaskills_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::LuaSkills)
            .count();
        if luaskills_tool_count > 0 {
            rows.push(format!("LuaSkills 工具：{} 个", luaskills_tool_count));
        }

        let mcp_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Mcp)
            .count();
        if mcp_tool_count > 0 {
            rows.push(format!("MCP 工具：{} 个", mcp_tool_count));
        }

        let ai_fallback_tool_count = self
            .tool_descriptors
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::AiFallback)
            .count();
        if ai_fallback_tool_count > 0 {
            rows.push(format!("AI 专用回退工具：{} 个", ai_fallback_tool_count));
        }

        if !self.delegate_agents.is_empty() {
            rows.push(format!("委派：{}", self.delegate_agents.join("、")));
        }

        if !self.enabled_mcp_servers.is_empty() {
            rows.push(format!("MCP：{}", self.enabled_mcp_servers.join("；")));
        }

        rows.push(format!("提示词：{}", self.prompt_preview));
        rows
    }
}

/// Real execution report returned by the async provider runtime after one run attempt.
/// 异步 Provider 运行时在单次执行后返回的真实执行报告。
#[derive(Debug, Clone)]
pub struct ExecutionRunReport {
    /// Session identifier owning the run.
    /// 拥有本次运行的会话标识。
    pub session_id: SessionId,
    /// Prepared execution snapshot when preparation succeeded.
    /// 当准备阶段成功时生成的执行准备快照。
    pub preparation: Option<ExecutionPreparation>,
    /// Assistant output returned by the real provider call.
    /// 真实 Provider 调用返回的助手输出。
    pub assistant_output: Option<String>,
    /// Assistant thinking output returned by the real provider call.
    /// 真实 Provider 调用返回的助手思考输出。
    pub thinking_output: Option<String>,
    /// Canonical conversation state returned from the last safe host-controlled controller boundary.
    /// 从最后一个安全的宿主控制器边界返回的规范会话状态。
    pub conversation_state: Option<SessionConversationState>,
    /// Number of real provider requests dispatched while this run was active.
    /// 本次运行期间真实发出的 Provider 请求次数。
    pub provider_request_count: usize,
    /// Latest successful provider usage captured by this run, excluding failed or aborted calls.
    /// 本次运行捕获到的最近一次成功 Provider 用量，不包含失败或中止调用。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    /// Successful MCP discovery summary rows collected during the run.
    /// 运行期间收集到的 MCP 成功发现摘要行集合。
    pub mcp_discovery_rows: Vec<String>,
    /// Failed MCP discovery summary rows collected during the run.
    /// 运行期间收集到的 MCP 失败发现摘要行集合。
    pub mcp_discovery_errors: Vec<String>,
    /// Non-fatal warnings collected during the run.
    /// 运行期间收集到的非致命警告集合。
    pub warnings: Vec<String>,
    /// Terminal error when the run failed.
    /// 当运行失败时的最终错误信息。
    pub error: Option<String>,
    /// Controller-owned event trace captured while the run was executing.
    /// 运行执行期间捕获的控制器自有事件轨迹。
    pub controller_events: Vec<CoreControllerEvent>,
}

impl ExecutionRunReport {
    /// Creates one success report.
    /// 创建一份成功执行报告。
    pub(crate) fn success(
        session_id: SessionId,
        preparation: ExecutionPreparation,
        thinking_output: Option<String>,
        assistant_output: String,
        conversation_state: Option<SessionConversationState>,
        provider_request_count: usize,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        controller_events: Vec<CoreControllerEvent>,
        last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    ) -> Self {
        Self {
            session_id,
            preparation: Some(preparation),
            assistant_output: Some(assistant_output),
            thinking_output,
            conversation_state,
            provider_request_count,
            last_successful_provider_usage,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: None,
            controller_events,
        }
    }

    /// Creates one failure report.
    /// 创建一份失败执行报告。
    pub(crate) fn failure(
        session_id: SessionId,
        preparation: Option<ExecutionPreparation>,
        provider_request_count: usize,
        last_successful_provider_usage: Option<ProviderUsageSnapshot>,
        mcp_discovery_rows: Vec<String>,
        mcp_discovery_errors: Vec<String>,
        warnings: Vec<String>,
        controller_events: Vec<CoreControllerEvent>,
        conversation_state: Option<SessionConversationState>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            preparation,
            assistant_output: None,
            thinking_output: None,
            conversation_state,
            provider_request_count,
            last_successful_provider_usage,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error: Some(error.into()),
            controller_events,
        }
    }

    /// Returns whether the current report completed successfully.
    /// 返回当前报告是否成功完成。
    pub const fn is_success(&self) -> bool {
        self.error.is_none()
    }
}
