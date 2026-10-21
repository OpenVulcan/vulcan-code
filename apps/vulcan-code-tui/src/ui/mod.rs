// Views. Everything here reads `App` and draws; state changes only record hit areas for input.
// 视图。此处只读取 `App` 并绘制；唯一的状态写入是为输入处理记录命中区域。
//
// Layout (wide):
// 布局（宽屏）：
//
//   ┌ header ─────────────────────────────────────────────────────────────────┐
//   │ ▤ │ sidebar │ transcript            [thinking box ⇢ float] │ panel     │
//   │ ◫ │         │ [thinking box ⇢ docked]                       │ Changes…  │
//   │ … │         │ [ask panel] [queued prompts]                  │           │
//   │   │         │ session bar                                   │           │
//   │ ⚙ │         │ composer                                      │           │
//   └ status bar ─────────────────────────────────────────────────────────────┘
//
// The activity bar on the left switches areas in every area; system areas replace the sidebar,
// conversation and panel with their own navigation and page.
// 左侧活动栏在所有区域中切换区域；系统区域以自身的导航与页面取代侧栏、对话与面板。
//
// The sidebar is either the full list or a narrow rail. A rail, or a full list without room,
// opens the list as a drawer over the conversation while the sidebar has focus; the panel does the
// same when the terminal is too narrow. Below the rail's own minimum width the sidebar is a drawer
// only.
// 侧栏为完整列表或窄栏。窄栏，或放不下的完整列表，在侧栏获得焦点时以抽屉形式覆盖对话区打开；
// 终端过窄时面板同样如此。宽度连窄栏都放不下时，侧栏只以抽屉形式出现。

mod activity;
pub mod age;
mod answer_components;
mod ask;
pub mod brand;
mod chrome;
mod composer;
mod dialogs;
mod history;
pub mod markdown;
mod mermaid;
mod notice;
mod overlays;
mod pages;
mod panel;
mod session_bar;
mod sidebar;
pub mod text;
pub mod theme;
pub mod thinking;
mod transcript;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::action::UiAction;
use crate::app::hit::Target;
use crate::app::pages::Area;
use crate::app::{App, Focus, SidebarMode, ThinkingPlacement};
use crate::backend::ConnectionState;
use crate::model::ReasoningEffort;
use crate::tr;

/// Narrowest sidebar and session panel; both grow on wide terminals so titles and paths fit.
/// 侧栏与会话面板的最小宽度；两者在宽终端上增宽，使标题与路径完整显示。
const SIDEBAR_WIDTH: u16 = 30;
const PANEL_WIDTH: u16 = 46;
const CENTER_MIN_WIDTH: u16 = 60;

/// Sidebar width for a body `width`: a seventh of it, never below [`SIDEBAR_WIDTH`].
/// 主体宽度为 `width` 时的侧栏宽度：取七分之一，不低于 [`SIDEBAR_WIDTH`]。
fn sidebar_width(width: u16) -> u16 {
    SIDEBAR_WIDTH.max(width / 7)
}

/// The first row of `area`.
/// `area` 的第一行。
fn top_row(area: Rect) -> Rect {
    Rect {
        height: area.height.min(1),
        ..area
    }
}

/// `area` without its first row.
/// 去掉第一行后的 `area`。
fn below_top_row(area: Rect) -> Rect {
    Rect {
        y: area.y + area.height.min(1),
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// Session panel width for a body `width`: a quarter of it, never below [`PANEL_WIDTH`].
/// 主体宽度为 `width` 时的会话面板宽度：取四分之一，不低于 [`PANEL_WIDTH`]。
fn panel_width(width: u16) -> u16 {
    PANEL_WIDTH.max(width / 4)
}

pub fn effort_label(effort: ReasoningEffort) -> String {
    match effort {
        ReasoningEffort::Off => tr!("composer.reasoning.off"),
        ReasoningEffort::Low => tr!("composer.reasoning.low"),
        ReasoningEffort::Medium => tr!("composer.reasoning.medium"),
        ReasoningEffort::High => tr!("composer.reasoning.high"),
        ReasoningEffort::Max => tr!("composer.reasoning.max"),
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let buf = frame.buffer_mut();
    buf.set_style(area, app.theme.base());
    app.hits.reset();

    if app.connection != ConnectionState::Connected && app.conversations.is_empty() {
        chrome::splash(app, area, buf);
        return;
    }

    let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    let in_chat = app.area == Area::Chat;
    // The activity bar takes the left edge; everything else lays out in the remaining columns.
    // 活动栏占据左边缘；其余内容在剩余列中排布。
    let workbench = sidebar_width(main.width) + CENTER_MIN_WIDTH + panel_width(main.width);
    let activity_width = activity::width(main.width, workbench, CENTER_MIN_WIDTH);
    let [activity_column, columns] =
        Layout::horizontal([Constraint::Length(activity_width), Constraint::Fill(1)]).areas(main);

    // The project and conversation list belongs to the workbench; system areas show only their
    // own navigation.
    // 项目与会话列表只属于工作台；系统区域只显示自身的导航。
    let body = below_top_row(columns);
    let sidebar_width = sidebar_width(body.width);
    let panel_width = panel_width(body.width);
    let sidebar_inline = in_chat
        && app.sidebar == SidebarMode::Expanded
        && body.width >= CENTER_MIN_WIDTH + sidebar_width;
    let rail_inline =
        in_chat && !sidebar_inline && body.width >= CENTER_MIN_WIDTH + sidebar::RAIL_WIDTH;
    let used = if sidebar_inline {
        sidebar_width
    } else if rail_inline {
        sidebar::RAIL_WIDTH
    } else {
        0
    };
    // The session panel belongs to the workbench as well.
    // 会话面板同样只属于工作台。
    let panel_inline =
        in_chat && app.panel_visible && body.width >= CENTER_MIN_WIDTH + panel_width + used;
    let [sidebar_area, center, panel_area] = Layout::horizontal([
        Constraint::Length(used),
        Constraint::Fill(1),
        Constraint::Length(if panel_inline { panel_width } else { 0 }),
    ])
    .areas(columns);
    // With the labeled activity bar and the panel beside the conversation, the top row only holds
    // the wordmark above the activity bar and the state above the panel; the sidebar and
    // conversation take the top row. Otherwise one header row spans the window.
    // 标签式活动栏与会话面板同时显示时，顶行只在活动栏上方放品牌字标、在面板上方放状态，侧栏与
    // 对话列占用顶行；否则由一整行标题栏横跨窗口。
    let split_header = activity_width >= chrome::brand_width(app)
        && panel_inline
        && chrome::header_state(app).width() < panel_width as usize;
    let (sidebar_area, center) = if split_header {
        chrome::header_cells(app, top_row(activity_column), top_row(panel_area), buf);
        (sidebar_area, center)
    } else {
        chrome::header(app, top_row(main), buf, activity_width == 0);
        (below_top_row(sidebar_area), below_top_row(center))
    };
    let panel_area = below_top_row(panel_area);
    if activity_width > 0 {
        activity::render(app, below_top_row(activity_column), buf);
    }
    // The session bar's visibility follows the conversation column's height.
    // 会话栏是否显示取决于对话列高度。
    let session_bar = in_chat && session_bar::visible(app, center.height);
    chrome::status_bar(app, status, buf, !session_bar);

    if sidebar_inline {
        sidebar::render(app, sidebar_area, buf);
    } else if rail_inline {
        sidebar::render_rail(app, sidebar_area, buf);
    }
    if panel_inline {
        panel::render(app, panel_area, buf);
    }
    let composer_area = if in_chat {
        Some(draw_center(app, center, session_bar, buf))
    } else {
        pages::render(app, center, buf);
        None
    };

    // Drawers for narrow terminals.
    // 窄终端的抽屉。
    if in_chat && !sidebar_inline && app.focus == Focus::Sidebar {
        let drawer = Rect {
            width: sidebar_width.min(body.width),
            ..body
        };
        text::clear_area(drawer, buf);
        sidebar::render(app, drawer, buf);
    }
    if in_chat && !panel_inline && app.panel_visible && app.focus == Focus::Panel {
        let width = panel_width.min(body.width);
        let drawer = Rect {
            x: body.right() - width,
            width,
            ..body
        };
        text::clear_area(drawer, buf);
        panel::render(app, drawer, buf);
    }

    if let Some(composer_area) = composer_area {
        composer::render_slash_popup(app, composer_area, buf);
    }
    if app.overlay.is_some() {
        // Clicks outside the dialog close it; the dialog registers its own targets on top.
        // 点击对话框外部即关闭；对话框在其上方登记自身的目标。
        app.hits.add(area, Target::Backdrop);
        overlays::render(app, area, buf);
    }
    notice::render(app, body, buf);
}

/// Draws the conversation column and returns the composer area.
/// 绘制对话列并返回输入框区域。
fn draw_center(
    app: &mut App,
    center: Rect,
    session_bar: bool,
    buf: &mut ratatui::buffer::Buffer,
) -> Rect {
    let column = Rect {
        x: center.x + 1,
        width: center.width.saturating_sub(2),
        ..center
    };
    let view = thinking::view(app);
    let docked = view.is_some() && app.thinking_placement == ThinkingPlacement::Docked;
    let pending = app.session().and_then(|s| s.pending_question.clone());
    let ask_height = pending
        .as_ref()
        .map_or(0, |p| ask::height(app, p, column.width, column.height / 2));
    let [
        title_area,
        transcript_area,
        thinking_area,
        ask_area,
        queued_area,
        bar_area,
        composer_area,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(if docked { thinking::DOCKED_HEIGHT } else { 0 }),
        Constraint::Length(ask_height),
        Constraint::Length(composer::queued_height(app)),
        Constraint::Length(u16::from(session_bar)),
        Constraint::Length(composer::height(
            app,
            column.width,
            composer::max_text_rows(column.height),
        )),
    ])
    .areas(column);

    title_bar(app, title_area, buf);
    transcript::render(app, transcript_area, buf);
    if let Some(view) = &view {
        match app.thinking_placement {
            ThinkingPlacement::Docked => thinking::render(app, view, thinking_area, buf),
            ThinkingPlacement::Float => {
                if let Some(area) = thinking::float_area(transcript_area) {
                    thinking::render(app, view, area, buf);
                }
            }
        }
    }
    if let Some(pending) = &pending {
        ask::render(app, pending, ask_area, buf);
    }
    composer::render_queued(app, queued_area, buf);
    if session_bar {
        session_bar::render(app, bar_area, buf);
    }
    composer::render(app, composer_area, buf);
    composer_area
}

/// Conversation title bar: `project › title` with history and menu buttons.
/// 会话标题栏：`项目 › 标题`，附历史与菜单按钮。
fn title_bar(app: &App, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    let theme = &app.theme;
    let Some(conversation) = app.conversation() else {
        buf.set_string(
            area.x,
            area.y,
            tr!("sidebar.waitingWorkspace"),
            theme.muted_text(),
        );
        return;
    };
    let history = format!(" ⌕ {} Ctrl+F ", tr!("tui.history.short"));
    let menu = " ⋯ ".to_string();
    let buttons_width = (text::width_of(&history) + text::width_of(&menu) + 1) as u16;
    let mut spans = Vec::new();
    if let Some(project) = app.project_of(conversation) {
        spans.push(Span::styled(project.name.clone(), theme.muted_text()));
        spans.push(Span::styled(" › ", Style::new().fg(theme.border)));
    }
    spans.push(Span::styled(conversation.title.clone(), theme.strong()));
    if let Some(session) = app.session()
        && session.reveal
    {
        spans.push(Span::styled(
            format!("  ◉ {}", tr!("tui.transcript.revealed")),
            Style::new().fg(theme.info),
        ));
    }
    Line::from(spans).render(
        Rect {
            width: area.width.saturating_sub(buttons_width + 1),
            ..area
        },
        buf,
    );
    if area.width <= buttons_width + 10 {
        return;
    }
    let button_style = Style::new().fg(theme.fg).bg(theme.accent);
    let x = area.right() - buttons_width;
    buf.set_string(x, area.y, &history, button_style);
    let hw = text::width_of(&history) as u16;
    app.hits.add(
        Rect {
            x,
            width: hw,
            ..area
        },
        Target::Do(UiAction::OpenHistory),
    );
    let mx = x + hw + 1;
    buf.set_string(mx, area.y, &menu, button_style.add_modifier(Modifier::BOLD));
    app.hits.add(
        Rect {
            x: mx,
            width: text::width_of(&menu) as u16,
            ..area
        },
        Target::Do(UiAction::ConversationMenu(conversation.id.clone())),
    );
}
