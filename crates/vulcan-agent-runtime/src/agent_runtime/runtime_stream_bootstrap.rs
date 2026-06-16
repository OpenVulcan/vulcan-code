//! Runtime stream bootstrap helpers for provider-visible history, tool resolution, and request-snapshot seeding.
//! 面向 Provider 可见历史、工具解析与请求快照播种的运行时流式 bootstrap 辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::execution_preparation::PreparedToolSnapshot;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap_core::{
    RuntimeBootstrapCore, build_runtime_bootstrap_core, seed_runtime_request_snapshot,
};
use crate::agent_runtime::runtime_stream_bootstrap_resources::{
    RuntimeBootstrapResources, resolve_runtime_bootstrap_resources,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use std::sync::Arc;
use vulcan_context::RuntimeProviderLoweringRepairEvent;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Bootstrap payload that captures the fully prepared runtime turn before provider streaming begins.
/// 在 Provider 流式执行开始前捕获完整运行时回合准备结果的启动载荷。
pub(crate) struct RuntimeStreamBootstrap {
    /// Updated execution preparation after request-snapshot seeding.
    /// 完成请求快照播种之后的更新执行准备对象。
    pub preparation: ExecutionPreparation,
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub active_agent: Option<RuntimeAgentDefinition>,
    /// Shared resumable task runtime reused by the current execution turn.
    /// 被当前执行回合复用的共享可恢复任务运行时。
    pub task_runtime: Arc<TaskRuntime>,
    /// Shared token-budget runtime initialized during host startup.
    /// 宿主启动期间初始化的共享 Token 预算运行时。
    pub token_budget_runtime: Arc<TokenBudgetRuntime>,
    /// Host-owned unified tool manager used by the transport layer.
    /// 供传输层使用的宿主自有统一工具管理器。
    pub tools_manager: VulcanToolsManager,
    /// Final resolved tool definitions exposed to the provider for this turn.
    /// 当前回合向 Provider 暴露的最终已解析工具定义集合。
    pub resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    /// Discovery events projected from enabled MCP runtime servers.
    /// 从已启用 MCP 运行时服务器投影得到的发现事件集合。
    pub discovery_events: Vec<RuntimeEvent>,
    /// Human-readable MCP discovery rows captured for the final execution report.
    /// 为最终执行报告捕获的人类可读 MCP 发现摘要行集合。
    pub discovery_rows: Vec<String>,
    /// MCP discovery errors captured for the final execution report.
    /// 为最终执行报告捕获的 MCP 发现错误集合。
    pub discovery_errors: Vec<String>,
    /// Mutable warning list accumulated before provider streaming begins.
    /// 在 Provider 流式执行开始前累计得到的可变警告列表。
    pub warnings: Vec<String>,
    /// Effective system preamble sent to the provider transport layer.
    /// 发送给 Provider 传输层的生效系统前导语。
    pub system_preamble: String,
    /// Provider-visible history reconstructed from canonical conversation messages.
    /// 从规范会话消息重建得到的 Provider 可见历史记录。
    pub request_history: Vec<CoreConversationMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub core_snapshot: CoreConversationSnapshot,
    /// Host-owned controller that tracks streaming deltas and execution events.
    /// 负责跟踪流式增量与执行事件的宿主自有控制器。
    pub core_controller: VulcanCoreController,
    /// Provider lowering repair events detected before streaming starts.
    /// 流式执行开始前检测到的 Provider 转换修复事件。
    pub lowering_repair_events: Vec<RuntimeProviderLoweringRepairEvent>,
}

/// Builds the full bootstrap payload required before provider streaming starts.
/// 构建 Provider 流式执行开始前所需的完整启动载荷。
pub(crate) async fn bootstrap_runtime_stream(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    request: &ExecutionPreparationRequest,
    mut preparation: ExecutionPreparation,
    tool_snapshot: PreparedToolSnapshot,
) -> Result<RuntimeStreamBootstrap, String> {
    let RuntimeBootstrapResources {
        active_agent,
        task_runtime,
        tools_manager,
        resolved_tool_definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        mut warnings,
    } = resolve_runtime_bootstrap_resources(
        foundation,
        luaskills_runtime,
        request,
        &preparation,
        tool_snapshot,
    );
    let RuntimeBootstrapCore {
        system_preamble,
        prompt_layers,
        request_history,
        core_snapshot,
        core_controller,
        lowering_repair_events,
    } = build_runtime_bootstrap_core(&preparation, request, active_agent.as_ref())?;
    let token_budget_runtime = foundation.token_budget_runtime();
    let prompt_purpose = preparation.prompt_purpose;
    seed_runtime_request_snapshot(
        &mut preparation,
        request,
        &system_preamble,
        prompt_layers,
        &resolved_tool_definitions,
        token_budget_runtime.estimator(),
        crate::agent_runtime::provider_runtime_context::resolve_runtime_max_turns(
            active_agent.as_ref(),
            prompt_purpose,
        ),
        &mut warnings,
    )
    .await;

    Ok(RuntimeStreamBootstrap {
        preparation,
        active_agent,
        task_runtime,
        token_budget_runtime,
        tools_manager,
        resolved_tool_definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        warnings,
        system_preamble,
        request_history,
        core_snapshot,
        core_controller,
        lowering_repair_events,
    })
}
