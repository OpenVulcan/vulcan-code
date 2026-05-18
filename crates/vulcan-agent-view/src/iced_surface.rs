//! Higher-level iced surfaces built on top of the agent-view transcript renderer.
//! 构建在 agent-view 转录渲染器之上的高阶 iced 展示表面。

use crate::iced::{
    SelectableTranscriptCommand, SelectableTranscriptDragAutoScroll,
    SelectableTranscriptInteraction, SelectableTranscriptSurfaceId,
    view_selectable_rendered_transcript, view_selectable_rendered_transcript_owned,
    view_selectable_rendered_transcript_owned_with_context_menu,
    view_selectable_rendered_transcript_with_context_menu,
};
use crate::iced_style::{thinking_modal_backdrop_style, thinking_modal_card_style};
use crate::{
    AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument,
    parse_agent_view_document,
};
use iced::widget::Id;
use iced::widget::button;
use iced::widget::lazy;
use iced::widget::scrollable;
use iced::widget::{column, container, mouse_area, opaque, row, rule, space, stack, text};
use iced::{Center, Element, Fill, FillPortion, Theme};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Shared duration used by the centered thinking modal open-and-close animation.
/// 居中思考弹层打开与关闭动画使用的共享时长。
pub const THINKING_MODAL_ANIMATION_DURATION: Duration = Duration::from_millis(320);

/// Shared tick interval used while the centered thinking modal animation is active.
/// 居中思考弹层动画激活期间使用的共享刷新间隔。
pub const THINKING_MODAL_TICK_INTERVAL: Duration = Duration::from_millis(10);

/// Shared width used by the floating streaming-thinking overlay.
/// 流式思考浮动面板使用的共享宽度。
const THINKING_OVERLAY_WIDTH: f32 = 440.0;

/// Shared height used by the floating streaming-thinking overlay.
/// 流式思考浮动面板使用的共享高度。
const THINKING_OVERLAY_HEIGHT: f32 = 280.0;

/// Lifecycle phase used by the centered thinking modal animation.
/// 居中思考弹层动画使用的生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingModalPhase {
    /// The modal is animating into view.
    /// 弹层正在执行入场动画。
    Opening,
    /// The modal is fully visible and interactive.
    /// 弹层已经完全显示并可交互。
    Open,
    /// The modal is animating out and should be removed when the configured duration elapses.
    /// 弹层正在执行退场动画，并应在配置时长结束后移除。
    Closing,
}

/// Stable dependency key used by the lazy modal body so expensive transcript widgets only rebuild when content or theme actually changes.
/// 供惰性弹层正文使用的稳定依赖键，使昂贵的转录组件仅在内容或主题真正变化时重建。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ModalBodyLazyDependency {
    /// Stable content signature computed from the prepared modal document snapshot.
    /// 基于已准备弹层文档快照计算得到的稳定内容签名。
    document_signature: u64,
    /// Stable theme signature used to invalidate the cached body when the visible palette changes.
    /// 当可见调色板变化时用于使缓存正文失效的稳定主题签名。
    theme_signature: u64,
    /// Stable command signature used to refresh cached bodies when host menu commands target this surface.
    /// 当宿主菜单命令命中该表面时用于刷新缓存正文的稳定命令签名。
    command_signature: u64,
}

/// Builds the centered thinking modal surface on top of one prepared thinking document.
/// 基于一份已准备好的思考文档构建居中思考弹层表面。
pub fn view_thinking_modal_surface<'a, Message: Clone + 'a + 'static>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    document_signature: u64,
    app_theme: &Theme,
    progress: f32,
    scrollable_id: Id,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a + 'static,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a + 'static,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a + 'static,
    on_close: Message,
) -> Element<'a, Message> {
    view_thinking_modal_surface_core(
        title,
        helper_text,
        document,
        document_signature,
        app_theme,
        progress,
        scrollable_id,
        None,
        None,
        on_drag_auto_scroll,
        on_interaction,
        on_scroll,
        on_close,
    )
}

/// Builds the centered thinking modal surface while routing transcript context menus through the host workbench.
/// 构建居中思考弹层表面，并将转录右键菜单路由到宿主工作台。
pub fn view_thinking_modal_surface_with_context_menu<'a, Message: Clone + 'a + 'static>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    document_signature: u64,
    app_theme: &Theme,
    progress: f32,
    scrollable_id: Id,
    surface_id: SelectableTranscriptSurfaceId,
    command: Option<SelectableTranscriptCommand>,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a + 'static,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a + 'static,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a + 'static,
    on_close: Message,
) -> Element<'a, Message> {
    view_thinking_modal_surface_core(
        title,
        helper_text,
        document,
        document_signature,
        app_theme,
        progress,
        scrollable_id,
        Some(surface_id),
        command,
        on_drag_auto_scroll,
        on_interaction,
        on_scroll,
        on_close,
    )
}

/// Builds the shared centered modal chrome and optionally enables host-routed transcript context menus.
/// 构建共享的居中弹层外框，并可选启用宿主路由的转录右键菜单。
fn view_thinking_modal_surface_core<'a, Message: Clone + 'a + 'static>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    document_signature: u64,
    app_theme: &Theme,
    progress: f32,
    scrollable_id: Id,
    context_menu_surface_id: Option<SelectableTranscriptSurfaceId>,
    command: Option<SelectableTranscriptCommand>,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a + 'static,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a + 'static,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a + 'static,
    on_close: Message,
) -> Element<'a, Message> {
    let reveal_progress = progress.clamp(0.0, 1.0);
    let vertical_inset = 22.0 + (1.0 - reveal_progress) * 6.0;
    let header_secondary = app_theme
        .palette()
        .background
        .base
        .text
        .scale_alpha(0.54 + reveal_progress * 0.24);

    let transcript_document = document.clone();
    let transcript_theme = app_theme.clone();
    let transcript_scrollable_id = scrollable_id.clone();
    let on_drag_auto_scroll = Rc::new(on_drag_auto_scroll);
    let on_interaction = Rc::new(on_interaction);
    let on_scroll = Rc::new(on_scroll);
    let modal_body_dependency = ModalBodyLazyDependency {
        document_signature,
        theme_signature: hash_modal_theme(app_theme),
        command_signature: selectable_transcript_command_signature(
            context_menu_surface_id.as_ref(),
            command.as_ref(),
        ),
    };
    let modal_body: Element<'a, Message> = lazy(modal_body_dependency, move |_| {
        let drag_callback = Rc::clone(&on_drag_auto_scroll);
        let interaction_callback = Rc::clone(&on_interaction);
        let scroll_callback = Rc::clone(&on_scroll);
        let transcript_surface_id = context_menu_surface_id.clone();
        let transcript_command = command.clone();

        let transcript_body = match transcript_surface_id {
            Some(surface_id) => {
                view_selectable_rendered_transcript_owned_with_context_menu::<Message>(
                    transcript_document.clone(),
                    transcript_theme.clone(),
                    surface_id,
                    transcript_command,
                    move |request| drag_callback(request),
                    move |interaction| interaction_callback(interaction),
                )
            }
            None => view_selectable_rendered_transcript_owned::<Message>(
                transcript_document.clone(),
                transcript_theme.clone(),
                move |request| drag_callback(request),
                move |interaction| interaction_callback(interaction),
            ),
        };

        scrollable(container(transcript_body).width(Fill))
            .id(transcript_scrollable_id.clone())
            .height(Fill)
            .on_scroll(move |viewport| scroll_callback(viewport))
    })
    .into();

    let modal_panel = container(
        column![
            row![
                column![
                    text(title).size(18),
                    text(helper_text).size(12).style(move |_theme: &Theme| {
                        iced::widget::text::Style {
                            color: Some(header_secondary),
                        }
                    }),
                ]
                .spacing(4),
                space::horizontal(),
                button(text("✕").size(14))
                    .padding([4, 8])
                    .style(button::text)
                    .on_press(on_close.clone()),
            ]
            .align_y(Center),
            rule::horizontal(1),
            container(modal_body).padding([4, 0]).height(Fill),
        ]
        .spacing(12),
    )
    .padding([16, 18])
    .width(Fill)
    .height(Fill)
    .style(move |theme| thinking_modal_card_style(theme, progress));

    let backdrop = mouse_area(
        container(space())
            .width(Fill)
            .height(Fill)
            .style(move |theme| thinking_modal_backdrop_style(theme, progress)),
    )
    .on_press(on_close);

    let centered_modal = column![
        space::vertical().height(FillPortion(15)),
        row![
            space::horizontal().width(FillPortion(15)),
            opaque(
                container(modal_panel)
                    .width(FillPortion(70))
                    .height(FillPortion(70))
            ),
            space::horizontal().width(FillPortion(15)),
        ]
        .width(Fill)
        .height(FillPortion(70)),
        space::vertical().height(FillPortion(15)),
    ]
    .width(Fill)
    .height(Fill)
    .padding([vertical_inset, 20.0]);

    opaque(
        stack([
            Element::from(backdrop),
            Element::from(container(centered_modal).width(Fill).height(Fill)),
        ])
        .width(Fill)
        .height(Fill),
    )
    .into()
}

/// Builds the floating streaming-thinking overlay surface on top of one prepared thinking document.
/// 基于一份已准备好的思考文档构建流式思考浮动面板表面。
pub fn view_thinking_overlay_surface<'a, Message: 'a>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    app_theme: &Theme,
    scrollable_id: Id,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
) -> Element<'a, Message> {
    view_thinking_overlay_surface_core(
        title,
        helper_text,
        document,
        app_theme,
        scrollable_id,
        None,
        None,
        on_drag_auto_scroll,
        on_interaction,
        on_scroll,
    )
}

/// Builds the floating streaming-thinking overlay while routing transcript context menus through the host workbench.
/// 构建流式思考浮动面板，并将转录右键菜单路由到宿主工作台。
pub fn view_thinking_overlay_surface_with_context_menu<'a, Message: 'a>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    app_theme: &Theme,
    scrollable_id: Id,
    surface_id: SelectableTranscriptSurfaceId,
    command: Option<SelectableTranscriptCommand>,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
) -> Element<'a, Message> {
    view_thinking_overlay_surface_core(
        title,
        helper_text,
        document,
        app_theme,
        scrollable_id,
        Some(surface_id),
        command,
        on_drag_auto_scroll,
        on_interaction,
        on_scroll,
    )
}

/// Builds the shared floating overlay chrome and optionally enables host-routed transcript context menus.
/// 构建共享的浮动面板外框，并可选启用宿主路由的转录右键菜单。
fn view_thinking_overlay_surface_core<'a, Message: 'a>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    app_theme: &Theme,
    scrollable_id: Id,
    context_menu_surface_id: Option<SelectableTranscriptSurfaceId>,
    command: Option<SelectableTranscriptCommand>,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
) -> Element<'a, Message> {
    let helper_color = app_theme.palette().background.base.text.scale_alpha(0.56);
    let transcript_body = match context_menu_surface_id {
        Some(surface_id) => view_selectable_rendered_transcript_with_context_menu::<Message>(
            document,
            app_theme,
            surface_id,
            command,
            on_drag_auto_scroll,
            on_interaction,
        ),
        None => view_selectable_rendered_transcript::<Message>(
            document,
            app_theme,
            on_drag_auto_scroll,
            on_interaction,
        ),
    };

    let overlay_body = scrollable(container(transcript_body).width(Fill))
        .id(scrollable_id)
        .height(Fill)
        .on_scroll(on_scroll);

    let overlay_panel = container(
        column![
            column![
                text(title).size(15),
                text(helper_text)
                    .size(11)
                    .style(move |_theme: &Theme| iced::widget::text::Style {
                        color: Some(helper_color),
                    }),
            ]
            .spacing(4),
            rule::horizontal(1),
            container(overlay_body).padding([2, 0]).height(Fill),
        ]
        .spacing(10),
    )
    .padding([12, 14])
    .width(THINKING_OVERLAY_WIDTH)
    .height(THINKING_OVERLAY_HEIGHT)
    .style(move |theme| thinking_modal_card_style(theme, 1.0));

    container(
        column![
            space::vertical(),
            row![space::horizontal(), container(overlay_panel),].align_y(Center),
        ]
        .width(Fill)
        .height(Fill)
        .padding([18, 18]),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

/// Returns the revision signature of one host command when it targets the provided transcript surface.
/// 当宿主命令命中给定转录表面时返回其修订签名。
fn selectable_transcript_command_signature(
    surface_id: Option<&SelectableTranscriptSurfaceId>,
    command: Option<&SelectableTranscriptCommand>,
) -> u64 {
    match (surface_id, command) {
        (
            Some(surface_id),
            Some(SelectableTranscriptCommand::SelectAll {
                surface_id: target_surface_id,
                revision,
            }),
        ) if target_surface_id == surface_id => *revision,
        _ => 0,
    }
}

/// Builds a single-block modal thinking document by selecting one decorated thinking directive from the provided source document.
/// 通过从给定源文档中选取一个已装饰的思考指令块，构建单区块弹层思考文档。
pub fn build_modal_thinking_surface_document(
    document: AgentViewDocument,
    block_id: &str,
) -> Option<(String, AgentViewDocument)> {
    for block in document.blocks {
        let AgentViewBlock::Directive(directive) = block else {
            continue;
        };
        if directive.kind != AgentViewDirectiveKind::Thinking {
            continue;
        }
        let current_block_id = directive
            .attributes
            .get("ui-block-id")
            .map(String::as_str)
            .unwrap_or_default();
        if current_block_id != block_id {
            continue;
        }

        return Some(finalize_thinking_surface_document(
            directive,
            "modal",
            String::from("思考过程"),
        ));
    }

    None
}

/// Builds a single-block overlay thinking document from the first decorated thinking directive of the provided source document.
/// 从给定源文档的首个已装饰思考指令块构建单区块浮动思考文档。
pub fn build_overlay_thinking_surface_document(
    document: AgentViewDocument,
) -> Option<(String, AgentViewDocument)> {
    for block in document.blocks {
        let AgentViewBlock::Directive(directive) = block else {
            continue;
        };
        if directive.kind != AgentViewDirectiveKind::Thinking {
            continue;
        }

        return Some(finalize_thinking_surface_document(
            directive,
            "overlay",
            String::from("思考过程"),
        ));
    }

    None
}

/// Builds one single-block tool surface document by selecting one decorated tool directive from the provided source document.
/// 通过从给定源文档中选取一个已装饰工具指令块，构建单区块工具表面文档。
pub fn build_modal_tool_surface_document(
    document: AgentViewDocument,
    block_id: &str,
) -> Option<(String, AgentViewDocument)> {
    for block in document.blocks {
        let AgentViewBlock::Directive(directive) = block else {
            continue;
        };
        if !matches!(
            directive.kind,
            AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
        ) {
            continue;
        }
        let current_block_id = directive
            .attributes
            .get("ui-block-id")
            .map(String::as_str)
            .unwrap_or_default();
        if current_block_id != block_id {
            continue;
        }

        let title = directive
            .attributes
            .get("title")
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("工具调用详情"));
        let surface_document = parse_agent_view_document(&directive.body);
        return Some((title, surface_document));
    }

    None
}

/// Estimates one readable token count for a thinking body so titles can surface a stable scale hint.
/// 为思考正文估算一个可读 token 数量，以便标题展示稳定的规模提示。
pub fn estimate_thinking_token_count(body: &str) -> usize {
    body.chars()
        .filter(|character| !character.is_whitespace())
        .count()
}

/// Builds the inline thinking title from one body payload and one preformatted duration label.
/// 根据思考正文与一条预格式化时长标签构建内联思考标题。
pub fn build_thinking_title(body: &str, duration_label: &str) -> String {
    let token_count = estimate_thinking_token_count(body);
    format!("思考过程（约 {token_count} tokens · {duration_label}）")
}

/// Formats one thinking duration in milliseconds into a compact Chinese label.
/// 将一个以毫秒表示的思考时长格式化为紧凑的中文标签。
pub fn format_thinking_duration_label(duration_millis: u128) -> String {
    if duration_millis < 1_000 {
        return String::from("不足 1 秒");
    }

    let total_seconds = duration_millis as f64 / 1_000.0;
    if total_seconds < 60.0 {
        return format!("{total_seconds:.1} 秒");
    }

    let total_minutes = total_seconds / 60.0;
    if total_minutes < 60.0 {
        return format!("{total_minutes:.1} 分钟");
    }

    let total_hours = total_minutes / 60.0;
    format!("{total_hours:.1} 小时")
}

/// Builds the stable scrollable identifier used by one floating streaming-thinking overlay.
/// 为单个流式思考浮层构建稳定滚动容器标识。
pub fn thinking_overlay_scrollable_id(session_id: impl std::fmt::Display) -> Id {
    Id::from(format!("session-{session_id}-thinking-overlay-scrollable"))
}

/// Returns the normalized open-or-close progress of the centered thinking modal animation snapshot.
/// 返回居中思考弹层动画快照的归一化开合进度。
pub fn thinking_modal_progress(started_at: Instant, phase: ThinkingModalPhase) -> f32 {
    let elapsed = started_at.elapsed();
    let duration = THINKING_MODAL_ANIMATION_DURATION.as_secs_f32();
    let raw_progress = if duration <= f32::EPSILON {
        1.0
    } else {
        (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0)
    };

    let eased_progress = smooth_modal_animation_progress(raw_progress);

    match phase {
        ThinkingModalPhase::Opening => eased_progress,
        ThinkingModalPhase::Open => 1.0,
        ThinkingModalPhase::Closing => 1.0 - eased_progress,
    }
}

/// Converts linear modal timeline progress into a softer eased curve without the abrupt first-frame jump.
/// 将线性弹层时间轴进度转换为更柔和的缓动曲线，避免首帧突兀跳变。
fn smooth_modal_animation_progress(progress: f32) -> f32 {
    let clamped_progress = progress.clamp(0.0, 1.0);
    clamped_progress * clamped_progress * (3.0 - 2.0 * clamped_progress)
}

/// Hashes the visible modal theme treatment into one stable signature so lazy modal bodies rebuild when palette changes propagate.
/// 将可见弹层主题外观哈希为稳定签名，使惰性弹层正文能在调色板变化传播时重建。
fn hash_modal_theme(theme: &Theme) -> u64 {
    let palette = theme.palette();
    let mut hasher = DefaultHasher::new();

    hash_color(palette.background.base.color, &mut hasher);
    hash_color(palette.background.base.text, &mut hasher);
    hash_color(palette.primary.strong.color, &mut hasher);
    hash_color(palette.secondary.strong.color, &mut hasher);

    hasher.finish()
}

/// Pushes one RGBA color into the provided hash state using raw float bit patterns to preserve visual identity changes.
/// 使用原始浮点位模式将一组 RGBA 颜色分量写入给定哈希状态，以保留可见外观变化。
fn hash_color(color: iced::Color, hasher: &mut DefaultHasher) {
    color.r.to_bits().hash(hasher);
    color.g.to_bits().hash(hasher);
    color.b.to_bits().hash(hasher);
    color.a.to_bits().hash(hasher);
}

/// Finalizes one thinking directive into a standalone single-block surface document with the requested surface mode.
/// 将一个思考指令整理为带指定展示模式的独立单区块表面文档。
fn finalize_thinking_surface_document(
    mut directive: AgentViewDirectiveBlock,
    surface_mode: &str,
    fallback_title: String,
) -> (String, AgentViewDocument) {
    let title = directive
        .attributes
        .get("ui-thinking-title")
        .cloned()
        .unwrap_or(fallback_title);
    directive
        .attributes
        .insert(String::from("ui-inline-collapsed"), String::from("false"));
    directive.attributes.insert(
        String::from("ui-render-thinking-header"),
        String::from("false"),
    );
    directive.attributes.insert(
        String::from("ui-thinking-surface"),
        String::from(surface_mode),
    );

    let mut surface_document = AgentViewDocument::new();
    surface_document.push_directive(directive);
    (title, surface_document)
}
