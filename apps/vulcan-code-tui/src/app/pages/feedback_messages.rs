// Feedback and messages page: the message-center setting, announcements, personal messages, the
// user's own reports, and the feedback compose form with its exact preview. It mirrors the WebUI
// settings area, message center and feedback window on one page. Personal content needs an
// OpenVulcan account, which the terminal cannot sign in to, so a signed-out summary points to the
// desktop app or the WebUI.
// 反馈与消息页：消息中心设置、公告、个人消息、用户本人的报告，以及带精确预览的反馈编写表单。它在一个
// 页面上镜像 WebUI 设置区、消息中心与反馈窗口。个人内容需要 OpenVulcan 账户，而终端无法登录，因此未登录的
// 摘要会指向桌面应用或 WebUI。

use super::*;
use crate::app::feedback::{self, FeedbackAction, FeedbackField};
use crate::service_api::feedback as api_feedback;

/// The page's loaders; the personal lists follow a signed-in summary and keep their shown page.
/// 本页的加载器；个人列表跟随已登录摘要，并保留其显示的页。
pub(super) fn loaders(app: &App) -> Vec<ServiceCall> {
    let mut calls = vec![
        api_feedback::load_message_center_settings(),
        api_feedback::list_announcements(),
        api_feedback::load_message_summary(),
    ];
    if feedback::signed_in(app) {
        calls.push(
            app.slots
                .loader(Slot::Messages)
                .cloned()
                .unwrap_or_else(|| api_feedback::list_messages(None, false)),
        );
        calls.push(
            app.slots
                .loader(Slot::FeedbackList)
                .cloned()
                .unwrap_or_else(|| api_feedback::list_feedback(None)),
        );
    }
    if let Some(id) = &app.feedback.detail {
        calls.push(api_feedback::read_feedback(id));
    }
    calls
}

/// Unread personal messages plus announcements not yet seen, as the WebUI rail badge counts them.
/// 未读个人消息与尚未看过的公告之和，与 WebUI 活动栏徽标的计数一致。
pub(super) fn badge(app: &App) -> Option<(String, Tone)> {
    let attention = n(app.slots.value(Slot::MessageSummary), "/unread")
        + n(app.slots.value(Slot::Announcements), "/unseen");
    (attention > 0).then(|| (attention.to_string(), Tone::Accent))
}

/// Formats an RFC 3339 instant as a UTC minute, keeping the raw text when it does not parse.
/// 将 RFC 3339 时间点格式化为 UTC 分钟；无法解析时保留原文。
fn time_of(value: &Value) -> String {
    let text = value.as_str().unwrap_or_default();
    chrono::DateTime::parse_from_rfc3339(text).map_or_else(
        |_| text.to_string(),
        |time| super::settings::utc_time(time.timestamp_millis()),
    )
}

/// The label and tone of a triage state.
/// 处理状态的标签与色调。
fn status(status: &str) -> (String, Tone) {
    match status {
        "received" => (tr!("messageCenter.status.received"), Tone::Muted),
        "triaged" => (tr!("messageCenter.status.triaged"), Tone::Normal),
        "accepted" => (tr!("messageCenter.status.accepted"), Tone::Accent),
        "duplicate" => (tr!("messageCenter.status.duplicate"), Tone::Muted),
        "wont_fix" => (tr!("messageCenter.status.wont_fix"), Tone::Muted),
        "done" => (tr!("messageCenter.status.done"), Tone::Good),
        other => (other.to_string(), Tone::Muted),
    }
}

/// The label of a report kind.
/// 报告类型的标签。
fn kind_label(kind: &str) -> String {
    if kind == "bug" {
        tr!("feedback.kind.bug")
    } else {
        tr!("feedback.kind.suggestion")
    }
}

/// The label of an announcement kind.
/// 公告类型的标签。
fn announcement_kind(kind: &str) -> String {
    match kind {
        "update" => tr!("messageCenter.kind.update"),
        "event" => tr!("messageCenter.kind.event"),
        "maintenance" => tr!("messageCenter.kind.maintenance"),
        _ => tr!("messageCenter.kind.notice"),
    }
}

/// A Markdown body rendered like a reply.
/// 与回答同样渲染的 Markdown 正文。
fn markdown(content: &Value) -> Row {
    Row::new(
        RowKind::Markdown {
            content: content.as_str().unwrap_or_default().to_string(),
        },
        "",
    )
}

/// The sign-in notes for personal content: the WebUI explanation, then where to sign in.
/// 个人内容的登录说明：WebUI 的解释，其后是登录的位置。
fn sign_in(rows: &mut Vec<Row>, description: String) {
    rows.push(note(tr!("messageCenter.signInTitle"), Tone::Warn));
    rows.push(note(description, Tone::Muted));
    rows.push(note(tr!("tui.feedback.signInElsewhere"), Tone::Info));
}

/// Paging buttons of one cursor list: the next page, and back to the newest one.
/// 一个游标列表的翻页按钮：下一页，以及回到最新一页。
fn paging(rows: &mut Vec<Row>, app: &App, slot: Slot, read: fn(Option<&str>) -> ServiceCall) {
    if let Some(next) = app.slots.at(slot, "/next_cursor").as_str() {
        rows.push(button(
            tr!("messageCenter.loadMore"),
            call(read(Some(next))),
            Tone::Normal,
        ));
    }
    let paged = app
        .slots
        .loader(slot)
        .is_some_and(|loader| loader.param("/cursor").is_string());
    if paged {
        rows.push(button(
            tr!("tui.feedback.newest"),
            call(read(None)),
            Tone::Muted,
        ));
    }
}

/// Builds the feedback and messages page.
/// 构建反馈与消息页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let mut rows = Vec::new();
    settings_rows(&mut rows, app);
    announcement_rows(&mut rows, app);
    message_rows(&mut rows, app);
    feedback_rows(&mut rows, app);
    compose_rows(&mut rows, app);
    rows
}

/// The receive-announcements switch, which saves the complete settings document.
/// 接收公告开关，保存完整的设置文档。
fn settings_rows(rows: &mut Vec<Row>, app: &App) {
    rows.push(section(tr!("settings.feedbackMessages.messages")));
    rows.push(note(
        tr!("settings.feedbackMessages.messagesDescription"),
        Tone::Muted,
    ));
    let settings = app.slots.value(Slot::MessageCenterSettings);
    if settings.is_object() {
        rows.push(
            toggle(
                tr!("settings.feedbackMessages.receiveAnnouncements"),
                b(settings, "/receiveAnnouncements"),
                dto(
                    settings.clone(),
                    "/receiveAnnouncements",
                    api_feedback::save_message_center_settings,
                ),
            )
            .detail(tr!(
                "settings.feedbackMessages.receiveAnnouncementsDescription"
            )),
        );
    } else {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
    }
}

/// The announcements in the interface language, each with its action and Markdown body.
/// 界面语言下的公告，每条带其操作与 Markdown 正文。
fn announcement_rows(rows: &mut Vec<Row>, app: &App) {
    rows.push(section(tr!("messageCenter.tab.announcements")));
    let view = app.slots.value(Slot::Announcements);
    if view.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return;
    }
    if !b(view, "/enabled") {
        rows.push(note(tr!("messageCenter.announcementsOff"), Tone::Muted));
        rows.push(note(tr!("messageCenter.announcementsOffHint"), Tone::Muted));
        return;
    }
    if b(view, "/offline") {
        rows.push(note(tr!("messageCenter.offline"), Tone::Warn));
    }
    let announcements = a(view, "/announcements");
    if announcements.is_empty() {
        rows.push(note(tr!("messageCenter.noAnnouncements"), Tone::Muted));
    }
    for announcement in announcements {
        let id = s(announcement, "/id");
        let important = s(announcement, "/priority") == "important";
        let link = v(announcement, "/link");
        let actions = match s(link, "/kind").as_str() {
            "app_update" => vec![action(
                tr!("messageCenter.link.app_update"),
                UiAction::GoPage(PageId::AppUpdate),
                Tone::Normal,
            )],
            "url" if link["url"].is_string() => vec![action(
                tr!("messageCenter.link.url"),
                call(crate::service_api::project_actions::open_external_url(&s(
                    link, "/url",
                ))),
                Tone::Normal,
            )],
            "feedback" if link["feedback_id"].is_string() && feedback::signed_in(app) => {
                vec![action(
                    tr!("messageCenter.link.feedback"),
                    UiAction::Feedback(FeedbackAction::OpenDetail(s(link, "/feedback_id"))),
                    Tone::Normal,
                )]
            }
            _ => Vec::new(),
        };
        rows.push(
            item(
                &format!("announcement:{id}"),
                s(announcement, "/title"),
                time_of(v(announcement, "/startsAt")),
            )
            .badge(
                announcement_kind(&s(announcement, "/kind")),
                if important { Tone::Warn } else { Tone::Muted },
            )
            .actions(actions),
        );
        rows.push(markdown(v(announcement, "/body")));
    }
}

/// The newest message id a mark-all covers: the first message of the newest page, or the
/// summary's latest id while an older page is shown.
/// 全部标记覆盖的最新消息编号：最新一页的第一条消息；显示较早的页时取摘要的最新编号。
fn latest_message(app: &App) -> Option<String> {
    let paged = app
        .slots
        .loader(Slot::Messages)
        .is_some_and(|loader| loader.param("/cursor").is_string());
    if paged {
        app.slots
            .at(Slot::MessageSummary, "/latestId")
            .as_str()
            .map(str::to_string)
    } else {
        app.slots
            .at(Slot::Messages, "/messages/0/id")
            .as_str()
            .map(str::to_string)
    }
}

/// Personal messages of the signed-in account, newest first, with read marks.
/// 已登录账户的个人消息，按从新到旧排列，带已读标记。
fn message_rows(rows: &mut Vec<Row>, app: &App) {
    rows.push(section(tr!("messageCenter.tab.messages")));
    let summary = app.slots.value(Slot::MessageSummary);
    if summary.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return;
    }
    if !feedback::signed_in(app) {
        sign_in(rows, tr!("messageCenter.messagesSignIn"));
        return;
    }
    let page = app.slots.value(Slot::Messages);
    if page.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return;
    }
    let messages = a(page, "/messages");
    let any_unread = messages
        .iter()
        .any(|message| v(message, "/read_at").is_null());
    if any_unread && let Some(latest) = latest_message(app) {
        rows.push(button(
            tr!("messageCenter.markAllRead"),
            call(api_feedback::mark_all_messages_read(&latest)),
            Tone::Normal,
        ));
    }
    if messages.is_empty() {
        rows.push(note(tr!("messageCenter.noMessages"), Tone::Muted));
    }
    for message in messages {
        let id = s(message, "/id");
        let unread = v(message, "/read_at").is_null();
        let mut actions = Vec::new();
        if let Some(report) = v(message, "/ref/feedback_id").as_str() {
            actions.push(action(
                tr!("messageCenter.openFeedback"),
                UiAction::Feedback(FeedbackAction::OpenMessageFeedback {
                    message: unread.then(|| id.clone()),
                    feedback: report.to_string(),
                }),
                Tone::Normal,
            ));
        }
        if unread {
            actions.push(action(
                tr!("messageCenter.markRead"),
                call(api_feedback::mark_messages_read(std::slice::from_ref(&id))),
                Tone::Normal,
            ));
        }
        let mut row = item(
            &format!("message:{id}"),
            s(message, "/title"),
            time_of(v(message, "/created_at")),
        )
        .actions(actions);
        if unread {
            row = row.badge(tr!("messageCenter.unread"), Tone::Accent);
        }
        rows.push(row);
        rows.push(markdown(v(message, "/body")));
    }
    paging(rows, app, Slot::Messages, |cursor| {
        api_feedback::list_messages(cursor, false)
    });
}

/// Links a pull request to the report in `ctx`; an empty URL is refused.
/// 将拉取请求关联到 `ctx` 中的报告；空 URL 被拒绝。
fn link_pull_request(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let url = value.as_str().trim();
    let id = ctx["feedbackId"].as_str()?;
    (!url.is_empty()).then(|| api_feedback::link_feedback_pull_request(id, url))
}

/// The user's own reports with their status, and the open report's detail above them.
/// 用户本人的报告及其状态，当前打开报告的详情位于其上方。
fn feedback_rows(rows: &mut Vec<Row>, app: &App) {
    rows.push(section(tr!("messageCenter.tab.feedback")));
    if app.slots.value(Slot::MessageSummary).is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return;
    }
    if !feedback::signed_in(app) {
        sign_in(rows, tr!("messageCenter.feedbackSignIn"));
        return;
    }
    if let Some(id) = &app.feedback.detail {
        detail_rows(rows, app, id);
    }
    let page = app.slots.value(Slot::FeedbackList);
    if page.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return;
    }
    let reports = a(page, "/feedback");
    if reports.is_empty() {
        rows.push(note(tr!("messageCenter.noFeedback"), Tone::Muted));
    }
    for report in reports {
        let id = s(report, "/id");
        let mut detail = vec![
            kind_label(&s(report, "/kind")),
            time_of(v(report, "/created_at")),
        ];
        let replies = n(report, "/reply_count");
        if replies > 0 {
            detail.push(tr!("messageCenter.replyCount", count = replies));
        }
        let (label, tone) = status(&s(report, "/status"));
        rows.push(
            item(
                &format!("feedback:{id}"),
                s(report, "/title"),
                detail.join(" · "),
            )
            .badge(label, tone)
            .actions(vec![action(
                tr!("messageCenter.details"),
                UiAction::Feedback(FeedbackAction::OpenDetail(id.clone())),
                Tone::Normal,
            )]),
        );
    }
    paging(rows, app, Slot::FeedbackList, api_feedback::list_feedback);
}

/// One report with its replies and links, linking a pull request and deleting it.
/// 一份报告及其回复与链接，可关联拉取请求并删除该报告。
fn detail_rows(rows: &mut Vec<Row>, app: &App, id: &str) {
    let detail = app.slots.value(Slot::FeedbackDetail);
    let back = action(
        tr!("messageCenter.back"),
        UiAction::Feedback(FeedbackAction::CloseDetail),
        Tone::Muted,
    );
    if detail.is_null() {
        rows.push(item("feedback-detail", tr!("tui.page.loading"), "").actions(vec![back]));
        return;
    }
    let (label, tone) = status(&s(detail, "/status"));
    let link = action(
        tr!("messageCenter.linkPullRequest"),
        super::super::action::prompt(
            tr!("messageCenter.pullRequestUrl"),
            String::from("https://github.com/OpenVulcan/vulcan-code/pull/…"),
            String::new(),
            PromptThen::Edit(Edit::Call(Setter::Build {
                ctx: json!({ "feedbackId": id }),
                build: link_pull_request,
            })),
        ),
        Tone::Accent,
    );
    let delete = action(
        tr!("messageCenter.deleteFeedback"),
        confirm(
            tr!("messageCenter.deleteTitle"),
            tr!("messageCenter.deleteDescription"),
            tr!("messageCenter.deleteFeedback"),
            call(api_feedback::delete_feedback(id)),
        ),
        Tone::Bad,
    );
    rows.push(
        item(
            "feedback-detail",
            s(detail, "/title"),
            format!(
                "{} · {}",
                kind_label(&s(detail, "/kind")),
                time_of(v(detail, "/created_at"))
            ),
        )
        .badge(label, tone)
        .actions(vec![back, link, delete]),
    );
    rows.push(note(s(detail, "/body"), Tone::Normal));
    if b(detail, "/attachments_purged") {
        rows.push(note(tr!("messageCenter.attachmentsPurged"), Tone::Muted));
    }
    let replies = a(detail, "/replies");
    rows.push(info(
        tr!("messageCenter.replies"),
        replies.len().to_string(),
        Tone::Normal,
    ));
    if replies.is_empty() {
        rows.push(note(tr!("messageCenter.noReplies"), Tone::Muted));
    }
    for reply in replies {
        rows.push(info(
            time_of(v(reply, "/created_at")),
            String::new(),
            Tone::Muted,
        ));
        rows.push(markdown(v(reply, "/body")));
    }
    let links = a(detail, "/links");
    rows.push(info(
        tr!("messageCenter.pullRequests"),
        links.len().to_string(),
        Tone::Normal,
    ));
    for (index, link) in links.iter().enumerate() {
        let url = s(link, "/url");
        rows.push(
            item(
                &format!("feedback-link:{index}"),
                url.clone(),
                time_of(v(link, "/created_at")),
            )
            .actions(vec![action(
                tr!("messageCenter.link.url"),
                call(crate::service_api::project_actions::open_external_url(&url)),
                Tone::Normal,
            )]),
        );
    }
}

/// The edit target of one compose field.
/// 一个编写字段的编辑目标。
fn field_edit(field: FeedbackField) -> Edit {
    Edit::Local(LocalSetting::Feedback(field))
}

/// The compose form, its Preview and Send actions, the receipt and the exact preview.
/// 编写表单、其预览与发送操作、回执以及精确预览。
fn compose_rows(rows: &mut Vec<Row>, app: &App) {
    let state = &app.feedback;
    let form = &state.form;
    rows.push(section(tr!("settings.feedbackMessages.feedback")));
    rows.push(note(tr!("feedback.description"), Tone::Muted));
    rows.push(note(tr!("feedback.privacy"), Tone::Muted));
    let receipt = app.slots.value(Slot::FeedbackReceipt);
    if receipt.is_object() {
        rows.push(note(tr!("feedback.sentTitle"), Tone::Good));
        rows.push(note(
            tr!("feedback.sentReceipt", id = s(receipt, "/feedbackId")),
            Tone::Good,
        ));
        rows.push(note(tr!("feedback.sentDescription"), Tone::Muted));
    }
    rows.push(choice(
        tr!("feedback.kind"),
        vec![
            ("bug".into(), tr!("feedback.kind.bug")),
            ("suggestion".into(), tr!("feedback.kind.suggestion")),
        ],
        form.kind.clone(),
        field_edit(FeedbackField::Kind),
    ));
    rows.push(
        text(
            tr!("feedback.titleLabel"),
            form.title.clone(),
            field_edit(FeedbackField::Title),
        )
        .detail(tr!("feedback.titlePlaceholder")),
    );
    rows.push(
        text(
            tr!("feedback.bodyLabel"),
            form.body.clone(),
            field_edit(FeedbackField::Body),
        )
        .detail(if form.kind == "bug" {
            tr!("feedback.bodyPlaceholder.bug")
        } else {
            tr!("feedback.bodyPlaceholder.suggestion")
        }),
    );
    rows.push(toggle(
        tr!("feedback.includeLog"),
        form.include_log,
        field_edit(FeedbackField::IncludeLog),
    ));
    rows.push(toggle(
        tr!("feedback.includeClientInfo"),
        form.include_client_info,
        field_edit(FeedbackField::IncludeClientInfo),
    ));
    rows.push(toggle(
        tr!("feedback.allowContact"),
        form.allow_contact,
        field_edit(FeedbackField::AllowContact),
    ));
    let sending = state.sending.is_some();
    rows.push(
        item("feedback-compose", tr!("feedback.menu"), "").actions(vec![
            RowAction {
                label: tr!("feedback.preview"),
                action: (form.ready() && !sending)
                    .then_some(UiAction::Feedback(FeedbackAction::Preview)),
                tone: Tone::Normal,
                switch: None,
            },
            RowAction {
                label: tr!("feedback.send"),
                action: feedback::can_send(app).then_some(UiAction::Feedback(FeedbackAction::Send)),
                tone: Tone::Accent,
                switch: None,
            },
        ]),
    );
    for slot in [Slot::FeedbackPreview, Slot::FeedbackReceipt] {
        if let Some(error) = app.slot_errors.get(&slot) {
            rows.push(note(error.clone(), Tone::Bad));
        }
    }
    if sending {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
    }
    if let Some(preview) = feedback::fresh_preview(app) {
        preview_rows(rows, preview, form.include_log);
    }
}

/// The exact request a send would transmit, its size and the replacements made on this device.
/// 发送时将传输的精确请求、其大小以及在本设备上的替换。
fn preview_rows(rows: &mut Vec<Row>, preview: &Value, include_log: bool) {
    rows.push(section(tr!("feedback.previewTitle")));
    rows.push(note(tr!("feedback.previewDescription"), Tone::Muted));
    if !b(preview, "/signedIn") {
        rows.push(note(tr!("feedback.signInTitle"), Tone::Warn));
        rows.push(note(tr!("feedback.signInDescription"), Tone::Muted));
        rows.push(note(tr!("tui.feedback.signInElsewhere"), Tone::Info));
    }
    rows.push(note(
        tr!(
            "feedback.previewSize",
            size = super::permission_judge::format_bytes(n(preview, "/byteLength") as f64)
        ),
        Tone::Normal,
    ));
    if include_log {
        rows.push(note(
            tr!(
                "feedback.previewLogLines",
                count = n(preview, "/logLineCount")
            ),
            Tone::Normal,
        ));
    }
    let hits: Vec<String> = v(preview, "/redactionHits")
        .as_object()
        .map(|hits| {
            hits.iter()
                .filter_map(|(category, count)| {
                    let count = count.as_u64().filter(|count| *count > 0)?;
                    Some(format!("{category} ×{count}"))
                })
                .collect()
        })
        .unwrap_or_default();
    rows.push(if hits.is_empty() {
        note(tr!("feedback.previewNothingRedacted"), Tone::Normal)
    } else {
        note(
            format!("{} {}", tr!("feedback.previewRedacted"), hits.join(", ")),
            Tone::Normal,
        )
    });
    rows.push(Row::new(
        RowKind::Document {
            content: s(preview, "/payload"),
        },
        tr!("feedback.previewPayload"),
    ));
}
