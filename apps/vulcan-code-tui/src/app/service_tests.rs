// App-level regression tests use inert channels and never discover a real service.
// 应用层回归测试使用静默通道，绝不发现真实服务。

use super::action::{UiAction, perform};
use super::*;
use crate::service_api::{ServiceCall, Slot, knowledge_base, memory_actions, settings_actions};
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

/// An empty answer to a stored secret keeps the service placeholder; an empty new secret stays empty.
/// 已存储机密留空回答保留服务占位符；新机密留空仍为空。
#[test]
fn empty_secret_answer_keeps_the_stored_secret_placeholder() {
    let placeholder = vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER;
    for (stored, expected) in [(placeholder, placeholder), ("", "")] {
        let (mut app, commands) = isolated_app();
        let form = form::Form {
            request: json!({"bearerToken": stored}),
            fields: vec![form::FormField {
                title: "Token".into(),
                pointer: "/bearerToken".into(),
                secret: true,
                optional: true,
                parse: |input| Some(json!(input)),
                validate: |_, _| true,
            }],
            index: 0,
            save: form::FormSubmit::Prepared(ServiceCall::mutate(
                "save_mcp_server_configuration",
                "service_save_mcp_server_configuration",
                Value::Null,
                &[],
            )),
        };
        perform(&mut app, UiAction::Form(Box::new(form)));
        perform(&mut app, UiAction::PromptAccept);
        let Command::Call(call) = commands.try_recv().unwrap() else {
            panic!("service call")
        };
        assert_eq!(call.param("/bearerToken"), expected);
    }
}

/// A live provider-task slot is re-read only while a task can still change phase.
/// 供应商任务槽位仅在仍有任务可能改变阶段时重读。
#[test]
fn provider_tasks_are_polled_only_while_live() {
    let (mut app, _commands) = isolated_app();
    for (tasks, armed) in [
        (json!([{"executionId": "a", "terminal": false}]), true),
        (json!([{"executionId": "a", "terminal": true}]), false),
    ] {
        app.slots.set(
            Slot::ProviderTasks,
            settings_actions::list_provider_tasks(),
            tasks,
        );
        app.arm_live_poll(Slot::ProviderTasks);
        assert_eq!(app.live_poll_due.contains_key(&Slot::ProviderTasks), armed);
    }
}

/// The permission judge status is re-read at the interval the service publishes in it.
/// 权限判定状态按服务在其中发布的间隔重读。
#[test]
fn judge_status_is_polled_at_its_published_interval() {
    let (mut app, _commands) = isolated_app();
    app.slots.set(
        Slot::PermissionJudgeStatus,
        crate::service_api::permission_judge::load_permission_judge_status(),
        json!({"worker": {}, "cudaInstall": null, "pollIntervalMs": 500}),
    );
    app.arm_live_poll(Slot::PermissionJudgeStatus);
    assert_eq!(
        app.live_poll_due.get(&Slot::PermissionJudgeStatus),
        Some(&(app.now + Duration::from_millis(500)))
    );
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
        UiAction::Send(Command::Call(settings_actions::load_public_ai_settings())),
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
    app.vmm_project = Some(102);
    app.profile_target = "project".into();
    for (slot, call) in [
        (
            Slot::KnowledgeWords,
            knowledge_base::list_knowledge_custom_words("a"),
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
        Slot::KnowledgeWords,
        knowledge_base::list_knowledge_custom_words("deleted"),
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
        call: knowledge_base::list_knowledge_libraries(None),
        value: json!([{"id":"survivor"}]),
    });
    assert_eq!(app.library.as_deref(), Some("survivor"));
    assert!(!app.slots.loaded(Slot::KnowledgeWords));
}

/// The system prompt editor follows the loaded scope: unchanged text rereads, edits save to the
/// snapshot's scope, and restore exists only while that scope has an override.
/// 系统提示词编辑器跟随已加载作用域：未修改的正文只重读，编辑保存到快照的作用域，且仅在该作用域
/// 存在覆盖时才提供恢复。
#[test]
fn system_prompt_editor_targets_the_loaded_scope() {
    let (mut app, commands) = isolated_app();
    let load = settings_actions::load_system_prompt_settings(Some("p-core"));
    app.apply(BackendEvent::Slot {
        slot: Slot::SystemPrompt,
        call: load.clone(),
        value: json!({"builtInText":"built-in","userPath":"u","userText":"user rules",
            "projectId":"p-core","projectPath":"p","projectText":null,"effectiveSource":"user"}),
    });
    let rows = pages::rows(&app, pages::PageId::Context);
    // Editor.
    // 正文编辑行。
    let edit = rows
        .iter()
        .find_map(|row| match &row.kind {
            pages::RowKind::Text { value, edit, .. } => {
                assert_eq!(
                    value, "user rules",
                    "a project without override inherits the user text"
                );
                Some(edit.clone())
            }
            _ => None,
        })
        .expect("system prompt editor");
    // Restore button label for the project scope, resolved from its catalog key.
    // 项目作用域恢复按钮的标签，由目录键解析。
    let restore = crate::tr!("settings.systemPrompt.resetProject");
    assert!(
        !rows.iter().any(|row| row.label == restore),
        "restore requires an override in the selected scope"
    );
    action::apply_edit(
        &mut app,
        edit.clone(),
        SettingValue::Text("user rules\n".into()),
    );
    action::apply_edit(&mut app, edit, SettingValue::Text("project rules".into()));
    let calls: Vec<_> = commands
        .try_iter()
        .filter_map(|command| match command {
            Command::Call(call) => Some(call),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].method, "service_system_prompt_settings");
    assert_eq!(calls[0].param("/projectId"), "p-core");
    assert_eq!(calls[1].method, "service_save_system_prompt");
    assert_eq!(
        calls[1].params,
        json!({"request":{"scope":"project","projectId":"p-core","text":"project rules"}})
    );
}

/// The official direct endpoint table is re-read at the status interval, so the background probe
/// shows up without the page probing.
/// 官方直连端点表按状态间隔重读，使后台探测结果无需页面探测即可显示。
#[test]
fn judge_endpoint_table_is_polled_at_the_status_interval() {
    let (mut app, _commands) = isolated_app();
    app.slots.set(
        Slot::PermissionJudgeStatus,
        crate::service_api::permission_judge::load_permission_judge_status(),
        json!({"worker": {}, "cudaInstall": null, "pollIntervalMs": 700}),
    );
    app.slots.set(
        Slot::PermissionJudgeEdgeEndpoints,
        crate::service_api::permission_judge::load_permission_judge_edge_endpoints(),
        json!({"endpoints": []}),
    );
    app.arm_live_poll(Slot::PermissionJudgeEdgeEndpoints);
    assert_eq!(
        app.live_poll_due.get(&Slot::PermissionJudgeEdgeEndpoints),
        Some(&(app.now + Duration::from_millis(700)))
    );
}

/// With the direct backend saved, the page offers automatic selection or a lock per endpoint and
/// a re-test that never replaces the table's loader.
/// 已保存官方直连后端时，页面提供自动选择或按端点锁定，以及不替换端点表加载器的重新测试。
#[test]
fn judge_endpoint_rows_lock_and_retest_through_their_calls() {
    use super::action::Edit;
    use super::pages::{PageId, RowKind, rows};
    use crate::model::SettingValue;
    use crate::service_api::CallKind;
    use crate::service_api::setter::Setter;
    let (mut app, _commands) = isolated_app();
    app.slots.set(
        Slot::PermissionJudgeSettings,
        crate::service_api::permission_judge::load_permission_judge_settings(),
        json!({"backend": "edge", "edgeModelId": "m", "edgeEndpointId": null, "models": [],
            "devices": [], "knownPackages": [], "policyOptions": []}),
    );
    app.slots.set(
        Slot::PermissionJudgeEdgeEndpoints,
        crate::service_api::permission_judge::load_permission_judge_edge_endpoints(),
        json!({"modelId": "m", "site": "global", "mode": "auto", "lockedEndpointId": null,
            "selectedEndpointId": "a", "probedAtMs": null, "nextProbeAtMs": null, "error": null,
            "endpoints": [
                {"endpointId": "a", "endpoint": "https://a.example.com", "priority": 10, "online": true,
                 "region": "r", "ready": true, "acceptsSite": true, "rttMs": 10, "computeP50Ms": null,
                 "scoreMs": 10, "error": null},
                {"endpointId": "b", "endpoint": "https://b.example.com", "priority": 20, "online": true,
                 "region": null, "ready": null, "acceptsSite": null, "rttMs": null, "computeP50Ms": null,
                 "scoreMs": null, "error": "down"}
            ]}),
    );
    let page = rows(&app, PageId::PermissionJudge);
    assert!(
        page.iter()
            .any(|row| matches!(&row.kind, RowKind::Item { id, .. } if id == "b"))
    );
    let lock = page
        .iter()
        .find_map(|row| match &row.kind {
            RowKind::Choice {
                options,
                edit: Edit::Call(Setter::Build { ctx, build }),
                ..
            } if options.iter().any(|(id, _)| id == "b") => Some((ctx.clone(), *build)),
            _ => None,
        })
        .expect("the endpoint choice");
    for (value, expected) in [("b", json!("b")), ("*", Value::Null)] {
        let call = (lock.1)(&lock.0, &SettingValue::Choice(value.into())).unwrap();
        assert_eq!(call.method, "service_lock_permission_judge_edge_endpoint");
        assert_eq!(call.params, json!({"request": {"endpointId": expected}}));
    }
    let probe = page
        .iter()
        .find_map(|row| match &row.kind {
            RowKind::Button {
                action: UiAction::Send(Command::Call(call)),
                ..
            } if call.method == "service_probe_permission_judge_edge_endpoints" => {
                Some(call.clone())
            }
            _ => None,
        })
        .expect("the re-test button");
    assert!(matches!(probe.kind, CallKind::Mutate(_)));
}

/// Each official direct endpoint sends a sample or a typed command as a request test of that
/// endpoint, the result lists every request, and the direct backend offers no speed test.
/// 每个官方直连端点都能将示例或输入的命令作为该端点的请求测试发送，结果列出每个请求，且直连后端不提供速度测试。
#[test]
fn judge_endpoint_request_test_targets_its_endpoint_and_lists_each_request() {
    use super::action::Edit;
    use super::pages::{PageId, RowKind, rows};
    use crate::model::SettingValue;
    use crate::service_api::permission_judge as judge;
    use crate::service_api::setter::Setter;
    let (mut app, _commands) = isolated_app();
    app.slots.set(
        Slot::PermissionJudgeSettings,
        judge::load_permission_judge_settings(),
        json!({"backend": "edge", "edgeModelId": "m", "edgeEndpointId": null, "models": [],
            "devices": [], "knownPackages": [], "policyOptions": []}),
    );
    app.slots.set(
        Slot::PermissionJudgeEdgeEndpoints,
        judge::load_permission_judge_edge_endpoints(),
        json!({"modelId": "m", "site": "global", "mode": "auto", "lockedEndpointId": null,
        "selectedEndpointId": "a", "probedAtMs": null, "nextProbeAtMs": null, "error": null,
        "endpoints": [
            {"endpointId": "a", "endpoint": "https://a.example.com", "priority": 10, "online": true,
             "region": "r", "ready": true, "acceptsSite": true, "rttMs": 10, "computeP50Ms": null,
             "scoreMs": 10, "error": null},
            {"endpointId": "b", "endpoint": "https://b.example.com", "priority": 20, "online": true,
             "region": "r", "ready": true, "acceptsSite": true, "rttMs": 20, "computeP50Ms": null,
             "scoreMs": 20, "error": null}
        ]}),
    );
    let page = rows(&app, PageId::PermissionJudge);
    assert!(!page.iter().any(|row| matches!(&row.kind, RowKind::Section)
        && row.label == crate::tr!("settings.permissionJudge.benchmark")));
    let actions = page
        .iter()
        .find_map(|row| match &row.kind {
            RowKind::Item { id, actions, .. } if id == "b" => Some(actions.clone()),
            _ => None,
        })
        .expect("the endpoint row");
    let preset = actions
        .iter()
        .find_map(|action| match &action.action {
            Some(UiAction::Pick {
                edit: Edit::Call(Setter::Build { ctx, build }),
                options,
                ..
            }) => Some((ctx.clone(), *build, options.clone())),
            _ => None,
        })
        .expect("the sample picker");
    assert!(preset.2.iter().any(|(value, _)| value == "database_reset"));
    let call = (preset.1)(&preset.0, &SettingValue::Choice("database_reset".into())).unwrap();
    assert_eq!(call.method, "service_test_permission_judge_edge_request");
    assert_eq!(
        call.params,
        json!({"request": {"endpointId": "b", "input": {"kind": "preset", "preset": "database_reset"}}})
    );
    let custom = actions
        .iter()
        .find_map(|action| match &action.action {
            Some(UiAction::Form(form)) => Some(form.clone()),
            _ => None,
        })
        .expect("the custom command form");
    assert_eq!(
        custom.request,
        json!({"endpointId": "b", "input": {"kind": "custom", "command": ""}})
    );
    assert_eq!(custom.fields[0].pointer, "/input/command");

    let attempt = |request_id: &str, error: Value| {
        json!({"requestId": request_id, "totalMs": 120.0, "networkMs": 75.0, "endpointMs": 45.0,
            "nodeMs": 40.0, "queueMs": 2.0, "computeMs": 31.0, "httpStatus": 200, "tokenRenewed": false,
            "answer": {"region": "r", "model": "m", "checkpointSha256": "c", "policyVersion": "p",
                "results": [{"label": "allow", "logits": [2.0, 0.5, -1.0], "inputTokens": 41,
                    "modelEvaluated": true, "reasonCode": null}]},
            "errorCode": null, "error": error})
    };
    app.slots.set(
        Slot::PermissionJudgeEdgeRequestTest,
        call,
        json!({"modelId": "m", "endpointId": "b", "endpoint": "https://b.example.com", "site": "global",
            "renderedChars": 52, "attempts": [attempt("r1", Value::Null), attempt("r2", json!("mismatch"))],
            "succeeded": 1, "minMs": 120.0, "medianMs": 120.0, "maxMs": 120.0}),
    );
    let page = rows(&app, PageId::PermissionJudge);
    let requests: Vec<_> = page
        .iter()
        .filter_map(|row| match &row.kind {
            RowKind::Info { tone, .. } if row.label.starts_with('#') => Some(*tone),
            _ => None,
        })
        .collect();
    assert_eq!(
        requests,
        [super::pages::Tone::Good, super::pages::Tone::Bad]
    );
    assert!(page.iter().any(|row| matches!(
        &row.kind,
        RowKind::Note {
            tone: super::pages::Tone::Bad
        }
    ) && row.detail.contains("r2: mismatch")));
}
