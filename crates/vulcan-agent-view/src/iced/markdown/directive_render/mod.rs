//! Directive-block rendering helpers for semantic transcript panels, inline thinking, and user-prompt chrome.
//! 语义转录面板、内联思考与用户提示词外框共用的指令区块渲染辅助逻辑。

mod tool_command_run;
mod tool_command_session;
mod tool_generated_audio;
mod tool_generated_document;
mod tool_generated_image;
mod tool_generated_video;
mod tool_generic;
mod tool_task_session;

use super::builder::RenderedTranscriptBuilder;
use super::markdown_render::{
    build_multiline_diff_spans, render_markdown_items, render_markdown_source_with_table_fallback,
};
use super::markdown_source::{
    prepare_markdown_source_for_rendering, raw_source_contains_markdown_table,
};
use super::scene::{
    PlainParagraphStyle, PlainTextRenderKind, RenderContext, RenderedCollapsibleDirectiveNode,
    RenderedMediaKind, RenderedPanelAccentRule, RenderedParagraphBackground,
    RenderedParagraphHorizontalPlacement, RenderedTranscriptLinkAction, RenderedTranscriptNode,
};
use super::text_routing::build_agent_view_plain_text_spans;
use crate::iced_theme as theme;
use crate::resolve_video_thumbnail_preview;
use crate::{AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind};
use iced::advanced::text;
use iced::font;
use iced::widget::image::Handle as ImageHandle;
use iced::widget::text::Span;
use iced::{Background, Border, Color, Font, Padding, Pixels, Theme};
use std::path::Path;
use time::{OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description};
use vulcan_markdown as markdown;

/// Static format description used by the user-prompt meta row timestamp projection.
/// 用户提示词元信息行时间戳投影使用的静态格式描述。
const USER_PROMPT_TIMESTAMP_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]");

/// Maximum width ratio used by the right-aligned user-prompt bubble relative to the transcript viewport width.
/// 右对齐用户提示词气泡相对于转录视口宽度使用的最大宽度比例。
const USER_PROMPT_BUBBLE_MAX_WIDTH_RATIO: f32 = 0.70;

/// Link-action prefix used by the selectable transcript to identify copy-user-prompt chrome interactions.
/// 可选择转录用于识别复制用户提示词交互的链接动作前缀。
const USER_PROMPT_COPY_ACTION_PREFIX: &str = "vulcan-agent-view://copy-user-prompt/";

/// Link-action prefix used by the selectable transcript to identify host-side user-prompt editing interactions.
/// 可选择转录用于识别宿主侧用户提示词编辑交互的链接动作前缀。
const USER_PROMPT_EDIT_ACTION_PREFIX: &str = "vulcan-agent-view://edit-user-prompt/";

/// Resolves the icon, summary color, body background, body text color, and fallback title used by one tool directive block.
/// 解析单个工具指令区块所使用的图标、摘要颜色、正文背景、正文文字颜色以及兜底标题。
fn resolve_tool_directive_visuals(
    block: &AgentViewDirectiveBlock,
    app_theme: &Theme,
) -> Option<(
    &'static str,
    Color,
    RenderedParagraphBackground,
    Color,
    &'static str,
)> {
    let palette = app_theme.palette();
    let presentation_kind =
        AgentViewToolPresentationKind::resolve_from_attributes(&block.attributes);
    let task_execution_status = block
        .attributes
        .get("task-session-execution-status")
        .map(String::as_str)
        .unwrap_or("running");
    let command_status = block
        .attributes
        .get("command-run-status")
        .or_else(|| block.attributes.get("command-session-status"))
        .map(String::as_str)
        .unwrap_or("running");
    let command_session_action = block
        .attributes
        .get("command-session-action")
        .map(String::as_str)
        .unwrap_or("read");
    let change_set_mode = block
        .attributes
        .get("change-set-mode")
        .map(String::as_str)
        .unwrap_or("");
    let change_set_verification_status = block
        .attributes
        .get("change-set-verification-status")
        .map(String::as_str)
        .unwrap_or("skipped");
    match block.kind {
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
            if presentation_kind == AgentViewToolPresentationKind::CommandRun =>
        {
            match command_status {
                "failed" => Some((
                    "⌘",
                    iced::color!(0xFFD6D1),
                    directive_background(
                        palette.danger.weak.color.scale_alpha(0.10),
                        palette.danger.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "命令执行",
                )),
                "timed_out" => Some((
                    "⌘",
                    iced::color!(0xFBE6B8),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.warning.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "命令执行",
                )),
                "completed" => Some((
                    "⌘",
                    iced::color!(0xD7F6E6),
                    directive_background(
                        palette.success.weak.color.scale_alpha(0.10),
                        palette.success.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "命令执行",
                )),
                _ => Some((
                    "⌘",
                    iced::color!(0xFFE4B8),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.warning.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "命令执行",
                )),
            }
        }
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
            if presentation_kind == AgentViewToolPresentationKind::CommandSession =>
        {
            let fallback_title = match command_session_action {
                "start" => "命令会话",
                "write" => "命令会话",
                "read" => "命令会话",
                "stop" => "命令会话",
                _ => "命令会话",
            };
            match command_status {
                "failed" => Some((
                    "⌨",
                    iced::color!(0xFFD6D1),
                    directive_background(
                        palette.danger.weak.color.scale_alpha(0.10),
                        palette.danger.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    fallback_title,
                )),
                "stopped" => Some((
                    "⌨",
                    iced::color!(0xF3E0BA),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.background.strong.color.scale_alpha(0.22),
                    ),
                    palette.background.base.text,
                    fallback_title,
                )),
                "completed" => Some((
                    "⌨",
                    iced::color!(0xD7F6E6),
                    directive_background(
                        palette.success.weak.color.scale_alpha(0.10),
                        palette.success.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    fallback_title,
                )),
                _ => Some((
                    "⌨",
                    iced::color!(0xFFE4B8),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.warning.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    fallback_title,
                )),
            }
        }
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
            if presentation_kind == AgentViewToolPresentationKind::TaskSession =>
        {
            match task_execution_status {
                "stopped" => Some((
                    "🤖",
                    iced::color!(0xF3E0BA),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.background.strong.color.scale_alpha(0.22),
                    ),
                    palette.background.base.text,
                    "调度代理",
                )),
                "failed" => Some((
                    "🤖",
                    iced::color!(0xFFD6D1),
                    directive_background(
                        palette.danger.weak.color.scale_alpha(0.10),
                        palette.danger.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "调度代理",
                )),
                "completed" => Some((
                    "🤖",
                    iced::color!(0xD7F6E6),
                    directive_background(
                        palette.success.weak.color.scale_alpha(0.10),
                        palette.success.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "调度代理",
                )),
                _ => Some((
                    "🤖",
                    iced::color!(0xFFE4B8),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.warning.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "调度代理",
                )),
            }
        }
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
            if !change_set_mode.trim().is_empty() =>
        {
            match (
                change_set_mode.trim(),
                change_set_verification_status.trim(),
            ) {
                ("preview", _) => Some((
                    "📝",
                    iced::color!(0xCDE7FF),
                    directive_background(
                        palette.primary.weak.color.scale_alpha(0.10),
                        palette.primary.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "文件变更",
                )),
                ("applied", "warning") => Some((
                    "⚠",
                    iced::color!(0xFFE4B8),
                    directive_background(
                        palette.warning.weak.color.scale_alpha(0.10),
                        palette.warning.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "文件变更",
                )),
                ("applied", _) => Some((
                    "🧾",
                    iced::color!(0xD7F6E6),
                    directive_background(
                        palette.success.weak.color.scale_alpha(0.10),
                        palette.success.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "文件变更",
                )),
                _ => Some((
                    "🧾",
                    iced::color!(0xD7F6E6),
                    directive_background(
                        palette.success.weak.color.scale_alpha(0.10),
                        palette.success.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "文件变更",
                )),
            }
        }
        AgentViewDirectiveKind::ToolCall => {
            let display_state = block
                .attributes
                .get("tool-display-state")
                .map(String::as_str)
                .unwrap_or("running");
            match display_state {
                "failed" => Some((
                    "❌",
                    iced::color!(0xFFD6E6),
                    directive_background(
                        palette.danger.weak.color.scale_alpha(0.08),
                        palette.danger.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "调用工具",
                )),
                "completed" => Some((
                    "✅",
                    iced::color!(0xD7F6E6),
                    directive_background(
                        palette.success.weak.color.scale_alpha(0.08),
                        palette.success.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "调用工具",
                )),
                _ => Some((
                    "🔄",
                    iced::color!(0xCDE7FF),
                    directive_background(
                        palette.primary.weak.color.scale_alpha(0.08),
                        palette.primary.strong.color.scale_alpha(0.24),
                    ),
                    palette.background.base.text,
                    "调用工具",
                )),
            }
        }
        AgentViewDirectiveKind::ToolResult => Some((
            "✅",
            iced::color!(0xD7F6E6),
            directive_background(
                palette.success.weak.color.scale_alpha(0.08),
                palette.success.strong.color.scale_alpha(0.24),
            ),
            palette.background.base.text,
            "工具返回",
        )),
        _ => None,
    }
}

/// Builds the tool-summary spans used by tool-call and tool-result rows with one flat state color chosen by the caller.
/// 使用调用方选定的统一状态色构建工具调用与工具返回行使用的工具摘要片段。
fn build_tool_summary_spans(
    _block: &AgentViewDirectiveBlock,
    summary_text: &str,
    summary_color: Color,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    build_agent_view_plain_text_spans(
        summary_text,
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(summary_color),
        app_theme,
    )
}

#[allow(dead_code)]
/// Link-action prefix used by compact collapsible semantic blocks such as tool call and tool result rows.
/// 工具调用与工具结果等紧凑折叠语义区块使用的链接动作前缀。
const BLOCK_TOGGLE_ACTION_PREFIX: &str = "vulcan-agent-view://toggle-block-inline/";

/// Link-action prefix used by compact tool rows to open the dedicated centered modal surface.
/// 紧凑工具行用于打开专用居中弹层表面的链接动作前缀。
const TOOL_MODAL_ACTION_PREFIX: &str = "vulcan-agent-view://open-tool-modal/";

/// Link-action prefix used by inline thinking headers to open the dedicated centered modal surface.
/// 内联思考标题用于打开专用居中弹层表面的链接动作前缀。
const THINKING_MODAL_ACTION_PREFIX: &str = "vulcan-agent-view://open-thinking-modal/";

/// Renders one directive block using the transcript builder and semantic block treatments.
/// 使用转录构建器和语义块样式渲染一个指令区块。
pub(super) fn render_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    if matches!(&block.kind, AgentViewDirectiveKind::Custom(label) if label == "turn-stats") {
        render_turn_stats_block(builder, block, markdown_settings, app_theme);
        return;
    }

    if block.kind == AgentViewDirectiveKind::UserPrompt {
        render_user_prompt_block(builder, block, markdown_settings, app_theme);
        return;
    }

    if block.kind == AgentViewDirectiveKind::Thinking {
        render_thinking_block(builder, block, markdown_settings, app_theme);
        return;
    }

    if matches!(
        block.kind,
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
    ) {
        render_compact_tool_modal_directive_block(builder, block, markdown_settings, app_theme);
        return;
    }

    let palette = app_theme.palette();
    let title_background = match block.kind {
        AgentViewDirectiveKind::UserPrompt => directive_background(
            palette.secondary.weak.color.scale_alpha(0.12),
            palette.secondary.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::Thinking => directive_background(
            palette.warning.weak.color.scale_alpha(0.12),
            palette.warning.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::ToolCall => directive_background(
            palette.primary.weak.color.scale_alpha(0.12),
            palette.primary.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::ToolResult => directive_background(
            palette.success.weak.color.scale_alpha(0.12),
            palette.success.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::Diff => directive_background(
            palette.background.weak.color.scale_alpha(0.35),
            palette.background.strong.color.scale_alpha(0.45),
        ),
        AgentViewDirectiveKind::Notice => directive_background(
            palette.danger.weak.color.scale_alpha(0.12),
            palette.danger.strong.color.scale_alpha(0.35),
        ),
        AgentViewDirectiveKind::Custom(_) => directive_background(
            palette.background.weak.color.scale_alpha(0.26),
            palette.background.strong.color.scale_alpha(0.35),
        ),
    };

    builder.push_plain_paragraph(
        block.selectable_title(),
        PlainParagraphStyle {
            text_size: markdown_settings.text_size,
            line_height: text::LineHeight::default(),
            wrapping: text::Wrapping::Word,
            indent_left: 0.0,
            gap_before: 0.0,
            gap_after: if block.body.trim().is_empty() {
                0.0
            } else {
                4.0
            },
            color: Some(palette.background.base.text),
            render_kind: PlainTextRenderKind::Flow,
            horizontal_placement: RenderedParagraphHorizontalPlacement::Leading,
            fit_width_to_content: false,
            max_width_ratio: None,
            background: Some(title_background),
            surface_background: None,
            inner_padding: Padding::ZERO,
            suppress_embedded_backgrounds: false,
            quote_depth: 0,
        },
        app_theme,
    );

    match block.kind {
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult => {
            let body_background = Some(title_background);
            builder.push_rich_paragraph(
                build_agent_view_plain_text_spans(
                    if block.body.is_empty() {
                        " "
                    } else {
                        &block.body
                    },
                    theme::code_font(),
                    PlainTextRenderKind::Code,
                    Some(palette.background.base.text.scale_alpha(0.92)),
                    app_theme,
                ),
                markdown_settings.code_size,
                text::LineHeight::default(),
                text::Wrapping::WordOrGlyph,
                0.0,
                0.0,
                4.0,
                Padding::from([10.0, 12.0]),
                body_background,
                0,
            );
        }
        AgentViewDirectiveKind::Diff => {
            builder.push_rich_paragraph(
                build_multiline_diff_spans(&block.body, app_theme),
                markdown_settings.code_size,
                text::LineHeight::default(),
                text::Wrapping::WordOrGlyph,
                0.0,
                0.0,
                4.0,
                Padding::from([10.0, 12.0]),
                Some(title_background),
                0,
            );
        }
        _ => {
            let prepared_body = prepare_markdown_source_for_rendering(&block.body);
            if raw_source_contains_markdown_table(&prepared_body) {
                render_markdown_source_with_table_fallback(
                    builder,
                    &prepared_body,
                    PlainParagraphStyle {
                        background: Some(title_background),
                        gap_after: 6.0,
                        ..PlainParagraphStyle::flow(markdown_settings.text_size)
                    },
                    markdown_settings,
                    app_theme,
                );
            } else {
                let body_markdown = markdown::Content::parse_transcript(&prepared_body);
                render_markdown_items(
                    builder,
                    body_markdown.items(),
                    RenderContext::default(),
                    markdown_settings,
                    app_theme,
                );
            }
        }
    }
}

/// Renders one compact per-turn statistics footer row so finished turns expose aggregated usage without duplicating every provider call.
/// 渲染单条紧凑按轮次统计页脚行，使已结束轮次能够暴露聚合用量而不会重复展示每次 Provider 调用。
fn render_turn_stats_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let palette = app_theme.palette();
    let status = block
        .attributes
        .get("turn-stats-status")
        .map(String::as_str)
        .unwrap_or("completed");
    let (icon, accent_color) = match status {
        "failed" => ("⚠", palette.danger.strong.color),
        "stopped" => ("⏹", palette.warning.strong.color),
        _ => ("◔", palette.success.strong.color),
    };
    let summary_text = block
        .attributes
        .get("turn-stats-summary")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| block.body.trim());
    let line = if summary_text.is_empty() {
        format!("{icon} Turn Stats")
    } else {
        format!("{icon} {summary_text}")
    };
    let tooltip_title = block
        .attributes
        .get("title")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from("Turn Stats"));
    let block_id = block
        .attributes
        .get("ui-block-id")
        .cloned()
        .unwrap_or_else(|| tooltip_title.clone());
    let tooltip_link_target = format!("vulcan-agent-view://turn-stats-tooltip/{block_id}");
    builder.register_link_action(
        tooltip_link_target.clone(),
        RenderedTranscriptLinkAction::ShowTurnStatsTooltip {
            title: tooltip_title,
            body: block.body.clone(),
        },
    );
    let background = directive_background(
        palette.background.weak.color.scale_alpha(0.12),
        palette.background.strong.color.scale_alpha(0.22),
    );

    let mut summary_node = builder.build_rich_paragraph_node_with_layout(
        build_agent_view_plain_text_spans(
            &line,
            Font {
                weight: font::Weight::Medium,
                ..theme::ui_font()
            },
            PlainTextRenderKind::Flow,
            Some(accent_color),
            app_theme,
        ),
        Pixels(markdown_settings.text_size.0 - 1.0),
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        2.0,
        8.0,
        Padding::from([2.0, 8.0]),
        Some(background),
        0,
        RenderedParagraphHorizontalPlacement::Leading,
        false,
        None,
    );
    summary_node.background_action_link = Some(tooltip_link_target);
    builder
        .nodes
        .push(RenderedTranscriptNode::Paragraph(summary_node));
    builder.pending_separator = Some("\n");
}

/// Renders one compact tool row that opens a dedicated modal instead of expanding inline.
/// 渲染一条会打开专用弹窗而不是在正文中内联展开的紧凑工具行。
fn render_compact_tool_modal_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    match AgentViewToolPresentationKind::resolve_from_attributes(&block.attributes) {
        AgentViewToolPresentationKind::Generic => {
            tool_generic::render_generic_tool_block(builder, block, markdown_settings, app_theme)
        }
        AgentViewToolPresentationKind::CommandRun => {
            tool_command_run::render_command_run_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
        AgentViewToolPresentationKind::CommandSession => {
            tool_command_session::render_command_session_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
        AgentViewToolPresentationKind::TaskSession => {
            tool_task_session::render_task_session_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
        AgentViewToolPresentationKind::GeneratedImage => {
            tool_generated_image::render_generated_image_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
        AgentViewToolPresentationKind::GeneratedVideo => {
            tool_generated_video::render_generated_video_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
        AgentViewToolPresentationKind::GeneratedAudio => {
            tool_generated_audio::render_generated_audio_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
        AgentViewToolPresentationKind::GeneratedDocument => {
            tool_generated_document::render_generated_document_tool_block(
                builder,
                block,
                markdown_settings,
                app_theme,
            )
        }
    }
}

/// Renders the compact clickable tool summary row shared by plain and media-producing tool blocks.
/// 渲染普通工具与生成媒体工具共同使用的紧凑可点击工具摘要行。
pub(super) fn render_compact_tool_modal_summary_row(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let block_id = block
        .attributes
        .get("ui-block-id")
        .cloned()
        .unwrap_or_else(|| block.selectable_title());
    let action_link_target = format!("{TOOL_MODAL_ACTION_PREFIX}{block_id}");
    builder.register_link_action(
        action_link_target.clone(),
        RenderedTranscriptLinkAction::OpenToolModal(block_id),
    );

    let Some((icon, summary_color, _, _, fallback_title)) =
        resolve_tool_directive_visuals(block, app_theme)
    else {
        return;
    };
    let title = block
        .attributes
        .get("tool-summary-text")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            block
                .attributes
                .get("title")
                .cloned()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| fallback_title.to_string());
    let summary_text = format!("{icon} {title}");
    let summary_spans = build_tool_summary_spans(block, &summary_text, summary_color, app_theme);
    let mut summary_node = builder.build_rich_paragraph_node_with_layout(
        summary_spans,
        markdown_settings.text_size,
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        0.0,
        0.0,
        Padding::ZERO,
        None,
        0,
        RenderedParagraphHorizontalPlacement::Leading,
        false,
        None,
    );
    summary_node.background_action_link = Some(action_link_target);
    builder
        .nodes
        .push(RenderedTranscriptNode::Paragraph(summary_node));
    builder.pending_separator = Some("\n");
}

/// Renders one generated-media tool result as a dedicated inline media card using caller-provided visual styling.
/// 使用调用方提供的视觉样式，将一个生成媒体工具结果渲染为专用内联媒体卡片。
pub(super) fn render_generated_media_card_with_style(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    _app_theme: &Theme,
    expected_kind: RenderedMediaKind,
    title: String,
    background: RenderedParagraphBackground,
    accent_color: Color,
) {
    let Some(kind) = generated_media_kind_from_attributes(&block.attributes) else {
        return;
    };
    if kind != expected_kind {
        return;
    }
    let Some(file_name) = block
        .attributes
        .get("generated-media-file-name")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let Some(local_path) = block
        .attributes
        .get("generated-media-local-path")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let output_count = block
        .attributes
        .get("generated-media-count")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1);
    let is_active = block
        .attributes
        .get("ui-generated-media-playing")
        .is_some_and(|value| value == "true");
    let action_link_target = format!("vulcan-agent-view://generated-media/{local_path}");
    let action = match kind {
        RenderedMediaKind::Image => RenderedTranscriptLinkAction::OpenImagePreview {
            file_name: file_name.to_string(),
            local_path: local_path.to_string(),
        },
        RenderedMediaKind::Audio => RenderedTranscriptLinkAction::ToggleAudioPlayback {
            file_name: file_name.to_string(),
            local_path: local_path.to_string(),
        },
        RenderedMediaKind::Video | RenderedMediaKind::Pdf => {
            RenderedTranscriptLinkAction::OpenMediaWithDefaultApp(local_path.to_string())
        }
    };
    builder.register_link_action(action_link_target.clone(), action);
    let video_thumbnail_preview = match kind {
        RenderedMediaKind::Video => resolve_video_thumbnail_preview(
            local_path,
            block
                .attributes
                .get("generated-media-mime-type")
                .map(String::as_str),
        ),
        _ => None,
    };
    let preview_image_handle = match kind {
        RenderedMediaKind::Image => Some(ImageHandle::from_path(local_path.to_string())),
        RenderedMediaKind::Video => video_thumbnail_preview
            .as_ref()
            .map(|preview| preview.handle.clone()),
        _ => None,
    };
    let image_aspect_ratio = match kind {
        RenderedMediaKind::Image => local_image_aspect_ratio(local_path),
        RenderedMediaKind::Video => video_thumbnail_preview
            .as_ref()
            .map(|preview| preview.aspect_ratio),
        _ => None,
    };
    builder.push_media_card(
        kind,
        title,
        file_name.to_string(),
        local_path.to_string(),
        preview_image_handle,
        image_aspect_ratio,
        output_count,
        false,
        is_active,
        background,
        accent_color,
        Some(action_link_target),
    );
}

/// Resolves the rendered media kind declared by one generated-media directive attribute map.
/// 解析某个生成媒体指令属性映射声明的渲染媒体种类。
pub(super) fn generated_media_kind_from_attributes(
    attributes: &crate::AgentViewAttributes,
) -> Option<RenderedMediaKind> {
    match AgentViewToolPresentationKind::resolve_from_attributes(attributes) {
        AgentViewToolPresentationKind::GeneratedImage => Some(RenderedMediaKind::Image),
        AgentViewToolPresentationKind::GeneratedAudio => Some(RenderedMediaKind::Audio),
        AgentViewToolPresentationKind::GeneratedVideo => Some(RenderedMediaKind::Video),
        AgentViewToolPresentationKind::GeneratedDocument => Some(RenderedMediaKind::Pdf),
        _ => None,
    }
}

/// Reads the intrinsic width-height ratio of one local raster image file when the file is present and decodable.
/// 当本地光栅图片文件存在且可解码时，读取其原始宽高比。
pub(super) fn local_image_aspect_ratio(local_path: &str) -> Option<f32> {
    if !Path::new(local_path).is_file() {
        return None;
    }
    let (width, height) = image::image_dimensions(local_path).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(width as f32 / height as f32)
}

#[allow(dead_code)]
/// Renders one compact collapsible directive block used by tool-call, tool-result, and inline thinking rows.
/// 渲染工具调用、工具结果以及内联思考行共用的紧凑折叠指令区块。
fn render_compact_collapsible_directive_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let palette = app_theme.palette();
    let (icon, summary_color, body_background, body_text_color) = match block.kind {
        AgentViewDirectiveKind::Thinking => (
            "🧠",
            iced::color!(0xD8E8FF),
            directive_background(
                iced::color!(0x151E2A),
                iced::color!(0x46566B).scale_alpha(0.70),
            ),
            iced::color!(0xEAF1FA),
        ),
        AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult => {
            let Some((icon, summary_color, body_background, body_text_color, _)) =
                resolve_tool_directive_visuals(block, app_theme)
            else {
                return;
            };
            (icon, summary_color, body_background, body_text_color)
        }
        _ => return,
    };
    let block_id = block
        .attributes
        .get("ui-block-id")
        .cloned()
        .unwrap_or_else(|| block.selectable_title());
    let is_collapsed = block
        .attributes
        .get("ui-inline-collapsed")
        .map(|value| value == "true")
        .or_else(|| {
            block
                .attributes
                .get("collapsed")
                .map(|value| value == "true")
        })
        .unwrap_or(block.collapsed_by_default);
    let toggle_link_target = if block.kind == AgentViewDirectiveKind::Thinking {
        thinking_modal_action_link(block)
    } else {
        Some(format!("{BLOCK_TOGGLE_ACTION_PREFIX}{block_id}"))
    };
    if let Some(toggle_link_target) = toggle_link_target.as_ref() {
        builder.register_link_action(
            toggle_link_target.clone(),
            if block.kind == AgentViewDirectiveKind::Thinking {
                RenderedTranscriptLinkAction::OpenThinkingModal(block_id.clone())
            } else {
                RenderedTranscriptLinkAction::ToggleBlockInline(block_id.clone())
            },
        );
    }

    let title_suffix = block
        .attributes
        .get("title")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| match block.kind {
            AgentViewDirectiveKind::Thinking => "思考过程",
            AgentViewDirectiveKind::ToolCall => "调用工具",
            AgentViewDirectiveKind::ToolResult => "工具返回",
            _ => "",
        });
    let toggle_glyph = if block.kind == AgentViewDirectiveKind::Thinking {
        ">"
    } else if is_collapsed {
        ">"
    } else {
        "v"
    };
    let summary_text = format!("{icon} {title_suffix} {toggle_glyph}");
    let summary_spans = build_tool_summary_spans(block, &summary_text, summary_color, app_theme);
    let mut summary_node = builder.build_rich_paragraph_node_with_layout(
        summary_spans,
        markdown_settings.text_size,
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        0.0,
        if is_collapsed { 8.0 } else { 4.0 },
        Padding::from([8.0, 0.0]),
        None,
        0,
        RenderedParagraphHorizontalPlacement::Leading,
        false,
        None,
    );
    summary_node.background_action_link = toggle_link_target;

    let body_node = if block.body.trim().is_empty() {
        None
    } else {
        Some(builder.build_rich_paragraph_node_with_layout(
            build_agent_view_plain_text_spans(
                block.body.trim_end(),
                if block.kind == AgentViewDirectiveKind::Thinking {
                    theme::ui_font()
                } else {
                    theme::code_font()
                },
                if block.kind == AgentViewDirectiveKind::Thinking {
                    PlainTextRenderKind::Flow
                } else {
                    PlainTextRenderKind::Code
                },
                Some(body_text_color),
                app_theme,
            ),
            if block.kind == AgentViewDirectiveKind::Thinking {
                markdown_settings.text_size
            } else {
                markdown_settings.code_size
            },
            text::LineHeight::default(),
            text::Wrapping::WordOrGlyph,
            0.0,
            0.0,
            8.0,
            Padding::from([10.0, 12.0]),
            Some(body_background),
            0,
            RenderedParagraphHorizontalPlacement::Leading,
            false,
            None,
        ))
    };

    builder.nodes.push(RenderedTranscriptNode::Collapsible(
        RenderedCollapsibleDirectiveNode {
            block_id,
            summary: summary_node,
            body: body_node,
            collapsed: is_collapsed,
            max_visible_lines: 10,
            body_background,
            scrollbar_track_color: palette.background.strong.color.scale_alpha(0.20),
            scrollbar_thumb_color: palette.background.base.text.scale_alpha(0.35),
        },
    ));
    builder.pending_separator = Some("\n");
}

/// Renders one thinking block; completed inline thinking becomes one quote-style note while modal and overlay surfaces keep the dedicated dark panel treatment.
/// 渲染单个思考区块；已完成的内联思考会转为引用备注样式，而弹层与流式覆盖层仍保持专用深色面板样式。
pub(super) fn render_thinking_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let thinking_surface = block
        .attributes
        .get("ui-thinking-surface")
        .map(String::as_str)
        .unwrap_or("inline");
    let is_inline_surface = thinking_surface == "inline";

    if is_inline_surface {
        render_inline_thinking_note(builder, block, markdown_settings, app_theme);
        return;
    }

    let outer_panel_background = RenderedParagraphBackground {
        background: Background::Color(iced::color!(0x1C2230)),
        border: Border {
            color: iced::color!(0x68748A).scale_alpha(0.84),
            width: 1.0,
            radius: 2.0.into(),
        },
    };
    let body_foreground = iced::color!(0xEEF2F7);
    let body_content_padding = Padding::ZERO;
    let action_link_target = thinking_modal_action_link(block);
    let render_header = block
        .attributes
        .get("ui-render-thinking-header")
        .map(|value| value != "false")
        .unwrap_or(true);
    let header_padding = Padding::ZERO;
    let header_gap_after = 0.0;

    if let Some(action_link_target) = action_link_target.as_ref() {
        if let Some(block_id) = block.attributes.get("ui-block-id").map(String::as_str) {
            builder.register_link_action(
                action_link_target.clone(),
                RenderedTranscriptLinkAction::OpenThinkingModal(block_id.to_string()),
            );
        }
    }

    if render_header {
        let header_title = block
            .attributes
            .get("ui-thinking-title")
            .cloned()
            .unwrap_or_else(|| String::from("思考过程"));
        let header_spans = build_thinking_header_spans(
            &header_title,
            action_link_target.as_deref(),
            body_foreground,
            app_theme,
        );

        builder.push_rich_paragraph_with_layout(
            header_spans,
            markdown_settings.text_size,
            text::LineHeight::default(),
            text::Wrapping::Word,
            0.0,
            0.0,
            header_gap_after,
            header_padding,
            None,
            0,
            RenderedParagraphHorizontalPlacement::Leading,
            false,
            None,
        );
        if let Some(RenderedTranscriptNode::Paragraph(header_node)) = builder.nodes.last_mut() {
            header_node.background_action_link = action_link_target.clone();
        }
    }

    let body_node_start = builder.node_count();
    let prepared_body = prepare_markdown_source_for_rendering(&block.body);
    let paragraph_style = PlainParagraphStyle {
        background: None,
        surface_background: if is_inline_surface {
            Some(outer_panel_background)
        } else {
            None
        },
        inner_padding: body_content_padding,
        color: Some(body_foreground),
        gap_after: 4.0,
        suppress_embedded_backgrounds: true,
        ..PlainParagraphStyle::flow(markdown_settings.text_size)
    };

    if raw_source_contains_markdown_table(&prepared_body) {
        render_markdown_source_with_table_fallback(
            builder,
            &prepared_body,
            paragraph_style,
            markdown_settings,
            app_theme,
        );
    } else {
        let body_markdown = markdown::Content::parse_transcript(&prepared_body);
        render_markdown_items(
            builder,
            body_markdown.items(),
            RenderContext {
                block_background: None,
                surface_background: if is_inline_surface {
                    Some(outer_panel_background)
                } else {
                    None
                },
                foreground_color: Some(body_foreground),
                inner_padding: body_content_padding,
                suppress_embedded_backgrounds: true,
                ..RenderContext::default()
            },
            markdown_settings,
            app_theme,
        );
    }

    if is_inline_surface {
        let body_node_count = builder.node_count().saturating_sub(body_node_start);
        if body_node_count > 0 {
            builder.insert_panel(body_node_start, body_node_count, outer_panel_background);
        }
    }
}

/// Renders one completed inline thinking block as a permanently expanded Markdown-note projection with a quote bar.
/// 将单个已完成的内联思考区块渲染为永久展开的 Markdown 备注投影，并带有引用竖线。
fn render_inline_thinking_note(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let content_indent_left = 14.0;
    let render_header = block
        .attributes
        .get("ui-render-thinking-header")
        .map(|value| value != "false")
        .unwrap_or(true);
    let title = block
        .attributes
        .get("ui-inline-thinking-title")
        .or_else(|| block.attributes.get("ui-thinking-title"))
        .cloned()
        .unwrap_or_else(|| String::from("思考过程"));
    let title_color = app_theme.palette().background.base.text.scale_alpha(0.88);
    let action_link_target = thinking_modal_action_link(block);

    if let Some(action_link_target) = action_link_target.as_ref()
        && let Some(block_id) = block.attributes.get("ui-block-id").map(String::as_str)
    {
        builder.register_link_action(
            action_link_target.clone(),
            RenderedTranscriptLinkAction::OpenThinkingModal(block_id.to_string()),
        );
    }

    if render_header {
        let mut title_spans = build_agent_view_plain_text_spans(
            &title,
            Font {
                weight: font::Weight::Semibold,
                ..theme::ui_font()
            },
            PlainTextRenderKind::Flow,
            Some(title_color),
            app_theme,
        );
        if let Some(action_link_target) = action_link_target.as_ref() {
            for span in &mut title_spans {
                span.link = Some(action_link_target.clone().into());
            }
        }
        builder.push_rich_paragraph(
            title_spans,
            markdown_settings.text_size,
            text::LineHeight::default(),
            text::Wrapping::Word,
            content_indent_left,
            0.0,
            if block.body.trim().is_empty() {
                0.0
            } else {
                4.0
            },
            Padding::ZERO,
            None,
            0,
        );
        if let Some(RenderedTranscriptNode::Paragraph(header_node)) = builder.nodes.last_mut() {
            header_node.background_action_link = action_link_target.clone();
        }
    }

    let body_node_start = builder.node_count();
    let prepared_body = prepare_markdown_source_for_rendering(&block.body);
    let paragraph_style = PlainParagraphStyle {
        background: None,
        surface_background: None,
        inner_padding: Padding::ZERO,
        color: None,
        indent_left: content_indent_left,
        quote_depth: 0,
        gap_after: 4.0,
        suppress_embedded_backgrounds: false,
        ..PlainParagraphStyle::flow(markdown_settings.text_size)
    };

    if raw_source_contains_markdown_table(&prepared_body) {
        render_markdown_source_with_table_fallback(
            builder,
            &prepared_body,
            paragraph_style,
            markdown_settings,
            app_theme,
        );
    } else {
        let body_markdown = markdown::Content::parse_transcript(&prepared_body);
        render_markdown_items(
            builder,
            body_markdown.items(),
            RenderContext {
                indent_left: content_indent_left,
                quote_depth: 0,
                ..RenderContext::default()
            },
            markdown_settings,
            app_theme,
        );
    }

    let panel_start_index = if render_header {
        body_node_start.saturating_sub(1)
    } else {
        body_node_start
    };
    let panel_child_count = builder.node_count().saturating_sub(panel_start_index);
    let note_panel_background = RenderedParagraphBackground {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
    };
    let accent_color = app_theme
        .palette()
        .background
        .strong
        .color
        .scale_alpha(0.58);

    if panel_child_count > 0 {
        builder.insert_panel_with_accent(
            panel_start_index,
            panel_child_count,
            note_panel_background,
            Some(RenderedPanelAccentRule {
                color: accent_color,
                width: 3.0,
                top_inset: 2.0,
                bottom_inset: 2.0,
            }),
        );
    }
}

/// Renders one user-prompt directive block as a right-aligned bubble followed by a right-aligned timestamp and copy action row.
/// 将单个用户提示词指令区块渲染为右对齐气泡，并在其下方追加右对齐的时间戳与复制动作行。
fn render_user_prompt_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
    let user_prompt_images = user_prompt_image_descriptors_from_attributes(block);
    let palette = app_theme.palette();
    let bubble_fill = mix_color(
        palette.background.strong.color,
        palette.primary.strong.color,
        0.22,
    )
    .scale_alpha(0.98);
    let bubble_border_color = mix_color(
        palette.primary.strong.color,
        palette.background.base.text,
        0.20,
    )
    .scale_alpha(0.84);
    let bubble_text_color = iced::color!(0xF3F8FD);
    let bubble_meta_color = iced::color!(0xAABACE);
    let bubble_action_color = iced::color!(0x84D2FF);
    let bubble_background = directive_background(bubble_fill, bubble_border_color);

    render_user_prompt_image_panels(builder, &user_prompt_images, app_theme);

    if !block.body.trim().is_empty() || user_prompt_images.is_empty() {
        builder.push_rich_paragraph_with_layout(
            build_agent_view_plain_text_spans(
                if block.body.is_empty() {
                    " "
                } else {
                    &block.body
                },
                theme::ui_font(),
                PlainTextRenderKind::Flow,
                Some(bubble_text_color),
                app_theme,
            ),
            markdown_settings.text_size,
            text::LineHeight::default(),
            text::Wrapping::Word,
            0.0,
            0.0,
            0.0,
            Padding::from([10.0, 12.0]),
            Some(bubble_background),
            0,
            RenderedParagraphHorizontalPlacement::Trailing,
            true,
            Some(USER_PROMPT_BUBBLE_MAX_WIDTH_RATIO),
        );
    }

    let copy_action_link = user_prompt_copy_action_link(block);
    if let Some(copy_action_link) = copy_action_link.as_ref() {
        builder.register_link_action(
            copy_action_link.clone(),
            RenderedTranscriptLinkAction::CopyText(block.body.clone()),
        );
    }
    let edit_action_link = user_prompt_edit_action_link(block);
    if let (Some(edit_action_link), Some(message_id)) =
        (edit_action_link.as_ref(), user_prompt_message_id(block))
    {
        builder.register_link_action(
            edit_action_link.clone(),
            RenderedTranscriptLinkAction::EditUserPrompt(message_id.to_string()),
        );
    }

    let meta_spans = build_user_prompt_meta_spans(
        block,
        copy_action_link.as_deref(),
        edit_action_link.as_deref(),
        bubble_meta_color,
        bubble_action_color,
        app_theme,
    );
    if meta_spans.is_empty() {
        return;
    }

    builder.push_rich_paragraph_with_layout(
        meta_spans,
        Pixels(11.0),
        text::LineHeight::default(),
        text::Wrapping::Word,
        0.0,
        4.0,
        10.0,
        Padding::ZERO,
        None,
        0,
        RenderedParagraphHorizontalPlacement::Trailing,
        true,
        None,
    );
}

/// One compact user-image descriptor projected from user-prompt metadata so the renderer can show staged image tiles above the prompt bubble.
/// 从用户提示词元数据投影出的一条紧凑用户图片描述符，供渲染器在提示词气泡上方显示已暂存图片方格。
#[derive(Debug, Clone)]
struct UserPromptImageDescriptor {
    /// Human-readable file name mirrored into preview actions.
    /// 镜像到预览动作中的人类可读文件名。
    file_name: String,
    /// Local absolute path used by the host image preview modal.
    /// 供宿主图片预览弹层使用的本地绝对路径。
    local_path: String,
}

/// Renders compact user image tiles above one prompt bubble so downgraded image-understanding inputs remain visible after send and reload.
/// 在提示词气泡上方渲染紧凑用户图片方格，使降级为图片理解工具输入的图片在发送后与重开后仍然可见。
fn render_user_prompt_image_panels(
    builder: &mut RenderedTranscriptBuilder,
    images: &[UserPromptImageDescriptor],
    app_theme: &Theme,
) {
    if images.is_empty() {
        return;
    }

    let palette = app_theme.palette();
    let background = directive_background(
        palette.primary.weak.color.scale_alpha(0.12),
        palette.primary.strong.color.scale_alpha(0.22),
    );

    for image in images {
        let action_link_target =
            format!("vulcan-agent-view://user-prompt-image/{}", image.local_path);
        builder.register_link_action(
            action_link_target.clone(),
            RenderedTranscriptLinkAction::OpenImagePreview {
                file_name: image.file_name.clone(),
                local_path: image.local_path.clone(),
            },
        );
        builder.push_media_card_with_options(
            RenderedMediaKind::Image,
            String::new(),
            image.file_name.clone(),
            image.local_path.clone(),
            None,
            local_image_aspect_ratio(&image.local_path),
            1,
            false,
            true,
            false,
            0.0,
            6.0,
            0.70,
            RenderedParagraphHorizontalPlacement::Trailing,
            background,
            palette.primary.strong.color,
            Some(action_link_target),
        );
    }
}

/// Extracts compact user-image descriptors from one user-prompt block attribute map.
/// 从单个用户提示词区块属性映射中提取紧凑用户图片描述符。
fn user_prompt_image_descriptors_from_attributes(
    block: &AgentViewDirectiveBlock,
) -> Vec<UserPromptImageDescriptor> {
    let Some(image_count) = block
        .attributes
        .get("user-image-count")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return Vec::new();
    };

    (0..image_count)
        .filter_map(|index| {
            let file_name = block
                .attributes
                .get(&format!("user-image-{index}-file-name"))?
                .trim()
                .to_string();
            let local_path = block
                .attributes
                .get(&format!("user-image-{index}-local-path"))?
                .trim()
                .to_string();
            if file_name.is_empty() || local_path.is_empty() {
                return None;
            }
            Some(UserPromptImageDescriptor {
                file_name,
                local_path,
            })
        })
        .collect()
}

/// Builds the timestamp and action spans shown under one right-aligned user-prompt bubble.
/// 构建显示在右对齐用户提示词气泡下方的时间戳与动作片段。
pub(super) fn build_user_prompt_meta_spans(
    block: &AgentViewDirectiveBlock,
    copy_action_link: Option<&str>,
    edit_action_link: Option<&str>,
    meta_text_color: Color,
    action_text_color: Color,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let mut spans = Vec::new();

    if let Some(timestamp_label) = user_prompt_created_at_label(block) {
        spans.extend(build_agent_view_plain_text_spans(
            &timestamp_label,
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            Some(meta_text_color),
            app_theme,
        ));
    }

    if let Some(copy_action_link) = copy_action_link {
        if !spans.is_empty() {
            spans.extend(build_agent_view_plain_text_spans(
                "   ",
                theme::ui_font(),
                PlainTextRenderKind::Flow,
                Some(meta_text_color.scale_alpha(0.72)),
                app_theme,
            ));
        }

        let mut copy_spans = build_agent_view_plain_text_spans(
            "⧉ 复制",
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            Some(action_text_color),
            app_theme,
        );
        for span in &mut copy_spans {
            span.link = Some(copy_action_link.into());
        }
        spans.extend(copy_spans);
    }

    if let Some(edit_action_link) = edit_action_link {
        if !spans.is_empty() {
            spans.extend(build_agent_view_plain_text_spans(
                "   ",
                theme::ui_font(),
                PlainTextRenderKind::Flow,
                Some(meta_text_color.scale_alpha(0.72)),
                app_theme,
            ));
        }

        let mut edit_spans = build_agent_view_plain_text_spans(
            "✎ 编辑",
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            Some(action_text_color),
            app_theme,
        );
        for span in &mut edit_spans {
            span.link = Some(edit_action_link.into());
        }
        spans.extend(edit_spans);
    }

    spans
}

/// Returns the formatted timestamp label shown below one user-prompt bubble, if the directive carries a valid creation timestamp.
/// 返回显示在用户提示词气泡下方的格式化时间标签；若指令携带了有效创建时间则返回该标签。
pub(super) fn user_prompt_created_at_label(block: &AgentViewDirectiveBlock) -> Option<String> {
    let created_at_millis = block
        .attributes
        .get("created-at")?
        .trim()
        .parse::<i128>()
        .ok()?;
    let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local_time = OffsetDateTime::from_unix_timestamp_nanos(created_at_millis * 1_000_000)
        .ok()?
        .to_offset(local_offset);

    local_time
        .format(USER_PROMPT_TIMESTAMP_FORMAT)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Returns the synthetic link target used by the user-prompt copy action, if the directive carries a stable message identifier.
/// 返回用户提示词复制动作使用的合成链接目标；若指令携带稳定消息标识则返回该目标。
pub(super) fn user_prompt_copy_action_link(block: &AgentViewDirectiveBlock) -> Option<String> {
    let message_id = user_prompt_message_id(block)?;
    Some(format!("{USER_PROMPT_COPY_ACTION_PREFIX}{message_id}"))
}

/// Returns the synthetic link target used by the user-prompt edit action when the host marks this prompt editable.
/// 当宿主将该用户提示词标记为可编辑时，返回编辑动作使用的合成链接目标。
pub(super) fn user_prompt_edit_action_link(block: &AgentViewDirectiveBlock) -> Option<String> {
    let can_edit = block
        .attributes
        .get("can-edit")
        .map(String::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let message_id = user_prompt_message_id(block)?;
    can_edit.then(|| format!("{USER_PROMPT_EDIT_ACTION_PREFIX}{message_id}"))
}

/// Returns the stable user-prompt message id carried by one directive block.
/// 返回单个指令区块携带的稳定用户提示词消息标识。
fn user_prompt_message_id(block: &AgentViewDirectiveBlock) -> Option<&str> {
    block
        .attributes
        .get("message-id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|message_id| !message_id.is_empty())
}

/// Builds the linked title-line spans used by one inline thinking header.
/// 构建单个内联思考标题使用的可点击标题行片段。
fn build_thinking_header_spans(
    title: &str,
    action_link_target: Option<&str>,
    foreground: Color,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let mut spans = build_agent_view_plain_text_spans(
        &format!("🧠 {title}"),
        Font {
            weight: font::Weight::Semibold,
            ..theme::ui_font()
        },
        PlainTextRenderKind::Flow,
        Some(foreground),
        app_theme,
    );

    if let Some(action_link_target) = action_link_target {
        for span in &mut spans {
            span.link = Some(action_link_target.into());
        }
    }

    spans
}

/// Returns the synthetic link target used by one inline thinking header to open the dedicated modal surface.
/// 返回单个内联思考标题使用的合成链接目标，用于打开专用弹层表面。
fn thinking_modal_action_link(block: &AgentViewDirectiveBlock) -> Option<String> {
    let block_id = block.attributes.get("ui-block-id")?.trim();
    (!block_id.is_empty()).then(|| format!("{THINKING_MODAL_ACTION_PREFIX}{block_id}"))
}

/// Returns one directive-like background treatment from the provided fill and border colors.
/// 根据提供的填充色与边框色返回一个指令风格背景样式。
pub(super) fn directive_background(
    fill: Color,
    border_color: Color,
) -> RenderedParagraphBackground {
    RenderedParagraphBackground {
        background: Background::Color(fill),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 10.0.into(),
        },
    }
}

/// Blends two colors using the provided interpolation ratio so semantic surfaces can derive subtle tonal variants.
/// 使用给定插值比例混合两种颜色，以便语义表面派生细微的色调变体。
pub(super) fn mix_color(base: Color, overlay: Color, ratio: f32) -> Color {
    let clamped_ratio = ratio.clamp(0.0, 1.0);
    Color {
        r: base.r + (overlay.r - base.r) * clamped_ratio,
        g: base.g + (overlay.g - base.g) * clamped_ratio,
        b: base.b + (overlay.b - base.b) * clamped_ratio,
        a: base.a + (overlay.a - base.a) * clamped_ratio,
    }
}

/// Enumerates the semantic line kinds used by rendered diff previews.
/// 枚举渲染 diff 预览使用的语义行类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DiffLineKind {
    /// Added diff line highlighted as an insertion.
    /// 作为新增内容高亮的 diff 行。
    Added,
    /// Removed diff line highlighted as a deletion.
    /// 作为删除内容高亮的 diff 行。
    Removed,
    /// Metadata or hunk-header diff line.
    /// 元数据或 hunk 头部 diff 行。
    Note,
    /// Neutral context diff line.
    /// 中性的上下文 diff 行。
    Context,
}

/// Returns the semantic diff color used for one rendered diff line.
/// 返回某个渲染 diff 行使用的语义颜色。
pub(super) fn diff_line_color(app_theme: &Theme, line_kind: DiffLineKind) -> Color {
    let palette = app_theme.palette();

    match line_kind {
        DiffLineKind::Added => palette.success.strong.color,
        DiffLineKind::Removed => palette.danger.strong.color,
        DiffLineKind::Note => palette.primary.strong.color,
        DiffLineKind::Context => palette.background.base.text.scale_alpha(0.92),
    }
}

/// Classifies one raw diff line into its semantic line kind.
/// 将一条原始 diff 行分类为对应的语义行类型。
pub(super) fn classify_diff_line_kind(line: &str) -> DiffLineKind {
    if line.starts_with('+') && !line.starts_with("+++") {
        DiffLineKind::Added
    } else if line.starts_with('-') && !line.starts_with("---") {
        DiffLineKind::Removed
    } else if line.starts_with("@@")
        || line.starts_with("diff --git ")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        DiffLineKind::Note
    } else {
        DiffLineKind::Context
    }
}

#[cfg(test)]
mod tests {
    //! Targeted rendering tests for tool-summary visual states.
    //! 面向工具摘要视觉状态的定向渲染测试。

    use super::{build_tool_summary_spans, render_directive_block};
    use crate::iced::builder::RenderedTranscriptBuilder;
    use crate::iced::scene::{RenderedMediaKind, RenderedTranscriptNode};
    use crate::iced_theme as theme;
    use crate::{AgentViewAttributes, AgentViewDirectiveBlock, AgentViewToolPresentationKind};
    use iced::{Theme, color};

    /// Verifies that one running tool call renders its summary using one flat sky-blue color after the shiny in-flight treatment was removed.
    /// 验证进行中的工具调用在移除闪亮进行中特效后会使用统一的天蓝色摘要颜色。
    #[test]
    fn running_tool_call_uses_flat_summary_color() {
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(
            String::from("title"),
            String::from("调用 read_file 工具 进行中"),
        );
        attributes.insert(String::from("tool-display-state"), String::from("running"));
        let block = AgentViewDirectiveBlock::tool_call("{}", true, attributes);
        let spans = build_tool_summary_spans(
            &block,
            "🔄 调用 read_file 工具 进行中",
            color!(0xCDE7FF),
            &Theme::Dark,
        );

        assert!(
            spans
                .iter()
                .all(|span| span.color == Some(color!(0xCDE7FF)))
        );
    }

    /// Verifies that one completed tool call keeps one flat success color and can therefore share the same compact rendering path with running and failed rows.
    /// 验证已完成工具调用会保持统一的成功色，从而与进行中和失败行共享同一条紧凑渲染路径。
    #[test]
    fn completed_tool_call_uses_flat_summary_color() {
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(
            String::from("title"),
            String::from("调用 read_file 工具 已完成"),
        );
        attributes.insert(
            String::from("tool-display-state"),
            String::from("completed"),
        );
        let block = AgentViewDirectiveBlock::tool_call("{}", true, attributes);
        let spans = build_tool_summary_spans(
            &block,
            "✅ 调用 read_file 工具 已完成",
            color!(0xD7F6E6),
            &Theme::Dark,
        );

        assert!(
            spans
                .iter()
                .all(|span| span.color == Some(color!(0xD7F6E6)))
        );
    }

    /// Verifies that one failed tool call uses one flat pink summary color so host-managed tool errors remain visually distinct.
    /// 验证失败工具调用会使用统一的粉红色摘要颜色，从而让宿主管理工具错误保持明确的视觉区分。
    #[test]
    fn failed_tool_call_uses_flat_failure_color() {
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(
            String::from("title"),
            String::from("调用 read_file 工具 失败"),
        );
        attributes.insert(String::from("tool-display-state"), String::from("failed"));
        let block = AgentViewDirectiveBlock::tool_call("{}", true, attributes);
        let spans = build_tool_summary_spans(
            &block,
            "❌ 调用 read_file 工具 失败",
            color!(0xFFD6E6),
            &Theme::Dark,
        );

        assert!(
            spans
                .iter()
                .all(|span| span.color == Some(color!(0xFFD6E6)))
        );
    }

    /// Verifies generated-media tool cards keep the clickable tool row before the media preview while preserving media playback state.
    /// 验证生成媒体工具卡会在媒体预览前保留可点击工具行，同时保持媒体播放状态。
    #[test]
    fn generated_media_tool_card_keeps_tool_summary_before_media_preview() {
        let mut builder = RenderedTranscriptBuilder::new(color!(0x4477AA), color!(0xE8EEF7));
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(
            String::from("title"),
            String::from("调用 image_generation 工具"),
        );
        AgentViewToolPresentationKind::GeneratedAudio.write_to_attributes(&mut attributes);
        attributes.insert(
            String::from("generated-media-file-name"),
            String::from("demo.mp3"),
        );
        attributes.insert(
            String::from("generated-media-local-path"),
            String::from("D:/demo.mp3"),
        );
        attributes.insert(String::from("generated-media-count"), String::from("1"));
        attributes.insert(
            String::from("ui-generated-media-playing"),
            String::from("true"),
        );
        let block = AgentViewDirectiveBlock::tool_call("{}", false, attributes);

        render_directive_block(
            &mut builder,
            &block,
            theme::markdown_settings(&Theme::Dark),
            &Theme::Dark,
        );

        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Paragraph(_))
        ));
        let Some(RenderedTranscriptNode::MediaCard(node)) = builder.nodes.get(1) else {
            panic!("expected media card after the tool summary row");
        };
        assert_eq!(node.kind, RenderedMediaKind::Audio);
        assert!(!node.show_caption);
        assert!(node.is_active);
    }

    /// Verifies that legacy string-based generated-media attributes still route into the typed generated-audio renderer after the refactor.
    /// 验证旧版基于字符串的 generated-media 属性在重构后仍能路由到强类型音频生成渲染器。
    #[test]
    fn generated_media_tool_card_keeps_legacy_attribute_compatibility() {
        let mut builder = RenderedTranscriptBuilder::new(color!(0x4477AA), color!(0xE8EEF7));
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(
            String::from("title"),
            String::from("调用 image_generation 工具"),
        );
        attributes.insert(
            String::from("tool-visual-kind"),
            String::from("generated-media"),
        );
        attributes.insert(String::from("generated-media-kind"), String::from("audio"));
        attributes.insert(
            String::from("generated-media-file-name"),
            String::from("legacy.mp3"),
        );
        attributes.insert(
            String::from("generated-media-local-path"),
            String::from("D:/legacy.mp3"),
        );
        attributes.insert(String::from("generated-media-count"), String::from("1"));
        let block = AgentViewDirectiveBlock::tool_call("{}", false, attributes);

        render_directive_block(
            &mut builder,
            &block,
            theme::markdown_settings(&Theme::Dark),
            &Theme::Dark,
        );

        let Some(RenderedTranscriptNode::MediaCard(node)) = builder.nodes.get(1) else {
            panic!("expected legacy media card after the tool summary row");
        };
        assert_eq!(node.kind, RenderedMediaKind::Audio);
        assert_eq!(node.local_path, "D:/legacy.mp3");
    }
}
