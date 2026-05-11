//! Viewport materialization helpers that derive visible-window state from the long-document render cache.
//! 从长文档渲染缓存中推导可见窗口状态的视口物化辅助逻辑。

use super::interaction::effective_selection;
use super::layout_engine::{localize_point, localize_rectangle, visible_flow_layout_range};
use super::{RenderedTranscriptDocument, SelectableTranscriptState};
use iced::advanced::mouse;
use iced::{Point, Rectangle};
use std::ops::Range;

/// Captures one materialized viewport snapshot derived from widget bounds, cursor position, and cached flow geometry.
/// 记录一份根据组件边界、指针位置与缓存流式几何信息推导出的视口物化快照。
pub(super) struct MaterializedTranscriptViewport {
    /// Widget-space origin used to translate cached local geometry into absolute renderer coordinates.
    /// 用于将缓存局部几何转换为绝对渲染坐标的组件空间原点。
    pub(super) widget_origin: Point,
    /// Current effective transcript selection resolved against the live cursor position when dragging.
    /// 在拖拽时根据实时指针位置解析出的当前有效转录选区。
    pub(super) selection: Option<(usize, usize)>,
    /// Range of flow-layout nodes intersecting the current viewport and overscan policy.
    /// 与当前视口及其预取策略相交的流式布局节点范围。
    pub(super) visible_flow_range: Range<usize>,
}

/// Materializes one viewport snapshot from the current widget bounds, parent viewport, and pointer state.
/// 根据当前组件边界、父级视口与指针状态物化一份视口快照。
pub(super) fn materialize_transcript_viewport(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
    viewport: Rectangle,
) -> MaterializedTranscriptViewport {
    let widget_origin = widget_bounds.position();
    let local_viewport = localize_rectangle(viewport, widget_origin);
    let local_cursor_position = cursor
        .position_including_levitation()
        .map(|position| localize_point(position, widget_origin));
    let selection = effective_selection(state, document, local_viewport, local_cursor_position);
    let visible_flow_range =
        visible_flow_layout_range(&state.render_cache.flow_layouts, local_viewport);

    MaterializedTranscriptViewport {
        widget_origin,
        selection,
        visible_flow_range,
    }
}
