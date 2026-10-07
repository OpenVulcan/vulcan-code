// Keys of the chat regions: transcript, sidebar, right panel and the ask panel.
// 对话区域的按键：会话记录、侧栏、右侧面板与提问面板。

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{execute, open_commit, open_diff, stage_all, toggle_stage, typed};
use crate::app::action::{self, AskStep, UiAction};
use crate::app::commands::AppCommand;
use crate::app::pages::Area;
use crate::app::{App, Focus, PanelTab, SidebarRow, insight, menus};
use crate::backend::Command;
use crate::model::*;

// ---- Transcript ----

pub(super) fn on_transcript_key(app: &mut App, key: KeyEvent) {
    let page = app.hits.transcript.height.saturating_sub(2).max(1) as isize;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => select_relative(app, -1),
        KeyCode::Down | KeyCode::Char('j') => select_relative(app, 1),
        KeyCode::PageUp => scroll_transcript(app, page),
        KeyCode::PageDown => scroll_transcript(app, -page),
        KeyCode::Home | KeyCode::Char('g') => scroll_transcript(app, isize::MAX / 2),
        KeyCode::End | KeyCode::Char('G') => {
            if let Some(session) = app.session_mut() {
                session.scroll_from_bottom = 0;
                session.selected = None;
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => toggle_selected(app),
        KeyCode::Char('i') => {
            if let Some(seq) = app.session().and_then(|s| s.selected) {
                action::perform(app, UiAction::Inspect(seq));
            }
        }
        KeyCode::Char('v') => {
            if let Some(session) = app.session_mut() {
                session.reveal = !session.reveal;
            }
        }
        KeyCode::Char('e') => edit_selected_prompt(app),
        KeyCode::Char('[') => jump_prompt(app, -1),
        KeyCode::Char(']') => jump_prompt(app, 1),
        KeyCode::Esc => app.focus = Focus::Composer,
        _ => {
            if let Some(ch) = typed(&key) {
                app.focus = Focus::Composer;
                app.composer.insert_char(ch);
            }
        }
    }
}

/// Flips the fold state of the selected block.
/// 翻转选中块的折叠状态。
pub(super) fn toggle_selected(app: &mut App) {
    if let Some(session) = app.session_mut()
        && let Some(seq) = session.selected
        && !session.toggled.remove(&seq)
    {
        session.toggled.insert(seq);
    }
}

/// Loads the selected user prompt into the composer so it can be edited and sent again.
/// 将选中的用户提示载入输入框，以便编辑后再次发送。
fn edit_selected_prompt(app: &mut App) {
    let text = app.session().and_then(|s| {
        let seq = s.selected?;
        s.events
            .iter()
            .find(|e| e.seq == seq)
            .and_then(|e| match &e.body {
                EventBody::UserInput { text, .. } => Some(text.clone()),
                _ => None,
            })
    });
    if let Some(text) = text {
        app.reset_composer();
        app.composer.insert_str(text);
        app.focus = Focus::Composer;
    }
}

/// Jumps the selection to the previous (`-1`) or next (`1`) user prompt.
/// 将选择跳到上一个（`-1`）或下一个（`1`）用户提示。
fn jump_prompt(app: &mut App, direction: isize) {
    let Some(session) = app.session() else {
        return;
    };
    let prompts: Vec<u64> = session
        .events
        .iter()
        .filter(|e| matches!(e.body, EventBody::UserInput { .. }))
        .map(|e| e.seq)
        .collect();
    let current = session.selected.unwrap_or(u64::MAX);
    let target = if direction < 0 {
        prompts.iter().rev().find(|seq| **seq < current).copied()
    } else {
        prompts.iter().find(|seq| **seq > current).copied()
    };
    if let Some(seq) = target {
        select_seq(app, seq);
    }
}

pub fn scroll_transcript(app: &mut App, delta: isize) {
    let total = app.hits.transcript_lines;
    let height = app.hits.transcript.height as usize;
    let max = total.saturating_sub(height);
    if let Some(session) = app.session_mut() {
        let next = (session.scroll_from_bottom as isize + delta).clamp(0, max as isize);
        session.scroll_from_bottom = next as usize;
    }
}

pub(super) fn select_relative(app: &mut App, delta: isize) {
    let items = &app.hits.transcript_items;
    if items.is_empty() {
        return;
    }
    let current = app
        .session()
        .and_then(|s| s.selected)
        .and_then(|seq| items.iter().position(|(s, _, _)| *s == seq));
    let next = match current {
        Some(index) => (index as isize + delta).clamp(0, items.len() as isize - 1) as usize,
        None => items.len() - 1,
    };
    let seq = items[next].0;
    select_seq(app, seq);
}

/// Selects one transcript item and scrolls it into view.
/// 选中一个会话记录条目并滚动到可见范围。
pub(super) fn select_seq(app: &mut App, seq: u64) {
    let item = app
        .hits
        .transcript_items
        .iter()
        .find(|(s, _, _)| *s == seq)
        .copied();
    let total = app.hits.transcript_lines;
    let height = app.hits.transcript.height as usize;
    let Some(session) = app.session_mut() else {
        return;
    };
    session.selected = Some(seq);
    let Some((_, start, len)) = item else {
        return;
    };
    let top = total.saturating_sub(height + session.scroll_from_bottom);
    if start < top {
        session.scroll_from_bottom = total.saturating_sub(height + start);
    } else if start + len > top + height {
        let new_top = (start + len).saturating_sub(height).min(start);
        session.scroll_from_bottom = total.saturating_sub(height + new_top);
    }
}

// ---- Sidebar ----

pub(super) fn on_sidebar_key(app: &mut App, key: KeyEvent) {
    let rows: Vec<SidebarRow> = app
        .sidebar_rows()
        .into_iter()
        .filter(SidebarRow::selectable)
        .collect();
    match key.code {
        KeyCode::Char('a') => return action::perform(app, UiAction::AddProject),
        KeyCode::Esc => return app.set_area(app.area),
        _ => {}
    }
    if rows.is_empty() {
        if key.code == KeyCode::Char('n') {
            action::perform(app, UiAction::NewConversation(None));
        }
        return;
    }
    let current = app
        .sidebar_cursor
        .as_ref()
        .and_then(|row| rows.iter().position(|r| r == row))
        .unwrap_or(0);
    let row = rows[current].clone();
    let move_to = |app: &mut App, index: usize| {
        app.sidebar_cursor = Some(rows[index].clone());
        app.sidebar_free_scroll = false;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => move_to(app, current.saturating_sub(1)),
        KeyCode::Down | KeyCode::Char('j') => move_to(app, (current + 1).min(rows.len() - 1)),
        KeyCode::Home => move_to(app, 0),
        KeyCode::End => move_to(app, rows.len() - 1),
        KeyCode::PageUp => {
            let page = app.hits.sidebar_height.max(1) as usize;
            move_to(app, current.saturating_sub(page));
        }
        KeyCode::PageDown => {
            let page = app.hits.sidebar_height.max(1) as usize;
            move_to(app, (current + page).min(rows.len() - 1));
        }
        KeyCode::Enter | KeyCode::Char(' ') => activate_sidebar_row(app, &row),
        KeyCode::Right => match &row {
            SidebarRow::Project(id) => {
                app.collapsed_projects.remove(id);
            }
            _ => activate_sidebar_row(app, &row),
        },
        KeyCode::Left => {
            let project = match &row {
                SidebarRow::Project(id) => Some(id.clone()),
                SidebarRow::Conversation(id) => app
                    .conversations
                    .iter()
                    .find(|c| &c.id == id)
                    .and_then(|c| c.project_id.clone()),
                _ => None,
            };
            if let Some(id) = project {
                app.collapsed_projects.insert(id.clone());
                app.sidebar_cursor = Some(SidebarRow::Project(id));
            }
        }
        KeyCode::Char('n') => {
            let project_id = match &row {
                SidebarRow::Project(id) => Some(id.clone()),
                SidebarRow::Conversation(id) => app
                    .conversations
                    .iter()
                    .find(|c| &c.id == id)
                    .and_then(|c| c.project_id.clone()),
                _ => None,
            };
            action::perform(app, UiAction::NewConversation(project_id));
        }
        KeyCode::Char('z') => action::perform(app, UiAction::FoldAllProjects),
        KeyCode::Char('m') | KeyCode::Menu => match row {
            SidebarRow::Project(id) => action::perform(app, UiAction::ProjectMenu(id)),
            SidebarRow::Conversation(id) => action::perform(app, UiAction::ConversationMenu(id)),
            _ => {}
        },
        // Letter shortcuts reuse the context menu entries that advertise them.
        // 字母快捷键复用声明了该提示的上下文菜单项。
        KeyCode::Char(_) | KeyCode::Delete => {
            let hint = match key.code {
                KeyCode::Delete => "Del".to_string(),
                KeyCode::Char(ch) => ch.to_string(),
                _ => return,
            };
            let menu = match &row {
                SidebarRow::Project(id) => menus::project_menu(app, id),
                SidebarRow::Conversation(id) => menus::conversation_menu(app, id),
                _ => None,
            };
            if let Some(item) = menu.and_then(|m| m.items.into_iter().find(|i| i.hint == hint)) {
                action::perform(app, item.action);
            }
        }
        _ => {}
    }
}

/// Enter on a sidebar row: fold a project or open a conversation.
/// 在侧栏行上回车：折叠项目或打开会话。
pub(super) fn activate_sidebar_row(app: &mut App, row: &SidebarRow) {
    match row {
        SidebarRow::Project(id) => action::perform(app, UiAction::ToggleProject(id.clone())),
        SidebarRow::Conversation(id) => {
            action::perform(app, UiAction::SelectConversation(id.clone()))
        }
        _ => {}
    }
}

// ---- Right panel ----

pub(super) fn on_panel_key(app: &mut App, key: KeyEvent) {
    let shown = app.panel_tabs();
    let tab_index = shown
        .iter()
        .position(|t| *t == app.active_panel_tab())
        .unwrap_or(0);
    let tabs = shown.len();
    match key.code {
        KeyCode::Left | KeyCode::Char('[') => {
            app.panel_tab = shown[(tab_index + tabs - 1) % tabs];
            return;
        }
        KeyCode::Right | KeyCode::Char(']') => {
            app.panel_tab = shown[(tab_index + 1) % tabs];
            return;
        }
        KeyCode::Esc => {
            app.focus = Focus::Composer;
            return;
        }
        _ => {}
    }
    match app.active_panel_tab() {
        PanelTab::Changes => {
            let count = app.git.ordered_files().len();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    app.git.cursor = app.git.cursor.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.git.cursor = (app.git.cursor + 1).min(count.saturating_sub(1));
                }
                KeyCode::Enter | KeyCode::Char('d') => open_diff(app, app.git.cursor),
                KeyCode::Char('s') | KeyCode::Char(' ') => toggle_stage(app, app.git.cursor),
                KeyCode::Char('a') => stage_all(app, true),
                KeyCode::Char('u') => stage_all(app, false),
                KeyCode::Char('c') => open_commit(app),
                KeyCode::Char('b') => execute(app, AppCommand::SwitchBranch, None),
                KeyCode::Char('p') => execute(app, AppCommand::Push, None),
                KeyCode::Char('r') => app.backend.send(Command::LoadWorkspaceDiffSnapshot),
                KeyCode::Char('g') => {
                    if let Some(key) = app.git.next_repository() {
                        action::perform(app, UiAction::SelectGitRepository(key));
                    }
                }
                KeyCode::Char('m') => {
                    if let Some(menu) = app.git.monitoring_menu() {
                        app.overlay = Some(crate::app::overlay::Overlay::Menu(menu));
                    }
                }
                _ => {}
            }
        }
        PanelTab::Terminal => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.terminal_cursor = app.terminal_cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.terminal_cursor =
                    (app.terminal_cursor + 1).min(app.command_sessions.len().saturating_sub(1));
            }
            _ => {}
        },
        PanelTab::Memory | PanelTab::Agents => on_panel_list_key(app, key),
        PanelTab::Stats => on_stats_key(app, key),
        PanelTab::Progress => {}
    }
}

/// Memory and Agents: a cursor over rows; Enter opens the detail sheet.
/// 记忆与代理：在行间移动光标；Enter 打开详情面板。
fn on_panel_list_key(app: &mut App, key: KeyEvent) {
    let tab = app.active_panel_tab();
    let count = insight::item_count(app, tab);
    let cursor = app.panel.cursor(tab);
    let select = |app: &mut App, index: usize| {
        action::perform(
            app,
            UiAction::PanelSelect(tab, index.min(count.saturating_sub(1))),
        )
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => select(app, cursor.saturating_sub(1)),
        KeyCode::Down | KeyCode::Char('j') => select(app, cursor + 1),
        KeyCode::PageUp => select(app, cursor.saturating_sub(5)),
        KeyCode::PageDown => select(app, cursor + 5),
        KeyCode::Home => select(app, 0),
        KeyCode::End => select(app, count),
        KeyCode::Enter if count > 0 => action::perform(app, UiAction::PanelOpen(tab, cursor)),
        KeyCode::Char('v') if tab == PanelTab::Memory => {
            let static_view = !app.panel.memory_static;
            action::perform(app, UiAction::MemoryView(static_view));
        }
        KeyCode::Char(ch @ ('s' | 'x')) if tab == PanelTab::Agents => {
            let (Some(conversation_id), Some(agent)) = (
                app.selected.clone(),
                app.session().and_then(|s| s.agents.get(cursor)).cloned(),
            ) else {
                return;
            };
            let action = match ch {
                's' if agent.state == AgentState::Running => {
                    Some(insight::stop_agent(&conversation_id, &agent))
                }
                'x' if agent.state != AgentState::Closed => {
                    Some(insight::close_agent(&conversation_id, &agent))
                }
                _ => None,
            };
            if let Some(action) = action {
                action::perform(app, action);
            }
        }
        _ => {}
    }
}

/// Stats: arrows scroll the page; `g` edits the goal and `p` pauses or resumes it.
/// 统计：方向键滚动页面；`g` 编辑目标，`p` 暂停或继续。
fn on_stats_key(app: &mut App, key: KeyEvent) {
    let tab = PanelTab::Stats;
    let scroll = |app: &mut App, delta: isize| {
        app.panel.free_scroll = true;
        let value = app.panel.scroll.entry(tab).or_insert(0);
        *value = value.saturating_add_signed(delta);
    };
    let Some(conversation_id) = app.selected.clone() else {
        return;
    };
    let goal = app.session().and_then(|s| s.insight.goal.clone());
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => scroll(app, -1),
        KeyCode::Down | KeyCode::Char('j') => scroll(app, 1),
        KeyCode::PageUp => scroll(app, -10),
        KeyCode::PageDown => scroll(app, 10),
        KeyCode::Home => {
            app.panel.scroll.insert(tab, 0);
        }
        KeyCode::Char('g') => {
            action::perform(app, insight::edit_goal(&conversation_id, goal.as_ref()))
        }
        KeyCode::Char('p') => {
            if let Some(command) = goal.and_then(|g| insight::toggle_goal(&conversation_id, &g)) {
                app.backend.send(command);
            }
        }
        _ => {}
    }
}

// ---- Ask ----

pub(super) fn on_ask_key(app: &mut App, key: KeyEvent) {
    let Some(pending) = app.session().and_then(|s| s.pending_question.clone()) else {
        app.focus = Focus::Composer;
        return;
    };
    let total = pending.questions.len();
    let question = &pending.questions[app.ask.index.min(total - 1)];
    let other_row = question.options.len();
    let ask = &mut app.ask;

    if ask.confirm_reject {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => ask_step(app, AskStep::RejectConfirm),
            KeyCode::Esc | KeyCode::Char('n') => ask_step(app, AskStep::RejectCancel),
            _ => {}
        }
        return;
    }
    if ask.editing_other {
        match key.code {
            KeyCode::Esc => ask.editing_other = false,
            KeyCode::Enter => {
                ask.editing_other = false;
                ask_step(app, AskStep::Next);
            }
            KeyCode::Backspace => {
                ask.other_mut(question.multiple).pop();
            }
            _ => {
                if let Some(ch) = typed(&key) {
                    ask.other_mut(question.multiple).push(ch);
                }
            }
        }
        return;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => ask.cursor = ask.cursor.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => ask.cursor = (ask.cursor + 1).min(other_row),
        KeyCode::Left => ask_step(app, AskStep::Prev),
        KeyCode::Right => {
            let next = ask.index + 1;
            if next < total {
                ask.go_to(next, total);
            }
        }
        // Digits only pick; moving on is always an explicit Enter or → so the active question
        // never changes under the user's hands.
        // 数字键只做选择；切换题目总是显式的回车或 →，当前题目不会在用户不知情时变化。
        KeyCode::Char(ch @ '1'..='9') => {
            let option = ch as usize - '1' as usize;
            if option < other_row {
                ask.cursor = option;
                ask.choose(option, question.multiple);
            }
        }
        KeyCode::Char(' ') => {
            if ask.cursor == other_row {
                ask.editing_other = true;
            } else {
                ask.choose(ask.cursor, question.multiple);
            }
        }
        KeyCode::Enter => {
            if ask.cursor == other_row {
                ask.editing_other = true;
                return;
            }
            // Enter on a multi-select question only picks the cursor row when nothing is chosen yet.
            // 多选问题中，仅在尚未选择任何项时 Enter 才选中光标行。
            if !question.multiple || ask.selections.get(&ask.index).is_none_or(Vec::is_empty) {
                ask.choose(ask.cursor, question.multiple);
            }
            ask_step(app, AskStep::Next);
        }
        KeyCode::Char('x') | KeyCode::Delete => ask.confirm_reject = true,
        KeyCode::Esc => app.focus = Focus::Composer,
        _ => {}
    }
}

/// Moves through the ask panel; `Next` on the last question submits.
/// 在提问面板中移动；最后一题上的 `Next` 即提交。
pub fn ask_step(app: &mut App, step: AskStep) {
    let Some(pending) = app.session().and_then(|s| s.pending_question.clone()) else {
        return;
    };
    if app.area != Area::Chat {
        app.set_area(Area::Chat);
    }
    app.focus = Focus::Ask;
    let total = pending.questions.len();
    let ask = &mut app.ask;
    match step {
        AskStep::Prev => {
            let prev = ask.index.saturating_sub(1);
            ask.go_to(prev, total);
        }
        AskStep::Next if ask.index + 1 < total => {
            let next = ask.index + 1;
            ask.go_to(next, total);
        }
        AskStep::Next | AskStep::Submit => match ask.answers(&pending) {
            Ok(answers) => app.backend.send(Command::ReplyPendingQuestion {
                request_id: pending.id.clone(),
                answers,
            }),
            Err(missing) => {
                ask.missing = Some(missing);
                ask.go_to(missing, total);
            }
        },
        AskStep::Reject => ask.confirm_reject = true,
        AskStep::RejectCancel => ask.confirm_reject = false,
        AskStep::RejectConfirm => {
            ask.confirm_reject = false;
            app.backend.send(Command::RejectPendingQuestion {
                request_id: pending.id.clone(),
            });
        }
        AskStep::EditOther => {
            let index = ask.index.min(total.saturating_sub(1));
            ask.cursor = pending.questions[index].options.len();
            ask.editing_other = true;
        }
    }
}

/// Clicks an option of the current question.
/// 点击当前问题的某个选项。
pub(super) fn ask_click_option(app: &mut App, option: usize) {
    let Some(pending) = app.session().and_then(|s| s.pending_question.clone()) else {
        return;
    };
    app.focus = Focus::Ask;
    let index = app.ask.index.min(pending.questions.len().saturating_sub(1));
    let multiple = pending.questions[index].multiple;
    app.ask.editing_other = false;
    app.ask.cursor = option;
    app.ask.choose(option, multiple);
}
