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
    LaidOutFlowNode, MeasuredRenderedTableNode, RenderedParagraphNode, RenderedTableNode,
    RenderedTranscriptDocument,
};
use super::viewport_scene::MaterializedTranscriptScene;
use iced::advanced::Renderer as _;
use iced::advanced::renderer;
use iced::advanced::text::Paragraph as _;
use iced::advanced::text::Renderer as _;
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
