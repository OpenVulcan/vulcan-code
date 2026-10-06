// Headless smoke test: drives the scripted mock backend through a real turn and renders every frame
// at a wide and a narrow size, so layout panics and overflow bugs surface without a terminal.
// 无头冒烟测试：以脚本化模拟后端驱动一次真实对话轮次，并在宽、窄两种尺寸下渲染每一帧，
// 使布局 panic 与越界问题无需真实终端即可暴露。

use std::time::{Duration, Instant};

use crate::app::action::UiAction;
use crate::app::hit::Target;
use crate::app::overlay::{Menu, Overlay};
use crate::app::pages::Area;
use crate::app::{App, PanelTab, SidebarMode, input};
use crate::backend::mock;
use crate::i18n;
use crate::model::{AgentState, GoalStatus};
use crate::ui::{self, theme::Theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
/// Dynamic catalog navigation and managed document reading are reachable by real hits and keys.
/// 动态目录导航及托管文档阅读可通过真实命中与按键触达。
/// Both terminal widths use inert channels and never connect to a real service.
/// 两种终端宽度均使用静默通道，绝不连接真实服务。
#[test]
fn catalog_navigation_and_managed_reader_are_mouse_keyboard_accessible() {
    use crate::app::pages::{PageId, RowKind, navigation, rows};
    use crate::app::{Focus, action::perform};
    use crate::backend::{BackendEvent, BackendHandle, Command, ConnectionState};
    use crate::service_api::{Slot, knowledge_base as kb, workbench_snapshot};
    use serde_json::{Value, json};
    use std::sync::mpsc;
    // Synthetic catalog source contains real category and managed-document fields.
    // 合成目录来源包含真实分类及托管文档字段。
    let fixture: Value =
        serde_json::from_str(include_str!("../src/backend/mock/service-fixtures.json")).unwrap();
    for (width, height) in [(160, 45), (64, 20)] {
        // Requests are observed and never executed.
        // 只观察请求，不执行。
        let (commands, observer) = mpsc::channel();
        // No worker produces external events.
        // 不运行产生外部事件的工作器。
        let (_, events) = mpsc::channel();
        // Isolated application uses actual page rendering and input dispatch.
        // 隔离应用使用真实页面渲染及输入分派。
        let mut app = App::new(
            BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            Theme::dark(),
            true,
        );
        app.apply(BackendEvent::Connection(ConnectionState::Connected));
        app.apply(BackendEvent::Slot {
            slot: Slot::Workbench,
            call: workbench_snapshot::load_workbench_snapshot(),
            value: fixture["workbench"].clone(),
        });
        app.apply(BackendEvent::Slot {
            slot: Slot::KnowledgeLibraries,
            call: kb::list_knowledge_libraries(),
            value: fixture["loads"]["service_list_knowledge_libraries"].clone(),
        });
        // Fresh terminal buffers prevent old wide-character cells from masking layout defects.
        // 全新终端缓冲区防止旧宽字符单元格掩盖布局缺陷。
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for area in [Area::Agents, Area::AgentSkills] {
            perform(&mut app, UiAction::Area(area));
            terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
            assert_eq!(
                navigation(&app)
                    .into_iter()
                    .map(|(_, entries)| entries.len())
                    .sum::<usize>(),
                3
            );
            click(&mut app, "development category", |target| {
                matches!(target, Target::NavItem(1))
            });
            assert_eq!(
                if area == Area::Agents {
                    &app.agent_category
                } else {
                    &app.skill_category
                },
                "development"
            );
            input::handle_event(&mut app, key(KeyCode::Down, KeyModifiers::NONE));
            assert_eq!(
                if area == Area::Agents {
                    &app.agent_category
                } else {
                    &app.skill_category
                },
                "analysis"
            );
            terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
            // Wheel movement uses the same current category mapping as keyboard and clicks.
            // 滚轮移动与键盘、点击使用相同当前分类映射。
            let rect = app
                .hits
                .find(|target| matches!(target, Target::NavItem(2)))
                .unwrap();
            input::handle_event(
                &mut app,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollUp,
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE,
                }),
            );
            assert_eq!(
                if area == Area::Agents {
                    &app.agent_category
                } else {
                    &app.skill_category
                },
                "development"
            );
            snapshot(&mut app, &format!("category-{area:?}-{width}"));
        }
        perform(&mut app, UiAction::Area(Area::Knowledge));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(buffer_text(terminal.backend().buffer()).contains("Demo Knowledge"));
        click(&mut app, "real library", |target| {
            matches!(target, Target::NavItem(1))
        });
        assert_eq!(app.library.as_deref(), Some("demo-library"));
        assert_eq!(app.current_page(), Some(PageId::KnowledgeLibrary));
        observer.try_iter().for_each(drop);
        app.focus = Focus::AreaPage;
        input::handle_event(&mut app, key(KeyCode::Home, KeyModifiers::NONE));
        // The overview's first selectable row is the real shared view selector.
        // 概览首个可选行是真实共享视图选择器。
        input::handle_event(&mut app, key(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.current_page(), Some(PageId::KnowledgeDocuments));
        assert!(observer.try_iter().any(|command|matches!(command,Command::Call(call)
            if call.method == "service_list_knowledge_library_documents" && call.param("/request/libraryId") == "demo-library" && call.param("/request/limit") == 21)));
        app.apply(BackendEvent::Slot {
            slot: Slot::KnowledgeFolders,
            call: kb::list_managed_knowledge_document_folders("demo-library"),
            value: json!(["", "guide"]),
        });
        app.apply(BackendEvent::Slot {
            slot: Slot::KnowledgeDocuments,
            call: kb::list_managed_knowledge_documents("demo-library", None, 21, 0),
            value: Value::Array(
                fixture["knowledgeDocuments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .take(21)
                    .map(|entry| entry["document"].clone())
                    .collect(),
            ),
        });
        // Exact reader action is located from its rendered document row.
        // 从已渲染文档行定位准确阅读动作。
        let row = rows(&app, PageId::KnowledgeDocuments)
            .iter()
            .position(|row| matches!(&row.kind,RowKind::Item{id,..} if id == "demo-document"))
            .unwrap();
        app.page_state_mut(PageId::KnowledgeDocuments).cursor = row;
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        snapshot(&mut app, &format!("managed-documents-{width}"));
        click(
            &mut app,
            "read document",
            |target| matches!(target,Target::PageRowAction(index,0) if *index == row),
        );
        assert_eq!(app.current_page(), Some(PageId::KnowledgeDocument));
        assert!(observer.try_iter().any(|command|matches!(command,Command::Call(call)
            if call.method == "service_read_knowledge_library_document" && call.param("/documentId") == "demo-document" && call.param("/corpus") == "raw")));
        app.apply(BackendEvent::Slot {slot:Slot::KnowledgeDocument,call:kb::read_managed_knowledge_document("demo-library","demo-document","raw"),
            value:json!({"document":fixture["knowledgeDocuments"][0]["document"],"content":fixture["knowledgeDocuments"][0]["raw"]})});
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        input::handle_event(&mut app, key(KeyCode::PageDown, KeyModifiers::NONE));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(buffer_text(terminal.backend().buffer()).contains("Synthetic demo"));
        assert!(buffer_text(terminal.backend().buffer()).contains("    indented line"));
        snapshot(&mut app, &format!("managed-reader-{width}"));
        perform(
            &mut app,
            UiAction::Edit(
                crate::app::action::Edit::Local(crate::app::action::LocalSetting::KnowledgeCorpus),
                crate::model::SettingValue::Text("display".into()),
            ),
        );
        app.apply(BackendEvent::SlotFailed {
            slot: Slot::KnowledgeDocument,
            call: kb::read_managed_knowledge_document("demo-library", "demo-document", "display"),
            message: "Display document is not built".into(),
        });
        app.toasts.clear();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(buffer_text(terminal.backend().buffer()).contains("not built"));
        snapshot(&mut app, &format!("managed-reader-error-{width}"));
    }
}

/// Renders and clicks a guarded import flow using inert channels at both terminal widths.
/// 在两种终端宽度下使用静默通道渲染并点击受保护导入流程。
/// No worker or service discovery runs; only the final request is observed.
/// 不运行工作器或服务发现；仅观察最终请求。
#[test]
fn preview_review_mouse_confirmation_and_subset_input_render() {
    use crate::backend::{BackendEvent, BackendHandle, Command, ConnectionState};
    use crate::service_api::knowledge_base;
    use serde_json::{Value, json};
    use std::sync::mpsc;
    // Wide and narrow layouts must expose the same approval actions.
    // 宽窄布局必须公开相同的批准动作。
    for (width, height) in [(160, 45), (64, 20)] {
        let (commands, observer) = mpsc::channel();
        let (_, events) = mpsc::channel();
        let mut app = App::new(
            BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            Theme::dark(),
            true,
        );
        app.apply(BackendEvent::Connection(ConnectionState::Connected));
        // Many preview rows exercise scrolling without moving the approval button out of reach.
        // 较多预检行验证滚动，且不能让批准按钮无法触达。
        let entries: Vec<Value> = (0..40)
            .map(|index| json!({"key":format!("file-{index}.md"),"disposition":"ready"}))
            .collect();
        app.apply(BackendEvent::Preview {
            call: knowledge_base::preview_knowledge_import("demo", "C:/demo", None),
            result: json!({"fingerprint":"preview-1","entries":entries}),
        });
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(matches!(app.overlay, Some(Overlay::Detail(_))));
        snapshot(&mut app, &format!("import-preview-{width}"));
        click(&mut app, "preview approval", |target| {
            matches!(target, Target::Do(UiAction::SheetPick(0)))
        });
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        click(&mut app, "final confirmation", |target| {
            matches!(target, Target::Do(UiAction::ConfirmAccept))
        });
        assert!(matches!(app.overlay, Some(Overlay::Prompt(_))));
        input::handle_event(&mut app, key(KeyCode::Char('u'), KeyModifiers::CONTROL));
        input::handle_event(&mut app, Event::Paste("outside.md".into()));
        input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(&app.overlay,Some(Overlay::Prompt(dialog)) if dialog.error.is_some()));
        assert!(observer.try_recv().is_err());
        // Replace the ready list through the same paste and newline events available in a terminal.
        // 通过终端可用的相同粘贴及换行事件替换就绪列表。
        input::handle_event(&mut app, key(KeyCode::Char('u'), KeyModifiers::CONTROL));
        input::handle_event(&mut app, Event::Paste("file-3.md".into()));
        input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::ALT));
        input::handle_event(&mut app, Event::Paste("file-7.md".into()));
        assert!(matches!(&app.overlay, Some(Overlay::Prompt(dialog))
            if dialog.input == "file-3.md\nfile-7.md" && dialog.error.is_none()));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        // Each selected key must occupy its own visible row, without accidental concatenation.
        // 每个选中键必须占据独立可见行，不能意外拼接。
        let rendered = terminal.backend().buffer();
        // Text rows observed directly from the rendered terminal cells.
        // 从已渲染终端单元格直接观察的文本行。
        let key_rows: Vec<String> = (0..height)
            .map(|row| {
                (0..width)
                    .map(|column| rendered[(column, row)].symbol())
                    .collect()
            })
            .collect();
        assert!(
            key_rows
                .iter()
                .any(|row| row.contains("file-3.md") && !row.contains("file-7.md"))
        );
        assert!(
            key_rows
                .iter()
                .any(|row| row.contains("file-7.md") && !row.contains("file-3.md"))
        );
        snapshot(&mut app, &format!("import-selection-{width}"));
        input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
        let Command::Call(call) = observer.try_recv().unwrap() else {
            panic!("import call")
        };
        assert_eq!(
            call.param("/request/selectedFileKeys"),
            &json!(["file-3.md", "file-7.md"])
        );
        assert_eq!(call.param("/request/previewFingerprint"), "preview-1");
        assert!(observer.try_recv().is_err());
    }
}

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
    let _locale = i18n::lock_locale("zh-TW");
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();

    // Splash while the mock "connects", then the workbench.
    // 模拟“连接”期间显示启动画面，之后进入工作台。
    wide.draw(|frame| ui::draw(frame, &mut app))
        .expect("splash draw");
    assert!(
        buffer_text(wide.backend().buffer()).contains("VulcanCode"),
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
pub(crate) fn snapshot(app: &mut App, name: &str) {
    let Ok(dir) = std::env::var("VULCAN_TUI_SNAPSHOT_DIR") else {
        return;
    };
    for (size, width, height) in [
        ("ultrawide", 240, 50),
        ("wide", 160, 45),
        ("narrow", 64, 20),
    ] {
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
    let _locale = i18n::lock_locale("en-US");
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

    // The tab strip stays on one line and some tabs fold into its overflow menu at this width.
    // 此宽度下标签条保持单行，部分标签收进溢出菜单。
    let tab_rows = |app: &App| {
        PanelTab::ALL
            .into_iter()
            .filter_map(|tab| {
                app.hits
                    .find(|t| matches!(t, Target::PanelTab(p) if *p == tab))
            })
            .map(|rect| rect.y)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(tab_rows(&app).len(), 1, "panel tabs must share one line");
    assert!(
        app.hits
            .find(|t| matches!(t, Target::Do(UiAction::OpenMenu(_))))
            .is_some(),
        "a 160-column layout should fold some panel tabs; update the width if the labels shrank"
    );

    // Every panel tab renders when reached by mouse, directly or through the overflow menu.
    // 每个面板标签页无论直接点击还是经溢出菜单进入，都能正常渲染。
    for tab in PanelTab::ALL {
        open_panel_tab(&mut app, tab, &mut wide);
        assert_eq!(app.panel_tab, tab);
        settle(&mut app, &mut wide, &mut narrow, 120);
        assert_eq!(tab_rows(&app).len(), 1, "panel tabs must share one line");
        snapshot(&mut app, &format!("panel-{tab:?}"));
    }

    // Memory: open a dynamic item and suppress it for this session.
    // 记忆：打开一条动态记忆并在本会话中停止注入。
    let dynamic = |app: &App| app.session().unwrap().insight.memory.dynamic_count();
    open_panel_tab(&mut app, PanelTab::Memory, &mut wide);
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
    let _locale = i18n::lock_locale("en-US");
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
    app.focus = crate::app::Focus::Composer;
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
                crate::model::EventBody::UserInput { text, .. } => {
                    text.ends_with("second line").then(|| text.clone())
                }
                _ => None,
            })
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    // Events are applied one at a time so a quick reply cannot land in the same batch.
    // 逐个应用事件，避免快速到达的回复落入同一批次。
    while sent(&app).is_none() && Instant::now() < deadline {
        while let Ok(event) = app.backend.events.try_recv() {
            app.apply(event);
            if sent(&app).is_some() {
                break;
            }
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

/// Reaches a panel tab by mouse: its tab when shown, else the strip's overflow menu entry.
/// 用鼠标进入面板标签页：标签可见时直接点击，否则经标签条的溢出菜单进入。
fn open_panel_tab(app: &mut App, tab: PanelTab, terminal: &mut Terminal<TestBackend>) {
    if app
        .hits
        .find(|t| matches!(t, Target::PanelTab(p) if *p == tab))
        .is_some()
    {
        click(
            app,
            "panel tab",
            |t| matches!(t, Target::PanelTab(p) if *p == tab),
        );
        return;
    }
    let lists = |menu: &Menu| {
        menu.items
            .iter()
            .position(|item| matches!(item.action, UiAction::ShowPanel(p) if p == tab))
    };
    click(
        app,
        "panel overflow",
        |t| matches!(t, Target::Do(UiAction::OpenMenu(menu)) if lists(menu).is_some()),
    );
    let Some(Overlay::Menu(menu)) = &app.overlay else {
        panic!("the overflow chip should open a menu");
    };
    let index = lists(menu).expect("menu lists the folded tab");
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    click(
        app,
        "folded tab",
        |t| matches!(t, Target::MenuItem(i) if *i == index),
    );
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
    let _locale = i18n::lock_locale("en-US");
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
        redraw(&mut app, &mut wide);
        assert!(
            app.hits
                .find(|t| matches!(t, Target::Sidebar(_) | Target::Do(UiAction::OpenSidebar)))
                .is_none(),
            "system area {area:?} must not show the project and conversation list"
        );
        // Dynamic catalogs have category/library rows instead of one row per content view.
        // 动态目录使用分类、知识库行，而非每个内容视图一行。
        let count = crate::app::pages::navigation(&app)
            .into_iter()
            .map(|(_, entries)| entries.len())
            .sum::<usize>();
        for page in 0..count {
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
    // Library subviews use the shared view selector and independent page state.
    // 知识库子视图使用共享视图选择器及独立页面状态。
    for page in Area::Knowledge.pages() {
        if page == crate::app::pages::PageId::KnowledgeDocument {
            crate::app::action::perform(
                &mut app,
                UiAction::OpenKnowledgeDocument("demo-document".into()),
            );
        } else {
            crate::app::action::perform(&mut app, UiAction::GoPage(page));
        }
        pump(
            &mut app,
            &mut wide,
            &mut narrow,
            Instant::now() + Duration::from_millis(120),
        );
        snapshot(&mut app, &format!("{page:?}"));
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

/// Presses the left button at `rect`'s first cell; two presses in a row form a double click.
/// 在 `rect` 的首个单元格按下左键；连续两次按下构成双击。
fn press(app: &mut App, rect: Rect) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    };
    input::handle_event(app, Event::Mouse(event));
}

/// A single click only selects transcript blocks and page items; a double click expands them.
/// 单击只选中会话记录块与页面条目；双击才展开。
#[test]
fn single_click_selects_and_double_click_expands() {
    let _locale = i18n::lock_locale("en-US");
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(1500),
    );
    wide.draw(|frame| ui::draw(frame, &mut app)).unwrap();

    // Transcript: the first fully visible block.
    // 会话记录：首个完整可见的块。
    let area = app.hits.transcript;
    let top = app.hits.transcript_top;
    let (seq, start, _) = app
        .hits
        .transcript_items
        .iter()
        .copied()
        .find(|(_, start, _)| *start >= top && *start < top + area.height as usize)
        .expect("a visible transcript block");
    let row = Rect {
        y: area.y + (start - top) as u16,
        height: 1,
        ..area
    };
    let folded = |app: &App| app.session().is_some_and(|s| s.toggled.contains(&seq));
    let before = folded(&app);
    press(&mut app, row);
    assert_eq!(app.session().and_then(|s| s.selected), Some(seq));
    assert_eq!(folded(&app), before, "a single click must not fold");
    press(&mut app, row);
    assert_ne!(folded(&app), before, "a double click folds or unfolds");

    // Page items: find an expandable item on the plugin pages.
    // 页面条目：在插件页面中查找可展开的条目。
    crate::app::action::perform(&mut app, UiAction::Area(Area::Plugins));
    let mut found = None;
    for _ in 0..2 {
        pump(
            &mut app,
            &mut wide,
            &mut narrow,
            Instant::now() + Duration::from_millis(400),
        );
        wide.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let page = app.current_page().expect("plugin page");
        found = crate::app::pages::rows(&app, page)
            .iter()
            .enumerate()
            .find_map(|(index, row)| match &row.kind {
                crate::app::pages::RowKind::Item { id, lines, .. } if !lines.is_empty() => {
                    Some((page, index, id.clone()))
                }
                _ => None,
            });
        if found.is_some() {
            break;
        }
    }
    let (page, index, id) = found.expect("an expandable plugin item");
    let rect = app
        .hits
        .find(|t| matches!(t, Target::PageRow(i) if *i == index))
        .expect("item row target");
    press(&mut app, rect);
    assert!(!app.page_state(page).expanded.contains(&id));
    press(&mut app, rect);
    assert!(app.page_state(page).expanded.contains(&id));
}

/// The activity bar labels its entries on wide terminals, shows icons on narrower ones and gives
/// way to the header switcher on the narrowest; the workbench entry toggles the sidebar when the
/// workbench is already open.
/// 活动栏在宽终端显示名称，较窄时只显示图标，最窄时让位于顶栏切换按钮；工作台已打开时点击工作台
/// 条目会切换侧栏。
#[test]
fn activity_bar_adapts_and_toggles_the_sidebar() {
    let _locale = i18n::lock_locale("en-US");
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();
    pump(
        &mut app,
        &mut wide,
        &mut narrow,
        Instant::now() + Duration::from_millis(1500),
    );
    let entry_width = |app: &App, area: Area| {
        app.hits
            .find(|t| matches!(t, Target::AreaTab(a) if *a == area))
            .map(|rect| rect.width)
    };
    // The status bar keeps its own F2 button; only a header switcher sits on the first row.
    // 状态栏保留自身的 F2 按钮；只有顶栏切换按钮位于第一行。
    let switcher = |app: &App| {
        app.hits
            .find(|t| matches!(t, Target::Do(UiAction::OpenAreaPicker)))
            .is_some_and(|rect| rect.y == 0)
    };

    wide.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    let labeled = entry_width(&app, Area::Chat).expect("labeled activity bar");
    assert!(!switcher(&app), "the bar replaces the header switcher");
    narrow.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    let icons = entry_width(&app, Area::Chat).expect("icon activity bar");
    assert!(icons < labeled, "narrow terminals show the icon form");
    let mut tiny = Terminal::new(TestBackend::new(50, 20)).unwrap();
    tiny.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    assert_eq!(entry_width(&app, Area::Chat), None);
    assert!(
        switcher(&app),
        "without the bar the header offers the switcher"
    );

    // Every area is reachable from the bar, Knowledge included.
    // 每个区域都能从活动栏进入，包括知识库。
    for area in Area::ALL {
        wide.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        click(
            &mut app,
            "activity entry",
            |t| matches!(t, Target::AreaTab(a) if *a == area),
        );
        assert_eq!(app.area, area);
    }

    // Inside the workbench its entry folds and unfolds the sidebar instead of reopening it.
    // 在工作台内，工作台条目改为折叠与展开侧栏，而不是重新打开工作台。
    wide.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    click(&mut app, "workbench entry", |t| {
        matches!(t, Target::AreaTab(Area::Chat))
    });
    assert_eq!(app.area, Area::Chat);
    let before = app.sidebar;
    wide.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    click(&mut app, "workbench entry", |t| {
        matches!(t, Target::AreaTab(Area::Chat))
    });
    assert_eq!(app.area, Area::Chat);
    assert_ne!(app.sidebar, before);
}

/// The memory recall notice shows while the conversation is in its PreCheck phase and leaves once
/// the answer starts.
/// 记忆召回通知在会话处于 PreCheck 阶段时显示，开始回答后消失。
#[test]
fn memory_recall_notice_follows_the_precheck_phase() {
    use crate::app::notice::live;
    use crate::model::RunPhase;

    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    let mut wide = Terminal::new(TestBackend::new(160, 45)).unwrap();
    let mut narrow = Terminal::new(TestBackend::new(64, 20)).unwrap();
    let recalling = |app: &App| {
        live(app)
            .iter()
            .any(|notice| notice.key.as_deref() == Some("memory-recall"))
    };
    // Pumps until `done` holds; the deadline only bounds a hung mock.
    // 持续处理直到 `done` 成立；截止时间只用于限制卡住的模拟后端。
    let mut wait = |app: &mut App, what: &str, done: &dyn Fn(&App) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(app) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            pump(
                app,
                &mut wide,
                &mut narrow,
                Instant::now() + Duration::from_millis(30),
            );
        }
    };

    wait(&mut app, "the workbench", &|app| {
        app.conversation().is_some()
    });
    type_text(&mut app, "recall prompt");
    input::handle_event(&mut app, key(KeyCode::Enter, KeyModifiers::NONE));
    wait(&mut app, "memory recall", &|app| {
        app.run_phase() == RunPhase::MemoryRecall
    });
    assert!(
        recalling(&app),
        "the PreCheck phase raises the recall notice"
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    assert!(
        buffer_text(terminal.backend().buffer()).contains("VMM"),
        "the recall notice is drawn during PreCheck"
    );

    wait(&mut app, "the answer to start", &|app| {
        app.run_phase() == RunPhase::Running
    });
    assert!(
        !recalling(&app),
        "the recall notice leaves once the answer starts"
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    assert!(
        !buffer_text(terminal.backend().buffer()).contains("VMM"),
        "the recall notice is no longer drawn"
    );
}
