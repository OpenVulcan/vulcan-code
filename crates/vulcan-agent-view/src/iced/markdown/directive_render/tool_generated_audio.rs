//! Dedicated generated-audio transcript renderer.
//! 专属音频生成转录渲染器。

use super::{RenderedMediaKind, RenderedTranscriptBuilder, directive_background};
use crate::AgentViewDirectiveBlock;
use iced::Theme;
use vulcan_markdown as markdown;

/// Renders one generated-audio tool block with audio-specific transcript chrome.
/// 使用音频专属转录外框渲染一条音频生成工具区块。
pub(super) fn render_generated_audio_tool_block(
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
        RenderedMediaKind::Audio,
        String::from("音频生成结果"),
        directive_background(
            palette.success.weak.color.scale_alpha(0.08),
            palette.success.strong.color.scale_alpha(0.18),
        ),
        palette.success.strong.color,
    );
}
