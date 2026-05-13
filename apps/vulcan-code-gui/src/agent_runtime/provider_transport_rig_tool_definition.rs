//! Rig-specific tool-definition conversion helpers kept at the final provider transport edge only.
//! 仅在最终 Provider 传输边界保留的 rig 专用工具定义转换辅助模块。

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
