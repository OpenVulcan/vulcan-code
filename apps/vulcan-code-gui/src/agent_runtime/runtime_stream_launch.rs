//! Runtime stream launch payload and launch-builder helpers for the outer execution shell.
//! 面向外层执行壳的运行时流启动载荷与启动构建辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
};
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::agent_runtime::runtime_stream_preparation::{
    RuntimePreparedLaunchInput, prepare_runtime_launch_input,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::model::SessionId;
use crate::provider_config::ProviderConfigurationFile;
use rig::message::Message as RigMessage;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Launch payload that captures everything the outer runtime stream needs after preparation and bootstrap have both succeeded.
/// 捕获准备阶段与启动阶段都成功后外层运行时流所需全部内容的启动载荷。
pub(crate) struct RuntimeStreamLaunch {
    /// Session identifier owned by the current runtime run.
    /// 当前运行时执行所属的会话标识。
    pub session_id: SessionId,
    /// Trimmed effective user prompt that will be sent to the provider transport layer.
    /// 将发送给 Provider 传输层的裁剪后生效用户提示词。
    pub effective_prompt: String,
    /// Updated execution preparation after request-snapshot seeding.
    /// 完成请求快照播种之后的更新执行准备对象。
    pub preparation: ExecutionPreparation,
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub active_agent: Option<RuntimeAgentDefinition>,
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
    pub request_history: Vec<RigMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub core_snapshot: CoreConversationSnapshot,
    /// Mutable streaming-loop state seeded from the bootstrapped core controller.
    /// 由已启动核心控制器播种得到的可变流式循环状态。
    pub stream_state: RuntimeStreamDeltaState,
}

/// Builds one fully prepared runtime-stream launch payload or one terminal preparation-failure report.
/// 构建一份完全就绪的运行时流启动载荷，或者生成一份终态准备失败报告。
pub(crate) async fn prepare_runtime_stream_launch(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let prepared_launch_input = prepare_runtime_launch_input(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        mcp_runtime,
        request,
    )?;
    build_runtime_stream_launch_from_prepared_input(prepared_launch_input).await
}

/// Assembles the final runtime-stream launch payload from one already prepared launch input.
/// 根据一份已经准备完成的启动输入组装最终运行时流启动载荷。
async fn build_runtime_stream_launch_from_prepared_input(
    prepared_launch_input: RuntimePreparedLaunchInput,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let RuntimePreparedLaunchInput {
        foundation,
        luaskills_runtime,
        mcp_runtime,
        request,
        preparation,
    } = prepared_launch_input;
    let bootstrap = bootstrap_runtime_stream(
        &foundation,
        luaskills_runtime,
        mcp_runtime,
        &request,
        preparation,
    )
    .await;

    Ok(RuntimeStreamLaunch {
        session_id: request.session_id,
        effective_prompt: request.prompt.trim().to_string(),
        preparation: bootstrap.preparation,
        active_agent: bootstrap.active_agent,
        tools_manager: bootstrap.tools_manager,
        resolved_tool_definitions: bootstrap.resolved_tool_definitions,
        discovery_events: bootstrap.discovery_events,
        discovery_rows: bootstrap.discovery_rows,
        discovery_errors: bootstrap.discovery_errors,
        warnings: bootstrap.warnings,
        system_preamble: bootstrap.system_preamble,
        request_history: bootstrap.request_history,
        core_snapshot: bootstrap.core_snapshot,
        stream_state: RuntimeStreamDeltaState::new(bootstrap.core_controller),
    })
}
