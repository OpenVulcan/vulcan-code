//! Paint helpers for transcript selection highlights, rendered tables, and related hit-testing.
//! 转录选区高亮、渲染表格及相关命中测试的绘制辅助逻辑。

use super::RendererParagraph;
use super::interaction::{
    clamp_hit_test_point_to_paragraph_text_bounds, paragraph_hit_test_to_byte_offset,
};
use super::scene::{
    MeasuredRenderedTableNode, RenderedParagraphNode, RenderedTableNode, RenderedTranscriptDocument,
};
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
