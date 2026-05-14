//! Rendered transcript document construction and layout-signature helpers for the selectable transcript surface.
//! 可选择转录表面的渲染文档构建与布局签名辅助逻辑。

use super::builder::RenderedTranscriptBuilder;
use super::directive_render::render_directive_block;
use super::markdown_render::{render_markdown_items, render_markdown_source_with_table_fallback};
use super::markdown_source::{
    normalize_markdown_source_for_transcript_display, prepare_markdown_source_for_rendering,
    raw_source_contains_markdown_table,
};
use super::scene::{
    PlainParagraphStyle, RenderContext, RenderedParagraphBackground, RenderedTranscriptDocument,
};
use crate::iced_theme as theme;
use crate::{AgentViewBlock, AgentViewDocument};
use iced::{Background, Border, Color, Theme};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

impl RenderedTranscriptDocument {
    /// Builds one rendered transcript model from the structured agent document.
    /// 根据结构化代理文档构建一份渲染转录模型。
    pub(super) fn from_agent_document(document: &AgentViewDocument, app_theme: &Theme) -> Self {
        let palette = app_theme.palette();
        let markdown_settings = theme::markdown_settings(app_theme);
        let layout_signature = compute_rendered_transcript_layout_signature(document, app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            palette.primary.weak.color.scale_alpha(0.28),
            palette.background.base.text.scale_alpha(0.94),
        );

        for block in &document.blocks {
            let child_start_index = builder.node_count();
            builder.begin_block();

            match block {
                AgentViewBlock::Markdown(markdown_block) => {
                    let prepared_source =
                        prepare_markdown_source_for_rendering(&markdown_block.raw_source);
                    if raw_source_contains_markdown_table(&prepared_source) {
                        render_markdown_source_with_table_fallback(
                            &mut builder,
                            &prepared_source,
                            PlainParagraphStyle::flow(markdown_settings.text_size),
                            markdown_settings,
                            app_theme,
                        );
                    } else {
                        let normalized_source =
                            normalize_markdown_source_for_transcript_display(&prepared_source);
                        let markdown_content =
                            iced::widget::markdown::Content::parse(&normalized_source);
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
                    render_directive_block(
                        &mut builder,
                        directive_block,
                        markdown_settings,
                        app_theme,
                    );
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
        }

        Self {
            nodes: builder.nodes,
            full_text: builder.full_text,
            selection_color: builder.selection_color,
            link_actions: builder.link_actions,
            layout_signature,
        }
    }
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
        block.raw_source().hash(&mut hasher);
        hash_block_attributes(block, &mut hasher);
    }

    hasher.finish()
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
