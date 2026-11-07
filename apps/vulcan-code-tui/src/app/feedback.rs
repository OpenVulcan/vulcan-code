// Feedback compose state and the message-center reactions of the feedback page.
// 反馈页的反馈编写状态与消息中心响应。
//
// The compose form mirrors the WebUI feedback window: nothing leaves the device before the user has
// seen the exact filtered request (`prepareFeedback`) and pressed Send, Send transmits exactly that
// preview's draft id and digest, and any edit drops the preview. The draft id survives edits and
// previews, so every retry reuses one idempotency key; a sent report starts a new draft.
// 编写表单镜像 WebUI 反馈窗口：用户看到精确的过滤后请求（`prepareFeedback`）并按下发送之前，不会有任何
// 内容离开设备；发送只传输该预览的草稿编号与摘要；任何编辑都会丢弃预览。草稿编号在编辑与预览间保留，
// 使每次重试复用同一幂等键；报告发送后开始新草稿。

use std::collections::HashSet;

use serde_json::{Value, json};

use super::App;
use super::action::{UiAction, perform};
use super::pages::PageId;
use crate::backend::Command;
use crate::model::{NoticeSeverity, SettingValue};
use crate::service_api::{Slot, feedback as api};
use crate::tr;

/// One edited field of the compose form.
/// 编写表单中被编辑的一个字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackField {
    Kind,
    Title,
    Body,
    IncludeLog,
    IncludeClientInfo,
    AllowContact,
}

/// Feedback page actions that need the compose state or the open report.
/// 需要编写状态或当前打开报告的反馈页动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedbackAction {
    /// Prepares the current draft and shows the exact request.
    /// 准备当前草稿并显示精确请求。
    Preview,
    /// Sends the shown preview; refused unless it still matches the form.
    /// 发送显示中的预览；除非它仍与表单一致，否则拒绝。
    Send,
    /// Opens one of the user's reports with its replies and links.
    /// 打开用户的一份报告及其回复与链接。
    OpenDetail(String),
    /// Opens the report a message refers to, first marking that message read while it is unread.
    /// 打开消息所引用的报告；消息未读时先将其标记为已读。
    OpenMessageFeedback {
        /// The unread message to mark read, or `None` when it is already read.
        /// 要标记为已读的未读消息；已读时为 `None`。
        message: Option<String>,
        feedback: String,
    },
    /// Closes the open report.
    /// 关闭当前打开的报告。
    CloseDetail,
}

/// What the user writes, as the WebUI `FeedbackForm`; logs and client facts are attached by
/// default, contact is not.
/// 用户编写的内容，对应 WebUI `FeedbackForm`；默认附带日志与客户端信息，不默认允许联系。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackForm {
    /// `bug` or `suggestion`; a manual draft starts as a suggestion.
    /// `bug` 或 `suggestion`；手动草稿以建议开始。
    pub kind: String,
    pub title: String,
    pub body: String,
    pub include_log: bool,
    pub include_client_info: bool,
    pub allow_contact: bool,
}

impl Default for FeedbackForm {
    fn default() -> Self {
        FeedbackForm {
            kind: "suggestion".into(),
            title: String::new(),
            body: String::new(),
            include_log: true,
            include_client_info: true,
            allow_contact: false,
        }
    }
}

impl FeedbackForm {
    /// Whether the draft can be previewed: a title and a body are both required.
    /// 草稿是否可预览：标题与正文均为必填。
    pub fn ready(&self) -> bool {
        !self.title.trim().is_empty() && !self.body.trim().is_empty()
    }
}

/// Feedback page state kept only in this process.
/// 仅保存在本进程中的反馈页状态。
#[derive(Debug, Clone, Default)]
pub struct FeedbackState {
    pub form: FeedbackForm,
    /// Draft id the service issued for this draft; reused by every later preview.
    /// 服务为本草稿签发的草稿编号；之后的每次预览都复用。
    pub draft_id: Option<String>,
    /// Digest of the preview being sent; Send is unavailable until its receipt or failure.
    /// 正在发送的预览摘要；收到回执或失败之前不能再次发送。
    pub sending: Option<String>,
    /// The open report.
    /// 当前打开的报告。
    pub detail: Option<String>,
    /// Announcements already recorded as seen in this process, so a read that still reports them
    /// unseen never marks them again.
    /// 本进程已记为已看的公告；读取仍报告其未看时不再重复标记。
    pub marked_seen: HashSet<String>,
}

/// The service draft built from the form, as the WebUI `draftOf` builds it for a manual entry.
/// 由表单构建的服务端草稿，与 WebUI `draftOf` 为手动入口构建的一致。
pub fn draft(state: &FeedbackState) -> Value {
    let form = &state.form;
    json!({
        "draftId": state.draft_id,
        "kind": form.kind,
        "source": "manual",
        "title": form.title,
        "body": form.body,
        "allowContact": form.allow_contact,
        "includeClientInfo": form.include_client_info,
        "includeLog": form.include_log,
        "error": null,
    })
}

/// The draft without its id: the content a preview must match to stay valid.
/// 去掉编号的草稿：预览保持有效时必须匹配的内容。
fn content(draft: &Value) -> Value {
    let mut content = draft.clone();
    if let Some(fields) = content.as_object_mut() {
        fields.remove("draftId");
    }
    content
}

/// The shown preview while it was prepared from exactly the current form, else `None`.
/// 显示中的预览——仅当它正是由当前表单准备时返回，否则为 `None`。
pub fn fresh_preview(app: &App) -> Option<&Value> {
    let preview = app.slots.value(Slot::FeedbackPreview);
    let loader = app.slots.loader(Slot::FeedbackPreview)?;
    (preview.is_object() && content(loader.param("/draft")) == content(&draft(&app.feedback)))
        .then_some(preview)
}

/// Whether Send is offered: a fresh preview for a signed-in account, not already being sent.
/// 是否提供发送：已登录账户的新鲜预览，且尚未在发送中。
pub fn can_send(app: &App) -> bool {
    app.feedback.sending.is_none()
        && fresh_preview(app).is_some_and(|preview| preview["signedIn"] == json!(true))
}

/// Applies one edit to the form; every edit drops the preview and the last receipt.
/// 将一项编辑应用到表单；每次编辑都丢弃预览与上一份回执。
pub fn edit(app: &mut App, field: FeedbackField, value: &SettingValue) {
    let form = &mut app.feedback.form;
    match field {
        FeedbackField::Kind => form.kind = value.as_str().to_string(),
        FeedbackField::Title => form.title = value.as_str().to_string(),
        FeedbackField::Body => form.body = value.as_str().to_string(),
        FeedbackField::IncludeLog => form.include_log = value.as_bool(),
        FeedbackField::IncludeClientInfo => form.include_client_info = value.as_bool(),
        FeedbackField::AllowContact => form.allow_contact = value.as_bool(),
    }
    for slot in [Slot::FeedbackPreview, Slot::FeedbackReceipt] {
        app.slots.clear(slot);
        app.slot_errors.remove(&slot);
    }
}

/// Runs one feedback page action.
/// 执行一个反馈页动作。
pub fn perform_action(app: &mut App, action: FeedbackAction) {
    match action {
        FeedbackAction::Preview => {
            if !app.feedback.form.ready() {
                app.toast(NoticeSeverity::Info, tr!("tui.form.invalid"));
                return;
            }
            app.slots.clear(Slot::FeedbackReceipt);
            app.slot_errors.remove(&Slot::FeedbackReceipt);
            let call = api::prepare_feedback(draft(&app.feedback));
            perform(app, UiAction::Send(Command::Call(call)));
        }
        FeedbackAction::Send => {
            // Only the exact preview the user is looking at may be sent.
            // 只能发送用户正在查看的那份精确预览。
            let target = can_send(app)
                .then(|| fresh_preview(app))
                .flatten()
                .and_then(|preview| {
                    Some((
                        preview["draftId"].as_str()?.to_string(),
                        preview["digest"].as_str()?.to_string(),
                    ))
                });
            let Some((draft_id, digest)) = target else {
                app.toast(NoticeSeverity::Info, tr!("tui.form.invalid"));
                return;
            };
            app.feedback.sending = Some(digest.clone());
            app.slot_errors.remove(&Slot::FeedbackReceipt);
            let call = api::submit_feedback(&draft_id, &digest);
            perform(app, UiAction::Send(Command::Call(call)));
        }
        FeedbackAction::OpenDetail(id) => {
            app.feedback.detail = Some(id.clone());
            app.slots.clear(Slot::FeedbackDetail);
            app.slot_errors.remove(&Slot::FeedbackDetail);
            perform(app, UiAction::Send(Command::Call(api::read_feedback(&id))));
        }
        FeedbackAction::OpenMessageFeedback { message, feedback } => {
            if let Some(message) = message {
                let call = api::mark_messages_read(&[message]);
                perform(app, UiAction::Send(Command::Call(call)));
            }
            perform_action(app, FeedbackAction::OpenDetail(feedback));
        }
        FeedbackAction::CloseDetail => close_detail(app),
    }
}

/// Closes the open report and drops its detail.
/// 关闭当前打开的报告并丢弃其详情。
fn close_detail(app: &mut App) {
    app.feedback.detail = None;
    app.slots.clear(Slot::FeedbackDetail);
    app.slot_errors.remove(&Slot::FeedbackDetail);
}

/// Whether the feedback page is shown, so its automatic reads and marks may run.
/// 反馈页是否正在显示，从而可执行其自动读取与标记。
fn page_shown(app: &App) -> bool {
    app.current_page() == Some(PageId::FeedbackMessages)
}

/// Whether the last summary reports a signed-in account.
/// 最近的摘要是否报告已登录账户。
pub fn signed_in(app: &App) -> bool {
    app.slots.at(Slot::MessageSummary, "/signedIn") == &json!(true)
}

/// Reacts to a slot result: records the issued draft id, finishes a send, marks the shown
/// announcements seen and follows the sign-in state with the personal lists.
/// 响应槽位结果：记录签发的草稿编号、完成发送、把显示的公告记为已看，并按登录状态读取个人列表。
pub fn slot_arrived(app: &mut App, slot: Slot) {
    match slot {
        Slot::FeedbackPreview => {
            if let Some(id) = app.slots.at(slot, "/draftId").as_str() {
                app.feedback.draft_id = Some(id.to_string());
            }
        }
        Slot::FeedbackReceipt => {
            // The sent report closes this draft; the next one starts empty with a new id.
            // 已发送的报告结束本草稿；下一份从空白与新编号开始。
            app.feedback.sending = None;
            app.feedback.form = FeedbackForm::default();
            app.feedback.draft_id = None;
            app.slots.clear(Slot::FeedbackPreview);
            if signed_in(app) {
                perform(app, UiAction::Send(Command::Call(api::list_feedback(None))));
            }
            perform(
                app,
                UiAction::Send(Command::Call(api::load_message_summary())),
            );
        }
        Slot::Announcements => mark_shown_seen(app),
        Slot::MessageSummary if page_shown(app) => {
            if signed_in(app) {
                for (list, call) in [
                    (Slot::Messages, api::list_messages(None, false)),
                    (Slot::FeedbackList, api::list_feedback(None)),
                ] {
                    if !app.slots.loaded(list) {
                        perform(app, UiAction::Send(Command::Call(call)));
                    }
                }
            } else {
                // Personal content belongs to an account; a signed-out summary drops it.
                // 个人内容属于账户；未登录的摘要丢弃这些内容。
                app.slots.clear(Slot::Messages);
                app.slots.clear(Slot::FeedbackList);
                close_detail(app);
            }
        }
        _ => {}
    }
}

/// Records the shown unseen announcements as seen, as opening the WebUI announcements tab does.
/// 把显示中的未看公告记为已看，与打开 WebUI 公告页一致。
fn mark_shown_seen(app: &mut App) {
    let view = app.slots.value(Slot::Announcements);
    if !page_shown(app) || view["enabled"] != json!(true) || app.call_pending {
        return;
    }
    let ids: Vec<String> = view["announcements"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|announcement| announcement["seen"] != json!(true))
        .filter_map(|announcement| announcement["id"].as_str())
        .filter(|id| !app.feedback.marked_seen.contains(*id))
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return;
    }
    app.feedback.marked_seen.extend(ids.iter().cloned());
    perform(
        app,
        UiAction::Send(Command::Call(api::mark_announcements(&ids, false))),
    );
}

/// Reacts to a failed slot read: a failed send can be sent again.
/// 响应槽位读取失败：发送失败后可再次发送。
pub fn slot_failed(app: &mut App, slot: Slot) {
    if slot == Slot::FeedbackReceipt {
        app.feedback.sending = None;
    }
}

/// Reacts to a completed write: a deleted report closes its detail.
/// 响应已完成的写入：已删除的报告关闭其详情。
pub fn completed(app: &mut App, function: &str) {
    if function == "delete_feedback" {
        close_detail(app);
    }
}

/// Whether a detail result belongs to the report still open.
/// 详情结果是否属于仍然打开的报告。
pub fn detail_is_current(app: &App, call: &crate::service_api::ServiceCall) -> bool {
    call.param("/feedbackId").as_str() == app.feedback.detail.as_deref()
}
