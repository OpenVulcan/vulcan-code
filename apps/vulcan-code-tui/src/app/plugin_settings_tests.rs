// Plugin settings layouts driven end to end through the synthetic mock service.
// 通过合成模拟服务端到端驱动插件设置版面。

use std::time::{Duration, Instant};

use super::action::{Edit, LocalSetting, UiAction, perform};
use super::pages::{PageId, Row, RowKind, navigation, rows};
use super::*;
use crate::backend::mock;
use crate::model::SettingValue;
use crate::service_api::{CallKind, Slot, skills_actions};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

/// Applies backend events until `done` holds; fails when the mock never gets there.
/// 持续应用后端事件直到 `done` 成立；模拟后端始终未到达时失败。
/// Writes count as done only once the service finished them, since one write runs at a time.
/// 写入只有在服务完成后才算完成，因为同一时间只运行一个写入。
fn settle(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) || app.call_pending {
        assert!(Instant::now() < deadline, "mock never reached: {what}");
        if let Ok(event) = app.backend.events.recv_timeout(Duration::from_millis(50)) {
            app.apply(event);
        }
    }
}

/// The open plugin page's rows.
/// 当前插件页面的行。
fn page(app: &App) -> Vec<Row> {
    rows(app, PageId::PluginMount)
}

/// The edit of the first row labelled `label`.
/// 第一个名称为 `label` 的行的编辑。
fn edit_of(app: &App, label: &str) -> Edit {
    page(app)
        .into_iter()
        .find(|row| row.label == label)
        .and_then(|row| match row.kind {
            RowKind::Toggle { edit, .. }
            | RowKind::Choice { edit, .. }
            | RowKind::Number { edit, .. }
            | RowKind::Text { edit, .. } => Some(edit),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no editable row `{label}`"))
}

/// Applies `value` to the first editable row labelled `label`.
/// 将 `value` 应用到第一个名称为 `label` 的可编辑行。
fn set(app: &mut App, label: &str, value: SettingValue) {
    let edit = edit_of(app, label);
    perform(app, UiAction::Edit(edit, value));
}

/// Presses the first button labelled `label`.
/// 按下第一个名称为 `label` 的按钮。
fn press(app: &mut App, label: &str) {
    let action = button_of(app, label);
    perform(app, action);
}

/// The action of the first button labelled `label`.
/// 第一个名称为 `label` 的按钮的动作。
fn button_of(app: &App, label: &str) -> UiAction {
    page(app)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Button { action, .. } if row.label == label => Some(action),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no button `{label}`"))
}

/// The configuration the mock currently serves through the page.
/// 模拟后端当前经页面提供的配置。
fn config(app: &App, pointer: &str) -> Value {
    app.slots.at(Slot::PluginConfig, pointer).clone()
}

/// An action call carries the renderer identity, a fresh invocation and no workspace; replays
/// renew only the invocation.
/// Action 调用携带渲染器身份、新的调用标识且不带工作区；重放只更新调用标识。
#[test]
fn plugin_action_calls_carry_identities_and_renew_on_replay() {
    let call = skills_actions::invoke_plugin_action(
        "plugin",
        "settings-get",
        json!({}),
        "runtime-1",
        CallKind::Load(Slot::PluginConfig),
    );
    assert_eq!(call.method, "service_invoke_plugin_action");
    assert_eq!(call.param("/pluginId"), "plugin");
    assert_eq!(call.param("/actionId"), "settings-get");
    assert_eq!(call.param("/runtimeIdentity"), "runtime-1");
    assert_eq!(call.param("/workspacePath"), &Value::Null);
    assert!(call.param("/identity/clientId").is_string());
    let replay = call.renewed();
    assert_eq!(
        replay.param("/identity/clientId"),
        call.param("/identity/clientId")
    );
    assert_ne!(
        replay.param("/identity/invocationId"),
        call.param("/identity/invocationId")
    );
    let other = skills_actions::invoke_plugin_action(
        "plugin",
        "settings-get",
        json!({}),
        "runtime-1",
        CallKind::Load(Slot::PluginConfig),
    );
    assert_ne!(
        other.param("/identity/invocationId"),
        call.param("/identity/invocationId")
    );
}

/// A settings mount opens from navigation, saves fields and credentials through its actions,
/// loads tab data lazily and renders at both widths.
/// 设置挂载从导航打开，经其 Action 保存字段与凭据，按需加载标签数据，并在两种宽度下渲染。
#[test]
fn plugin_settings_layout_reads_writes_and_renders() {
    crate::i18n::set_active_locale(crate::i18n::resolve_locale(Some("en-US"), None));
    let mut app = App::new(mock::spawn(), Theme::dark(), true);
    settle(&mut app, "workbench", |app| {
        app.slots.loaded(Slot::Workbench)
    });

    // The mount is a navigation row of the plugins area.
    // 挂载是插件区域的一个导航行。
    perform(&mut app, UiAction::Area(pages::Area::Plugins));
    let index = navigation(&app)
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .position(|entry| entry.label == "Demo settings")
        .expect("settings mount listed in navigation");
    perform(&mut app, UiAction::Navigate(index));
    assert_eq!(app.current_page(), Some(PageId::PluginMount));
    settle(&mut app, "configuration and credential", |app| {
        app.slots.loaded(Slot::PluginConfig) && app.slots.loaded(Slot::PluginCredential(0))
    });
    assert!(
        page(&app)
            .iter()
            .any(|row| row.label.contains("Configuration loaded")),
        "header status reflects the configuration read"
    );

    // A switch writes the whole configuration; the reload proves replays renew their identity.
    // 开关写入整份配置；重新加载证明重放会更新调用身份。
    set(&mut app, "Fast mode", SettingValue::Bool(true));
    settle(&mut app, "fast mode saved", |app| {
        config(app, "/fast") == json!(true)
    });
    assert_eq!(config(&app, "/label"), "demo", "sibling fields are kept");
    assert!(app.slot_errors.is_empty(), "{:?}", app.slot_errors);

    // Conditional fields follow the saved value; checklist entries store the selection.
    // 条件字段跟随已保存的值；清单条目保存所选项。
    assert!(!page(&app).iter().any(|row| row.label == "git status"));
    set(
        &mut app,
        "Route mode",
        SettingValue::Choice("\"selected\"".into()),
    );
    settle(&mut app, "mode saved", |app| {
        config(app, "/mode") == json!("selected")
    });
    set(&mut app, "git status", SettingValue::Bool(true));
    settle(&mut app, "route checked", |app| {
        config(app, "/routes") == json!(["git status"])
    });

    // Out-of-range input is refused before any write.
    // 超出范围的输入在任何写入前被拒绝。
    set(&mut app, "Timeout (seconds)", SettingValue::Number(900));
    assert!(app.overlay.is_none());
    assert_eq!(config(&app, "/timeout"), json!(30));

    // A credential is typed once into a masked prompt; only its state comes back.
    // 凭据在遮蔽输入框中只输入一次；只返回其状态。
    let credential = page(&app)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Item { id, actions, .. } if id == "credential:api-key" => Some(actions),
            _ => None,
        })
        .expect("credential item");
    perform(&mut app, credential[0].action.clone());
    assert!(matches!(&app.overlay, Some(overlay::Overlay::Prompt(dialog)) if dialog.secret));
    input::handle_event(
        &mut app,
        ratatui::crossterm::event::Event::Paste("synthetic-key".into()),
    );
    perform(&mut app, UiAction::PromptAccept);
    settle(&mut app, "credential stored", |app| {
        app.slots.at(Slot::PluginCredential(0), "/configured") == &json!(true)
    });
    press(&mut app, "Test connection");
    settle(&mut app, "credential verified", |app| {
        app.slots.at(Slot::PluginCredential(0), "/validationStatus") == "valid"
    });

    app.focus = Focus::AreaPage;
    crate::render_smoke_tests::snapshot(&mut app, "plugin-settings-general");

    // Package documents read in place and close again.
    // 包内文档就地阅读并可再次关闭。
    press(&mut app, "License");
    settle(&mut app, "license", |app| {
        app.slots.loaded(Slot::PluginAsset)
    });
    assert!(page(&app).iter().any(|row| matches!(&row.kind,
        RowKind::Document { content } if content.contains("Synthetic text"))));
    press(&mut app, &tr!("tui.plugin.closeDocument"));
    assert!(!app.slots.loaded(Slot::PluginAsset));

    // Data loads when its tab opens; arguments and periods change what is shown.
    // 数据在其标签打开时加载；参数与周期改变显示内容。
    assert!(!app.slots.loaded(Slot::PluginData(0)));
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::PluginTab),
            SettingValue::Choice("stats".into()),
        ),
    );
    settle(&mut app, "statistics", |app| {
        app.slots.loaded(Slot::PluginData(0))
    });
    let shown = page(&app);
    assert!(
        shown
            .iter()
            .any(|row| matches!(row.kind, RowKind::Stats(_)))
    );
    assert!(
        shown
            .iter()
            .any(|row| matches!(row.kind, RowKind::Spark { .. }))
    );
    assert!(shown.iter().any(|row| row.label == "1. git status"));
    set(&mut app, "Range", SettingValue::Choice("30".into()));
    settle(&mut app, "thirty days", |app| {
        app.slots
            .at(Slot::PluginData(0), "/daily")
            .as_array()
            .is_some_and(|days| days.len() == 30)
    });
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::PluginChartPeriod),
            SettingValue::Choice(format!("usage/1{}weekly", pages::PERIOD_SEPARATOR)),
        ),
    );
    assert!(page(&app).iter().any(|row| matches!(&row.kind,
        RowKind::Spark { text, .. } if text.contains("W39"))));

    crate::render_smoke_tests::snapshot(&mut app, "plugin-settings-stats");
    for (width, height) in [(160, 45), (64, 20)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        app.focus = Focus::AreaPage;
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("Demo settings"),
            "page title at {width} columns"
        );
    }
}
