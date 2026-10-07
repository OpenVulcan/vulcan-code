// Keys of modal overlays. An open overlay receives every key except Ctrl+C / Ctrl+Q.
// 模态浮层的按键。浮层打开时接收除 Ctrl+C / Ctrl+Q 以外的所有按键。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    alt, ctrl, diff_step, generate_commit_message, open_commit, submit_commit, toggle_stage, typed,
};
use crate::app::action::{self, UiAction};
use crate::app::overlay::{CommitField, HistoryFilter, Overlay};
use crate::app::{App, history};

pub(super) fn on_key(app: &mut App, key: KeyEvent) {
    let Some(overlay) = app.overlay.as_mut() else {
        return;
    };
    match overlay {
        Overlay::Help { scroll } => match key.code {
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *scroll += 1,
            KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
            KeyCode::PageDown => *scroll += 10,
            KeyCode::Home => *scroll = 0,
            _ => app.overlay = None,
        },
        Overlay::Picker(picker) => match key.code {
            KeyCode::Esc => app.overlay = None,
            KeyCode::Up => picker.cursor = picker.cursor.saturating_sub(1),
            KeyCode::Down => {
                picker.cursor += 1;
                picker.clamp();
            }
            KeyCode::PageUp => picker.cursor = picker.cursor.saturating_sub(8),
            KeyCode::PageDown => {
                picker.cursor += 8;
                picker.clamp();
            }
            KeyCode::Backspace => {
                picker.filter.pop();
                picker.clamp();
            }
            KeyCode::Enter => confirm_picker(app),
            _ => {
                if let Some(ch) = typed(&key) {
                    picker.filter.push(ch);
                    picker.cursor = 0;
                    picker.clamp();
                }
            }
        },
        Overlay::Menu(menu) => match key.code {
            KeyCode::Esc | KeyCode::Left => app.overlay = None,
            KeyCode::Up | KeyCode::Char('k') => menu.cursor = menu.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                menu.cursor = (menu.cursor + 1).min(menu.items.len().saturating_sub(1));
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => {
                let index = menu.cursor;
                menu_pick(app, index);
            }
            KeyCode::Delete => {
                if let Some(index) = menu.items.iter().position(|i| i.hint == "Del") {
                    menu_pick(app, index);
                }
            }
            _ => {
                if let Some(ch) = typed(&key)
                    && let Some(index) = menu.items.iter().position(|i| i.hint == ch.to_string())
                {
                    menu_pick(app, index);
                }
            }
        },
        Overlay::Confirm(dialog) => match key.code {
            KeyCode::Esc | KeyCode::Char('n') => app.overlay = None,
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                dialog.focus_confirm = !dialog.focus_confirm;
            }
            KeyCode::Char('y') => action::perform(app, UiAction::ConfirmAccept),
            KeyCode::Enter => {
                if dialog.focus_confirm {
                    action::perform(app, UiAction::ConfirmAccept);
                } else {
                    app.overlay = None;
                }
            }
            _ => {}
        },
        // Detail sheet: arrows scroll, Tab cycles the action buttons, Enter runs the focused one.
        // 详情面板：方向键滚动，Tab 在操作按钮间切换，Enter 执行聚焦的按钮。
        Overlay::Detail(sheet) => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.overlay = None,
            KeyCode::Up | KeyCode::Char('k') => sheet.scroll = sheet.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => sheet.scroll += 1,
            KeyCode::PageUp => sheet.scroll = sheet.scroll.saturating_sub(10),
            KeyCode::PageDown => sheet.scroll += 10,
            KeyCode::Home => sheet.scroll = 0,
            KeyCode::Tab | KeyCode::Right | KeyCode::BackTab | KeyCode::Left => {
                let count = sheet.actions.len();
                if count > 0 {
                    let back = matches!(key.code, KeyCode::BackTab | KeyCode::Left);
                    sheet.focus = Some(match (sheet.focus, back) {
                        (None, false) => 0,
                        (None, true) => count - 1,
                        (Some(i), false) => (i + 1) % count,
                        (Some(i), true) => (i + count - 1) % count,
                    });
                }
            }
            KeyCode::Enter => match sheet.focus {
                Some(index) => action::perform(app, UiAction::SheetPick(index)),
                None => app.overlay = None,
            },
            _ => {}
        },
        Overlay::Prompt(dialog) => match key.code {
            KeyCode::Esc => app.overlay = None,
            // Modified Enter adds a real newline; plain Enter keeps the shared submit action.
            // 带修饰键的 Enter 添加真实换行；普通 Enter 保持统一提交动作。
            KeyCode::Enter
                if dialog.accepts_multiline()
                    && key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
            {
                dialog.input.push('\n');
                dialog.error = None;
            }
            KeyCode::Enter => action::perform(app, UiAction::PromptAccept),
            KeyCode::Backspace => {
                dialog.input.pop();
                dialog.error = None;
            }
            _ if ctrl(&key, 'u') => {
                dialog.input.clear();
                dialog.error = None;
            }
            _ => {
                if let Some(ch) = typed(&key) {
                    dialog.input.push(ch);
                    dialog.error = None;
                }
            }
        },
        Overlay::Folder(picker) => {
            if let Some(typing) = &mut picker.typing {
                match key.code {
                    KeyCode::Esc => picker.typing = None,
                    KeyCode::Enter => action::perform(app, UiAction::FolderChoose),
                    KeyCode::Backspace => {
                        typing.pop();
                    }
                    _ if ctrl(&key, 'u') => typing.clear(),
                    _ => {
                        if let Some(ch) = typed(&key) {
                            typing.push(ch);
                        }
                    }
                }
                return;
            }
            let count = picker.listing.as_ref().map_or(0, |l| l.entries.len());
            match key.code {
                KeyCode::Esc => app.overlay = None,
                KeyCode::Up | KeyCode::Char('k') => picker.cursor = picker.cursor.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    picker.cursor = (picker.cursor + 1).min(count.saturating_sub(1));
                }
                KeyCode::PageUp => picker.cursor = picker.cursor.saturating_sub(10),
                KeyCode::PageDown => {
                    picker.cursor = (picker.cursor + 10).min(count.saturating_sub(1))
                }
                KeyCode::Home => picker.cursor = 0,
                KeyCode::End => picker.cursor = count.saturating_sub(1),
                KeyCode::Enter | KeyCode::Right => action::perform(app, UiAction::FolderChoose),
                KeyCode::Backspace | KeyCode::Left => action::perform(app, UiAction::FolderUp),
                KeyCode::Char('.') => action::perform(app, UiAction::FolderToggleHidden),
                KeyCode::Char('/') | KeyCode::Char('~') => {
                    action::perform(app, UiAction::FolderType)
                }
                KeyCode::Char('s') if picker.mode.picks_directory() => {
                    action::folder_use_current(app)
                }
                _ if ctrl(&key, 'l') => action::perform(app, UiAction::FolderType),
                _ => {}
            }
        }
        Overlay::History(state) => {
            // Copy the filter first: the session lookup borrows the whole app.
            // 先复制过滤条件：查询会话需要借用整个 app。
            let (filter, query, cursor) = (state.filter, state.query.clone(), state.cursor);
            let entries = app
                .session()
                .map(|s| history::visible(&s.events, filter, &query))
                .unwrap_or_default();
            let last = entries.len().saturating_sub(1);
            let cursor = cursor.min(last);
            let Some(Overlay::History(state)) = app.overlay.as_mut() else {
                return;
            };
            let filters = HistoryFilter::ALL;
            let filter_index = filters.iter().position(|f| *f == state.filter).unwrap_or(0);
            match key.code {
                KeyCode::Esc if !state.query.is_empty() => {
                    state.query.clear();
                    state.cursor = usize::MAX;
                }
                KeyCode::Esc => app.overlay = None,
                KeyCode::Up => state.cursor = cursor.saturating_sub(1),
                KeyCode::Down => state.cursor = (cursor + 1).min(last),
                KeyCode::PageUp => state.cursor = cursor.saturating_sub(10),
                KeyCode::PageDown => state.cursor = (cursor + 10).min(last),
                KeyCode::Home => state.cursor = 0,
                KeyCode::End => state.cursor = last,
                KeyCode::Tab | KeyCode::Right => {
                    state.filter = filters[(filter_index + 1) % filters.len()];
                    state.cursor = usize::MAX;
                }
                KeyCode::BackTab | KeyCode::Left => {
                    state.filter = filters[(filter_index + filters.len() - 1) % filters.len()];
                    state.cursor = usize::MAX;
                }
                KeyCode::Backspace => {
                    state.query.pop();
                    state.cursor = usize::MAX;
                }
                KeyCode::Enter => {
                    state.cursor = cursor;
                    if let Some(entry) = entries.get(cursor) {
                        action::perform(app, UiAction::Inspect(entry.seq));
                    }
                }
                _ => {
                    if let Some(ch) = typed(&key) {
                        state.query.push(ch);
                        state.cursor = usize::MAX;
                    }
                }
            }
        }
        Overlay::Inspector(inspector) => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                inspector.scroll = inspector.scroll.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => inspector.scroll += 1,
            KeyCode::PageUp => inspector.scroll = inspector.scroll.saturating_sub(15),
            KeyCode::PageDown | KeyCode::Char(' ') => inspector.scroll += 15,
            KeyCode::Home | KeyCode::Char('g') => inspector.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => inspector.scroll = usize::MAX / 2,
            KeyCode::Char('[') | KeyCode::Char(']') => {
                let forward = key.code == KeyCode::Char(']');
                let seq = inspector.seq;
                let entries = app
                    .session()
                    .map(|s| history::entries(&s.events))
                    .unwrap_or_default();
                let at = entries.iter().position(|e| e.seq == seq);
                let next = match (at, forward) {
                    (Some(i), true) => entries.get(i + 1),
                    (Some(i), false) => i.checked_sub(1).and_then(|i| entries.get(i)),
                    (None, _) => None,
                };
                if let (Some(next), Some(Overlay::Inspector(inspector))) =
                    (next, app.overlay.as_mut())
                {
                    inspector.seq = next.seq;
                    inspector.scroll = 0;
                }
            }
            _ => app.close_overlay(),
        },
        Overlay::Diff(view) => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.overlay = None,
            KeyCode::Up | KeyCode::Char('k') => view.scroll = view.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => view.scroll += 1,
            KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(15),
            KeyCode::PageDown => view.scroll += 15,
            KeyCode::Home => view.scroll = 0,
            KeyCode::Tab | KeyCode::Char('n') | KeyCode::Right => diff_step(app, 1),
            KeyCode::BackTab | KeyCode::Char('N') | KeyCode::Left => diff_step(app, -1),
            KeyCode::Char('s') | KeyCode::Char(' ') => {
                let index = view.file_index;
                toggle_stage(app, index);
            }
            KeyCode::Char('c') => open_commit(app),
            KeyCode::Char('r') => diff_step(app, 0),
            _ => {}
        },
        Overlay::Commit(dialog) => {
            if key.code == KeyCode::Esc {
                app.overlay = None;
                return;
            }
            if ctrl(&key, 's') {
                submit_commit(app, false);
                return;
            }
            if alt(&key, 's') {
                submit_commit(app, true);
                return;
            }
            if ctrl(&key, 'g') {
                generate_commit_message(app);
                return;
            }
            if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                dialog.field = dialog
                    .field
                    .next(dialog.new_branch, key.code == KeyCode::BackTab);
                return;
            }
            match dialog.field {
                CommitField::Target => {
                    if matches!(
                        key.code,
                        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') | KeyCode::Enter
                    ) {
                        dialog.new_branch = !dialog.new_branch;
                        dialog.error = None;
                    }
                }
                CommitField::BranchName => match key.code {
                    KeyCode::Backspace => {
                        dialog.branch_name.pop();
                    }
                    KeyCode::Char(ch) if !ch.is_whitespace() => {
                        dialog.branch_name.push(ch);
                        dialog.error = None;
                    }
                    _ => {}
                },
                CommitField::Message => {
                    dialog.message.input(key);
                }
                CommitField::IncludeUnstaged => {
                    if matches!(key.code, KeyCode::Char(' ') | KeyCode::Enter) {
                        dialog.include_unstaged = !dialog.include_unstaged;
                    }
                }
            }
        }
    }
}

/// Confirms the picker row under the cursor.
/// 确认光标所在的选择器条目。
pub(super) fn confirm_picker(app: &mut App) {
    let Some(Overlay::Picker(picker)) = app.overlay.take() else {
        return;
    };
    if let Some(item) = picker.selected() {
        action::confirm_picker(
            app,
            picker.kind,
            item.action,
            picker.filter.trim().to_string(),
        );
    }
}

/// Runs one menu entry; the menu closes first so the entry may open its own dialog.
/// 执行一个菜单项；菜单先关闭，使该项可以打开自己的对话框。
pub(super) fn menu_pick(app: &mut App, index: usize) {
    let Some(Overlay::Menu(menu)) = app.overlay.take() else {
        return;
    };
    if let Some(item) = menu.items.into_iter().nth(index) {
        action::perform(app, item.action);
    }
}
