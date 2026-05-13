//! Rig-specific tool-definition conversion helpers that keep third-party request schema types at the final transport edge only.
//! 仅在最终传输边界保留第三方请求 schema 类型的 rig 专用工具定义转换辅助模块。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;

/// Converts one host-owned provider tool definition into the low-level rig tool-definition shape at the final transport boundary.
/// 在最终传输边界将一条宿主自有 Provider 工具定义转换为低层 rig 工具定义形状。
pub(crate) fn provider_tool_definition_to_rig_tool_definition(
    definition: &ProviderToolDefinition,
) -> rig::completion::ToolDefinition {
    rig::completion::ToolDefinition {
        name: definition.name.clone(),
        description: definition.description.clone(),
        parameters: definition.parameters.clone(),
    }
}
