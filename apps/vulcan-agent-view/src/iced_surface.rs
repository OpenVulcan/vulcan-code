//! Higher-level iced surfaces built on top of the agent-view transcript renderer.
//! 构建在 agent-view 转录渲染器之上的高阶 iced 展示表面。

use crate::iced::{
    SelectableTranscriptDragAutoScroll, SelectableTranscriptInteraction,
    view_selectable_rendered_transcript,
};
use crate::iced_style::{thinking_modal_backdrop_style, thinking_modal_card_style};
use crate::{AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument};
use iced::widget::Id;
use iced::widget::button;
use iced::widget::scrollable;
use iced::widget::{column, container, mouse_area, opaque, row, rule, space, stack, text};
use iced::{Center, Element, Fill, FillPortion, Theme};
use std::time::{Duration, Instant};

/// Shared duration used by the centered thinking modal open-and-close animation.
/// 居中思考弹层打开与关闭动画使用的共享时长。
pub const THINKING_MODAL_ANIMATION_DURATION: Duration = Duration::from_millis(220);

/// Shared tick interval used while the centered thinking modal animation is active.
/// 居中思考弹层动画激活期间使用的共享刷新间隔。
pub const THINKING_MODAL_TICK_INTERVAL: Duration = Duration::from_millis(16);

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

/// Builds the centered thinking modal surface on top of one prepared thinking document.
/// 基于一份已准备好的思考文档构建居中思考弹层表面。
pub fn view_thinking_modal_surface<'a, Message: Clone + 'a>(
    title: String,
    helper_text: String,
    document: &AgentViewDocument,
    app_theme: &Theme,
    progress: f32,
    scrollable_id: Id,
    on_drag_auto_scroll: impl Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message + 'a,
    on_interaction: impl Fn(SelectableTranscriptInteraction) -> Message + 'a,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
    on_close: Message,
) -> Element<'a, Message> {
    let vertical_inset = 22.0 + (1.0 - progress) * 22.0;
    let header_secondary = app_theme
        .extended_palette()
        .background
        .base
        .text
        .scale_alpha(0.58 + progress * 0.20);

    let modal_body = scrollable(
        container(view_selectable_rendered_transcript::<Message>(
            document,
            app_theme,
            on_drag_auto_scroll,
            on_interaction,
        ))
        .width(Fill),
    )
    .id(scrollable_id)
    .height(Fill)
    .on_scroll(on_scroll);

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
    let helper_color = app_theme
        .extended_palette()
        .background
        .base
        .text
        .scale_alpha(0.56);

    let overlay_body = scrollable(
        container(view_selectable_rendered_transcript::<Message>(
            document,
            app_theme,
            on_drag_auto_scroll,
            on_interaction,
        ))
        .width(Fill),
    )
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
pub fn thinking_overlay_scrollable_id(session_id: usize) -> Id {
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

    match phase {
        ThinkingModalPhase::Opening => raw_progress,
        ThinkingModalPhase::Open => 1.0,
        ThinkingModalPhase::Closing => 1.0 - raw_progress,
    }
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
