// Memory mode and Markdown memory pages driven end to end through the synthetic mock service.
// 通过合成模拟服务端到端驱动记忆模式与 Markdown 记忆页面。

use super::action::{Edit, UiAction, perform};
use super::pages::{Area, PageId, Row, RowKind, navigation, rows};
use super::*;
use crate::backend::{Command, mock};
use crate::model::SettingValue;
use crate::service_api::markdown_memory as markdown;
use crate::service_api::{ServiceCall, Slot};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;

/// Applies backend events until `done` holds and no write is pending.
/// 持续应用后端事件，直到 `done` 成立且没有待完成的写入。
fn settle(app: &mut App, driver: &mut mock::Inline, what: &str, done: impl Fn(&App) -> bool) {
    while !done(app) || app.call_pending {
        let event = driver
            .next_event(&app.backend.events)
            .unwrap_or_else(|| panic!("mock never reached: {what}"));
        app.apply(event);
    }
}

/// Labels of the navigation rows of the current area.
/// 当前区域导航行的名称。
fn nav_labels(app: &App) -> Vec<String> {
    navigation(app)
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .map(|entry| entry.label)
        .collect()
}

/// Rows of the page currently shown.
/// 当前显示页面的行。
fn shown(app: &App) -> Vec<Row> {
    rows(app, app.current_page().expect("a memory page"))
}

/// Draws the current page at a wide and a narrow size.
/// 以宽、窄两种尺寸绘制当前页面。
fn draw(app: &mut App) {
    for (width, height) in [(160, 45), (64, 20)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    }
}

/// Opens `page` and waits for its overview.
/// 打开 `page` 并等待其总览。
fn open(app: &mut App, driver: &mut mock::Inline, page: PageId) {
    app.slots.clear(Slot::MarkdownMemory);
    perform(app, UiAction::GoPage(page));
    settle(app, driver, "overview", |app| {
        app.slots.loaded(Slot::MarkdownMemory)
    });
}

// Switching to Markdown mode replaces the VMM pages; the scope pages list notes, read the file
// for editing and follow the chosen host project.
// 切换到 Markdown 模式会替换 VMM 页面；层页面列出笔记、读取文件以供编辑，并跟随所选宿主项目。
#[test]
fn markdown_mode_pages_follow_the_mode_and_render() {
    let _locale = crate::i18n::lock_locale("en-US");
    let (backend, mut driver) = mock::inline();
    let mut app = App::new(backend, Theme::dark(), true);
    settle(&mut app, &mut driver, "workbench", |app| {
        app.slots.loaded(Slot::Workbench)
    });

    // The demo starts in VMM mode: VMM pages plus the mode page, no Markdown scopes.
    // 演示以 VMM 模式启动：VMM 页面加模式页，没有 Markdown 层。
    perform(&mut app, UiAction::Area(Area::Memory));
    settle(&mut app, &mut driver, "memory mode", |app| {
        app.memory_mode.is_some()
    });
    assert_eq!(app.current_page(), Some(PageId::MemoryOverview));
    let labels = nav_labels(&app);
    assert!(labels.contains(&PageId::MemoryMode.label()));
    assert!(!labels.contains(&PageId::MarkdownUser.label()));

    // Choosing Markdown on the mode page saves it and swaps the navigation.
    // 在模式页选择 Markdown 会保存并切换导航。
    open(&mut app, &mut driver, PageId::MemoryMode);
    let mode = shown(&app)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Choice { edit, .. } => Some(edit),
            _ => None,
        })
        .expect("the mode choice");
    perform(
        &mut app,
        UiAction::Edit(mode, SettingValue::Choice("markdown".into())),
    );
    settle(&mut app, &mut driver, "markdown mode", |app| {
        app.memory_mode.as_deref() == Some("markdown")
    });
    assert_eq!(app.current_page(), Some(PageId::MemoryMode));
    let labels = nav_labels(&app);
    assert!(labels.contains(&PageId::MarkdownProject.label()));
    assert!(!labels.contains(&PageId::MemoryOverview.label()));
    let budget = shown(&app)
        .iter()
        .filter(|row| matches!(row.kind, RowKind::Number { .. }))
        .count();
    assert_eq!(budget, 5, "one number row per budget part");
    draw(&mut app);

    // The user scope lists its notes and opens the file for whole-file editing.
    // 用户层列出其笔记，并打开文件以整文件编辑。
    open(&mut app, &mut driver, PageId::MarkdownUser);
    assert!(
        shown(&app)
            .iter()
            .any(|row| matches!(row.kind, RowKind::Item { .. }) && row.label.trim() == "Role")
    );
    let read = shown(&app)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Button { action, .. } if row.label == "Edit file" => Some(action),
            _ => None,
        })
        .expect("the file button");
    perform(&mut app, read);
    settle(&mut app, &mut driver, "file", |app| {
        app.slots.loaded(Slot::MarkdownMemoryFile)
    });
    let file = shown(&app)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Text { edit, .. } if row.label == "memory/user.md" => Some(edit),
            _ => None,
        })
        .expect("the file text row");
    assert!(matches!(file, Edit::Call(_)));
    draw(&mut app);

    // The project scope shows the overview of the chosen host project only.
    // 项目层只显示所选宿主项目的总览。
    open(&mut app, &mut driver, PageId::MarkdownProject);
    let project = app.markdown_memory_project.clone().expect("a host project");
    assert_eq!(
        app.slots
            .at(Slot::MarkdownMemory, "/hostProjectId")
            .as_str(),
        Some(project.as_str())
    );
    assert!(shown(&app).iter().any(|row| row.label.trim() == "Build"));
    draw(&mut app);

    // Import: the package preview asks for the project target first, then offers the conflict
    // with its newer side chosen and applies the reviewed resolutions.
    // 导入：导出包预览先要求项目去向，再以较新一侧为默认提供冲突，并应用已审阅的解决方式。
    open(&mut app, &mut driver, PageId::MemoryMode);
    perform(
        &mut app,
        UiAction::Send(Command::Call(markdown::preview_markdown_memory_import(
            "X:/demo/memory.zip",
            json!([]),
        ))),
    );
    settle(&mut app, &mut driver, "import preview", |app| {
        app.slots.loaded(Slot::MarkdownMemoryImport)
    });
    assert!(
        apply_button(&app).is_none(),
        "every project needs a target first"
    );
    let create_rest = shown(&app)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Button { action, .. } if row.label == "Create the rest" => Some(action),
            _ => None,
        })
        .expect("the create-the-rest button");
    perform(&mut app, create_rest);
    settle(&mut app, &mut driver, "mapped preview", |app| {
        app.slots
            .loader(Slot::MarkdownMemoryImport)
            .is_some_and(|loader| loader.params.pointer("/request/projects/0").is_some())
    });
    let request = apply_button(&app).expect("the apply button").params;
    assert_eq!(
        request.pointer("/request/resolutions/0/choice"),
        Some(&json!("take_imported"))
    );
    assert_eq!(
        request.pointer("/request/projects/0/target/action"),
        Some(&json!("create"))
    );
    draw(&mut app);
}

/// The import call behind the apply button, when the page offers one.
/// 页面提供应用按钮时，其背后的导入调用。
fn apply_button(app: &App) -> Option<ServiceCall> {
    shown(app).into_iter().find_map(|row| match row.kind {
        RowKind::Button {
            action: UiAction::Send(Command::Call(call)),
            ..
        } if call.method == "service_import_markdown_memory" => Some(call),
        _ => None,
    })
}
