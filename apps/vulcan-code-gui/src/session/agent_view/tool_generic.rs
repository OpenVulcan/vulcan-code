//! Generic tool transcript projection used for MCP, unknown, and fallback tool rows.
//! 供 MCP、未知工具以及兜底工具行使用的通用工具转录投影。

use super::tool_shared::{build_base_tool_attributes, build_tool_directive_block};
use super::AgentViewBuildOptions;
use crate::model::SessionMessageEntry;
use vulcan_agent_view::{
    AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
};

/// Builds one generic tool transcript block when no dedicated presentation family applies.
/// 当不存在专属展示家族时，构建一条通用工具转录区块。
pub(crate) fn build_generic_tool_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) -> AgentViewDirectiveBlock {
    let attributes = build_base_tool_attributes(
        message,
        AgentViewToolPresentationKind::Generic,
        options,
    );
    build_tool_directive_block(kind, attributes)
}
