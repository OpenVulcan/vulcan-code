// Small modal dialogs: context menus, confirmations, text prompts and the folder picker used
// to add projects and attach files.
// 小型模态对话框：上下文菜单、确认框、文本输入框，以及用于添加项目与附加文件的文件夹选择器。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use super::chrome::{ButtonKind, button, button_style};
use super::overlays::{button_row, centered, frame, modal};
use super::text::{compact_count, fit, spinner, truncate, width_of, wrap};
use super::theme::mix;
use crate::app::App;
use crate::app::action::{ConfirmDialog, PromptDialog, UiAction};
use crate::app::hit::Target;
use crate::app::overlay::{DetailSheet, FolderMode, FolderPicker, Menu};
use crate::tr;

/// Context menu at its anchor (clamped on screen) or centered.
/// 位于锚点（钳制在屏幕内）或居中的上下文菜单。
pub fn render_menu(app: &App, menu: &mut Menu, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let label_width = menu
        .items
        .iter()
        .map(|i| width_of(&i.label))
        .max()
        .unwrap_or(0);
    let hint_width = menu
        .items
        .iter()
        .map(|i| width_of(&i.hint))
        .max()
        .unwrap_or(0);
    let width = ((label_width + hint_width + 7).max(width_of(&menu.title) + 6) as u16)
        .min(area.width.saturating_sub(2));
    let height = (menu.items.len() as u16 + 2).min(area.height.saturating_sub(2));
    let rect = match menu.anchor {
        Some((x, y)) => Rect {
            x: x.min(area.right().saturating_sub(width + 1)).max(area.x),
            y: if y + height < area.bottom() {
                y + 1
            } else {
                y.saturating_sub(height).max(area.y)
            },
            width,
            height,
        },
        None => centered(area, width, height),
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .style(Style::new().bg(theme.popover).fg(theme.fg))
        .title(Line::from(Span::styled(
            format!(
                " {} ",
                truncate(&menu.title, width.saturating_sub(8) as usize)
            ),
            theme.muted_text(),
        )));
    let inner = modal(app, rect, block, buf);
    menu.cursor = menu.cursor.min(menu.items.len().saturating_sub(1));
    // The visible window follows the cursor so the chosen item is always on screen.
    // 可见窗口跟随光标，使选中项始终可见。
    let visible = inner.height as usize;
    let first = (menu.cursor + 1).saturating_sub(visible);
    for (row, (index, item)) in menu
        .items
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .enumerate()
    {
        let y = inner.y + row as u16;
        let selected = index == menu.cursor;
        let fg = if item.danger { theme.danger } else { theme.fg };
        let hint = width_of(&item.hint);
        let label = truncate(&item.label, (inner.width as usize).saturating_sub(hint + 4));
        let pad = (inner.width as usize).saturating_sub(width_of(&label) + hint + 3);
        let row = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };
        Line::from(vec![
            Span::styled(if selected { "›" } else { " " }, theme.primary_text()),
            Span::styled(format!(" {label}"), Style::new().fg(fg)),
            Span::raw(" ".repeat(pad)),
            Span::styled(format!("{} ", item.hint), theme.muted_text()),
        ])
        .style(if selected {
            Style::new().bg(theme.selection_bg())
        } else {
            Style::new()
        })
        .render(row, buf);
        app.hits.add(row, Target::MenuItem(index));
    }
}

pub fn render_confirm(app: &App, dialog: &mut ConfirmDialog, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let width = 60u16.min(area.width.saturating_sub(2));
    let body = wrap_text(
        &dialog.body,
        width.saturating_sub(6) as usize,
        theme.base().bg(theme.popover),
    );
    let rect = centered(area, width, body.len() as u16 + 5);
    let inner = modal(
        app,
        rect,
        frame(theme, dialog.title.clone(), "Tab · Enter · Esc".to_string()),
        buf,
    );
    for (row, line) in body
        .iter()
        .enumerate()
        .take(inner.height.saturating_sub(2) as usize)
    {
        line.render(
            Rect {
                x: inner.x + 2,
                y: inner.y + 1 + row as u16,
                width: inner.width.saturating_sub(4),
                height: 1,
            },
            buf,
        );
    }
    let confirm_kind = if dialog.danger {
        ButtonKind::Danger
    } else {
        ButtonKind::Primary
    };
    let (cancel, confirm) = if dialog.focus_confirm {
        (ButtonKind::Normal, confirm_kind)
    } else {
        (ButtonKind::Focused, ButtonKind::Normal)
    };
    button_row(
        app,
        buf,
        Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        },
        inner.bottom().saturating_sub(1),
        vec![
            (tr!("tui.dialog.cancel"), cancel, UiAction::Close),
            (
                dialog.confirm_label.clone(),
                confirm,
                UiAction::ConfirmAccept,
            ),
        ],
    );
}

/// Renders transient text with visible line boundaries and an append-position cursor.
/// 渲染临时文本，显示真实行边界及追加位置光标。
/// `app` provides theme and hit registration; `dialog` owns the transient field value.
/// `app` 提供主题及命中登记；`dialog` 持有临时字段值。
/// `area` bounds the modal; `buf` receives its cells. Returns no value.
/// `area` 限制模态范围；`buf` 接收单元格。无返回值。
pub fn render_prompt(app: &App, dialog: &mut PromptDialog, area: Rect, buf: &mut Buffer) {
    // Shared theme used by input, validation and buttons.
    // 输入、校验及按钮使用的共享主题。
    let theme = &app.theme;
    // Modal width leaves the shared close control inside the frame.
    // 模态宽度使共享关闭控件保留在边框内。
    let width = super::overlays::content_modal_width(area, 64);
    // Instructions wrap independently from editable text.
    // 说明与可编辑文本分别换行。
    let description = if dialog.description.is_empty() {
        Vec::new()
    } else {
        wrap_text(
            &dialog.description,
            width.saturating_sub(6) as usize,
            theme.muted_text(),
        )
    };
    // Preserve line boundaries even when masking; no credential value enters a rendered cell.
    // 遮蔽时也保留行边界；任何凭据值都不进入渲染单元格。
    let shown = if dialog.secret {
        dialog
            .input
            .split('\n')
            .map(|line| "•".repeat(line.chars().count()))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        dialog.input.clone()
    };
    // Split keeps a trailing empty line so a newly inserted newline has its own cursor.
    // 分割保留末尾空行，使新插入的换行有独立光标位置。
    let lines: Vec<&str> = shown.split('\n').collect();
    // Show the append position and at most six lines, retaining the complete input in the dialog.
    // 显示追加位置及最多六行，在输入框中保留完整输入。
    let desired_rows = lines.len().min(6) as u16;
    // Shared geometry clamps the modal to the terminal.
    // 共享几何逻辑将模态范围限制在终端内。
    let rect = centered(area, width, description.len() as u16 + desired_rows + 7);
    // Multiline continuations expose their explicit newline shortcut.
    // 多行后续步骤显示明确的换行快捷键。
    let footer = if dialog.accepts_multiline() {
        "Alt+Enter ↵ · Ctrl+U · Enter · Esc"
    } else {
        "Enter · Ctrl+U · Esc"
    };
    // Modal clears wide-character spill and registers the existing close action.
    // 模态清除宽字符溢出并登记现有关闭动作。
    let inner = modal(
        app,
        rect,
        frame(
            theme,
            truncate(&dialog.title, width.saturating_sub(8) as usize),
            footer.into(),
        ),
        buf,
    );
    if inner.height < 5 || inner.width < 5 {
        return;
    }
    // Inset input origin.
    // 内缩的输入起点。
    let x = inner.x + 2;
    // Input columns reserve one leading space and a cursor cell.
    // 输入列保留一个前导空格及一个光标单元格。
    let field_width = inner.width.saturating_sub(4);
    // Reserve input, validation and the shared button row before rendering descriptions.
    // 渲染说明前预留输入、校验及共享按钮行。
    let description_limit = inner.height.saturating_sub(desired_rows + 4) as usize;
    // Current body row.
    // 当前主体行。
    let mut y = inner.y + 1;
    for line in description.iter().take(description_limit) {
        line.render(
            Rect {
                x,
                y,
                width: field_width,
                height: 1,
            },
            buf,
        );
        y += 1;
    }
    y += u16::from(!description.is_empty() && description_limit > 0);
    // Actual visible input rows after terminal clamping.
    // 终端限制后的实际可见输入行数。
    let rows = desired_rows.min(inner.bottom().saturating_sub(y + 2));
    // First visible source line; older lines remain in the submitted value.
    // 首条可见来源行；更早的行仍保留在提交值中。
    let first_line = lines.len().saturating_sub(rows as usize);
    // Complete input background, preserving indentation and source newline boundaries.
    // 完整输入背景，保留缩进及来源换行边界。
    let field = Rect {
        x,
        y,
        width: field_width,
        height: rows,
    };
    buf.set_style(field, Style::new().bg(mix(theme.input, theme.popover, 0.5)));
    for (offset, line) in lines.iter().skip(first_line).enumerate() {
        // The end of a long source line stays visible during append-only editing.
        // 在仅追加的编辑方式下，较长来源行的末尾保持可见。
        let tail = truncate_left(line, field_width.saturating_sub(2) as usize);
        // Cursor belongs only to the final logical line.
        // 光标仅属于最后一条逻辑行。
        let cursor = if first_line + offset + 1 == lines.len() {
            "▏"
        } else {
            ""
        };
        Line::from(vec![
            Span::styled(format!(" {tail}"), Style::new().fg(theme.fg)),
            Span::styled(cursor, theme.primary_text()),
        ])
        .render(
            Rect {
                x,
                y: y + offset as u16,
                width: field_width,
                height: 1,
            },
            buf,
        );
    }
    y += rows;
    if let Some(error) = &dialog.error {
        buf.set_string(
            x,
            y,
            truncate(error, field_width as usize),
            Style::new().fg(theme.danger),
        );
    }
    button_row(
        app,
        buf,
        Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        },
        inner.bottom().saturating_sub(1),
        vec![
            (
                tr!("tui.dialog.cancel"),
                ButtonKind::Normal,
                UiAction::Close,
            ),
            (
                tr!("tui.dialog.ok"),
                ButtonKind::Primary,
                UiAction::PromptAccept,
            ),
        ],
    );
}

/// Folder picker: location bar, filesystem roots, the listing, and mode-specific buttons.
/// 文件夹选择器：地址栏、文件系统根、目录列表与随模式变化的按钮。
pub fn render_folder(app: &App, picker: &mut FolderPicker, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let bg = theme.popover;
    let title = match picker.mode {
        FolderMode::Project => tr!("sidebar.addProject"),
        FolderMode::Attachment => tr!("composer.attachment.add"),
        FolderMode::Plugin => tr!("tui.plugins.installLocal"),
    };
    let height = area.height.saturating_sub(4).clamp(10, 26);
    let rect = centered(area, super::overlays::content_modal_width(area, 84), height);
    let footer = if picker.typing.is_some() {
        "Enter · Esc".to_string()
    } else {
        "↑↓ · Enter · ⌫ · / · . · Esc".to_string()
    };
    let inner = modal(app, rect, frame(theme, title, footer), buf);
    if inner.height < 6 {
        return;
    }
    let x = inner.x + 1;
    let width = inner.width.saturating_sub(2);
    let normal = button_style(app, ButtonKind::Normal, bg);

    // Row 0: up button, location bar (click to type), hidden-files toggle.
    // 第 0 行：上级按钮、地址栏（点击可输入）、隐藏文件开关。
    let y = inner.y;
    let mut cx = x;
    let up = picker.listing.as_ref().is_some_and(|l| l.parent.is_some());
    let up_style = if up {
        normal
    } else {
        normal.fg(theme.faint_fg(bg))
    };
    cx += button(app, buf, cx, y, "↑", up_style, UiAction::FolderUp) + 1;
    let hidden_label = format!(
        "{} {}",
        if picker.show_hidden { "☑" } else { "☐" },
        tr!("tui.folder.hidden")
    );
    let hidden_w = width_of(&hidden_label) as u16 + 2;
    let bar_w = (x + width).saturating_sub(cx + hidden_w + 1);
    let bar = Rect {
        x: cx,
        y,
        width: bar_w,
        height: 1,
    };
    buf.set_style(bar, Style::new().bg(mix(theme.input, bg, 0.5)));
    match &picker.typing {
        Some(typed) => {
            let shown = truncate_left(typed, bar_w.saturating_sub(3) as usize);
            Line::from(vec![
                Span::styled(format!(" {shown}"), Style::new().fg(theme.fg)),
                Span::styled("▏", theme.primary_text()),
            ])
            .render(bar, buf);
        }
        None => {
            let path = picker
                .listing
                .as_ref()
                .map(|l| l.path.clone())
                .unwrap_or_default();
            let shown = truncate_left(&path, bar_w.saturating_sub(4) as usize);
            Line::from(vec![
                Span::styled(" ⌂ ", theme.primary_text()),
                Span::styled(shown, Style::new().fg(theme.fg)),
            ])
            .render(bar, buf);
            app.hits.add(bar, Target::Do(UiAction::FolderType));
        }
    }
    button(
        app,
        buf,
        bar.right() + 1,
        y,
        &hidden_label,
        normal,
        UiAction::FolderToggleHidden,
    );

    // Row 1: filesystem roots as quick jumps.
    // 第 1 行：文件系统根，用于快速跳转。
    let y = inner.y + 1;
    let mut rx = x;
    if let Some(listing) = &picker.listing {
        buf.set_string(
            rx,
            y,
            format!("{} ", tr!("tui.folder.roots")),
            theme.muted_text(),
        );
        rx += width_of(&tr!("tui.folder.roots")) as u16 + 1;
        for root in &listing.roots {
            let w = width_of(root) as u16 + 2;
            if rx + w > x + width {
                break;
            }
            let style = if listing.path.eq_ignore_ascii_case(root) {
                button_style(app, ButtonKind::Focused, bg)
            } else {
                normal
            };
            rx += button(
                app,
                buf,
                rx,
                y,
                root,
                style,
                UiAction::FolderGo(root.clone()),
            ) + 1;
        }
    }
    buf.set_string(
        inner.x,
        inner.y + 2,
        "─".repeat(inner.width as usize),
        Style::new().fg(theme.border),
    );

    // Listing.
    // 目录列表。
    let list = Rect {
        x,
        y: inner.y + 3,
        width,
        height: inner.height.saturating_sub(5),
    };
    match &picker.listing {
        None => {
            buf.set_string(
                list.x + 1,
                list.y,
                format!("{} {}", spinner(app.phase()), tr!("tui.folder.loading")),
                theme.muted_text(),
            );
        }
        Some(listing) => {
            if let Some(error) = &listing.error {
                buf.set_string(
                    list.x + 1,
                    list.y,
                    truncate(error, width as usize - 2),
                    Style::new().fg(theme.danger),
                );
            } else if listing.entries.is_empty() {
                buf.set_string(
                    list.x + 1,
                    list.y,
                    tr!("tui.folder.empty"),
                    theme.muted_text(),
                );
            }
            let rows = list.height as usize;
            picker.cursor = picker.cursor.min(listing.entries.len().saturating_sub(1));
            if picker.cursor < picker.scroll {
                picker.scroll = picker.cursor;
            } else if rows > 0 && picker.cursor >= picker.scroll + rows {
                picker.scroll = picker.cursor + 1 - rows;
            }
            picker.scroll = picker
                .scroll
                .min(listing.entries.len().saturating_sub(rows));
            for (index, entry) in listing
                .entries
                .iter()
                .enumerate()
                .skip(picker.scroll)
                .take(rows)
            {
                let y = list.y + (index - picker.scroll) as u16;
                let selected = index == picker.cursor && picker.typing.is_none();
                let (icon, icon_style) = if entry.is_repo {
                    ("⎇ ", Style::new().fg(theme.teal))
                } else if entry.is_dir {
                    ("▸ ", theme.primary_text())
                } else {
                    ("· ", theme.muted_text())
                };
                let size = entry
                    .size
                    .filter(|_| !entry.is_dir)
                    .map(|s| format!("{}B ", compact_count(s)))
                    .unwrap_or_default();
                let name_width = (width as usize).saturating_sub(width_of(&size) + 4);
                let name = truncate(&entry.name, name_width);
                let pad = name_width.saturating_sub(width_of(&name));
                let name_style = if entry.hidden {
                    theme.muted_text()
                } else if entry.is_dir {
                    Style::new().fg(theme.fg).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(theme.fg)
                };
                let row = Rect {
                    x: list.x,
                    y,
                    width: list.width,
                    height: 1,
                };
                Line::from(vec![
                    Span::styled(if selected { "›" } else { " " }, theme.primary_text()),
                    Span::styled(icon, icon_style),
                    Span::styled(name, name_style),
                    Span::raw(" ".repeat(pad + 1)),
                    Span::styled(size, theme.muted_text()),
                ])
                .style(if selected {
                    Style::new().bg(theme.selection_bg())
                } else {
                    Style::new()
                })
                .render(row, buf);
                app.hits.add(row, Target::FolderEntry(index));
            }
        }
    }
    if picker.loading && picker.listing.is_some() {
        buf.set_string(
            inner.right().saturating_sub(3),
            inner.y + 1,
            spinner(app.phase()),
            theme.primary_text(),
        );
    }

    // Bottom buttons.
    // 底部按钮。
    let y = inner.bottom() - 1;
    let selected_is_dir = picker
        .listing
        .as_ref()
        .and_then(|l| l.entries.get(picker.cursor))
        .is_some_and(|e| e.is_dir);
    let mut items = vec![(
        tr!("tui.dialog.cancel"),
        ButtonKind::Normal,
        UiAction::Close,
    )];
    match picker.mode {
        FolderMode::Project | FolderMode::Plugin => {
            if selected_is_dir {
                items.push((
                    format!("▸ {}", tr!("tui.folder.open")),
                    ButtonKind::Normal,
                    UiAction::FolderChoose,
                ));
            }
            items.push((
                format!("✓ {}", tr!("tui.folder.useThis")),
                ButtonKind::Primary,
                UiAction::FolderUseCurrent,
            ));
        }
        FolderMode::Attachment => {
            let label = if selected_is_dir {
                format!("▸ {}", tr!("tui.folder.open"))
            } else {
                format!("⊕ {}", tr!("tui.folder.attach"))
            };
            items.push((label, ButtonKind::Primary, UiAction::FolderChoose));
        }
    }
    let left = button_row(app, buf, Rect { x, width, ..inner }, y, items);
    if picker.mode == FolderMode::Project {
        let hint = tr!("tui.folder.projectHint");
        buf.set_string(
            x,
            y,
            truncate(&hint, left.saturating_sub(x + 1) as usize),
            theme.muted_text(),
        );
    }
}

/// Detail sheet: an aligned field table, headed text sections and a row of action buttons.
/// 详情面板：对齐的字段表、带标题的文本段落与一排操作按钮。
pub fn render_detail(app: &App, sheet: &mut DetailSheet, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let width = super::overlays::content_modal_width(area, 76);
    let text_width = width.saturating_sub(6) as usize;
    let label_width = sheet
        .fields
        .iter()
        .map(|(label, _)| width_of(label))
        .max()
        .unwrap_or(0)
        .min(text_width / 3);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (label, value) in &sheet.fields {
        let value_lines = wrap(
            &[Span::styled(value.clone(), theme.base().bg(theme.popover))],
            text_width.saturating_sub(label_width + 2).max(1),
            &[],
            &[],
        );
        for (index, mut line) in value_lines.into_iter().enumerate() {
            let head = if index == 0 {
                fit(label, label_width)
            } else {
                " ".repeat(label_width)
            };
            line.spans
                .insert(0, Span::styled(format!("{head}  "), theme.muted_text()));
            lines.push(line);
        }
    }
    for (heading, body) in &sheet.sections {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(
            heading.clone(),
            theme.primary_text().add_modifier(Modifier::BOLD),
        )));
        for paragraph in body.lines() {
            if paragraph.is_empty() {
                lines.push(Line::default());
                continue;
            }
            lines.extend(wrap(
                &[Span::styled(
                    paragraph.to_string(),
                    theme.base().bg(theme.popover),
                )],
                text_width.saturating_sub(2).max(1),
                &[Span::raw("  ")],
                &[Span::raw("  ")],
            ));
        }
    }
    // Action buttons wrap onto as many bottom rows as they need, Close last.
    // 操作按钮按需换成多行放在底部，关闭按钮在最后。
    let mut buttons: Vec<(String, ButtonKind, UiAction)> = sheet
        .actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let kind = if sheet.focus == Some(index) {
                ButtonKind::Focused
            } else if action.danger {
                ButtonKind::Danger
            } else {
                ButtonKind::Normal
            };
            (action.label.clone(), kind, UiAction::SheetPick(index))
        })
        .collect();
    buttons.push((tr!("tui.dialog.close"), ButtonKind::Normal, UiAction::Close));
    let room = width.saturating_sub(4) as usize;
    let mut button_rows: Vec<Vec<(String, ButtonKind, UiAction)>> = vec![Vec::new()];
    let mut used = 0;
    for button in buttons {
        let w = width_of(&button.0) + 3;
        if used + w > room && used > 0 {
            button_rows.push(Vec::new());
            used = 0;
        }
        used += w;
        button_rows.last_mut().expect("one row").push(button);
    }
    let rows = button_rows.len() as u16;
    let rect = centered(area, width, lines.len() as u16 + 4 + rows);
    let inner = modal(
        app,
        rect,
        frame(
            theme,
            sheet.title.clone(),
            "↑↓ · Tab · Enter · Esc".to_string(),
        ),
        buf,
    );
    let body = Rect {
        x: inner.x + 2,
        y: inner.y + 1,
        width: inner.width.saturating_sub(4),
        height: inner.height.saturating_sub(2 + rows),
    };
    let height = body.height as usize;
    sheet.scroll = sheet.scroll.min(lines.len().saturating_sub(height));
    for (row, line) in lines.iter().skip(sheet.scroll).take(height).enumerate() {
        line.render(
            Rect {
                y: body.y + row as u16,
                height: 1,
                ..body
            },
            buf,
        );
    }
    if lines.len() > height && height > 0 {
        let more = format!("{}/{}", sheet.scroll + height, lines.len());
        buf.set_string(
            inner.right().saturating_sub(width_of(&more) as u16 + 1),
            inner.bottom().saturating_sub(rows + 1),
            more,
            Style::new().fg(theme.faint_fg(theme.popover)),
        );
    }
    for (row, buttons) in button_rows.into_iter().enumerate() {
        button_row(
            app,
            buf,
            Rect {
                x: inner.x + 1,
                width: inner.width.saturating_sub(2),
                ..inner
            },
            inner.bottom().saturating_sub(rows - row as u16),
            buttons,
        );
    }
}

fn wrap_text(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrap(
        &[Span::styled(text.to_string(), style)],
        width.max(1),
        &[],
        &[],
    )
}

/// Keeps the tail of a path when it does not fit, prefixed with `…`.
/// 路径放不下时保留尾部，并以 `…` 开头。
fn truncate_left(text: &str, width: usize) -> String {
    if width_of(text) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut start = chars.len();
    let mut used = 1;
    while start > 0 {
        let w = width_of(&chars[start - 1].to_string());
        if used + w > width {
            break;
        }
        used += w;
        start -= 1;
    }
    format!("…{}", chars[start..].iter().collect::<String>())
}
