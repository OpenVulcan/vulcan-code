//! Directive-block rendering helpers for semantic transcript panels, inline thinking, and user-prompt chrome.
//! 语义转录面板、内联思考与用户提示词外框共用的指令区块渲染辅助逻辑。

use super::builder::RenderedTranscriptBuilder;
use super::markdown_render::{
    build_multiline_diff_spans, render_markdown_items, render_markdown_source_with_table_fallback,
};
use super::markdown_source::{
    normalize_markdown_source_for_transcript_display, prepare_markdown_source_for_rendering,
    raw_source_contains_markdown_table,
};
use super::scene::{
    PlainParagraphStyle, PlainTextRenderKind, RenderContext, RenderedCollapsibleDirectiveNode,
    RenderedParagraphBackground, RenderedParagraphHorizontalPlacement,
    RenderedTranscriptLinkAction, RenderedTranscriptNode,
};
use super::text_routing::build_agent_view_plain_text_spans;
use crate::iced_theme as theme;
use crate::{AgentViewDirectiveBlock, AgentViewDirectiveKind};
use iced::advanced::text;
use iced::font;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{Background, Border, Color, Font, Padding, Pixels, Theme};
use time::{OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description};

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
                let normalized_body =
                    normalize_markdown_source_for_transcript_display(&prepared_body);
                let body_markdown = markdown::Content::parse(&normalized_body);
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

/// Renders one compact tool row that opens a dedicated modal instead of expanding inline.
/// 渲染一条会打开专用弹窗而不是在正文中内联展开的紧凑工具行。
fn render_compact_tool_modal_directive_block(
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

    let (icon, summary_color, fallback_title) = match block.kind {
        AgentViewDirectiveKind::ToolCall => ("🛠", iced::color!(0xA8D5FF), String::from("调用工具")),
        AgentViewDirectiveKind::ToolResult => {
            ("📦", iced::color!(0xAEE8C8), String::from("工具返回"))
        }
        _ => return,
    };
    let title = block
        .attributes
        .get("title")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_title);
    let summary_text = format!("{icon} {title}");
    let summary_spans = build_agent_view_plain_text_spans(
        &summary_text,
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(summary_color),
        app_theme,
    );
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
        AgentViewDirectiveKind::ToolCall => (
            "🛠",
            iced::color!(0xCDE7FF),
            directive_background(
                palette.primary.weak.color.scale_alpha(0.08),
                palette.primary.strong.color.scale_alpha(0.24),
            ),
            palette.background.base.text,
        ),
        AgentViewDirectiveKind::ToolResult => (
            "📦",
            iced::color!(0xD7F6E6),
            directive_background(
                palette.success.weak.color.scale_alpha(0.08),
                palette.success.strong.color.scale_alpha(0.24),
            ),
            palette.background.base.text,
        ),
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
    let summary_spans = build_agent_view_plain_text_spans(
        &summary_text,
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(summary_color),
        app_theme,
    );
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

/// Renders one thinking block with a full-width dark panel treatment; inline rows open the dedicated modal instead of expanding in place.
/// 使用全宽深色面板样式渲染单个思考区块；内联标题行会直接打开专用弹层，而不会在正文中原地展开。
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
    let is_inline_surface = thinking_surface == "inline";
    let is_inline_collapsed = is_inline_surface
        && block
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

    if is_inline_collapsed {
        return;
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
        let normalized_body = normalize_markdown_source_for_transcript_display(&prepared_body);
        let body_markdown = markdown::Content::parse(&normalized_body);
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

/// Renders one user-prompt directive block as a right-aligned bubble followed by a right-aligned timestamp and copy action row.
/// 将单个用户提示词指令区块渲染为右对齐气泡，并在其下方追加右对齐的时间戳与复制动作行。
fn render_user_prompt_block(
    builder: &mut RenderedTranscriptBuilder,
    block: &AgentViewDirectiveBlock,
    markdown_settings: markdown::Settings,
    app_theme: &Theme,
) {
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

    let copy_action_link = user_prompt_copy_action_link(block);
    if let Some(copy_action_link) = copy_action_link.as_ref() {
        builder.register_link_action(
            copy_action_link.clone(),
            RenderedTranscriptLinkAction::CopyText(block.body.clone()),
        );
    }

    let meta_spans = build_user_prompt_meta_spans(
        block,
        copy_action_link.as_deref(),
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

/// Builds the timestamp and copy-action spans shown under one right-aligned user-prompt bubble.
/// 构建显示在右对齐用户提示词气泡下方的时间戳与复制动作片段。
pub(super) fn build_user_prompt_meta_spans(
    block: &AgentViewDirectiveBlock,
    copy_action_link: Option<&str>,
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
    let message_id = block.attributes.get("message-id")?.trim();
    (!message_id.is_empty()).then(|| format!("{USER_PROMPT_COPY_ACTION_PREFIX}{message_id}"))
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
