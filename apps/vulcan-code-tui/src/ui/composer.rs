// Composer: attachment chips, the prompt input, a toolbar of clickable badges (mode, attach,
// model, reasoning, stop), queued prompts and the slash popup.
// 输入框：附件标记、提示输入、可点击徽标工具栏（模式、附件、模型、推理强度、停止），以及排队提示
// 与斜杠命令弹窗。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use super::chrome::{ButtonKind, button, button_style};
use super::effort_label;
use super::text::{spinner, truncate, width_of};
use super::theme::mix;
use crate::app::action::UiAction;
use crate::app::commands::AppCommand;
use crate::app::hit::Target;
use crate::app::{App, Focus, input};
use crate::backend::Command;
use crate::model::ExecutionMode;
use crate::tr;

/// Text rows the composer may grow to, as a share of the conversation column's height.
/// 输入框文本最多可增长到的行数，按对话列高度的比例计算。
const TEXT_ROWS_SHARE: u16 = 3;
const TEXT_ROWS_MIN: u16 = 3;
const TEXT_ROWS_MAX: u16 = 12;

/// Most text rows the composer shows in a column `column_height` rows tall; longer drafts scroll.
/// 在高 `column_height` 行的列中输入框最多显示的文本行数；更长的草稿滚动显示。
pub fn max_text_rows(column_height: u16) -> u16 {
    (column_height / TEXT_ROWS_SHARE).clamp(TEXT_ROWS_MIN, TEXT_ROWS_MAX)
}

/// Rows the composer needs: text (at most `max_rows`), optional attachment row, toolbar and
/// borders.
/// 输入框所需行数：文本（至多 `max_rows` 行）、可选的附件行、工具栏与边框。
pub fn height(app: &App, width: u16, max_rows: u16) -> u16 {
    let inner = width.saturating_sub(2).max(1) as usize;
    let rows: usize = app
        .composer
        .lines()
        .iter()
        .map(|line| (width_of(line) / inner) + 1)
        .sum();
    (rows as u16).clamp(1, max_rows.max(1)) + 3 + u16::from(!app.attachments.is_empty())
}

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let focused = app.focus == Focus::Composer;
    let border = if focused { theme.primary } else { theme.input };
    // No border hint: the upstream `composer.shortcutHint` names Shift+Enter, which terminals cannot
    // tell apart from Enter; the status bar shows the TUI's own keys (`tui.hint.composer`).
    // 不显示边框提示：上游 `composer.shortcutHint` 使用 Shift+Enter，终端无法将其与 Enter 区分；
    // 状态栏会显示 TUI 自己的按键（`tui.hint.composer`）。
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .style(Style::new().bg(theme.card));
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(area, Target::Focus(Focus::Composer));
    if inner.height < 2 {
        return;
    }
    let attachment_rows = u16::from(!app.attachments.is_empty() && inner.height > 2);
    let text_area = Rect {
        y: inner.y + attachment_rows,
        height: inner.height.saturating_sub(1 + attachment_rows),
        ..inner
    };
    if attachment_rows > 0 {
        attachments(app, Rect { height: 1, ..inner }, buf);
    }
    toolbar(
        app,
        Rect {
            y: inner.bottom() - 1,
            height: 1,
            ..inner
        },
        buf,
    );

    app.composer.remove_block();
    app.composer
        .set_style(Style::new().fg(theme.fg).bg(theme.card));
    app.composer
        .set_placeholder_style(Style::new().fg(mix(theme.muted_fg, theme.card, 0.75)));
    app.composer.set_cursor_line_style(Style::new());
    app.composer.set_cursor_style(if focused {
        Style::new().fg(theme.card).bg(theme.primary)
    } else {
        Style::new().bg(theme.card)
    });
    (&app.composer).render(text_area, buf);
    paste_chips(app, text_area, buf);
}

/// Restyles the paste tokens the text area drew as chips; clicking one previews that paste.
/// A token wrapped across two rows keeps the plain style.
/// 将文本区绘制的粘贴标记重设为标签样式；点击即预览该粘贴。跨两行折返的标记保持普通样式。
fn paste_chips(app: &App, area: Rect, buf: &mut Buffer) {
    if app.pastes.is_empty() {
        return;
    }
    let theme = &app.theme;
    let chip = Style::new()
        .fg(theme.fg)
        .bg(mix(theme.violet, theme.card, 0.7));
    for y in area.top()..area.bottom() {
        // The row's text and the column where each cell's symbol starts.
        // 该行文本，以及每个单元格符号起始的列。
        let mut row = String::new();
        let mut starts = Vec::new();
        let mut x = area.x;
        while x < area.right() {
            let symbol = buf[(x, y)].symbol();
            starts.push((row.len(), x));
            row.push_str(symbol);
            x += width_of(symbol).max(1) as u16;
        }
        for (index, paste) in app.pastes.iter().enumerate() {
            let mut from = 0;
            while let Some(offset) = row[from..].find(&paste.token) {
                let byte = from + offset;
                from = byte + paste.token.len();
                let Ok(cell) = starts.binary_search_by_key(&byte, |(b, _)| *b) else {
                    continue;
                };
                let rect = Rect {
                    x: starts[cell].1,
                    y,
                    width: (width_of(&paste.token) as u16).min(area.right() - starts[cell].1),
                    height: 1,
                };
                buf.set_style(rect, chip);
                app.hits
                    .add(rect, Target::Do(UiAction::PastePreview(Some(index))));
            }
        }
    }
}

/// Attachment chips `⊕ name ×`; clicking × removes that attachment.
/// 附件标记 `⊕ 名称 ×`；点击 × 移除该附件。
fn attachments(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let mut x = area.x;
    for (index, path) in app.attachments.iter().enumerate() {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let text = format!(" ⊕ {} ", truncate(name, 24));
        let w = width_of(&text) as u16;
        if x + w + 6 > area.right() {
            buf.set_string(
                x,
                area.y,
                format!("+{}", app.attachments.len() - index),
                theme.muted_text(),
            );
            break;
        }
        let chip = Style::new()
            .fg(theme.fg)
            .bg(mix(theme.info, theme.card, 0.75));
        buf.set_string(x, area.y, &text, chip);
        buf.set_string(
            x + w,
            area.y,
            "× ",
            chip.fg(theme.danger).add_modifier(Modifier::BOLD),
        );
        app.hits.add(
            Rect {
                x: x + w,
                y: area.y,
                width: 2,
                height: 1,
            },
            Target::Do(UiAction::RemoveAttachment(index)),
        );
        x += w + 3;
    }
}

/// Toolbar: mode and attach on the left; model, reasoning and (while running) stop on the right.
/// 工具栏：左侧为模式与附件；右侧为模型、推理强度与（运行中时的）停止。
fn toolbar(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let bg = theme.card;
    let (mode_label, mode_bg) = match app.mode() {
        ExecutionMode::Build => (tr!("composer.mode.execute"), theme.info),
        ExecutionMode::Plan => (tr!("composer.mode.plan"), theme.violet),
    };
    let mut x = area.x;
    x += button(
        app,
        buf,
        x,
        area.y,
        &mode_label,
        Style::new()
            .fg(theme.primary_fg)
            .bg(mode_bg)
            .add_modifier(Modifier::BOLD),
        UiAction::App(AppCommand::TogglePlan),
    );
    let hint = " ⇧Tab ";
    buf.set_string(
        x,
        area.y,
        hint,
        Style::new().fg(mix(theme.muted_fg, bg, 0.6)),
    );
    x += width_of(hint) as u16;
    let attach = format!("⊕ {}", tr!("composer.attachment.add"));
    if x + width_of(&attach) as u16 + 24 < area.right() {
        x += button(
            app,
            buf,
            x,
            area.y,
            &attach,
            button_style(app, ButtonKind::Normal, bg),
            UiAction::Attach,
        );
    }

    // Right side, laid out from the right edge; items that do not fit are dropped from the left.
    // 右侧从右边缘向左排布；放不下的项从左侧开始省略。
    let model = app.model().map(|m| m.label.clone()).unwrap_or_default();
    let chip = Style::new().fg(theme.fg).bg(theme.control_bg());
    let mut right: Vec<(String, Style, UiAction)> = vec![
        (
            format!("{} ▾", truncate(&model, 28)),
            chip,
            UiAction::App(AppCommand::SelectModel),
        ),
        (
            format!("◔ {}", effort_label(app.effort)),
            chip.fg(theme.muted_fg),
            UiAction::App(AppCommand::CycleReasoning),
        ),
    ];
    if app.run_phase().is_active()
        && let Some(id) = app.selected.clone()
    {
        right.push((
            format!("{} ■ {}", spinner(app.phase()), tr!("composer.aria.stop")),
            Style::new()
                .fg(theme.primary_fg)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
            UiAction::Send(Command::StopConversationRun {
                conversation_id: id,
            }),
        ));
    }
    let mut rx = area.right();
    let floor = x + 1;
    for (label, style, action) in right.into_iter().rev() {
        let w = width_of(&label) as u16 + 2;
        if rx < floor + w {
            break;
        }
        rx -= w;
        button(app, buf, rx, area.y, &label, style, action);
        rx = rx.saturating_sub(1);
    }
}

/// Slash command completion above the composer; returns nothing when no command matches.
/// 输入框上方的斜杠命令补全；没有匹配时不显示。
pub fn render_slash_popup(app: &App, composer: Rect, buf: &mut Buffer) {
    let matches = input::slash_matches(app);
    if matches.is_empty() || app.focus != Focus::Composer {
        return;
    }
    let theme = &app.theme;
    let rows = matches.len().min(8) as u16;
    let width = composer.width.min(64);
    let area = Rect {
        x: composer.x + 1,
        y: composer.y.saturating_sub(rows + 2),
        width,
        height: rows + 2,
    };
    super::text::clear_area(area, buf);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .style(Style::new().bg(theme.popover))
        .title_bottom(Line::from(Span::styled(" Tab ", theme.muted_text())).right_aligned());
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(area, Target::Inert);
    for (row, command) in matches.iter().take(rows as usize).enumerate() {
        let highlight = row == 0;
        let name = format!("/{:<9}", command.slash());
        let shortcut = command.shortcut();
        let label_width =
            (inner.width as usize).saturating_sub(width_of(&name) + width_of(shortcut) + 3);
        let line = Line::from(vec![
            Span::styled(
                format!(" {name}"),
                if highlight {
                    theme.primary_text().add_modifier(Modifier::BOLD)
                } else {
                    theme.primary_text()
                },
            ),
            Span::styled(
                format!(" {}", truncate(&command.label(), label_width)),
                theme.base().bg(theme.popover),
            ),
        ])
        .style(if highlight {
            Style::new().bg(theme.selection_bg())
        } else {
            Style::new()
        });
        let y = inner.y + row as u16;
        let rect = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };
        line.render(rect, buf);
        app.hits.add(rect, Target::SlashItem(row));
        if !shortcut.is_empty() {
            let x = inner.right().saturating_sub(width_of(shortcut) as u16 + 1);
            buf.set_string(x, y, shortcut, theme.muted_text());
        }
    }
}

/// Queued prompts waiting for the current run, listed above the composer.
/// 等待当前运行结束的排队提示，列在输入框上方。
pub fn queued_height(app: &App) -> u16 {
    app.session().map_or(0, |s| {
        if s.prompts.is_empty() {
            0
        } else {
            s.prompts.len().min(3) as u16 + 1
        }
    })
}

pub fn render_queued(app: &App, area: Rect, buf: &mut Buffer) {
    let Some(session) = app.session() else {
        return;
    };
    if area.height == 0 {
        return;
    }
    let theme = &app.theme;
    let header = Line::from(vec![
        Span::styled("  ⋯ ", theme.primary_text()),
        Span::styled(
            format!("{} ({})", tr!("pendingPrompt.queue"), session.prompts.len()),
            theme.muted_text(),
        ),
    ]);
    header.render(Rect { height: 1, ..area }, buf);
    for (row, prompt) in session.prompts.iter().take(3).enumerate() {
        let text = truncate(
            &prompt.preview.replace('\n', " "),
            area.width.saturating_sub(6) as usize,
        );
        buf.set_string(
            area.x + 4,
            area.y + 1 + row as u16,
            format!("› {text}"),
            theme.base(),
        );
    }
}
