// Headless smoke test: drives the scripted mock backend through a real turn and renders every frame
// at a wide and a narrow size, so layout panics and overflow bugs surface without a terminal.
// 无头冒烟测试：以脚本化模拟后端驱动一次真实对话轮次，并在宽、窄两种尺寸下渲染每一帧，
// 使布局 panic 与越界问题无需真实终端即可暴露。

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use vulcan_code_tui::app::action::UiAction;
use vulcan_code_tui::app::hit::Target;
use vulcan_code_tui::app::overlay::Overlay;
use vulcan_code_tui::app::pages::Area;
use vulcan_code_tui::app::{App, PanelTab, SidebarMode, input};
use vulcan_code_tui::backend::mock;
use vulcan_code_tui::i18n;
use vulcan_code_tui::model::{AgentState, GoalStatus};
use vulcan_code_tui::ui::{self, theme::Theme};

fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        input::handle_event(app, key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
}

fn buffer_text(buffer: &Buffer) -> String {
    buffer.content().iter().map(|cell| cell.symbol()).collect()
}

/// Pumps backend events and renders at both sizes until `deadline`.
/// 在 `deadline` 之前持续处理后端事件并以两种尺寸渲染。
fn pump(
    app: &mut App,
    wide: &mut Terminal<TestBackend>,
    narrow: &mut Terminal<TestBackend>,
    deadline: Instant,
) {
    while Instant::now() < deadline {
        while let Ok(event) = app.backend.events.try_recv() {
            app.apply(event);
        }
        app.tick(Instant::now());
        wide.draw(|frame| ui::draw(frame, app)).expect("wide draw");
        narrow
            .draw(|frame| ui::draw(frame, app))
            .expect("narrow draw");
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[test]
fn mock_turn_renders_at_wide_and_narrow_sizes() {
    i18n::set_active_locale(i18n::resolve_locale(Some("zh-TW"), None));
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();

    // Splash while the mock "connects", then the workbench.
    // 模拟“连接”期间显示启动画面，之后进入工作台。
    wide.draw(|frame| ui::draw(frame, &mut app))
        .expect("splash draw");
    assert!(
        buffer_text(wide.backend().buffer()).contains("V U L C A N"),
        "splash shows the wordmark"
    );
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(2000),
    );
    wide.draw(|frame| ui::draw(frame, &mut app))
        .expect("wide draw");
    assert!(
        app.hits.find(|t| matches!(t, Target::Sidebar(_))).is_some(),
        "the workbench sidebar should list mock projects once connected"
    );

    type_text(&mut app, "smoke prompt");
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(2500),
    );

    // Overlays and focus changes mid-run must render at both sizes too.
    // 运行中打开覆盖层与切换焦点，同样需在两种尺寸下正常渲染。
    for event in [
        key(KeyCode::F(1), KeyModifiers::NONE),
        key(KeyCode::Esc, KeyModifiers::NONE),
        key(KeyCode::Char('p'), KeyModifiers::CONTROL),
        key(KeyCode::Esc, KeyModifiers::NONE),
        key(KeyCode::Char('t'), KeyModifiers::CONTROL),
        key(KeyCode::Tab, KeyModifiers::NONE),
        key(KeyCode::Tab, KeyModifiers::NONE),
        key(KeyCode::Tab, KeyModifiers::NONE),
    ] {
        input::handle_event(&mut app, event);
        pump(
            &mut app,
            &mut wide,
            &mut narrow,
            Instant::now() + Duration::from_millis(150),
        );
    }
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(4000),
    );

    snapshot(&mut app, "turn");
    assert!(
        buffer_text(wide.backend().buffer()).contains("smoke prompt"),
        "the submitted prompt should stay visible in the transcript"
    );
    assert!(!app.should_quit);
}

/// Optional text snapshots for eyeballing layouts: `VULCAN_TUI_SNAPSHOT_DIR=<dir> cargo test`.
/// Renders into fresh backends: TestBackend only applies diffs, so cells hidden behind wide
/// characters keep stale symbols that make a reused buffer unreadable.
/// 可选文本快照，便于目视检查布局：`VULCAN_TUI_SNAPSHOT_DIR=<目录> cargo test`。
/// 渲染到全新后端：TestBackend 只应用差量，被宽字符遮住的单元格会残留旧符号，使复用的缓冲区难以阅读。
fn snapshot(app: &mut App, name: &str) {
    let Ok(dir) = std::env::var("VULCAN_TUI_SNAPSHOT_DIR") else {
        return;
    };
    for (size, width, height) in [("wide", 160, 45), ("narrow", 64, 20)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let frame = terminal.draw(|frame| ui::draw(frame, app)).unwrap();
        let text: Vec<String> = (0..height)
            .map(|y| {
                let mut line = String::new();
                let mut skip = 0;
                for x in 0..width {
                    let symbol = frame.buffer[(x, y)].symbol();
                    if skip > 0 {
                        skip -= 1;
                        continue;
                    }
                    skip = unicode_width::UnicodeWidthStr::width(symbol).saturating_sub(1);
                    line.push_str(symbol);
                }
                line
            })
            .collect();
        std::fs::write(format!("{dir}/{name}-{size}.txt"), text.join("\n")).unwrap();
    }
}

#[test]
fn session_panel_memory_stats_and_agents() {
    i18n::set_active_locale(i18n::resolve_locale(Some("en-US"), None));
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();
    let settle = |app: &mut App, wide: &mut Terminal<TestBackend>, narrow: &mut _, ms| {
        pump(
            app,
            wide,
            narrow,
            Instant::now() + Duration::from_millis(ms),
        );
        wide.draw(|frame| ui::draw(frame, app)).expect("wide draw");
    };
    settle(&mut app, &mut wide, &mut narrow, 2000);

    // Every panel tab renders when reached by mouse.
    // 通过鼠标进入的每个面板标签页都能正常渲染。
    for tab in PanelTab::ALL {
        click(
            &mut app,
            "panel tab",
            |t| matches!(t, Target::PanelTab(p) if *p == tab),
        );
        assert_eq!(app.panel_tab, tab);
        settle(&mut app, &mut wide, &mut narrow, 120);
        snapshot(&mut app, &format!("panel-{tab:?}"));
    }

    // Memory: open a dynamic item and suppress it for this session.
    // 记忆：打开一条动态记忆并在本会话中停止注入。
    let dynamic = |app: &App| app.session().unwrap().insight.memory.dynamic_count();
    click(&mut app, "memory tab", |t| {
        matches!(t, Target::PanelTab(PanelTab::Memory))
    });
    settle(&mut app, &mut wide, &mut narrow, 100);
    let before = dynamic(&app);
    assert!(
        before > 0,
        "the demo conversation should carry dynamic memory"
    );
    click(&mut app, "memory row", |t| {
        matches!(t, Target::PanelItem(PanelTab::Memory, 1))
    });
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
    settle(&mut app, &mut wide, &mut narrow, 100);
    assert!(
        matches!(app.overlay, Some(Overlay::Detail(_))),
        "Enter on a memory row should open its detail sheet"
    );
    snapshot(&mut app, "memory-detail");
    click(&mut app, "suppress button", |t| {
        matches!(t, Target::Do(UiAction::SheetPick(0)))
    });
    settle(&mut app, &mut wide, &mut narrow, 200);
    assert!(
        app.overlay.is_none(),
        "running a sheet action closes the sheet"
    );
    assert_eq!(
        dynamic(&app),
        before - 1,
        "the suppressed memory leaves the list"
    );

    // Static profile view.
    // 静态画像视图。
    click(&mut app, "static chip", |t| {
        matches!(t, Target::Do(UiAction::MemoryView(true)))
    });
    settle(&mut app, &mut wide, &mut narrow, 100);
    assert!(app.panel.memory_static);
    snapshot(&mut app, "memory-static");

    // Stats: pause the goal from its chip.
    // 统计：通过按钮暂停目标。
    let goal = |app: &App| {
        app.session()
            .unwrap()
            .insight
            .goal
            .as_ref()
            .map(|g| g.status)
    };
    input::handle_event(&mut app, key(KeyCode::Char('5'), KeyModifiers::ALT));
    settle(&mut app, &mut wide, &mut narrow, 100);
    assert_eq!(app.panel_tab, PanelTab::Stats);
    assert_eq!(goal(&app), Some(GoalStatus::Active));
    input::handle_event(&mut app, key(KeyCode::Char('p'), KeyModifiers::NONE));
    settle(&mut app, &mut wide, &mut narrow, 200);
    assert_eq!(goal(&app), Some(GoalStatus::Paused));

    // Agents: select the running reviewer and stop it through the confirmation.
    // 代理：选中运行中的 reviewer 并经确认后停止。
    input::handle_event(&mut app, key(KeyCode::Char('6'), KeyModifiers::ALT));
    settle(&mut app, &mut wide, &mut narrow, 100);
    let running = app
        .session()
        .unwrap()
        .agents
        .iter()
        .position(|a| a.state == AgentState::Running)
        .expect("the demo conversation should have a running subagent");
    click(
        &mut app,
        "agent row",
        |t| matches!(t, Target::PanelItem(PanelTab::Agents, i) if *i == running),
    );
    settle(&mut app, &mut wide, &mut narrow, 100);
    snapshot(&mut app, "agents-selected");
    input::handle_event(&mut app, key(KeyCode::Char('s'), KeyModifiers::NONE));
    settle(&mut app, &mut wide, &mut narrow, 100);
    assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
    input::handle_event(&mut app, key(KeyCode::Char('y'), KeyModifiers::NONE));
    settle(&mut app, &mut wide, &mut narrow, 200);
    assert_eq!(
        app.session().unwrap().agents[running].state,
        AgentState::Stopped
    );
    assert!(!app.should_quit);
}

#[test]
fn session_bar_composer_pastes_and_sidebar_rail() {
    i18n::set_active_locale(i18n::resolve_locale(Some("en-US"), None));
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();
    let settle = |app: &mut App, wide: &mut Terminal<TestBackend>, narrow: &mut _, ms| {
        pump(
            app,
            wide,
            narrow,
            Instant::now() + Duration::from_millis(ms),
        );
        wide.draw(|frame| ui::draw(frame, app)).expect("wide draw");
    };
    settle(&mut app, &mut wide, &mut narrow, 2000);

    // Session bar: a running turn shows live numbers; its segments open the matching panel tab.
    // 会话栏：运行中的轮次显示实时数据；各段打开对应的面板标签页。
    type_text(&mut app, "bar prompt");
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
    settle(&mut app, &mut wide, &mut narrow, 700);
    assert!(
        app.session().unwrap().live_turn(Instant::now()).is_some(),
        "a submitted prompt starts a live turn"
    );
    snapshot(&mut app, "session-bar-live");
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.run_phase().is_active() && Instant::now() < deadline {
        settle(&mut app, &mut wide, &mut narrow, 200);
    }
    snapshot(&mut app, "session-bar-idle");
    assert!(app.session().unwrap().live_turn(Instant::now()).is_none());
    assert!(app.session().unwrap().last_turn().is_some());
    input::handle_event(&mut app, key(KeyCode::Char('1'), KeyModifiers::ALT));
    settle(&mut app, &mut wide, &mut narrow, 100);
    click(&mut app, "session bar segment", |t| {
        matches!(t, Target::Do(UiAction::ShowPanel(PanelTab::Stats)))
    });
    assert_eq!(app.panel_tab, PanelTab::Stats);

    // A long paste collapses into a token, renders as a chip and expands on send.
    // 长粘贴折叠为标记，渲染为标签，发送时展开。
    let pasted: String = (1..=30)
        .map(|i| {
            format!(
                "pasted line {i}
"
            )
        })
        .collect();
    input::handle_event(&mut app, Event::Paste(pasted.clone()));
    assert_eq!(app.pastes.len(), 1);
    assert_eq!(app.composer.lines().len(), 1, "the token stays on one line");
    assert!(app.composer_prompt().contains("pasted line 30"));
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::ALT));
    type_text(&mut app, "second line");
    assert_eq!(app.composer.lines().len(), 2, "Alt+Enter breaks the line");
    settle(&mut app, &mut wide, &mut narrow, 100);
    snapshot(&mut app, "composer-paste");
    settle(&mut app, &mut wide, &mut narrow, 50);
    click(&mut app, "paste chip", |t| {
        matches!(t, Target::Do(UiAction::PastePreview(Some(0))))
    });
    assert!(matches!(app.overlay, Some(Overlay::Detail(_))));
    snapshot(&mut app, "paste-preview");
    input::handle_event(&mut app, key(KeyCode::Esc, KeyModifiers::NONE));

    // Sending expands the paste; the long message folds in the transcript.
    // 发送时展开粘贴；较长的消息在会话记录中折叠。
    app.focus = vulcan_code_tui::app::Focus::Composer;
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.pastes.is_empty(), "sending clears the draft's pastes");
    // Draw as soon as the prompt lands: the transcript follows the bottom, so the new message
    // is in view before the reply pushes it up.
    // 提示一到达就绘制：会话记录跟随底部，在回复将其推上去之前新消息位于视野内。
    let sent = |app: &App| {
        app.session()
            .unwrap()
            .events
            .iter()
            .rev()
            .find_map(|e| match &e.body {
                vulcan_code_tui::model::EventBody::UserInput { text, .. } => {
                    text.ends_with("second line").then(|| text.clone())
                }
                _ => None,
            })
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while sent(&app).is_none() && Instant::now() < deadline {
        while let Ok(event) = app.backend.events.try_recv() {
            app.apply(event);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        sent(&app).is_some_and(|t| t.contains("pasted line 30")),
        "the sent prompt carries the full paste"
    );
    wide.draw(|frame| ui::draw(frame, &mut app))
        .expect("wide draw");
    assert!(
        buffer_text(wide.backend().buffer()).contains("more lines"),
        "a long user message folds to a preview"
    );
    snapshot(&mut app, "user-fold");
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.run_phase().is_active() && Instant::now() < deadline {
        settle(&mut app, &mut wide, &mut narrow, 200);
    }

    // Collapse the sidebar to the rail, open a project from it, and fold every project.
    // 将侧栏收起为窄栏，从窄栏打开项目，并折叠全部项目。
    settle(&mut app, &mut wide, &mut narrow, 50);
    click(&mut app, "collapse button", |t| {
        matches!(t, Target::Do(UiAction::SidebarMode(SidebarMode::Rail)))
    });
    assert_eq!(app.sidebar, SidebarMode::Rail);
    settle(&mut app, &mut wide, &mut narrow, 100);
    snapshot(&mut app, "rail");
    settle(&mut app, &mut wide, &mut narrow, 50);
    click(&mut app, "rail project", |t| {
        matches!(t, Target::Do(UiAction::RailProject(_)))
    });
    settle(&mut app, &mut wide, &mut narrow, 100);
    snapshot(&mut app, "rail-drawer");
    settle(&mut app, &mut wide, &mut narrow, 50);
    click(&mut app, "fold all", |t| {
        matches!(t, Target::Do(UiAction::FoldAllProjects))
    });
    assert_eq!(app.collapsed_projects.len(), app.projects.len());
    input::handle_event(&mut app, key(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(app.sidebar, SidebarMode::Expanded);
    settle(&mut app, &mut wide, &mut narrow, 100);
    assert!(!app.should_quit);
}

/// Left-clicks the centre of the first registered target matching `pred`.
/// 左键点击首个满足 `pred` 的已登记目标的中心。
fn click(app: &mut App, what: &str, pred: impl Fn(&Target) -> bool) {
    let rect: Rect = app
        .hits
        .find(pred)
        .unwrap_or_else(|| panic!("no clickable target for {what} in the last frame"));
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x + rect.width / 2,
        row: rect.y + rect.height / 2,
        modifiers: KeyModifiers::NONE,
    };
    input::handle_event(app, Event::Mouse(event));
}

#[test]
fn mouse_reaches_areas_overlays_and_dialogs() {
    i18n::set_active_locale(i18n::resolve_locale(Some("en-US"), None));
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(2000),
    );
    // The wide frame is drawn after the narrow one inside `pump`, so redraw it last for hit lookups.
    // `pump` 中宽尺寸先于窄尺寸绘制，因此最后重绘宽尺寸以供命中查找。
    let redraw = |app: &mut App, wide: &mut Terminal<TestBackend>| {
        wide.draw(|frame| ui::draw(frame, app)).expect("wide draw");
    };
    redraw(&mut app, &mut wide);

    // Sidebar button opens the folder picker in project mode; its close button dismisses it.
    // 侧栏按钮以项目模式打开文件夹选择器；其关闭按钮可将其关闭。
    click(&mut app, "add project", |t| {
        matches!(t, Target::Do(UiAction::AddProject))
    });
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(400),
    );
    assert!(
        matches!(app.overlay, Some(Overlay::Folder(_))),
        "folder picker should open"
    );
    snapshot(&mut app, "folder");
    redraw(&mut app, &mut wide);
    click(&mut app, "modal close", |t| {
        matches!(t, Target::Do(UiAction::Close))
    });
    assert!(
        app.overlay.is_none(),
        "the modal close button should dismiss the picker"
    );

    // Every system area and each of its pages renders when reached by mouse.
    // 通过鼠标进入的每个系统区域及其各页面都能正常渲染。
    for area in Area::ALL.into_iter().filter(|a| *a != Area::Chat) {
        redraw(&mut app, &mut wide);
        click(
            &mut app,
            "area tab",
            |t| matches!(t, Target::AreaTab(a) if *a == area),
        );
        assert_eq!(app.area, area);
        pump(
            &mut app,
            &mut wide,
            &mut narrow,
            Instant::now() + Duration::from_millis(300),
        );
        for page in 0..area.pages().len() {
            redraw(&mut app, &mut wide);
            click(
                &mut app,
                "nav item",
                |t| matches!(t, Target::NavItem(i) if *i == page),
            );
            pump(
                &mut app,
                &mut wide,
                &mut narrow,
                Instant::now() + Duration::from_millis(120),
            );
            snapshot(&mut app, &format!("{area:?}-{page}"));
        }
    }
    redraw(&mut app, &mut wide);
    click(&mut app, "chat tab", |t| {
        matches!(t, Target::AreaTab(Area::Chat))
    });
    assert_eq!(app.area, Area::Chat);

    // Run a turn, then browse it through the history overlay and the inspector.
    // 执行一轮对话，然后通过历史浮层与详情查看器浏览。
    type_text(&mut app, "mouse prompt");
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(6000),
    );
    input::handle_event(&mut app, key(KeyCode::Char('f'), KeyModifiers::CONTROL));
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(150),
    );
    assert!(
        matches!(app.overlay, Some(Overlay::History(_))),
        "Ctrl+F should open history"
    );
    snapshot(&mut app, "history");
    redraw(&mut app, &mut wide);
    click(&mut app, "inspect button", |t| {
        matches!(t, Target::Do(UiAction::Inspect(_)))
    });
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(150),
    );
    assert!(
        matches!(app.overlay, Some(Overlay::Inspector(_))),
        "inspect should open the inspector"
    );
    snapshot(&mut app, "inspector");
    input::handle_event(&mut app, key(KeyCode::Esc, KeyModifiers::NONE));
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(150),
    );
    assert!(!app.should_quit);
}
