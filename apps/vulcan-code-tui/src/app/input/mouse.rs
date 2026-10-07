// Mouse input: every click and wheel event resolves against the hit map of the last frame and
// ends in the same functions the keyboard uses.
// 鼠标输入：每次点击与滚轮事件都根据上一帧的命中表解析，最终调用与键盘相同的函数。

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::overlay::{confirm_picker, menu_pick};
use super::regions::{
    activate_sidebar_row, ask_click_option, mark_seq, scroll_transcript, toggle_selected,
};
use super::{open_diff, pages, run_slash, slash_matches};
use crate::app::action::{self, AskStep, UiAction};
use crate::app::hit::{ScrollTarget, Target};
use crate::app::overlay::Overlay;
use crate::app::pages::Area;
use crate::app::{App, Focus, PanelTab, SidebarRow, history};

/// Lines moved per wheel notch.
/// 每格滚轮移动的行数。
const WHEEL_LINES: usize = 3;

/// Two clicks on the same cell within this window form a double click.
/// 同一单元格上在此时间窗内的两次点击构成双击。
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

pub(super) fn on_mouse(app: &mut App, event: MouseEvent) {
    let at = Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::ScrollUp => wheel(app, at, -1),
        MouseEventKind::ScrollDown => wheel(app, at, 1),
        MouseEventKind::Down(MouseButton::Left) => {
            let now = Instant::now();
            let double = app.last_click.is_some_and(|(t, x, y)| {
                x == at.x && y == at.y && now.duration_since(t) < DOUBLE_CLICK
            });
            // A double click consumes the pair so a third click starts over.
            // 双击会消耗这一对点击，第三次点击重新计数。
            app.last_click = if double {
                None
            } else {
                Some((now, at.x, at.y))
            };
            if let Some(target) = app.hits.hit(at) {
                left_click(app, target, at, double);
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.last_click = Some((Instant::now(), at.x, at.y));
            if let Some(target) = app.hits.hit(at) {
                right_click(app, target, at);
            }
        }
        _ => {}
    }
}

/// `direction` is -1 for up, 1 for down.
/// `direction` 为 -1 表示向上，1 表示向下。
fn wheel(app: &mut App, at: Position, direction: isize) {
    let Some(target) = app.hits.scroll_at(at) else {
        return;
    };
    let step = |value: usize| {
        if direction < 0 {
            value.saturating_sub(WHEEL_LINES)
        } else {
            value + WHEEL_LINES
        }
    };
    match target {
        // The transcript counts from the bottom, so wheel-up scrolls further away from it.
        // 会话记录从底部计数，因此向上滚动即远离底部。
        ScrollTarget::Transcript => scroll_transcript(app, -direction * WHEEL_LINES as isize),
        ScrollTarget::Sidebar => {
            app.sidebar_free_scroll = true;
            app.sidebar_scroll = step(app.sidebar_scroll);
        }
        ScrollTarget::Nav => {
            // Catalog movement must also load the selected library, just like a click.
            // 目录移动也必须加载所选知识库，与点击一致。
            let count = crate::app::pages::navigation(app)
                .into_iter()
                .map(|(_, entries)| entries.len())
                .sum::<usize>();
            // Clamp a navigation index only; server identities are resolved by Navigate.
            // 只限制导航下标；服务端身份由 Navigate 解析。
            let index = (app.navigation_index() as isize + direction)
                .clamp(0, count.saturating_sub(1) as isize) as usize;
            action::perform(app, UiAction::Navigate(index));
        }
        ScrollTarget::Page => {
            if let Some(page) = app.current_page() {
                let state = app.page_state_mut(page);
                state.free_scroll = true;
                state.scroll = step(state.scroll);
            }
        }
        ScrollTarget::Panel => match app.active_panel_tab() {
            PanelTab::Terminal => {
                let last = app.command_sessions.len().saturating_sub(1);
                app.terminal_cursor = step(app.terminal_cursor).min(last);
            }
            // Scrolling tabs detach the view from the cursor; the renderer clamps the offset.
            // 可滚动标签页使视图脱离光标；由渲染器限制偏移。
            tab if tab.scrolls() => {
                app.panel.free_scroll = true;
                let scroll = app.panel.scroll.entry(tab).or_insert(0);
                *scroll = step(*scroll);
            }
            PanelTab::Progress => {}
            _ => {
                let last = app.git.ordered_files().len().saturating_sub(1);
                app.git.cursor = step(app.git.cursor).min(last);
            }
        },
        ScrollTarget::Overlay => wheel_overlay(app, direction),
    }
}

fn wheel_overlay(app: &mut App, direction: isize) {
    let history_len = match &app.overlay {
        Some(Overlay::History(state)) => app.session().map_or(0, |s| {
            history::visible(&s.events, state.filter, &state.query).len()
        }),
        _ => 0,
    };
    let step = |value: usize, max: usize| {
        if direction < 0 {
            value.saturating_sub(WHEEL_LINES)
        } else {
            (value + WHEEL_LINES).min(max)
        }
    };
    match &mut app.overlay {
        Some(Overlay::Help { scroll }) => *scroll = step(*scroll, usize::MAX),
        Some(Overlay::Diff(view)) => view.scroll = step(view.scroll, usize::MAX),
        Some(Overlay::Inspector(inspector)) => {
            inspector.scroll = step(inspector.scroll, usize::MAX)
        }
        Some(Overlay::Detail(sheet)) => sheet.scroll = step(sheet.scroll, usize::MAX),
        Some(Overlay::Picker(picker)) => {
            let max = picker.visible().len().saturating_sub(1);
            picker.cursor = step(picker.cursor, max);
        }
        Some(Overlay::Menu(menu)) => {
            let max = menu.items.len().saturating_sub(1);
            menu.cursor = step(menu.cursor, max);
        }
        Some(Overlay::Folder(picker)) => {
            let max = picker
                .listing
                .as_ref()
                .map_or(0, |l| l.entries.len())
                .saturating_sub(1);
            picker.cursor = step(picker.cursor, max);
        }
        Some(Overlay::History(state)) => {
            let max = history_len.saturating_sub(1);
            state.cursor = step(state.cursor.min(max), max);
        }
        Some(Overlay::Commit(_) | Overlay::Confirm(_) | Overlay::Prompt(_)) | None => {}
    }
}

fn left_click(app: &mut App, target: Target, at: Position, double: bool) {
    match target {
        Target::Backdrop => app.close_overlay(),
        Target::Inert => {}
        Target::Focus(focus) => {
            if matches!(focus, Focus::AreaNav | Focus::AreaPage) && app.area == Area::Chat {
                return;
            }
            app.focus = focus;
        }
        Target::AreaTab(area) => action::perform(app, UiAction::Activity(area)),
        Target::PanelTab(tab) => {
            app.panel_tab = tab;
            app.focus = Focus::Panel;
        }
        Target::Sidebar(row) => {
            app.focus = Focus::Sidebar;
            if row.selectable() {
                app.sidebar_cursor = Some(row.clone());
            }
            match &row {
                SidebarRow::Project(_) | SidebarRow::Conversation(_) => {
                    activate_sidebar_row(app, &row)
                }
                SidebarRow::Unlinked => {}
            }
            // Selecting a conversation from the sidebar moves focus to the composer; a click
            // keeps it in the sidebar only for projects.
            // 从侧栏选择会话会把焦点移到输入框；只有点击项目时焦点留在侧栏。
        }
        Target::GitFile(index) => {
            app.focus = Focus::Panel;
            app.git.cursor = index;
            if double {
                open_diff(app, index);
            }
        }
        Target::TerminalSession(index) => {
            app.focus = Focus::Panel;
            app.terminal_cursor = index;
        }
        Target::PanelItem(tab, index) => {
            let action = if double {
                UiAction::PanelOpen(tab, index)
            } else {
                UiAction::PanelSelect(tab, index)
            };
            action::perform(app, action);
        }
        Target::Transcript => click_transcript(app, at, double),
        Target::ThinkingBox => {
            if let Some(seq) = app
                .session()
                .and_then(|s| s.live_reasoning())
                .map(|e| e.seq)
            {
                action::perform(app, UiAction::Inspect(seq));
            }
        }
        Target::AskOption(option) => {
            ask_click_option(app, option);
            if double {
                action::perform(app, UiAction::Ask(AskStep::Next));
            }
        }
        Target::AskOther => action::perform(app, UiAction::Ask(AskStep::EditOther)),
        Target::AskQuestion(index) => {
            let total = app
                .session()
                .and_then(|s| s.pending_question.as_ref())
                .map_or(0, |q| q.questions.len());
            app.focus = Focus::Ask;
            app.ask.go_to(index, total);
        }
        Target::NavItem(index) => action::perform(app, UiAction::Navigate(index)),
        Target::PageRow(index) => {
            if let Some(page) = app.current_page() {
                pages::click_row(app, page, index, double);
            }
        }
        Target::PageRowAction(index, chip) => {
            if let Some(page) = app.current_page() {
                app.focus = Focus::AreaPage;
                pages::row_action(app, page, index, chip);
            }
        }
        Target::PickerItem(index) => {
            if let Some(Overlay::Picker(picker)) = &mut app.overlay {
                picker.cursor = index;
            }
            confirm_picker(app);
        }
        Target::MenuItem(index) => menu_pick(app, index),
        Target::FolderEntry(index) => {
            if let Some(Overlay::Folder(picker)) = &mut app.overlay {
                picker.typing = None;
                picker.cursor = index;
            }
            if double {
                action::perform(app, UiAction::FolderChoose);
            }
        }
        Target::CommitField(field) => {
            if let Some(Overlay::Commit(dialog)) = &mut app.overlay {
                dialog.field = field;
                match field {
                    crate::app::overlay::CommitField::Target => {
                        dialog.new_branch = !dialog.new_branch;
                        dialog.error = None;
                    }
                    crate::app::overlay::CommitField::IncludeUnstaged => {
                        dialog.include_unstaged = !dialog.include_unstaged;
                    }
                    _ => {}
                }
            }
        }
        Target::HistoryItem(index) => {
            let seq = match &mut app.overlay {
                Some(Overlay::History(state)) => {
                    state.cursor = index;
                    let (filter, query) = (state.filter, state.query.clone());
                    app.session().and_then(|s| {
                        history::visible(&s.events, filter, &query)
                            .get(index)
                            .map(|e| e.seq)
                    })
                }
                _ => None,
            };
            if double && let Some(seq) = seq {
                action::perform(app, UiAction::Inspect(seq));
            }
        }
        Target::HistoryFilter(filter) => {
            if let Some(Overlay::History(state)) = &mut app.overlay {
                state.filter = filter;
                state.cursor = usize::MAX;
            }
        }
        Target::SlashItem(index) => {
            if let Some(command) = slash_matches(app).get(index).copied() {
                run_slash(app, command);
            }
        }
        Target::Do(action) => action::perform(app, action),
    }
}

/// A click inside the transcript only selects the block under it, because most clicks aim at a
/// block rather than at its fold; a double click folds or unfolds it, like Enter, and a right click
/// opens the inspector, like `i`.
/// 在会话记录内单击只选中其下方的块，因为多数点击是为了选取而非展开；双击才折叠或展开（同 Enter），
/// 右键打开检视器（同 `i`）。
fn click_transcript(app: &mut App, at: Position, double: bool) {
    app.focus = Focus::Transcript;
    let Some((seq, _, _)) = transcript_item_at(app, at) else {
        return;
    };
    mark_seq(app, seq);
    if double {
        toggle_selected(app);
    }
}

fn transcript_item_at(app: &App, at: Position) -> Option<(u64, usize, usize)> {
    let area = app.hits.transcript;
    if at.y < area.y {
        return None;
    }
    let line = app.hits.transcript_top + (at.y - area.y) as usize;
    app.hits
        .transcript_items
        .iter()
        .find(|(_, start, len)| line >= *start && line < start + len)
        .copied()
}

fn right_click(app: &mut App, target: Target, at: Position) {
    match target {
        Target::Sidebar(SidebarRow::Project(id)) => {
            app.focus = Focus::Sidebar;
            app.sidebar_cursor = Some(SidebarRow::Project(id.clone()));
            action::perform(app, UiAction::ProjectMenu(id));
        }
        Target::Sidebar(SidebarRow::Conversation(id)) => {
            app.focus = Focus::Sidebar;
            app.sidebar_cursor = Some(SidebarRow::Conversation(id.clone()));
            action::perform(app, UiAction::ConversationMenu(id));
        }
        Target::Transcript => {
            app.focus = Focus::Transcript;
            if let Some((seq, _, _)) = transcript_item_at(app, at) {
                mark_seq(app, seq);
                action::perform(app, UiAction::Inspect(seq));
            }
        }
        Target::GitFile(index) => {
            app.focus = Focus::Panel;
            app.git.cursor = index;
            open_diff(app, index);
        }
        Target::PanelItem(tab, index) => action::perform(app, UiAction::PanelOpen(tab, index)),
        Target::Backdrop => app.close_overlay(),
        _ => {}
    }
}
