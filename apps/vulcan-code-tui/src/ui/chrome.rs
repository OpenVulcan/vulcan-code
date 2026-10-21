// Window chrome: splash screen, header with area tabs and status bar with buttons.
// 窗口框架：启动画面、带区域标签的标题栏、带按钮的状态栏与提示。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::brand;
use super::text::{OVERFLOW_CHIP, compact_count, shimmer, spinner, truncate, width_of};
use super::theme::{Theme, mix};
use crate::app::action::{self, UiAction};
use crate::app::commands::AppCommand;
use crate::app::hit::Target;
use crate::app::overlay::{Menu, MenuItem};
use crate::app::pages::Area;
use crate::app::{App, Focus};
use crate::backend::ConnectionState;
use crate::tr;

/// Full-screen splash shown until the first workbench snapshot arrives.
/// 首个工作台快照到达前显示的全屏启动画面。
pub fn splash(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    buf.set_style(area, theme.base());
    let phase = app.phase();
    let center = |w: usize| area.x + area.width.saturating_sub(w as u16) / 2;
    let top = area.y + area.height.saturating_sub(5) / 2;
    let y = top;
    let wordmark = brand::wordmark(theme);
    let w = wordmark.width();
    wordmark.render(
        Rect {
            x: center(w),
            y,
            width: (w as u16).min(area.width),
            height: 1,
        },
        buf,
    );
    let status = tr!("startup.connectingService");
    let mut spans = vec![Span::styled(
        format!("{} ", spinner(phase)),
        theme.primary_text(),
    )];
    spans.extend(shimmer(&status, theme.muted_fg, theme.fg, phase));
    let w = width_of(&status) + 2;
    Line::from(spans).render(
        Rect {
            x: center(w),
            y: y + 2,
            width: (w as u16).min(area.width),
            height: 1,
        },
        buf,
    );
    if app.backend.is_mock {
        let badge = format!(" {} ", tr!("tui.mock.badge"));
        buf.set_string(
            center(width_of(&badge)),
            y + 4,
            &badge,
            Style::new().fg(theme.warning),
        );
    }
}

/// Width of the wordmark cell: the wordmark with one column of padding on each side.
/// 品牌字标单元格宽度：字标两侧各留一列空白。
pub fn brand_width(app: &App) -> u16 {
    brand::wordmark(&app.theme).width() as u16 + 2
}

/// Git and connection state shown at the right end of the top row.
/// 顶行右端显示的 Git 与连接状态。
pub fn header_state(app: &App) -> Line<'static> {
    let theme = &app.theme;
    let mut right = Vec::new();
    if let Some(snapshot) = app.git.repository() {
        right.push(Span::styled(
            format!("⎇ {}", snapshot.branch),
            Style::new().fg(theme.teal),
        ));
        if snapshot.ahead > 0 {
            right.push(Span::styled(
                format!(" ↑{}", snapshot.ahead),
                Style::new().fg(theme.success),
            ));
        }
        right.push(Span::raw("  "));
    }
    let (dot, label, color) = match app.connection {
        ConnectionState::Connected => ("●", app.backend.label.clone(), theme.success),
        ConnectionState::Connecting => (
            spinner(app.phase()),
            tr!("tui.connection.connecting"),
            theme.warning,
        ),
        ConnectionState::Reconnecting => (
            spinner(app.phase()),
            tr!("tui.connection.reconnecting"),
            theme.warning,
        ),
    };
    right.push(Span::styled(format!("{dot} "), Style::new().fg(color)));
    right.push(Span::styled(label, theme.muted_text()));
    if app.backend.is_mock {
        right.push(Span::raw(" "));
        right.push(Span::styled(
            format!(" {} ", tr!("tui.mock.badge")),
            Style::new().fg(theme.primary_fg).bg(theme.warning),
        ));
    }
    right.push(Span::raw(" "));
    Line::from(right)
}

/// The top row split over the side columns: the wordmark above the activity bar and the state
/// above the session panel, so the sidebar and conversation reach the top row. Each cell continues
/// the border of the column below it.
/// 拆分到两侧列的顶行：品牌字标位于活动栏上方，状态位于会话面板上方，使侧栏与对话列直达顶行。
/// 每个单元格延续其下方列的边框。
pub fn header_cells(app: &App, brand: Rect, state: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let style = Style::new().bg(theme.sidebar).fg(theme.sidebar_fg);
    buf.set_style(brand, style);
    buf.set_style(state, style);
    brand::wordmark(theme).render(
        Rect {
            x: brand.x + 1,
            width: brand.width.saturating_sub(2),
            ..brand
        },
        buf,
    );
    if brand.width > 0 {
        buf.set_string(
            brand.right() - 1,
            brand.y,
            "│",
            Style::new().fg(theme.sidebar_border),
        );
    }
    if state.width == 0 {
        return;
    }
    let panel_border = if app.focus == Focus::Panel {
        theme.primary
    } else {
        theme.sidebar_border
    };
    buf.set_string(state.x, state.y, "│", Style::new().fg(panel_border));
    let line = header_state(app);
    let width = (line.width() as u16).min(state.width.saturating_sub(1));
    line.render(
        Rect {
            x: state.right() - width,
            width,
            ..state
        },
        buf,
    );
}

/// Top bar: wordmark, then Git and connection state on the right. `switcher` adds the area switcher
/// button used when the activity bar has no room; it takes precedence over the right-hand state.
/// 顶栏：品牌字标，右侧为 Git 与连接状态。`switcher` 为真时加入区域切换按钮，供活动栏放不下时使用；
/// 其优先级高于右侧状态。
pub fn header(app: &App, area: Rect, buf: &mut Buffer, switcher: bool) {
    let theme = &app.theme;
    let bg = theme.sidebar;
    buf.set_style(area, Style::new().bg(bg).fg(theme.sidebar_fg));

    let right_line = header_state(app);
    let wordmark = brand::wordmark(theme);
    let brand_width = brand_width(app);
    // The current area plus a switcher that opens the area picker.
    // 当前区域加一个打开区域选择器的切换按钮。
    let switch_text = switcher.then(|| format!(" {} ▾ F2 ", app.area.label()));
    let switch_width = switch_text.as_deref().map_or(0, |t| width_of(t) as u16 + 1);
    let right_width = right_line.width() as u16;
    let right_shown = brand_width + 1 + switch_width + right_width <= area.width;
    if right_shown {
        right_line.render(
            Rect {
                x: area.right() - right_width,
                width: right_width,
                ..area
            },
            buf,
        );
    }

    wordmark.render(
        Rect {
            x: area.x + 1,
            width: (brand_width - 1).min(area.width.saturating_sub(1)),
            ..area
        },
        buf,
    );

    let Some(text) = switch_text else {
        return;
    };
    let x = area.x + brand_width + 1;
    let limit = if right_shown {
        area.right().saturating_sub(right_width + 1)
    } else {
        area.right()
    };
    let text = truncate(&text, limit.saturating_sub(x) as usize);
    let w = width_of(&text) as u16;
    buf.set_string(
        x,
        area.y,
        &text,
        Style::new().fg(theme.primary_fg).bg(theme.primary),
    );
    app.hits.add(
        Rect {
            x,
            width: w,
            ..area
        },
        Target::Do(UiAction::OpenAreaPicker),
    );
}

fn focus_hint(app: &App) -> String {
    match app.focus {
        Focus::Composer => tr!("tui.hint.composer"),
        Focus::Transcript => tr!("tui.hint.transcript"),
        Focus::Sidebar => tr!("tui.hint.sidebar"),
        Focus::Panel => tr!("tui.hint.panel"),
        Focus::Ask
            if app
                .session()
                .and_then(|s| s.pending_question.as_ref())
                .is_some_and(|q| q.approval.is_some()) =>
        {
            tr!("tui.hint.approval")
        }
        Focus::Ask => tr!("tui.hint.ask"),
        Focus::AreaNav => tr!("tui.hint.nav"),
        Focus::AreaPage => tr!("tui.hint.page"),
    }
}

/// Status bar: focus badge and key hints on the left; context usage and buttons on the right.
/// 状态栏：左侧为焦点标记与按键提示；右侧为上下文用量与按钮。
/// Cells of the compact context gauge shown in the status bar and the session bar.
/// 状态栏与会话栏中紧凑上下文进度条的格数。
const GAUGE_CELLS: usize = 5;

/// Compact context gauge `▰▰▱▱▱` colored by usage.
/// 按用量着色的紧凑上下文进度条 `▰▰▱▱▱`。
pub fn usage_gauge(theme: &Theme, percent: u8) -> Vec<Span<'static>> {
    let filled = (percent as usize * GAUGE_CELLS)
        .div_ceil(100)
        .min(GAUGE_CELLS);
    vec![
        Span::styled(
            "▰".repeat(filled),
            Style::new().fg(theme.usage_color(percent)),
        ),
        Span::styled(
            "▱".repeat(GAUGE_CELLS - filled),
            Style::new().fg(theme.border),
        ),
    ]
}

/// Bottom status bar. `show_usage` adds the context gauge, left out while the session bar above
/// the composer already shows it.
/// 底部状态栏。`show_usage` 时显示上下文进度条；输入框上方的会话栏已显示时省略。
pub fn status_bar(app: &App, area: Rect, buf: &mut Buffer, show_usage: bool) {
    let theme = &app.theme;
    let bg = theme.sidebar;
    buf.set_style(area, Style::new().bg(bg).fg(theme.muted_fg));
    let focus_name = match app.focus {
        Focus::Composer => tr!("composer.aria.input"),
        Focus::Transcript => tr!("area.chat"),
        Focus::Sidebar => tr!("sidebar.projects"),
        Focus::Panel => tr!("tui.panel.title"),
        Focus::Ask => tr!("tui.focus.ask"),
        Focus::AreaNav => tr!("tui.focus.nav"),
        Focus::AreaPage => app.area.label(),
    };

    // Buttons, right to left: palette, help, areas, theme, language.
    // 按钮（从右到左）：命令面板、帮助、区域、主题、语言。
    let key_style = Style::new().fg(theme.fg).bg(theme.accent);
    let buttons: Vec<(String, Style, UiAction)> = vec![
        (
            format!(" {} ", action::locale_mark()),
            key_style,
            UiAction::OpenLanguagePicker,
        ),
        (
            " ◐ ".to_string(),
            key_style,
            UiAction::App(AppCommand::ToggleTheme),
        ),
        (
            format!(" F2 {} ", tr!("tui.area.short")),
            key_style,
            UiAction::OpenAreaPicker,
        ),
        (
            format!(" F1 {} ", tr!("tui.help.short")),
            key_style,
            UiAction::App(AppCommand::Help),
        ),
        (
            " Ctrl+P ".to_string(),
            key_style,
            UiAction::App(AppCommand::Palette),
        ),
    ];
    let mut x = area.right();
    x = x.saturating_sub(1);
    for (text, style, action) in buttons.into_iter().rev() {
        let w = width_of(&text) as u16;
        if x < area.x + w + 20 {
            break;
        }
        x -= w;
        buf.set_string(x, area.y, &text, style);
        app.hits.add(
            Rect {
                x,
                y: area.y,
                width: w,
                height: 1,
            },
            Target::Do(action),
        );
        x = x.saturating_sub(1);
    }

    let mut usage_spans = Vec::new();
    if let Some(session) = app.session()
        && app.area == Area::Chat
        && show_usage
    {
        let usage = session.usage;
        if usage.max_tokens > 0 {
            let percent = usage.percent();
            usage_spans.extend(usage_gauge(theme, percent));
            usage_spans.push(Span::styled(
                format!(" {percent}% · {} ", compact_count(usage.used_tokens)),
                theme.muted_text(),
            ));
        }
    }
    let usage = Line::from(usage_spans);
    let usage_width = usage.width() as u16;
    if usage_width > 0 && x > area.x + usage_width + 20 {
        x -= usage_width;
        usage.render(
            Rect {
                x,
                y: area.y,
                width: usage_width,
                height: 1,
            },
            buf,
        );
    }

    // The buttons keep only 20 columns for the left side, so a long focus name is shortened with
    // an ellipsis instead of being cut mid-word at the edge.
    // 按钮只为左侧保留 20 列，因此过长的焦点名称以省略号缩短，而不是在边缘处从词中截断。
    let left_room = x.saturating_sub(area.x + 1) as usize;
    let mut left = vec![
        Span::styled(
            format!(" {} ", truncate(&focus_name, left_room.saturating_sub(2))),
            Style::new()
                .fg(theme.primary_fg)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    if app.quit_armed_until.is_some() {
        left.push(Span::styled(
            tr!("tui.quit.confirm"),
            Style::new().fg(theme.warning),
        ));
    } else {
        let used: usize = left.iter().map(|s| width_of(&s.content)).sum();
        let room = left_room.saturating_sub(used + 1);
        left.push(Span::styled(
            truncate(&focus_hint(app), room),
            Style::new().fg(mix(theme.muted_fg, bg, 0.85)),
        ));
    }
    Line::from(left).render(
        Rect {
            width: left_room as u16,
            ..area
        },
        buf,
    );
}

/// A clickable chip `label` at `(x, y)`; returns its width. Shared by pages, dialogs and panels.
/// 在 `(x, y)` 处绘制可点击的按钮 `label` 并返回其宽度。页面、对话框与面板共用。
pub fn button(
    app: &App,
    buf: &mut Buffer,
    x: u16,
    y: u16,
    label: &str,
    style: Style,
    action: UiAction,
) -> u16 {
    let text = format!(" {label} ");
    let w = width_of(&text) as u16;
    buf.set_string(x, y, &text, style);
    app.hits.add(
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
        Target::Do(action),
    );
    w
}

/// Menu listing the items a one-line strip folded away.
/// 列出单行按钮条收起项目的菜单。
pub fn overflow_menu(items: Vec<MenuItem>) -> Menu {
    Menu {
        title: tr!("activityRail.more"),
        items,
        cursor: 0,
        anchor: None,
    }
}

/// Draws the overflow chip at `(x, y)` when `items` is not empty; a click opens them as a menu.
/// `items` 非空时在 `(x, y)` 绘制溢出按钮；点击后以菜单打开这些项目。
/// Returns the drawn width, zero when nothing was folded.
/// 返回绘制宽度；没有收起项目时为零。
pub fn overflow_chip(
    app: &App,
    buf: &mut Buffer,
    x: u16,
    y: u16,
    style: Style,
    items: Vec<MenuItem>,
) -> u16 {
    if items.is_empty() {
        return 0;
    }
    let w = width_of(OVERFLOW_CHIP) as u16;
    buf.set_string(x, y, OVERFLOW_CHIP, style);
    app.hits.add(
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
        Target::Do(UiAction::OpenMenu(Box::new(overflow_menu(items)))),
    );
    w
}

/// Button styles: primary (filled accent), normal (subtle), danger.
/// 按钮样式：主要（强调色填充）、普通（低调）、危险。
pub fn button_style(app: &App, kind: ButtonKind, bg: Color) -> Style {
    let theme = &app.theme;
    match kind {
        ButtonKind::Primary => Style::new()
            .fg(theme.primary_fg)
            .bg(theme.primary)
            .add_modifier(Modifier::BOLD),
        ButtonKind::Normal => Style::new().fg(theme.fg).bg(theme.control_bg()),
        ButtonKind::Danger => Style::new()
            .fg(theme.primary_fg)
            .bg(theme.danger)
            .add_modifier(Modifier::BOLD),
        ButtonKind::Focused => Style::new()
            .fg(theme.fg)
            .bg(mix(theme.primary, bg, 0.35))
            .add_modifier(Modifier::BOLD),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Normal,
    Danger,
    Focused,
}
