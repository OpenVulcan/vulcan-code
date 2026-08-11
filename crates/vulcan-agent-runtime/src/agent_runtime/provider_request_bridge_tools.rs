//! Tool-definition bridge helpers reused by provider request snapshots and low-level transport dispatch.
//! 供 Provider 请求快照与低层传输分发复用的工具定义桥接辅助逻辑。

use crate::agent_runtime::provider_request_snapshot_support::{tool_source_key, tool_source_label};
use vulcan_core::types::ProviderRequestToolSchema as CoreProviderRequestToolSchema;
use vulcan_tools_manager::ResolvedToolDefinition;

/// Converts one resolved host tool definition into the `vulcan-core` request-tool schema shape.
/// 将一条已解析宿主工具定义转换为 `vulcan-core` 请求工具 schema 形状。
pub(crate) fn resolved_tool_definition_to_core_tool_schema(
    definition: &ResolvedToolDefinition,
) -> CoreProviderRequestToolSchema {
    CoreProviderRequestToolSchema {
        id: definition.descriptor.id.clone(),
        name: definition.descriptor.name.clone(),
        description: definition.descriptor.description.clone(),
        source_kind: tool_source_key(definition.descriptor.source).to_string(),
        source_label: tool_source_label(definition.descriptor.source).to_string(),
        parameters_schema: definition.parameters_schema.clone(),
    }
}
