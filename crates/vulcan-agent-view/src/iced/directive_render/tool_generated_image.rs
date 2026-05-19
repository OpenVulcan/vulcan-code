//! Dedicated generated-image transcript renderer.
//! 专属图片生成转录渲染器。

use super::{RenderedMediaKind, RenderedTranscriptBuilder, directive_background};
use crate::AgentViewDirectiveBlock;
use iced::Theme;
use vulcan_markdown as markdown;

/// Renders one generated-image tool block with image-specific transcript chrome.
/// 使用图片专属转录外框渲染一条图片生成工具区块。
pub(super) fn render_generated_image_tool_block(
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
        RenderedMediaKind::Image,
        String::from("图片生成结果"),
        directive_background(
            palette.primary.weak.color.scale_alpha(0.08),
            palette.primary.strong.color.scale_alpha(0.18),
        ),
        palette.primary.strong.color,
    );
}
