// Modal overlays: pickers (palette, model, language, branch, area, choice), diff viewer, commit
// dialog and help. Menus, confirm/prompt dialogs and the folder picker live in `dialogs`; the
// history browser and inspector in `history`.
// 模态浮层：选择器（命令面板、模型、语言、分支、区域、选项）、diff 查看器、提交对话框与帮助。
// 菜单、确认/输入对话框与文件夹选择器位于 `dialogs`；历史浏览器与检视器位于 `history`。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use super::chrome::{ButtonKind, button, button_style};
use super::text::{spinner, truncate, width_of};
use super::theme::{Theme, mix};
use super::{dialogs, history};
use crate::app::App;
use crate::app::action::UiAction;
use crate::app::commands::AppCommand;
use crate::app::hit::{ScrollTarget, Target};
use crate::app::overlay::{
    CommitDialog, CommitField, DiffView, Overlay, Picker, PickerAction, PickerKind,
};
use crate::backend::GitBusy;
use crate::model::*;
use crate::tr;

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    // Dim the whole screen behind the modal.
    // 将模态框后面的整个屏幕调暗。
    let theme = app.theme;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                if let ratatui::style::Color::Rgb(..) = cell.fg {
                    cell.fg = mix(cell.fg, theme.bg, 0.45);
                }
                if let ratatui::style::Color::Rgb(..) = cell.bg {
                    cell.bg = mix(cell.bg, theme.bg, 0.45);
                }
            }
        }
    }
    // Taken out while drawing so renderers can read `app` and mutate the overlay (scroll clamps,
    // TextArea styling) at the same time.
    // 绘制期间取出浮层，使渲染器可以同时读取 `app` 并修改浮层（滚动钳制、TextArea 样式）。
    let Some(mut overlay) = app.overlay.take() else {
        return;
    };
    match &mut overlay {
        Overlay::Picker(picker) => render_picker(app, picker, area, buf),
        Overlay::Diff(view) => render_diff(app, view, area, buf),
        Overlay::Help { scroll } => render_help(app, scroll, area, buf),
        Overlay::Commit(dialog) => render_commit(app, dialog, area, buf),
        Overlay::Menu(menu) => dialogs::render_menu(app, menu, area, buf),
        Overlay::Confirm(dialog) => dialogs::render_confirm(app, dialog, area, buf),
        Overlay::Prompt(dialog) => dialogs::render_prompt(app, dialog, area, buf),
        Overlay::Folder(picker) => dialogs::render_folder(app, picker, area, buf),
        Overlay::History(state) => history::render_history(app, state, area, buf),
        Overlay::Inspector(inspector) => history::render_inspector(app, inspector, area, buf),
        Overlay::Detail(sheet) => dialogs::render_detail(app, sheet, area, buf),
    }
    app.overlay = Some(overlay);
}

/// Width of a modal that lists or edits content: its preferred width, growing to half of a wide
/// terminal so long names and values are not cut.
/// 列出或编辑内容的模态宽度：取其首选宽度，并在宽终端上增长到一半，避免长名称与值被截断。
/// `area` is the screen and `preferred` the width on a normal terminal; returns the width.
/// `area` 为屏幕，`preferred` 为常规终端上的宽度；返回宽度。
pub(super) fn content_modal_width(area: Rect, preferred: u16) -> u16 {
    preferred
        .max(area.width / 2)
        .min(area.width.saturating_sub(2))
}

pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 3,
        width,
        height,
    }
}

pub(super) fn frame<'a>(theme: &Theme, title: String, footer: String) -> Block<'a> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.primary))
        .style(Style::new().bg(theme.popover).fg(theme.fg))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            theme.strong(),
        )))
        .title_bottom(
            Line::from(Span::styled(format!(" {footer} "), theme.muted_text())).right_aligned(),
        )
}

/// Clears `rect`, draws the frame, and registers the modal's inert body and wheel area.
/// 清空 `rect`、绘制边框，并登记模态框的惰性主体与滚轮区域。
pub(super) fn modal(app: &App, rect: Rect, block: Block<'_>, buf: &mut Buffer) -> Rect {
    super::text::clear_area(rect, buf);
    let inner = block.inner(rect);
    block.render(rect, buf);
    app.hits.add(rect, Target::Inert);
    app.hits.add_scroll(rect, ScrollTarget::Overlay);
    // A close button on the top border.
    // 上边框上的关闭按钮。
    if rect.width > 8 {
        let x = rect.right() - 5;
        buf.set_string(
            x,
            rect.y,
            " ✕ ",
            Style::new().fg(app.theme.muted_fg).bg(app.theme.popover),
        );
        app.hits.add(
            Rect {
                x,
                y: rect.y,
                width: 3,
                height: 1,
            },
            Target::Do(UiAction::Close),
        );
    }
    inner
}

/// A row of buttons right-aligned at `y` inside `area`; returns the leftmost x used.
/// 在 `area` 内 `y` 行右对齐绘制一排按钮；返回使用到的最左 x。
pub(super) fn button_row(
    app: &App,
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    items: Vec<(String, ButtonKind, UiAction)>,
) -> u16 {
    let bg = app.theme.popover;
    let mut x = area.right();
    for (label, kind, action) in items.into_iter().rev() {
        let w = width_of(&label) as u16 + 2;
        if x < area.x + w {
            break;
        }
        x -= w;
        button(app, buf, x, y, &label, button_style(app, kind, bg), action);
        x = x.saturating_sub(1);
    }
    x
}

fn render_picker(app: &App, picker: &mut Picker, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let visible_len = picker.visible().len();
    let rows = (visible_len.max(1) as u16).min(16);
    let rect = centered(area, content_modal_width(area, 72), rows + 4);
    let inner = modal(
        app,
        rect,
        frame(theme, picker.title.clone(), "↑↓ · Enter · Esc".to_string()),
        buf,
    );

    let filter = if picker.filter.is_empty() {
        Line::from(vec![
            Span::styled("⌕ ", theme.primary_text()),
            Span::styled(
                picker.placeholder.clone(),
                Style::new().fg(theme.faint_fg(theme.popover)),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("⌕ ", theme.primary_text()),
            Span::styled(
                format!("{}▏", picker.filter),
                theme.base().bg(theme.popover),
            ),
        ])
    };
    filter.render(
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        buf,
    );
    buf.set_string(
        inner.x,
        inner.y + 1,
        "─".repeat(inner.width as usize),
        Style::new().fg(theme.border),
    );

    let visible = picker.visible();
    let list_height = inner.height.saturating_sub(2) as usize;
    if visible.is_empty()
        || (picker.kind == PickerKind::Branch && visible.len() == 1 && app.git.branches.is_empty())
    {
        let empty = match picker.kind {
            PickerKind::Model => tr!("composer.modelPicker.noMatches"),
            PickerKind::Branch if app.git.branches.is_empty() => {
                format!("{} {}", spinner(app.phase()), tr!("gitBranchMenu.loading"))
            }
            PickerKind::Branch => tr!("gitBranchMenu.empty"),
            _ => tr!("diff.noMatchingFiles"),
        };
        buf.set_string(
            inner.x + 2,
            inner.y + 2,
            truncate(&empty, (inner.width as usize).saturating_sub(3)),
            theme.muted_text(),
        );
        if visible.is_empty() {
            return;
        }
    }
    let offset = picker.cursor.saturating_sub(list_height.saturating_sub(1));
    for (row, item) in visible.iter().enumerate().skip(offset).take(list_height) {
        let y = inner.y + 2 + (row - offset) as u16;
        let selected = row == picker.cursor;
        let marker = if item.current { "● " } else { "  " };
        let label_style = match item.action {
            PickerAction::CreateBranch => theme.primary_text(),
            _ if item.current => theme.strong(),
            _ => theme.base().bg(theme.popover),
        };
        let hint_width = width_of(&item.hint) as u16;
        let text_width = inner.width.saturating_sub(hint_width + 5) as usize;
        let label = truncate(&item.label, text_width);
        let detail_room = text_width.saturating_sub(width_of(&label) + 2);
        let mut spans = vec![
            Span::styled(if selected { "›" } else { " " }, theme.primary_text()),
            Span::styled(marker, theme.primary_text()),
            Span::styled(label, label_style),
        ];
        if detail_room > 3 && !item.detail.is_empty() {
            spans.push(Span::styled(
                format!("  {}", truncate(&item.detail, detail_room)),
                theme.muted_text(),
            ));
        }
        let line = Line::from(spans).style(if selected {
            Style::new().bg(theme.selection_bg())
        } else {
            Style::new()
        });
        let rect = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };
        line.render(rect, buf);
        app.hits.add(rect, Target::PickerItem(row));
        if hint_width > 0 {
            buf.set_string(
                inner.right() - hint_width - 1,
                y,
                &item.hint,
                theme.muted_text(),
            );
        }
    }
}

fn render_diff(app: &App, view: &mut DiffView, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let rect = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };
    let files = app.git.ordered_files();
    let Some(file) = files.get(view.file_index) else {
        modal(
            app,
            rect,
            frame(theme, tr!("diff.noMatchingFiles"), "Esc".to_string()),
            buf,
        );
        return;
    };
    let badge = if file.staged {
        tr!("diff.mode.staged")
    } else {
        tr!("diff.mode.unstaged")
    };
    let title = format!(
        "{}  ({}/{}) · {badge}",
        file.path,
        view.file_index + 1,
        files.len()
    );
    let footer = "j/k · PgUp/PgDn · Tab/n · [/] · s · c · Esc".to_string();
    let inner = modal(app, rect, frame(theme, title, footer), buf);
    if inner.height < 3 {
        return;
    }

    // Button bar on the last inner row.
    // 最后一行为按钮栏。
    let bar_y = inner.bottom() - 1;
    let stage_label = if file.staged {
        tr!("tui.panel.unstage")
    } else {
        tr!("tui.panel.stage")
    };
    let mut x = inner.x + 1;
    let normal = button_style(app, ButtonKind::Normal, theme.popover);
    for (label, action) in [
        (
            format!("‹ {}", tr!("tui.diff.prevFile")),
            UiAction::DiffStep(-1),
        ),
        (
            format!("{} ›", tr!("tui.diff.nextFile")),
            UiAction::DiffStep(1),
        ),
        (
            format!("s {stage_label}"),
            UiAction::ToggleStage(view.file_index),
        ),
    ] {
        x += button(app, buf, x, bar_y, &label, normal, action) + 1;
    }
    button_row(
        app,
        buf,
        Rect {
            x,
            width: inner.right().saturating_sub(x + 1),
            ..inner
        },
        bar_y,
        vec![
            (
                format!("✓ {}", tr!("gitCommitDialog.open")),
                ButtonKind::Primary,
                UiAction::App(AppCommand::Commit),
            ),
            (tr!("tui.dialog.close"), ButtonKind::Normal, UiAction::Close),
        ],
    );
    let body = Rect {
        height: inner.height - 2,
        ..inner
    };

    // The displayed row owns both the repository and staged/unstaged cache identity.
    // 显示行同时拥有仓库身份及暂存／未暂存缓存身份。
    let Some(key) = app.git.diff_key(file) else {
        return;
    };
    if let Some(reason) = app.git.diff_failures.get(&key) {
        let mut y = body.y + 1;
        buf.set_string(
            body.x + 2,
            y,
            format!("✕ {}", tr!("diff.load.failed")),
            Style::new().fg(theme.danger),
        );
        if let Some(reason) = reason {
            y += 1;
            buf.set_string(
                body.x + 4,
                y,
                truncate(reason, body.width.saturating_sub(6) as usize),
                theme.muted_text(),
            );
        }
        let retry = format!("r {}", tr!("diff.load.retry"));
        let normal = button_style(app, ButtonKind::Normal, theme.popover);
        button(
            app,
            buf,
            body.x + 2,
            y + 2,
            &retry,
            normal,
            UiAction::DiffStep(0),
        );
        return;
    }
    let Some(diff) = app.git.diffs.get(&key) else {
        buf.set_string(
            body.x + 2,
            body.y + 1,
            format!(
                "{} {}",
                spinner(app.phase()),
                tr!("diff.load.loadingDetail")
            ),
            theme.muted_text(),
        );
        return;
    };
    let mut lines: Vec<Line> = Vec::new();
    let gutter_style = Style::new().fg(theme.faint_fg(theme.popover));
    for row in diff.view_rows() {
        let line = match row {
            DiffViewRow::Header(header) => {
                lines.push(
                    Line::from(Span::styled(
                        header.to_string(),
                        Style::new().fg(theme.info),
                    ))
                    .style(Style::new().bg(mix(
                        theme.info,
                        theme.popover,
                        0.08,
                    ))),
                );
                continue;
            }
            DiffViewRow::Gap => {
                lines.push(Line::default());
                continue;
            }
            DiffViewRow::Line(line) => line,
        };
        let number = |n: Option<u32>| {
            n.map(|n| format!("{n:>4}"))
                .unwrap_or_else(|| "    ".into())
        };
        let (sign, bg, fg) = match line.kind {
            DiffLineKind::Add => ("+", theme.diff_add_bg(), theme.success),
            DiffLineKind::Delete => ("-", theme.diff_del_bg(), theme.danger),
            DiffLineKind::Context => (" ", theme.popover, theme.muted_fg),
        };
        let text_style = if line.kind == DiffLineKind::Context {
            theme.base().bg(theme.popover)
        } else {
            Style::new().fg(theme.fg)
        };
        lines.push(
            Line::from(vec![
                Span::styled(
                    format!("{} {} ", number(line.old_no), number(line.new_no)),
                    gutter_style,
                ),
                Span::styled(
                    format!("{sign} "),
                    Style::new().fg(fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(line.text.replace('\t', "    "), text_style),
            ])
            .style(Style::new().bg(bg)),
        );
    }
    if diff.hunks.is_empty() {
        lines.push(Line::from(Span::styled(
            tr!("diff.file.noContent"),
            theme.muted_text(),
        )));
    }
    let height = body.height as usize;
    view.max_scroll = lines.len().saturating_sub(height);
    view.scroll = view.scroll.min(view.max_scroll);
    for (row, line) in lines.iter().skip(view.scroll).take(height).enumerate() {
        line.render(
            Rect {
                x: body.x,
                y: body.y + row as u16,
                width: body.width,
                height: 1,
            },
            buf,
        );
    }
}

fn render_commit(app: &App, dialog: &mut CommitDialog, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let git = &app.git;
    let rect = centered(area, 76, if dialog.new_branch { 22 } else { 20 });
    let footer = "Ctrl+S · Alt+S · Ctrl+G ✦ · Tab · Esc".to_string();
    let inner = modal(
        app,
        rect,
        frame(theme, tr!("gitCommitDialog.title"), footer),
        buf,
    );
    let x = inner.x + 2;
    let width = inner.width.saturating_sub(4);
    let mut y = inner.y;

    let branch = git
        .snapshot
        .as_ref()
        .map(|s| s.branch.clone())
        .unwrap_or_default();
    let label = |field: CommitField, text: String| {
        let style = if dialog.field == field {
            theme.primary_text().add_modifier(Modifier::BOLD)
        } else {
            theme.muted_text()
        };
        Span::styled(text, style)
    };
    let radio = |on: bool| if on { "(•) " } else { "( ) " };
    let row = Rect {
        x,
        y,
        width,
        height: 1,
    };
    Line::from(vec![
        label(
            CommitField::Target,
            format!("{}  ", tr!("gitCommitDialog.commitTo")),
        ),
        Span::styled(radio(!dialog.new_branch), theme.primary_text()),
        Span::styled(
            format!("{} ", tr!("gitCommitDialog.currentBranch")),
            theme.base().bg(theme.popover),
        ),
        Span::styled(format!("⎇ {branch}   "), Style::new().fg(theme.teal)),
        Span::styled(radio(dialog.new_branch), theme.primary_text()),
        Span::styled(
            tr!("gitCommitDialog.newBranch"),
            theme.base().bg(theme.popover),
        ),
    ])
    .render(row, buf);
    app.hits.add(row, Target::CommitField(CommitField::Target));
    y += 2;

    if dialog.new_branch {
        let editing = dialog.field == CommitField::BranchName;
        let value = if dialog.branch_name.is_empty() && !editing {
            Span::styled(
                tr!("gitBranchCreate.placeholder", prefix = "feat/"),
                Style::new().fg(theme.faint_fg(theme.popover)),
            )
        } else {
            Span::styled(
                format!("{}{}", dialog.branch_name, if editing { "▏" } else { "" }),
                theme.base().bg(theme.popover),
            )
        };
        let row = Rect {
            x,
            y,
            width,
            height: 1,
        };
        Line::from(vec![
            label(
                CommitField::BranchName,
                format!("{}  ", tr!("gitCommitDialog.branchName")),
            ),
            value,
        ])
        .render(row, buf);
        app.hits
            .add(row, Target::CommitField(CommitField::BranchName));
        y += 2;
    }

    let message_label = tr!("gitCommitDialog.message");
    Line::from(label(CommitField::Message, message_label.clone())).render(
        Rect {
            x,
            y,
            width,
            height: 1,
        },
        buf,
    );
    // Generate button beside the message label.
    // 信息标签旁的生成按钮。
    let generate = format!("✦ {}", tr!("tui.commit.generate"));
    let gx = x + width.saturating_sub(width_of(&generate) as u16 + 2);
    if gx > x + width_of(&message_label) as u16 + 1 {
        button(
            app,
            buf,
            gx,
            y,
            &generate,
            button_style(app, ButtonKind::Normal, theme.popover),
            UiAction::CommitGenerate,
        );
    }
    y += 1;
    let focused = dialog.field == CommitField::Message;
    let message_block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { theme.primary } else { theme.input }));
    dialog.message.set_block(message_block);
    dialog
        .message
        .set_style(Style::new().fg(theme.fg).bg(theme.popover));
    dialog.message.set_cursor_line_style(Style::new());
    dialog
        .message
        .set_placeholder_style(Style::new().fg(theme.faint_fg(theme.popover)));
    dialog.message.set_cursor_style(if focused {
        Style::new().bg(theme.primary).fg(theme.popover)
    } else {
        Style::new().bg(theme.popover)
    });
    let message_area = Rect {
        x,
        y,
        width,
        height: 7,
    };
    (&dialog.message).render(message_area, buf);
    app.hits
        .add(message_area, Target::CommitField(CommitField::Message));
    y += 8;

    let check = if dialog.include_unstaged {
        "[x] "
    } else {
        "[ ] "
    };
    let row = Rect {
        x,
        y,
        width,
        height: 1,
    };
    Line::from(vec![
        Span::styled(check, theme.primary_text()),
        label(
            CommitField::IncludeUnstaged,
            tr!("gitCommitDialog.includeUnstaged"),
        ),
    ])
    .render(row, buf);
    app.hits
        .add(row, Target::CommitField(CommitField::IncludeUnstaged));
    y += 1;

    if let Some(snapshot) = &git.snapshot {
        let staged = snapshot.files.iter().filter(|f| f.staged).count();
        let included = if dialog.include_unstaged {
            snapshot.files.len()
        } else {
            staged
        };
        let adds: u32 = snapshot
            .files
            .iter()
            .filter(|f| dialog.include_unstaged || f.staged)
            .map(|f| f.additions)
            .sum();
        let dels: u32 = snapshot
            .files
            .iter()
            .filter(|f| dialog.include_unstaged || f.staged)
            .map(|f| f.deletions)
            .sum();
        Line::from(vec![
            Span::styled(
                tr!("composer.progress.changedFiles", count = included),
                theme.muted_text(),
            ),
            Span::styled(format!("  +{adds}"), Style::new().fg(theme.success)),
            Span::styled(format!(" -{dels}"), Style::new().fg(theme.danger)),
        ])
        .render(
            Rect {
                x,
                y,
                width,
                height: 1,
            },
            buf,
        );
    }
    y += 2;
    if let Some(busy) = git.busy {
        let text = match busy {
            GitBusy::Generating => tr!("gitCommitDialog.generatingCommitMessage"),
            GitBusy::Committing => tr!("gitCommitDialog.committing"),
            GitBusy::Pushing => tr!("gitCommitDialog.pushingDetailed"),
            GitBusy::Pulling => tr!("gitCommitDialog.pulling"),
        };
        buf.set_string(
            x,
            y,
            format!("{} {text}", spinner(app.phase())),
            theme.primary_text(),
        );
    } else if let Some(error) = &dialog.error {
        buf.set_string(
            x,
            y,
            truncate(error, width as usize),
            Style::new().fg(theme.danger),
        );
    }

    // Buttons on the last inner row.
    // 最后一行为按钮。
    if inner.height > 1 {
        button_row(
            app,
            buf,
            Rect { x, width, ..inner },
            inner.bottom() - 1,
            vec![
                (
                    tr!("tui.dialog.cancel"),
                    ButtonKind::Normal,
                    UiAction::Close,
                ),
                (
                    tr!("gitCommitDialog.commit"),
                    ButtonKind::Normal,
                    UiAction::CommitSubmit(false),
                ),
                (
                    tr!("gitCommitDialog.commitAndPush"),
                    ButtonKind::Primary,
                    UiAction::CommitSubmit(true),
                ),
            ],
        );
    }
}

fn render_help(app: &App, scroll: &mut usize, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let mut lines: Vec<(String, String)> = AppCommand::ALL
        .iter()
        .map(|c| {
            let key = if c.shortcut().is_empty() {
                format!("/{}", c.slash())
            } else {
                format!("{:<10} /{}", c.shortcut(), c.slash())
            };
            (key, c.label())
        })
        .collect();
    lines.push(("Ctrl+P".into(), tr!("tui.palette.title")));
    lines.push(("Ctrl+F".into(), tr!("tui.history.title")));
    lines.push(("F2".into(), tr!("tui.area.switch")));
    lines.push(("/ · n · N".into(), tr!("conversation.find.label")));
    lines.push(("/".into(), tr!("sidebar.searchTree")));
    lines.push(("Tab".into(), tr!("tui.help.focus")));
    lines.push(("Alt+1..6".into(), tr!("workspaceContext.switchTabs")));
    lines.push(("Ctrl+C".into(), tr!("tui.help.interrupt")));
    lines.push((tr!("tui.help.mouseKey"), tr!("tui.help.mouse")));
    let rect = centered(area, 76, lines.len() as u16 + 4);
    let inner = modal(
        app,
        rect,
        frame(theme, tr!("tui.help.title"), "↑↓ · Esc".to_string()),
        buf,
    );
    let height = inner.height.saturating_sub(1) as usize;
    *scroll = (*scroll).min(lines.len().saturating_sub(height));
    for (row, (key, label)) in lines.iter().skip(*scroll).take(height).enumerate() {
        let y = inner.y + 1 + row as u16;
        buf.set_string(
            inner.x + 2,
            y,
            truncate(&format!("{key:<20}"), 21),
            theme.primary_text(),
        );
        buf.set_string(
            inner.x + 24,
            y,
            truncate(label, inner.width.saturating_sub(26) as usize),
            theme.base().bg(theme.popover),
        );
    }
}
