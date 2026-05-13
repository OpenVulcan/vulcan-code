//! Render-cache state separated from live interaction state so long-document layout data can evolve independently.
//! 与实时交互状态分离的渲染缓存状态，使长文档布局数据可以独立演进。

use super::RendererParagraph;
use super::scene::{CachedTableLayout, LaidOutFlowNode, LaidOutPanelNode};

/// Stores the heavy layout and paragraph caches used to render long transcript surfaces efficiently.
/// 存储用于高效渲染长转录表面的重型布局与段落缓存。
pub(super) struct TranscriptRenderCache {
    /// Cached paragraphs aligned with text-bearing nodes in the rendered transcript.
    /// 与渲染转录中文本节点对齐的缓存段落集合。
    pub(super) paragraphs: Vec<RendererParagraph>,
    /// Cached table layouts aligned with rendered table nodes in the transcript.
    /// 与转录中渲染表格节点对齐的缓存表格布局集合。
    pub(super) table_layouts: Vec<CachedTableLayout>,
    /// Cached flow-node layout metrics kept in visual order for viewport-range culling and hit testing.
    /// 按可视顺序保留的流式节点布局指标缓存，用于视口裁剪与命中测试。
    pub(super) flow_layouts: Vec<LaidOutFlowNode>,
    /// Cached background-panel layout metrics resolved from the current flow-node geometry.
    /// 根据当前流式节点几何信息解析得到的背景面板布局指标缓存。
    pub(super) panel_layouts: Vec<LaidOutPanelNode>,
    /// Whether the current paragraph and flow-layout caches are valid for the last measured width and document signature.
    /// 当前段落与流式布局缓存是否对上一次测量宽度和文档签名仍然有效。
    pub(super) layout_ready: bool,
    /// Width used by the last successful layout-cache build.
    /// 最近一次成功构建布局缓存时使用的宽度。
    pub(super) cached_width: f32,
    /// Stable rendered-document signature used to invalidate cached layout metrics when content or theme changes.
    /// 当内容或主题变化时用于使布局缓存失效的稳定渲染文档签名。
    pub(super) cached_signature: u64,
    /// Height produced by the last successful layout-cache build.
    /// 最近一次成功构建布局缓存时产出的高度。
    pub(super) cached_height: f32,
}

impl TranscriptRenderCache {
    /// Returns whether the current cache can be safely reused for the provided width and document signature.
    /// 返回当前缓存是否可以针对给定宽度与文档签名安全复用。
    pub(super) fn matches_layout(&self, layout_signature: u64, available_width: f32) -> bool {
        self.layout_ready
            && self.cached_signature == layout_signature
            && (self.cached_width - available_width).abs() <= 0.5
    }
}

impl Default for TranscriptRenderCache {
    /// Creates one empty render-cache state with no reusable geometry.
    /// 创建一个不包含可复用几何信息的空渲染缓存状态。
    fn default() -> Self {
        Self {
            paragraphs: Vec::new(),
            table_layouts: Vec::new(),
            flow_layouts: Vec::new(),
            panel_layouts: Vec::new(),
            layout_ready: false,
            cached_width: 0.0,
            cached_signature: 0,
            cached_height: 1.0,
        }
    }
}
