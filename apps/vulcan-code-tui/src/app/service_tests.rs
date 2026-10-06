// App-level regression tests use inert channels and never discover a real service.
// 应用层回归测试使用静默通道，绝不发现真实服务。

use super::action::{UiAction, perform};
use super::*;
use crate::service_api::{
    ServiceCall, Slot, knowledge_base, memory_actions, settings_actions, vce,
};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};

/// Creates an isolated app and returns its command observer without a backend worker.
/// 创建隔离应用并返回命令观察端，不运行后端工作器。
fn isolated_app() -> (App, Receiver<Command>) {
    // No external I/O is connected to either channel.
    // 两条通道都没有连接外部输入输出。
    let (commands, receiver) = mpsc::channel();
    // Events.
    // 事件集合或事件接收通道。
    let (_, events) = mpsc::channel();
    (
        App::new(
            BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            Theme::dark(),
            true,
        ),
        receiver,
    )
}

/// A form preserves exact secret bytes, masks its prompt and sends a completed request once.
/// 表单保留准确密钥字节、遮蔽输入框，并仅发送一次完整请求。
#[test]
fn secret_form_is_transient_masked_and_sends_once() {
    // App; Commands.
    // 当前应用状态；命令发送或观察通道。
    let (mut app, commands) = isolated_app();
    // Transient one-shot form.
    // 一次性临时输入表单。
    let form = form::Form {
        request: json!({"request":{"secret":"","displayName":"Demo"}}),
        fields: vec![form::FormField {
            title: "Credential".into(),
            pointer: "/request/secret".into(),
            secret: true,
            optional: false,
            parse: |input| Some(json!(input)),
            validate: |_, _| true,
        }],
        index: 0,
        save: form::FormSubmit::Prepared(ServiceCall::mutate(
            "add_public_ai_connection",
            "service_add_public_ai_connection",
            Value::Null,
            &[Slot::PublicAiSettings],
        )),
    };
    perform(&mut app, UiAction::Form(Box::new(form)));
    let Some(Overlay::Prompt(dialog)) = &mut app.overlay else {
        panic!("secret prompt")
    };
    assert!(dialog.secret);
    // An actual paste must preserve opaque secret bytes, including original CRLF and spaces.
    // 真实粘贴必须保留不透明密钥字节，包含原始 CRLF 及空格。
    input::handle_event(
        &mut app,
        ratatui::crossterm::event::Event::Paste("  synthetic-credential\r\n ".into()),
    );
    assert!(matches!(&app.overlay, Some(Overlay::Prompt(dialog))
        if !format!("{dialog:?}").contains("synthetic-credential")));
    perform(&mut app, UiAction::PromptAccept);
    let Command::Call(call) = commands.try_recv().unwrap() else {
        panic!("service call")
    };
    assert_eq!(call.param("/request/secret"), "  synthetic-credential\r\n ");
    assert!(commands.try_recv().is_err());
    assert!(app.overlay.is_none());
    assert!(!format!("{call:?}").contains("synthetic-credential"));
}

/// Resource input preserves pasted line breaks, indentation and explicit newline keys through confirmation.
/// 资源输入通过确认保留粘贴换行、缩进及明确换行按键。
#[test]
fn multiline_resource_events_preserve_the_complete_submission() {
    use super::action::{PromptDialog, PromptThen};
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    // Isolated UI and exact command observer.
    // 隔离界面及准确命令观察端。
    let (mut app, commands) = isolated_app();
    perform(
        &mut app,
        UiAction::Prompt(Box::new(PromptDialog::new(
            "Resource".into(),
            String::new(),
            String::new(),
            PromptThen::ResourceWrite("prompts/en-US/default.md".into()),
            false,
        ))),
    );
    input::handle_event(&mut app, Event::Paste("first\r\n  second".into()));
    input::handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
    );
    input::handle_event(&mut app, Event::Paste("第三行  ".into()));
    assert!(matches!(&app.overlay, Some(Overlay::Prompt(dialog))
        if dialog.input == "first\n  second\n第三行  "));
    assert!(commands.try_recv().is_err());
    input::handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );
    assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
    assert!(commands.try_recv().is_err());
    perform(&mut app, UiAction::ConfirmAccept);
    // The second confirmation sends exactly the reviewed complete text, once.
    // 第二次确认仅发送一次已审查的完整文本。
    let Command::Call(call) = commands.try_recv().unwrap() else {
        panic!("resource write")
    };
    assert_eq!(
        call.param("/request/payload/content"),
        "first\n  second\n第三行  "
    );
    assert_eq!(call.param("/request/payload/confirmedPurpose"), true);
    assert_eq!(call.param("/request/payload/confirmedModification"), true);
    assert!(commands.try_recv().is_err());
}

/// A pasted multiline setting reaches the exact authoritative array while preserving sibling fields.
/// 粘贴的多行设置到达准确权威数组，同时保留同级字段。
#[test]
fn multiline_setting_paste_keeps_every_array_entry() {
    use super::action::{Edit, PromptDialog, PromptThen};
    use crate::service_api::setter::Setter;
    use crate::service_api::skills_actions;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    // Isolated UI and exact command observer.
    // 隔离界面及准确命令观察端。
    let (mut app, commands) = isolated_app();
    perform(
        &mut app,
        UiAction::Prompt(Box::new(PromptDialog::new(
            "Allow list".into(),
            String::new(),
            String::new(),
            PromptThen::Edit(Edit::Call(Setter::StringList {
                base: json!({"network":{"privateSkillSourceAllowlist":[],"enablePrivateUrlSkillInstall":false}}),
                pointer: "/network/privateSkillSourceAllowlist".into(),
                save: skills_actions::save_luaskills_runtime_settings,
            })),
            false,
        ))),
    );
    input::handle_event(
        &mut app,
        Event::Paste("one.invalid\r\ntwo.invalid\rthree.invalid".into()),
    );
    input::handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );
    // Parsing applies the entire normalized list to its verified DTO pointer.
    // 解析将完整规范化列表应用到已确认 DTO 指针。
    let Command::Call(call) = commands.try_recv().unwrap() else {
        panic!("settings write")
    };
    assert_eq!(
        call.param("/settings/network/privateSkillSourceAllowlist"),
        &json!(["one.invalid", "two.invalid", "three.invalid"])
    );
    assert_eq!(
        call.param("/settings/network/enablePrivateUrlSkillInstall"),
        false
    );
    assert!(commands.try_recv().is_err());
}

/// Pending writes block another stale-document save while permitting independent reads.
/// 等待中的写入阻止另一份过时文档保存，同时允许独立读取。
#[test]
fn pending_mutation_gate_is_released_on_completion_or_disconnect() {
    // App; Commands.
    // 当前应用状态；命令发送或观察通道。
    let (mut app, commands) = isolated_app();
    // Mutation.
    // 测试使用的准确变更请求。
    let mutation = settings_actions::save_git_commit_settings(json!({"branchPrefix":"demo/"}));
    perform(&mut app, UiAction::Send(Command::Call(mutation.clone())));
    perform(&mut app, UiAction::Send(Command::Call(mutation.clone())));
    perform(
        &mut app,
        UiAction::Send(Command::Call(vce::load_vce_configuration_status())),
    );
    assert_eq!(commands.try_iter().count(), 2);
    assert!(app.call_pending);
    app.apply(BackendEvent::CallFinished);
    perform(&mut app, UiAction::Send(Command::Call(mutation)));
    assert_eq!(commands.try_iter().count(), 1);
    app.apply(BackendEvent::Connection(ConnectionState::Reconnecting));
    assert!(!app.call_pending);
}

/// Old library, project and profile responses never populate a newly selected page.
/// 旧知识库、项目及画像响应绝不填充到新选择的页面。
#[test]
fn scoped_responses_do_not_cross_selected_owners() {
    // App.
    // 当前应用状态。
    let (mut app, _) = isolated_app();
    app.library = Some("b".into());
    app.vce_project = Some("project-b".into());
    app.vmm_project = Some(102);
    app.profile_target = "project".into();
    for (slot, call) in [
        (
            Slot::KnowledgeSources,
            knowledge_base::list_knowledge_sources("a"),
        ),
        (
            Slot::VceOverview,
            vce::load_vce_project_overview("project-a"),
        ),
        (Slot::VmmRecords, memory_actions::native_memory_records(101)),
        (
            Slot::VmmProfile,
            memory_actions::native_profile_records(102, "user"),
        ),
    ] {
        app.apply(BackendEvent::Slot {
            slot,
            call,
            value: json!({"old":true}),
        });
        assert!(!app.slots.loaded(slot));
    }
    app.apply(BackendEvent::Slot {
        slot: Slot::VmmRecords,
        call: memory_actions::native_memory_records(102),
        value: json!({"projectId":102}),
    });
    assert_eq!(app.slots.at(Slot::VmmRecords, "/projectId"), 102);
}

/// Reconnection reloads the visible page and a removed catalog selection clears dependent slots.
/// 重连重新加载可见页面，删除目录选择项时清空依赖槽位。
#[test]
fn reconnect_and_catalog_removal_reload_current_scope() {
    // App; Commands.
    // 当前应用状态；命令发送或观察通道。
    let (mut app, commands) = isolated_app();
    app.area = pages::Area::Knowledge;
    app.library = Some("deleted".into());
    app.slots.set(
        Slot::KnowledgeSources,
        knowledge_base::list_knowledge_sources("deleted"),
        json!([{"id":"old"}]),
    );
    app.apply(BackendEvent::Connection(ConnectionState::Connected));
    assert!(
        commands
            .try_iter()
            .any(|call| matches!(call, Command::Call(_)))
    );
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeLibraries,
        call: knowledge_base::list_knowledge_libraries(),
        value: json!([{"id":"survivor"}]),
    });
    assert_eq!(app.library.as_deref(), Some("survivor"));
    assert!(!app.slots.loaded(Slot::KnowledgeSources));
}
