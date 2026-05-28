//! Render-cache state separated from live interaction state so long-document layout data can evolve independently.
//! 与实时交互状态分离的渲染缓存状态，使长文档布局数据可以独立演进。

use super::RendererParagraph;
use super::perf::TranscriptPerfTrace;
use super::scene::{
    CachedTableLayout, LaidOutFlowNode, LaidOutPanelNode, RenderedTranscriptDocument,
    RenderedTranscriptLinkAction, RenderedTranscriptNode,
};
use crate::AgentViewDocument;
use iced::Theme;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use vulcan_markdown as markdown;

thread_local! {
    /// Surface-scoped transcript render-cache stash that survives widget-state teardown during session switching.
    /// 在会话切换导致组件状态销毁时仍可存活的按表面归属转录渲染缓存暂存区。
    static STASHED_SURFACE_RENDER_CACHES: RefCell<BTreeMap<String, TranscriptRenderCache>> =
        RefCell::new(BTreeMap::new());
}

const MAX_LAYOUT_CACHE_SLOTS: usize = 4;

/// Clears all stashed surface-scoped render caches so future transcript widgets must rebuild from fresh state.
/// 清空所有按表面归属暂存的渲染缓存，使后续转录组件必须从全新状态重建。
pub(super) fn clear_stashed_surface_render_caches() {
    STASHED_SURFACE_RENDER_CACHES.with(|stashed_caches| {
        stashed_caches.borrow_mut().clear();
    });
}

/// Stores one reusable parsed Markdown snapshot aligned with one rendered transcript block.
/// 存储一份与单个渲染转录区块对齐的可复用 Markdown 解析快照。
#[derive(Debug, Clone)]
pub(super) struct CachedRenderedMarkdownParse {
    /// Prepared Markdown source string that was used to produce the parsed content snapshot.
    /// 用于生成当前解析内容快照的预处理 Markdown 源文本。
    pub(super) prepared_source: String,
    /// Parsed Markdown content snapshot that can seed future incremental suffix appends.
    /// 可为未来尾部增量追加提供种子的已解析 Markdown 内容快照。
    pub(super) content: markdown::Content,
}

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
    /// Optional parsed Markdown snapshot retained only for incrementally growing Markdown blocks.
    /// 仅为支持增量增长 Markdown 区块而保留的可选已解析快照。
    pub(super) markdown_parse: Option<CachedRenderedMarkdownParse>,
}

/// Stores one complete reusable layout snapshot for a specific rendered-document signature and width.
/// 存储针对特定渲染文档签名与宽度的一份完整可复用布局快照。
pub(super) struct TranscriptLayoutCacheSlot {
    /// Stable rendered-document signature associated with this layout.
    /// 与该布局关联的稳定渲染文档签名。
    signature: u64,
    /// Rounded width key used to tolerate sub-pixel width noise while keeping distinct layout widths apart.
    /// 用于容忍亚像素宽度噪声、同时区分不同布局宽度的舍入宽度键。
    width_key: i32,
    /// Exact width used by the original layout build.
    /// 原始布局构建时使用的精确宽度。
    width: f32,
    /// Cached paragraphs aligned with text-bearing nodes.
    /// 与文本节点对齐的缓存段落集合。
    paragraphs: Vec<RendererParagraph>,
    /// Stable paragraph-render signatures aligned with paragraph slots.
    /// 与段落槽位对齐的稳定段落渲染签名。
    paragraph_signatures: Vec<u64>,
    /// Cached table layouts aligned with rendered table nodes.
    /// 与渲染表格节点对齐的缓存表格布局集合。
    table_layouts: Vec<CachedTableLayout>,
    /// Cached flow-node layout metrics in visual order.
    /// 按可视顺序保存的流式节点布局指标缓存。
    flow_layouts: Vec<LaidOutFlowNode>,
    /// Stable layout signatures aligned with flow nodes.
    /// 与流式节点对齐的稳定布局签名。
    flow_layout_signatures: Vec<u64>,
    /// Cached background-panel layouts.
    /// 缓存的背景面板布局。
    panel_layouts: Vec<LaidOutPanelNode>,
    /// Height produced by this cached layout.
    /// 该缓存布局产出的高度。
    height: f32,
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
    /// Recently built alternate layout snapshots for the same surface, keyed by document signature and width.
    /// 同一表面最近构建过的备用布局快照，按文档签名与宽度区分。
    layout_slots: Vec<TranscriptLayoutCacheSlot>,
}

impl TranscriptRenderCache {
    /// Ensures the cached rendered transcript document reflects the provided source document and returns whether it had to be rebuilt.
    /// 确保缓存的渲染转录文档与给定源文档一致，并返回本次是否发生了重建。
    pub(super) fn ensure_rendered_document(
        &mut self,
        document: &AgentViewDocument,
        app_theme: &Theme,
    ) -> bool {
        let mut perf_trace = TranscriptPerfTrace::begin(
            "ensure_rendered_document",
            format!("blocks={}", document.blocks.len()),
        );
        let mut phase_started_at = Instant::now();
        let signature =
            RenderedTranscriptDocument::layout_signature_for_agent_document(document, app_theme);
        perf_trace.checkpoint("layout_signature_for_agent_document", phase_started_at);
        if self.rendered_document_signature == signature && self.rendered_document.is_some() {
            perf_trace.finish();
            return false;
        }

        phase_started_at = Instant::now();
        self.rendered_document = Some(
            RenderedTranscriptDocument::from_agent_document_with_block_cache_and_signature(
                document,
                app_theme,
                &mut self.rendered_blocks,
                signature,
            ),
        );
        perf_trace.checkpoint("rebuild_rendered_document", phase_started_at);
        self.rendered_document_signature = signature;
        phase_started_at = Instant::now();
        self.prune_rendered_blocks(document, app_theme);
        perf_trace.checkpoint("prune_rendered_blocks", phase_started_at);
        perf_trace.finish();
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

    /// Restores a previously built alternate-width layout into the active cache when available.
    /// 如果存在匹配的备用宽度布局，则将其恢复为当前活动缓存。
    pub(super) fn restore_layout_slot(
        &mut self,
        layout_signature: u64,
        available_width: f32,
    ) -> bool {
        let Some(slot_index) = self.layout_slots.iter().position(|slot| {
            slot.signature == layout_signature
                && slot.width_key == layout_width_key(available_width)
        }) else {
            return false;
        };

        self.stash_active_layout_slot();
        let slot = self.layout_slots.remove(slot_index);
        self.paragraphs = slot.paragraphs;
        self.paragraph_signatures = slot.paragraph_signatures;
        self.table_layouts = slot.table_layouts;
        self.flow_layouts = slot.flow_layouts;
        self.flow_layout_signatures = slot.flow_layout_signatures;
        self.panel_layouts = slot.panel_layouts;
        self.layout_ready = true;
        self.cached_width = slot.width;
        self.cached_signature = slot.signature;
        self.cached_height = slot.height;
        true
    }

    /// Moves the currently active layout into the alternate-width slot list before it is replaced.
    /// 在当前活动布局被替换之前，将其移动到备用宽度槽列表中。
    pub(super) fn stash_active_layout_slot(&mut self) {
        if !self.layout_ready || self.cached_signature == 0 || self.flow_layouts.is_empty() {
            return;
        }

        let slot = TranscriptLayoutCacheSlot {
            signature: self.cached_signature,
            width_key: layout_width_key(self.cached_width),
            width: self.cached_width,
            paragraphs: std::mem::take(&mut self.paragraphs),
            paragraph_signatures: std::mem::take(&mut self.paragraph_signatures),
            table_layouts: std::mem::take(&mut self.table_layouts),
            flow_layouts: std::mem::take(&mut self.flow_layouts),
            flow_layout_signatures: std::mem::take(&mut self.flow_layout_signatures),
            panel_layouts: std::mem::take(&mut self.panel_layouts),
            height: self.cached_height,
        };
        self.layout_ready = false;
        self.cached_width = 0.0;
        self.cached_signature = 0;
        self.cached_height = 1.0;
        self.push_layout_slot(slot);
    }

    /// Adds one alternate layout slot while keeping only a small most-recent set.
    /// 添加一个备用布局槽，并仅保留少量最近使用的槽位。
    fn push_layout_slot(&mut self, slot: TranscriptLayoutCacheSlot) {
        self.layout_slots.retain(|existing| {
            existing.signature != slot.signature || existing.width_key != slot.width_key
        });
        self.layout_slots.push(slot);
        while self.layout_slots.len() > MAX_LAYOUT_CACHE_SLOTS {
            self.layout_slots.remove(0);
        }
    }

    /// Returns a small quality score describing how useful this cache is for a future surface restore.
    /// 返回一个描述该缓存对后续表面恢复有多大复用价值的小型质量分数。
    fn reuse_score(&self) -> u8 {
        if self.layout_ready && self.cached_signature != 0 && !self.flow_layouts.is_empty() {
            2
        } else if self.rendered_document.is_some() || !self.rendered_blocks.is_empty() {
            1
        } else {
            0
        }
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
            layout_slots: Vec::new(),
        }
    }
}

/// Converts a logical width into a stable half-pixel bucket for alternate layout-cache lookup.
/// 将逻辑宽度转换为稳定的半像素桶，用于备用布局缓存查找。
fn layout_width_key(width: f32) -> i32 {
    (width * 2.0).round() as i32
}

/// Rebinds one live render cache to the provided surface key, stashing the previous cache and restoring any matching cached payload for the new surface.
/// 将一个活动渲染缓存重新绑定到提供的表面键，暂存旧缓存并为新表面恢复任何匹配的已缓存载荷。
pub(super) fn rebind_stashed_surface_render_cache(
    render_cache: &mut TranscriptRenderCache,
    bound_surface_cache_key: &mut Option<String>,
    next_surface_cache_key: Option<&str>,
) {
    if bound_surface_cache_key.as_deref() == next_surface_cache_key {
        return;
    }

    let next_surface_cache_key = next_surface_cache_key.map(ToOwned::to_owned);
    STASHED_SURFACE_RENDER_CACHES.with(|stashed_caches| {
        let mut stashed_caches = stashed_caches.borrow_mut();
        if let Some(previous_surface_cache_key) = bound_surface_cache_key.take() {
            stash_render_cache_if_reusable(
                &mut stashed_caches,
                previous_surface_cache_key,
                std::mem::take(render_cache),
            );
        }

        if let Some(ref next_surface_cache_key) = next_surface_cache_key {
            *render_cache = stashed_caches
                .remove(next_surface_cache_key)
                .unwrap_or_default();
        } else {
            *render_cache = TranscriptRenderCache::default();
        }
    });
    *bound_surface_cache_key = next_surface_cache_key;
}

/// Stashes one live render cache under the provided surface key so a later widget instance can restore it after session switching.
/// 使用提供的表面键暂存一份活动渲染缓存，以便稍后的组件实例在会话切换后恢复它。
pub(super) fn stash_surface_render_cache(
    render_cache: &mut TranscriptRenderCache,
    bound_surface_cache_key: &mut Option<String>,
) {
    let Some(surface_cache_key) = bound_surface_cache_key.take() else {
        return;
    };

    STASHED_SURFACE_RENDER_CACHES.with(|stashed_caches| {
        stash_render_cache_if_reusable(
            &mut stashed_caches.borrow_mut(),
            surface_cache_key,
            std::mem::take(render_cache),
        );
    });
}

/// Stores one cache only when it contains reusable render or layout payload and does not downgrade an existing warm cache.
/// 仅在缓存包含可复用渲染或布局载荷时保存，并避免用低质量缓存覆盖已有热缓存。
fn stash_render_cache_if_reusable(
    stashed_caches: &mut BTreeMap<String, TranscriptRenderCache>,
    surface_cache_key: String,
    render_cache: TranscriptRenderCache,
) {
    let incoming_score = render_cache.reuse_score();
    if incoming_score == 0 {
        return;
    }
    let existing_score = stashed_caches
        .get(&surface_cache_key)
        .map(TranscriptRenderCache::reuse_score)
        .unwrap_or(0);
    if incoming_score >= existing_score {
        stashed_caches.insert(surface_cache_key, render_cache);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TranscriptRenderCache, clear_stashed_surface_render_caches,
        rebind_stashed_surface_render_cache, stash_surface_render_cache,
    };
    use crate::iced::scene::LaidOutRuleNode;
    use std::collections::BTreeMap;

    /// Verifies one stashed surface render cache can be restored by a later widget instance bound to the same surface identifier.
    /// 验证一份已暂存的表面渲染缓存可以被后续绑定到相同表面标识的组件实例恢复出来。
    #[test]
    fn stashed_surface_render_cache_round_trips() {
        let unique_surface_id = String::from("surface:test:roundtrip");
        let mut original_cache = TranscriptRenderCache::default();
        original_cache.cached_signature = 42;
        original_cache.cached_width = 777.0;
        original_cache.rendered_blocks.insert(
            42,
            super::CachedRenderedTranscriptBlock {
                signature: 42,
                nodes: Vec::new(),
                full_text: String::from("cached"),
                full_text_char_count: 6,
                link_actions: BTreeMap::new(),
                markdown_parse: None,
            },
        );
        let mut original_surface_key = Some(unique_surface_id.clone());

        stash_surface_render_cache(&mut original_cache, &mut original_surface_key);
        assert!(original_surface_key.is_none());
        assert_eq!(original_cache.cached_signature, 0);

        let mut restored_cache = TranscriptRenderCache::default();
        let mut restored_surface_key = None;
        rebind_stashed_surface_render_cache(
            &mut restored_cache,
            &mut restored_surface_key,
            Some(unique_surface_id.as_str()),
        );

        assert_eq!(
            restored_surface_key.as_deref(),
            Some(unique_surface_id.as_str())
        );
        assert_eq!(restored_cache.cached_signature, 42);
        assert_eq!(restored_cache.cached_width, 777.0);
    }

    /// Verifies clearing the stashed surface cache forces later bindings to start from a cold render-cache state.
    /// 验证清空暂存表面缓存后，后续绑定会强制回到冷启动渲染缓存状态。
    #[test]
    fn clearing_stashed_surface_render_caches_discards_old_entries() {
        let unique_surface_id = String::from("surface:test:clear");
        let mut original_cache = TranscriptRenderCache::default();
        original_cache.cached_signature = 99;
        let mut original_surface_key = Some(unique_surface_id.clone());

        stash_surface_render_cache(&mut original_cache, &mut original_surface_key);
        clear_stashed_surface_render_caches();

        let mut restored_cache = TranscriptRenderCache::default();
        let mut restored_surface_key = None;
        rebind_stashed_surface_render_cache(
            &mut restored_cache,
            &mut restored_surface_key,
            Some(unique_surface_id.as_str()),
        );

        assert_eq!(
            restored_surface_key.as_deref(),
            Some(unique_surface_id.as_str())
        );
        assert_eq!(restored_cache.cached_signature, 0);
    }

    /// Verifies a cold teardown cache cannot overwrite an already warm stashed layout cache.
    /// 验证冷销毁缓存不能覆盖已经暂存的热布局缓存。
    #[test]
    fn stashed_surface_render_cache_keeps_warm_cache_when_cold_cache_arrives() {
        clear_stashed_surface_render_caches();
        let unique_surface_id = String::from("surface:test:warm-not-overwritten");
        let mut warm_cache = TranscriptRenderCache::default();
        warm_cache.layout_ready = true;
        warm_cache.cached_signature = 123;
        warm_cache.cached_width = 640.0;
        warm_cache
            .flow_layouts
            .push(super::LaidOutFlowNode::Rule(LaidOutRuleNode {
                node_index: 0,
                outer_bounds: iced::Rectangle::new(
                    iced::Point::ORIGIN,
                    iced::Size::new(640.0, 8.0),
                ),
                flow_end_y: 8.0,
            }));
        let mut warm_surface_key = Some(unique_surface_id.clone());
        stash_surface_render_cache(&mut warm_cache, &mut warm_surface_key);

        let mut cold_cache = TranscriptRenderCache::default();
        let mut cold_surface_key = Some(unique_surface_id.clone());
        stash_surface_render_cache(&mut cold_cache, &mut cold_surface_key);

        let mut restored_cache = TranscriptRenderCache::default();
        let mut restored_surface_key = None;
        rebind_stashed_surface_render_cache(
            &mut restored_cache,
            &mut restored_surface_key,
            Some(unique_surface_id.as_str()),
        );

        assert_eq!(restored_cache.cached_signature, 123);
        assert_eq!(restored_cache.cached_width, 640.0);
        assert!(restored_cache.layout_ready);
    }

    /// Verifies alternate width layout slots can be restored after width jitter swaps the active cache.
    /// 验证宽度抖动切换活动缓存后，可以恢复备用宽度布局槽。
    #[test]
    fn alternate_width_layout_slots_round_trip() {
        let mut cache = TranscriptRenderCache::default();
        cache.layout_ready = true;
        cache.cached_signature = 55;
        cache.cached_width = 1119.0;
        cache.cached_height = 200.0;
        cache
            .flow_layouts
            .push(super::LaidOutFlowNode::Rule(LaidOutRuleNode {
                node_index: 0,
                outer_bounds: iced::Rectangle::new(
                    iced::Point::ORIGIN,
                    iced::Size::new(1119.0, 8.0),
                ),
                flow_end_y: 200.0,
            }));

        cache.stash_active_layout_slot();
        cache.layout_ready = true;
        cache.cached_signature = 55;
        cache.cached_width = 1143.0;
        cache.cached_height = 240.0;
        cache
            .flow_layouts
            .push(super::LaidOutFlowNode::Rule(LaidOutRuleNode {
                node_index: 0,
                outer_bounds: iced::Rectangle::new(
                    iced::Point::ORIGIN,
                    iced::Size::new(1143.0, 8.0),
                ),
                flow_end_y: 240.0,
            }));

        assert!(cache.restore_layout_slot(55, 1119.0));
        assert_eq!(cache.cached_width, 1119.0);
        assert_eq!(cache.cached_height, 200.0);
        assert!(cache.restore_layout_slot(55, 1143.0));
        assert_eq!(cache.cached_width, 1143.0);
        assert_eq!(cache.cached_height, 240.0);
    }
}
