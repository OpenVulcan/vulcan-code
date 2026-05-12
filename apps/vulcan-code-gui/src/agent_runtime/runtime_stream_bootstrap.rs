//! Runtime stream bootstrap helpers for provider-visible history, tool resolution, and request-snapshot seeding.
//! 面向 Provider 可见历史、工具解析与请求快照播种的运行时流式 bootstrap 辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
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

/// Resolved runtime bootstrap resources collected before request-snapshot seeding begins.
/// 在请求快照播种开始之前收集完成的运行时 bootstrap 资源集合。
struct RuntimeBootstrapResources {
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    active_agent: Option<RuntimeAgentDefinition>,
    /// Host-owned unified tool manager used by the transport layer.
    /// 供传输层使用的宿主自有统一工具管理器。
    tools_manager: VulcanToolsManager,
    /// Final resolved tool definitions exposed to the provider for this turn.
    /// 当前回合向 Provider 暴露的最终已解析工具定义集合。
    resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    /// Discovery events projected from enabled MCP runtime servers.
    /// 从已启用 MCP 运行时服务器投影得到的发现事件集合。
    discovery_events: Vec<RuntimeEvent>,
    /// Human-readable MCP discovery rows captured for the final execution report.
    /// 为最终执行报告捕获的人类可读 MCP 发现摘要行集合。
    discovery_rows: Vec<String>,
    /// MCP discovery errors captured for the final execution report.
    /// 为最终执行报告捕获的 MCP 发现错误集合。
    discovery_errors: Vec<String>,
    /// Mutable warning list accumulated before provider streaming begins.
    /// 在 Provider 流式执行开始前累计得到的可变警告列表。
    warnings: Vec<String>,
}

/// Core provider-facing runtime bootstrap state derived from canonical conversation inputs.
/// 从规范会话输入推导得到的面向 Provider 的运行时 bootstrap 核心状态。
struct RuntimeBootstrapCore {
    /// Effective system preamble sent to the provider transport layer.
    /// 发送给 Provider 传输层的生效系统前导语。
    system_preamble: String,
    /// Provider-visible history reconstructed from canonical conversation messages.
    /// 从规范会话消息重建得到的 Provider 可见历史记录。
    request_history: Vec<RigMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    core_snapshot: CoreConversationSnapshot,
    /// Host-owned controller that tracks streaming deltas and execution events.
    /// 负责跟踪流式增量与执行事件的宿主自有控制器。
    core_controller: VulcanCoreController,
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
    let RuntimeBootstrapResources {
        active_agent,
        tools_manager,
        resolved_tool_definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        mut warnings,
    } = resolve_runtime_bootstrap_resources(
        foundation,
        luaskills_runtime,
        mcp_runtime.clone(),
        &preparation,
    );
    let RuntimeBootstrapCore {
        system_preamble,
        request_history,
        core_snapshot,
        core_controller,
    } = build_runtime_bootstrap_core(&preparation, request, active_agent.as_ref());
    seed_runtime_request_snapshot(
        &mut preparation,
        request,
        &system_preamble,
        &resolved_tool_definitions,
        resolve_runtime_max_turns(active_agent.as_ref()),
        &mut warnings,
    )
    .await;

    RuntimeStreamBootstrap {
        preparation,
        active_agent,
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
    }
}

/// Resolves agent-scoped tools and MCP discovery outputs before provider-facing bootstrap state is derived.
/// 在推导面向 Provider 的 bootstrap 状态之前，先解析代理作用域工具与 MCP 发现结果。
fn resolve_runtime_bootstrap_resources(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    preparation: &ExecutionPreparation,
) -> RuntimeBootstrapResources {
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
    let resolved_tool_definitions = tools_manager
        .resolve_for_execution_mode(&preparation.execution_mode)
        .definitions;
    let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
        collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
    let mut warnings = preparation.warnings.clone();
    warnings.extend(discovery_warnings);

    RuntimeBootstrapResources {
        active_agent,
        tools_manager,
        resolved_tool_definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        warnings,
    }
}

/// Builds the provider-facing runtime bootstrap core from the canonical conversation state and execution preparation.
/// 根据规范会话状态与执行准备构建面向 Provider 的运行时 bootstrap 核心对象。
fn build_runtime_bootstrap_core(
    preparation: &ExecutionPreparation,
    request: &ExecutionPreparationRequest,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> RuntimeBootstrapCore {
    let system_preamble = build_runtime_system_preamble(preparation);
    let request_history =
        conversation_messages_to_rig_history(&request.conversation_state.messages);
    let core_snapshot = build_core_conversation_snapshot(&request.conversation_state);
    let core_controller = VulcanCoreController::new(
        build_core_controller_config(
            preparation,
            &system_preamble,
            resolve_runtime_max_turns(active_agent),
            resolve_provider_additional_params(&preparation.model_blueprint),
        ),
        core_snapshot.clone(),
    );

    RuntimeBootstrapCore {
        system_preamble,
        request_history,
        core_snapshot,
        core_controller,
    }
}

/// Seeds the final request preview and structured request snapshot onto the mutable execution preparation object.
/// 将最终请求预览与结构化请求快照播种到可变执行准备对象上。
async fn seed_runtime_request_snapshot(
    preparation: &mut ExecutionPreparation,
    request: &ExecutionPreparationRequest,
    system_preamble: &str,
    resolved_tool_definitions: &[ResolvedToolDefinition],
    runtime_max_turns: Option<usize>,
    warnings: &mut Vec<String>,
) {
    match build_core_provider_request_snapshot(
        preparation,
        &request.conversation_state,
        system_preamble,
        request.prompt.trim(),
        resolved_tool_definitions,
        runtime_max_turns,
    )
    .await
    {
        Ok(request_snapshot) => {
            preparation.provider_request_preview_markdown =
                build_provider_request_preview_markdown(preparation, &request_snapshot);
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
}
