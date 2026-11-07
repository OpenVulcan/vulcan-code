// Synthetic feedback and message center: drafts with exact previews, the user's reports,
// announcements, personal messages and the message-center settings.
// 合成反馈与消息中心：带精确预览的草稿、用户的报告、公告、个人消息与消息中心设置。
//
// Sending follows the real contract: only a prepared draft can be sent, and only with the digest of
// its latest preview, so a client that sends anything but the exact preview it showed is refused.
// 发送遵循真实协议：只有已准备的草稿才能发送，且必须带其最近一次预览的摘要，因此客户端发送的若不是它所
// 显示的精确预览就会被拒绝。

use std::hash::{Hash, Hasher};

use super::{FakeService, required, string};
use serde_json::{Value, json};

/// Synthetic feedback authority, a repository-owned fixture.
/// 合成反馈权威数据，仓库持有的夹具。
pub(super) fn fixture() -> Value {
    serde_json::from_str(include_str!("feedback-fixtures.json"))
        .expect("valid demo feedback fixture")
}

/// The fixed instant the demo stamps on new records, so runs stay deterministic.
/// 演示为新记录标记的固定时间点，使运行保持确定。
const DEMO_NOW: &str = "2026-11-07T00:00:00Z";

/// Fields a `FeedbackDraft` may carry; the service rejects any other.
/// `FeedbackDraft` 可携带的字段；服务拒绝其他字段。
const DRAFT_FIELDS: [&str; 9] = [
    "draftId",
    "kind",
    "source",
    "title",
    "body",
    "allowContact",
    "includeClientInfo",
    "includeLog",
    "error",
];

/// Reads a required boolean request value.
/// 读取必需的布尔请求值。
fn flag(value: &Value, pointer: &str) -> Result<bool, String> {
    required(value, pointer)?
        .as_bool()
        .ok_or_else(|| format!("{pointer} must be a boolean"))
}

/// A stable digest of the exact payload text.
/// 精确载荷文本的稳定摘要。
fn digest(payload: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    payload.hash(&mut hasher);
    format!("demo-{:016x}", hasher.finish())
}

impl FakeService {
    /// Whether a method belongs to the feedback and message-center contract modeled here.
    /// 某方法是否属于此处建模的反馈与消息中心协议。
    pub(super) fn is_feedback_method(method: &str) -> bool {
        matches!(
            method,
            "service_prepare_feedback"
                | "service_submit_feedback"
                | "service_list_feedback"
                | "service_feedback_detail"
                | "service_link_feedback_pr"
                | "service_delete_feedback"
                | "service_list_announcements"
                | "service_mark_announcements"
                | "service_message_summary"
                | "service_list_messages"
                | "service_mark_messages_read"
                | "service_message_center_settings"
                | "service_save_message_center_settings"
        )
    }

    /// Answers one feedback or message-center call against the synthetic authority.
    /// 针对合成权威数据回答一次反馈或消息中心调用。
    pub(super) fn feedback(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "service_prepare_feedback" => self.prepare_feedback(required(params, "/draft")?),
            "service_submit_feedback" => {
                self.submit_feedback(string(params, "/draftId")?, string(params, "/digest")?)
            }
            "service_list_feedback" => {
                self.require_sign_in()?;
                let feedback: Vec<Value> = self.inbox["feedback"]
                    .as_array()
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .map(|report| {
                        json!({
                            "id": report["id"], "kind": report["kind"], "status": report["status"],
                            "title": report["title"], "created_at": report["created_at"],
                            "updated_at": report["updated_at"], "reply_count": report["reply_count"],
                        })
                    })
                    .collect();
                Ok(json!({ "feedback": feedback, "next_cursor": null }))
            }
            "service_feedback_detail" => {
                self.require_sign_in()?;
                Ok(self.report(string(params, "/feedbackId")?)?.clone())
            }
            "service_link_feedback_pr" => {
                self.require_sign_in()?;
                let url = string(params, "/url")?.trim().to_string();
                if !url.starts_with("https://github.com/OpenVulcan/vulcan-code/pull/") {
                    return Err("Only Vulcan Code pull requests can be linked.".into());
                }
                let report = self.report(string(params, "/feedbackId")?)?;
                let links = report["links"].as_array_mut().ok_or("Feedback links")?;
                links.push(json!({ "kind": "github_pr", "url": url, "created_at": DEMO_NOW }));
                Ok(json!({ "links": links }))
            }
            "service_delete_feedback" => {
                self.require_sign_in()?;
                let id = string(params, "/feedbackId")?;
                let reports = self.inbox["feedback"]
                    .as_array_mut()
                    .ok_or("Feedback fixture")?;
                let before = reports.len();
                reports.retain(|report| report["id"] != id);
                if reports.len() == before {
                    return Err(format!("Unknown feedback: {id}"));
                }
                Ok(Value::Null)
            }
            "service_list_announcements" => Ok(self.announcements()),
            "service_mark_announcements" => {
                let ids = required(params, "/announcementIds")?
                    .as_array()
                    .filter(|ids| !ids.is_empty())
                    .ok_or("announcementIds must list at least one announcement")?
                    .clone();
                let mark = if flag(params, "/dismissed")? {
                    "dismissed"
                } else {
                    "seen"
                };
                for announcement in self.inbox["announcements"]
                    .as_array_mut()
                    .ok_or("Announcements fixture")?
                {
                    if ids.contains(&announcement["id"]) {
                        announcement[mark] = json!(true);
                    }
                }
                Ok(Value::Null)
            }
            "service_message_summary" => Ok(self.message_summary()),
            "service_list_messages" => {
                self.require_sign_in()?;
                let unread_only = flag(params, "/unreadOnly")?;
                let messages: Vec<Value> = self
                    .messages()
                    .iter()
                    .filter(|message| !unread_only || message["read_at"].is_null())
                    .cloned()
                    .collect();
                Ok(json!({ "messages": messages, "next_cursor": null }))
            }
            "service_mark_messages_read" => self.mark_messages_read(params),
            "service_message_center_settings" => Ok(self.inbox["settings"].clone()),
            "service_save_message_center_settings" => {
                let settings = required(params, "/settings")?;
                let fields = settings.as_object().ok_or("settings must be an object")?;
                if fields.len() != 1 {
                    return Err("settings must hold exactly receiveAnnouncements".into());
                }
                let receive = flag(settings, "/receiveAnnouncements")?;
                self.inbox["settings"] = json!({ "receiveAnnouncements": receive });
                Ok(self.inbox["settings"].clone())
            }
            other => Err(format!("No demo feedback method {other}")),
        }
    }

    /// Refuses personal content while the demo account is signed out.
    /// 演示账户未登录时拒绝个人内容。
    fn require_sign_in(&self) -> Result<(), String> {
        if self.inbox["signedIn"] == json!(true) {
            Ok(())
        } else {
            Err("Sign in to your OpenVulcan account first.".into())
        }
    }

    /// The stored report with the given id.
    /// 具有给定编号的已存储报告。
    fn report(&mut self, id: &str) -> Result<&mut Value, String> {
        self.inbox["feedback"]
            .as_array_mut()
            .ok_or("Feedback fixture")?
            .iter_mut()
            .find(|report| report["id"] == id)
            .ok_or_else(|| format!("Unknown feedback: {id}"))
    }

    /// The stored personal messages, newest first.
    /// 已存储的个人消息，按从新到旧排列。
    fn messages(&self) -> &[Value] {
        self.inbox["messages"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
    }

    /// Filters a draft and stores its exact payload digest under its draft id.
    /// 过滤草稿，并以其草稿编号保存精确载荷摘要。
    fn prepare_feedback(&mut self, draft: &Value) -> Result<Value, String> {
        let fields = draft.as_object().ok_or("draft must be an object")?;
        if let Some(unknown) = fields
            .keys()
            .find(|key| !DRAFT_FIELDS.contains(&key.as_str()))
        {
            return Err(format!("Unknown draft field: {unknown}"));
        }
        let kind = string(draft, "/kind")?;
        if !matches!(kind, "bug" | "suggestion") {
            return Err(format!("Unknown feedback kind: {kind}"));
        }
        let source = string(draft, "/source")?;
        let title = string(draft, "/title")?.trim().to_string();
        let body = string(draft, "/body")?.trim().to_string();
        if title.is_empty() || body.is_empty() {
            return Err("A title and details are required.".into());
        }
        let include_log = flag(draft, "/includeLog")?;
        let include_client_info = flag(draft, "/includeClientInfo")?;
        let allow_contact = flag(draft, "/allowContact")?;
        let draft_id = match draft.get("draftId").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => id.trim().to_string(),
            _ => crate::service_api::fresh_command_id(),
        };
        // The demo filter replaces the word "secret" the way the real one replaces credentials.
        // 演示过滤器替换单词 "secret"，模拟真实过滤器替换凭据的方式。
        let hits = title.matches("secret").count() + body.matches("secret").count();
        let filter = |text: &str| text.replace("secret", "[secret]");
        let log: Vec<&str> = if include_log {
            vec!["demo log line 1", "demo log line 2", "demo log line 3"]
        } else {
            Vec::new()
        };
        let request = json!({
            "kind": kind,
            "source": source,
            "title": filter(&title),
            "body": filter(&body),
            "allowContact": allow_contact,
            "client": include_client_info.then(|| json!({ "version": "demo", "os": "demo-os" })),
            "log": include_log.then(|| json!(log)),
        });
        let payload = serde_json::to_string_pretty(&request).map_err(|error| error.to_string())?;
        let digest = digest(&payload);
        self.inbox["drafts"][&draft_id] = json!({ "digest": digest, "request": request });
        let mut redaction_hits = serde_json::Map::new();
        if hits > 0 {
            redaction_hits.insert("secret".into(), json!(hits));
        }
        Ok(json!({
            "draftId": draft_id,
            "digest": digest,
            "byteLength": payload.len(),
            "payload": payload,
            "redactionHits": redaction_hits,
            "logLineCount": log.len(),
            "signedIn": self.inbox["signedIn"],
        }))
    }

    /// Sends a prepared draft whose latest preview has exactly this digest.
    /// 发送最近一次预览摘要正是此值的已准备草稿。
    fn submit_feedback(&mut self, draft_id: &str, digest: &str) -> Result<Value, String> {
        self.require_sign_in()?;
        let stored = self.inbox["drafts"][draft_id].clone();
        if stored.is_null() {
            return Err("This draft was not prepared; preview it again.".into());
        }
        if stored["digest"] != digest {
            return Err("The preview changed; preview it again before sending.".into());
        }
        let id = self.id("feedback");
        let request = &stored["request"];
        let report = json!({
            "id": id, "kind": request["kind"], "status": "received", "title": request["title"],
            "created_at": DEMO_NOW, "updated_at": DEMO_NOW, "reply_count": 0,
            "body": request["body"], "allow_contact": request["allowContact"],
            "attachments_purged": false, "replies": [], "links": [],
        });
        self.inbox["feedback"]
            .as_array_mut()
            .ok_or("Feedback fixture")?
            .insert(0, report);
        if let Some(drafts) = self.inbox["drafts"].as_object_mut() {
            drafts.remove(draft_id);
        }
        Ok(json!({ "feedbackId": id, "status": "received", "createdAt": DEMO_NOW }))
    }

    /// The announcements view; switched-off announcements come back empty and disabled.
    /// 公告视图；关闭接收时返回空的禁用视图。
    fn announcements(&self) -> Value {
        if self.inbox["settings"]["receiveAnnouncements"] != json!(true) {
            return json!({ "enabled": false, "region": null, "offline": false,
                "announcements": [], "unseen": 0 });
        }
        let announcements = &self.inbox["announcements"];
        let unseen = announcements.as_array().map_or(0, |list| {
            list.iter()
                .filter(|item| item["seen"] != json!(true))
                .count()
        });
        json!({ "enabled": true, "region": self.inbox["region"], "offline": false,
            "announcements": announcements, "unseen": unseen })
    }

    /// The unread summary; a signed-out account has no personal messages.
    /// 未读摘要；未登录账户没有个人消息。
    fn message_summary(&self) -> Value {
        if self.require_sign_in().is_err() {
            return json!({ "signedIn": false, "available": false, "unread": 0, "latestId": null });
        }
        let messages = self.messages();
        json!({
            "signedIn": true,
            "available": true,
            "unread": messages.iter().filter(|message| message["read_at"].is_null()).count(),
            "latestId": messages.first().map(|message| message["id"].clone()),
        })
    }

    /// Marks the given messages, or every message up to and including `allBefore`, read.
    /// 将给定消息，或 `allBefore` 及之前的全部消息标记为已读。
    fn mark_messages_read(&mut self, params: &Value) -> Result<Value, String> {
        self.require_sign_in()?;
        let ids = params.get("ids").and_then(Value::as_array).cloned();
        let all_before = params
            .get("allBefore")
            .and_then(Value::as_str)
            .map(str::to_string);
        let messages = self.inbox["messages"]
            .as_array_mut()
            .ok_or("Messages fixture")?;
        let selected: Vec<usize> = match (ids, all_before) {
            (Some(ids), None) => (0..messages.len())
                .filter(|index| ids.contains(&messages[*index]["id"]))
                .collect(),
            (None, Some(latest)) => {
                let start = messages
                    .iter()
                    .position(|message| message["id"] == latest.as_str())
                    .ok_or_else(|| format!("Unknown message: {latest}"))?;
                (start..messages.len()).collect()
            }
            _ => return Err("Pass either ids or allBefore.".into()),
        };
        let mut updated = 0;
        for index in selected {
            if messages[index]["read_at"].is_null() {
                messages[index]["read_at"] = json!(DEMO_NOW);
                updated += 1;
            }
        }
        let unread = messages
            .iter()
            .filter(|message| message["read_at"].is_null())
            .count();
        Ok(json!({ "updated": updated, "unread": unread }))
    }
}
