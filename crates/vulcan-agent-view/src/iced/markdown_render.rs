//! Markdown rendering pipeline helpers shared by transcript directive and prose surfaces.
//! 转录指令区与正文表面共享的 Markdown 渲染管线辅助逻辑。

use super::builder::RenderedTranscriptBuilder;
use super::directive_render::{
    classify_diff_line_kind, diff_line_color, directive_background, mix_color,
};
use super::markdown_source::*;
use super::scene::{
    MarkdownRenderKind, PlainParagraphStyle, PlainTextRenderKind, RenderContext, RenderedTableStyle,
};
use super::text_routing::{build_agent_view_markdown_spans, build_agent_view_plain_text_spans};
use crate::iced_theme as theme;
use iced::advanced::text;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{Background, Color, Font, Padding, Pixels, Theme};

/// Renders one slice of Markdown items recursively into the transcript builder.
/// 将一段 Markdown 节点递归渲染到转录构建器中。
pub(super) fn render_markdown_items(
    builder: &mut RenderedTranscriptBuilder,
    items: &[markdown::Item],
    context: RenderContext,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let flow_gap_after =
        if context.block_background.is_some() || context.surface_background.is_some() {
            4.0
        } else {
            8.0
        };
    for item in items {
        match item {
            markdown::Item::Heading(level, text) => {
                let mut spans = build_agent_view_markdown_spans(
                    text,
                    settings.style,
                    MarkdownRenderKind::Flow,
                    app_theme,
                );
                if context.suppress_embedded_backgrounds {
                    for span in &mut spans {
                        span.highlight = None;
                    }
                }
                if let Some(foreground_color) = context.foreground_color {
                    for span in &mut spans {
                        if span.link.is_none() {
                            span.color = Some(foreground_color);
                        }
                    }
                }
                builder.push_rich_paragraph(
                    spans,
                    heading_size(settings, level),
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    context.inner_padding,
                    context.block_background,
                    context.quote_depth,
                );
            }
            markdown::Item::Paragraph(text) => {
                let mut spans = build_agent_view_markdown_spans(
                    text,
                    settings.style,
                    MarkdownRenderKind::Flow,
                    app_theme,
                );
                if context.suppress_embedded_backgrounds {
                    for span in &mut spans {
                        span.highlight = None;
                    }
                }
                if let Some(foreground_color) = context.foreground_color {
                    for span in &mut spans {
                        if span.link.is_none() {
                            span.color = Some(foreground_color);
                        }
                    }
                }
                builder.push_rich_paragraph(
                    spans,
                    settings.text_size,
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    context.inner_padding,
                    context.block_background,
                    context.quote_depth,
                );
            }
            markdown::Item::CodeBlock {
                language,
                code,
                lines,
            } => {
                let code_background = if context.suppress_embedded_backgrounds {
                    None
                } else {
                    context
                        .block_background
                        .map(|background| {
                            directive_background(
                                match background.background {
                                    Background::Color(color) => {
                                        mix_color(color, Color::BLACK.scale_alpha(0.12), 0.08)
                                    }
                                    _ => {
                                        app_theme.palette().background.weak.color.scale_alpha(0.22)
                                    }
                                },
                                background.border.color.scale_alpha(
                                    if background.border.width <= f32::EPSILON {
                                        0.0
                                    } else {
                                        0.72
                                    },
                                ),
                            )
                        })
                        .or_else(|| {
                            Some(directive_background(
                                app_theme.palette().background.weak.color.scale_alpha(0.24),
                                app_theme
                                    .palette()
                                    .background
                                    .strong
                                    .color
                                    .scale_alpha(0.34),
                            ))
                        })
                };
                if is_markdown_code_block_language(language.as_deref()) {
                    let preview_inner_padding = if code_background.is_some() {
                        Padding::from([8.0, 10.0])
                    } else {
                        context.inner_padding
                    };
                    let child_start_index = builder.node_count();
                    let preview_source = prepare_markdown_source_for_rendering(code);
                    let preview_context = RenderContext {
                        indent_left: context.indent_left,
                        quote_depth: context.quote_depth,
                        block_background: None,
                        surface_background: code_background,
                        foreground_color: context.foreground_color,
                        inner_padding: preview_inner_padding,
                        suppress_embedded_backgrounds: false,
                    };

                    if raw_source_contains_markdown_table(&preview_source) {
                        render_markdown_source_with_table_fallback(
                            builder,
                            &preview_source,
                            PlainParagraphStyle {
                                indent_left: context.indent_left,
                                gap_after: flow_gap_after,
                                color: context.foreground_color,
                                background: None,
                                surface_background: code_background,
                                inner_padding: preview_inner_padding,
                                suppress_embedded_backgrounds: false,
                                quote_depth: context.quote_depth,
                                ..PlainParagraphStyle::flow(settings.text_size)
                            },
                            settings,
                            app_theme,
                        );
                    } else {
                        let normalized_preview =
                            normalize_markdown_source_for_transcript_display(&preview_source);
                        let preview_markdown = markdown::Content::parse(&normalized_preview);
                        render_markdown_items(
                            builder,
                            preview_markdown.items(),
                            preview_context,
                            settings,
                            app_theme,
                        );
                    }

                    if let Some(panel_background) = code_background {
                        let child_node_count =
                            builder.node_count().saturating_sub(child_start_index);
                        builder.insert_panel(child_start_index, child_node_count, panel_background);
                    }
                    continue;
                }
                builder.push_rich_paragraph(
                    build_multiline_markdown_code_spans(
                        lines,
                        settings.style,
                        MarkdownRenderKind::CodeBlock,
                        app_theme,
                    ),
                    settings.code_size,
                    text::LineHeight::default(),
                    text::Wrapping::WordOrGlyph,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    if code_background.is_some() {
                        Padding::from([8.0, 10.0])
                    } else {
                        context.inner_padding
                    },
                    code_background,
                    context.quote_depth,
                );
            }
            markdown::Item::List { start, bullets } => {
                render_markdown_list(builder, bullets, *start, context, settings, app_theme);
            }
            markdown::Item::Quote(quote_items) => {
                render_markdown_items(
                    builder,
                    quote_items,
                    RenderContext {
                        indent_left: context.indent_left + 18.0,
                        quote_depth: context.quote_depth + 1,
                        block_background: context.block_background,
                        surface_background: context.surface_background,
                        foreground_color: context.foreground_color,
                        inner_padding: context.inner_padding,
                        suppress_embedded_backgrounds: context.suppress_embedded_backgrounds,
                    },
                    settings,
                    app_theme,
                );
            }
            markdown::Item::Rule => {
                builder.push_rule(
                    app_theme
                        .palette()
                        .background
                        .strong
                        .color
                        .scale_alpha(0.35),
                );
            }
            markdown::Item::Image { alt, url, .. } => {
                builder.push_rich_paragraph(
                    build_markdown_image_spans(alt, url, settings.style, app_theme),
                    settings.text_size,
                    text::LineHeight::default(),
                    text::Wrapping::Word,
                    context.indent_left,
                    0.0,
                    flow_gap_after,
                    context.inner_padding,
                    context.block_background,
                    context.quote_depth,
                );
            }
            markdown::Item::Table { .. } => {
                builder.push_plain_paragraph(
                    String::from("[表格内容当前使用保底文本渲染]"),
                    PlainParagraphStyle::flow(settings.text_size),
                    app_theme,
                );
            }
        }
    }
}

/// Builds one readable image paragraph span list so non-web transcript surfaces still preserve alt text and a clickable source URL.
/// 构建一组可读的图片段落片段，使非网页转录表面仍能保留替代文本和可点击的源地址。
pub(super) fn build_markdown_image_spans(
    alt: &markdown::Text,
    url: &markdown::Uri,
    style: markdown::Style,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let mut spans = build_agent_view_plain_text_spans(
        "[图片] ",
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        None,
        app_theme,
    );
    let mut alt_spans =
        build_agent_view_markdown_spans(alt, style, MarkdownRenderKind::Flow, app_theme);
    let alt_is_empty = alt_spans.iter().all(|span| span.text.trim().is_empty());

    if !alt_is_empty {
        spans.append(&mut alt_spans);
        spans.extend(build_agent_view_plain_text_spans(
            " ",
            theme::ui_font(),
            PlainTextRenderKind::Flow,
            None,
            app_theme,
        ));
    }

    let mut url_spans = build_agent_view_plain_text_spans(
        url.as_ref(),
        theme::ui_font(),
        PlainTextRenderKind::Flow,
        Some(style.link_color),
        app_theme,
    );
    for span in &mut url_spans {
        span.link = Some(url.clone());
    }
    spans.append(&mut url_spans);

    spans
}

/// Renders one Markdown list while preserving bullet or ordered prefixes in the visible transcript.
/// 渲染一个 Markdown 列表，同时在可见转录中保留项目符号或有序前缀。
pub(super) const MARKDOWN_LIST_INDENT_STEP: f32 = 20.0;
const MARKDOWN_ORDERED_LIST_MIN_DIGITS: usize = 2;

/// Returns the stable ordered-list prefix text used by the transcript renderer so single-digit and multi-digit items align to one shared gutter.
/// 返回转录渲染器使用的稳定有序列表前缀文本，使单数位和多位数字条目对齐到同一前导槽位。
pub(super) fn ordered_list_prefix_text(start: u64, index: usize, item_count: usize) -> String {
    let ordinal = start + index as u64;
    let last_ordinal = start + item_count.saturating_sub(1) as u64;
    let digit_width = decimal_digit_count(last_ordinal).max(MARKDOWN_ORDERED_LIST_MIN_DIGITS);

    format!("{ordinal:>digit_width$}. ")
}

/// Returns the decimal digit count of one non-negative integer so ordered-list prefixes can reserve one stable width.
/// 返回某个非负整数的十进制位数，以便有序列表前缀预留稳定宽度。
fn decimal_digit_count(value: u64) -> usize {
    value
        .checked_ilog10()
        .map(|digits| digits as usize + 1)
        .unwrap_or(1)
}

pub(super) fn render_markdown_list(
    builder: &mut RenderedTranscriptBuilder,
    bullets: &[markdown::Bullet],
    start: Option<u64>,
    context: RenderContext,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let list_text_color = context
        .foreground_color
        .unwrap_or(app_theme.palette().background.base.text);
    let list_gap_after = if context.block_background.is_some() {
        4.0
    } else {
        6.0
    };

    for (index, bullet) in bullets.iter().enumerate() {
        let bullet_prefix = match bullet {
            markdown::Bullet::Point { .. } => String::from("• "),
            markdown::Bullet::Task { done, .. } => {
                format!("[{}] ", if *done { "x" } else { " " })
            }
        };
        let ordered_prefix = start
            .map(|start| ordered_list_prefix_text(start, index, bullets.len()))
            .unwrap_or_default();
        let prefix_text = if ordered_prefix.is_empty() {
            bullet_prefix
        } else {
            ordered_prefix
        };

        let items = match bullet {
            markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => items,
        };
        let mut rendered_anything = false;

        for item in items {
            match item {
                markdown::Item::Paragraph(text) | markdown::Item::Heading(_, text) => {
                    let mut spans = build_agent_view_markdown_spans(
                        text,
                        settings.style,
                        MarkdownRenderKind::Flow,
                        app_theme,
                    );
                    if context.suppress_embedded_backgrounds {
                        for span in &mut spans {
                            span.highlight = None;
                        }
                    }
                    if let Some(foreground_color) = context.foreground_color {
                        for span in &mut spans {
                            if span.link.is_none() {
                                span.color = Some(foreground_color);
                            }
                        }
                    }
                    let mut prefix_spans = build_agent_view_plain_text_spans(
                        &prefix_text,
                        theme::ui_font(),
                        PlainTextRenderKind::Flow,
                        Some(list_text_color),
                        app_theme,
                    );
                    prefix_spans.append(&mut spans);
                    builder.push_rich_paragraph(
                        prefix_spans,
                        settings.text_size,
                        text::LineHeight::default(),
                        text::Wrapping::Word,
                        context.indent_left + MARKDOWN_LIST_INDENT_STEP,
                        0.0,
                        list_gap_after,
                        context.inner_padding,
                        context.block_background,
                        context.quote_depth,
                    );
                    rendered_anything = true;
                }
                _ => {
                    render_markdown_items(
                        builder,
                        std::slice::from_ref(item),
                        RenderContext {
                            indent_left: context.indent_left + MARKDOWN_LIST_INDENT_STEP,
                            quote_depth: context.quote_depth,
                            block_background: context.block_background,
                            surface_background: context.surface_background,
                            foreground_color: context.foreground_color,
                            inner_padding: context.inner_padding,
                            suppress_embedded_backgrounds: context.suppress_embedded_backgrounds,
                        },
                        settings,
                        app_theme,
                    );
                    rendered_anything = true;
                }
            }
        }

        if !rendered_anything {
            builder.push_plain_paragraph(
                prefix_text,
                PlainParagraphStyle {
                    indent_left: context.indent_left + MARKDOWN_LIST_INDENT_STEP,
                    gap_after: list_gap_after,
                    color: Some(list_text_color),
                    background: context.block_background,
                    surface_background: context.surface_background,
                    inner_padding: context.inner_padding,
                    suppress_embedded_backgrounds: context.suppress_embedded_backgrounds,
                    quote_depth: context.quote_depth,
                    ..PlainParagraphStyle::flow(settings.text_size)
                },
                app_theme,
            );
        }
    }
}

/// Returns whether the fenced-code language should be rendered as embedded Markdown preview instead of raw code.
/// 返回围栏代码语言是否应作为内嵌 Markdown 预览渲染，而不是按原始代码显示。
pub(super) fn is_markdown_code_block_language(language: Option<&str>) -> bool {
    language.map(str::trim).is_some_and(|language| {
        language.eq_ignore_ascii_case("md") || language.eq_ignore_ascii_case("markdown")
    })
}

/// Builds one multiline Markdown-code span list so visually boxed code regions behave like one continuous selectable paragraph.
/// 构建一组多行 Markdown 代码片段，使带背景的代码区域按一个连续可选段落工作。
pub(super) fn build_multiline_markdown_code_spans(
    lines: &[markdown::Text],
    style: markdown::Style,
    render_kind: MarkdownRenderKind,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    if lines.is_empty() {
        return build_agent_view_plain_text_spans(
            " ",
            theme::code_font(),
            PlainTextRenderKind::Code,
            None,
            app_theme,
        );
    }

    let mut output = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        if line_index > 0 {
            output.extend(build_agent_view_plain_text_spans(
                "\n",
                style.code_block_font,
                PlainTextRenderKind::Code,
                None,
                app_theme,
            ));
        }

        output.extend(build_agent_view_markdown_spans(
            line,
            style,
            render_kind,
            app_theme,
        ));
    }

    output
}

/// Builds one multiline diff span list so boxed diff regions keep per-line colors while remaining one continuous selectable paragraph.
/// 构建一组多行 diff 片段，使带背景的 diff 区域在保留逐行颜色的同时仍是连续可选段落。
pub(super) fn build_multiline_diff_spans(
    diff_body: &str,
    app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let source_lines = if diff_body.is_empty() {
        vec![""]
    } else {
        diff_body.split('\n').collect::<Vec<_>>()
    };
    let mut output = Vec::new();

    for (line_index, line) in source_lines.iter().enumerate() {
        if line_index > 0 {
            output.extend(build_agent_view_plain_text_spans(
                "\n",
                theme::code_font(),
                PlainTextRenderKind::Code,
                None,
                app_theme,
            ));
        }

        output.extend(build_agent_view_plain_text_spans(
            line,
            theme::code_font(),
            PlainTextRenderKind::Code,
            Some(diff_line_color(app_theme, classify_diff_line_kind(line))),
            app_theme,
        ));
    }

    output
}

/// Renders one Markdown source slice with a source-aware table fallback so visible table content stays selectable.
/// 使用源码感知的表格回退渲染一段 Markdown 源文本，从而让可见表格内容保持可选择。
pub(super) fn render_markdown_source_with_table_fallback(
    builder: &mut RenderedTranscriptBuilder,
    markdown_source: &str,
    paragraph_style: PlainParagraphStyle,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let repaired_source = repair_compact_markdown_tables(markdown_source);
    let lines = repaired_source.lines().collect::<Vec<_>>();
    let mut buffered_plain_lines = Vec::new();
    let mut line_index = 0usize;
    let mut active_fence_marker = None::<&'static str>;

    while line_index < lines.len() {
        let trimmed_line = lines[line_index].trim_start();

        if let Some(active_marker) = active_fence_marker {
            buffered_plain_lines.push(lines[line_index]);
            if trimmed_line.starts_with(active_marker) {
                active_fence_marker = None;
            }
            line_index += 1;
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            active_fence_marker = Some(fence_marker);
            buffered_plain_lines.push(lines[line_index]);
            line_index += 1;
            continue;
        }

        if is_markdown_table_header_line(lines[line_index])
            && lines
                .get(line_index + 1)
                .is_some_and(|line| is_markdown_table_alignment_line(line))
        {
            flush_plain_markdown_fallback_buffer(
                builder,
                &mut buffered_plain_lines,
                paragraph_style,
                settings,
                app_theme,
            );

            let mut table_lines = vec![lines[line_index], lines[line_index + 1]];
            line_index += 2;

            while line_index < lines.len() && is_markdown_table_body_line(lines[line_index]) {
                table_lines.push(lines[line_index]);
                line_index += 1;
            }

            if let Some(parsed_table) = parse_markdown_table_source(&table_lines) {
                builder.push_table(
                    parsed_table,
                    rendered_table_style(paragraph_style, settings, app_theme),
                    app_theme,
                );
            } else {
                builder.push_plain_paragraph(
                    table_lines.into_iter().collect::<Vec<_>>().join("\n"),
                    PlainParagraphStyle {
                        text_size: settings.code_size,
                        line_height: text::LineHeight::default(),
                        wrapping: text::Wrapping::None,
                        indent_left: paragraph_style.indent_left,
                        horizontal_placement: paragraph_style.horizontal_placement,
                        gap_before: paragraph_style.gap_before,
                        gap_after: paragraph_style.gap_after.max(8.0),
                        color: paragraph_style.color,
                        render_kind: PlainTextRenderKind::Code,
                        fit_width_to_content: paragraph_style.fit_width_to_content,
                        max_width_ratio: paragraph_style.max_width_ratio,
                        background: paragraph_style.background,
                        surface_background: paragraph_style.surface_background,
                        inner_padding: paragraph_style.inner_padding,
                        suppress_embedded_backgrounds: paragraph_style
                            .suppress_embedded_backgrounds,
                        quote_depth: paragraph_style.quote_depth,
                    },
                    app_theme,
                );
            }
            continue;
        }

        buffered_plain_lines.push(lines[line_index]);
        line_index += 1;
    }

    flush_plain_markdown_fallback_buffer(
        builder,
        &mut buffered_plain_lines,
        paragraph_style,
        settings,
        app_theme,
    );
}

/// Flushes one buffered plain Markdown fallback chunk into the rendered transcript builder while preserving source newlines.
/// 将一段缓冲中的普通 Markdown 回退片段刷新到渲染转录构建器中，并保留源码换行。
fn flush_plain_markdown_fallback_buffer(
    builder: &mut RenderedTranscriptBuilder,
    buffered_plain_lines: &mut Vec<&str>,
    paragraph_style: PlainParagraphStyle,
    settings: markdown::Settings,
    app_theme: &Theme,
) {
    let markdown_source = buffered_plain_lines.join("\n");
    buffered_plain_lines.clear();

    if markdown_source.trim().is_empty() {
        return;
    }

    let prepared_source = rewrite_markdown_footnotes_for_transcript(&markdown_source);
    let normalized_source = normalize_markdown_source_for_transcript_display(&prepared_source);
    let markdown_content = markdown::Content::parse(&normalized_source);
    render_markdown_items(
        builder,
        markdown_content.items(),
        RenderContext {
            indent_left: paragraph_style.indent_left,
            quote_depth: paragraph_style.quote_depth,
            block_background: paragraph_style.background,
            surface_background: paragraph_style.surface_background,
            foreground_color: paragraph_style.color,
            inner_padding: paragraph_style.inner_padding,
            suppress_embedded_backgrounds: paragraph_style.suppress_embedded_backgrounds,
        },
        settings,
        app_theme,
    );
}

/// Builds the default rendered-table style from the surrounding paragraph context and current theme.
/// 根据周围段落上下文和当前主题构建默认渲染表格样式。
pub(super) fn rendered_table_style(
    paragraph_style: PlainParagraphStyle,
    settings: markdown::Settings,
    app_theme: &Theme,
) -> RenderedTableStyle {
    let palette = app_theme.palette();
    let inherited_background = paragraph_style
        .surface_background
        .or(paragraph_style.background)
        .map(|background| background.background);
    let body_background = if paragraph_style.suppress_embedded_backgrounds {
        match inherited_background {
            Some(Background::Color(color)) => {
                Background::Color(mix_color(color, Color::WHITE.scale_alpha(0.06), 0.04))
            }
            Some(background) => background,
            None => Background::Color(Color::TRANSPARENT),
        }
    } else {
        inherited_background
            .unwrap_or_else(|| Background::Color(palette.background.base.color.scale_alpha(0.98)))
    };
    let body_color = paragraph_style
        .color
        .unwrap_or(palette.background.base.text.scale_alpha(0.96));
    let header_background = if paragraph_style.suppress_embedded_backgrounds {
        match body_background {
            Background::Color(color) if color != Color::TRANSPARENT => {
                Background::Color(mix_color(color, Color::WHITE.scale_alpha(0.18), 0.16))
            }
            _ => Background::Color(Color::TRANSPARENT),
        }
    } else {
        match body_background {
            Background::Color(color) => {
                Background::Color(mix_color(color, Color::WHITE.scale_alpha(0.16), 0.14))
            }
            _ => Background::Color(palette.background.weak.color.scale_alpha(0.88)),
        }
    };

    RenderedTableStyle {
        text_size: settings.text_size,
        text_color: body_color,
        indent_left: paragraph_style.indent_left,
        gap_before: paragraph_style.gap_before,
        gap_after: paragraph_style.gap_after.max(8.0),
        cell_padding: Padding::from([8.0, 10.0]),
        outer_padding: if paragraph_style.surface_background.is_some() {
            Padding {
                top: 0.0,
                right: paragraph_style.inner_padding.right,
                bottom: 0.0,
                left: paragraph_style.inner_padding.left,
            }
        } else {
            Padding::ZERO
        },
        border_color: body_color.scale_alpha(if paragraph_style.suppress_embedded_backgrounds {
            0.28
        } else {
            0.22
        }),
        header_background,
        body_background,
        quote_depth: paragraph_style.quote_depth,
    }
}

/// Returns the heading text size matching the provided Markdown heading level.
/// 返回与给定 Markdown 标题级别匹配的标题字号。
pub(super) fn heading_size(settings: markdown::Settings, level: &markdown::HeadingLevel) -> Pixels {
    match level {
        markdown::HeadingLevel::H1 => settings.h1_size,
        markdown::HeadingLevel::H2 => settings.h2_size,
        markdown::HeadingLevel::H3 => settings.h3_size,
        markdown::HeadingLevel::H4 => settings.h4_size,
        markdown::HeadingLevel::H5 => settings.h5_size,
        markdown::HeadingLevel::H6 => settings.h6_size,
    }
}
