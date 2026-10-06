// History browser and inspector: explore everything the conversation produced, including completed
// thinking, full tool input/output and replies, without folding.
// 历史浏览器与检视器：浏览会话产生的全部内容，包括已完成的思考、完整的工具输入/输出与回复，
// 不做折叠。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::chrome::ButtonKind;
use super::markdown;
use super::overlays::{button_row, frame, modal};
use super::text::{compact_count, compact_duration, truncate, width_of, wrap};
use super::theme::{Theme, mix};
use super::transcript::severity_icon;
use crate::app::App;
use crate::app::action::UiAction;
use crate::app::history::{self as index, EntryKind};
use crate::app::hit::Target;
use crate::app::overlay::{History, HistoryFilter, Inspector};
use crate::model::*;
use crate::tr;

fn kind_icon(theme: &Theme, kind: EntryKind) -> (&'static str, Style) {
    match kind {
        EntryKind::Prompt => ("›", theme.primary_text()),
        EntryKind::Thinking => ("✦", Style::new().fg(theme.violet)),
        EntryKind::Tools => ("⚙", Style::new().fg(theme.info)),
        EntryKind::Reply => ("◆", Style::new().fg(theme.success)),
        EntryKind::Note => ("ℹ", Style::new().fg(theme.warning)),
        EntryKind::Stats => ("Σ", theme.muted_text()),
    }
}

fn big_rect(area: Rect) -> Rect {
    let width = area.width.saturating_sub(8);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + 1,
        width,
        height: area.height.saturating_sub(2),
    }
}

pub fn render_history(app: &App, state: &mut History, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let rect = big_rect(area);
    let footer = "↑↓ · Enter · ←→ · Esc".to_string();
    let inner = modal(
        app,
        rect,
        frame(theme, tr!("tui.history.title"), footer),
        buf,
    );
    if inner.height < 5 {
        return;
    }
    let x = inner.x + 1;
    let width = inner.width.saturating_sub(2);

    // Filter tabs.
    // 过滤标签。
    let mut fx = x;
    for filter in HistoryFilter::ALL {
        let text = format!(" {} ", filter.label());
        let w = width_of(&text) as u16;
        if fx + w > x + width {
            break;
        }
        let style = if state.filter == filter {
            Style::new()
                .fg(theme.primary_fg)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new()
                .fg(theme.muted_fg)
                .bg(mix(theme.accent, theme.popover, 0.9))
        };
        buf.set_string(fx, inner.y, &text, style);
        app.hits.add(
            Rect {
                x: fx,
                y: inner.y,
                width: w,
                height: 1,
            },
            Target::HistoryFilter(filter),
        );
        fx += w + 1;
    }

    // Query field (typing goes straight into it).
    // 查询输入框（键入内容直接进入）。
    let query_row = Rect {
        x,
        y: inner.y + 1,
        width,
        height: 1,
    };
    buf.set_style(
        query_row,
        Style::new().bg(mix(theme.input, theme.popover, 0.5)),
    );
    let query = if state.query.is_empty() {
        Line::from(vec![
            Span::styled(" ⌕ ", theme.primary_text()),
            Span::styled(
                tr!("tui.history.search"),
                Style::new().fg(mix(theme.muted_fg, theme.popover, 0.6)),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ⌕ ", theme.primary_text()),
            Span::styled(state.query.clone(), Style::new().fg(theme.fg)),
            Span::styled("▏", theme.primary_text()),
        ])
    };
    query.render(query_row, buf);

    let events = app
        .session()
        .map(|s| s.events.as_slice())
        .unwrap_or_default();
    let entries = index::visible(events, state.filter, &state.query);
    let list = Rect {
        x,
        y: inner.y + 3,
        width,
        height: inner.height.saturating_sub(5),
    };
    if entries.is_empty() {
        buf.set_string(
            list.x + 1,
            list.y,
            tr!("tui.history.empty"),
            theme.muted_text(),
        );
    }
    // `usize::MAX` means "the newest entry" until the user moves.
    // `usize::MAX` 表示“最新条目”，直到用户移动光标。
    state.cursor = state.cursor.min(entries.len().saturating_sub(1));
    let rows = list.height as usize;
    if state.cursor < state.scroll {
        state.scroll = state.cursor;
    } else if rows > 0 && state.cursor >= state.scroll + rows {
        state.scroll = state.cursor + 1 - rows;
    }
    state.scroll = state.scroll.min(entries.len().saturating_sub(rows));
    let start_ms = events.first().map_or(0, |e| e.created_at_ms);
    for (i, entry) in entries.iter().enumerate().skip(state.scroll).take(rows) {
        let y = list.y + (i - state.scroll) as u16;
        let selected = i == state.cursor;
        let (icon, icon_style) = kind_icon(theme, entry.kind);
        let time = format!(
            " +{} ",
            compact_duration(entry.at_ms.saturating_sub(start_ms))
        );
        let seq = format!("#{:<4}", entry.seq);
        let title_width = (width as usize).saturating_sub(width_of(&time) + width_of(&seq) + 5);
        let title = truncate(&entry.title, title_width);
        let pad = title_width.saturating_sub(width_of(&title));
        let row = Rect {
            x: list.x,
            y,
            width: list.width,
            height: 1,
        };
        Line::from(vec![
            Span::styled(if selected { "›" } else { " " }, theme.primary_text()),
            Span::styled(
                seq,
                Style::new().fg(mix(theme.muted_fg, theme.popover, 0.6)),
            ),
            Span::styled(format!(" {icon} "), icon_style),
            Span::styled(
                title,
                if selected {
                    theme.strong()
                } else {
                    theme.base().bg(theme.popover)
                },
            ),
            Span::raw(" ".repeat(pad)),
            Span::styled(time, theme.muted_text()),
        ])
        .style(if selected {
            Style::new().bg(theme.selection_bg())
        } else {
            Style::new()
        })
        .render(row, buf);
        app.hits.add(row, Target::HistoryItem(i));
    }

    let bar_y = inner.bottom() - 1;
    let count = tr!("tui.history.count", count = entries.len());
    buf.set_string(x, bar_y, truncate(&count, 24), theme.muted_text());
    let mut buttons = vec![(tr!("tui.dialog.close"), ButtonKind::Normal, UiAction::Close)];
    if let Some(entry) = entries.get(state.cursor) {
        buttons.push((
            format!("⤢ {}", tr!("tui.history.inspect")),
            ButtonKind::Primary,
            UiAction::Inspect(entry.seq),
        ));
    }
    button_row(app, buf, Rect { x, width, ..inner }, bar_y, buttons);
}

pub fn render_inspector(app: &App, inspector: &mut Inspector, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let rect = big_rect(area);
    let events = app
        .session()
        .map(|s| s.events.as_slice())
        .unwrap_or_default();
    let entries = index::entries(events);
    let position = entries.iter().position(|e| e.seq == inspector.seq);
    let title = match position.and_then(|i| entries.get(i)) {
        Some(entry) => format!(
            "{} #{} · {}",
            tr!("tui.inspector.title"),
            entry.seq,
            truncate(&entry.title, 48)
        ),
        None => tr!("tui.inspector.title"),
    };
    let footer = "↑↓ · PgUp/PgDn · [ ] · Esc".to_string();
    let inner = modal(app, rect, frame(theme, title, footer), buf);
    if inner.height < 3 {
        return;
    }
    let body = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        height: inner.height - 2,
        ..inner
    };
    let width = body.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for event in index::events_of(events, inspector.seq) {
        event_lines(theme, event, width, &mut lines);
        lines.push(Line::default());
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            tr!("tui.history.empty"),
            theme.muted_text(),
        )));
    }
    let height = body.height as usize;
    inspector.scroll = inspector.scroll.min(lines.len().saturating_sub(height));
    for (row, line) in lines.iter().skip(inspector.scroll).take(height).enumerate() {
        line.render(
            Rect {
                y: body.y + row as u16,
                height: 1,
                ..body
            },
            buf,
        );
    }
    // Scroll position.
    // 滚动位置。
    if lines.len() > height {
        let text = format!(
            " {}/{} ",
            (inspector.scroll + height).min(lines.len()),
            lines.len()
        );
        buf.set_string(rect.x + 2, rect.bottom() - 1, text, theme.muted_text());
    }

    // Navigation buttons: previous / next entry, back to the list, close.
    // 导航按钮：上一条 / 下一条、返回列表、关闭。
    let bar_y = inner.bottom() - 1;
    let mut buttons = Vec::new();
    if let Some(prev) = position
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| entries.get(i))
    {
        buttons.push((
            format!("‹ {}", tr!("tui.inspector.prev")),
            ButtonKind::Normal,
            UiAction::Inspect(prev.seq),
        ));
    }
    if let Some(next) = position.and_then(|i| entries.get(i + 1)) {
        buttons.push((
            format!("{} ›", tr!("tui.inspector.next")),
            ButtonKind::Normal,
            UiAction::Inspect(next.seq),
        ));
    }
    if inspector.back.is_some() {
        buttons.push((
            format!("↩ {}", tr!("tui.inspector.back")),
            ButtonKind::Normal,
            UiAction::Close,
        ));
    } else {
        buttons.push((tr!("tui.dialog.close"), ButtonKind::Normal, UiAction::Close));
    }
    button_row(
        app,
        buf,
        Rect {
            x: body.x,
            width: body.width,
            ..inner
        },
        bar_y,
        buttons,
    );
}

/// One line of code or tool output, indented by two columns. `wrap` drops leading spaces, so the
/// line's own indentation moves into the prefix and survives on continuation lines too.
/// 一行代码或工具输出，缩进两列。`wrap` 会丢弃行首空格，因此把该行自身的缩进并入前缀，续行同样保留。
fn code_line(line: &str, style: Style, width: usize, out: &mut Vec<Line<'static>>) {
    let body = line.trim_start();
    let prefix = [Span::raw(format!("  {}", &line[..line.len() - body.len()]))];
    out.extend(wrap(
        &[Span::styled(body.to_string(), style)],
        width,
        &prefix,
        &prefix,
    ));
}

/// Every line of one event, unfolded.
/// 单个事件的全部行，不折叠。
fn event_lines(
    theme: &Theme,
    event: &ConversationEvent,
    width: usize,
    out: &mut Vec<Line<'static>>,
) {
    let header = |icon: &str, style: Style, text: String| {
        Line::from(vec![
            Span::styled(format!("{icon} "), style),
            Span::styled(text, style.add_modifier(Modifier::BOLD)),
        ])
    };
    let indent = [Span::raw("  ".to_string())];
    match &event.body {
        EventBody::UserInput {
            text, attachments, ..
        } => {
            out.push(header(
                "›",
                theme.primary_text(),
                tr!("tui.inspector.prompt"),
            ));
            out.extend(wrap(
                &[Span::styled(text.clone(), Style::new().fg(theme.fg))],
                width,
                &indent,
                &indent,
            ));
            for name in attachments {
                out.push(Line::from(Span::styled(
                    format!("  ⊕ {name}"),
                    Style::new().fg(theme.info),
                )));
            }
        }
        EventBody::AssistantReasoning { text, status, .. } => {
            let mut title = tr!("tui.inspector.thinking");
            if *status == AssistantSemanticStatus::Streaming {
                title.push_str(" …");
            }
            out.push(header("✦", Style::new().fg(theme.violet), title));
            out.extend(wrap(
                &[Span::styled(
                    text.clone(),
                    theme.muted_text().add_modifier(Modifier::ITALIC),
                )],
                width,
                &indent,
                &indent,
            ));
        }
        EventBody::AssistantText { text, .. } => {
            out.push(header(
                "◆",
                Style::new().fg(theme.success),
                tr!("tui.inspector.reply"),
            ));
            for line in markdown::render(text, width.saturating_sub(2), theme) {
                let mut spans = vec![Span::raw("  ")];
                spans.extend(line.spans);
                out.push(Line::from(spans).style(line.style));
            }
        }
        EventBody::ToolCall {
            tool_name,
            input,
            output,
            changeset,
            status,
            started_at_ms,
            finished_at_ms,
            ..
        } => {
            let (mark, color) = match status {
                ToolCallStatus::Completed => ("✓", theme.success),
                ToolCallStatus::Failed => ("✗", theme.danger),
                ToolCallStatus::Cancelled
                | ToolCallStatus::Stopped
                | ToolCallStatus::Interrupted => ("■", theme.muted_fg),
                ToolCallStatus::WaitingUser => ("?", theme.warning),
                ToolCallStatus::Prepared | ToolCallStatus::Running => ("…", theme.primary),
            };
            let duration = match (started_at_ms, finished_at_ms) {
                (Some(start), Some(end)) => {
                    format!("  {}", compact_duration(end.saturating_sub(*start)))
                }
                _ => String::new(),
            };
            out.push(Line::from(vec![
                Span::styled("⚙ ", Style::new().fg(theme.info)),
                Span::styled(tool_name.clone(), theme.strong()),
                Span::styled(format!("  {mark}"), Style::new().fg(color)),
                Span::styled(duration, theme.muted_text()),
            ]));
            section(theme, tr!("tui.inspector.input"), out);
            let pretty = serde_json::to_string_pretty(input).unwrap_or_default();
            let code = Style::new()
                .fg(theme.fg)
                .bg(mix(theme.muted, theme.popover, 0.6));
            for line in pretty.lines() {
                code_line(line, code, width, out);
            }
            if !changeset.is_empty() {
                section(theme, tr!("tui.inspector.changes"), out);
                for file in changeset {
                    out.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(file.path.clone(), Style::new().fg(theme.fg)),
                        Span::styled(
                            format!("  +{}", file.additions),
                            Style::new().fg(theme.success),
                        ),
                        Span::styled(
                            format!(" -{}", file.deletions),
                            Style::new().fg(theme.danger),
                        ),
                    ]));
                }
            }
            if let Some(output) = output {
                section(theme, tr!("tui.inspector.output"), out);
                for line in output.lines() {
                    code_line(line, theme.muted_text(), width, out);
                }
            }
        }
        EventBody::DisplayNote { severity, message } => {
            let (icon, color) = severity_icon(theme, *severity);
            out.extend(wrap(
                &[Span::styled(message.clone(), Style::new().fg(color))],
                width,
                &[Span::styled(format!("{icon} "), Style::new().fg(color))],
                &indent,
            ));
        }
        EventBody::TurnStats {
            duration_ms,
            total_tokens,
            requests,
            tool_calls,
            ..
        } => {
            out.push(Line::from(Span::styled(
                format!(
                    "Σ {} · {} tokens · {} req · {} tools",
                    compact_duration(*duration_ms),
                    compact_count(*total_tokens),
                    requests,
                    tool_calls
                ),
                theme.muted_text(),
            )));
        }
    }
}

fn section(theme: &Theme, title: String, out: &mut Vec<Line<'static>>) {
    out.push(Line::from(Span::styled(
        format!("  ─ {title}"),
        theme.muted_text().add_modifier(Modifier::BOLD),
    )));
}
