//! Selectable rendered transcript widget used by the primary transcript surface.
//! 主转录渲染表面使用的可选择渲染转录组件。

mod builder;
mod context_menu;
mod directive_render;
mod document_model;
mod event_routing;
mod interaction;
mod layout_engine;
mod markdown_render;
mod markdown_source;
mod paint;
mod render_cache;
mod scene;
mod surface_state;
mod text_routing;
mod text_utils;
mod viewport_materialization;
mod viewport_scene;

use self::context_menu::*;
use self::event_routing::*;
use self::layout_engine::*;
use self::paint::*;
use self::scene::*;
use self::surface_state::*;
pub use self::surface_state::{
    SelectableTranscriptDragAutoScroll, SelectableTranscriptDragAutoScrollDirection,
    SelectableTranscriptInteraction,
};
use self::text_utils::*;
use self::viewport_scene::*;
#[cfg(test)]
use self::{
    builder::RenderedTranscriptBuilder, directive_render::*, interaction::*, markdown_render::*,
    markdown_source::*,
};
use crate::AgentViewDocument;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::{self};
use iced::advanced::widget::tree;
use iced::advanced::{Shell, Widget, overlay};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::marker::PhantomData;

/// Builds one selectable rendered transcript element from the provided structured agent document.
/// 根据提供的结构化代理文档构建一个可选择的渲染转录元素。
pub fn view_selectable_rendered_transcript<'a, Message: 'a>(
    document: &AgentViewDocument,
    app_theme: &Theme,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
) -> Element<'a, Message> {
    let rendered_document = RenderedTranscriptDocument::from_agent_document(document, app_theme);
    Element::new(SelectableTranscript::new(
        rendered_document,
        on_drag_auto_scroll,
        on_interaction,
    ))
}

/// Builds one selectable rendered transcript element from owned structured agent-document inputs so lazy wrappers can cache the heavy widget tree without borrowing external state.
/// 根据拥有所有权的结构化代理文档输入构建一个可选择的渲染转录元素，使惰性包装器能够缓存重型组件树而不借用外部状态。
pub fn view_selectable_rendered_transcript_owned<'a, Message: 'a>(
    document: AgentViewDocument,
    app_theme: Theme,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
) -> Element<'a, Message> {
    let rendered_document = RenderedTranscriptDocument::from_agent_document(&document, &app_theme);
    Element::new(SelectableTranscript::new(
        rendered_document,
        on_drag_auto_scroll,
        on_interaction,
    ))
}

/// Custom widget keeping the rendered transcript selectable without falling back to a text editor surface.
/// 在不退回文本编辑器表面的情况下保持渲染转录可选择的自定义组件。
struct SelectableTranscript<'a, Message> {
    /// Fully materialized rendered transcript model used for layout, hit-testing, and copy.
    /// 用于布局、命中测试与复制的完整已物化渲染转录模型。
    document: RenderedTranscriptDocument,
    /// Callback used to bubble drag auto-scroll intent back to the parent scroll container.
    /// 用于将拖拽自动滚动意图回传给父级滚动容器的回调。
    on_drag_auto_scroll: Box<dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a>,
    /// Callback used to bubble high-level transcript interactions back to the application shell.
    /// 用于将高层转录交互回传给应用壳层的回调。
    on_interaction: Box<dyn Fn(SelectableTranscriptInteraction) -> Message + 'a>,
    /// Marker preserving the message type even though the widget is internally message-free.
    /// 即使组件内部不主动发消息，也用于保留消息类型的标记。
    message_marker: PhantomData<Message>,
}

impl<'a, Message> SelectableTranscript<'a, Message> {
    /// Creates one selectable transcript widget from a rendered transcript model.
    /// 根据一个渲染转录模型创建可选择转录组件。
    fn new(
        document: RenderedTranscriptDocument,
        on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
        on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
    ) -> Self {
        Self {
            document,
            on_drag_auto_scroll: Box::new(on_drag_auto_scroll),
            on_interaction: Box::new(on_interaction),
            message_marker: PhantomData,
        }
    }
}

/// Concrete paragraph type used by the default iced renderer.
/// 默认 iced 渲染器使用的具体段落类型。
type RendererParagraph = <Renderer as text::Renderer>::Paragraph;

impl<'a, Message> Widget<Message, Theme, Renderer> for SelectableTranscript<'a, Message> {
    /// Returns the runtime state tag used by the selectable transcript widget.
    /// 返回可选择转录组件使用的运行时状态标签。
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SelectableTranscriptState>()
    }

    /// Returns the runtime state stored by the selectable transcript widget.
    /// 返回可选择转录组件存储的运行时状态。
    fn state(&self) -> tree::State {
        tree::State::new(SelectableTranscriptState::default())
    }

    /// Returns the widget size policy used by the selectable transcript widget.
    /// 返回可选择转录组件使用的尺寸策略。
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    /// Lays out the transcript widget by measuring every rendered paragraph node.
    /// 通过测量每个渲染段落节点来布局转录组件。
    fn layout(
        &mut self,
        tree: &mut tree::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();
        let max_size = limits.width(Length::Fill).height(Length::Shrink).max();
        let available_width = max_size.width.max(1.0);
        let local_widget_bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(available_width, max_size.height.max(1.0)),
        );

        if state
            .render_cache
            .matches_layout(self.document.layout_signature, available_width)
        {
            return layout::Node::new(Size::new(
                available_width,
                state.render_cache.cached_height.max(1.0),
            ));
        }
        let layout_height =
            rebuild_layout_cache(&self.document, state, local_widget_bounds, available_width);

        layout::Node::new(Size::new(available_width, layout_height))
    }

    /// Draws the transcript widget, its block backgrounds, quote bars, text, and active selection highlight.
    /// 绘制转录组件及其区块背景、引用导线、文本和当前活动选择高亮。
    fn draw(
        &self,
        tree: &tree::Tree,
        renderer: &mut Renderer,
        app_theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<SelectableTranscriptState>();
        let materialized_scene =
            materialize_transcript_scene(&self.document, state, layout.bounds(), cursor, *viewport);
        draw_visible_transcript_content(renderer, defaults, &materialized_scene);
        let _ = app_theme;
    }

    /// Updates transcript focus, selection dragging, and copy shortcuts based on runtime input events.
    /// 根据运行时输入事件更新转录焦点、选择拖拽和复制快捷键。
    fn update(
        &mut self,
        tree: &mut tree::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();
        handle_transcript_widget_event(
            &self.document,
            self.on_drag_auto_scroll.as_ref(),
            self.on_interaction.as_ref(),
            state,
            event,
            layout.bounds(),
            cursor,
            shell,
            *_viewport,
        );
    }

    /// Returns the mouse interaction shown while hovering the selectable transcript widget.
    /// 返回悬停在可选择转录组件上时展示的鼠标交互样式。
    fn mouse_interaction(
        &self,
        tree: &tree::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<SelectableTranscriptState>();
        transcript_mouse_interaction(&self.document, state, layout.bounds(), cursor)
    }

    /// Returns the transcript context-menu overlay when one right-click menu is currently open.
    /// 当存在一个已打开右键菜单时返回其转录上下文菜单浮层。
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut tree::Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<SelectableTranscriptState>();
        let menu_state = state.context_menu?;
        let translated_bounds = Rectangle::new(
            Point::new(
                layout.bounds().x + translation.x,
                layout.bounds().y + translation.y,
            ),
            layout.bounds().size(),
        );

        Some(build_context_menu_overlay(
            state,
            &self.document,
            translated_bounds,
            menu_state,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MARKDOWN_LIST_INDENT_STEP, PlainParagraphStyle, RenderContext,
        RenderedParagraphHorizontalPlacement, RenderedParagraphNode, RenderedTranscriptBuilder,
        RenderedTranscriptNode, SelectableTableAlignment,
        SelectableTranscriptDragAutoScrollDirection, build_markdown_image_spans,
        build_user_prompt_meta_spans, byte_offset_to_char_offset,
        clamp_hit_test_point_to_paragraph_text_bounds, clamp_point_to_rectangle,
        drag_auto_scroll_request_for_position, expand_compact_markdown_table_line,
        multiline_cursor_to_byte_offset, normalize_markdown_source_for_transcript_display,
        ordered_list_prefix_text, parse_markdown_table_alignment, parse_markdown_table_source,
        prepare_markdown_source_for_rendering, raw_source_contains_markdown_table,
        render_markdown_items, render_markdown_source_with_table_fallback, render_thinking_block,
        rendered_table_style, user_prompt_copy_action_link, user_prompt_created_at_label,
    };
    use crate::iced_theme as theme;
    use crate::{AgentViewAttributes, AgentViewDirectiveBlock, AgentViewDirectiveKind};
    use iced::advanced::text;
    use iced::widget::markdown;
    use iced::widget::text::Span;
    use iced::{Background, Color, Padding, Pixels, Point, Size, Theme};
    use std::ops::Range;

    /// Verifies ASCII byte offsets remain identical after conversion because one byte still means one visible character.
    /// 验证 ASCII 字节偏移在转换后保持一致，因为单字节仍然对应一个可见字符。
    #[test]
    fn byte_offsets_match_char_offsets_for_ascii_text() {
        let text = "hello world";

        assert_eq!(byte_offset_to_char_offset(text, 0), 0);
        assert_eq!(byte_offset_to_char_offset(text, 1), 1);
        assert_eq!(byte_offset_to_char_offset(text, 5), 5);
        assert_eq!(byte_offset_to_char_offset(text, text.len()), 11);
    }

    /// Verifies UTF-8 byte offsets from mixed Chinese and Latin text are normalized back to logical character positions.
    /// 验证混合中英文本的 UTF-8 字节偏移会被归一化回逻辑字符位置。
    #[test]
    fn byte_offsets_are_normalized_for_mixed_cjk_text() {
        let text = "A中B文C";

        assert_eq!(byte_offset_to_char_offset(text, 0), 0);
        assert_eq!(byte_offset_to_char_offset(text, 1), 1);
        assert_eq!(byte_offset_to_char_offset(text, 4), 2);
        assert_eq!(byte_offset_to_char_offset(text, 5), 3);
        assert_eq!(byte_offset_to_char_offset(text, 8), 4);
        assert_eq!(byte_offset_to_char_offset(text, text.len()), 5);
    }

    /// Verifies invalid in-character byte offsets are snapped back to the previous character boundary instead of drifting forward.
    /// 验证落在字符内部的非法字节偏移会回退到上一个字符边界，而不是继续向前漂移。
    #[test]
    fn invalid_utf8_byte_offsets_snap_to_previous_boundary() {
        let text = "中A";

        assert_eq!(byte_offset_to_char_offset(text, 1), 0);
        assert_eq!(byte_offset_to_char_offset(text, 2), 0);
        assert_eq!(byte_offset_to_char_offset(text, 3), 1);
        assert_eq!(byte_offset_to_char_offset(text, 4), 2);
    }

    /// Verifies plain transcript prose keeps single source newlines visible while fenced code keeps its original line semantics.
    /// 验证普通转录正文会保留单个源码换行的可见性，同时围栏代码保持原始换行语义。
    #[test]
    fn transcript_markdown_normalization_preserves_visible_plain_line_breaks() {
        let source = "第一行\n第二行\n\n```rust\nlet value = 1;\nlet next = 2;\n```";
        let normalized = normalize_markdown_source_for_transcript_display(source);

        assert!(normalized.contains("第一行  \n第二行"));
        assert!(normalized.contains("```rust\nlet value = 1;\nlet next = 2;\n```"));
    }

    /// Verifies collapsed inline thinking headers keep larger inner padding so the folded title no longer hugs the panel edges.
    /// 验证折叠态内联思考头部会保留更大的内边距，从而避免折叠标题紧贴面板边缘。
    #[test]
    fn collapsed_thinking_header_uses_relaxed_padding() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("ui-thinking-surface"), String::from("inline"));
        attributes.insert(String::from("ui-inline-collapsed"), String::from("true"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            "这里是思考内容",
            ":::thinking\n这里是思考内容\n:::",
            false,
        );

        render_thinking_block(&mut builder, &block, markdown_settings, &app_theme);

        let header_node = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .expect("collapsed thinking should still render one header paragraph");

        assert_eq!(header_node.inner_padding, Padding::from([14.0, 16.0]));
        assert_eq!(header_node.gap_after, 12.0);
    }

    /// Verifies expanded inline thinking keeps the same header inset while giving descendant markdown content a stable horizontal padding.
    /// 验证展开态内联思考会保持与折叠态一致的头部内缩，并为后续 Markdown 内容提供稳定的水平内边距。
    #[test]
    fn expanded_thinking_keeps_stable_header_and_body_padding() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("ui-thinking-surface"), String::from("inline"));
        attributes.insert(String::from("ui-inline-collapsed"), String::from("false"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            "**Draft the Response (Mental Refinement):**\n*(Structure according to Plan First)*\n### 标题",
            ":::thinking\n**Draft the Response (Mental Refinement):**\n*(Structure according to Plan First)*\n### 标题\n:::",
            false,
        );

        render_thinking_block(&mut builder, &block, markdown_settings, &app_theme);

        let mut paragraph_nodes = builder.nodes.iter().filter_map(|node| match node {
            RenderedTranscriptNode::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        });
        let header_node = paragraph_nodes
            .next()
            .expect("expanded thinking should render one header paragraph");
        let body_node = paragraph_nodes
            .next()
            .expect("expanded thinking should render at least one body paragraph");

        assert_eq!(header_node.inner_padding, Padding::from([14.0, 16.0]));
        assert_eq!(body_node.inner_padding, Padding::from([0.0, 16.0]));
        assert!(body_node.display_text.contains('\n'));
    }

    /// Verifies inline thinking tables inherit the body-side horizontal inset and retain a visible header tint.
    /// 验证内联思考表格会继承正文两侧水平留白，并保留可见的表头着色。
    #[test]
    fn inline_thinking_tables_keep_inset_and_header_tint() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("ui-thinking-surface"), String::from("inline"));
        attributes.insert(String::from("ui-inline-collapsed"), String::from("false"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            "| 列1 | 列2 |\n|:---|---:|\n| 数据A | 数据B |",
            ":::thinking\n| 列1 | 列2 |\n|:---|---:|\n| 数据A | 数据B |\n:::",
            false,
        );

        render_thinking_block(&mut builder, &block, markdown_settings, &app_theme);

        let table_node = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Table(table) => Some(table),
                _ => None,
            })
            .expect("inline thinking should render one table node");

        assert_eq!(table_node.outer_padding.left, 16.0);
        assert_eq!(table_node.outer_padding.right, 16.0);
        assert!(matches!(
            table_node.header_background,
            Background::Color(color) if color != Color::TRANSPARENT
        ));
        assert!(matches!(
            table_node.body_background,
            Background::Color(color) if color != Color::TRANSPARENT
        ));
    }

    /// Verifies background-suppressed standalone tables stay transparent when no semantic surface background is supplied.
    /// 验证未提供语义表面背景时，禁止局部背景的独立表格仍会保持透明。
    #[test]
    fn standalone_suppressed_tables_remain_transparent() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let style = rendered_table_style(
            PlainParagraphStyle {
                suppress_embedded_backgrounds: true,
                ..PlainParagraphStyle::flow(markdown_settings.text_size)
            },
            markdown_settings,
            &app_theme,
        );

        assert!(matches!(
            style.header_background,
            Background::Color(color) if color == Color::TRANSPARENT
        ));
        assert!(matches!(
            style.body_background,
            Background::Color(color) if color == Color::TRANSPARENT
        ));
    }

    /// Verifies modal thinking tables do not stack another horizontal inset on top of the popup container padding.
    /// 验证弹出态思考表格不会在弹窗容器已有留白之外再叠加额外水平内缩。
    #[test]
    fn modal_thinking_tables_do_not_add_extra_outer_padding() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let style = rendered_table_style(
            PlainParagraphStyle {
                suppress_embedded_backgrounds: true,
                inner_padding: Padding::from([0.0, 16.0]),
                ..PlainParagraphStyle::flow(markdown_settings.text_size)
            },
            markdown_settings,
            &app_theme,
        );

        assert_eq!(style.outer_padding, Padding::ZERO);
    }

    /// Verifies Markdown table source is parsed into a structured table payload instead of falling back to a placeholder sentence.
    /// 验证 Markdown 表格源码会被解析为结构化表格载荷，而不是退化成占位提示语。
    #[test]
    fn markdown_tables_parse_into_structured_payload() {
        let table_lines = [
            "| 列1 | 列2 | 列3 |",
            "|---|---|---|",
            "| 数据A | 数据B | 数据C |",
            "| 数据D | 数据E | 数据F |",
        ];

        let parsed = parse_markdown_table_source(&table_lines)
            .expect("table source should parse into one structured payload");

        assert_eq!(parsed.header_cells, vec!["列1", "列2", "列3"]);
        assert_eq!(parsed.body_rows.len(), 2);
        assert_eq!(parsed.body_rows[0], vec!["数据A", "数据B", "数据C"]);
        assert_eq!(parsed.body_rows[1], vec!["数据D", "数据E", "数据F"]);
    }

    /// Verifies one compacted prose-plus-table line is expanded back into paragraph text followed by real table lines.
    /// 验证一条被压缩的“正文加表格”行会被展开回正文和真实表格行。
    #[test]
    fn compacted_markdown_table_boundaries_are_repaired_before_rendering() {
        let source = "以下是示例表格。|列1|列2||---|---||数据A|数据B||数据C|数据D|";
        let repaired = prepare_markdown_source_for_rendering(source);

        assert_eq!(
            repaired,
            "以下是示例表格。\n|列1|列2|\n|---|---|\n|数据A|数据B|\n|数据C|数据D|"
        );
    }

    /// Verifies a prose-prefixed compact table with multiple glued body rows keeps expanding into a valid multi-line table.
    /// 验证带正文前缀且多行正文被粘连的紧凑表格仍会被展开为合法的多行表格。
    #[test]
    fn real_world_compacted_table_output_is_repaired() {
        let source = "以下是基于当前协调代理配置与调度上下文的示例表格。如需指定具体字段、数据源或业务场景，请补充说明，我将按需生成或调整。|代理名称 |核心职责 |当前状态 |调度触发条件 | 备注||:---|:---|:---|:---|:---|\n|`default-worker`| 常规任务处理、代码生成、基础工具调用|待命 (Standby)| 默认路由，无特殊技能或记忆需求时 |当前阶段仅注入协作上下文，不触发真实执行|| `lua-specialist` | Lua脚本解析、技能执行、动态逻辑编排 | 待命 (Standby)| 任务匹配`.lua` 文件或`registry.list_skills` 返回结果| 依赖后续阶段工具注册增强|\n| `memory-specialist`| 上下文记忆管理、历史状态检索、长期知识维护 |待命 (Standby)| 需跨会话上下文或显式调用记忆接口时 |当前仅支持上下文注入，未启用持久化存储|| `coordinator (本代理)` | 路由决策、上下文注入、执行链编排 |运行中 (Active)| 接收用户指令后按`Plan First`模式评估分发| 当前阶段仅做确定性调度规划，不自动拉起子代理 |> 💡 **提示**：当前处于 `Plan First`阶段";
        let repaired = prepare_markdown_source_for_rendering(source);

        assert!(
            repaired.contains(
                "我将按需生成或调整。\n|代理名称 |核心职责 |当前状态 |调度触发条件 | 备注|"
            )
        );
        assert!(repaired.contains("\n|:---|:---|:---|:---|:---|"));
        assert!(repaired.contains("\n|`default-worker`| 常规任务处理、代码生成、基础工具调用|待命 (Standby)| 默认路由，无特殊技能或记忆需求时 |当前阶段仅注入协作上下文，不触发真实执行|"));
        assert!(repaired.contains("\n| `lua-specialist` | Lua脚本解析、技能执行、动态逻辑编排 | 待命 (Standby)| 任务匹配`.lua` 文件或`registry.list_skills` 返回结果| 依赖后续阶段工具注册增强|"));
        assert!(repaired.contains("\n| `memory-specialist`| 上下文记忆管理、历史状态检索、长期知识维护 |待命 (Standby)| 需跨会话上下文或显式调用记忆接口时 |当前仅支持上下文注入，未启用持久化存储|"));
        assert!(repaired.contains("\n| `coordinator (本代理)` | 路由决策、上下文注入、执行链编排 |运行中 (Active)| 接收用户指令后按`Plan First`模式评估分发| 当前阶段仅做确定性调度规划，不自动拉起子代理 |"));
    }

    /// Verifies the compact-table expander can at least expand the first prose-plus-header line from the real sample.
    /// 验证紧凑表格展开器至少能够展开真实样例中的首条“正文加表头”行。
    #[test]
    fn first_real_compacted_table_line_expands() {
        let line = "以下是基于当前协调代理配置与调度上下文的示例表格。如需指定具体字段、数据源或业务场景，请补充说明，我将按需生成或调整。|代理名称 |核心职责 |当前状态 |调度触发条件 | 备注||:---|:---|:---|:---|:---|";
        let expanded = expand_compact_markdown_table_line(line);
        assert!(expanded.is_some());
    }

    /// Verifies Markdown content after one rendered table still preserves quote and inline-format semantics.
    /// 验证渲染表格后的 Markdown 内容仍会保留引用与行内格式语义。
    #[test]
    fn markdown_after_table_keeps_quote_and_inline_formatting() {
        let source =
            "| 列1 | 列2 |\n|---|---|\n| 数据A | 数据B |\n> 💡 **提示**：当前处于 `Plan First`阶段";
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_source_with_table_fallback(
            &mut builder,
            source,
            PlainParagraphStyle::flow(Pixels(16.0)),
            markdown_settings,
            &app_theme,
        );

        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Table(_))
        ));

        let quote_paragraph = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Paragraph(paragraph) if paragraph.quote_depth > 0 => {
                    Some(paragraph)
                }
                _ => None,
            })
            .expect("quote paragraph should remain rendered as markdown");

        assert!(quote_paragraph.display_text.contains("提示"));
        assert!(quote_paragraph.display_text.contains("Plan First"));
        assert!(!quote_paragraph.display_text.contains("**"));
        assert!(!quote_paragraph.display_text.contains('`'));
        assert!(!quote_paragraph.display_text.contains('>'));
    }

    /// Verifies fenced Markdown tables render as true Markdown content while remaining enclosed by the code-style background panel.
    /// 验证围栏 Markdown 表格会继续按真实 Markdown 内容渲染，同时仍被代码风格背板包裹。
    #[test]
    fn fenced_markdown_tables_stay_inside_code_blocks() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_source_with_table_fallback(
            &mut builder,
            "```markdown\n| 列1 | 列2 |\n|---|---|\n| 数据A | 数据B |\n```",
            PlainParagraphStyle::flow(Pixels(16.0)),
            markdown_settings,
            &app_theme,
        );

        assert!(
            builder
                .nodes
                .iter()
                .any(|node| matches!(node, RenderedTranscriptNode::Table(_)))
        );
        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Panel(_))
        ));

        assert!(!builder.nodes.iter().any(|node| matches!(
            node,
            RenderedTranscriptNode::Paragraph(paragraph)
                if paragraph.display_text.contains("```")
        )));

        let rendered_table = builder
            .nodes
            .iter()
            .find_map(|node| match node {
                RenderedTranscriptNode::Table(table) => Some(table),
                _ => None,
            })
            .expect("fenced markdown should render one real table node");

        assert_eq!(rendered_table.column_count, 2);
        assert_eq!(rendered_table.row_count, 2);
    }

    /// Verifies fenced-code tables neither trigger standalone table detection nor get rewritten by the compact-table repair pass.
    /// 验证围栏代码中的表格既不会触发独立表格检测，也不会被紧凑表格修复器改写。
    #[test]
    fn fenced_markdown_tables_are_ignored_by_table_detection_and_repair() {
        let source = "```markdown\n|列1|列2||---|---||数据A|数据B|\n```";

        assert!(!raw_source_contains_markdown_table(source));
        assert_eq!(prepare_markdown_source_for_rendering(source), source);
    }

    /// Verifies fenced Markdown blocks render their inner content as Markdown preview within one background panel.
    /// 验证围栏 Markdown 代码块会在同一个背板内把内部内容渲染成 Markdown 预览。
    #[test]
    fn fenced_markdown_blocks_render_tables_inside_their_panel() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let markdown_content = markdown::Content::parse(
            "```markdown\n| 列1 | 列2 |\n|---|---|\n| 数据A | 数据B |\n```",
        );
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_items(
            &mut builder,
            markdown_content.items(),
            RenderContext::default(),
            markdown_settings,
            &app_theme,
        );

        assert!(matches!(
            builder.nodes.first(),
            Some(RenderedTranscriptNode::Panel(_))
        ));
        assert!(
            builder
                .nodes
                .iter()
                .any(|node| matches!(node, RenderedTranscriptNode::Table(_)))
        );
        assert!(!builder.nodes.iter().any(|node| matches!(
            node,
            RenderedTranscriptNode::Paragraph(paragraph)
                if paragraph.display_text.contains("```")
        )));
    }

    /// Verifies Markdown footnote references and definitions are rewritten into readable transcript text outside fenced code blocks.
    /// 验证 Markdown 脚注引用和定义会在围栏代码块之外被改写成可读的转录文本。
    #[test]
    fn markdown_footnotes_are_rewritten_for_transcript_display() {
        let source =
            "这是一个脚注引用[^1]。\n\n[^1]: 这里是脚注说明\n```md\n[^2]: 保持代码块原样\n```";
        let prepared = prepare_markdown_source_for_rendering(source);

        assert!(prepared.contains("这是一个脚注引用[1]。"));
        assert!(prepared.contains("[1] 这里是脚注说明"));
        assert!(prepared.contains("```md\n[^2]: 保持代码块原样\n```"));
    }

    /// Verifies Markdown table alignment markers are parsed into left, center, and right transcript alignments.
    /// 验证 Markdown 表格对齐标记会被解析成左、中、右三种转录对齐方式。
    #[test]
    fn markdown_table_alignment_markers_are_parsed() {
        assert_eq!(
            parse_markdown_table_alignment(":---"),
            SelectableTableAlignment::Left
        );
        assert_eq!(
            parse_markdown_table_alignment(":---:"),
            SelectableTableAlignment::Center
        );
        assert_eq!(
            parse_markdown_table_alignment("---:"),
            SelectableTableAlignment::Right
        );
    }

    /// Verifies ordered-list prefixes reserve one stable marker width so one-digit and multi-digit items align to the same content gutter.
    /// 验证有序列表前缀会预留稳定的标记宽度，从而让单数位和多位数字条目对齐到同一正文槽位。
    #[test]
    fn ordered_list_prefixes_use_one_stable_gutter_width() {
        assert_eq!(ordered_list_prefix_text(1, 0, 4), " 1. ");
        assert_eq!(ordered_list_prefix_text(1, 3, 4), " 4. ");
        assert_eq!(ordered_list_prefix_text(8, 0, 4), " 8. ");
        assert_eq!(ordered_list_prefix_text(8, 2, 4), "10. ");
    }

    /// Verifies nested Markdown lists now advance by one shared indent step instead of mixing different magic offsets.
    /// 验证嵌套 Markdown 列表现在按统一缩进步进推进，而不是混用不同的魔法偏移。
    #[test]
    fn nested_markdown_lists_use_one_consistent_indent_step() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let markdown_content = markdown::Content::parse("1. 顶层\n   - 子项");
        let mut builder = RenderedTranscriptBuilder::new(
            Color::from_rgb8(0, 122, 255),
            Color::from_rgb8(24, 24, 24),
        );

        render_markdown_items(
            &mut builder,
            markdown_content.items(),
            RenderContext::default(),
            markdown_settings,
            &app_theme,
        );

        let paragraph_nodes = builder
            .nodes
            .iter()
            .filter_map(|node| match node {
                RenderedTranscriptNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(paragraph_nodes.len() >= 2);
        assert_eq!(paragraph_nodes[0].indent_left, MARKDOWN_LIST_INDENT_STEP);
        assert_eq!(
            paragraph_nodes[1].indent_left,
            MARKDOWN_LIST_INDENT_STEP * 2.0
        );
    }

    /// Verifies Markdown image fallback spans keep a clickable source URL on the rendered transcript surface.
    /// 验证 Markdown 图片回退片段会在渲染转录表面上保留可点击的源地址链接。
    #[test]
    fn markdown_image_fallback_keeps_clickable_url() {
        let app_theme = Theme::Dark;
        let markdown_settings = theme::markdown_settings(&app_theme);
        let markdown_content = markdown::Content::parse("![示意图](https://example.com/demo.png)");
        let image_item = markdown_content
            .items()
            .iter()
            .find_map(|item| match item {
                markdown::Item::Image { alt, url, .. } => Some((alt, url)),
                _ => None,
            })
            .expect("markdown image item should exist");

        let spans = build_markdown_image_spans(
            image_item.0,
            image_item.1,
            markdown_settings.style,
            &app_theme,
        );

        assert!(spans.iter().any(|span| {
            span.link
                .as_ref()
                .is_some_and(|link| link == "https://example.com/demo.png")
        }));
    }

    /// Verifies padded block hit-testing is clamped back into the real text area instead of drifting inside decorative box padding.
    /// 验证带内边距区块的命中点会被钳制回真实文本区域，而不会在装饰性盒子留白中漂移。
    #[test]
    fn padded_block_hit_test_points_are_clamped_to_text_bounds() {
        let paragraph_node = RenderedParagraphNode {
            display_text: String::from("[思考过程]"),
            spans: vec![Span::<markdown::Uri, iced::Font>::new("[思考过程]")],
            global_char_range: Range { start: 0, end: 6 },
            local_grapheme_ranges: vec![0..6],
            text_size: Pixels(16.0),
            line_height: text::LineHeight::default(),
            wrapping: text::Wrapping::Word,
            indent_left: 0.0,
            horizontal_placement: RenderedParagraphHorizontalPlacement::Leading,
            gap_before: 0.0,
            gap_after: 0.0,
            inner_padding: Padding::from([10.0, 12.0]),
            fit_width_to_content: false,
            max_width_ratio: None,
            background: None,
            background_action_link: None,
            quote_depth: 0,
        };

        let clamped = clamp_hit_test_point_to_paragraph_text_bounds(
            Point::new(250.0, 80.0),
            &paragraph_node,
            Size::new(220.0, 44.0),
            24.0,
        );

        assert_eq!(clamped.x, 195.5);
        assert_eq!(clamped.y, 23.5);
    }

    /// Verifies multi-line hit offsets are accumulated across preceding lines instead of staying trapped on the first line.
    /// 验证多行命中偏移会累加前序各行，而不是始终被困在第一行。
    #[test]
    fn multiline_cursor_offsets_accumulate_across_previous_lines() {
        let text = "第一行\n第二行\nthird";

        assert_eq!(multiline_cursor_to_byte_offset(text, 0, 0), 0);
        assert_eq!(
            multiline_cursor_to_byte_offset(text, 1, 0),
            "第一行\n".len()
        );
        assert_eq!(
            multiline_cursor_to_byte_offset(text, 1, "第二".len()),
            "第一行\n第二".len()
        );
        assert_eq!(
            multiline_cursor_to_byte_offset(text, 2, 2),
            "第一行\n第二行\nth".len()
        );
    }

    /// Verifies pointer positions that cross above or below the widget bounds emit the expected drag auto-scroll direction and overshoot distance.
    /// 验证越过组件上下边界的指针位置会发出正确的拖拽自动滚动方向与越界距离。
    #[test]
    fn drag_auto_scroll_request_tracks_vertical_overshoot() {
        let bounds = iced::Rectangle::new(Point::new(20.0, 40.0), Size::new(320.0, 180.0));

        let upward_request = drag_auto_scroll_request_for_position(bounds, Point::new(64.0, 28.0))
            .expect("upward overshoot should emit one request");
        assert_eq!(
            upward_request.direction,
            SelectableTranscriptDragAutoScrollDirection::Up
        );
        assert_eq!(upward_request.overshoot, 12.0);

        let downward_request =
            drag_auto_scroll_request_for_position(bounds, Point::new(96.0, 244.0))
                .expect("downward overshoot should emit one request");
        assert_eq!(
            downward_request.direction,
            SelectableTranscriptDragAutoScrollDirection::Down
        );
        assert_eq!(downward_request.overshoot, 24.0);

        assert!(drag_auto_scroll_request_for_position(bounds, Point::new(96.0, 160.0)).is_none());
    }

    /// Verifies out-of-bounds drag positions are clamped back onto the nearest visible edge so hit-testing can keep selecting while auto-scrolling.
    /// 验证越界拖拽位置会被钳制回最近的可见边缘，从而让命中测试在自动滚动期间继续工作。
    #[test]
    fn drag_positions_are_clamped_back_into_widget_bounds() {
        let bounds = iced::Rectangle::new(Point::new(12.0, 24.0), Size::new(180.0, 96.0));

        assert_eq!(
            clamp_point_to_rectangle(Point::new(-30.0, 8.0), bounds),
            Point::new(12.0, 24.0)
        );
        assert_eq!(
            clamp_point_to_rectangle(Point::new(240.0, 160.0), bounds),
            Point::new(192.0, 120.0)
        );
        assert_eq!(
            clamp_point_to_rectangle(Point::new(40.0, 60.0), bounds),
            Point::new(40.0, 60.0)
        );
    }

    /// Verifies user-prompt metadata lines expose a readable timestamp plus a synthetic copy action link tied to the prompt body.
    /// 验证用户提示词元信息行会暴露可读时间戳以及绑定到提示词正文的合成复制动作链接。
    #[test]
    fn user_prompt_meta_spans_include_timestamp_and_copy_action() {
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("message-id"), String::from("message-1"));
        attributes.insert(String::from("created-at"), String::from("1715251200000"));
        let block = AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::UserPrompt,
            attributes,
            "请输出一个表格",
            ":::user-prompt\n请输出一个表格\n:::",
            false,
        );

        let copy_action_link = user_prompt_copy_action_link(&block);
        let timestamp_label = user_prompt_created_at_label(&block);
        let spans = build_user_prompt_meta_spans(
            &block,
            copy_action_link.as_deref(),
            Color::from_rgb8(90, 90, 90),
            Color::from_rgb8(20, 120, 220),
            &Theme::Dark,
        );

        assert!(timestamp_label.is_some());
        assert!(
            timestamp_label
                .as_deref()
                .is_some_and(|value| value.contains('-'))
        );
        assert!(
            timestamp_label
                .as_deref()
                .is_some_and(|value| value.contains(':'))
        );
        assert_eq!(
            copy_action_link.as_deref(),
            Some("vulcan-agent-view://copy-user-prompt/message-1")
        );
        assert!(spans.iter().any(|span| span.text.contains('复')));
        assert!(spans.iter().any(|span| {
            span.link
                .as_ref()
                .is_some_and(|link| link == "vulcan-agent-view://copy-user-prompt/message-1")
        }));
    }
}
