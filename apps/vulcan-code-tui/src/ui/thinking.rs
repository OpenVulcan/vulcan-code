// The dynamic thinking box: a live window onto streaming reasoning.
// 动态思考框：实时显示流式思考内容的窗口。
//
// While reasoning streams, a bordered box follows the tail of the text with a shimmering title
// and a pulsing border. When the stream settles the box fades into the background over
// `FADE_MS`, after which only the collapsed "Thinking process" row remains in the transcript.
// `Float` pins it to the top-right of the transcript; `Docked` places it above the composer.
// 思考流式输出时，带边框的框体跟随文本尾部，标题带微光、边框呼吸闪烁。流结束后框体在 `FADE_MS`
// 内淡出到背景，此后会话记录中只保留折叠的“思考过程”行。`Float` 固定在会话记录右上角；
// `Docked` 位于输入框上方。

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use super::text::{shimmer, spinner, wrap};
use super::theme::mix;
use crate::app::{App, ThinkingPlacement};
use crate::model::{AssistantSemanticStatus, EventBody};
use crate::tr;

pub const FADE_MS: u64 = 900;

/// What the box shows this frame: text, whether it is still streaming, and fade progress (0..1).
/// 本帧框体内容：文本、是否仍在流式输出，以及淡出进度（0..1）。
pub struct ThinkingView {
    pub text: String,
    pub live: bool,
    pub fade: f32,
}

pub fn view(app: &App) -> Option<ThinkingView> {
    let session = app.session()?;
    if let Some(event) = session.live_reasoning()
        && let EventBody::AssistantReasoning { text, .. } = &event.body
    {
        return Some(ThinkingView {
            text: text.clone(),
            live: true,
            fade: 0.0,
        });
    }
    let (seq, at) = session.reasoning_settled?;
    let elapsed = app.now.saturating_duration_since(at);
    if elapsed >= Duration::from_millis(FADE_MS) {
        return None;
    }
    let event = session.events.iter().find(|e| e.seq == seq)?;
    let EventBody::AssistantReasoning { text, status, .. } = &event.body else {
        return None;
    };
    if *status == AssistantSemanticStatus::Streaming {
        return None;
    }
    Some(ThinkingView {
        text: text.clone(),
        live: false,
        fade: elapsed.as_secs_f32() * 1000.0 / FADE_MS as f32,
    })
}

/// Rows the docked box takes from the conversation column.
/// 停靠模式下框体占用对话列的行数。
pub const DOCKED_HEIGHT: u16 = 6;

/// Floating box geometry inside the transcript area.
/// 浮动框在会话记录区域内的几何位置。
pub fn float_area(transcript: Rect) -> Option<Rect> {
    if transcript.width < 40 || transcript.height < 10 {
        return None;
    }
    // The box takes a little over half the transcript, so it never covers the reply it floats beside.
    // 浮动框占会话记录略多于一半的宽度，不会遮住其旁边的回复。
    let width = (transcript.width * 11 / 20).max(36);
    Some(Rect {
        x: transcript.right() - width - 1,
        y: transcript.y,
        width,
        height: 8.min(transcript.height / 2),
    })
}

pub fn render(app: &App, view: &ThinkingView, area: Rect, buf: &mut Buffer) {
    // Clicking the box opens the live reasoning in the inspector.
    // 点击该框会在检查器中打开实时推理。
    app.hits.add(area, crate::app::hit::Target::ThinkingBox);
    let theme = &app.theme;
    let phase = app.phase();
    let bg = mix(theme.popover, theme.bg, 0.9);
    // `fade` 0 → full color, 1 → background.
    // `fade` 为 0 时全彩，为 1 时与背景一致。
    let fade = |color| mix(color, bg, 1.0 - view.fade);
    let pulse = if view.live {
        0.5 + 0.5 * (phase * 3.0).sin()
    } else {
        1.0
    };
    let border = fade(mix(theme.primary, theme.border, 0.35 + 0.65 * pulse));

    let mut title = vec![Span::raw(" ")];
    if view.live {
        title.push(Span::styled(
            format!("{} ", spinner(phase)),
            Style::new().fg(theme.primary),
        ));
        title.extend(shimmer(
            &tr!("conversation.block.thinkingActive"),
            theme.primary,
            theme.flame_top,
            phase,
        ));
    } else {
        title.push(Span::styled(
            format!("✓ {}", tr!("conversation.block.thinking")),
            Style::new().fg(fade(theme.violet)),
        ));
    }
    title.push(Span::raw(" "));
    let placement = match app.thinking_placement {
        ThinkingPlacement::Float => "Ctrl+T ⇣",
        ThinkingPlacement::Docked => "Ctrl+T ⇡",
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .style(Style::new().bg(bg))
        .title(Line::from(title))
        .title_bottom(
            Line::from(Span::styled(
                format!(" {placement} "),
                Style::new().fg(fade(theme.muted_fg)),
            ))
            .right_aligned(),
        );
    super::text::clear_area(area, buf);
    let inner = block.inner(area);
    block.render(area, buf);

    let style = Style::new()
        .fg(fade(theme.muted_fg))
        .add_modifier(Modifier::ITALIC);
    let mut lines: Vec<Line> = Vec::new();
    for paragraph in view.text.split('\n').filter(|p| !p.trim().is_empty()) {
        lines.extend(wrap(
            &[Span::styled(paragraph.to_string(), style)],
            inner.width.saturating_sub(1) as usize,
            &[],
            &[],
        ));
    }
    // Tail-follow: always show the newest lines; the oldest visible line is dimmed.
    // 跟随尾部：总是显示最新的行；最旧的可见行变暗。
    let height = inner.height as usize;
    let skip = lines.len().saturating_sub(height);
    for (row, mut line) in lines.into_iter().skip(skip).enumerate() {
        if row == 0 && skip > 0 {
            line = line.style(Style::new().fg(mix(theme.muted_fg, bg, 0.45 * (1.0 - view.fade))));
            for span in &mut line.spans {
                span.style = span
                    .style
                    .fg(mix(theme.muted_fg, bg, 0.45 * (1.0 - view.fade)));
            }
        }
        line.render(
            Rect {
                x: inner.x + 1,
                y: inner.y + row as u16,
                width: inner.width.saturating_sub(1),
                height: 1,
            },
            buf,
        );
    }
}
