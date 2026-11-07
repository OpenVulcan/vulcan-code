// The feedback and messages page: the preview and send protocol on an isolated app, then the whole
// page driven end to end through the synthetic mock service.
// 反馈与消息页：先在隔离应用上验证预览与发送协议，再通过合成模拟服务端到端驱动整个页面。

use super::action::{Edit, LocalSetting, UiAction, perform};
use super::feedback::{FeedbackAction, FeedbackField, can_send, fresh_preview};
use super::pages::{PageId, Row, RowKind, rows};
use super::*;
use crate::backend::{Command, mock};
use crate::model::SettingValue;
use crate::service_api::{ServiceCall, Slot, feedback as api};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};

/// Creates an isolated app and returns its command observer without a backend worker.
/// 创建隔离应用并返回命令观察端，不运行后端工作器。
fn isolated_app() -> (App, Receiver<Command>) {
    let (commands, receiver) = mpsc::channel();
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

/// Edits one compose field the way its page row does.
/// 以页面行的方式编辑一个编写字段。
fn edit(app: &mut App, field: FeedbackField, value: SettingValue) {
    perform(
        app,
        UiAction::Edit(Edit::Local(LocalSetting::Feedback(field)), value),
    );
}

/// The next service call the app sent, if any.
/// 应用发出的下一个服务调用（如有）。
fn sent(commands: &Receiver<Command>) -> Option<ServiceCall> {
    match commands.try_recv().ok()? {
        Command::Call(call) => Some(call),
        _ => None,
    }
}

/// A synthetic preview answer for `call`, reusing its draft id or issuing `issued`.
/// 针对 `call` 的合成预览应答，复用其草稿编号或签发 `issued`。
fn answer_preview(app: &mut App, call: ServiceCall, issued: &str, digest: &str) {
    let draft_id = call
        .param("/draft/draftId")
        .as_str()
        .unwrap_or(issued)
        .to_string();
    app.apply(BackendEvent::Slot {
        slot: Slot::FeedbackPreview,
        call,
        value: json!({
            "draftId": draft_id, "digest": digest, "payload": "{}", "byteLength": 2,
            "redactionHits": {}, "logLineCount": 0, "signedIn": true,
        }),
    });
}

// Send transmits exactly the shown preview's draft id and digest; an edit drops the preview, a
// late preview of older content is never offered, and later previews reuse the issued draft id.
// 发送只传输显示中预览的草稿编号与摘要；编辑会丢弃预览，旧内容的迟到预览不会被提供，之后的预览复用
// 已签发的草稿编号。
#[test]
fn send_uses_the_previewed_digest_and_edits_drop_the_preview() {
    let _locale = crate::i18n::lock_locale("en-US");
    let (mut app, commands) = isolated_app();
    edit(
        &mut app,
        FeedbackField::Title,
        SettingValue::Text("Title".into()),
    );
    edit(
        &mut app,
        FeedbackField::Body,
        SettingValue::Text("Body".into()),
    );

    perform(&mut app, UiAction::Feedback(FeedbackAction::Preview));
    let first = sent(&commands).expect("a prepare call");
    assert_eq!(first.method, "service_prepare_feedback");
    assert_eq!(first.param("/draft/draftId"), &Value::Null);
    assert_eq!(first.param("/draft/source"), "manual");
    assert_eq!(first.param("/draft/includeLog"), true);
    assert_eq!(first.param("/draft/includeClientInfo"), true);
    assert_eq!(first.param("/draft/allowContact"), false);
    answer_preview(&mut app, first, "draft-1", "digest-1");
    assert!(can_send(&app));

    perform(&mut app, UiAction::Feedback(FeedbackAction::Send));
    let submit = sent(&commands).expect("a submit call");
    assert_eq!(submit.method, "service_submit_feedback");
    assert_eq!(
        submit.params,
        json!({ "draftId": "draft-1", "digest": "digest-1" })
    );
    assert!(!can_send(&app), "a preview being sent is not sent twice");
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::FeedbackReceipt,
        call: submit,
        message: "synthetic failure".into(),
    });
    assert!(can_send(&app), "a failed send may be sent again");

    // An edit drops the preview, so nothing can be sent until the next preview.
    // 编辑会丢弃预览，因此在下一次预览之前无法发送。
    edit(
        &mut app,
        FeedbackField::AllowContact,
        SettingValue::Bool(true),
    );
    assert!(fresh_preview(&app).is_none());
    perform(&mut app, UiAction::Feedback(FeedbackAction::Send));
    assert!(sent(&commands).is_none(), "send needs a fresh preview");

    // A preview prepared before the edit arrives late and is not offered.
    // 编辑前准备的预览迟到时不会被提供。
    let stale = api::prepare_feedback(json!({ "draftId": "draft-1", "kind": "suggestion",
        "source": "manual", "title": "Title", "body": "Body", "allowContact": false,
        "includeClientInfo": true, "includeLog": true, "error": null }));
    answer_preview(&mut app, stale, "draft-1", "digest-stale");
    assert!(fresh_preview(&app).is_none());
    assert!(!can_send(&app));

    // The next preview reuses the issued draft id and carries the edited content.
    // 下一次预览复用已签发的草稿编号，并携带编辑后的内容。
    perform(&mut app, UiAction::Feedback(FeedbackAction::Preview));
    let second = sent(&commands).expect("a second prepare call");
    assert_eq!(second.param("/draft/draftId"), "draft-1");
    assert_eq!(second.param("/draft/allowContact"), true);
    answer_preview(&mut app, second, "unused", "digest-2");
    perform(&mut app, UiAction::Feedback(FeedbackAction::Send));
    assert_eq!(
        sent(&commands).expect("a second submit").params,
        json!({ "draftId": "draft-1", "digest": "digest-2" })
    );
}

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

/// Draws the current page at a wide and a narrow size.
/// 以宽、窄两种尺寸绘制当前页面。
fn draw(app: &mut App) {
    for (width, height) in [(160, 45), (64, 20)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    }
}

/// The first page action calling `method`, from a button row or an item chip; an action behind a
/// confirmation is returned as confirmed.
/// 调用 `method` 的第一个页面动作，来自按钮行或条目按钮；需确认的动作按已确认返回。
fn page_call(app: &App, method: &str) -> Option<UiAction> {
    let confirmed = |action: UiAction| match action {
        UiAction::Confirm(dialog) => dialog.then,
        action => action,
    };
    let calls = |action: &UiAction| matches!(action, UiAction::Send(Command::Call(call)) if call.method == method);
    rows(app, PageId::FeedbackMessages)
        .into_iter()
        .find_map(|row: Row| match row.kind {
            RowKind::Button { action, .. } => Some(confirmed(action)).filter(calls),
            RowKind::Item { actions, .. } => actions
                .into_iter()
                .filter_map(|chip| chip.action.map(confirmed))
                .find(calls),
            _ => None,
        })
}

// The page reads the settings, announcements and the signed-in lists, marks the shown
// announcements seen, marks messages read, opens and deletes a report, and sends a previewed
// report that the synthetic service accepts only with its latest digest.
// 页面读取设置、公告与已登录列表，把显示的公告记为已看、标记消息已读、打开并删除报告，并发送预览过的
// 报告；合成服务只接受其最近一次摘要。
#[test]
fn mock_page_marks_lists_and_sends_a_previewed_report() {
    let _locale = crate::i18n::lock_locale("en-US");
    let (backend, mut driver) = mock::inline();
    let mut app = App::new(backend, Theme::dark(), true);
    settle(&mut app, &mut driver, "workbench", |app| {
        app.slots.loaded(Slot::Workbench)
    });

    perform(&mut app, UiAction::GoPage(PageId::FeedbackMessages));
    settle(&mut app, &mut driver, "page reads", |app| {
        [
            Slot::MessageCenterSettings,
            Slot::Messages,
            Slot::FeedbackList,
        ]
        .into_iter()
        .all(|slot| app.slots.loaded(slot))
            && app.slots.at(Slot::Announcements, "/unseen") == &json!(0)
    });
    assert!(
        app.feedback.marked_seen.contains("demo-announcement-1"),
        "viewing the page records the shown announcement as seen"
    );
    draw(&mut app);

    // Mark all read covers the newest message.
    // 全部已读覆盖最新消息。
    let mark_all = page_call(&app, "service_mark_messages_read").expect("mark all read");
    perform(&mut app, mark_all);
    settle(&mut app, &mut driver, "messages read", |app| {
        app.slots.at(Slot::MessageSummary, "/unread") == &json!(0)
    });

    // A report opens with its replies and is deleted after confirmation.
    // 报告打开后显示其回复，确认后删除。
    perform(
        &mut app,
        UiAction::Feedback(FeedbackAction::OpenDetail("demo-feedback-1".into())),
    );
    settle(&mut app, &mut driver, "detail", |app| {
        app.slots.loaded(Slot::FeedbackDetail)
    });
    draw(&mut app);
    let delete = page_call(&app, "service_delete_feedback").expect("delete inside confirm");
    perform(&mut app, delete);
    settle(&mut app, &mut driver, "deleted", |app| {
        app.feedback.detail.is_none()
            && app
                .slots
                .at(Slot::FeedbackList, "/feedback")
                .as_array()
                .is_some_and(Vec::is_empty)
    });

    // Compose, preview and send exactly the previewed request.
    // 编写、预览并发送恰为预览过的请求。
    edit(
        &mut app,
        FeedbackField::Kind,
        SettingValue::Choice("bug".into()),
    );
    edit(
        &mut app,
        FeedbackField::Title,
        SettingValue::Text("A title".into()),
    );
    edit(
        &mut app,
        FeedbackField::Body,
        SettingValue::Text("Has a secret".into()),
    );
    perform(&mut app, UiAction::Feedback(FeedbackAction::Preview));
    settle(&mut app, &mut driver, "preview", |app| {
        fresh_preview(app).is_some()
    });
    assert_eq!(
        app.slots.at(Slot::FeedbackPreview, "/redactionHits/secret"),
        &json!(1)
    );
    draw(&mut app);

    // The service refuses a digest that is not the latest preview's.
    // 服务拒绝不是最近一次预览的摘要。
    let draft_id = app.feedback.draft_id.clone().expect("issued draft id");
    perform(
        &mut app,
        UiAction::Send(Command::Call(api::submit_feedback(&draft_id, "tampered"))),
    );
    settle(&mut app, &mut driver, "refused send", |app| {
        app.slot_errors.contains_key(&Slot::FeedbackReceipt)
    });

    perform(&mut app, UiAction::Feedback(FeedbackAction::Send));
    settle(&mut app, &mut driver, "receipt", |app| {
        app.slots.loaded(Slot::FeedbackReceipt)
            && app
                .slots
                .at(Slot::FeedbackList, "/feedback")
                .as_array()
                .is_some_and(|reports| reports.len() == 1)
    });
    assert_eq!(app.slots.at(Slot::FeedbackList, "/feedback/0/kind"), "bug");
    assert!(
        app.feedback.draft_id.is_none(),
        "a sent report starts a new draft"
    );
    assert!(!app.feedback.form.ready(), "a sent report clears the form");
    draw(&mut app);
}
