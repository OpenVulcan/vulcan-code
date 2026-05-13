//! Host-side tool runtime context helpers reused by provider transport entrypoints.
//! 供 Provider 传输入口复用的宿主侧工具运行时上下文辅助模块。

use crate::agent_runtime::provider_protocol_tool_definition::{
    ProviderToolDefinition, resolved_tool_definition_to_provider_tool_definition,
};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use std::sync::Arc;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Prepared host-side tool runtime context reused by provider transport wrappers so bridge creation and rig-tool mirroring stay consistent.
/// 供 Provider 传输包装器复用的宿主侧工具运行时上下文，使桥接层创建与 rig 工具镜像保持一致。
pub(crate) struct PreparedHostToolRuntimeContext {
    /// Shared host-managed bridge that owns the real tool dispatch path for the current execution turn.
    /// 持有当前执行回合真实工具调度路径的共享宿主管理桥接层。
    pub(crate) runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Mirrored provider-visible tool definitions exposed to the transport request builder.
    /// 暴露给传输请求构建器的镜像 Provider 可见工具定义集合。
    pub(crate) provider_tool_definitions: Vec<ProviderToolDefinition>,
}

/// Prepares one shared host-managed tool bridge plus the mirrored rig tool definitions for the current execution turn.
/// 为当前执行回合准备共享宿主管理工具桥接层及其镜像 rig 工具定义集合。
pub(crate) fn prepare_host_tool_runtime_context(
    tools_manager: VulcanToolsManager,
    resolved_tools: &[ResolvedToolDefinition],
    session_id: SessionId,
) -> PreparedHostToolRuntimeContext {
    PreparedHostToolRuntimeContext {
        runtime_bridge: Arc::new(VulcanManagedToolRuntimeBridge::new(
            Arc::new(tools_manager),
            resolved_tools,
            session_id,
        )),
        provider_tool_definitions: resolved_tools
            .iter()
            .map(resolved_tool_definition_to_provider_tool_definition)
            .collect(),
    }
}
