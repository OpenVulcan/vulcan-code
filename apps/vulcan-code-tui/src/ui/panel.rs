// Right-side session tools panel: Git changes, progress, command sessions, injected memory,
// usage statistics and subagents.
// 右侧会话工具面板：Git 变更、进度、命令会话、注入记忆、用量统计与子代理。
//
// The Changes tab is the TUI home for the WebUI review sidebar: branch and upstream state, staged
// and unstaged files, unpushed commits, and key hints for diff, stage, commit, branch and push.
// Memory mirrors the WebUI dynamic / static context panel, Stats the usage popover and goal bar,
// and Agents the subagent status list with its stop / end controls.
// 变更页是 WebUI 审阅侧栏在 TUI 中的位置：分支与上游状态、已暂存与未暂存文件、未推送提交，
// 以及 diff、暂存、提交、分支与推送的按键提示。记忆页对应 WebUI 的动态 / 静态上下文面板，
// 统计页对应用量浮层与目标栏，代理页对应子代理状态列表及其停止 / 结束控制。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use super::chrome::{ButtonKind, button, button_style, overflow_chip, overflow_menu};
use super::text::{
    OVERFLOW_CHIP, compact_count, compact_duration, fit, fit_strip, spinner, truncate, width_of,
    wrap,
};
use super::theme::{Theme, mix};
use crate::app::action::UiAction;
use crate::app::commands::AppCommand;
use crate::app::hit::{ScrollTarget, Target};
use crate::app::insight::{self, MemoryEntry};
use crate::app::overlay::MenuItem;
use crate::app::{App, Focus, PanelTab};
use crate::backend::GitBusy;
use crate::model::*;
use crate::tr;

pub fn tab_label(tab: PanelTab) -> String {
    match tab {
        PanelTab::Changes => tr!("sessionTools.changes"),
        PanelTab::Progress => tr!("sessionTools.progress"),
        PanelTab::Terminal => tr!("sessionTools.terminal"),
        PanelTab::Memory => tr!("contextPanel.memory"),
        PanelTab::Stats => tr!("tui.panel.stats"),
        PanelTab::Agents => tr!("area.agents"),
    }
}

/// One rendered panel line: an optional whole-line target plus clickable chips `(x, width, action)`.
/// 一行面板内容：可选的整行目标，以及可点击的按钮 `(x, 宽度, 动作)`。
struct Row {
    line: Line<'static>,
    target: Option<Target>,
    chips: Vec<(u16, u16, UiAction)>,
}

impl Row {
    fn new(line: Line<'static>) -> Self {
        Row {
            line,
            target: None,
            chips: Vec::new(),
        }
    }

    fn target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }
}

fn rows(lines: Vec<Line<'static>>) -> Vec<Row> {
    lines.into_iter().map(Row::new).collect()
}

/// Tab label with its live count, e.g. `Changes 7`.
/// 带实时计数的标签，例如 `Changes 7`。
fn tab_text(app: &App, tab: PanelTab) -> String {
    let label = tab_label(tab);
    let count = match tab {
        PanelTab::Changes => app.git.snapshot.as_ref().map_or(0, |s| s.files.len()),
        PanelTab::Memory => app
            .session()
            .map_or(0, |s| s.insight.memory.dynamic_count()),
        PanelTab::Agents => app.session().map_or(0, |s| {
            s.agents
                .iter()
                .filter(|a| a.state != AgentState::Closed)
                .count()
        }),
        _ => 0,
    };
    if count > 0 {
        format!(" {label} {count} ")
    } else {
        format!(" {label} ")
    }
}

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let focused = app.focus == Focus::Panel;
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(Style::new().fg(if focused {
            theme.primary
        } else {
            theme.sidebar_border
        }))
        .style(Style::new().bg(theme.sidebar).fg(theme.sidebar_fg));
    let inner = block.inner(area);
    block.render(area, buf);
    app.hits.add(inner, Target::Focus(Focus::Panel));
    if inner.height < 4 || inner.width < 12 {
        return;
    }

    // Tab strip: one line; tabs without room fold into an overflow menu, the active tab stays.
    // 标签条：单行；放不下的标签收进溢出菜单，当前标签始终可见。
    let left = inner.x + 1;
    let right = inner.right().saturating_sub(1);
    let y = inner.y;
    let texts: Vec<(PanelTab, String)> = PanelTab::ALL
        .into_iter()
        .map(|tab| (tab, tab_text(app, tab)))
        .collect();
    let widths: Vec<usize> = texts.iter().map(|(_, text)| width_of(text)).collect();
    let active_index = PanelTab::ALL.iter().position(|tab| *tab == app.panel_tab);
    let shown = fit_strip(
        &widths,
        1,
        right.saturating_sub(left) as usize,
        width_of(OVERFLOW_CHIP),
        active_index,
    );
    let mut x = left;
    for &index in &shown {
        let (tab, text) = (texts[index].0, &texts[index].1);
        let w = widths[index] as u16;
        let active = app.panel_tab == tab;
        let style = if active {
            Style::new()
                .fg(if focused { theme.primary } else { theme.fg })
                .add_modifier(Modifier::BOLD)
                .bg(theme.sidebar_accent)
        } else {
            Style::new().fg(theme.muted_fg)
        };
        let rect = Rect {
            x,
            y,
            width: w.min(right.saturating_sub(x)),
            height: 1,
        };
        buf.set_string(x, y, truncate(text, rect.width as usize), style);
        app.hits.add(rect, Target::PanelTab(tab));
        x += w + 1;
    }
    let folded: Vec<MenuItem> = texts
        .iter()
        .enumerate()
        .filter(|(index, _)| !shown.contains(index))
        .map(|(_, (tab, text))| MenuItem {
            label: text.trim().to_string(),
            hint: String::new(),
            action: UiAction::ShowPanel(*tab),
            danger: false,
        })
        .collect();
    overflow_chip(app, buf, x, y, Style::new().fg(theme.muted_fg), folded);
    let rule_y = y + 1;
    buf.set_string(
        inner.x,
        rule_y,
        "─".repeat(inner.width as usize),
        Style::new().fg(mix(theme.sidebar_border, theme.sidebar, 0.3)),
    );
    let top = rule_y + 1;
    let body = Rect {
        x: inner.x + 1,
        y: top,
        width: inner.width.saturating_sub(3),
        height: inner.bottom().saturating_sub(top),
    };
    if body.height == 0 {
        return;
    }
    app.hits.add_scroll(body, ScrollTarget::Panel);
    let tab = app.panel_tab;
    // Scrolling tabs keep one blank column before their scrollbar.
    // 可滚动标签页在滚动条前保留一列空白。
    let width = body.width as usize - usize::from(tab.scrolls());
    let lines: Vec<Row> = match tab {
        PanelTab::Changes => changes(app, width, focused),
        PanelTab::Progress => rows(progress(app, width)),
        PanelTab::Terminal => terminal(app, width, body.height as usize),
        PanelTab::Memory => memory(app, width, focused),
        PanelTab::Stats => stats(app, width),
        PanelTab::Agents => agents(app, width, focused),
    };
    // Changes gets a two-row button bar; other tabs a one-row key hint while focused.
    // 变更页有两行按钮栏；其他标签页在聚焦时有一行按键提示。
    let hint = match tab {
        PanelTab::Terminal if !app.command_sessions.is_empty() => {
            Some(tr!("tui.panel.terminalHint"))
        }
        PanelTab::Memory => Some(tr!("tui.panel.memoryHint")),
        PanelTab::Stats => Some(tr!("tui.panel.statsHint")),
        PanelTab::Agents if !lines.is_empty() => Some(tr!("tui.panel.agentsHint")),
        _ => None,
    }
    .filter(|_| focused);
    let footer_rows: u16 = match tab {
        PanelTab::Changes if app.git.snapshot.is_some() => 2,
        _ if hint.is_some() => 1,
        _ => 0,
    };
    let height = body.height.saturating_sub(footer_rows) as usize;
    let offset = if tab.scrolls() {
        scroll_offset(app, tab, &lines, height)
    } else {
        // Keep the cursor row visible: the list scrolls just enough to show it.
        // 保持光标行可见：列表只滚动到恰好显示它的位置。
        let cursor_line = lines.iter().position(|row| match (&row.target, tab) {
            (Some(Target::GitFile(i)), PanelTab::Changes) => *i == app.git.cursor,
            (Some(Target::TerminalSession(i)), PanelTab::Terminal) => *i == app.terminal_cursor,
            _ => false,
        });
        cursor_line.map_or(0, |line| (line + 1).saturating_sub(height))
    };
    for (index, row) in lines.iter().skip(offset).take(height).enumerate() {
        let rect = Rect {
            y: body.y + index as u16,
            height: 1,
            ..body
        };
        row.line.clone().render(rect, buf);
        if let Some(target) = &row.target {
            app.hits.add(rect, target.clone());
        }
        for (dx, w, action) in &row.chips {
            app.hits.add(
                Rect {
                    x: rect.x + dx,
                    width: *w,
                    ..rect
                },
                Target::Do(action.clone()),
            );
        }
    }
    if tab.scrolls() && lines.len() > height && height > 0 {
        scrollbar(
            &theme,
            Rect {
                x: body.right(),
                width: 1,
                height: height as u16,
                ..body
            },
            offset,
            lines.len(),
            buf,
        );
    }
    let footer_y = body.bottom().saturating_sub(footer_rows);
    match (tab, hint) {
        (PanelTab::Changes, _) if footer_rows > 0 => git_buttons(
            app,
            Rect {
                y: footer_y,
                height: 2,
                ..body
            },
            buf,
        ),
        (_, Some(hint)) => {
            buf.set_string(
                body.x,
                footer_y,
                truncate(&hint, body.width as usize),
                Style::new().fg(mix(theme.muted_fg, theme.sidebar, 0.75)),
            );
        }
        _ => {}
    }
}

/// First visible line of a scrolling tab. Follows the cursor rows unless the wheel detached the
/// view; the result is stored back so the wheel continues from it.
/// 可滚动标签页的首个可见行。除非滚轮使视图脱离，否则跟随光标行；结果写回，使滚轮从此继续。
fn scroll_offset(app: &mut App, tab: PanelTab, lines: &[Row], height: usize) -> usize {
    let count = insight::item_count(app, tab);
    let cursor = app.panel.cursor(tab).min(count.saturating_sub(1));
    app.panel.cursor.insert(tab, cursor);
    let mut scroll = app.panel.scroll.get(&tab).copied().unwrap_or(0);
    if !app.panel.free_scroll {
        let owned = |row: &Row| matches!(row.target, Some(Target::PanelItem(t, i)) if t == tab && i == cursor);
        if let (Some(first), Some(last)) =
            (lines.iter().position(owned), lines.iter().rposition(owned))
        {
            // The expanded agent brings its controls along, so show the whole block when it fits.
            // 展开的代理会带出控制按钮，放得下时显示整个块。
            let last = lines[last..]
                .iter()
                .position(|r| r.line.spans.is_empty())
                .map_or(last, |gap| last + gap);
            if last + 1 > scroll + height {
                scroll = (last + 1).saturating_sub(height);
            }
            if first < scroll {
                scroll = first;
            }
        }
    }
    let scroll = scroll.min(lines.len().saturating_sub(height));
    app.panel.scroll.insert(tab, scroll);
    scroll
}

fn scrollbar(theme: &Theme, track: Rect, offset: usize, total: usize, buf: &mut Buffer) {
    let height = track.height as usize;
    let thumb = (height * height / total).clamp(1, height);
    let start = if total > height {
        (offset * (height - thumb)) / (total - height)
    } else {
        0
    };
    for row in 0..height {
        let (glyph, color) = if row >= start && row < start + thumb {
            ("┃", mix(theme.muted_fg, theme.sidebar, 0.4))
        } else {
            ("│", mix(theme.sidebar_border, theme.sidebar, 0.5))
        };
        buf.set_string(track.x, track.y + row as u16, glyph, Style::new().fg(color));
    }
}

/// A one-line row of small buttons starting at `indent` columns within `width`; buttons without
/// room fold into an overflow chip, while the active one always stays visible.
/// 从 `indent` 列开始、宽度不超过 `width` 的单行小按钮；放不下的按钮收进溢出按钮，当前按钮始终可见。
fn chip_row(
    app: &App,
    indent: u16,
    width: usize,
    items: Vec<(String, bool, bool, UiAction)>,
) -> Row {
    let theme = &app.theme;
    let texts: Vec<String> = items
        .iter()
        .map(|(label, ..)| format!(" {label} "))
        .collect();
    let widths: Vec<usize> = texts.iter().map(|text| width_of(text)).collect();
    let shown = fit_strip(
        &widths,
        1,
        width.saturating_sub(indent as usize),
        width_of(OVERFLOW_CHIP),
        items.iter().position(|(_, active, ..)| *active),
    );
    let normal = button_style(app, ButtonKind::Normal, theme.sidebar);
    let mut spans = vec![Span::raw(" ".repeat(indent as usize))];
    let mut chips = Vec::new();
    let mut folded = Vec::new();
    let mut x = indent;
    for (index, ((label, active, danger, action), text)) in items.into_iter().zip(texts).enumerate()
    {
        if !shown.contains(&index) {
            folded.push(MenuItem {
                label,
                hint: String::new(),
                action,
                danger,
            });
            continue;
        }
        let w = width_of(&text) as u16;
        let style = if active {
            button_style(app, ButtonKind::Focused, theme.sidebar)
        } else if danger {
            Style::new()
                .fg(theme.danger)
                .bg(mix(theme.accent, theme.sidebar, 0.9))
        } else {
            normal
        };
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
        chips.push((x, w, action));
        x += w + 1;
    }
    if !folded.is_empty() {
        spans.push(Span::styled(OVERFLOW_CHIP, normal));
        chips.push((
            x,
            width_of(OVERFLOW_CHIP) as u16,
            UiAction::OpenMenu(Box::new(overflow_menu(folded))),
        ));
    }
    Row {
        line: Line::from(spans),
        target: None,
        chips,
    }
}

/// Git action buttons: diff / stage toggle / stage all on the first row; commit, branch and push on
/// the second.
/// Git 操作按钮：第一行为 diff / 切换暂存 / 全部暂存；第二行为提交、分支与推送。
fn git_buttons(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let bg = theme.sidebar;
    let normal = button_style(app, ButtonKind::Normal, bg);
    let files = app.git.ordered_files();
    let cursor = app.git.cursor;
    let has_file = cursor < files.len();
    let all_staged = !files.is_empty() && files.iter().all(|f| f.staged);
    let first: Vec<(String, Style, UiAction)> = [
        has_file.then(|| {
            (
                format!("⏎ {}", tr!("tui.panel.diff")),
                normal,
                UiAction::OpenDiff(cursor),
            )
        }),
        has_file.then(|| {
            let label = if files[cursor].staged {
                tr!("tui.panel.unstage")
            } else {
                tr!("tui.panel.stage")
            };
            (format!("s {label}"), normal, UiAction::ToggleStage(cursor))
        }),
        (!files.is_empty()).then(|| {
            let label = if all_staged {
                tr!("tui.panel.unstageAll")
            } else {
                tr!("tui.panel.stageAll")
            };
            (label, normal, UiAction::StageAll(!all_staged))
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    let second = vec![
        (
            format!("✓ {}", tr!("gitCommitDialog.open")),
            button_style(app, ButtonKind::Primary, bg),
            UiAction::App(AppCommand::Commit),
        ),
        (
            format!("⎇ {}", tr!("gitBranchMenu.label")),
            normal,
            UiAction::App(AppCommand::SwitchBranch),
        ),
        (
            format!("↑ {}", tr!("gitCommitDialog.push")),
            normal,
            UiAction::App(AppCommand::Push),
        ),
    ];
    // Each row stays on one line; buttons without room fold into its overflow menu.
    // 每行保持单行；放不下的按钮收进该行的溢出菜单。
    for (row, items) in [first, second].into_iter().enumerate() {
        let y = area.y + row as u16;
        let widths: Vec<usize> = items
            .iter()
            .map(|(label, _, _)| width_of(label) + 2)
            .collect();
        let shown = fit_strip(
            &widths,
            1,
            area.width as usize,
            width_of(OVERFLOW_CHIP),
            None,
        );
        let mut x = area.x;
        let mut folded = Vec::new();
        for (index, (label, style, action)) in items.into_iter().enumerate() {
            if shown.contains(&index) {
                x += button(app, buf, x, y, &label, style, action) + 1;
            } else {
                folded.push(MenuItem {
                    label,
                    hint: String::new(),
                    action,
                    danger: false,
                });
            }
        }
        overflow_chip(app, buf, x, y, normal, folded);
    }
}

fn section(theme: &Theme, title: String, count: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            title,
            Style::new().fg(theme.muted_fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {count}"),
            Style::new().fg(mix(theme.muted_fg, theme.sidebar, 0.6)),
        ),
    ])
}

/// A heading line: `─ title` in bold muted text.
/// 标题行：粗体弱化文字的 `─ title`。
fn heading(theme: &Theme, title: String) -> Line<'static> {
    Line::from(Span::styled(
        format!("─ {title}"),
        Style::new().fg(theme.muted_fg).add_modifier(Modifier::BOLD),
    ))
}

/// `label ····· value` with the value right-aligned to `width`.
/// `label ····· value`，值右对齐到 `width`。
fn key_value(
    theme: &Theme,
    indent: usize,
    label: &str,
    value: String,
    width: usize,
) -> Line<'static> {
    let value_w = width_of(&value);
    let label = truncate(label, width.saturating_sub(indent + value_w + 2));
    let dots = width.saturating_sub(indent + width_of(&label) + value_w + 2);
    Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(label, theme.muted_text()),
        Span::styled(
            format!(" {} ", "·".repeat(dots)),
            Style::new().fg(mix(theme.border, theme.sidebar, 0.6)),
        ),
        Span::styled(value, Style::new().fg(theme.fg)),
    ])
}

fn status_color(theme: &Theme, status: GitFileStatus) -> Color {
    match status {
        GitFileStatus::Modified => theme.warning,
        GitFileStatus::Added | GitFileStatus::Untracked => theme.success,
        GitFileStatus::Deleted => theme.danger,
        GitFileStatus::Renamed => theme.info,
    }
}

fn changes(app: &App, width: usize, focused: bool) -> Vec<Row> {
    let theme = &app.theme;
    let Some(snapshot) = &app.git.snapshot else {
        return rows(vec![Line::from(Span::styled(
            format!("{} {}", spinner(app.phase()), tr!("diff.loading")),
            theme.muted_text(),
        ))]);
    };
    let mut out: Vec<Row> = Vec::new();
    let mut branch = vec![
        Span::styled("⎇ ", theme.primary_text()),
        Span::styled(snapshot.branch.clone(), theme.strong()),
    ];
    if snapshot.ahead > 0 {
        branch.push(Span::styled(
            format!("  ↑{}", snapshot.ahead),
            Style::new().fg(theme.success),
        ));
    }
    if snapshot.behind > 0 {
        branch.push(Span::styled(
            format!(" ↓{}", snapshot.behind),
            Style::new().fg(theme.warning),
        ));
    }
    out.push(Row::new(Line::from(branch)));
    if let Some(upstream) = &snapshot.upstream {
        out.push(Row::new(Line::from(Span::styled(
            format!("  ↳ {}", truncate(upstream, width.saturating_sub(4))),
            theme.muted_text(),
        ))));
    }
    if let Some(busy) = app.git.busy {
        let label = match busy {
            GitBusy::Committing => tr!("gitCommitDialog.committing"),
            GitBusy::Pushing => tr!("gitCommitDialog.pushingDetailed"),
            GitBusy::Generating => tr!("gitCommitDialog.generatingCommitMessage"),
        };
        out.push(Row::new(Line::from(Span::styled(
            format!("{} {label}", spinner(app.phase())),
            theme.primary_text(),
        ))));
    }
    out.push(Row::new(Line::default()));

    let files = app.git.ordered_files();
    if files.is_empty() {
        out.push(Row::new(Line::from(Span::styled(
            tr!("gitCommitDialog.noChanges"),
            theme.muted_text(),
        ))));
    }
    let staged = files.iter().filter(|f| f.staged).count();
    for (index, file) in files.iter().enumerate() {
        if index == 0 && staged > 0 {
            out.push(Row::new(section(theme, tr!("diff.mode.staged"), staged)));
        }
        if index == staged {
            if staged > 0 {
                out.push(Row::new(Line::default()));
            }
            out.push(Row::new(section(
                theme,
                tr!("diff.mode.unstaged"),
                files.len() - staged,
            )));
        }
        let cursor = focused && index == app.git.cursor;
        let stats = format!("+{} -{}", file.additions, file.deletions);
        let path_width = width.saturating_sub(width_of(&stats) + 5);
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name.to_string()),
            None => (String::new(), file.path.clone()),
        };
        let name = truncate(&name, path_width);
        let dir = truncate(&dir, path_width.saturating_sub(width_of(&name)));
        let pad = path_width.saturating_sub(width_of(&name) + width_of(&dir));
        let spans = vec![
            Span::styled(if cursor { "›" } else { " " }, theme.primary_text()),
            Span::styled(
                format!("{} ", file.status.letter()),
                Style::new()
                    .fg(status_color(theme, file.status))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(name, Style::new().fg(theme.fg)),
            Span::styled(
                format!(" {dir}"),
                Style::new().fg(mix(theme.muted_fg, theme.sidebar, 0.7)),
            ),
            Span::raw(" ".repeat(pad)),
            Span::styled(
                format!("+{}", file.additions),
                Style::new().fg(theme.success),
            ),
            Span::styled(
                format!(" -{}", file.deletions),
                Style::new().fg(theme.danger),
            ),
        ];
        let mut line = Line::from(spans);
        if cursor {
            line = line.style(Style::new().bg(theme.sidebar_accent));
        }
        out.push(Row::new(line).target(Target::GitFile(index)));
    }

    if !snapshot.unpushed.is_empty() {
        out.push(Row::new(Line::default()));
        out.push(Row::new(section(
            theme,
            tr!("diff.mode.commit"),
            snapshot.unpushed.len(),
        )));
        for commit in &snapshot.unpushed {
            out.push(Row::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", commit.short_sha),
                    Style::new().fg(theme.warning),
                ),
                Span::styled(
                    truncate(&commit.subject, width.saturating_sub(10)),
                    theme.base().bg(theme.sidebar),
                ),
            ])));
        }
    }
    out
}

fn progress(app: &App, width: usize) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let mut lines = Vec::new();
    let Some(session) = app.session() else {
        return lines;
    };
    let total = session.todo.len();
    let done = session
        .todo
        .iter()
        .filter(|t| t.status == TodoStatus::Completed)
        .count();
    if total == 0 {
        lines.push(Line::from(Span::styled(
            tr!("tui.progress.empty"),
            theme.muted_text(),
        )));
        return lines;
    }
    lines.push(Line::from(Span::styled(
        tr!(
            "composer.progress.progressLabel",
            current = done,
            total = total
        ),
        theme.strong(),
    )));
    lines.push(gauge(
        theme,
        done as f32 / total as f32,
        width,
        theme.success,
    ));
    lines.push(Line::default());
    for item in &session.todo {
        let (icon, style) = match item.status {
            TodoStatus::Completed => (
                "☑",
                Style::new()
                    .fg(theme.muted_fg)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            TodoStatus::InProgress => (
                spinner(app.phase()),
                Style::new().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
            TodoStatus::Pending => ("☐", Style::new().fg(theme.fg)),
        };
        let icon_style = if item.status == TodoStatus::InProgress {
            theme.primary_text()
        } else {
            theme.muted_text()
        };
        lines.extend(wrap(
            &[Span::styled(item.content.clone(), style)],
            width,
            &[Span::styled(format!("{icon} "), icon_style)],
            &[Span::raw("  ")],
        ));
    }
    lines
}

fn gauge(theme: &Theme, ratio: f32, width: usize, color: Color) -> Line<'static> {
    let cells = width.saturating_sub(1).max(4);
    let filled = ((ratio.clamp(0.0, 1.0) * cells as f32).round() as usize).min(cells);
    Line::from(vec![
        Span::styled("━".repeat(filled), Style::new().fg(color)),
        Span::styled(
            "━".repeat(cells - filled),
            Style::new().fg(mix(theme.border, theme.sidebar, 0.8)),
        ),
    ])
}

fn terminal(app: &App, width: usize, height: usize) -> Vec<Row> {
    let theme = &app.theme;
    let mut out = Vec::new();
    if app.command_sessions.is_empty() {
        out.push(Row::new(Line::from(Span::styled(
            tr!("tui.terminal.empty"),
            theme.muted_text(),
        ))));
        return out;
    }
    for (index, session) in app.command_sessions.iter().enumerate() {
        let selected = index == app.terminal_cursor;
        let (icon, color) = match (session.running, session.exit_code) {
            (true, _) => (spinner(app.phase()), theme.primary),
            (false, Some(0)) => ("✓", theme.success),
            (false, Some(_)) => ("✗", theme.danger),
            (false, None) => ("■", theme.muted_fg),
        };
        let exit = session
            .exit_code
            .map(|c| format!(" exit {c}"))
            .unwrap_or_default();
        let mut line = Line::from(vec![
            Span::styled(format!("{icon} "), Style::new().fg(color)),
            Span::styled(
                truncate(&session.title, width.saturating_sub(width_of(&exit) + 3)),
                if selected {
                    theme.strong()
                } else {
                    theme.base().bg(theme.sidebar)
                },
            ),
            Span::styled(exit, theme.muted_text()),
        ]);
        if selected {
            line = line.style(Style::new().bg(theme.sidebar_accent));
        }
        out.push(Row::new(line).target(Target::TerminalSession(index)));
    }
    let Some(session) = app.command_sessions.get(app.terminal_cursor) else {
        return out;
    };
    out.push(Row::new(Line::default()));
    let screen_bg = mix(theme.bg, Color::Rgb(0, 0, 0), 0.6);
    out.push(Row::new(
        Line::from(vec![
            Span::styled("$ ", Style::new().fg(theme.success)),
            Span::styled(
                truncate(&session.command, width.saturating_sub(2)),
                Style::new().fg(theme.fg),
            ),
        ])
        .style(Style::new().bg(screen_bg)),
    ));
    let room = height.saturating_sub(out.len() + 2);
    let skip = session.screen.len().saturating_sub(room);
    for text in &session.screen[skip..] {
        let color = if text.contains("error") {
            theme.danger
        } else if text.contains("warning") {
            theme.warning
        } else {
            theme.muted_fg
        };
        out.push(Row::new(
            Line::from(Span::styled(truncate(text, width), Style::new().fg(color)))
                .style(Style::new().bg(screen_bg)),
        ));
    }
    out
}

// ---- Memory ----

/// Dynamic TTL injections grouped by recall tag, or the static user / project profile.
/// 按召回标签分组的动态 TTL 注入，或静态的用户 / 项目画像。
fn memory(app: &App, width: usize, focused: bool) -> Vec<Row> {
    let theme = &app.theme;
    let Some(session) = app.session() else {
        return Vec::new();
    };
    let insight = &session.insight;
    let static_view = app.panel.memory_static;
    let mut out = vec![chip_row(
        app,
        0,
        width,
        vec![
            (
                format!(
                    "{} {}",
                    tr!("contextPanel.dynamic"),
                    insight.memory.dynamic_count()
                ),
                !static_view,
                false,
                UiAction::MemoryView(false),
            ),
            (
                format!(
                    "{} {}",
                    tr!("contextPanel.static"),
                    insight.memory.static_count()
                ),
                static_view,
                false,
                UiAction::MemoryView(true),
            ),
        ],
    )];
    out.push(Row::new(Line::default()));
    let cursor = app.panel.cursor(PanelTab::Memory);
    let entries = insight::memory_entries(insight, static_view);
    if entries.is_empty() {
        let empty = if static_view {
            tr!("contextPanel.staticEmpty")
        } else {
            tr!("contextPanel.dynamicEmpty")
        };
        out.extend(rows(wrap(
            &[Span::styled(empty, theme.muted_text())],
            width,
            &[],
            &[],
        )));
        return out;
    }
    let mut last_card: Option<&str> = None;
    let mut last_section: Option<(bool, &str)> = None;
    let mut last_date: Option<&str> = None;
    let mut expired_heading = false;
    for (index, entry) in entries.iter().enumerate() {
        let selected = index == cursor;
        let highlight = selected && focused;
        let target = Target::PanelItem(PanelTab::Memory, index);
        let mark = Span::styled(if selected { "› " } else { "  " }, theme.primary_text());
        let (content, meta, dim) = match entry {
            MemoryEntry::Dynamic(card, item) => {
                if last_card != Some(card.tag.as_str()) {
                    if last_card.is_some() {
                        out.push(Row::new(Line::default()));
                    }
                    last_card = Some(card.tag.as_str());
                    out.push(Row::new(Line::from(vec![
                        Span::styled("◆ ", theme.primary_text()),
                        Span::styled(card.tag.clone(), theme.strong()),
                        Span::styled(
                            format!(
                                "  {}",
                                truncate(
                                    &card.description,
                                    width.saturating_sub(width_of(&card.tag) + 4)
                                )
                            ),
                            theme.muted_text(),
                        ),
                    ])));
                }
                (
                    item.content.clone(),
                    format!(
                        "{} · {:.2} · {}",
                        insight::ttl_label(item),
                        item.score,
                        item.turn_id
                    ),
                    false,
                )
            }
            MemoryEntry::Expired(item) => {
                if !expired_heading {
                    expired_heading = true;
                    out.push(Row::new(Line::default()));
                    out.push(Row::new(Line::from(Span::styled(
                        format!("─ {}", tr!("workspaceMemory.expiredRegionTitle")),
                        Style::new().fg(theme.warning).add_modifier(Modifier::BOLD),
                    ))));
                    out.extend(rows(wrap(
                        &[Span::styled(
                            tr!("workspaceMemory.expiredRegionDescription"),
                            theme.muted_text(),
                        )],
                        width,
                        &[Span::raw("  ")],
                        &[Span::raw("  ")],
                    )));
                }
                (
                    item.content.clone(),
                    format!("{:.2} · {}", item.score, item.turn_id),
                    true,
                )
            }
            MemoryEntry::Profile(section, node) => {
                let key = (section.user, section.name.as_str());
                if last_section != Some(key) {
                    if last_section.is_some() {
                        out.push(Row::new(Line::default()));
                    }
                    last_section = Some(key);
                    last_date = None;
                    out.push(Row::new(Line::from(vec![
                        Span::styled(if section.user { "◉ " } else { "◈ " }, theme.primary_text()),
                        Span::styled(
                            truncate(&insight::profile_section_label(section), width - 2),
                            theme.strong(),
                        ),
                    ])));
                }
                if last_date != Some(node.date.as_str()) {
                    last_date = Some(node.date.as_str());
                    out.push(Row::new(Line::from(Span::styled(
                        format!("  {}", node.date),
                        Style::new().fg(mix(theme.muted_fg, theme.sidebar, 0.5)),
                    ))));
                }
                (
                    node.content.clone(),
                    format!(
                        "P{} · L{} · W{:.2} · {}",
                        node.priority,
                        node.level,
                        node.weight,
                        insight::profile_source_label(node.source)
                    ),
                    false,
                )
            }
        };
        let text_style = if dim {
            Style::new()
                .fg(theme.muted_fg)
                .add_modifier(Modifier::CROSSED_OUT)
        } else if selected {
            Style::new().fg(theme.fg).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme.fg)
        };
        let mut block: Vec<Line<'static>> = wrap(
            &[Span::styled(content, text_style)],
            width,
            &[mark],
            &[Span::raw("  ")],
        );
        // Unselected items stay at two lines so the list reads as an index.
        // 未选中条目最多两行，使列表保持索引式的紧凑。
        if !selected {
            clip(&mut block, 2, width);
        }
        block.push(Line::from(Span::styled(
            format!("  {}", truncate(&meta, width.saturating_sub(2))),
            Style::new().fg(mix(theme.muted_fg, theme.sidebar, 0.35)),
        )));
        for mut line in block {
            if highlight {
                line = line.style(Style::new().bg(theme.sidebar_accent));
            }
            out.push(Row::new(line).target(target.clone()));
        }
    }
    out
}

/// Keeps at most `max` wrapped lines, ending the last kept one with `…` within `width`.
/// 最多保留 `max` 行折行结果，并在 `width` 内以 `…` 结束最后一行。
fn clip(lines: &mut Vec<Line<'static>>, max: usize, width: usize) {
    if lines.len() <= max {
        return;
    }
    lines.truncate(max);
    let Some(last) = lines.last_mut() else {
        return;
    };
    let used: usize = last.spans.iter().map(|s| width_of(&s.content)).sum();
    let Some(span) = last.spans.last_mut() else {
        return;
    };
    let overflow = (used + 1).saturating_sub(width);
    let keep = width_of(&span.content).saturating_sub(overflow);
    let text = truncate(&span.content, keep);
    span.content = if text.ends_with('…') {
        text.into()
    } else {
        format!("{text}…").into()
    };
}

// ---- Stats ----

/// Token counts: `0` without requests, `≥` when some requests did not report usage.
/// token 计数：无请求时为 `0`，部分请求未报告用量时加 `≥`。
fn tokens(bucket: &UsageBucket, value: u64) -> String {
    if bucket.requests == 0 {
        "0".into()
    } else if bucket.partial() {
        format!("≥{}", compact_count(value))
    } else {
        compact_count(value)
    }
}

/// Timing values: `—` without samples.
/// 耗时值：无样本时为 `—`。
fn timing(value: Option<u64>) -> String {
    value.map_or_else(|| "—".into(), compact_duration)
}

fn usage_block(theme: &Theme, bucket: &UsageBucket, width: usize, out: &mut Vec<Row>) {
    let kv = |label: String, value: String| Row::new(key_value(theme, 2, &label, value, width));
    out.push(Row::new(Line::from(Span::styled(
        format!(" {}", tr!("conversation.usage.calls")),
        theme.strong(),
    ))));
    out.push(kv(
        tr!("conversation.usage.requests"),
        bucket.requests.to_string(),
    ));
    out.push(kv(
        tr!("conversation.usage.toolCalls"),
        bucket.tool_calls.to_string(),
    ));
    out.push(Row::new(Line::from(Span::styled(
        format!(" {}", tr!("conversation.usage.tokens")),
        theme.strong(),
    ))));
    for (label, value) in [
        (tr!("conversation.usage.input"), bucket.input),
        (
            tr!("conversation.usage.uncachedInput"),
            bucket.uncached_input,
        ),
        (tr!("conversation.usage.cacheRead"), bucket.cache_read),
        (tr!("conversation.usage.cacheWrite"), bucket.cache_write),
        (tr!("conversation.usage.output"), bucket.output),
        (tr!("conversation.usage.reasoning"), bucket.reasoning),
        (tr!("conversation.turnStats.totalTokens"), bucket.total()),
    ] {
        out.push(kv(label, tokens(bucket, value)));
    }
    out.push(Row::new(Line::from(Span::styled(
        format!(" {}", tr!("conversation.usage.timing")),
        theme.strong(),
    ))));
    out.push(kv(
        tr!("conversation.usage.averageFirstToken"),
        timing(bucket.average_first_token_ms()),
    ));
    out.push(kv(
        tr!("conversation.usage.averageDuration"),
        timing(bucket.average_duration_ms()),
    ));
    out.push(kv(
        tr!("conversation.usage.averageSpeed"),
        bucket
            .average_speed()
            .map_or_else(|| "—".into(), |s| format!("{s:.1} tok/s")),
    ));
    out.push(kv(
        tr!("conversation.usage.totalDuration"),
        timing((bucket.duration_samples > 0).then_some(bucket.duration_ms)),
    ));
    if bucket.partial() {
        out.extend(rows(wrap(
            &[Span::styled(
                tr!(
                    "conversation.usage.requestsDidNotReport",
                    unknownUsageCount = bucket.unknown_usage
                ),
                Style::new().fg(theme.warning),
            )],
            width,
            &[Span::raw("  ⚠ ")],
            &[Span::raw("    ")],
        )));
    }
}

/// Context window, goal, and lifetime usage of the main session and its subagents.
/// 上下文窗口、目标，以及主会话与子代理的生命周期用量。
fn stats(app: &App, width: usize) -> Vec<Row> {
    let theme = &app.theme;
    let (Some(session), Some(conversation_id)) = (app.session(), app.selected.as_deref()) else {
        return Vec::new();
    };
    let insight = &session.insight;
    let mut out = Vec::new();

    // Context window and what fills it.
    // 上下文窗口及其构成。
    let usage = session.usage;
    out.push(Row::new(Line::from(vec![
        Span::styled(
            tr!(
                "composer.contextUsage.usedPercent",
                percent = usage.percent()
            ),
            theme.strong(),
        ),
        Span::styled(
            format!(
                "  {} / {}",
                compact_count(usage.used_tokens),
                compact_count(usage.max_tokens)
            ),
            theme.muted_text(),
        ),
    ])));
    out.push(Row::new(gauge(
        theme,
        usage.percent() as f32 / 100.0,
        width,
        theme.usage_color(usage.percent()),
    )));
    let biggest = insight
        .breakdown
        .iter()
        .map(|s| s.tokens)
        .max()
        .unwrap_or(0);
    for slice in &insight.breakdown {
        let label = fit(&insight::context_part_label(slice.part), 10);
        let count = compact_count(slice.tokens);
        let bar_room = width.saturating_sub(10 + 8);
        let filled = if biggest == 0 {
            0
        } else {
            ((slice.tokens as f32 / biggest as f32) * bar_room as f32).ceil() as usize
        };
        out.push(Row::new(Line::from(vec![
            Span::styled(label, theme.muted_text()),
            Span::styled(" ", Style::default()),
            Span::styled(
                "▆".repeat(filled),
                Style::new().fg(mix(theme.info, theme.sidebar, 0.2)),
            ),
            Span::raw(" ".repeat(bar_room - filled.min(bar_room))),
            Span::styled(format!("{count:>7}"), Style::new().fg(theme.fg)),
        ])));
    }
    let cache = insight
        .lifetime
        .main
        .cache_hit_rate()
        .map_or_else(|| "—".into(), |r| format!("{:.0}%", r * 100.0));
    out.push(Row::new(key_value(
        theme,
        0,
        &tr!("composer.contextUsage.cacheHitRate"),
        cache,
        width,
    )));
    out.push(Row::new(Line::default()));

    // Goal.
    // 目标。
    out.push(Row::new(heading(theme, tr!("composer.goal"))));
    match &insight.goal {
        None => {
            out.push(Row::new(Line::from(Span::styled(
                format!("  {}", tr!("toolDomain.goal.unset")),
                theme.muted_text(),
            ))));
            out.push(chip_row(
                app,
                2,
                width,
                vec![(
                    format!("＋ {}", tr!("composer.goal")),
                    false,
                    false,
                    insight::edit_goal(conversation_id, None),
                )],
            ));
        }
        Some(goal) => {
            let color = match goal.status {
                GoalStatus::Active => theme.primary,
                GoalStatus::Completed => theme.success,
                GoalStatus::Paused => theme.muted_fg,
                _ => theme.warning,
            };
            let icon = if goal.status == GoalStatus::Active {
                spinner(app.phase())
            } else {
                "◎"
            };
            out.push(Row::new(Line::from(vec![
                Span::styled(format!("  {icon} "), Style::new().fg(color)),
                Span::styled(
                    insight::goal_status_label(goal.status),
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                ),
            ])));
            out.extend(rows(wrap(
                &[Span::styled(
                    goal.objective.clone(),
                    Style::new().fg(theme.fg),
                )],
                width,
                &[Span::raw("  ")],
                &[Span::raw("  ")],
            )));
            out.push(Row::new(key_value(
                theme,
                2,
                &tr!("composer.goal.duration"),
                compact_duration(goal.elapsed_ms),
                width,
            )));
            let spent = goal.usage.total();
            let value = match goal.budget {
                Some(budget) => {
                    format!("{} / {}", tokens(&goal.usage, spent), compact_count(budget))
                }
                None => tokens(&goal.usage, spent),
            };
            out.push(Row::new(key_value(
                theme,
                2,
                &tr!("composer.goal.usage"),
                value,
                width,
            )));
            if let Some(budget) = goal.budget.filter(|b| *b > 0) {
                let ratio = spent as f32 / budget as f32;
                let mut bar = gauge(
                    theme,
                    ratio,
                    width.saturating_sub(2),
                    theme.usage_color((ratio * 100.0).min(100.0) as u8),
                );
                bar.spans.insert(0, Span::raw("  "));
                out.push(Row::new(bar));
            }
            out.push(chip_row(
                app,
                2,
                width,
                insight::goal_actions(conversation_id, goal)
                    .into_iter()
                    .map(|(label, danger, action)| (label, false, danger, action))
                    .collect(),
            ));
        }
    }
    out.push(Row::new(Line::default()));

    // Lifetime usage of the main session.
    // 主会话的生命周期用量。
    let lifetime = &insight.lifetime;
    out.push(Row::new(heading(
        theme,
        tr!("conversation.usage.mainSessionUsage"),
    )));
    usage_block(theme, &lifetime.main, width, &mut out);
    out.push(Row::new(Line::default()));

    // Subagents: one line each, then their combined lifetime usage.
    // 子代理：每个一行，然后是合计的生命周期用量。
    out.push(Row::new(heading(
        theme,
        tr!(
            "conversation.usage.subagentUsageAgents",
            childCount = lifetime.children.len()
        ),
    )));
    if lifetime.children.is_empty() {
        out.push(Row::new(Line::from(Span::styled(
            format!("  {}", tr!("composer.contextUsage.noData")),
            theme.muted_text(),
        ))));
    } else {
        for (name, bucket) in &lifetime.children {
            out.push(Row::new(key_value(
                theme,
                2,
                name,
                format!(
                    "{} · {}",
                    tr!("conversation.stats.providerCalls", count = bucket.requests),
                    tokens(bucket, bucket.total())
                ),
                width,
            )));
        }
        out.push(Row::new(Line::from(Span::styled(
            format!(" {}", tr!("conversation.usage.childLifetimeUsage")),
            theme.muted_text().add_modifier(Modifier::BOLD),
        ))));
        usage_block(theme, &lifetime.children_total(), width, &mut out);
    }
    if lifetime.history_incomplete {
        out.push(Row::new(Line::default()));
        out.extend(rows(wrap(
            &[Span::styled(
                tr!("conversation.usage.someHistoryIsUnavailable"),
                theme.muted_text(),
            )],
            width,
            &[Span::raw("ⓘ ")],
            &[Span::raw("  ")],
        )));
    }
    out
}

// ---- Agents ----

fn agent_look(app: &App, state: AgentState) -> (&'static str, Color) {
    let theme = &app.theme;
    match state {
        AgentState::Running => (spinner(app.phase()), theme.primary),
        AgentState::Standby => ("●", theme.success),
        AgentState::Stopped => ("■", theme.warning),
        AgentState::Failed => ("✗", theme.danger),
        AgentState::Closed => ("○", theme.muted_fg),
    }
}

/// Status summary, then one block per agent; the selected agent expands with its queue, usage,
/// recent activity, result and controls.
/// 状态汇总，然后每个代理一块；选中的代理展开其队列、用量、最近活动、结果与控制。
fn agents(app: &App, width: usize, focused: bool) -> Vec<Row> {
    let theme = &app.theme;
    let (Some(session), Some(conversation_id)) = (app.session(), app.selected.as_deref()) else {
        return Vec::new();
    };
    let agents = &session.agents;
    let mut out = Vec::new();
    if agents.is_empty() {
        out.push(Row::new(Line::from(Span::styled(
            tr!("conversation.subagent.detail.noOpenAgents"),
            theme.muted_text(),
        ))));
        return out;
    }
    let mut summary = Vec::new();
    for state in AgentState::ALL {
        let count = agents.iter().filter(|a| a.state == state).count();
        if count == 0 {
            continue;
        }
        let (_, color) = agent_look(app, state);
        if !summary.is_empty() {
            summary.push(Span::raw("  "));
        }
        summary.push(Span::styled("● ", Style::new().fg(color)));
        summary.push(Span::styled(
            format!("{} {count}", insight::agent_state_label(state)),
            theme.muted_text(),
        ));
    }
    out.extend(rows(wrap(&summary, width, &[], &[])));
    out.push(Row::new(Line::default()));

    let cursor = app.panel.cursor(PanelTab::Agents);
    for (index, agent) in agents.iter().enumerate() {
        let selected = index == cursor;
        let target = Target::PanelItem(PanelTab::Agents, index);
        let (icon, color) = agent_look(app, agent.state);
        let count = compact_count(agent.usage.total());
        let state = insight::agent_state_label(agent.state);
        let name_room = width.saturating_sub(width_of(&state) + width_of(&count) + 7);
        let name = truncate(&agent.name, name_room);
        let pad = name_room.saturating_sub(width_of(&name));
        let mut head = Line::from(vec![
            Span::styled(if selected { "›" } else { " " }, theme.primary_text()),
            Span::styled(format!("{icon} "), Style::new().fg(color)),
            Span::styled(
                name,
                if agent.state == AgentState::Closed {
                    theme.muted_text()
                } else {
                    theme.strong()
                },
            ),
            Span::raw(" ".repeat(pad)),
            Span::styled(format!(" {state}"), Style::new().fg(color)),
            Span::styled(format!("  {count}"), theme.muted_text()),
        ]);
        if selected && focused {
            head = head.style(Style::new().bg(theme.sidebar_accent));
        }
        out.push(Row::new(head).target(target.clone()));
        let task_lines = wrap(
            &[Span::styled(agent.task.clone(), theme.muted_text())],
            width,
            &[Span::raw("   ")],
            &[Span::raw("   ")],
        );
        let take = if selected { task_lines.len() } else { 1 };
        for line in task_lines.into_iter().take(take) {
            out.push(Row::new(line).target(target.clone()));
        }
        if selected {
            agent_detail(app, conversation_id, agent, width, &target, &mut out);
        }
        out.push(Row::new(Line::default()));
    }
    out
}

fn agent_detail(
    app: &App,
    conversation_id: &str,
    agent: &AgentInfo,
    width: usize,
    target: &Target,
    out: &mut Vec<Row>,
) {
    let theme = &app.theme;
    let mut push = |line: Line<'static>| out.push(Row::new(line).target(target.clone()));
    push(key_value(
        theme,
        3,
        &tr!("conversation.subagent.detail.agent"),
        agent.profile.clone(),
        width,
    ));
    let usage = &agent.usage;
    push(key_value(
        theme,
        3,
        &tr!("conversation.subagent.detail.usage"),
        format!(
            "{} · {} · {}",
            tr!("conversation.stats.providerCalls", count = usage.requests),
            tr!("conversation.stats.toolCalls", count = usage.tool_calls),
            tokens(usage, usage.total())
        ),
        width,
    ));
    for message in &agent.pending {
        let kind = if message.steering {
            tr!("conversation.subagent.queue.steering")
        } else {
            tr!("conversation.subagent.queue.followup")
        };
        for line in wrap(
            &[Span::styled(
                message.text.clone(),
                Style::new().fg(theme.fg),
            )],
            width,
            &[
                Span::raw("   "),
                Span::styled(format!("⤷ {kind} "), Style::new().fg(theme.warning)),
            ],
            &[Span::raw("     ")],
        ) {
            push(line);
        }
    }
    let recent = agent.activity.len().saturating_sub(3);
    for activity in &agent.activity[recent..] {
        push(Line::from(Span::styled(
            format!("   · {}", truncate(activity, width.saturating_sub(5))),
            Style::new().fg(mix(theme.muted_fg, theme.sidebar, 0.3)),
        )));
    }
    if let Some(result) = &agent.result {
        let mut lines = wrap(
            &[Span::styled(result.clone(), Style::new().fg(theme.fg))],
            width,
            &[
                Span::raw("   "),
                Span::styled(
                    format!("{} ", tr!("conversation.subagent.detail.result")),
                    Style::new().fg(theme.success).add_modifier(Modifier::BOLD),
                ),
            ],
            &[Span::raw("   ")],
        );
        clip(&mut lines, 3, width);
        for line in lines {
            push(line);
        }
    }
    let mut chips = vec![(
        tr!("agents.viewDetails"),
        false,
        false,
        UiAction::PanelOpen(PanelTab::Agents, app.panel.cursor(PanelTab::Agents)),
    )];
    chips.extend(
        insight::agent_actions(conversation_id, agent)
            .into_iter()
            .map(|(label, danger, action)| (label, false, danger, action)),
    );
    out.push(chip_row(app, 3, width, chips));
}
