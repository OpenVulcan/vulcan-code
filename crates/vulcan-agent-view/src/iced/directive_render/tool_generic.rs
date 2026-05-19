//! Generic tool transcript renderer for MCP, unknown, and fallback tool rows.
//! 供 MCP、未知工具以及兜底工具行使用的通用工具转录渲染器。

use super::RenderedTranscriptBuilder;
use crate::AgentViewDirectiveBlock;
use iced::Theme;
use vulcan_markdown as markdown;

/// Renders one generic tool block using the shared compact summary-row presentation.
/// 使用共享紧凑摘要行样式渲染一条通用工具区块。
pub(super) fn render_generic_tool_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    super::render_compact_tool_modal_summary_row(builder, block, markdown_settings, app_theme);
}
