//! Dedicated command-run transcript renderer.
//! 专属命令执行转录渲染器。

use super::RenderedTranscriptBuilder;
use crate::AgentViewDirectiveBlock;
use iced::Theme;
use vulcan_markdown as markdown;

/// Renders one command-run tool block using the shared compact summary-row presentation.
/// 使用共享紧凑摘要行样式渲染一条 command-run 工具区块。
pub(super) fn render_command_run_tool_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    super::render_compact_tool_modal_summary_row(builder, block, markdown_settings, app_theme);
}
