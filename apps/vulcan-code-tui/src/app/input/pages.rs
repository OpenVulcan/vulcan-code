// Keys and clicks of the system areas: the navigation column and the generic page view.
// 系统区域的按键与点击：导航列与通用页面视图。

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::typed;
use crate::app::action::{self, PromptDialog, PromptThen, UiAction};
use crate::app::pages::{self, Area, PageId, RowKind};
use crate::app::{App, Focus};
use crate::model::SettingValue;

/// How a row was activated.
/// 行的激活方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activate {
    Enter,
    Space,
    Left,
    Right,
    Click,
    /// The second click of a double click: expands items and leaves controls as the first click set them.
    /// 双击中的第二次点击：展开条目，控件保持第一次点击设定的状态。
    DoubleClick,
}

pub(super) fn on_nav_key(app: &mut App, key: KeyEvent) {
    // Navigation identities are rebuilt from the current catalogs, including empty-library management.
    // 导航身份从当前目录重建，包括空知识库的管理入口。
    let count = crate::app::pages::navigation(app)
        .into_iter()
        .map(|(_, entries)| entries.len())
        .sum::<usize>();
    // Selected catalog row rather than the independent content-page index.
    // 已选目录行，而非独立内容页面下标。
    let index = app.navigation_index();
    // Resolve keyboard movement through the same action used by mouse and wheel.
    // 经鼠标及滚轮共用的动作解析键盘移动。
    let destination = match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(index.saturating_sub(1)),
        KeyCode::Down | KeyCode::Char('j') => Some((index + 1).min(count.saturating_sub(1))),
        KeyCode::Home => Some(0),
        KeyCode::End => Some(count.saturating_sub(1)),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => {
            app.focus = Focus::AreaPage;
            None
        }
        KeyCode::Esc | KeyCode::Left => {
            action::perform(app, UiAction::Area(Area::Chat));
            None
        }
        KeyCode::Char('/') => {
            search(app);
            None
        }
        _ => None,
    };
    if let Some(destination) = destination {
        action::perform(app, UiAction::Navigate(destination));
    }
}

fn search(app: &mut App) {
    if let Some(action) = app.current_page().and_then(|page| page.search(app)) {
        action::perform(app, action);
    }
}

pub(super) fn on_page_key(app: &mut App, key: KeyEvent) {
    let Some(page) = app.current_page() else {
        app.focus = Focus::AreaNav;
        return;
    };
    let rows = pages::rows(app, page);
    let selectable: Vec<usize> = (0..rows.len()).filter(|i| rows[*i].selectable()).collect();
    let state = app.page_state(page);
    let position = selectable.iter().position(|i| *i == state.cursor);
    let move_to = |app: &mut App, target: Option<usize>| {
        if let Some(index) = target {
            let state = app.page_state_mut(page);
            state.cursor = index;
            state.action = 0;
            state.free_scroll = false;
        }
    };
    let page_rows = app.hits.page_height.max(1) as usize;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            let target = match position {
                Some(p) => selectable.get(p.saturating_sub(1)).copied(),
                None => selectable
                    .iter()
                    .rev()
                    .find(|i| **i < state.cursor)
                    .copied(),
            };
            move_to(app, target.or(selectable.first().copied()));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let target = match position {
                Some(p) => selectable
                    .get((p + 1).min(selectable.len().saturating_sub(1)))
                    .copied(),
                None => selectable.iter().find(|i| **i > state.cursor).copied(),
            };
            move_to(app, target.or(selectable.last().copied()));
        }
        KeyCode::Home => move_to(app, selectable.first().copied()),
        KeyCode::End => move_to(app, selectable.last().copied()),
        KeyCode::PageUp => {
            let state = app.page_state_mut(page);
            state.scroll = state.scroll.saturating_sub(page_rows);
            state.free_scroll = true;
        }
        KeyCode::PageDown => {
            let state = app.page_state_mut(page);
            state.scroll += page_rows;
            state.free_scroll = true;
        }
        KeyCode::Esc => app.focus = Focus::AreaNav,
        KeyCode::Char('/') => search(app),
        KeyCode::Enter => activate(app, page, state.cursor, Activate::Enter),
        KeyCode::Char(' ') => activate(app, page, state.cursor, Activate::Space),
        KeyCode::Left | KeyCode::Char('h') => {
            if position.is_some() {
                activate(app, page, state.cursor, Activate::Left);
            } else {
                app.focus = Focus::AreaNav;
            }
        }
        KeyCode::Right | KeyCode::Char('l') => activate(app, page, state.cursor, Activate::Right),
        _ => {
            if typed(&key).is_some() && position.is_none() {
                move_to(app, selectable.first().copied());
            }
        }
    }
}

/// Activates one page row; keyboard and mouse share this so both always behave the same.
/// 激活页面中的一行；键盘与鼠标共用此函数，行为始终一致。
pub fn activate(app: &mut App, page: PageId, index: usize, how: Activate) {
    let rows = pages::rows(app, page);
    let Some(row) = rows.get(index) else {
        return;
    };
    let label = row.label.clone();
    match row.kind.clone() {
        RowKind::Toggle { .. } if how == Activate::DoubleClick => {}
        RowKind::Toggle { on, edit } => {
            let value = match how {
                Activate::Left => false,
                Activate::Right => true,
                _ => !on,
            };
            if value != on {
                action::apply_edit(app, edit, SettingValue::Bool(value));
            }
        }
        RowKind::Choice {
            options,
            current,
            edit,
        } => match how {
            Activate::Enter | Activate::Click => action::perform(
                app,
                UiAction::Pick {
                    title: label,
                    edit,
                    options,
                    current,
                },
            ),
            Activate::DoubleClick => {}
            Activate::Space | Activate::Right | Activate::Left => {
                if options.is_empty() {
                    return;
                }
                let at = options
                    .iter()
                    .position(|(id, _)| *id == current)
                    .unwrap_or(0) as isize;
                let delta = if how == Activate::Left { -1 } else { 1 };
                let next = (at + delta).rem_euclid(options.len() as isize) as usize;
                action::apply_edit(app, edit, SettingValue::Choice(options[next].0.clone()));
            }
        },
        RowKind::Number {
            value,
            min,
            max,
            step,
            edit,
        } => {
            let next = match how {
                Activate::DoubleClick => return,
                Activate::Left => (value - step).max(min),
                Activate::Right | Activate::Space => (value + step).min(max),
                Activate::Enter | Activate::Click => {
                    // Typing a value reaches any point of a wide range without stepping.
                    // 直接输入数值可到达大范围内的任意值，无需逐步调整。
                    let dialog = PromptDialog::new(
                        label,
                        crate::tr!("tui.prompt.rangeHint", min = min, max = max),
                        value.to_string(),
                        PromptThen::Number { edit, min, max },
                        false,
                    );
                    action::perform(app, UiAction::Prompt(Box::new(dialog)));
                    return;
                }
            };
            if next != value {
                action::apply_edit(app, edit, SettingValue::Number(next));
            }
        }
        RowKind::Text {
            value,
            edit,
            secret,
        } => {
            if matches!(how, Activate::Enter | Activate::Space | Activate::Click) {
                let mut dialog = PromptDialog::new(
                    label,
                    row.detail.clone(),
                    value,
                    PromptThen::Edit(edit),
                    secret,
                );
                if secret {
                    dialog.input.clear();
                }
                action::perform(app, UiAction::Prompt(Box::new(dialog)));
            }
        }
        RowKind::Button { action, .. } => {
            if matches!(how, Activate::Enter | Activate::Space | Activate::Click) {
                action::perform(app, action);
            }
        }
        RowKind::Item {
            id, actions, lines, ..
        } => {
            let state = app.page_state(page);
            let chosen = state.action.min(actions.len().saturating_sub(1));
            match how {
                Activate::Left => app.page_state_mut(page).action = chosen.saturating_sub(1),
                Activate::Right => {
                    app.page_state_mut(page).action =
                        (chosen + 1).min(actions.len().saturating_sub(1));
                }
                Activate::Enter if !actions.is_empty() => {
                    action::perform(app, actions[chosen].action.clone());
                }
                // A single click only selects an item; most clicks aim at the item, not its details.
                // 单击只选中条目；多数点击是为了选取条目而非展开其详情。
                Activate::Click => {}
                _ if !lines.is_empty() => {
                    let expanded = &mut app.page_state_mut(page).expanded;
                    if !expanded.remove(&id) {
                        expanded.insert(id);
                    }
                }
                Activate::Space if !actions.is_empty() => {
                    action::perform(app, actions[chosen].action.clone());
                }
                _ => {}
            }
        }
        RowKind::Section
        | RowKind::Info { .. }
        | RowKind::Note { .. }
        | RowKind::Document { .. }
        | RowKind::Stats(_)
        | RowKind::Meter { .. }
        | RowKind::Spark { .. }
        | RowKind::Brand
        | RowKind::Spacer => {}
    }
}

/// Clicks the `action`-th chip of a row: an item action, or the ‹ › steppers of choices and
/// numbers.
/// 点击某行的第 `action` 个按钮：条目操作，或选项与数值的 ‹ › 步进器。
pub fn row_action(app: &mut App, page: PageId, index: usize, chip: usize) {
    let rows = pages::rows(app, page);
    let Some(row) = rows.get(index) else {
        return;
    };
    {
        let state = app.page_state_mut(page);
        state.cursor = index;
        state.action = chip;
    }
    match &row.kind {
        RowKind::Item { actions, .. } => {
            if let Some(chip) = actions.get(chip) {
                action::perform(app, chip.action.clone());
            }
        }
        RowKind::Choice { .. } | RowKind::Number { .. } | RowKind::Toggle { .. } => {
            let how = if chip == 0 {
                Activate::Left
            } else {
                Activate::Right
            };
            activate(app, page, index, how);
        }
        _ => {}
    }
}

/// Mouse click on a page row: move the cursor there and activate it.
/// 鼠标点击页面行：将光标移到该行并激活。
/// `double` marks the second click of a double click.
/// `double` 表示双击中的第二次点击。
pub fn click_row(app: &mut App, page: PageId, index: usize, double: bool) {
    app.focus = Focus::AreaPage;
    let rows = pages::rows(app, page);
    if !rows.get(index).is_some_and(|r| r.selectable()) {
        return;
    }
    let state = app.page_state_mut(page);
    state.cursor = index;
    state.free_scroll = true;
    let how = if double {
        Activate::DoubleClick
    } else {
        Activate::Click
    };
    activate(app, page, index, how);
}
