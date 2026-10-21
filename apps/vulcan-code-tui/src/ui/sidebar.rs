// Left sidebar: new-conversation and add-project buttons, projects with their
// conversations, and conversations without a project.
// 左侧栏：新建会话与添加项目按钮、置顶会话、项目及其会话，以及无项目的会话。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use super::age::conversation_age;
use super::text::{spinner, truncate, width_of};
use super::theme::mix;
use crate::app::action::UiAction;
use crate::app::hit::{ScrollTarget, Target};
use crate::app::{App, Focus, SidebarFilter, SidebarMode, SidebarRow};
use crate::tr;

/// Rows above the list: two buttons and a rule.
/// 列表上方的行：两个按钮与一条分隔线。
const HEADER_ROWS: u16 = 3;

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let focused = app.focus == Focus::Sidebar;
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(if focused {
            theme.primary
        } else {
            theme.sidebar_border
        }))
        .style(Style::new().bg(theme.sidebar).fg(theme.sidebar_fg));
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(inner, Target::Focus(Focus::Sidebar));
    if inner.width < 12 || inner.height < HEADER_ROWS + 2 {
        return;
    }
    let width = inner.width as usize;

    // Buttons: new conversation in the context project, add a project folder.
    // 按钮：在当前上下文项目中新建会话、添加项目文件夹。
    let context_project = context_project(app);
    let buttons = [
        (
            "+",
            tr!("projectMenu.newConversation"),
            "Ctrl+N",
            UiAction::NewConversation(context_project),
        ),
        ("⊕", tr!("sidebar.addProject"), "a", UiAction::AddProject),
    ];
    for (row, (icon, label, key, action)) in buttons.into_iter().enumerate() {
        let rect = Rect {
            x: inner.x,
            y: inner.y + row as u16,
            width: inner.width,
            height: 1,
        };
        let label_width = width.saturating_sub(width_of(key) + 6);
        let label = truncate(&label, label_width);
        let pad = label_width.saturating_sub(width_of(&label));
        Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::new().fg(theme.primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(label, Style::new().fg(theme.sidebar_fg)),
            Span::raw(" ".repeat(pad + 1)),
            Span::styled(key, Style::new().fg(theme.faint_fg(theme.sidebar))),
        ])
        .render(rect, buf);
        app.hits.add(rect, Target::Do(action));
    }
    buf.set_string(
        inner.x,
        inner.y + 2,
        "─".repeat(width),
        Style::new().fg(theme.sidebar_border),
    );
    rule_buttons(
        app,
        Rect {
            y: inner.y + 2,
            height: 1,
            ..inner
        },
        buf,
    );

    let mut list = Rect {
        x: inner.x,
        y: inner.y + HEADER_ROWS,
        width: inner.width,
        height: inner.height - HEADER_ROWS,
    };
    // An open search takes the first list row.
    // 打开的搜索占用列表首行。
    if let Some(filter) = app.sidebar_filter.clone() {
        search_row(app, &filter, Rect { height: 1, ..list }, buf);
        list.y += 1;
        list.height -= 1;
    }
    app.hits.sidebar_height = list.height;
    app.hits.add_scroll(list, ScrollTarget::Sidebar);
    if app.projects.is_empty() && app.conversations.is_empty() {
        let text =
            if app.slots.loaded(crate::service_api::Slot::Workbench) || !app.models.is_empty() {
                tr!("sidebar.noProjects")
            } else {
                tr!("sidebar.waitingWorkspace")
            };
        buf.set_string(
            list.x + 1,
            list.y,
            truncate(&text, width - 2),
            theme.muted_text(),
        );
        return;
    }

    let rows = app.sidebar_rows();
    if rows.is_empty() {
        buf.set_string(
            list.x + 1,
            list.y,
            truncate(&tr!("sidebar.noMatches"), width - 2),
            theme.muted_text(),
        );
        return;
    }
    let height = list.height as usize;
    let max_scroll = rows.len().saturating_sub(height);
    let cursor_index = app
        .sidebar_cursor
        .as_ref()
        .and_then(|c| rows.iter().position(|r| r == c));
    // Follow the cursor unless the wheel detached the view.
    // 除非滚轮使视图脱离，否则跟随光标。
    if !app.sidebar_free_scroll
        && let Some(index) = cursor_index
    {
        if index < app.sidebar_scroll {
            app.sidebar_scroll = index.saturating_sub(1);
        } else if index >= app.sidebar_scroll + height {
            app.sidebar_scroll = index + 1 - height;
        }
    }
    app.sidebar_scroll = app.sidebar_scroll.min(max_scroll);

    let cursor_row = app.sidebar_cursor.clone();
    for (row, entry) in rows
        .iter()
        .enumerate()
        .skip(app.sidebar_scroll)
        .take(height)
    {
        let y = list.y + (row - app.sidebar_scroll) as u16;
        let rect = Rect {
            x: list.x,
            y,
            width: list.width,
            height: 1,
        };
        let is_cursor = cursor_row.as_ref() == Some(entry);
        match entry {
            SidebarRow::Unlinked => {
                let label = tr!("sidebar.unlinkedConversations");
                buf.set_string(
                    list.x + 1,
                    y,
                    truncate(&label, width - 2),
                    theme.muted_text().add_modifier(Modifier::BOLD),
                );
            }
            SidebarRow::Project(id) => {
                let Some(project) = app.projects.iter().find(|p| &p.id == id) else {
                    continue;
                };
                let collapsed = app.collapsed_projects.contains(id);
                let count = app
                    .conversations
                    .iter()
                    .filter(|c| c.project_id.as_ref() == Some(id))
                    .count();
                let running = app
                    .conversations
                    .iter()
                    .any(|c| c.project_id.as_ref() == Some(id) && c.run_phase.is_active());
                let tail = if is_cursor {
                    " ⋯ ".to_string()
                } else {
                    format!(" {count} ")
                };
                let pin = if project.pinned { " ⚲" } else { "" };
                let name_width = width
                    .saturating_sub(4 + width_of(&tail) + width_of(pin) + usize::from(running) * 2);
                let name = truncate(&project.name, name_width);
                let pad = name_width.saturating_sub(width_of(&name));
                let bg = if is_cursor && focused {
                    theme.sidebar_accent
                } else {
                    theme.sidebar
                };
                let mut spans = vec![
                    Span::styled(
                        if is_cursor && focused { "▌" } else { " " },
                        Style::new().fg(theme.primary),
                    ),
                    Span::styled(
                        if collapsed { "▸ " } else { "▾ " },
                        Style::new().fg(theme.muted_fg),
                    ),
                    Span::styled(
                        name,
                        Style::new()
                            .fg(theme.sidebar_fg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(pin, Style::new().fg(theme.primary)),
                ];
                if running {
                    spans.push(Span::styled(
                        format!(" {}", spinner(app.phase())),
                        theme.primary_text(),
                    ));
                }
                spans.push(Span::raw(" ".repeat(pad + 1)));
                spans.push(Span::styled(
                    tail.clone(),
                    Style::new().fg(theme.faint_fg(theme.sidebar)),
                ));
                Line::from(spans)
                    .style(Style::new().bg(bg))
                    .render(rect, buf);
                app.hits.add(rect, Target::Sidebar(entry.clone()));
                if is_cursor {
                    let w = width_of(&tail) as u16;
                    app.hits.add(
                        Rect {
                            x: rect.right() - w,
                            width: w,
                            ..rect
                        },
                        Target::Do(UiAction::ProjectMenu(id.clone())),
                    );
                }
            }
            SidebarRow::Conversation(id) => {
                let now_ms = crate::model::now_unix_ms();
                let Some(conversation) = app.conversations.iter().find(|c| &c.id == id) else {
                    continue;
                };
                let selected = app.selected.as_ref() == Some(id);
                let (marker, marker_style) = if conversation.run_phase.is_active() {
                    (spinner(app.phase()), theme.primary_text())
                } else if conversation.attention {
                    (
                        "!",
                        Style::new().fg(theme.warning).add_modifier(Modifier::BOLD),
                    )
                } else if conversation.unread {
                    ("•", Style::new().fg(theme.info))
                } else {
                    (" ", Style::new())
                };
                let tail = if is_cursor {
                    " ⋯ ".to_string()
                } else {
                    format!(" {} ", conversation_age(conversation, now_ms))
                };
                // Dedicated sessions lead their title with a kind glyph, mirroring the WebUI mark.
                // 专职会话在标题前带有种类符号，与 WebUI 标记一致。
                let kind = if conversation.dedicated { "◆ " } else { "" };
                let title_width = width.saturating_sub(width_of(&tail) + 5 + width_of(kind));
                let title = truncate(&conversation.title, title_width);
                let pad = title_width.saturating_sub(width_of(&title));
                let title_style = if selected {
                    Style::new().fg(theme.fg).add_modifier(Modifier::BOLD)
                } else if conversation.unread {
                    Style::new().fg(theme.fg)
                } else {
                    Style::new().fg(theme.sidebar_fg)
                };
                let bg = if selected || (is_cursor && focused) {
                    theme.sidebar_accent
                } else {
                    theme.sidebar
                };
                Line::from(vec![
                    Span::styled(
                        if is_cursor && focused { "▌" } else { " " },
                        Style::new().fg(theme.primary),
                    ),
                    Span::raw("  "),
                    Span::styled(marker, marker_style),
                    Span::raw(" "),
                    Span::styled(kind, theme.primary_text()),
                    Span::styled(title, title_style),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(tail.clone(), Style::new().fg(theme.faint_fg(theme.sidebar))),
                ])
                .style(Style::new().bg(bg))
                .render(rect, buf);
                app.hits.add(rect, Target::Sidebar(entry.clone()));
                if is_cursor {
                    let w = width_of(&tail) as u16;
                    app.hits.add(
                        Rect {
                            x: rect.right() - w,
                            width: w,
                            ..rect
                        },
                        Target::Do(UiAction::ConversationMenu(id.clone())),
                    );
                }
            }
        }
    }

    // Scroll indicator when the list overflows.
    // 列表溢出时的滚动指示。
    if let Some(top) = ((list.height as usize)
        .saturating_sub((height * height / rows.len()).max(1))
        * app.sidebar_scroll)
        .checked_div(max_scroll)
    {
        let track = list.height as usize;
        let thumb = (track * height / rows.len()).max(1);
        for i in 0..thumb {
            if let Some(cell) = buf.cell_mut((list.right() - 1, list.y + (top + i) as u16)) {
                cell.set_char('▐')
                    .set_fg(mix(theme.muted_fg, theme.sidebar, 0.5));
            }
        }
    }
}

/// Buttons set into the rule under the header: fold or unfold every project, and collapse the
/// sidebar to the rail.
/// 嵌在标题下方分隔线中的按钮：折叠或展开全部项目，以及将侧栏收起为窄栏。
fn rule_buttons(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let any_open = app
        .projects
        .iter()
        .any(|p| !app.collapsed_projects.contains(&p.id));
    let buttons = [
        (" / ", UiAction::SearchSidebar),
        (
            if any_open { " ⊟ " } else { " ⊞ " },
            UiAction::FoldAllProjects,
        ),
        (" « ", UiAction::SidebarMode(SidebarMode::Rail)),
    ];
    let style = Style::new()
        .fg(theme.muted_fg)
        .bg(theme.sidebar)
        .add_modifier(Modifier::BOLD);
    let mut x = area.right().saturating_sub(1);
    for (label, action) in buttons.into_iter().rev() {
        let w = width_of(label) as u16;
        if x < area.x + w {
            break;
        }
        x -= w;
        buf.set_string(x, area.y, label, style);
        app.hits.add(
            Rect {
                x,
                width: w,
                ..area
            },
            Target::Do(action),
        );
    }
}

/// The sidebar search field: the query, or what it searches while empty, and the key that closes it.
/// 侧栏搜索框：查询文本（为空时显示搜索范围）及关闭它的按键。
fn search_row(app: &App, filter: &SidebarFilter, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    buf.set_style(area, Style::new().fg(theme.sidebar_fg).bg(theme.popover));
    let hint = " Esc ";
    let room = (area.width as usize).saturating_sub(width_of(hint) + 4);
    let query = if filter.query.is_empty() {
        Span::styled(
            truncate(&tr!("sidebar.searchTree"), room),
            Style::new().fg(theme.muted_fg),
        )
    } else {
        Span::raw(truncate(&filter.query, room))
    };
    Line::from(vec![
        Span::styled(" / ", theme.primary_text()),
        query,
        Span::styled(if filter.editing { "▏" } else { "" }, theme.primary_text()),
    ])
    .render(area, buf);
    buf.set_string(
        area.right().saturating_sub(width_of(hint) as u16),
        area.y,
        hint,
        Style::new().fg(theme.faint_fg(theme.popover)),
    );
    app.hits.add(area, Target::Do(UiAction::SearchSidebar));
}

/// Width of the collapsed sidebar, its right border included.
/// 收起后侧栏的宽度，包括右边框。
pub const RAIL_WIDTH: u16 = 4;

/// The collapsed sidebar: expand, new conversation and add project buttons, then one initial per
/// project with its activity. Clicking an initial opens the list on that project.
/// 收起的侧栏：展开、新建会话与添加项目按钮，随后每个项目一个首字母及其活动状态。点击首字母打开
/// 列表并定位到该项目。
pub fn render_rail(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(theme.sidebar_border))
        .style(Style::new().bg(theme.sidebar).fg(theme.sidebar_fg));
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(inner, Target::Inert);
    if inner.width < 2 || inner.height < HEADER_ROWS + 3 {
        return;
    }
    let row = |y: u16| Rect {
        y,
        height: 1,
        ..inner
    };
    let buttons = [
        ("»", theme.muted_fg, UiAction::OpenSidebar),
        (
            "+",
            theme.primary,
            UiAction::NewConversation(context_project(app)),
        ),
        ("⊕", theme.primary, UiAction::AddProject),
    ];
    let rule_y = inner.y + buttons.len() as u16;
    for (i, (icon, color, action)) in buttons.into_iter().enumerate() {
        let rect = row(inner.y + i as u16);
        buf.set_string(
            rect.x + 1,
            rect.y,
            icon,
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        );
        app.hits.add(rect, Target::Do(action));
    }
    buf.set_string(
        inner.x,
        rule_y,
        "─".repeat(inner.width as usize),
        Style::new().fg(theme.sidebar_border),
    );

    let selected_project = app.conversation().and_then(|c| c.project_id.clone());
    let projects: Vec<String> = app
        .sidebar_rows()
        .into_iter()
        .filter_map(|r| match r {
            SidebarRow::Project(id) => Some(id),
            _ => None,
        })
        .collect();
    let first = rule_y + 1;
    let slots = inner.bottom().saturating_sub(first) as usize;
    // The last slot links to the full list when the projects do not fit.
    // 项目放不下时，最后一格链接到完整列表。
    let overflow = projects.len() > slots;
    let shown = if overflow {
        slots.saturating_sub(1)
    } else {
        projects.len()
    };
    for (i, id) in projects.iter().take(shown).enumerate() {
        let Some(project) = app.projects.iter().find(|p| &p.id == id) else {
            continue;
        };
        let rect = row(first + i as u16);
        let conversations = app
            .conversations
            .iter()
            .filter(|c| c.project_id.as_ref() == Some(id));
        let (mut running, mut attention) = (false, false);
        for conversation in conversations {
            running |= conversation.run_phase.is_active();
            attention |= conversation.attention;
        }
        let (marker, marker_style) = if running {
            (spinner(app.phase()), theme.primary_text())
        } else if attention {
            (
                "!",
                Style::new().fg(theme.warning).add_modifier(Modifier::BOLD),
            )
        } else {
            (" ", Style::new())
        };
        let selected = selected_project.as_ref() == Some(id);
        let bg = if selected {
            theme.sidebar_accent
        } else {
            theme.sidebar
        };
        let initial = project_initial(&project.name);
        Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(
                initial,
                Style::new()
                    .fg(if selected { theme.fg } else { theme.sidebar_fg })
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(Style::new().bg(bg))
        .render(rect, buf);
        app.hits
            .add(rect, Target::Do(UiAction::RailProject(id.clone())));
    }
    if overflow {
        let rect = row(first + shown as u16);
        buf.set_string(rect.x + 1, rect.y, "⋯", theme.muted_text());
        app.hits.add(rect, Target::Do(UiAction::OpenSidebar));
    }
}

/// The first letter or ideograph of a project name, uppercased.
/// 项目名称的第一个字母或汉字，转为大写。
fn project_initial(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map_or_else(|| "·".to_string(), |c| c.to_uppercase().collect())
}

/// The project new conversations go into: the cursor's project, else the open conversation's.
/// 新会话所属的项目：光标所在项目，否则为当前打开会话的项目。
fn context_project(app: &App) -> Option<String> {
    match &app.sidebar_cursor {
        Some(SidebarRow::Project(id)) => Some(id.clone()),
        Some(SidebarRow::Conversation(id)) => app
            .conversations
            .iter()
            .find(|c| &c.id == id)
            .and_then(|c| c.project_id.clone()),
        _ => app.conversation().and_then(|c| c.project_id.clone()),
    }
}
