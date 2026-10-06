// Notice renderer: draws every notice component, pushed or live, at its anchor.
// 通知渲染器：在锚点绘制每个通知组件，无论是推送的还是实时派生的。

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use super::pages::tone_color;
use super::text::{clear_area, spinner, truncate, width_of, wrap};
use crate::app::App;
use crate::app::notice::{Frame, NoticeSpec, Placement, live};
use crate::app::pages::Tone;

/// Message lines a notice shows before it is cut.
/// 通知在截断前显示的消息行数。
const MAX_LINES: usize = 3;

/// Icon of a tone.
/// 色调的图标。
fn icon(tone: Tone) -> &'static str {
    match tone {
        Tone::Info => "ℹ",
        Tone::Good => "✓",
        Tone::Warn => "⚠",
        Tone::Bad => "✗",
        Tone::Accent => "◆",
        Tone::Normal | Tone::Muted => "•",
    }
}

/// Draws live notices first, nearest their anchors, then pushed ones from newest to oldest.
/// 先绘制实时通知（最靠近锚点），再从新到旧绘制推送通知。
pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let live = live(app);
    let pushed = app.toasts.iter().rev().map(|notice| &notice.spec);
    // Rows already used at each anchor.
    // 每个锚点已占用的行数。
    let mut used: HashMap<Placement, u16> = HashMap::new();
    for spec in live.iter().chain(pushed) {
        let offset = used.entry(spec.placement).or_default();
        match draw(app, spec, area, *offset, buf) {
            Some(height) => *offset += height,
            None => continue,
        }
    }
}

/// Draws one notice `offset` rows away from its anchor; returns its height, or None when it does
/// not fit.
/// 在距锚点 `offset` 行处绘制一个通知；返回其高度，放不下时返回 None。
fn draw(app: &App, spec: &NoticeSpec, area: Rect, offset: u16, buf: &mut Buffer) -> Option<u16> {
    let theme = &app.theme;
    let tone = tone_color(theme, spec.tone);
    let title_color = tone_color(theme, spec.title_tone.unwrap_or(spec.tone));
    let frame_color = tone_color(theme, spec.frame_tone.unwrap_or(spec.tone));
    let glyph = if spec.progress {
        spinner(app.phase())
    } else {
        icon(spec.tone)
    };
    let max_width = (area.width / 2).clamp(20, 56) as usize;
    // Columns around the text: the box border or the bar and icon, plus padding.
    // 文本周围的列：方框边框或竖条与图标，再加留白。
    let chrome = match spec.frame {
        Frame::Box => 4,
        Frame::Bar => 5,
        Frame::Plain => 4,
    };
    let room = max_width.saturating_sub(chrome).max(1);
    let mut message: Vec<Line<'static>> = wrap(
        &[Span::styled(
            spec.message.clone(),
            Style::new().fg(theme.fg),
        )],
        room,
        &[],
        &[],
    );
    if message.len() > MAX_LINES {
        message.truncate(MAX_LINES);
        if let Some(last) = message.last_mut() {
            let text: String = last
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            *last = Line::from(Span::styled(
                truncate(&format!("{text}…"), room),
                Style::new().fg(theme.fg),
            ));
        }
    }
    let title = spec
        .title
        .as_ref()
        .map(|title| truncate(title, room.saturating_sub(2)));
    let text_width = message
        .iter()
        .map(Line::width)
        .chain(title.iter().map(|title| width_of(title) + 2))
        .max()
        .unwrap_or(0);
    let width = (text_width + chrome) as u16;
    let height = match spec.frame {
        Frame::Box => message.len() + 2,
        _ => message.len() + usize::from(title.is_some()),
    } as u16;
    // One row of margin from the anchoring edge, as before.
    // 与锚定边缘保留一行间距，保持原有效果。
    let y = if spec.placement.bottom() {
        area.bottom().checked_sub(1 + offset + height)?
    } else {
        area.y + 1 + offset
    };
    if width + 2 > area.width || y < area.y || y + height > area.bottom() {
        return None;
    }
    let x = match spec.placement {
        Placement::TopRight | Placement::BottomRight => area.right() - width - 1,
        Placement::TopLeft | Placement::BottomLeft => area.x + 1,
        Placement::TopCenter | Placement::BottomCenter => area.x + (area.width - width) / 2,
    };
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    clear_area(rect, buf);
    let background = Style::new().bg(theme.popover);
    let title_style = Style::new().fg(title_color).add_modifier(Modifier::BOLD);
    let icon_style = Style::new().fg(tone).add_modifier(Modifier::BOLD);
    match spec.frame {
        Frame::Box => {
            let mut block = Block::new()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(frame_color))
                .style(background);
            let mut heading = vec![Span::styled(format!(" {glyph} "), icon_style)];
            if let Some(title) = &title {
                heading.push(Span::styled(format!("{title} "), title_style));
            }
            block = block.title(Line::from(heading));
            let inner = block.inner(rect);
            block.render(rect, buf);
            for (row, line) in message.into_iter().enumerate() {
                let mut spans = vec![Span::raw(" ")];
                spans.extend(line.spans);
                Line::from(spans).render(
                    Rect {
                        y: inner.y + row as u16,
                        height: 1,
                        ..inner
                    },
                    buf,
                );
            }
        }
        Frame::Bar | Frame::Plain => {
            let lead = |first: bool| {
                let mut spans = Vec::new();
                if spec.frame == Frame::Bar {
                    spans.push(Span::styled("▌", Style::new().fg(frame_color)));
                } else {
                    spans.push(Span::raw(" "));
                }
                spans.push(if first {
                    Span::styled(format!("{glyph} "), icon_style)
                } else {
                    Span::raw("  ")
                });
                spans
            };
            let mut lines = Vec::new();
            if let Some(title) = &title {
                let mut spans = lead(true);
                spans.push(Span::styled(title.clone(), title_style));
                lines.push(spans);
            }
            for (row, line) in message.into_iter().enumerate() {
                let mut spans = lead(row == 0 && title.is_none());
                spans.extend(line.spans);
                lines.push(spans);
            }
            for (row, spans) in lines.into_iter().enumerate() {
                Line::from(spans).style(background).render(
                    Rect {
                        y: rect.y + row as u16,
                        height: 1,
                        ..rect
                    },
                    buf,
                );
            }
        }
    }
    Some(height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::notice::Component;
    use crate::backend::mock;
    use crate::ui::theme::Theme;
    use serde_json::{Value, json};

    fn declare(value: Value) -> NoticeSpec {
        match Component::parse(value) {
            Ok(Component::Notice(spec)) => spec,
            Err(error) => panic!("test declaration is valid: {error}"),
        }
    }

    /// Column and row where `text` starts in the buffer.
    /// `text` 在缓冲区中起始的列与行。
    fn locate(buf: &Buffer, text: &str) -> (u16, u16) {
        let area = buf.area;
        (area.y..area.bottom())
            .find_map(|y| {
                let row: Vec<&str> = (area.x..area.right())
                    .map(|x| buf[(x, y)].symbol())
                    .collect();
                let line = row.concat();
                let byte = line.find(text)?;
                // Cell symbols here are single characters, so characters count columns.
                // 此处单元格符号均为单个字符，因此字符数即列数。
                Some((line[..byte].chars().count() as u16 + area.x, y))
            })
            .unwrap_or_else(|| panic!("`{text}` is drawn"))
    }

    /// Each placement anchors to its edge, newer notices sit nearest the anchor, and box frames
    /// carry their title in the border.
    /// 每种定位锚定到其边缘，较新的通知最靠近锚点，方框外框在边框中显示标题。
    #[test]
    fn placements_anchor_to_their_edges_and_boxes_carry_titles() {
        let mut app = App::new(mock::spawn(), Theme::dark(), true);
        app.notify(declare(
            json!({ "type": "notice", "message": "older right" }),
        ));
        app.notify(declare(
            json!({ "type": "notice", "message": "newer right" }),
        ));
        app.notify(declare(json!({
            "type": "notice",
            "message": "left box",
            "title": "Sync",
            "frame": "box",
            "placement": "bottom_left",
        })));
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        render(&app, area, &mut buf);

        let (newer_x, newer_y) = locate(&buf, "newer right");
        let (_, older_y) = locate(&buf, "older right");
        assert_eq!(
            newer_y,
            area.y + 1,
            "the newest notice touches the top margin"
        );
        assert!(
            older_y > newer_y,
            "older notices stack away from the anchor"
        );
        assert!(
            newer_x > area.width / 2,
            "top-right notices sit on the right half"
        );

        let (box_x, box_y) = locate(&buf, "left box");
        let (title_x, title_y) = locate(&buf, "Sync");
        assert!(
            box_x < area.width / 4,
            "bottom-left notices sit on the left"
        );
        assert!(
            box_y > area.height / 2,
            "bottom notices sit on the lower half"
        );
        assert_eq!(
            title_y + 1,
            box_y,
            "the title sits in the border above the message"
        );
        assert_eq!(
            buf[(title_x - 4, title_y)].symbol(),
            "╭",
            "box frames draw a rounded border"
        );

        app.toasts.clear();
        app.notify(declare(json!({
            "type": "notice",
            "message": "centered",
            "placement": "top_center",
            "frame": "plain",
        })));
        let mut buf = Buffer::empty(area);
        render(&app, area, &mut buf);
        let (x, _) = locate(&buf, "centered");
        let middle = x + width_of("centered") as u16 / 2;
        assert!(
            middle.abs_diff(area.width / 2) <= 3,
            "top-center notices are centered"
        );
    }
}
