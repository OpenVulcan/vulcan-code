//! Paint helpers for transcript selection highlights, rendered tables, and related hit-testing.
//! 转录选区高亮、渲染表格及相关命中测试的绘制辅助逻辑。

use super::RendererParagraph;
use super::interaction::{
    clamp_hit_test_point_to_paragraph_text_bounds, paragraph_hit_test_to_byte_offset,
};
use super::layout_engine::{
    paragraph_local_origin, translate_measured_table_node, translate_rectangle,
};
use super::scene::{
    LaidOutFlowNode, LaidOutMediaCardNode, MeasuredRenderedTableNode, RenderedMediaCardNode,
    RenderedMediaKind, RenderedParagraphNode, RenderedTableNode, RenderedTranscriptDocument,
};
use super::viewport_scene::MaterializedTranscriptScene;
use iced::advanced::Renderer as _;
use iced::advanced::image::{self, Renderer as _};
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::text::Paragraph as _;
use iced::advanced::text::Renderer as _;
use iced::widget::image::Handle as ImageHandle;
use iced::{Background, Border, Color, Point, Rectangle, Renderer, Size};

/// Draws the active selection portion intersecting one paragraph node.
/// 绘制与某个段落节点相交的当前活动选择区域。
pub(super) fn draw_paragraph_selection(
    renderer: &mut Renderer,
    document: &RenderedTranscriptDocument,
    paragraph_node: &RenderedParagraphNode,
    paragraph: &RendererParagraph,
    selection_start: usize,
    selection_end: usize,
    paragraph_origin: Point,
    viewport: &Rectangle,
) {
    let local_selection_start =
        selection_start.saturating_sub(paragraph_node.global_char_range.start);
    let local_selection_end = selection_end
        .min(paragraph_node.global_char_range.end)
        .saturating_sub(paragraph_node.global_char_range.start);

    if local_selection_start >= local_selection_end {
        return;
    }

    for (grapheme_index, grapheme_range) in paragraph_node.local_grapheme_ranges.iter().enumerate()
    {
        if grapheme_range.end <= local_selection_start
            || grapheme_range.start >= local_selection_end
        {
            continue;
        }

        for bounds in paragraph.span_bounds(grapheme_index) {
            let translated_bounds = Rectangle::new(
                bounds.position() + (paragraph_origin - Point::ORIGIN),
                bounds.size(),
            );

            if let Some(visible_bounds) = viewport.intersection(&translated_bounds) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: visible_bounds,
                        ..renderer::Quad::default()
                    },
                    Background::Color(document.selection_color),
                );
            }
        }
    }
}

/// Draws one rendered table node, including cell fills, grid lines, selections, and text.
/// 绘制一个渲染表格节点，包含单元格填充、网格线、选择高亮和文本。
pub(super) fn draw_rendered_table_node(
    renderer: &mut Renderer,
    document: &RenderedTranscriptDocument,
    defaults: &renderer::Style,
    selection: Option<(usize, usize)>,
    table_node: &RenderedTableNode,
    paragraphs: &[RendererParagraph],
    measured_table: &MeasuredRenderedTableNode,
    viewport: &Rectangle,
) {
    if table_node.quote_depth > 0 {
        let quote_bar_x = (measured_table.outer_bounds.x - 10.0).max(viewport.x);
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(quote_bar_x, measured_table.outer_bounds.y),
                    Size::new(4.0, measured_table.outer_bounds.height),
                ),
                ..renderer::Quad::default()
            },
            Background::Color(defaults.text_color.scale_alpha(0.18)),
        );
    }

    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let Some(cell_bounds) = measured_table.cell_bounds.get(cell_index).copied() else {
            continue;
        };
        let background = if cell_node.is_header {
            table_node.header_background
        } else {
            table_node.body_background
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: cell_bounds,
                ..renderer::Quad::default()
            },
            background,
        );

        if let Some((selection_start, selection_end)) = selection {
            draw_paragraph_selection(
                renderer,
                document,
                &cell_node.paragraph,
                &paragraphs[cell_index],
                selection_start,
                selection_end,
                measured_table.cell_text_origins[cell_index],
                viewport,
            );
        }

        renderer.fill_paragraph(
            &paragraphs[cell_index],
            measured_table.cell_text_origins[cell_index],
            cell_node
                .paragraph
                .spans
                .first()
                .and_then(|span| span.color)
                .unwrap_or(defaults.text_color),
            *viewport,
        );
    }

    renderer.fill_quad(
        renderer::Quad {
            bounds: measured_table.outer_bounds,
            border: Border {
                color: table_node.border_color,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..renderer::Quad::default()
        },
        Background::Color(Color::TRANSPARENT),
    );

    for (cell_index, cell_bounds) in measured_table.cell_bounds.iter().enumerate() {
        let Some(cell_node) = table_node.cells.get(cell_index) else {
            continue;
        };

        if cell_node.column_index > 0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(cell_bounds.x - 1.0, cell_bounds.y),
                        Size::new(1.0, cell_bounds.height),
                    ),
                    ..renderer::Quad::default()
                },
                Background::Color(table_node.border_color),
            );
        }

        if cell_node.row_index > 0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(cell_bounds.x, cell_bounds.y - 1.0),
                        Size::new(cell_bounds.width, 1.0),
                    ),
                    ..renderer::Quad::default()
                },
                Background::Color(table_node.border_color),
            );
        }
    }
}

/// Draws one inline generated-media card, including the preview chrome and host-facing metadata rows.
/// 绘制一个内联生成媒体卡片，包含预览外框与面向宿主的元信息行。
fn draw_rendered_media_card_node(
    renderer: &mut Renderer,
    defaults: &renderer::Style,
    media_card_node: &RenderedMediaCardNode,
    layout_node: &LaidOutMediaCardNode,
    scene: &MaterializedTranscriptScene<'_>,
) {
    let outer_bounds = translate_rectangle(layout_node.outer_bounds, scene.widget_origin);
    let preview_bounds = translate_rectangle(layout_node.preview_bounds, scene.widget_origin);
    let accent_background = media_card_node.accent_color.scale_alpha(0.16);
    let card_radius = 14.0;

    renderer.fill_quad(
        renderer::Quad {
            bounds: outer_bounds,
            border: Border {
                color: media_card_node.accent_color.scale_alpha(0.24),
                width: 1.0,
                radius: card_radius.into(),
            },
            ..renderer::Quad::default()
        },
        media_card_node.background.background,
    );

    renderer.fill_quad(
        renderer::Quad {
            bounds: preview_bounds,
            border: Border {
                color: media_card_node.accent_color.scale_alpha(0.18),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..renderer::Quad::default()
        },
        Background::Color(accent_background),
    );

    match media_card_node.kind {
        RenderedMediaKind::Image => {
            renderer.draw_image(
                image::Image::new(ImageHandle::from_path(media_card_node.local_path.clone())),
                preview_bounds,
                scene.viewport,
            );
        }
        RenderedMediaKind::Audio => {
            draw_audio_media_preview(renderer, media_card_node, preview_bounds);
        }
        RenderedMediaKind::Video => {
            draw_video_media_preview(renderer, media_card_node, preview_bounds, scene.viewport);
        }
        RenderedMediaKind::Pdf => {
            draw_document_media_preview(renderer, media_card_node, preview_bounds);
        }
    }

    if media_card_node.show_caption {
        let title_bounds = translate_rectangle(layout_node.title_bounds, scene.widget_origin);
        let meta_bounds = translate_rectangle(layout_node.meta_bounds, scene.widget_origin);
        let secondary_color = defaults.text_color.scale_alpha(0.68);
        let meta_text = if media_card_node.output_count > 1 {
            format!(
                "{} · {} 项输出",
                media_card_node.file_name, media_card_node.output_count
            )
        } else {
            media_card_node.file_name.clone()
        };

        renderer.fill_text(
            text::Text {
                content: media_card_node.title.clone(),
                bounds: title_bounds.size(),
                size: iced::Pixels(15.0),
                line_height: text::LineHeight::default(),
                font: iced::Font {
                    weight: iced::font::Weight::Semibold,
                    ..crate::iced_theme::ui_font()
                },
                align_x: text::Alignment::Left,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::Word,
                ellipsis: text::Ellipsis::default(),
                hint_factor: None,
            },
            title_bounds.position(),
            defaults.text_color,
            scene.viewport,
        );
        renderer.fill_text(
            text::Text {
                content: meta_text,
                bounds: meta_bounds.size(),
                size: iced::Pixels(12.0),
                line_height: text::LineHeight::default(),
                font: crate::iced_theme::ui_font(),
                align_x: text::Alignment::Left,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::Word,
                ellipsis: text::Ellipsis::default(),
                hint_factor: None,
            },
            meta_bounds.position(),
            secondary_color,
            scene.viewport,
        );
    }
}

/// Draws one audio-strip preview using a play pill and deterministic waveform bars.
/// 使用播放胶囊和确定性波形条绘制一个语音条预览。
fn draw_audio_media_preview(
    renderer: &mut Renderer,
    media_card_node: &RenderedMediaCardNode,
    preview_bounds: Rectangle,
) {
    let play_pill_bounds = Rectangle::new(
        Point::new(preview_bounds.x + 10.0, preview_bounds.y + 10.0),
        Size::new(42.0, (preview_bounds.height - 20.0).max(22.0)),
    );
    renderer.fill_quad(
        renderer::Quad {
            bounds: play_pill_bounds,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: (play_pill_bounds.height / 2.0).into(),
            },
            ..renderer::Quad::default()
        },
        Background::Color(media_card_node.accent_color.scale_alpha(0.92)),
    );

    if media_card_node.is_active {
        let stop_size = 13.0;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(
                        play_pill_bounds.x + play_pill_bounds.width / 2.0 - stop_size / 2.0,
                        play_pill_bounds.y + play_pill_bounds.height / 2.0 - stop_size / 2.0,
                    ),
                    Size::new(stop_size, stop_size),
                ),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 3.0.into(),
                },
                ..renderer::Quad::default()
            },
            Background::Color(Color::WHITE.scale_alpha(0.96)),
        );
    } else {
        draw_right_pointing_play_glyph(
            renderer,
            play_pill_bounds,
            12.0,
            Color::WHITE.scale_alpha(0.97),
        );
    }

    let bar_count = 22usize;
    let bars_left = play_pill_bounds.x + play_pill_bounds.width + 14.0;
    let bars_right = preview_bounds.x + preview_bounds.width - 14.0;
    let bar_spacing = ((bars_right - bars_left) / bar_count as f32).max(4.0);
    for index in 0..bar_count {
        let normalized = ((index * 17 + 11) % 29) as f32 / 28.0;
        let bar_height = (preview_bounds.height * (0.20 + normalized * 0.46)).max(8.0);
        let bar_x = bars_left + index as f32 * bar_spacing;
        let bar_y = preview_bounds.y + (preview_bounds.height - bar_height) / 2.0;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(Point::new(bar_x, bar_y), Size::new(3.0, bar_height)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 2.0.into(),
                },
                ..renderer::Quad::default()
            },
            Background::Color(media_card_node.accent_color.scale_alpha(0.76)),
        );
    }
}

/// Draws one video preview placeholder with a centered translucent play button overlay.
/// 绘制一个带居中半透明播放按钮覆盖层的视频预览占位卡片。
fn draw_video_media_preview(
    renderer: &mut Renderer,
    media_card_node: &RenderedMediaCardNode,
    preview_bounds: Rectangle,
    viewport: Rectangle,
) {
    if let Some(preview_local_path) = media_card_node.preview_image_handle.as_ref() {
        renderer.draw_image(
            image::Image::new(preview_local_path.clone()),
            preview_bounds,
            viewport,
        );
    }

    let play_bounds = Rectangle::new(
        Point::new(
            preview_bounds.x + preview_bounds.width / 2.0 - 31.0,
            preview_bounds.y + preview_bounds.height / 2.0 - 31.0,
        ),
        Size::new(62.0, 62.0),
    );
    renderer.fill_quad(
        renderer::Quad {
            bounds: play_bounds,
            border: Border {
                color: Color::WHITE.scale_alpha(0.88),
                width: 1.2,
                radius: 31.0.into(),
            },
            ..renderer::Quad::default()
        },
        Background::Color(Color::WHITE.scale_alpha(0.30)),
    );
    draw_right_pointing_play_glyph(renderer, play_bounds, 24.0, Color::WHITE.scale_alpha(0.98));
}

/// Draws one right-pointing play glyph using a symmetric stepped geometry so play and stop states share the same visual baseline.
/// 使用对称阶梯几何绘制一个朝右的播放图标，使播放与停止状态共享同一视觉基线。
fn draw_right_pointing_play_glyph(
    renderer: &mut Renderer,
    bounds: Rectangle,
    height: f32,
    color: Color,
) {
    let clamped_height = height.max(6.0);
    let column_width = (clamped_height / 6.0).clamp(2.0, 4.0);
    let total_width = column_width * 5.0;
    let origin = Point::new(
        bounds.x + bounds.width / 2.0 - total_width / 2.0,
        bounds.y + bounds.height / 2.0 - clamped_height / 2.0,
    );
    let triangle_columns = [
        (0.0, clamped_height),
        (column_width, clamped_height * 0.82),
        (column_width * 2.0, clamped_height * 0.64),
        (column_width * 3.0, clamped_height * 0.46),
        (column_width * 4.0, clamped_height * 0.28),
    ];

    for (offset_x, column_height) in triangle_columns {
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(
                        origin.x + offset_x,
                        origin.y + (clamped_height - column_height) / 2.0,
                    ),
                    Size::new(column_width, column_height.max(2.0)),
                ),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 1.0.into(),
                },
                ..renderer::Quad::default()
            },
            Background::Color(color),
        );
    }
}

/// Draws one document-style placeholder preview for PDF-like generated outputs.
/// 为 PDF 类生成输出绘制一个文档风格的占位预览。
fn draw_document_media_preview(
    renderer: &mut Renderer,
    media_card_node: &RenderedMediaCardNode,
    preview_bounds: Rectangle,
) {
    let icon_bounds = Rectangle::new(
        Point::new(preview_bounds.x + 12.0, preview_bounds.y + 8.0),
        Size::new(42.0, 40.0),
    );
    renderer.fill_quad(
        renderer::Quad {
            bounds: icon_bounds,
            border: Border {
                color: media_card_node.accent_color.scale_alpha(0.20),
                width: 1.0,
                radius: 10.0.into(),
            },
            ..renderer::Quad::default()
        },
        Background::Color(media_card_node.accent_color.scale_alpha(0.24)),
    );
    renderer.fill_text(
        text::Text {
            content: String::from("PDF"),
            bounds: icon_bounds.size(),
            size: iced::Pixels(13.0),
            line_height: text::LineHeight::default(),
            font: crate::iced_theme::code_font(),
            align_x: text::Alignment::Center,
            align_y: iced::alignment::Vertical::Center,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: None,
        },
        icon_bounds.position(),
        media_card_node.accent_color,
        preview_bounds,
    );
}

/// Draws panel backgrounds and all visible flow nodes for one transcript viewport.
/// 绘制单个转录视口中的面板背景以及全部可见流式节点。
pub(super) fn draw_visible_transcript_content(
    renderer: &mut Renderer,
    defaults: &renderer::Style,
    scene: &MaterializedTranscriptScene<'_>,
) {
    let mut panel_border_overlays = Vec::new();

    for panel_layout in scene.visible_panel_layouts() {
        let Some(panel_node) = scene.panel_node(panel_layout) else {
            continue;
        };
        let outer_bounds = translate_rectangle(panel_layout.outer_bounds, scene.widget_origin);
        renderer.fill_quad(
            renderer::Quad {
                bounds: outer_bounds,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: panel_node.background.border.radius,
                },
                ..renderer::Quad::default()
            },
            panel_node.background.background,
        );
        if panel_node.background.border.width > f32::EPSILON {
            panel_border_overlays.push((outer_bounds, panel_node.background.border));
        }
        if let Some(accent_rule) = panel_node.left_accent_rule {
            let accent_height =
                (outer_bounds.height - accent_rule.top_inset - accent_rule.bottom_inset).max(1.0);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(
                            outer_bounds.x,
                            outer_bounds.y + accent_rule.top_inset.max(0.0),
                        ),
                        Size::new(accent_rule.width.max(1.0), accent_height),
                    ),
                    ..renderer::Quad::default()
                },
                Background::Color(accent_rule.color),
            );
        }
    }

    for flow_layout in scene.visible_flow_layouts() {
        match flow_layout {
            LaidOutFlowNode::Collapsible(layout_node) => {
                let Some(collapsible_node) = scene.collapsible_node(layout_node) else {
                    continue;
                };
                let Some(summary_paragraph) = scene.paragraph(layout_node.summary_paragraph_index)
                else {
                    continue;
                };
                let summary_outer_bounds =
                    translate_rectangle(layout_node.summary_outer_bounds, scene.widget_origin);

                if let Some(background) = collapsible_node.summary.background {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: summary_outer_bounds,
                            border: background.border,
                            ..renderer::Quad::default()
                        },
                        background.background,
                    );
                }

                renderer.fill_paragraph(
                    summary_paragraph,
                    paragraph_local_origin(summary_outer_bounds, &collapsible_node.summary),
                    collapsible_node
                        .summary
                        .spans
                        .first()
                        .and_then(|span| span.color)
                        .unwrap_or(defaults.text_color),
                    scene.viewport,
                );

                if let (Some(body_node), Some(body_paragraph_index), Some(body_outer_bounds)) = (
                    collapsible_node.body.as_ref(),
                    layout_node.body_paragraph_index,
                    layout_node.body_outer_bounds,
                ) {
                    let Some(body_paragraph) = scene.paragraph(body_paragraph_index) else {
                        continue;
                    };
                    let body_outer_bounds =
                        translate_rectangle(body_outer_bounds, scene.widget_origin);
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: body_outer_bounds,
                            border: collapsible_node.body_background.border,
                            ..renderer::Quad::default()
                        },
                        collapsible_node.body_background.background,
                    );

                    let max_scroll =
                        (layout_node.body_full_text_height - body_outer_bounds.height).max(0.0);
                    let scroll_offset = scene
                        .collapsible_body_scroll_offset(&collapsible_node.block_id, max_scroll);
                    let text_clip_bounds = Rectangle::new(
                        Point::new(
                            body_outer_bounds.x + body_node.inner_padding.left,
                            body_outer_bounds.y + body_node.inner_padding.top,
                        ),
                        Size::new(
                            (body_outer_bounds.width
                                - body_node.inner_padding.left
                                - body_node.inner_padding.right
                                - if max_scroll > 0.0 { 10.0 } else { 0.0 })
                            .max(1.0),
                            (body_outer_bounds.height
                                - body_node.inner_padding.top
                                - body_node.inner_padding.bottom)
                                .max(1.0),
                        ),
                    );

                    renderer.with_layer(text_clip_bounds, |renderer| {
                        renderer.fill_paragraph(
                            body_paragraph,
                            Point::new(text_clip_bounds.x, text_clip_bounds.y - scroll_offset),
                            body_node
                                .spans
                                .first()
                                .and_then(|span| span.color)
                                .unwrap_or(defaults.text_color),
                            scene.viewport,
                        );
                    });

                    if max_scroll > f32::EPSILON {
                        let track_bounds = Rectangle::new(
                            Point::new(
                                body_outer_bounds.x + body_outer_bounds.width - 6.0,
                                body_outer_bounds.y + 6.0,
                            ),
                            Size::new(3.0, (body_outer_bounds.height - 12.0).max(18.0)),
                        );
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: track_bounds,
                                border: Border {
                                    color: Color::TRANSPARENT,
                                    width: 0.0,
                                    radius: 2.0.into(),
                                },
                                ..renderer::Quad::default()
                            },
                            Background::Color(collapsible_node.scrollbar_track_color),
                        );

                        let thumb_height = ((track_bounds.height
                            * (body_outer_bounds.height / layout_node.body_full_text_height))
                            .clamp(18.0, track_bounds.height))
                        .max(18.0);
                        let travel = (track_bounds.height - thumb_height).max(0.0);
                        let thumb_y = track_bounds.y
                            + if max_scroll <= f32::EPSILON {
                                0.0
                            } else {
                                (scroll_offset / max_scroll) * travel
                            };
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(track_bounds.x, thumb_y),
                                    Size::new(track_bounds.width, thumb_height),
                                ),
                                border: Border {
                                    color: Color::TRANSPARENT,
                                    width: 0.0,
                                    radius: 2.0.into(),
                                },
                                ..renderer::Quad::default()
                            },
                            Background::Color(collapsible_node.scrollbar_thumb_color),
                        );
                    }
                }
            }
            LaidOutFlowNode::Paragraph(layout_node) => {
                let Some(paragraph_node) = scene.paragraph_node(layout_node) else {
                    continue;
                };
                let Some(paragraph) = scene.paragraph(layout_node.paragraph_index) else {
                    continue;
                };
                let outer_bounds =
                    translate_rectangle(layout_node.outer_bounds, scene.widget_origin);

                if let Some(background) = paragraph_node.background {
                    let background_bounds = if background.border.width <= f32::EPSILON {
                        Rectangle::new(
                            outer_bounds.position(),
                            Size::new(
                                outer_bounds.width,
                                outer_bounds.height
                                    + (layout_node.flow_end_y
                                        - layout_node.outer_bounds.y
                                        - layout_node.outer_bounds.height),
                            ),
                        )
                    } else {
                        outer_bounds
                    };
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: background_bounds,
                            border: background.border,
                            ..renderer::Quad::default()
                        },
                        background.background,
                    );
                }

                if paragraph_node.quote_depth > 0 {
                    let quote_bar_x = (outer_bounds.x - 10.0).max(scene.widget_bounds.x);
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(quote_bar_x, outer_bounds.y),
                                Size::new(4.0, outer_bounds.height),
                            ),
                            ..renderer::Quad::default()
                        },
                        Background::Color(defaults.text_color.scale_alpha(0.18)),
                    );
                }

                if let Some((selection_start, selection_end)) = scene.selection {
                    draw_paragraph_selection(
                        renderer,
                        scene.document,
                        paragraph_node,
                        paragraph,
                        selection_start,
                        selection_end,
                        paragraph_local_origin(outer_bounds, paragraph_node),
                        &scene.viewport,
                    );
                }

                renderer.fill_paragraph(
                    paragraph,
                    paragraph_local_origin(outer_bounds, paragraph_node),
                    paragraph_node
                        .spans
                        .first()
                        .and_then(|span| span.color)
                        .unwrap_or(defaults.text_color),
                    scene.viewport,
                );
            }
            LaidOutFlowNode::Table(layout_node) => {
                let Some(table_node) = scene.table_node(layout_node) else {
                    continue;
                };
                let Some(table_layout) = scene.table_layout(layout_node.table_index) else {
                    continue;
                };
                let table_paragraphs = scene.table_paragraphs(table_layout);
                let measured_table =
                    translate_measured_table_node(&layout_node.measured_table, scene.widget_origin);

                draw_rendered_table_node(
                    renderer,
                    scene.document,
                    defaults,
                    scene.selection,
                    table_node,
                    table_paragraphs,
                    &measured_table,
                    &scene.viewport,
                );
            }
            LaidOutFlowNode::MediaCard(layout_node) => {
                let Some(media_card_node) = scene.media_card_node(layout_node) else {
                    continue;
                };
                draw_rendered_media_card_node(
                    renderer,
                    defaults,
                    media_card_node,
                    layout_node,
                    scene,
                );
            }
            LaidOutFlowNode::Rule(layout_node) => {
                let Some(rule_node) = scene.rule_node(layout_node) else {
                    continue;
                };
                let outer_bounds =
                    translate_rectangle(layout_node.outer_bounds, scene.widget_origin);
                let rule_y = outer_bounds.y + 3.0;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(
                            Point::new(outer_bounds.x, rule_y),
                            Size::new(outer_bounds.width.max(1.0), 2.0),
                        ),
                        ..renderer::Quad::default()
                    },
                    Background::Color(rule_node.color),
                );
            }
        }
    }

    for (bounds, border) in panel_border_overlays {
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border,
                ..renderer::Quad::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
    }
}

/// Hit-tests one rendered table node and returns the nearest global character offset when a cell is hit.
/// 对一个渲染表格节点执行命中测试，并在命中单元格时返回最近的全局字符偏移。
pub(super) fn hit_test_rendered_table_node(
    table_node: &RenderedTableNode,
    paragraphs: &[RendererParagraph],
    measured_table: &MeasuredRenderedTableNode,
    position: Point,
) -> Option<usize> {
    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let cell_bounds = *measured_table.cell_bounds.get(cell_index)?;

        if !cell_bounds.contains(position) {
            continue;
        }

        let paragraph = paragraphs.get(cell_index)?;
        let paragraph_height = paragraph.min_bounds().height;
        let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
            Point::new(
                position.x - cell_bounds.x - table_node.cell_padding.left,
                position.y - cell_bounds.y - table_node.cell_padding.top,
            ),
            &cell_node.paragraph,
            cell_bounds.size(),
            paragraph_height,
        );

        let local_byte_offset = paragraph_hit_test_to_byte_offset(
            paragraph,
            &cell_node.paragraph.display_text,
            local_point,
        )?;
        let local_char_offset =
            super::byte_offset_to_char_offset(&cell_node.paragraph.display_text, local_byte_offset);
        return Some(
            (cell_node.paragraph.global_char_range.start + local_char_offset)
                .min(cell_node.paragraph.global_char_range.end),
        );
    }

    None
}
