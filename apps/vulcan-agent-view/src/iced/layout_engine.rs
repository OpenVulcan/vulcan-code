//! Layout and viewport-geometry helpers for the selectable transcript surface.
//! 可选择转录表面的布局与视口几何辅助逻辑。

use super::RendererParagraph;
use super::SelectableTranscriptState;
use super::scene::{
    CachedTableLayout, LaidOutFlowNode, LaidOutPanelNode, MeasuredRenderedCollapsibleDirectiveNode,
    MeasuredRenderedTableNode, RenderedCollapsibleDirectiveNode, RenderedPanelNode,
    RenderedParagraphNode, RenderedTableNode, RenderedTranscriptDocument, RenderedTranscriptNode,
    SelectableTableAlignment,
};
use crate::iced_theme as theme;
use iced::advanced::text;
use iced::advanced::text::Paragraph as _;
use iced::{Point, Rectangle, Size};
use std::ops::Range;

const TRANSCRIPT_VIEWPORT_OVERSCAN_Y: f32 = 240.0;

/// Returns the number of renderer paragraph slots needed by the whole rendered transcript document.
/// 返回整份渲染转录文档需要的渲染器段落槽位数量。
pub(super) fn count_rendered_paragraph_slots(document: &RenderedTranscriptDocument) -> usize {
    document
        .nodes
        .iter()
        .map(|node| match node {
            RenderedTranscriptNode::Panel(_) => 0,
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                1 + usize::from(collapsible_node.body.is_some())
            }
            RenderedTranscriptNode::Paragraph(_) => 1,
            RenderedTranscriptNode::Table(table_node) => table_node.cells.len(),
            RenderedTranscriptNode::Rule(_) => 0,
        })
        .sum()
}

/// Rebuilds the full layout cache for the rendered transcript document at the provided width and returns the resulting flow height.
/// 按给定宽度为渲染转录文档重建完整布局缓存，并返回生成后的流式总高度。
pub(super) fn rebuild_layout_cache(
    document: &RenderedTranscriptDocument,
    state: &mut SelectableTranscriptState,
    local_widget_bounds: Rectangle,
    available_width: f32,
) -> f32 {
    let render_cache = &mut state.render_cache;
    let paragraph_count = count_rendered_paragraph_slots(document);
    let table_count = document
        .nodes
        .iter()
        .filter(|node| matches!(node, RenderedTranscriptNode::Table(_)))
        .count();

    if render_cache.paragraphs.len() != paragraph_count {
        render_cache.paragraphs = (0..paragraph_count)
            .map(|_| RendererParagraph::default())
            .collect();
    }

    if render_cache.table_layouts.len() != table_count {
        render_cache.table_layouts = (0..table_count)
            .map(|_| CachedTableLayout::default())
            .collect();
    }

    render_cache.flow_layouts.clear();
    render_cache.panel_layouts.clear();

    let mut paragraph_index = 0usize;
    let mut table_index = 0usize;
    let mut current_y = 0.0f32;

    for (node_index, node) in document.nodes.iter().enumerate() {
        match node {
            RenderedTranscriptNode::Panel(_) => {}
            RenderedTranscriptNode::Collapsible(collapsible_node) => {
                let summary_paragraph_index = paragraph_index;
                render_cache.paragraphs[summary_paragraph_index] = build_renderer_paragraph(
                    &collapsible_node.summary,
                    Size::new(
                        paragraph_measurement_width(local_widget_bounds, &collapsible_node.summary),
                        f32::INFINITY,
                    ),
                    text::Alignment::Default,
                );

                let body_paragraph_index = collapsible_node.body.as_ref().map(|body_node| {
                    let body_index = summary_paragraph_index + 1;
                    render_cache.paragraphs[body_index] = build_renderer_paragraph(
                        body_node,
                        Size::new(
                            paragraph_measurement_width(local_widget_bounds, body_node),
                            f32::INFINITY,
                        ),
                        text::Alignment::Default,
                    );
                    body_index
                });

                let measured_collapsible = measure_rendered_collapsible_directive_node(
                    collapsible_node,
                    &render_cache.paragraphs[summary_paragraph_index],
                    body_paragraph_index.map(|body_index| &render_cache.paragraphs[body_index]),
                    local_widget_bounds,
                    current_y,
                );
                let flow_end_y = if let (Some(body_node), Some(body_outer_bounds)) = (
                    collapsible_node.body.as_ref(),
                    measured_collapsible.body_outer_bounds,
                ) {
                    body_outer_bounds.y + body_outer_bounds.height + body_node.gap_after
                } else {
                    measured_collapsible.summary_outer_bounds.y
                        + measured_collapsible.summary_outer_bounds.height
                        + collapsible_node.summary.gap_after
                };

                render_cache.flow_layouts.push(LaidOutFlowNode::Collapsible(
                    super::scene::LaidOutCollapsibleNode {
                        node_index,
                        summary_paragraph_index,
                        body_paragraph_index,
                        summary_outer_bounds: measured_collapsible.summary_outer_bounds,
                        body_outer_bounds: measured_collapsible.body_outer_bounds,
                        body_full_text_height: measured_collapsible.body_full_text_height,
                        flow_end_y,
                    },
                ));

                current_y = flow_end_y;
                paragraph_index += 1 + usize::from(body_paragraph_index.is_some());
            }
            RenderedTranscriptNode::Paragraph(paragraph_node) => {
                render_cache.paragraphs[paragraph_index] = build_renderer_paragraph(
                    paragraph_node,
                    Size::new(
                        paragraph_measurement_width(local_widget_bounds, paragraph_node),
                        f32::INFINITY,
                    ),
                    text::Alignment::Default,
                );
                let outer_bounds = resolve_paragraph_outer_bounds(
                    local_widget_bounds,
                    current_y + paragraph_node.gap_before,
                    paragraph_node,
                    &render_cache.paragraphs[paragraph_index],
                );
                let flow_end_y = outer_bounds.y + outer_bounds.height + paragraph_node.gap_after;

                render_cache.flow_layouts.push(LaidOutFlowNode::Paragraph(
                    super::scene::LaidOutParagraphNode {
                        node_index,
                        paragraph_index,
                        outer_bounds,
                        flow_end_y,
                    },
                ));

                current_y = flow_end_y;
                paragraph_index += 1;
            }
            RenderedTranscriptNode::Table(table_node) => {
                let (table_layout, _) = layout_rendered_table_node(
                    table_node,
                    &mut render_cache.paragraphs,
                    paragraph_index,
                    available_width,
                );
                let measured_table = measure_rendered_table_node(
                    table_node,
                    &table_layout,
                    &render_cache.paragraphs[table_layout.paragraph_range.clone()],
                    local_widget_bounds,
                    current_y,
                );
                let flow_end_y = measured_table.outer_bounds.y
                    + measured_table.outer_bounds.height
                    + table_node.gap_after;

                paragraph_index = table_layout.paragraph_range.end;
                render_cache.table_layouts[table_index] = table_layout;
                render_cache.flow_layouts.push(LaidOutFlowNode::Table(
                    super::scene::LaidOutTableNode {
                        node_index,
                        table_index,
                        measured_table,
                        flow_end_y,
                    },
                ));

                current_y = flow_end_y;
                table_index += 1;
            }
            RenderedTranscriptNode::Rule(rule_node) => {
                let rule_y = current_y + rule_node.gap_before;
                let outer_bounds = Rectangle::new(
                    Point::new(0.0, rule_y),
                    Size::new(available_width.max(1.0), 8.0),
                );
                let flow_end_y = rule_y + 8.0 + rule_node.gap_after;

                render_cache.flow_layouts.push(LaidOutFlowNode::Rule(
                    super::scene::LaidOutRuleNode {
                        node_index,
                        outer_bounds,
                        flow_end_y,
                    },
                ));

                current_y = flow_end_y;
            }
        }
    }

    render_cache.panel_layouts = document
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node_index, node)| {
            let RenderedTranscriptNode::Panel(panel_node) = node else {
                return None;
            };
            measure_laid_out_panel_node(
                node_index,
                panel_node,
                &render_cache.flow_layouts,
                available_width,
            )
        })
        .collect();

    render_cache.layout_ready = true;
    render_cache.cached_width = available_width;
    render_cache.cached_signature = document.layout_signature;
    render_cache.cached_height = current_y.max(1.0);

    render_cache.cached_height
}

/// Builds one renderer paragraph from a rendered paragraph node and the provided text bounds.
/// 根据一个渲染段落节点和给定文本边界构建渲染器段落。
pub(super) fn build_renderer_paragraph(
    paragraph_node: &RenderedParagraphNode,
    text_bounds: Size,
    alignment: text::Alignment,
) -> RendererParagraph {
    RendererParagraph::with_spans(text::Text {
        content: paragraph_node.spans.as_slice(),
        bounds: text_bounds,
        size: paragraph_node.text_size,
        line_height: paragraph_node.line_height,
        font: theme::ui_font(),
        align_x: alignment,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: paragraph_node.wrapping,
        ellipsis: text::Ellipsis::None,
        hint_factor: None,
    })
}

/// Returns the visible origin at which one paragraph should be drawn inside its padded background box.
/// 返回段落在其带内边距背景框中应绘制的可见起点。
pub(super) fn paragraph_local_origin(
    outer_bounds: Rectangle,
    paragraph_node: &RenderedParagraphNode,
) -> Point {
    Point::new(
        outer_bounds.x + paragraph_node.inner_padding.left,
        outer_bounds.y + paragraph_node.inner_padding.top,
    )
}

/// Returns the outer flow width available to one paragraph node after indentation is applied.
/// 返回在应用缩进之后可供单个段落节点使用的外部流宽。
fn paragraph_available_outer_width(
    widget_bounds: Rectangle,
    paragraph_node: &RenderedParagraphNode,
) -> f32 {
    (widget_bounds.width - paragraph_node.indent_left).max(1.0)
}

/// Returns the width used to measure one paragraph before it is aligned and optionally shrunk to content.
/// 返回单个段落在对齐并可选收缩到内容之前用于测量的宽度。
pub(super) fn paragraph_measurement_width(
    widget_bounds: Rectangle,
    paragraph_node: &RenderedParagraphNode,
) -> f32 {
    let available_outer_width = paragraph_available_outer_width(widget_bounds, paragraph_node);
    let capped_outer_width = paragraph_node
        .max_width_ratio
        .map(|ratio| (available_outer_width * ratio.clamp(0.0, 1.0)).max(1.0))
        .unwrap_or(available_outer_width);

    (capped_outer_width - paragraph_node.inner_padding.left - paragraph_node.inner_padding.right)
        .max(1.0)
}

/// Resolves the final outer bounds occupied by one paragraph after alignment and optional content-width fitting are applied.
/// 在应用对齐和可选内容宽度收缩后，解析单个段落最终占据的外部边界。
pub(super) fn resolve_paragraph_outer_bounds(
    widget_bounds: Rectangle,
    current_y: f32,
    paragraph_node: &RenderedParagraphNode,
    paragraph: &RendererParagraph,
) -> Rectangle {
    let available_outer_width = paragraph_available_outer_width(widget_bounds, paragraph_node);
    let capped_outer_width = paragraph_node
        .max_width_ratio
        .map(|ratio| (available_outer_width * ratio.clamp(0.0, 1.0)).max(1.0))
        .unwrap_or(available_outer_width);
    let paragraph_height = paragraph.min_bounds().height;
    let fitted_outer_width = (paragraph.min_bounds().width
        + paragraph_node.inner_padding.left
        + paragraph_node.inner_padding.right)
        .min(capped_outer_width)
        .max(1.0);
    let outer_width = if paragraph_node.fit_width_to_content {
        fitted_outer_width
    } else {
        capped_outer_width
    };
    let outer_x = match paragraph_node.horizontal_placement {
        super::scene::RenderedParagraphHorizontalPlacement::Leading => {
            widget_bounds.x + paragraph_node.indent_left
        }
        super::scene::RenderedParagraphHorizontalPlacement::Trailing => {
            widget_bounds.x + widget_bounds.width - outer_width
        }
    };

    Rectangle::new(
        Point::new(outer_x, current_y),
        Size::new(
            outer_width,
            paragraph_node.inner_padding.top
                + paragraph_height
                + paragraph_node.inner_padding.bottom,
        ),
    )
}

/// Returns the height cap used by the visible body viewport of one collapsible semantic block.
/// 返回某个折叠语义区块正文视口使用的可见高度上限。
fn collapsible_body_visible_height_cap(
    body_node: &RenderedParagraphNode,
    max_visible_lines: usize,
) -> f32 {
    let line_height = match body_node.line_height {
        text::LineHeight::Relative(value) => body_node.text_size.0 * value,
        text::LineHeight::Absolute(value) => value.0,
    };

    body_node.inner_padding.top
        + (line_height * max_visible_lines.max(1) as f32)
        + body_node.inner_padding.bottom
}

/// Measures one compact collapsible semantic block node using the provided cached renderer paragraphs.
/// 使用给定缓存渲染段落测量单个紧凑折叠语义区块节点。
pub(super) fn measure_rendered_collapsible_directive_node(
    collapsible_node: &RenderedCollapsibleDirectiveNode,
    summary_paragraph: &RendererParagraph,
    body_paragraph: Option<&RendererParagraph>,
    widget_bounds: Rectangle,
    current_y: f32,
) -> MeasuredRenderedCollapsibleDirectiveNode {
    let summary_outer_bounds = resolve_paragraph_outer_bounds(
        widget_bounds,
        current_y + collapsible_node.summary.gap_before,
        &collapsible_node.summary,
        summary_paragraph,
    );

    let (body_outer_bounds, body_full_text_height) = match (&collapsible_node.body, body_paragraph)
    {
        (Some(body_node), Some(body_paragraph)) if !collapsible_node.collapsed => {
            let full_body_height = body_paragraph.min_bounds().height
                + body_node.inner_padding.top
                + body_node.inner_padding.bottom;
            let visible_body_height = full_body_height
                .min(collapsible_body_visible_height_cap(
                    body_node,
                    collapsible_node.max_visible_lines,
                ))
                .max(1.0);
            let body_y =
                summary_outer_bounds.y + summary_outer_bounds.height + body_node.gap_before;
            let body_width = (widget_bounds.width - body_node.indent_left).max(1.0);

            (
                Some(Rectangle::new(
                    Point::new(widget_bounds.x + body_node.indent_left, body_y),
                    Size::new(body_width, visible_body_height),
                )),
                full_body_height.max(1.0),
            )
        }
        _ => (None, 0.0),
    };

    MeasuredRenderedCollapsibleDirectiveNode {
        summary_outer_bounds,
        body_outer_bounds,
        body_full_text_height,
    }
}

/// Lays out one rendered table node and returns both its cached geometry and total flow height.
/// 布局一个渲染表格节点，并返回其缓存几何信息与总流式高度。
pub(super) fn layout_rendered_table_node(
    table_node: &RenderedTableNode,
    paragraphs: &mut [RendererParagraph],
    paragraph_start: usize,
    available_width: f32,
) -> (CachedTableLayout, f32) {
    if table_node.column_count == 0 || table_node.row_count == 0 || table_node.cells.is_empty() {
        return (
            CachedTableLayout {
                paragraph_range: paragraph_start..paragraph_start,
                column_widths: Vec::new(),
                row_heights: Vec::new(),
                border_thickness: 1.0,
            },
            table_node.gap_before + table_node.gap_after,
        );
    }

    let border_thickness = 1.0;
    let table_available_width = (available_width
        - table_node.indent_left
        - table_node.outer_padding.left
        - table_node.outer_padding.right)
        .max(1.0);
    let horizontal_padding = table_node.cell_padding.left + table_node.cell_padding.right;
    let vertical_padding = table_node.cell_padding.top + table_node.cell_padding.bottom;
    let usable_width = (table_available_width
        - border_thickness * (table_node.column_count as f32 + 1.0))
        .max(table_node.column_count as f32);
    let measurement_text_width = (usable_width - horizontal_padding).max(1.0);

    let paragraph_range = paragraph_start..paragraph_start + table_node.cells.len();
    let mut natural_column_widths = vec![0.0_f32; table_node.column_count];

    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let paragraph = build_renderer_paragraph(
            &cell_node.paragraph,
            Size::new(measurement_text_width, f32::INFINITY),
            table_cell_text_alignment(cell_node.alignment),
        );
        paragraphs[paragraph_start + cell_index] = paragraph;
        natural_column_widths[cell_node.column_index] = natural_column_widths
            [cell_node.column_index]
            .max(paragraphs[paragraph_start + cell_index].min_bounds().width + horizontal_padding);
    }

    let column_widths = resolve_rendered_table_column_widths(&natural_column_widths, usable_width);

    let mut row_heights = vec![0.0_f32; table_node.row_count];

    for (cell_index, cell_node) in table_node.cells.iter().enumerate() {
        let column_width = column_widths
            .get(cell_node.column_index)
            .copied()
            .unwrap_or(usable_width / table_node.column_count as f32);
        paragraphs[paragraph_start + cell_index].resize(Size::new(
            (column_width - horizontal_padding).max(1.0),
            f32::INFINITY,
        ));
        row_heights[cell_node.row_index] = row_heights[cell_node.row_index]
            .max(paragraphs[paragraph_start + cell_index].min_bounds().height + vertical_padding);
    }

    let table_height = table_node.gap_before
        + border_thickness * (table_node.row_count as f32 + 1.0)
        + row_heights.iter().sum::<f32>()
        + table_node.gap_after;

    (
        CachedTableLayout {
            paragraph_range,
            column_widths,
            row_heights,
            border_thickness,
        },
        table_height,
    )
}

/// Resolves final table column widths from natural measurements and the available content width.
/// 根据自然测量结果和可用内容宽度解析最终表格列宽。
fn resolve_rendered_table_column_widths(natural_widths: &[f32], available_width: f32) -> Vec<f32> {
    if natural_widths.is_empty() {
        return Vec::new();
    }

    let natural_sum = natural_widths.iter().sum::<f32>();

    if natural_sum <= 0.0 {
        return vec![available_width / natural_widths.len() as f32; natural_widths.len()];
    }

    if natural_sum <= available_width {
        let extra_width = (available_width - natural_sum) / natural_widths.len() as f32;
        return natural_widths
            .iter()
            .map(|width| width + extra_width)
            .collect();
    }

    let even_width = available_width / natural_widths.len() as f32;
    natural_widths.iter().map(|_| even_width.max(1.0)).collect()
}

/// Returns the text alignment used by one rendered table cell based on the parsed Markdown alignment marker.
/// 根据解析得到的 Markdown 对齐标记返回某个渲染表格单元格使用的文本对齐方式。
fn table_cell_text_alignment(alignment: SelectableTableAlignment) -> text::Alignment {
    match alignment {
        SelectableTableAlignment::Left => text::Alignment::Default,
        SelectableTableAlignment::Center => text::Alignment::Center,
        SelectableTableAlignment::Right => text::Alignment::Right,
    }
}

/// Measures one rendered table node into concrete cell rectangles and text origins using the cached layout.
/// 使用缓存布局将一个渲染表格节点测量为具体单元格矩形和文本起点。
pub(super) fn measure_rendered_table_node(
    table_node: &RenderedTableNode,
    table_layout: &CachedTableLayout,
    paragraphs: &[RendererParagraph],
    widget_bounds: Rectangle,
    current_y: f32,
) -> MeasuredRenderedTableNode {
    let outer_x = widget_bounds.x + table_node.indent_left + table_node.outer_padding.left;
    let outer_y = current_y + table_node.gap_before;
    let outer_width = table_layout.column_widths.iter().sum::<f32>()
        + table_layout.border_thickness * (table_node.column_count as f32 + 1.0);
    let outer_height = table_layout.row_heights.iter().sum::<f32>()
        + table_layout.border_thickness * (table_node.row_count as f32 + 1.0);
    let outer_bounds = Rectangle::new(
        Point::new(outer_x, outer_y),
        Size::new(outer_width, outer_height),
    );

    let mut cell_bounds = Vec::with_capacity(table_node.cells.len());
    let mut cell_text_origins = Vec::with_capacity(table_node.cells.len());
    let mut current_row_y = outer_y + table_layout.border_thickness;

    for row_index in 0..table_node.row_count {
        let row_height = table_layout.row_heights[row_index];
        let mut current_column_x = outer_x + table_layout.border_thickness;

        for column_index in 0..table_node.column_count {
            let cell_width = table_layout.column_widths[column_index];
            let cell_bounds_rect = Rectangle::new(
                Point::new(current_column_x, current_row_y),
                Size::new(cell_width, row_height),
            );
            cell_bounds.push(cell_bounds_rect);
            cell_text_origins.push(Point::new(
                cell_bounds_rect.x + table_node.cell_padding.left,
                cell_bounds_rect.y + table_node.cell_padding.top,
            ));
            current_column_x += cell_width + table_layout.border_thickness;
        }

        current_row_y += row_height + table_layout.border_thickness;
    }

    let _ = paragraphs;

    MeasuredRenderedTableNode {
        outer_bounds,
        cell_bounds,
        cell_text_origins,
    }
}

/// Measures one background panel node from the already cached local flow layouts it spans.
/// 根据其所覆盖的已缓存本地流式布局测量一个背景面板节点。
pub(super) fn measure_laid_out_panel_node(
    node_index: usize,
    panel_node: &RenderedPanelNode,
    flow_layouts: &[LaidOutFlowNode],
    available_width: f32,
) -> Option<LaidOutPanelNode> {
    let start_node_index = node_index + 1;
    let end_node_index = start_node_index + panel_node.child_node_count;
    let mut first_y = None::<f32>;
    let mut last_y = None::<f32>;

    for flow_layout in flow_layouts.iter().filter(|layout| {
        let child_node_index = layout.node_index();
        child_node_index >= start_node_index && child_node_index < end_node_index
    }) {
        first_y.get_or_insert(flow_layout.top_y());
        last_y = Some(flow_layout.bottom_y());
    }

    let (Some(panel_top), Some(panel_bottom)) = (first_y, last_y) else {
        return None;
    };

    Some(LaidOutPanelNode {
        node_index,
        outer_bounds: Rectangle::new(
            Point::new(0.0, panel_top),
            Size::new(available_width, (panel_bottom - panel_top).max(1.0)),
        ),
    })
}

/// Returns the visible flow-layout slice intersecting the provided local viewport plus one overscan band.
/// 返回与给定本地视口及一层预渲染带相交的可见流式布局切片。
pub(super) fn visible_flow_layout_range(
    flow_layouts: &[LaidOutFlowNode],
    viewport: Rectangle,
) -> Range<usize> {
    if flow_layouts.is_empty() {
        return 0..0;
    }

    let start_y = (viewport.y - TRANSCRIPT_VIEWPORT_OVERSCAN_Y).max(0.0);
    let end_y = viewport.y + viewport.height + TRANSCRIPT_VIEWPORT_OVERSCAN_Y;
    let start_index = flow_layouts.partition_point(|layout| layout.bottom_y() < start_y);
    let end_index = flow_layouts.partition_point(|layout| layout.top_y() <= end_y);

    start_index.min(flow_layouts.len())..end_index.min(flow_layouts.len())
}

/// Returns the cached flow-layout index intersecting the provided local y-position, if any.
/// 返回与给定本地纵向位置相交的缓存流式布局索引（如果存在）。
pub(super) fn flow_layout_index_at_local_y(
    flow_layouts: &[LaidOutFlowNode],
    local_y: f32,
) -> Option<usize> {
    let index = flow_layouts.partition_point(|layout| layout.bottom_y() < local_y);
    (index < flow_layouts.len()).then_some(index)
}

/// Builds one translated rectangle from local transcript coordinates into absolute renderer coordinates.
/// 将一个本地转录坐标矩形转换为绝对渲染坐标矩形。
pub(super) fn translate_rectangle(bounds: Rectangle, origin: Point) -> Rectangle {
    Rectangle::new(
        Point::new(bounds.x + origin.x, bounds.y + origin.y),
        bounds.size(),
    )
}

/// Builds one translated point from local transcript coordinates into absolute renderer coordinates.
/// 将一个本地转录坐标点转换为绝对渲染坐标点。
pub(super) fn translate_point(point: Point, origin: Point) -> Point {
    Point::new(point.x + origin.x, point.y + origin.y)
}

/// Converts one absolute pointer position into transcript-local coordinates.
/// 将一个绝对指针位置转换为转录本地坐标。
pub(super) fn localize_point(position: Point, origin: Point) -> Point {
    Point::new(position.x - origin.x, position.y - origin.y)
}

/// Converts one absolute viewport rectangle into transcript-local coordinates.
/// 将一个绝对视口矩形转换为转录本地坐标。
pub(super) fn localize_rectangle(viewport: Rectangle, origin: Point) -> Rectangle {
    Rectangle::new(localize_point(viewport.position(), origin), viewport.size())
}

/// Builds one translated measured-table payload from local transcript coordinates into absolute renderer coordinates.
/// 将一个本地转录坐标下的测量表格载荷转换为绝对渲染坐标。
pub(super) fn translate_measured_table_node(
    measured_table: &MeasuredRenderedTableNode,
    origin: Point,
) -> MeasuredRenderedTableNode {
    MeasuredRenderedTableNode {
        outer_bounds: translate_rectangle(measured_table.outer_bounds, origin),
        cell_bounds: measured_table
            .cell_bounds
            .iter()
            .map(|bounds| translate_rectangle(*bounds, origin))
            .collect(),
        cell_text_origins: measured_table
            .cell_text_origins
            .iter()
            .map(|point| translate_point(*point, origin))
            .collect(),
    }
}
