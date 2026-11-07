// Mirror of `service/feedback-service.ts`: feedback drafts and their exact previews, the user's own
// reports, announcements, personal messages and the message-center settings.
// `service/feedback-service.ts` 的镜像：反馈草稿及其精确预览、用户本人的报告、公告、个人消息与
// 消息中心设置。
//
// `prepareFeedback` and `submitFeedback` answer with a document the page shows (the preview and the
// receipt), so they fill their own slots, as the judge benchmark does; neither is a page loader, so
// a reload or a reconnect never sends them again.
// `prepareFeedback` 与 `submitFeedback` 返回页面要显示的文档（预览与回执），因此与判定测速一样填入
// 各自的槽位；二者都不是页面加载器，重新加载或重新连接不会再次发送。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

/// Builds the request for `service_prepare_feedback`.
/// 构建 `service_prepare_feedback` 的请求。
/// `draft` is the complete `FeedbackDraft` document, its `draftId` included.
/// `draft` 为完整的 `FeedbackDraft` 文档，含其 `draftId`。
pub fn prepare_feedback(draft: Value) -> ServiceCall {
    ServiceCall::load(
        "prepare_feedback",
        "service_prepare_feedback",
        json!({ "draft": draft }),
        Slot::FeedbackPreview,
    )
}

/// Builds the request for `service_submit_feedback`; the digest proves which preview was seen.
/// 构建 `service_submit_feedback` 的请求；摘要证明看到的是哪一份预览。
pub fn submit_feedback(draft_id: &str, digest: &str) -> ServiceCall {
    ServiceCall::load(
        "submit_feedback",
        "service_submit_feedback",
        json!({ "draftId": draft_id, "digest": digest }),
        Slot::FeedbackReceipt,
    )
}

/// Builds the request for `service_list_feedback`; `cursor` is omitted for the newest page.
/// 构建 `service_list_feedback` 的请求；最新一页省略 `cursor`。
pub fn list_feedback(cursor: Option<&str>) -> ServiceCall {
    ServiceCall::load(
        "list_feedback",
        "service_list_feedback",
        match cursor {
            Some(cursor) if !cursor.is_empty() => json!({ "cursor": cursor }),
            _ => json!({}),
        },
        Slot::FeedbackList,
    )
}

/// Builds the request for `service_feedback_detail`.
/// 构建 `service_feedback_detail` 的请求。
pub fn read_feedback(feedback_id: &str) -> ServiceCall {
    ServiceCall::load(
        "read_feedback",
        "service_feedback_detail",
        json!({ "feedbackId": feedback_id }),
        Slot::FeedbackDetail,
    )
}

/// Builds the request for `service_link_feedback_pr`; the open detail rereads its links.
/// 构建 `service_link_feedback_pr` 的请求；打开的详情重读其链接。
pub fn link_feedback_pull_request(feedback_id: &str, url: &str) -> ServiceCall {
    ServiceCall::mutate(
        "link_feedback_pull_request",
        "service_link_feedback_pr",
        json!({ "feedbackId": feedback_id, "url": url }),
        &[Slot::FeedbackDetail],
    )
}

/// Builds the request for `service_delete_feedback`; the report is removed for good.
/// 构建 `service_delete_feedback` 的请求；报告被永久删除。
pub fn delete_feedback(feedback_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "delete_feedback",
        "service_delete_feedback",
        json!({ "feedbackId": feedback_id }),
        &[Slot::FeedbackList],
    )
}

/// Builds the request for `service_list_announcements`.
/// 构建 `service_list_announcements` 的请求。
pub fn list_announcements() -> ServiceCall {
    ServiceCall::load(
        "list_announcements",
        "service_list_announcements",
        json!({}),
        Slot::Announcements,
    )
}

/// Builds the request for `service_mark_announcements`: seen, or with `dismissed` banner closed.
/// 构建 `service_mark_announcements` 的请求：记为已看，`dismissed` 时记为横幅已关闭。
pub fn mark_announcements(announcement_ids: &[String], dismissed: bool) -> ServiceCall {
    ServiceCall::mutate(
        "mark_announcements",
        "service_mark_announcements",
        json!({ "announcementIds": announcement_ids, "dismissed": dismissed }),
        &[Slot::Announcements],
    )
}

/// Builds the request for `service_message_summary`.
/// 构建 `service_message_summary` 的请求。
pub fn load_message_summary() -> ServiceCall {
    ServiceCall::load(
        "load_message_summary",
        "service_message_summary",
        json!({}),
        Slot::MessageSummary,
    )
}

/// Builds the request for `service_list_messages`; `cursor` is omitted for the newest page.
/// 构建 `service_list_messages` 的请求；最新一页省略 `cursor`。
pub fn list_messages(cursor: Option<&str>, unread_only: bool) -> ServiceCall {
    let mut params = json!({ "unreadOnly": unread_only });
    if let Some(cursor) = cursor.filter(|cursor| !cursor.is_empty()) {
        params["cursor"] = json!(cursor);
    }
    ServiceCall::load(
        "list_messages",
        "service_list_messages",
        params,
        Slot::Messages,
    )
}

/// The slots a read mark changes: the shown messages and the unread summary.
/// 已读标记改变的槽位：显示的消息与未读摘要。
const MESSAGE_MARKS: &[Slot] = &[Slot::Messages, Slot::MessageSummary];

/// Builds the request for `service_mark_messages_read` with the given message ids.
/// 以给定消息 ID 构建 `service_mark_messages_read` 的请求。
pub fn mark_messages_read(ids: &[String]) -> ServiceCall {
    ServiceCall::mutate(
        "mark_messages_read",
        "service_mark_messages_read",
        json!({ "ids": ids }),
        MESSAGE_MARKS,
    )
}

/// Builds the request for `service_mark_messages_read` up to and including `latest_id`.
/// 构建将 `latest_id` 及之前消息标记为已读的 `service_mark_messages_read` 请求。
pub fn mark_all_messages_read(latest_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "mark_all_messages_read",
        "service_mark_messages_read",
        json!({ "allBefore": latest_id }),
        MESSAGE_MARKS,
    )
}

/// Builds the request for `service_message_center_settings`.
/// 构建 `service_message_center_settings` 的请求。
pub fn load_message_center_settings() -> ServiceCall {
    ServiceCall::load(
        "load_message_center_settings",
        "service_message_center_settings",
        json!({}),
        Slot::MessageCenterSettings,
    )
}

/// Builds the request for `service_save_message_center_settings`; like the WebUI, the
/// announcements and the message summary are read again afterwards.
/// 构建 `service_save_message_center_settings` 的请求；与 WebUI 一致，之后重新读取公告与消息摘要。
/// `settings` is the complete `MessageCenterSettings` document.
/// `settings` 为完整的 `MessageCenterSettings` 文档。
pub fn save_message_center_settings(settings: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_message_center_settings",
        "service_save_message_center_settings",
        json!({ "settings": settings }),
        &[
            Slot::MessageCenterSettings,
            Slot::Announcements,
            Slot::MessageSummary,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_api::CallKind;

    // Optional cursors are omitted for the newest page, as the WebUI spreads them.
    // 最新一页省略可选游标，与 WebUI 的展开写法一致。
    #[test]
    fn cursors_are_omitted_for_the_newest_page() {
        assert_eq!(list_feedback(None).params, json!({}));
        assert_eq!(list_feedback(Some("c2")).params, json!({ "cursor": "c2" }));
        assert_eq!(
            list_messages(None, false).params,
            json!({ "unreadOnly": false })
        );
        assert_eq!(
            list_messages(Some("c2"), true).params,
            json!({ "cursor": "c2", "unreadOnly": true })
        );
    }

    // Both read marks share one method and differ only in their selection parameter.
    // 两种已读标记共用一个方法，只在选择参数上不同。
    #[test]
    fn read_marks_select_ids_or_everything_before() {
        let some = mark_messages_read(&["m1".into()]);
        let all = mark_all_messages_read("m9");
        assert_eq!(some.method, all.method);
        assert_eq!(some.params, json!({ "ids": ["m1"] }));
        assert_eq!(all.params, json!({ "allBefore": "m9" }));
        assert_eq!(some.kind, CallKind::Mutate(MESSAGE_MARKS.into()));
    }
}
