//! Host-side tool runtime context helpers reused by provider transport entrypoints.
//! 供 Provider 传输入口复用的宿主侧工具运行时上下文辅助模块。

use crate::agent_runtime::code_mode::{CodeModeActivation, CodeModeTurn};
use crate::agent_runtime::provider_protocol_tool_definition::{
    ProviderToolDefinition, resolved_tool_definition_to_provider_tool_definition,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use std::sync::Arc;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Prepared host-side tool runtime context reused by provider transport wrappers so bridge creation and provider-visible tool projection stay consistent.
/// 供 Provider 传输包装器复用的宿主侧工具运行时上下文，使桥接层创建与 Provider 可见工具投影保持一致。
pub(crate) struct PreparedHostToolRuntimeContext {
    /// Stable controller session identifier used by the current execution turn.
    /// 当前执行回合使用的稳定控制器会话标识。
    pub(crate) session_id: SessionId,
    /// Shared host-managed bridge that owns the real tool dispatch path for the current execution turn.
    /// 持有当前执行回合真实工具调度路径的共享宿主管理桥接层。
    pub(crate) runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Shared resumable task runtime reused by the current execution turn.
    /// 被当前执行回合复用的共享可恢复任务运行时。
    pub(crate) task_runtime: Arc<TaskRuntime>,
    /// Mirrored provider-visible tool definitions exposed to the transport request builder.
    /// 暴露给传输请求构建器的镜像 Provider 可见工具定义集合。
    pub(crate) provider_tool_definitions: Vec<ProviderToolDefinition>,
}

/// Prepares one shared host-managed tool bridge plus the mirrored provider-visible tool definitions for the current execution turn.
/// 为当前执行回合准备共享宿主管理工具桥接层及其镜像 Provider 可见工具定义集合。
///
/// `code_mode` replaces or extends the visible tools with `exec` and `wait`; JavaScript cannot
/// call any tool listed in `rejected_tool_ids`.
/// `code_mode` 以 `exec` 与 `wait` 替换或扩展可见工具；JavaScript 不能调用
/// `rejected_tool_ids` 中的任何工具。
pub(crate) fn prepare_host_tool_runtime_context(
    tools_manager: VulcanToolsManager,
    resolved_tools: &[ResolvedToolDefinition],
    session_id: SessionId,
    execution_mode: &str,
    task_runtime: Arc<TaskRuntime>,
    code_mode: Option<CodeModeActivation>,
    rejected_tool_ids: &[String],
) -> PreparedHostToolRuntimeContext {
    let tools_manager = Arc::new(tools_manager);
    let code_mode = code_mode.map(|activation| {
        Arc::new(CodeModeTurn::new(
            activation,
            resolved_tools,
            rejected_tool_ids,
        ))
    });
    let direct_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_provider_tool_definition)
        .collect::<Vec<_>>();
    let provider_tool_definitions = match code_mode.as_ref() {
        Some(code_mode) => code_mode.provider_tool_definitions(direct_tool_definitions),
        None => direct_tool_definitions,
    };
    PreparedHostToolRuntimeContext {
        session_id,
        runtime_bridge: Arc::new(
            VulcanManagedToolRuntimeBridge::new(
                Arc::clone(&tools_manager),
                resolved_tools,
                session_id,
                execution_mode,
            )
            .with_sensitive_tool_output_values(task_runtime.sensitive_tool_output_values())
            .with_tool_permission_host(task_runtime.tool_permission_host())
            .with_code_mode(code_mode),
        ),
        task_runtime,
        provider_tool_definitions,
    }
}
