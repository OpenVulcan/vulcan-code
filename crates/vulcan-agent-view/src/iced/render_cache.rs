//! Render-cache state separated from live interaction state so long-document layout data can evolve independently.
//! 与实时交互状态分离的渲染缓存状态，使长文档布局数据可以独立演进。

use super::RendererParagraph;
use super::scene::{
    CachedTableLayout, LaidOutFlowNode, LaidOutPanelNode, RenderedTranscriptDocument,
    RenderedTranscriptLinkAction, RenderedTranscriptNode,
};
use crate::AgentViewDocument;
use iced::Theme;
use std::collections::{BTreeMap, BTreeSet};

/// Stores the reusable rendered-node projection for one top-level transcript block.
/// 存储单个顶层转录区块可复用的渲染节点投影。
#[derive(Debug, Clone)]
pub(super) struct CachedRenderedTranscriptBlock {
    /// Stable block-render signature derived from source content, attributes, and render-affecting theme inputs.
    /// 根据源码内容、属性以及影响渲染的主题输入派生出的稳定区块渲染签名。
    pub(super) signature: u64,
    /// Render nodes produced by this block with paragraph character ranges kept block-local.
    /// 当前区块产出的渲染节点，段落字符范围保持为区块局部范围。
    pub(super) nodes: Vec<RenderedTranscriptNode>,
    /// Copyable text projected by this block without outer inter-block separators.
    /// 当前区块投影出的可复制文本，不包含外层区块间分隔符。
    pub(super) full_text: String,
    /// Character count of the copyable text, cached to avoid recounting long transcript prefixes on every stream tick.
    /// 可复制文本的字符数缓存，用于避免每次流式刷新都重新统计长转录前缀。
    pub(super) full_text_char_count: usize,
    /// Synthetic link actions registered by this block.
    /// 当前区块注册的合成链接动作集合。
    pub(super) link_actions: BTreeMap<String, RenderedTranscriptLinkAction>,
}

/// Stores the heavy layout and paragraph caches used to render long transcript surfaces efficiently.
/// 存储用于高效渲染长转录表面的重型布局与段落缓存。
pub(super) struct TranscriptRenderCache {
    /// Cached rendered transcript document assembled from block-level render caches.
    /// 由区块级渲染缓存组装出的已缓存转录渲染文档。
    pub(super) rendered_document: Option<RenderedTranscriptDocument>,
    /// Stable signature for the currently cached rendered document.
    /// 当前已缓存渲染文档对应的稳定签名。
    pub(super) rendered_document_signature: u64,
    /// Rendered block cache keyed by stable per-block render signatures.
    /// 以每个区块稳定渲染签名为键保存的区块渲染缓存。
    pub(super) rendered_blocks: BTreeMap<u64, CachedRenderedTranscriptBlock>,
    /// Cached paragraphs aligned with text-bearing nodes in the rendered transcript.
    /// 与渲染转录中文本节点对齐的缓存段落集合。
    pub(super) paragraphs: Vec<RendererParagraph>,
    /// Stable paragraph-render signatures aligned with the paragraph cache slots.
    /// 与段落缓存槽位对齐的稳定段落渲染签名。
    pub(super) paragraph_signatures: Vec<u64>,
    /// Cached table layouts aligned with rendered table nodes in the transcript.
    /// 与转录中渲染表格节点对齐的缓存表格布局集合。
    pub(super) table_layouts: Vec<CachedTableLayout>,
    /// Cached flow-node layout metrics kept in visual order for viewport-range culling and hit testing.
    /// 按可视顺序保留的流式节点布局指标缓存，用于视口裁剪与命中测试。
    pub(super) flow_layouts: Vec<LaidOutFlowNode>,
    /// Stable layout signatures aligned with flow nodes so unchanged prefixes can skip remeasurement.
    /// 与流式节点对齐的稳定布局签名，使未变化前缀可以跳过重新测量。
    pub(super) flow_layout_signatures: Vec<u64>,
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
    /// Ensures the cached rendered transcript document reflects the provided source document and returns whether it had to be rebuilt.
    /// 确保缓存的渲染转录文档与给定源文档一致，并返回本次是否发生了重建。
    pub(super) fn ensure_rendered_document(
        &mut self,
        document: &AgentViewDocument,
        app_theme: &Theme,
    ) -> bool {
        let signature =
            RenderedTranscriptDocument::layout_signature_for_agent_document(document, app_theme);
        if self.rendered_document_signature == signature && self.rendered_document.is_some() {
            return false;
        }

        self.rendered_document = Some(
            RenderedTranscriptDocument::from_agent_document_with_block_cache_and_signature(
                document,
                app_theme,
                &mut self.rendered_blocks,
                signature,
            ),
        );
        self.rendered_document_signature = signature;
        self.prune_rendered_blocks(document, app_theme);
        true
    }

    /// Removes block render-cache entries that are no longer referenced by the current source document.
    /// 移除当前源文档已经不再引用的区块渲染缓存项。
    fn prune_rendered_blocks(&mut self, document: &AgentViewDocument, app_theme: &Theme) {
        let active_signatures = document
            .blocks
            .iter()
            .map(|block| {
                RenderedTranscriptDocument::block_render_signature_for_agent_block(block, app_theme)
            })
            .collect::<BTreeSet<_>>();

        self.rendered_blocks
            .retain(|signature, _| active_signatures.contains(signature));
    }

    /// Returns the cached rendered transcript document when one has already been materialized.
    /// 在已物化渲染转录文档时返回其缓存引用。
    pub(super) fn rendered_document(&self) -> Option<&RenderedTranscriptDocument> {
        self.rendered_document.as_ref()
    }

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
            rendered_document: None,
            rendered_document_signature: 0,
            rendered_blocks: BTreeMap::new(),
            paragraphs: Vec::new(),
            paragraph_signatures: Vec::new(),
            table_layouts: Vec::new(),
            flow_layouts: Vec::new(),
            flow_layout_signatures: Vec::new(),
            panel_layouts: Vec::new(),
            layout_ready: false,
            cached_width: 0.0,
            cached_signature: 0,
            cached_height: 1.0,
        }
    }
}
