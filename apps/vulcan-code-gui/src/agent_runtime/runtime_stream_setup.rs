//! Runtime streaming bootstrap helpers for preparation, tool resolution, and request snapshot seeding.
//! 面向准备阶段、工具解析与请求快照播种的运行时流式启动辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::mcp_runtime_projection::collect_enabled_mcp_runtime_events;
use crate::agent_runtime::provider_request_bridge::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_snapshot::{
    build_core_controller_config, build_core_provider_request_snapshot,
    build_provider_request_preview_markdown,
};
use crate::agent_runtime::provider_runtime_context::{
    build_runtime_system_preamble, resolve_runtime_max_turns,
};
use crate::agent_runtime::provider_runtime_inputs::resolve_provider_additional_params;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::conversation_messages_to_rig_history;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::model::SessionId;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use rig::message::Message as RigMessage;
use std::path::PathBuf;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
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
    /// Host-owned controller that tracks streaming deltas and execution events.
    /// 负责跟踪流式增量与执行事件的宿主自有控制器。
    pub core_controller: VulcanCoreController,
}

/// Builds the full bootstrap payload required before provider streaming starts.
/// 构建 Provider 流式执行开始前所需的完整启动载荷。
pub(crate) async fn bootstrap_runtime_stream(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: &ExecutionPreparationRequest,
    mut preparation: ExecutionPreparation,
) -> RuntimeStreamBootstrap {
    let active_agent = foundation.active_agent_definition().cloned();
    let active_agent_tool_id = active_agent
        .as_ref()
        .map(|agent| agent.id.clone())
        .unwrap_or_else(|| preparation.agent_id.clone());
    let tools_manager = build_vulcan_tools_manager(
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        mcp_runtime.clone(),
        (!preparation.workspace_path.is_empty())
            .then(|| PathBuf::from(preparation.workspace_path.clone())),
        foundation.resolved_native_tools_for_agent(&active_agent_tool_id),
        foundation.resolved_luaskills_tools_for_agent(&active_agent_tool_id),
        foundation.resolved_mcp_tools_for_agent(&active_agent_tool_id),
    );
    let resolved_tools = tools_manager.resolve_for_execution_mode(&preparation.execution_mode);
    let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
        collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
    let mut warnings = preparation.warnings.clone();
    warnings.extend(discovery_warnings);

    let system_preamble = build_runtime_system_preamble(&preparation);
    let request_history =
        conversation_messages_to_rig_history(&request.conversation_state.messages);
    let core_snapshot = build_core_conversation_snapshot(&request.conversation_state);
    let core_controller = VulcanCoreController::new(
        build_core_controller_config(
            &preparation,
            &system_preamble,
            resolve_runtime_max_turns(active_agent.as_ref()),
            resolve_provider_additional_params(&preparation.model_blueprint),
        ),
        core_snapshot.clone(),
    );

    match build_core_provider_request_snapshot(
        &preparation,
        &request.conversation_state,
        &system_preamble,
        request.prompt.trim(),
        &resolved_tools.definitions,
        resolve_runtime_max_turns(active_agent.as_ref()),
    )
    .await
    {
        Ok(request_snapshot) => {
            preparation.provider_request_preview_markdown =
                build_provider_request_preview_markdown(&preparation, &request_snapshot);
            preparation.provider_request_snapshot_json =
                serde_json::to_value(&request_snapshot).ok();
        }
        Err(error) => {
            preparation.provider_request_preview_markdown =
                format!("# 最后一次真实 API 请求快照\n\n请求快照构建失败：{}", error);
            preparation.provider_request_snapshot_json = None;
            warnings.push(format!("请求快照构建失败：{error}"));
        }
    }

    RuntimeStreamBootstrap {
        preparation,
        active_agent,
        tools_manager,
        resolved_tool_definitions: resolved_tools.definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        warnings,
        system_preamble,
        request_history,
        core_snapshot,
        core_controller,
    }
}

/// Builds the standard runtime-context event emitted right after preparation succeeds.
/// 构建在准备成功后立即发出的标准运行时上下文事件。
pub(crate) fn build_runtime_context_event(preparation: &ExecutionPreparation) -> RuntimeEvent {
    RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "执行上下文",
        format!(
            "已进入真实执行阶段，目标模型为 `{}`，工作区为 `{}`。",
            preparation.model_blueprint.model_name, preparation.workspace_path
        ),
    )
}

/// Builds the initial execution event batch emitted immediately after bootstrap succeeds.
/// 构建在启动阶段成功后立即发出的初始执行事件批次。
pub(crate) fn build_runtime_start_events(
    session_id: SessionId,
    preparation: &ExecutionPreparation,
    discovery_events: Vec<RuntimeEvent>,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::with_capacity(discovery_events.len() + 2);
    events.push(ExecutionStreamEvent::Prepared(preparation.clone()));
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_runtime_context_event(preparation),
    });
    events.extend(
        discovery_events
            .into_iter()
            .map(|event| ExecutionStreamEvent::RuntimeEvent { session_id, event }),
    );
    events
}
