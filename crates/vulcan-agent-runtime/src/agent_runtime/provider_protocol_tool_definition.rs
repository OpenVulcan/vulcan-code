//! Host-owned tool-definition protocol used by provider transport and request bridge layers.
//! 供 Provider 传输与请求桥接层使用的宿主自有工具定义协议。

use crate::agent_runtime::provider_request_snapshot_support::{tool_source_key, tool_source_label};
use serde_json::Value;
use vulcan_tools_manager::ResolvedToolDefinition;

/// Provider-visible tool definition preserved inside the host runtime before the final transport-specific conversion step.
/// 在最终传输专用转换步骤之前，保存在宿主运行时内的 Provider 可见工具定义。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderToolDefinition {
    /// Stable tool name exposed to the model.
    /// 暴露给模型的稳定工具名称。
    pub name: String,
    /// Human-readable tool description exposed to the model.
    /// 暴露给模型的人类可读工具描述。
    pub description: String,
    /// Stable machine-readable source kind retained through transport projection.
    /// 贯穿传输投影保留的稳定机器可读来源类型。
    pub source_kind: String,
    /// Human-readable source label retained for diagnostics.
    /// 为诊断保留的人类可读来源标签。
    pub source_label: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    pub parameters: Value,
}

/// Converts one resolved host tool definition into the host-owned provider tool definition shape.
/// 将一条已解析宿主工具定义转换为宿主自有 Provider 工具定义形状。
pub(crate) fn resolved_tool_definition_to_provider_tool_definition(
    definition: &ResolvedToolDefinition,
) -> ProviderToolDefinition {
    ProviderToolDefinition {
        name: definition.descriptor.id.clone(),
        description: definition.descriptor.description.clone(),
        source_kind: tool_source_key(definition.descriptor.source).to_string(),
        source_label: tool_source_label(definition.descriptor.source).to_string(),
        parameters: definition.parameters_schema.clone(),
    }
}
