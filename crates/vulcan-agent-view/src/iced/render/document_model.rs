//! Rendered transcript document construction and layout-signature helpers for the selectable transcript surface.
//! 可选择转录表面的渲染文档构建与布局签名辅助逻辑。

use super::builder::RenderedTranscriptBuilder;
use super::directive_render::render_directive_block;
use super::markdown_render::{render_markdown_items, render_markdown_source_with_table_fallback};
use super::markdown_source::{
    prepare_markdown_source_for_rendering, raw_source_contains_markdown_table,
};
use super::render_cache::{CachedRenderedMarkdownParse, CachedRenderedTranscriptBlock};
use super::scene::{
    PlainParagraphStyle, RenderContext, RenderedParagraphBackground, RenderedTranscriptDocument,
    RenderedTranscriptNode,
};
use crate::iced_theme as theme;
use crate::{AgentViewBlock, AgentViewDirectiveKind, AgentViewDocument};
use iced::{Background, Border, Color, Theme};
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use vulcan_markdown as markdown;

impl RenderedTranscriptDocument {
    /// Builds one rendered transcript model from the structured agent document.
    /// 根据结构化代理文档构建一份渲染转录模型。
    #[cfg(test)]
    pub(super) fn from_agent_document(document: &AgentViewDocument, app_theme: &Theme) -> Self {
        let mut block_cache = BTreeMap::new();
        Self::from_agent_document_with_block_cache(document, app_theme, &mut block_cache)
    }

    /// Builds one rendered transcript model from block-level render caches, refreshing only changed blocks.
    /// 基于区块级渲染缓存构建渲染转录模型，仅刷新已经变化的区块。
    #[cfg(test)]
    pub(super) fn from_agent_document_with_block_cache(
        document: &AgentViewDocument,
        app_theme: &Theme,
        block_cache: &mut BTreeMap<u64, CachedRenderedTranscriptBlock>,
    ) -> Self {
        let layout_signature = Self::layout_signature_for_agent_document(document, app_theme);
        Self::from_agent_document_with_block_cache_and_signature(
            document,
            app_theme,
            block_cache,
            layout_signature,
        )
    }

    /// Builds one rendered transcript model using a caller-provided layout signature so long documents are not hashed twice.
    /// 使用调用方提供的布局签名构建渲染转录模型，避免长文档被重复哈希。
    pub(super) fn from_agent_document_with_block_cache_and_signature(
        document: &AgentViewDocument,
        app_theme: &Theme,
        block_cache: &mut BTreeMap<u64, CachedRenderedTranscriptBlock>,
        layout_signature: u64,
    ) -> Self {
        let palette = app_theme.palette();
        let selection_color = palette.primary.weak.color.scale_alpha(0.28);
        let fallback_text_color = palette.background.base.text.scale_alpha(0.94);
        let mut nodes = Vec::new();
        let mut full_text = String::new();
        let mut full_text_char_count = 0usize;
        let mut link_actions = BTreeMap::new();

        for block in &document.blocks {
            let block_signature = Self::block_render_signature_for_agent_block(block, app_theme);
            if !block_cache.contains_key(&block_signature) {
                let rendered_block = render_agent_view_block(
                    block_signature,
                    block,
                    app_theme,
                    selection_color,
                    fallback_text_color,
                    block_cache,
                );
                block_cache.insert(block_signature, rendered_block);
            }
            let cached_block = block_cache
                .get(&block_signature)
                .expect("rendered block must be cached after insertion");
            debug_assert_eq!(cached_block.signature, block_signature);

            let separator = if full_text.is_empty() || cached_block.full_text.trim().is_empty() {
                ""
            } else {
                "\n\n"
            };
            let separator_char_count = separator.chars().count();
            let block_char_offset = full_text_char_count + separator_char_count;
            if !separator.is_empty() {
                full_text.push_str(separator);
            }
            full_text.push_str(&cached_block.full_text);
            full_text_char_count = block_char_offset + cached_block.full_text_char_count;

            nodes.extend(cached_block.nodes.iter().cloned().map(|mut node| {
                shift_rendered_node_char_ranges(&mut node, block_char_offset);
                node
            }));
            link_actions.extend(cached_block.link_actions.clone());
        }

        Self {
            nodes,
            full_text,
            selection_color,
            link_actions,
            layout_signature,
        }
    }

    /// Computes the full-document layout signature used by render and layout caches.
    /// 计算渲染与布局缓存使用的整份文档布局签名。
    pub(super) fn layout_signature_for_agent_document(
        document: &AgentViewDocument,
        app_theme: &Theme,
    ) -> u64 {
        compute_rendered_transcript_layout_signature(document, app_theme)
    }

    /// Computes the render signature for one top-level agent-view block.
    /// 计算单个顶层 agent-view 区块的渲染签名。
    pub(super) fn block_render_signature_for_agent_block(
        block: &AgentViewBlock,
        app_theme: &Theme,
    ) -> u64 {
        compute_rendered_transcript_block_signature(block, app_theme)
    }
}

/// Renders one top-level agent block into a block-local cached rendered projection.
/// 将单个顶层代理区块渲染为区块局部的可缓存渲染投影。
fn render_agent_view_block(
    signature: u64,
    block: &AgentViewBlock,
    app_theme: &Theme,
    selection_color: Color,
    fallback_text_color: Color,
    block_cache: &BTreeMap<u64, CachedRenderedTranscriptBlock>,
) -> CachedRenderedTranscriptBlock {
    let markdown_settings = theme::markdown_settings(app_theme);
    let mut builder = RenderedTranscriptBuilder::new(selection_color, fallback_text_color);
    let child_start_index = builder.node_count();
    builder.begin_block();
    let mut markdown_parse = None;

    match block {
        AgentViewBlock::Markdown(markdown_block) => {
            let prepared_source = prepare_markdown_source_for_rendering(&markdown_block.raw_source);
            if raw_source_contains_markdown_table(&prepared_source) {
                render_markdown_source_with_table_fallback(
                    &mut builder,
                    &prepared_source,
                    PlainParagraphStyle::flow(markdown_settings.text_size),
                    markdown_settings,
                    app_theme,
                );
            } else {
                let markdown_content =
                    incremental_markdown_content_from_cache(block_cache, &prepared_source)
                        .unwrap_or_else(|| markdown::Content::parse_transcript(&prepared_source));
                markdown_parse = Some(CachedRenderedMarkdownParse {
                    prepared_source,
                    content: markdown_content.clone(),
                });
                render_markdown_items(
                    &mut builder,
                    markdown_content.items(),
                    RenderContext::default(),
                    markdown_settings,
                    app_theme,
                );
            }
        }
        AgentViewBlock::Directive(directive_block) => {
            render_directive_block(&mut builder, directive_block, markdown_settings, app_theme);
        }
    }

    if block_has_workspace_history_search_hit_marker(block) {
        let child_node_count = builder.node_count().saturating_sub(child_start_index);
        builder.insert_panel(
            child_start_index,
            child_node_count,
            workspace_history_search_hit_background(app_theme),
        );
    }

    CachedRenderedTranscriptBlock {
        signature,
        full_text_char_count: builder.full_text.chars().count(),
        nodes: builder.nodes,
        full_text: builder.full_text,
        link_actions: builder.link_actions,
        markdown_parse,
    }
}

/// Restores one parsed Markdown snapshot from the nearest cached prefix match and appends only the new suffix when possible.
/// 尝试从最近的缓存前缀匹配恢复一份已解析 Markdown 快照，并在可行时只追加新增后缀。
fn incremental_markdown_content_from_cache(
    block_cache: &BTreeMap<u64, CachedRenderedTranscriptBlock>,
    prepared_source: &str,
) -> Option<markdown::Content> {
    let seed = block_cache
        .values()
        .filter_map(|cached_block| cached_block.markdown_parse.as_ref())
        .filter(|cached_parse| {
            prepared_source.len() > cached_parse.prepared_source.len()
                && prepared_source.starts_with(cached_parse.prepared_source.as_str())
        })
        .max_by_key(|cached_parse| cached_parse.prepared_source.len())?;

    let mut content = seed.content.clone();
    let suffix = &prepared_source[seed.prepared_source.len()..];
    content.push_str(suffix);
    Some(content)
}

/// Computes one stable transcript-layout signature from the source document and render-affecting theme inputs.
/// 根据源文档与影响渲染的主题输入计算一份稳定的转录布局签名。
fn compute_rendered_transcript_layout_signature(
    document: &AgentViewDocument,
    app_theme: &Theme,
) -> u64 {
    let markdown_settings = theme::markdown_settings(app_theme);
    let palette = app_theme.palette();
    let mut hasher = DefaultHasher::new();

    document.blocks.len().hash(&mut hasher);
    markdown_settings.text_size.0.to_bits().hash(&mut hasher);
    hash_render_color(palette.background.base.color, &mut hasher);
    hash_render_color(palette.background.base.text, &mut hasher);
    hash_render_color(palette.primary.weak.color, &mut hasher);
    hash_render_color(palette.primary.strong.color, &mut hasher);
    hash_render_color(palette.secondary.strong.color, &mut hasher);

    for block in &document.blocks {
        hash_agent_view_block_layout_identity(block, &mut hasher);
        hash_block_attributes(block, &mut hasher);
    }

    hasher.finish()
}

/// Computes one stable rendered-block signature from the source block and render-affecting theme inputs.
/// 根据源区块与影响渲染的主题输入计算稳定的区块渲染签名。
fn compute_rendered_transcript_block_signature(block: &AgentViewBlock, app_theme: &Theme) -> u64 {
    let markdown_settings = theme::markdown_settings(app_theme);
    let palette = app_theme.palette();
    let mut hasher = DefaultHasher::new();

    markdown_settings.text_size.0.to_bits().hash(&mut hasher);
    hash_render_color(palette.background.base.color, &mut hasher);
    hash_render_color(palette.background.base.text, &mut hasher);
    hash_render_color(palette.primary.weak.color, &mut hasher);
    hash_render_color(palette.primary.strong.color, &mut hasher);
    hash_render_color(palette.secondary.strong.color, &mut hasher);
    hash_agent_view_block_layout_identity(block, &mut hasher);
    hash_block_attributes(block, &mut hasher);

    hasher.finish()
}

/// Hashes the render-visible identity of one agent-view block, excluding hidden compact-tool bodies.
/// 哈希单个 agent-view 区块的渲染可见身份，并排除隐藏的紧凑工具正文。
fn hash_agent_view_block_layout_identity(block: &AgentViewBlock, hasher: &mut DefaultHasher) {
    match block {
        AgentViewBlock::Directive(directive)
            if matches!(
                directive.kind,
                AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
            ) =>
        {
            "compact-tool".hash(hasher);
            directive.kind.as_label().hash(hasher);
        }
        _ => block.raw_source().hash(hasher),
    }
}

/// Shifts all global character ranges inside one rendered node by the provided document-level offset.
/// 按给定的文档级偏移平移单个渲染节点内部的所有全局字符范围。
fn shift_rendered_node_char_ranges(node: &mut RenderedTranscriptNode, char_offset: usize) {
    match node {
        RenderedTranscriptNode::Panel(_) => {}
        RenderedTranscriptNode::Collapsible(collapsible_node) => {
            shift_paragraph_char_range(&mut collapsible_node.summary, char_offset);
            if let Some(body) = collapsible_node.body.as_mut() {
                shift_paragraph_char_range(body, char_offset);
            }
        }
        RenderedTranscriptNode::Paragraph(paragraph_node) => {
            shift_paragraph_char_range(paragraph_node, char_offset);
        }
        RenderedTranscriptNode::MediaCard(_) => {}
        RenderedTranscriptNode::Table(table_node) => {
            for cell in &mut table_node.cells {
                shift_paragraph_char_range(&mut cell.paragraph, char_offset);
            }
        }
        RenderedTranscriptNode::Rule(_) => {}
    }
}

/// Shifts one rendered paragraph's document-global character range by the provided offset.
/// 按给定偏移平移单个渲染段落的文档全局字符范围。
fn shift_paragraph_char_range(
    paragraph_node: &mut super::scene::RenderedParagraphNode,
    char_offset: usize,
) {
    paragraph_node.global_char_range = (paragraph_node.global_char_range.start + char_offset)
        ..(paragraph_node.global_char_range.end + char_offset);
}

/// Returns whether one rendered block should receive the transient workspace-history search-hit panel treatment.
/// 返回某个渲染区块是否应获得工作台历史搜索命中的临时面板高亮处理。
fn block_has_workspace_history_search_hit_marker(block: &AgentViewBlock) -> bool {
    block
        .attributes()
        .get("ui-search-hit")
        .is_some_and(|value| value == "true")
}

/// Builds the subtle background panel used to emphasize one freshly navigated workspace-history search hit.
/// 构建用于强调一条刚刚定位到的工作台历史搜索命中的轻量背景面板。
fn workspace_history_search_hit_background(app_theme: &Theme) -> RenderedParagraphBackground {
    let palette = app_theme.palette();

    RenderedParagraphBackground {
        background: Background::Color(palette.primary.weak.color.scale_alpha(0.12)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: palette.primary.strong.color.scale_alpha(0.30),
        },
    }
}

/// Hashes every block attribute into the layout signature so runtime-only render decorations invalidate cached geometry correctly.
/// 将全部区块属性写入布局签名，确保仅运行时可见的渲染装饰能够正确使缓存几何失效。
fn hash_block_attributes(block: &AgentViewBlock, hasher: &mut DefaultHasher) {
    for (key, value) in block.attributes() {
        key.hash(hasher);
        value.hash(hasher);
    }
}

/// Hashes one RGBA color into the provided transcript-layout hash state using raw float bit patterns.
/// 使用原始浮点位模式将一组 RGBA 颜色写入给定的转录布局哈希状态。
fn hash_render_color(color: Color, hasher: &mut DefaultHasher) {
    color.r.to_bits().hash(hasher);
    color.g.to_bits().hash(hasher);
    color.b.to_bits().hash(hasher);
    color.a.to_bits().hash(hasher);
}

#[cfg(test)]
mod tests {
    //! Focused rendered-transcript document-model tests.
    //! 聚焦渲染转录文档模型的定向测试。

    use super::RenderedTranscriptDocument;
    use crate::{AgentViewDocument, AgentViewMarkdownBlock};
    use iced::Theme;

    /// Verifies that one Markdown block marked as a workspace-history search hit receives one surrounding highlight panel.
    /// 验证当某个 Markdown 区块被标记为工作台历史搜索命中时，会获得一个包裹其内容的高亮面板。
    #[test]
    fn workspace_history_search_hit_block_gets_highlight_panel() {
        let mut document = AgentViewDocument::new();
        let mut markdown_block = AgentViewMarkdownBlock::new("命中正文");
        markdown_block
            .attributes
            .insert(String::from("ui-search-hit"), String::from("true"));
        document
            .blocks
            .push(crate::AgentViewBlock::Markdown(markdown_block));

        let rendered_document =
            RenderedTranscriptDocument::from_agent_document(&document, &Theme::TokyoNight);

        assert!(matches!(
            rendered_document.nodes.first(),
            Some(super::super::scene::RenderedTranscriptNode::Panel(_))
        ));
    }
}
