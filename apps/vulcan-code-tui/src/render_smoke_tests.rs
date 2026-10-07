// Headless smoke test: drives the scripted mock backend through a real turn and renders every frame
// at a wide and a narrow size, so layout panics and overflow bugs surface without a terminal.
// 无头冒烟测试：以脚本化模拟后端驱动一次真实对话轮次，并在宽、窄两种尺寸下渲染每一帧，
// 使布局 panic 与越界问题无需真实终端即可暴露。

use std::time::Instant;

use crate::app::action::UiAction;
use crate::app::hit::Target;
use crate::app::overlay::{Menu, Overlay};
use crate::app::pages::Area;
use crate::app::{App, PanelTab, SidebarMode, input};
use crate::backend::{Command, mock};
use crate::i18n;
use crate::model::{AgentState, SessionGoalStatus};
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

/// Headless app on the caller-driven mock, rendered at a wide and a narrow size after every event.
/// 运行在调用方驱动模拟上的无头应用，每条事件后都以宽、窄两种尺寸渲染。
struct Harness {
    app: App,
    mock: mock::Inline,
    wide: Terminal<TestBackend>,
    narrow: Terminal<TestBackend>,
}

impl Harness {
    /// Starts the app on a fresh inline mock that has not connected yet.
    /// 在尚未连接的全新内联模拟上启动应用。
    fn new() -> Self {
        let (backend, mock) = mock::inline();
        Harness {
            app: App::new(backend, Theme::dark(), true),
            mock,
            wide: Terminal::new(TestBackend::new(160, 45)).unwrap(),
            narrow: Terminal::new(TestBackend::new(64, 20)).unwrap(),
        }
    }

    /// Renders one frame at both sizes; the wide one goes last so hit lookups follow its layout.
    /// 以两种尺寸渲染一帧；宽尺寸最后绘制，使命中查找遵循其布局。
    fn frame(&mut self) {
        self.app.tick(Instant::now());
        let app = &mut self.app;
        self.narrow
            .draw(|frame| ui::draw(frame, app))
            .expect("narrow draw");
        self.wide
            .draw(|frame| ui::draw(frame, app))
            .expect("wide draw");
    }

    /// Applies mock events one at a time until `done` holds, rendering after each.
    /// 逐条应用模拟事件直到 `done` 成立，每条之后都渲染。
    /// Later events stay queued, so intermediate states remain observable.
    /// 之后的事件保持排队，因此中间状态仍可观察。
    fn until(&mut self, what: &str, done: impl Fn(&App) -> bool) {
        self.frame();
        while !done(&self.app) {
            let event = self
                .mock
                .next_event(&self.app.backend.events)
                .unwrap_or_else(|| panic!("the mock went quiet before {what}"));
            self.app.apply(event);
            self.frame();
        }
    }

    /// Applies every event until neither the mock nor the app has anything left to exchange.
    /// 应用全部事件，直到模拟与应用之间不再有任何待交换内容。
    fn settle(&mut self) {
        self.frame();
        while let Some(event) = self.mock.next_event(&self.app.backend.events) {
            self.app.apply(event);
            self.frame();
        }
    }

    /// Settles once the simulated connection has delivered the workbench.
    /// 模拟连接送达工作台后完成稳定。
    fn connected() -> Self {
        let mut harness = Harness::new();
        harness.until("the workbench", |app| app.conversation().is_some());
        harness.settle();
        harness
    }
}

#[test]
fn mock_turn_renders_at_wide_and_narrow_sizes() {
    let _locale = i18n::lock_locale("zh-TW");
    let mut h = Harness::new();

    // Splash while the mock "connects", then the workbench.
    // 模拟“连接”期间显示启动画面，之后进入工作台。
    h.frame();
    assert!(
        buffer_text(h.wide.backend().buffer()).contains("VulcanCode"),
        "splash shows the wordmark"
    );
    h.until("mock projects in the sidebar", |app| {
        app.hits.find(|t| matches!(t, Target::Sidebar(_))).is_some()
    });
    h.settle();

    type_text(&mut h.app, "smoke prompt");
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::NONE));
    h.until("the turn to run", |app| app.run_phase().is_active());

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
        input::handle_event(&mut h.app, event);
        h.frame();
    }
    assert!(h.app.run_phase().is_active(), "the overlays opened mid-run");
    h.settle();
    assert!(!h.app.run_phase().is_active(), "the scripted turn finishes");

    snapshot(&mut h.app, "turn");
    assert!(
        buffer_text(h.wide.backend().buffer()).contains("smoke prompt"),
        "the submitted prompt should stay visible in the transcript"
    );
    assert!(!h.app.should_quit);
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
    let mut h = Harness::connected();

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
    assert_eq!(tab_rows(&h.app).len(), 1, "panel tabs must share one line");
    assert!(
        h.app
            .hits
            .find(|t| matches!(t, Target::Do(UiAction::OpenMenu(_))))
            .is_some(),
        "a 160-column layout should fold some panel tabs; update the width if the labels shrank"
    );

    // Every panel tab renders when reached by mouse, directly or through the overflow menu.
    // 每个面板标签页无论直接点击还是经溢出菜单进入，都能正常渲染。
    for tab in PanelTab::ALL {
        open_panel_tab(&mut h.app, tab, &mut h.wide);
        assert_eq!(h.app.panel_tab, tab);
        h.settle();
        assert_eq!(tab_rows(&h.app).len(), 1, "panel tabs must share one line");
        snapshot(&mut h.app, &format!("panel-{tab:?}"));
    }

    // Memory: open a dynamic item and suppress it for this session.
    // 记忆：打开一条动态记忆并在本会话中停止注入。
    let dynamic = |app: &App| app.session().unwrap().insight.memory.dynamic_count();
    open_panel_tab(&mut h.app, PanelTab::Memory, &mut h.wide);
    h.settle();
    let before = dynamic(&h.app);
    assert!(
        before > 0,
        "the demo conversation should carry dynamic memory"
    );
    click(&mut h.app, "memory row", |t| {
        matches!(t, Target::PanelItem(PanelTab::Memory, 1))
    });
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::NONE));
    h.settle();
    assert!(
        matches!(h.app.overlay, Some(Overlay::Detail(_))),
        "Enter on a memory row should open its detail sheet"
    );
    snapshot(&mut h.app, "memory-detail");
    click(&mut h.app, "suppress button", |t| {
        matches!(t, Target::Do(UiAction::SheetPick(0)))
    });
    h.settle();
    assert!(
        h.app.overlay.is_none(),
        "running a sheet action closes the sheet"
    );
    assert_eq!(
        dynamic(&h.app),
        before - 1,
        "the suppressed memory leaves the list"
    );

    // Static profile view.
    // 静态画像视图。
    click(&mut h.app, "static chip", |t| {
        matches!(t, Target::Do(UiAction::MemoryView(true)))
    });
    h.settle();
    assert!(h.app.panel.memory_static);
    snapshot(&mut h.app, "memory-static");

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
    input::handle_event(&mut h.app, key(KeyCode::Char('5'), KeyModifiers::ALT));
    h.settle();
    assert_eq!(h.app.panel_tab, PanelTab::Stats);
    assert_eq!(goal(&h.app), Some(SessionGoalStatus::Active));
    input::handle_event(&mut h.app, key(KeyCode::Char('p'), KeyModifiers::NONE));
    h.settle();
    assert_eq!(goal(&h.app), Some(SessionGoalStatus::Paused));

    // Agents: select the running reviewer and stop it through the confirmation.
    // 代理：选中运行中的 reviewer 并经确认后停止。
    input::handle_event(&mut h.app, key(KeyCode::Char('6'), KeyModifiers::ALT));
    h.settle();
    let running = h
        .app
        .session()
        .unwrap()
        .agents
        .iter()
        .position(|a| a.state == AgentState::Running)
        .expect("the demo conversation should have a running subagent");
    click(
        &mut h.app,
        "agent row",
        |t| matches!(t, Target::PanelItem(PanelTab::Agents, i) if *i == running),
    );
    h.settle();
    snapshot(&mut h.app, "agents-selected");
    input::handle_event(&mut h.app, key(KeyCode::Char('s'), KeyModifiers::NONE));
    h.settle();
    assert!(matches!(h.app.overlay, Some(Overlay::Confirm(_))));
    input::handle_event(&mut h.app, key(KeyCode::Char('y'), KeyModifiers::NONE));
    h.settle();
    assert_eq!(
        h.app.session().unwrap().agents[running].state,
        AgentState::Stopped
    );
    assert!(!h.app.should_quit);
}

#[test]
fn session_bar_composer_pastes_and_sidebar_rail() {
    let _locale = i18n::lock_locale("en-US");
    let mut h = Harness::connected();

    // Session bar: a running turn shows live numbers; its segments open the matching panel tab.
    // 会话栏：运行中的轮次显示实时数据；各段打开对应的面板标签页。
    type_text(&mut h.app, "bar prompt");
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::NONE));
    h.until("a live turn", |app| {
        app.session().unwrap().live_turn(Instant::now()).is_some()
    });
    snapshot(&mut h.app, "session-bar-live");
    h.settle();
    snapshot(&mut h.app, "session-bar-idle");
    assert!(h.app.session().unwrap().live_turn(Instant::now()).is_none());
    assert!(h.app.session().unwrap().last_turn().is_some());
    input::handle_event(&mut h.app, key(KeyCode::Char('1'), KeyModifiers::ALT));
    h.settle();
    click(&mut h.app, "session bar segment", |t| {
        matches!(t, Target::Do(UiAction::ShowPanel(PanelTab::Stats)))
    });
    assert_eq!(h.app.panel_tab, PanelTab::Stats);

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
    input::handle_event(&mut h.app, Event::Paste(pasted.clone()));
    assert_eq!(h.app.pastes.len(), 1);
    assert_eq!(
        h.app.composer.lines().len(),
        1,
        "the token stays on one line"
    );
    assert!(h.app.composer_prompt().contains("pasted line 30"));
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::ALT));
    type_text(&mut h.app, "second line");
    assert_eq!(h.app.composer.lines().len(), 2, "Alt+Enter breaks the line");
    h.settle();
    snapshot(&mut h.app, "composer-paste");
    h.settle();
    click(&mut h.app, "paste chip", |t| {
        matches!(t, Target::Do(UiAction::PastePreview(Some(0))))
    });
    assert!(matches!(h.app.overlay, Some(Overlay::Detail(_))));
    snapshot(&mut h.app, "paste-preview");
    input::handle_event(&mut h.app, key(KeyCode::Esc, KeyModifiers::NONE));

    // Sending expands the paste; the long message folds in the transcript.
    // 发送时展开粘贴；较长的消息在会话记录中折叠。
    h.app.focus = crate::app::Focus::Composer;
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(h.app.pastes.is_empty(), "sending clears the draft's pastes");
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
    // Events are applied one at a time so the reply stays queued behind the prompt.
    // 逐个应用事件，使回复排在提示之后。
    h.until("the sent prompt", |app| sent(app).is_some());
    assert!(
        sent(&h.app).is_some_and(|t| t.contains("pasted line 30")),
        "the sent prompt carries the full paste"
    );
    assert!(
        buffer_text(h.wide.backend().buffer()).contains("more lines"),
        "a long user message folds to a preview"
    );
    snapshot(&mut h.app, "user-fold");
    h.settle();

    // Collapse the sidebar to the rail, open a project from it, and fold every project.
    // 将侧栏收起为窄栏，从窄栏打开项目，并折叠全部项目。
    h.settle();
    click(&mut h.app, "collapse button", |t| {
        matches!(t, Target::Do(UiAction::SidebarMode(SidebarMode::Rail)))
    });
    assert_eq!(h.app.sidebar, SidebarMode::Rail);
    h.settle();
    snapshot(&mut h.app, "rail");
    h.settle();
    click(&mut h.app, "rail project", |t| {
        matches!(t, Target::Do(UiAction::RailProject(_)))
    });
    h.settle();
    snapshot(&mut h.app, "rail-drawer");
    h.settle();
    click(&mut h.app, "fold all", |t| {
        matches!(t, Target::Do(UiAction::FoldAllProjects))
    });
    assert_eq!(h.app.collapsed_projects.len(), h.app.projects.len());
    input::handle_event(&mut h.app, key(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(h.app.sidebar, SidebarMode::Expanded);
    h.settle();
    assert!(!h.app.should_quit);
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
    let mut h = Harness::connected();

    // Sidebar button opens the folder picker in project mode; its close button dismisses it.
    // 侧栏按钮以项目模式打开文件夹选择器；其关闭按钮可将其关闭。
    click(&mut h.app, "add project", |t| {
        matches!(t, Target::Do(UiAction::AddProject))
    });
    h.settle();
    assert!(
        matches!(h.app.overlay, Some(Overlay::Folder(_))),
        "folder picker should open"
    );
    snapshot(&mut h.app, "folder");
    h.frame();
    click(&mut h.app, "modal close", |t| {
        matches!(t, Target::Do(UiAction::Close))
    });
    assert!(
        h.app.overlay.is_none(),
        "the modal close button should dismiss the picker"
    );

    // Every system area and each of its pages renders when reached by mouse.
    // 通过鼠标进入的每个系统区域及其各页面都能正常渲染。
    for area in Area::ALL.into_iter().filter(|a| *a != Area::Chat) {
        h.frame();
        click(
            &mut h.app,
            "area tab",
            |t| matches!(t, Target::AreaTab(a) if *a == area),
        );
        assert_eq!(h.app.area, area);
        h.settle();
        assert!(
            h.app
                .hits
                .find(|t| matches!(t, Target::Sidebar(_) | Target::Do(UiAction::OpenSidebar)))
                .is_none(),
            "system area {area:?} must not show the project and conversation list"
        );
        // Dynamic catalogs have category/library rows instead of one row per content view.
        // 动态目录使用分类、知识库行，而非每个内容视图一行。
        let count = crate::app::pages::navigation(&h.app)
            .into_iter()
            .map(|(_, entries)| entries.len())
            .sum::<usize>();
        for page in 0..count {
            h.frame();
            click(
                &mut h.app,
                "nav item",
                |t| matches!(t, Target::NavItem(i) if *i == page),
            );
            h.settle();
            snapshot(&mut h.app, &format!("{area:?}-{page}"));
        }
    }
    // Library subviews use the shared view selector and independent page state.
    // 知识库子视图使用共享视图选择器及独立页面状态。
    for page in Area::Knowledge.pages() {
        if page == crate::app::pages::PageId::KnowledgeDocument {
            crate::app::action::perform(
                &mut h.app,
                UiAction::OpenKnowledgeDocument("demo-document".into()),
            );
        } else {
            crate::app::action::perform(&mut h.app, UiAction::GoPage(page));
        }
        h.settle();
        snapshot(&mut h.app, &format!("{page:?}"));
    }
    h.frame();
    click(&mut h.app, "chat tab", |t| {
        matches!(t, Target::AreaTab(Area::Chat))
    });
    assert_eq!(h.app.area, Area::Chat);

    // Run a turn, then browse it through the history overlay and the inspector.
    // 执行一轮对话，然后通过历史浮层与详情查看器浏览。
    type_text(&mut h.app, "mouse prompt");
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::NONE));
    h.settle();
    assert!(!h.app.run_phase().is_active(), "the scripted turn finishes");
    input::handle_event(&mut h.app, key(KeyCode::Char('f'), KeyModifiers::CONTROL));
    h.frame();
    assert!(
        matches!(h.app.overlay, Some(Overlay::History(_))),
        "Ctrl+F should open history"
    );
    snapshot(&mut h.app, "history");
    h.frame();
    click(&mut h.app, "inspect button", |t| {
        matches!(t, Target::Do(UiAction::Inspect(_)))
    });
    h.frame();
    assert!(
        matches!(h.app.overlay, Some(Overlay::Inspector(_))),
        "inspect should open the inspector"
    );
    snapshot(&mut h.app, "inspector");
    input::handle_event(&mut h.app, key(KeyCode::Esc, KeyModifiers::NONE));
    h.frame();
    assert!(!h.app.should_quit);
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
    let mut h = Harness::connected();

    // Transcript: the first fully visible block.
    // 会话记录：首个完整可见的块。
    let area = h.app.hits.transcript;
    let top = h.app.hits.transcript_top;
    let (seq, start, _) = h
        .app
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
    let before = folded(&h.app);
    press(&mut h.app, row);
    assert_eq!(h.app.session().and_then(|s| s.selected), Some(seq));
    assert_eq!(folded(&h.app), before, "a single click must not fold");
    press(&mut h.app, row);
    assert_ne!(folded(&h.app), before, "a double click folds or unfolds");

    // Page items: find an expandable item on the plugin pages.
    // 页面条目：在插件页面中查找可展开的条目。
    crate::app::action::perform(&mut h.app, UiAction::Area(Area::Plugins));
    h.settle();
    let page = h.app.current_page().expect("plugin page");
    let (index, id) = crate::app::pages::rows(&h.app, page)
        .iter()
        .enumerate()
        .find_map(|(index, row)| match &row.kind {
            crate::app::pages::RowKind::Item { id, lines, .. } if !lines.is_empty() => {
                Some((index, id.clone()))
            }
            _ => None,
        })
        .expect("an expandable plugin item");
    let rect = h
        .app
        .hits
        .find(|t| matches!(t, Target::PageRow(i) if *i == index))
        .expect("item row target");
    press(&mut h.app, rect);
    assert!(!h.app.page_state(page).expanded.contains(&id));
    press(&mut h.app, rect);
    assert!(h.app.page_state(page).expanded.contains(&id));
}

/// The activity bar labels its entries on wide terminals, shows icons on narrower ones and gives
/// way to the header switcher on the narrowest; the workbench entry toggles the sidebar when the
/// workbench is already open.
/// 活动栏在宽终端显示名称，较窄时只显示图标，最窄时让位于顶栏切换按钮；工作台已打开时点击工作台
/// 条目会切换侧栏。
#[test]
fn activity_bar_adapts_and_toggles_the_sidebar() {
    let _locale = i18n::lock_locale("en-US");
    let mut h = Harness::connected();
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

    h.wide.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    let labeled = entry_width(&h.app, Area::Chat).expect("labeled activity bar");
    assert!(!switcher(&h.app), "the bar replaces the header switcher");
    h.narrow.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    let icons = entry_width(&h.app, Area::Chat).expect("icon activity bar");
    assert!(icons < labeled, "narrow terminals show the icon form");
    let mut tiny = Terminal::new(TestBackend::new(50, 20)).unwrap();
    tiny.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    assert_eq!(entry_width(&h.app, Area::Chat), None);
    assert!(
        switcher(&h.app),
        "without the bar the header offers the switcher"
    );

    // Every area is reachable from the bar, Knowledge included.
    // 每个区域都能从活动栏进入，包括知识库。
    for area in Area::ALL {
        h.wide.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
        click(
            &mut h.app,
            "activity entry",
            |t| matches!(t, Target::AreaTab(a) if *a == area),
        );
        assert_eq!(h.app.area, area);
    }

    // Inside the workbench its entry folds and unfolds the sidebar instead of reopening it.
    // 在工作台内，工作台条目改为折叠与展开侧栏，而不是重新打开工作台。
    h.wide.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    click(&mut h.app, "workbench entry", |t| {
        matches!(t, Target::AreaTab(Area::Chat))
    });
    assert_eq!(h.app.area, Area::Chat);
    let before = h.app.sidebar;
    h.wide.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    click(&mut h.app, "workbench entry", |t| {
        matches!(t, Target::AreaTab(Area::Chat))
    });
    assert_eq!(h.app.area, Area::Chat);
    assert_ne!(h.app.sidebar, before);
}

/// The memory recall notice shows while the conversation is in its PreCheck phase and leaves once
/// the answer starts.
/// 记忆召回通知在会话处于 PreCheck 阶段时显示，开始回答后消失。
#[test]
fn memory_recall_notice_follows_the_precheck_phase() {
    use crate::app::notice::live;
    use crate::model::RunPhase;

    let mut h = Harness::connected();
    let recalling = |app: &App| {
        live(app)
            .iter()
            .any(|notice| notice.key.as_deref() == Some("memory-recall"))
    };

    type_text(&mut h.app, "recall prompt");
    input::handle_event(&mut h.app, key(KeyCode::Enter, KeyModifiers::NONE));
    h.until("memory recall", |app| {
        app.run_phase() == RunPhase::MemoryRecall
    });
    assert!(
        recalling(&h.app),
        "the PreCheck phase raises the recall notice"
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    assert!(
        buffer_text(terminal.backend().buffer()).contains("VMM"),
        "the recall notice is drawn during PreCheck"
    );

    h.until("the answer to start", |app| {
        app.run_phase() == RunPhase::Running
    });
    assert!(
        !recalling(&h.app),
        "the recall notice leaves once the answer starts"
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    assert!(
        !buffer_text(terminal.backend().buffer()).contains("VMM"),
        "the recall notice is no longer drawn"
    );
}

/// The Changes tab lists every repository; a click selects a monitored one, and turning its
/// monitoring off returns the details to the primary repository.
/// 变更标签页列出全部仓库；点击可选择受监听仓库，关闭其监听后详情回到主仓库。
#[test]
fn changes_tab_selects_and_unmonitors_repositories() {
    let _locale = i18n::lock_locale("en-US");
    let mut h = Harness::connected();
    h.app.panel_tab = PanelTab::Changes;
    h.frame();
    let selected = |app: &App| {
        app.git
            .repository()
            .and_then(|snapshot| snapshot.selected.clone())
    };
    assert_eq!(selected(&h.app).as_deref(), Some("."));
    click(&mut h.app, "second repository", |t| {
        matches!(t, Target::Do(UiAction::SelectGitRepository(key))
            if key == "vendor/demo-lib")
    });
    h.settle();
    assert_eq!(selected(&h.app).as_deref(), Some("vendor/demo-lib"));
    snapshot(&mut h.app, "changes-repositories");
    h.frame();

    click(&mut h.app, "repository settings", |t| {
        matches!(t, Target::Do(UiAction::OpenMenu(menu))
            if menu.title == i18n::text("gitCommitDialog.monitoringManager"))
    });
    h.frame();
    let Some(Overlay::Menu(menu)) = &h.app.overlay else {
        panic!("the repository settings open a menu");
    };
    let index = menu
        .items
        .iter()
        .position(|item| {
            matches!(&item.action, UiAction::Send(Command::SetGitMonitoring { repository_key, enabled })
                if repository_key == "vendor/demo-lib" && !enabled)
        })
        .expect("the selected repository can be unmonitored");
    click(
        &mut h.app,
        "monitoring switch",
        |t| matches!(t, Target::MenuItem(i) if *i == index),
    );
    h.settle();
    assert_eq!(selected(&h.app).as_deref(), Some("."));
    assert_eq!(h.app.git.next_repository(), None);
}

/// With the labeled activity bar and the panel inline, the sidebar starts on the top row and the
/// panel tabs sit below the header cell; narrower layouts keep one header row across the window.
/// 标签式活动栏与面板同时内联时，侧栏从顶行开始，面板标签位于标题单元格下方；较窄布局保留横跨窗口的
/// 标题行。
#[test]
fn top_row_holds_the_sidebar_when_the_side_columns_carry_the_header() {
    let _locale = i18n::lock_locale("en-US");
    let mut h = Harness::connected();
    let top = |app: &App, pred: &dyn Fn(&Target) -> bool| app.hits.find(pred).map(|rect| rect.y);
    let new_conversation = |t: &Target| matches!(t, Target::Do(UiAction::NewConversation(_)));
    let panel_tab = |t: &Target| matches!(t, Target::PanelTab(_));
    h.frame();
    assert_eq!(top(&h.app, &new_conversation), Some(0));
    assert_eq!(top(&h.app, &panel_tab), Some(1));
    let mut narrow = Terminal::new(TestBackend::new(100, 30)).unwrap();
    narrow.draw(|frame| ui::draw(frame, &mut h.app)).unwrap();
    assert!(top(&h.app, &new_conversation).is_some_and(|y| y > 0));
}
