//! Dedicated generated-document transcript renderer.
//! 专属文档生成转录渲染器。

use super::{RenderedMediaKind, RenderedTranscriptBuilder, directive_background};
use crate::AgentViewDirectiveBlock;
use iced::Theme;
use iced::widget::markdown;

/// Renders one generated-document tool block with document-specific transcript chrome.
/// 使用文档专属转录外框渲染一条文档生成工具区块。
pub(super) fn render_generated_document_tool_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    super::render_compact_tool_modal_summary_row(builder, block, markdown_settings, app_theme);
    let palette = app_theme.palette();
    super::render_generated_media_card_with_style(
        builder,
        block,
        app_theme,
        RenderedMediaKind::Pdf,
        String::from("文档生成结果"),
        directive_background(
            palette.secondary.weak.color.scale_alpha(0.08),
            palette.secondary.strong.color.scale_alpha(0.18),
        ),
        palette.secondary.strong.color,
    );
}
