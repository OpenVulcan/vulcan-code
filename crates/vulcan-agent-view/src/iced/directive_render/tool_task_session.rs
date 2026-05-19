//! Dedicated task-session transcript renderer.
//! 专属 task-session 转录渲染器。

use super::RenderedTranscriptBuilder;
use crate::AgentViewDirectiveBlock;
use iced::Theme;
use iced::widget::markdown;

/// Renders one task-session tool block using the shared compact summary-row presentation.
/// 使用共享紧凑摘要行样式渲染一条 task-session 工具区块。
pub(super) fn render_task_session_tool_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    super::render_compact_tool_modal_summary_row(builder, block, markdown_settings, app_theme);
}
