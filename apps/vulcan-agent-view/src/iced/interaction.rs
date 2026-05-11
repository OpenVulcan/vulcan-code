//! Interaction helpers for transcript hit-testing, selection projection, and drag auto-scroll.
//! 转录命中测试、选区投影与拖拽自动滚动的交互辅助逻辑。

use super::layout_engine::flow_layout_index_at_local_y;
use super::paint::hit_test_rendered_table_node;
use super::scene::{
    LaidOutFlowNode, RenderedParagraphNode, RenderedTranscriptDocument, RenderedTranscriptNode,
};
use super::{
    RendererParagraph, SelectableTranscriptDragAutoScroll,
    SelectableTranscriptDragAutoScrollDirection, SelectableTranscriptState,
};
use iced::advanced::text::Paragraph as _;
use iced::{Point, Rectangle, Size};

/// Performs hit-testing against the rendered transcript and returns the nearest global character offset.
/// 对渲染转录执行命中测试，并返回最近的全局字符偏移位置。
pub(super) fn hit_test_rendered_document(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    position: Point,
) -> Option<usize> {
    let render_cache = &state.render_cache;
    let flow_layout_index = flow_layout_index_at_local_y(&render_cache.flow_layouts, position.y)?;

    match render_cache.flow_layouts.get(flow_layout_index)? {
        LaidOutFlowNode::Collapsible(layout_node) => {
            let RenderedTranscriptNode::Collapsible(collapsible_node) =
                &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            let summary_paragraph = render_cache
                .paragraphs
                .get(layout_node.summary_paragraph_index)?;

            if layout_node.summary_outer_bounds.contains(position) {
                let summary_height = summary_paragraph.min_bounds().height;
                let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
                    Point::new(
                        position.x
                            - layout_node.summary_outer_bounds.x
                            - collapsible_node.summary.inner_padding.left,
                        position.y
                            - layout_node.summary_outer_bounds.y
                            - collapsible_node.summary.inner_padding.top,
                    ),
                    &collapsible_node.summary,
                    layout_node.summary_outer_bounds.size(),
                    summary_height,
                );
                let local_byte_offset = paragraph_hit_test_to_byte_offset(
                    summary_paragraph,
                    &collapsible_node.summary.display_text,
                    local_point,
                )?;
                let local_char_offset = super::byte_offset_to_char_offset(
                    &collapsible_node.summary.display_text,
                    local_byte_offset,
                );
                return Some(
                    (collapsible_node.summary.global_char_range.start + local_char_offset)
                        .min(collapsible_node.summary.global_char_range.end),
                );
            }

            if let (Some(body_node), Some(body_paragraph_index), Some(body_outer_bounds)) = (
                collapsible_node.body.as_ref(),
                layout_node.body_paragraph_index,
                layout_node.body_outer_bounds,
            ) {
                if body_outer_bounds.contains(position) {
                    let body_paragraph = render_cache.paragraphs.get(body_paragraph_index)?;
                    let max_scroll =
                        (layout_node.body_full_text_height - body_outer_bounds.height).max(0.0);
                    let scroll_offset = state
                        .collapsible_body_scroll_offsets
                        .get(&collapsible_node.block_id)
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(0.0, max_scroll);
                    let body_height = body_paragraph.min_bounds().height;
                    let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
                        Point::new(
                            position.x - body_outer_bounds.x - body_node.inner_padding.left,
                            position.y - body_outer_bounds.y - body_node.inner_padding.top
                                + scroll_offset,
                        ),
                        body_node,
                        body_outer_bounds.size(),
                        body_height,
                    );
                    let local_byte_offset = paragraph_hit_test_to_byte_offset(
                        body_paragraph,
                        &body_node.display_text,
                        local_point,
                    )?;
                    let local_char_offset = super::byte_offset_to_char_offset(
                        &body_node.display_text,
                        local_byte_offset,
                    );
                    return Some(
                        (body_node.global_char_range.start + local_char_offset)
                            .min(body_node.global_char_range.end),
                    );
                }
            }

            None
        }
        LaidOutFlowNode::Paragraph(layout_node) => {
            let RenderedTranscriptNode::Paragraph(paragraph_node) =
                &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            if !layout_node.outer_bounds.contains(position) {
                return None;
            }

            let paragraph = render_cache.paragraphs.get(layout_node.paragraph_index)?;
            let paragraph_height = paragraph.min_bounds().height;
            let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
                Point::new(
                    position.x - layout_node.outer_bounds.x - paragraph_node.inner_padding.left,
                    position.y - layout_node.outer_bounds.y - paragraph_node.inner_padding.top,
                ),
                paragraph_node,
                layout_node.outer_bounds.size(),
                paragraph_height,
            );
            let local_byte_offset = paragraph_hit_test_to_byte_offset(
                paragraph,
                &paragraph_node.display_text,
                local_point,
            )?;
            let local_char_offset =
                super::byte_offset_to_char_offset(&paragraph_node.display_text, local_byte_offset);
            Some(
                (paragraph_node.global_char_range.start + local_char_offset)
                    .min(paragraph_node.global_char_range.end),
            )
        }
        LaidOutFlowNode::Table(layout_node) => {
            let RenderedTranscriptNode::Table(table_node) = &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            let table_layout = render_cache.table_layouts.get(layout_node.table_index)?;
            let table_paragraphs = &render_cache.paragraphs[table_layout.paragraph_range.clone()];
            hit_test_rendered_table_node(
                table_node,
                table_paragraphs,
                &layout_node.measured_table,
                position,
            )
        }
        LaidOutFlowNode::Rule(_) => None,
    }
}

/// Converts one paragraph-local hit-test point into a paragraph-global UTF-8 byte offset across all visual lines.
/// 将一个段落局部命中点转换为覆盖全部可视行的段落全局 UTF-8 字节偏移。
pub(super) fn paragraph_hit_test_to_byte_offset(
    paragraph: &RendererParagraph,
    paragraph_text: &str,
    local_point: Point,
) -> Option<usize> {
    let cursor = paragraph.buffer().hit(local_point.x, local_point.y)?;
    Some(multiline_cursor_to_byte_offset(
        paragraph_text,
        cursor.line,
        cursor.index,
    ))
}

/// Converts one multi-line cursor location into a paragraph-global UTF-8 byte offset while preserving explicit line breaks.
/// 将一个多行光标位置转换为段落全局 UTF-8 字节偏移，并保留显式换行符的占位。
pub(super) fn multiline_cursor_to_byte_offset(
    paragraph_text: &str,
    line_index: usize,
    line_byte_offset: usize,
) -> usize {
    let mut accumulated_byte_offset = 0usize;

    for (current_line_index, source_line) in paragraph_text.split_inclusive('\n').enumerate() {
        let line_text = source_line.strip_suffix('\n').unwrap_or(source_line);

        if current_line_index == line_index {
            return accumulated_byte_offset + line_byte_offset.min(line_text.len());
        }

        accumulated_byte_offset += source_line.len();
    }

    if line_index == paragraph_text.lines().count() {
        paragraph_text.len()
    } else {
        accumulated_byte_offset.min(paragraph_text.len())
    }
}

/// Clamps one local hit-test point so padded block regions resolve onto the actual text bounds.
/// 将局部命中测试点钳制到真实文本边界内，使带内边距的区块区域命中到实际文本。
pub(super) fn clamp_hit_test_point_to_paragraph_text_bounds(
    local_point: Point,
    paragraph_node: &RenderedParagraphNode,
    outer_size: Size,
    paragraph_height: f32,
) -> Point {
    let max_x =
        (outer_size.width - paragraph_node.inner_padding.left - paragraph_node.inner_padding.right)
            .max(1.0)
            - 0.5;
    let max_y = paragraph_height.max(1.0) - 0.5;

    Point::new(
        local_point.x.clamp(0.0, max_x),
        local_point.y.clamp(0.0, max_y),
    )
}

/// Returns the normalized `(start, end)` selection tuple for the provided anchor and head character offsets.
/// 根据提供的锚点和头部字符偏移返回归一化后的 `(start, end)` 选择区间元组。
fn normalized_selection_for_offsets(
    anchor_char: usize,
    head_char: usize,
) -> Option<(usize, usize)> {
    let start = anchor_char.min(head_char);
    let end = anchor_char.max(head_char);

    if start == end {
        None
    } else {
        Some((start, end))
    }
}

/// Returns the effective selection tuple, including live drag positions that may currently sit outside the visible widget bounds.
/// 返回有效的选择区间，包含当前可能位于可见组件边界之外的实时拖拽位置。
pub(super) fn effective_selection(
    state: &SelectableTranscriptState,
    document: &RenderedTranscriptDocument,
    widget_bounds: Rectangle,
    cursor_position: Option<Point>,
) -> Option<(usize, usize)> {
    let head_char = if state.dragging {
        cursor_position
            .and_then(|position| drag_selection_head_char(document, state, widget_bounds, position))
            .unwrap_or(state.head_char)
    } else {
        state.head_char
    };

    normalized_selection_for_offsets(state.anchor_char, head_char)
}

/// Returns the current static transcript selection when no live drag extrapolation is needed.
/// 在不需要实时拖拽外推时返回当前静态转录选区。
pub(super) fn current_selection_for_state(
    state: &SelectableTranscriptState,
) -> Option<(usize, usize)> {
    normalized_selection_for_offsets(state.anchor_char, state.head_char)
}

/// Projects one pointer position onto the nearest selectable content inside the owning flow node.
/// 将一个指针位置投影到所属流式节点内最近的可选择文本内容上。
fn projected_hit_test_rendered_document(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    position: Point,
) -> Option<usize> {
    let render_cache = &state.render_cache;
    let flow_layout_index = flow_layout_index_at_local_y(&render_cache.flow_layouts, position.y)?;

    match render_cache.flow_layouts.get(flow_layout_index)? {
        LaidOutFlowNode::Collapsible(layout_node) => {
            let RenderedTranscriptNode::Collapsible(collapsible_node) =
                &document.nodes[layout_node.node_index]
            else {
                return None;
            };

            let summary_distance = layout_node.summary_outer_bounds.distance(position);
            let body_candidate =
                if let (Some(body_node), Some(body_paragraph_index), Some(body_outer_bounds)) = (
                    collapsible_node.body.as_ref(),
                    layout_node.body_paragraph_index,
                    layout_node.body_outer_bounds,
                ) {
                    Some((
                        body_node,
                        body_paragraph_index,
                        body_outer_bounds,
                        body_outer_bounds.distance(position),
                    ))
                } else {
                    None
                };

            if let Some((body_node, body_paragraph_index, body_outer_bounds, body_distance)) =
                body_candidate
                && body_distance < summary_distance
            {
                let body_paragraph = render_cache.paragraphs.get(body_paragraph_index)?;
                let max_scroll =
                    (layout_node.body_full_text_height - body_outer_bounds.height).max(0.0);
                let scroll_offset = state
                    .collapsible_body_scroll_offsets
                    .get(&collapsible_node.block_id)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, max_scroll);

                return projected_hit_test_rendered_paragraph(
                    body_node,
                    body_paragraph,
                    body_outer_bounds,
                    position,
                    scroll_offset,
                );
            }

            let summary_paragraph = render_cache
                .paragraphs
                .get(layout_node.summary_paragraph_index)?;
            projected_hit_test_rendered_paragraph(
                &collapsible_node.summary,
                summary_paragraph,
                layout_node.summary_outer_bounds,
                position,
                0.0,
            )
        }
        LaidOutFlowNode::Paragraph(layout_node) => {
            let RenderedTranscriptNode::Paragraph(paragraph_node) =
                &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            let paragraph = render_cache.paragraphs.get(layout_node.paragraph_index)?;

            projected_hit_test_rendered_paragraph(
                paragraph_node,
                paragraph,
                layout_node.outer_bounds,
                position,
                0.0,
            )
        }
        LaidOutFlowNode::Table(layout_node) => {
            let RenderedTranscriptNode::Table(table_node) = &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            let table_layout = render_cache.table_layouts.get(layout_node.table_index)?;
            let table_paragraphs = &render_cache.paragraphs[table_layout.paragraph_range.clone()];
            let projected_position =
                clamp_point_to_rectangle(position, layout_node.measured_table.outer_bounds);

            hit_test_rendered_table_node(
                table_node,
                table_paragraphs,
                &layout_node.measured_table,
                projected_position,
            )
        }
        LaidOutFlowNode::Rule(_) => None,
    }
}

/// Resolves one projected character hit inside a rendered paragraph while preserving line and column intent.
/// 在保留行列意图的前提下，解析一个渲染段落内部的投影字符命中结果。
fn projected_hit_test_rendered_paragraph(
    paragraph_node: &RenderedParagraphNode,
    paragraph: &RendererParagraph,
    outer_bounds: Rectangle,
    position: Point,
    scroll_offset: f32,
) -> Option<usize> {
    let projected_position = clamp_point_to_rectangle(position, outer_bounds);
    let paragraph_height = paragraph.min_bounds().height;
    let local_point = clamp_hit_test_point_to_paragraph_text_bounds(
        Point::new(
            projected_position.x - outer_bounds.x - paragraph_node.inner_padding.left,
            projected_position.y - outer_bounds.y - paragraph_node.inner_padding.top
                + scroll_offset,
        ),
        paragraph_node,
        outer_bounds.size(),
        paragraph_height,
    );
    let local_byte_offset =
        paragraph_hit_test_to_byte_offset(paragraph, &paragraph_node.display_text, local_point)?;
    let local_char_offset =
        super::byte_offset_to_char_offset(&paragraph_node.display_text, local_byte_offset);

    Some(
        (paragraph_node.global_char_range.start + local_char_offset)
            .min(paragraph_node.global_char_range.end),
    )
}

/// Returns the nearest selection head character offset for one drag pointer position, even when it has crossed above or below the widget bounds.
/// 即使拖拽指针已经越过组件上下边界，也返回其最近的选择头部字符偏移。
pub(super) fn drag_selection_head_char(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    position: Point,
) -> Option<usize> {
    let clamped_position = clamp_point_to_rectangle(position, widget_bounds);

    hit_test_rendered_document(document, state, clamped_position)
        .or_else(|| projected_hit_test_rendered_document(document, state, clamped_position))
        .or_else(|| {
            if position.y <= widget_bounds.y {
                Some(0)
            } else if position.y >= widget_bounds.y + widget_bounds.height {
                Some(document.len_chars())
            } else {
                None
            }
        })
}

/// Clamps one absolute pointer position into the provided rectangle while preserving the closest edge semantics.
/// 将一个绝对指针位置钳制到给定矩形内，同时保留最近边缘的语义。
pub(super) fn clamp_point_to_rectangle(position: Point, bounds: Rectangle) -> Point {
    Point::new(
        position.x.clamp(bounds.x, bounds.x + bounds.width.max(0.0)),
        position
            .y
            .clamp(bounds.y, bounds.y + bounds.height.max(0.0)),
    )
}

/// Builds one drag auto-scroll request from the current pointer position when it crosses above or below the visible widget bounds.
/// 当当前指针位置越过可见组件上下边界时，构建一份拖拽自动滚动请求。
pub(super) fn drag_auto_scroll_request_for_position(
    widget_bounds: Rectangle,
    position: Point,
) -> Option<SelectableTranscriptDragAutoScroll> {
    if position.y < widget_bounds.y {
        Some(SelectableTranscriptDragAutoScroll {
            direction: SelectableTranscriptDragAutoScrollDirection::Up,
            overshoot: widget_bounds.y - position.y,
        })
    } else {
        let bottom = widget_bounds.y + widget_bounds.height;

        (position.y > bottom).then_some(SelectableTranscriptDragAutoScroll {
            direction: SelectableTranscriptDragAutoScrollDirection::Down,
            overshoot: position.y - bottom,
        })
    }
}

/// Returns the synthetic background action link hit by the provided pointer position, if any.
/// 返回给定指针位置命中的合成背景动作链接（如果存在）。
pub(super) fn background_action_link_at_position(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    position: Point,
) -> Option<String> {
    let render_cache = &state.render_cache;
    let flow_layout_index = flow_layout_index_at_local_y(&render_cache.flow_layouts, position.y)?;

    match render_cache.flow_layouts.get(flow_layout_index)? {
        LaidOutFlowNode::Collapsible(layout_node) => {
            let RenderedTranscriptNode::Collapsible(collapsible_node) =
                &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            layout_node
                .summary_outer_bounds
                .contains(position)
                .then(|| collapsible_node.summary.background_action_link.clone())
                .flatten()
        }
        LaidOutFlowNode::Paragraph(layout_node) => {
            let RenderedTranscriptNode::Paragraph(paragraph_node) =
                &document.nodes[layout_node.node_index]
            else {
                return None;
            };
            layout_node
                .outer_bounds
                .contains(position)
                .then(|| paragraph_node.background_action_link.clone())
                .flatten()
        }
        LaidOutFlowNode::Table(_) | LaidOutFlowNode::Rule(_) => None,
    }
}

/// Applies one wheel-scroll delta to the collapsible semantic block body below the provided pointer position.
/// 将一段滚轮滚动增量应用到给定指针位置下方的折叠语义区块正文。
pub(super) fn scroll_collapsible_body_at_position(
    document: &RenderedTranscriptDocument,
    state: &mut SelectableTranscriptState,
    position: Point,
    delta_y: f32,
) -> bool {
    let render_cache = &state.render_cache;
    let Some(flow_layout_index) =
        flow_layout_index_at_local_y(&render_cache.flow_layouts, position.y)
    else {
        return false;
    };
    let Some(LaidOutFlowNode::Collapsible(layout_node)) =
        render_cache.flow_layouts.get(flow_layout_index)
    else {
        return false;
    };
    let RenderedTranscriptNode::Collapsible(collapsible_node) =
        &document.nodes[layout_node.node_index]
    else {
        return false;
    };
    let Some(body_outer_bounds) = layout_node.body_outer_bounds else {
        return false;
    };
    if !body_outer_bounds.contains(position) {
        return false;
    }

    let max_scroll = (layout_node.body_full_text_height - body_outer_bounds.height).max(0.0);
    if max_scroll <= f32::EPSILON {
        return false;
    }

    let entry = state
        .collapsible_body_scroll_offsets
        .entry(collapsible_node.block_id.clone())
        .or_insert(0.0);
    *entry = (*entry + delta_y).clamp(0.0, max_scroll);
    true
}
