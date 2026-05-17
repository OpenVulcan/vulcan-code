//! Materialized viewport-scene helpers that bundle the visible transcript window into one paint-friendly facade.
//! 将可见转录窗口打包为单个便于绘制的外观对象的视口场景辅助逻辑。

use super::RendererParagraph;
use super::SelectableTranscriptState;
use super::layout_engine::translate_rectangle;
use super::render_cache::TranscriptRenderCache;
use super::scene::{
    CachedTableLayout, LaidOutCollapsibleNode, LaidOutFlowNode, LaidOutMediaCardNode,
    LaidOutPanelNode, LaidOutParagraphNode, LaidOutRuleNode, LaidOutTableNode,
    RenderedCollapsibleDirectiveNode, RenderedMediaCardNode, RenderedPanelNode,
    RenderedParagraphNode, RenderedRuleNode, RenderedTableNode, RenderedTranscriptDocument,
    RenderedTranscriptNode,
};
use super::viewport_materialization::materialize_transcript_viewport;
use iced::advanced::mouse;
use iced::{Point, Rectangle};
use std::collections::BTreeMap;
use std::ops::Range;

/// Paint-facing facade that carries the currently visible transcript viewport together with visible panel coverage.
/// 携带当前可见转录视口及可见面板覆盖信息、面向绘制阶段的外观对象。
pub(super) struct MaterializedTranscriptScene<'a> {
    /// Immutable rendered transcript document backing all flow-node lookup and selection copy semantics.
    /// 支撑全部流式节点查找与选区复制语义的不可变渲染转录文档。
    pub(super) document: &'a RenderedTranscriptDocument,
    /// Shared render-cache reference containing the visible paragraph, table, and flow-layout geometry.
    /// 包含可见段落、表格与流式布局几何信息的共享渲染缓存引用。
    pub(super) render_cache: &'a TranscriptRenderCache,
    /// Per-collapsible body scroll offsets needed to render clipped inline semantic blocks.
    /// 渲染被裁剪的内联语义区块正文所需的按折叠块保存的滚动偏移。
    pub(super) collapsible_body_scroll_offsets: &'a BTreeMap<String, f32>,
    /// Absolute widget bounds used by paint helpers that need to clamp quote bars or related surface chrome.
    /// 供绘制辅助逻辑用于钳制引用导线等表面外框的绝对组件边界。
    pub(super) widget_bounds: Rectangle,
    /// Parent viewport rectangle used by renderer clipping and translated paragraph drawing.
    /// 渲染器裁剪与平移段落绘制使用的父级视口矩形。
    pub(super) viewport: Rectangle,
    /// Widget-space origin used to translate local cached geometry into absolute renderer coordinates.
    /// 用于将局部缓存几何转换为绝对渲染坐标的组件空间原点。
    pub(super) widget_origin: Point,
    /// Effective transcript selection resolved against the live cursor state.
    /// 结合实时指针状态解析得到的有效转录选区。
    pub(super) selection: Option<(usize, usize)>,
    /// Flow-layout slice intersecting the visible viewport plus overscan policy.
    /// 与可见视口及其预取策略相交的流式布局切片。
    pub(super) visible_flow_range: Range<usize>,
    /// Visible panel-layout indexes intersecting the current viewport.
    /// 与当前视口相交的可见面板布局索引集合。
    pub(super) visible_panel_indexes: Vec<usize>,
}

/// Materializes one paint-facing transcript scene from widget bounds, pointer state, and the cached render geometry.
/// 根据组件边界、指针状态与缓存渲染几何信息物化一份面向绘制的转录场景。
pub(super) fn materialize_transcript_scene<'a>(
    document: &'a RenderedTranscriptDocument,
    state: &'a SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
    viewport: Rectangle,
) -> MaterializedTranscriptScene<'a> {
    let materialized_viewport =
        materialize_transcript_viewport(document, state, widget_bounds, cursor, viewport);
    let visible_panel_indexes =
        collect_visible_panel_indexes(state, materialized_viewport.widget_origin, viewport);

    MaterializedTranscriptScene {
        document,
        render_cache: &state.render_cache,
        collapsible_body_scroll_offsets: &state.collapsible_body_scroll_offsets,
        widget_bounds,
        viewport,
        widget_origin: materialized_viewport.widget_origin,
        selection: materialized_viewport.selection,
        visible_flow_range: materialized_viewport.visible_flow_range,
        visible_panel_indexes,
    }
}

/// Collects the panel-layout indexes whose translated bounds currently intersect the renderer viewport.
/// 收集当前平移后边界与渲染视口相交的面板布局索引。
fn collect_visible_panel_indexes(
    state: &SelectableTranscriptState,
    widget_origin: Point,
    viewport: Rectangle,
) -> Vec<usize> {
    state
        .render_cache
        .panel_layouts
        .iter()
        .enumerate()
        .filter_map(|(panel_index, panel_layout)| {
            translate_rectangle(panel_layout.outer_bounds, widget_origin)
                .intersects(&viewport)
                .then_some(panel_index)
        })
        .collect()
}

impl<'a> MaterializedTranscriptScene<'a> {
    /// Returns the visible panel layouts intersecting the current viewport in paint order.
    /// 返回按绘制顺序与当前视口相交的可见面板布局集合。
    pub(super) fn visible_panel_layouts(&'a self) -> impl Iterator<Item = &'a LaidOutPanelNode> {
        self.visible_panel_indexes
            .iter()
            .filter_map(|panel_index| self.render_cache.panel_layouts.get(*panel_index))
    }

    /// Returns the visible flow-layout slice intersecting the current viewport and overscan policy.
    /// 返回与当前视口及其预取策略相交的可见流式布局切片。
    pub(super) fn visible_flow_layouts(&'a self) -> &'a [LaidOutFlowNode] {
        &self.render_cache.flow_layouts[self.visible_flow_range.clone()]
    }

    /// Returns one cached renderer paragraph by index when it belongs to the current render cache.
    /// 当给定索引属于当前渲染缓存时，返回对应的缓存渲染段落。
    pub(super) fn paragraph(&'a self, paragraph_index: usize) -> Option<&'a RendererParagraph> {
        self.render_cache.paragraphs.get(paragraph_index)
    }

    /// Returns one cached table layout by index when it belongs to the current render cache.
    /// 当给定索引属于当前渲染缓存时，返回对应的缓存表格布局。
    pub(super) fn table_layout(&'a self, table_index: usize) -> Option<&'a CachedTableLayout> {
        self.render_cache.table_layouts.get(table_index)
    }

    /// Returns the cached paragraph slice backing one measured table layout.
    /// 返回支撑某个已测量表格布局的缓存段落切片。
    pub(super) fn table_paragraphs(
        &'a self,
        table_layout: &CachedTableLayout,
    ) -> &'a [RendererParagraph] {
        &self.render_cache.paragraphs[table_layout.paragraph_range.clone()]
    }

    /// Returns the current clamped body scroll offset for one collapsible block.
    /// 返回某个折叠区块当前经过钳制的正文滚动偏移。
    pub(super) fn collapsible_body_scroll_offset(&self, block_id: &str, max_scroll: f32) -> f32 {
        self.collapsible_body_scroll_offsets
            .get(block_id)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, max_scroll)
    }

    /// Returns the rendered panel node paired with one visible panel layout.
    /// 返回与某个可见面板布局配对的渲染面板节点。
    pub(super) fn panel_node(
        &'a self,
        panel_layout: &LaidOutPanelNode,
    ) -> Option<&'a RenderedPanelNode> {
        let RenderedTranscriptNode::Panel(panel_node) =
            self.document.nodes.get(panel_layout.node_index)?
        else {
            return None;
        };
        Some(panel_node)
    }

    /// Returns the rendered collapsible node paired with one visible collapsible flow layout.
    /// 返回与某个可见折叠流式布局配对的渲染折叠节点。
    pub(super) fn collapsible_node(
        &'a self,
        layout_node: &LaidOutCollapsibleNode,
    ) -> Option<&'a RenderedCollapsibleDirectiveNode> {
        let RenderedTranscriptNode::Collapsible(collapsible_node) =
            self.document.nodes.get(layout_node.node_index)?
        else {
            return None;
        };
        Some(collapsible_node)
    }

    /// Returns the rendered paragraph node paired with one visible paragraph flow layout.
    /// 返回与某个可见段落流式布局配对的渲染段落节点。
    pub(super) fn paragraph_node(
        &'a self,
        layout_node: &LaidOutParagraphNode,
    ) -> Option<&'a RenderedParagraphNode> {
        let RenderedTranscriptNode::Paragraph(paragraph_node) =
            self.document.nodes.get(layout_node.node_index)?
        else {
            return None;
        };
        Some(paragraph_node)
    }

    /// Returns the rendered table node paired with one visible table flow layout.
    /// 返回与某个可见表格流式布局配对的渲染表格节点。
    pub(super) fn table_node(
        &'a self,
        layout_node: &LaidOutTableNode,
    ) -> Option<&'a RenderedTableNode> {
        let RenderedTranscriptNode::Table(table_node) =
            self.document.nodes.get(layout_node.node_index)?
        else {
            return None;
        };
        Some(table_node)
    }

    /// Returns the rendered media-card node paired with one visible media-card flow layout.
    /// 返回与某个可见媒体卡片流式布局配对的渲染媒体卡片节点。
    pub(super) fn media_card_node(
        &'a self,
        layout_node: &LaidOutMediaCardNode,
    ) -> Option<&'a RenderedMediaCardNode> {
        let RenderedTranscriptNode::MediaCard(media_card_node) =
            self.document.nodes.get(layout_node.node_index)?
        else {
            return None;
        };
        Some(media_card_node)
    }

    /// Returns the rendered rule node paired with one visible rule flow layout.
    /// 返回与某个可见分隔线流式布局配对的渲染分隔线节点。
    pub(super) fn rule_node(
        &'a self,
        layout_node: &LaidOutRuleNode,
    ) -> Option<&'a RenderedRuleNode> {
        let RenderedTranscriptNode::Rule(rule_node) =
            self.document.nodes.get(layout_node.node_index)?
        else {
            return None;
        };
        Some(rule_node)
    }
}
