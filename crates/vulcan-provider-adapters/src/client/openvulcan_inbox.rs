//! Native OpenVulcan feedback, announcement and personal message routes.
//! 原生 OpenVulcan 反馈、公告与个人消息路由。
//!
//! Contract authority: openvulcan-web `contracts/openapi-vulcancode.yaml` (tags Feedback and Messages,
//! sourced from `httpapi/control/vulcancode_inbox.go`). Re-read it when these routes change.
//! 契约权威：openvulcan-web `contracts/openapi-vulcancode.yaml`（Feedback 与 Messages 标签，来源于
//! `httpapi/control/vulcancode_inbox.go`）。这些路由变化时须回读该文件。

use crate::CredentialExecutionSecret;
use crate::OpenVulcanRegion;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Time allowed for one inbox request, connection and answer included.
/// 单次收件箱请求允许的时间，包含连接与回答。
const INBOX_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Largest feedback or message answer read; a page holds at most 50 entries of 8000 characters.
/// 读取的最大反馈或消息回答；一页最多 50 条、每条 8000 字符。
const INBOX_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Largest announcement document read; the deployment serves at most 50 localized announcements.
/// 读取的最大公告文档；部署最多下发 50 条多语言公告。
const ANNOUNCEMENTS_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Longest opaque cursor the routes accept.
/// 路由接受的最长不透明游标。
const INBOX_CURSOR_MAX_CHARS: usize = 128;

/// Most message ids one read mark may name.
/// 单次已读标记可指定的最多消息编号数。
pub const OPENVULCAN_MARK_READ_MAX_IDS: usize = 100;

/// Only pull requests of this repository may be linked to feedback.
/// 只有该仓库的拉取请求可以关联到反馈。
pub const OPENVULCAN_FEEDBACK_PR_URL_PREFIX: &str =
    "https://github.com/OpenVulcan/vulcan-code/pull/";

/// Longest pull request address the link route accepts.
/// 关联路由接受的最长拉取请求地址。
const FEEDBACK_PR_URL_MAX_CHARS: usize = 200;

/// Failure of one inbox request.
/// 一次收件箱请求的失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenVulcanInboxError {
    /// The caller passed an argument the contract forbids; nothing was sent.
    /// 调用方传入契约禁止的参数；未发送任何内容。
    InvalidArgument(&'static str),
    /// The request could not be sent or its answer could not be read.
    /// 请求无法发送，或回答无法读取。
    Transport,
    /// The user center answered with a documented error.
    /// 用户中心以协议规定的错误作答。
    Rejected {
        /// HTTP status of the answer.
        /// 回答的 HTTP 状态码。
        status: u16,
        /// Stable error code, empty when the body carried none.
        /// 稳定错误码；正文未携带时为空。
        code: String,
        /// Seconds from `Retry-After` on a rate-limited answer.
        /// 限流回答中 `Retry-After` 给出的秒数。
        retry_after_secs: Option<u64>,
    },
    /// The answer violated its contract.
    /// 回答违反其契约。
    InvalidAnswer,
}

impl std::fmt::Display for OpenVulcanInboxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument(detail) => write!(formatter, "Invalid inbox request: {detail}"),
            Self::Transport => formatter.write_str("The OpenVulcan inbox request failed."),
            Self::Rejected { status, code, .. } => {
                write!(
                    formatter,
                    "OpenVulcan refused the inbox request with HTTP {status} `{code}`."
                )
            }
            Self::InvalidAnswer => {
                formatter.write_str("The OpenVulcan inbox answer violates its contract.")
            }
        }
    }
}

impl std::error::Error for OpenVulcanInboxError {}

/// Receipt of one accepted feedback item.
/// 一条已接受反馈的回执。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackReceipt {
    /// Public feedback id (`fb_…`).
    /// 公开反馈编号（`fb_…`）。
    pub id: String,
    /// Current triage status.
    /// 当前分拣状态。
    pub status: String,
    /// RFC 3339 creation time.
    /// RFC 3339 创建时间。
    pub created_at: String,
}

/// One feedback item of the caller in a list page.
/// 列表页中调用者的一条反馈。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackSummary {
    /// Public feedback id.
    /// 公开反馈编号。
    pub id: String,
    /// `suggestion` or `bug`.
    /// `suggestion` 或 `bug`。
    pub kind: String,
    /// Current triage status.
    /// 当前分拣状态。
    pub status: String,
    /// Submitted title.
    /// 提交的标题。
    pub title: String,
    /// RFC 3339 creation time.
    /// RFC 3339 创建时间。
    pub created_at: String,
    /// RFC 3339 time of the last status change or public reply.
    /// 最近一次状态变化或公开回复的 RFC 3339 时间。
    pub updated_at: String,
    /// Number of public replies.
    /// 公开回复数量。
    pub reply_count: u64,
}

/// One page of the caller's feedback, newest first.
/// 调用者反馈的一页，按从新到旧排列。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackPage {
    /// Items of this page.
    /// 本页条目。
    pub feedback: Vec<OpenVulcanFeedbackSummary>,
    /// Opaque continuation; `None` on the last page.
    /// 不透明续页游标；末页为 `None`。
    pub next_cursor: Option<String>,
}

/// One public maintainer reply.
/// 一条公开维护者回复。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackReply {
    /// Plain-text reply body.
    /// 纯文本回复正文。
    pub body: String,
    /// RFC 3339 reply time.
    /// RFC 3339 回复时间。
    pub created_at: String,
}

/// One pull request linked to a feedback item.
/// 关联到反馈的一个拉取请求。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackLink {
    /// Link kind; currently `github_pr`.
    /// 链接类型；当前为 `github_pr`。
    pub kind: String,
    /// Pull request address.
    /// 拉取请求地址。
    pub url: String,
    /// RFC 3339 time the link was added.
    /// 添加链接的 RFC 3339 时间。
    pub created_at: String,
}

/// Every link of one feedback item.
/// 一条反馈的全部链接。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackLinks {
    /// Links in the order the deployment returns them.
    /// 按部署返回顺序排列的链接。
    pub links: Vec<OpenVulcanFeedbackLink>,
}

/// One feedback item with its public replies and links.
/// 一条反馈及其公开回复与链接。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanFeedbackDetail {
    /// List fields of the item.
    /// 该条目的列表字段。
    #[serde(flatten)]
    pub summary: OpenVulcanFeedbackSummary,
    /// Submitted body.
    /// 提交的正文。
    pub body: String,
    /// Whether public replies also arrive as personal messages.
    /// 公开回复是否同时以个人消息送达。
    pub allow_contact: bool,
    /// Whether the retention job already cleared the attachments.
    /// 留存任务是否已清除附件。
    pub attachments_purged: bool,
    /// Public replies, oldest first.
    /// 公开回复，按从旧到新排列。
    #[serde(default)]
    pub replies: Vec<OpenVulcanFeedbackReply>,
    /// Linked pull requests.
    /// 关联的拉取请求。
    #[serde(default)]
    pub links: Vec<OpenVulcanFeedbackLink>,
}

/// Targeting conditions an announcement carries; the client evaluates them.
/// 公告携带的定向条件；由客户端判断。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanAnnouncementTarget {
    /// Inclusive lowest product version, or `None` for no bound.
    /// 包含式最低产品版本；无下限时为 `None`。
    pub min_version: Option<String>,
    /// Inclusive highest product version, or `None` for no bound.
    /// 包含式最高产品版本；无上限时为 `None`。
    pub max_version: Option<String>,
    /// Operating systems addressed; empty means every system.
    /// 面向的操作系统；为空表示全部系统。
    #[serde(default)]
    pub os: Vec<String>,
}

/// Localized title and Markdown body of an announcement.
/// 公告的本地化标题与 Markdown 正文。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanAnnouncementContent {
    /// Title.
    /// 标题。
    pub title: String,
    /// Markdown body without raw HTML.
    /// 不含原始 HTML 的 Markdown 正文。
    pub body: String,
}

/// Action attached to an announcement.
/// 公告附带的操作。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanAnnouncementLink {
    /// `app_update`, `url` or `feedback`.
    /// `app_update`、`url` 或 `feedback`。
    pub kind: String,
    /// https address for `url`.
    /// `url` 类型的 https 地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Feedback id for `feedback`.
    /// `feedback` 类型的反馈编号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_id: Option<String>,
}

/// One published announcement.
/// 一条已发布公告。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanAnnouncement {
    /// Public announcement id (`an_…`).
    /// 公开公告编号（`an_…`）。
    pub id: String,
    /// `update`, `event`, `maintenance` or `notice`.
    /// `update`、`event`、`maintenance` 或 `notice`。
    pub kind: String,
    /// `normal` or `important`.
    /// `normal` 或 `important`。
    pub priority: String,
    /// RFC 3339 start of the display window.
    /// 显示窗口的 RFC 3339 开始时间。
    pub starts_at: String,
    /// RFC 3339 end of the display window, or `None` when open.
    /// 显示窗口的 RFC 3339 结束时间；不设结束时为 `None`。
    pub ends_at: Option<String>,
    /// Client-evaluated targeting.
    /// 由客户端判断的定向条件。
    pub target: OpenVulcanAnnouncementTarget,
    /// Locale tag to content; `zh-CN` and `en-US` are always present.
    /// 语言标签到内容的映射；始终包含 `zh-CN` 与 `en-US`。
    pub content: BTreeMap<String, OpenVulcanAnnouncementContent>,
    /// Optional action.
    /// 可选操作。
    pub link: Option<OpenVulcanAnnouncementLink>,
}

/// Every announcement currently inside its window.
/// 当前处于显示窗口内的全部公告。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanAnnouncementDocument {
    /// Deployment-wide announcement revision.
    /// 部署范围的公告修订号。
    pub revision: String,
    /// Announcements of this deployment.
    /// 本部署的公告。
    pub announcements: Vec<OpenVulcanAnnouncement>,
}

/// Result of a conditional announcement read.
/// 条件公告读取的结果。
#[derive(Clone, Debug)]
pub enum OpenVulcanAnnouncementsFetch {
    /// The supplied ETag still matches; the cached document is current.
    /// 提供的 ETag 仍然匹配；缓存文档仍是最新。
    NotModified,
    /// A new document and the ETag to send next time.
    /// 新文档以及下次发送的 ETag。
    Current {
        /// Opaque ETag, when the deployment sent one.
        /// 不透明 ETag，部署发送时存在。
        etag: Option<String>,
        /// Current document.
        /// 当前文档。
        document: OpenVulcanAnnouncementDocument,
    },
}

/// Reference from a personal message to the feedback it concerns.
/// 个人消息指向其相关反馈的引用。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenVulcanMessageRef {
    /// Feedback id for feedback messages.
    /// 反馈类消息的反馈编号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_id: Option<String>,
    /// New status for status messages.
    /// 状态类消息的新状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_status: Option<String>,
}

/// One personal message.
/// 一条个人消息。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanPersonalMessage {
    /// Public message id (`msg_…`).
    /// 公开消息编号（`msg_…`）。
    pub id: String,
    /// `feedback_reply`, `feedback_status`, `direct` or `account`.
    /// `feedback_reply`、`feedback_status`、`direct` 或 `account`。
    pub kind: String,
    /// Title.
    /// 标题。
    pub title: String,
    /// Plain-text body.
    /// 纯文本正文。
    pub body: String,
    /// RFC 3339 creation time.
    /// RFC 3339 创建时间。
    pub created_at: String,
    /// RFC 3339 read time, or `None` while unread.
    /// RFC 3339 已读时间；未读时为 `None`。
    pub read_at: Option<String>,
    /// Related feedback.
    /// 相关反馈。
    #[serde(rename = "ref", default)]
    pub reference: OpenVulcanMessageRef,
}

/// One page of personal messages, newest first.
/// 个人消息的一页，按从新到旧排列。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanMessagePage {
    /// Messages of this page.
    /// 本页消息。
    pub messages: Vec<OpenVulcanPersonalMessage>,
    /// Opaque continuation; `None` on the last page.
    /// 不透明续页游标；末页为 `None`。
    pub next_cursor: Option<String>,
}

/// Which messages one read mark covers.
/// 一次已读标记覆盖的消息。
#[derive(Clone, Debug)]
pub enum OpenVulcanMarkRead {
    /// These exact messages (1 to `OPENVULCAN_MARK_READ_MAX_IDS`).
    /// 这些精确消息（1 至 `OPENVULCAN_MARK_READ_MAX_IDS` 条）。
    Ids(Vec<String>),
    /// Every message up to and including this one.
    /// 截至并包括该消息的全部消息。
    AllBefore(String),
}

/// Outcome of a read mark.
/// 已读标记的结果。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpenVulcanMarkReadResult {
    /// Messages newly marked read.
    /// 新标记为已读的消息数。
    pub updated: u64,
    /// Remaining unread messages.
    /// 剩余未读消息数。
    pub unread: u64,
}

/// Unread summary carried by the account route.
/// account 路由携带的未读摘要。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenVulcanMessageSummary {
    /// Unread personal messages.
    /// 未读个人消息数。
    pub unread: u64,
    /// Newest message id, or `None` without messages.
    /// 最新消息编号；没有消息时为 `None`。
    pub latest_id: Option<String>,
}

/// Envelope of a feedback receipt.
/// 反馈回执的封装。
#[derive(Deserialize)]
struct ReceiptEnvelope {
    feedback: OpenVulcanFeedbackReceipt,
}

/// Envelope of a feedback detail.
/// 反馈详情的封装。
#[derive(Deserialize)]
struct DetailEnvelope {
    feedback: OpenVulcanFeedbackDetail,
}

/// Account fields the message summary needs.
/// 消息摘要需要的账户字段。
#[derive(Deserialize)]
struct AccountMessages {
    authorization_id: String,
    #[serde(default)]
    messages: Option<OpenVulcanMessageSummary>,
}

/// Stable error envelope of the user center.
/// 用户中心的稳定错误封装。
#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

/// Stable error body.
/// 稳定错误正文。
#[derive(Deserialize)]
struct ErrorBody {
    code: String,
}

/// Tells whether `value` is `prefix` followed by exactly 26 base32 characters (`a-z`, `2-7`).
/// 判断 `value` 是否为 `prefix` 后接恰好 26 个 base32 字符（`a-z`、`2-7`）。
fn is_public_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|rest| {
        rest.len() == 26
            && rest
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    })
}

/// Tells whether `value` is a feedback id the routes accept.
/// 判断 `value` 是否为路由接受的反馈编号。
pub fn is_openvulcan_feedback_id(value: &str) -> bool {
    is_public_id(value, "fb_")
}

/// Tells whether `value` is a message id the routes accept.
/// 判断 `value` 是否为路由接受的消息编号。
pub fn is_openvulcan_message_id(value: &str) -> bool {
    is_public_id(value, "msg_")
}

/// Tells whether `url` is a pull request of the Vulcan Code repository the link route accepts.
/// 判断 `url` 是否为关联路由接受的 Vulcan Code 仓库拉取请求。
pub fn is_openvulcan_feedback_pr_url(url: &str) -> bool {
    url.len() <= FEEDBACK_PR_URL_MAX_CHARS
        && url
            .strip_prefix(OPENVULCAN_FEEDBACK_PR_URL_PREFIX)
            .is_some_and(|number| {
                number.bytes().all(|byte| byte.is_ascii_digit())
                    && number.bytes().next().is_some_and(|first| first != b'0')
            })
}

/// Returns the region bound to `secret`.
/// 返回 `secret` 绑定的区域。
fn region_of(secret: &CredentialExecutionSecret) -> Result<OpenVulcanRegion, OpenVulcanInboxError> {
    secret
        .openvulcan_region()
        .ok_or(OpenVulcanInboxError::InvalidArgument(
            "the inbox requires a native OpenVulcan account",
        ))
}

/// Builds a route URL under the region's authorization origin with optional query pairs.
/// 在区域授权来源下构建路由 URL，可附带查询参数。
fn route_url(
    region: OpenVulcanRegion,
    route: &str,
    query: &[(&str, &str)],
) -> Result<reqwest::Url, OpenVulcanInboxError> {
    let mut url = reqwest::Url::parse(&format!(
        "{}/api/v1/vulcancode/{route}",
        region.auth_origin()
    ))
    .map_err(|_| OpenVulcanInboxError::InvalidArgument("the inbox route is malformed"))?;
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            pairs.append_pair(name, value);
        }
    }
    Ok(url)
}

/// Validates an optional cursor against the route limit.
/// 按路由上限校验可选游标。
fn checked_cursor(cursor: Option<&str>) -> Result<Option<&str>, OpenVulcanInboxError> {
    match cursor.filter(|value| !value.is_empty()) {
        Some(value) if value.chars().count() > INBOX_CURSOR_MAX_CHARS => Err(
            OpenVulcanInboxError::InvalidArgument("the cursor exceeds its contract limit"),
        ),
        other => Ok(other),
    }
}

/// Reads at most `limit` bytes of the answer body.
/// 最多读取回答正文的 `limit` 字节。
async fn read_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, OpenVulcanInboxError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| OpenVulcanInboxError::Transport)?
    {
        if body.len() + chunk.len() > limit {
            return Err(OpenVulcanInboxError::InvalidAnswer);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Converts a non-success answer into its documented rejection.
/// 将非成功回答转换为协议规定的拒绝。
async fn rejection(response: reqwest::Response) -> OpenVulcanInboxError {
    let status = response.status().as_u16();
    let retry_after_secs = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let code = read_limited(response, INBOX_MAX_RESPONSE_BYTES)
        .await
        .ok()
        .and_then(|body| serde_json::from_slice::<ErrorEnvelope>(&body).ok())
        .map(|envelope| envelope.error.code)
        .unwrap_or_default();
    OpenVulcanInboxError::Rejected {
        status,
        code,
        retry_after_secs,
    }
}

/// Sends one authorized request and returns the successful answer body.
/// 发送一次已授权请求并返回成功回答的正文。
async fn send_authorized(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    method: reqwest::Method,
    url: reqwest::Url,
    body: Option<Vec<u8>>,
) -> Result<(reqwest::StatusCode, Vec<u8>), OpenVulcanInboxError> {
    let authorization =
        crate::openvulcan::authorization_header(secret, url.as_str()).map_err(|_| {
            OpenVulcanInboxError::InvalidArgument("the credential cannot reach this route")
        })?;
    let mut request = http
        .request(method, url)
        .timeout(INBOX_REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, authorization);
    if let Some(body) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
    }
    let response = request
        .send()
        .await
        .map_err(|_| OpenVulcanInboxError::Transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(rejection(response).await);
    }
    Ok((
        status,
        read_limited(response, INBOX_MAX_RESPONSE_BYTES).await?,
    ))
}

/// Decodes one JSON answer.
/// 解码一个 JSON 回答。
fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, OpenVulcanInboxError> {
    serde_json::from_slice(body).map_err(|_| OpenVulcanInboxError::InvalidAnswer)
}

/// Submits one already encoded feedback request; the bytes are sent exactly as given.
/// 提交一个已编码的反馈请求；字节按原样发送。
///
/// # Parameters
/// 参数
/// `encoded` is the exact `FeedbackCreateRequest` JSON the user previewed.
/// `encoded` 是用户预览过的精确 `FeedbackCreateRequest` JSON。
///
/// # Returns
/// 返回值
/// The receipt of a new item, or of the original item when the request id was already accepted.
/// 新条目的回执；请求编号已被接受时返回原条目的回执。
pub async fn create_openvulcan_feedback(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    encoded: Vec<u8>,
) -> Result<OpenVulcanFeedbackReceipt, OpenVulcanInboxError> {
    let url = route_url(region_of(secret)?, "feedback", &[])?;
    let (_, body) =
        send_authorized(http, secret, reqwest::Method::POST, url, Some(encoded)).await?;
    let envelope: ReceiptEnvelope = decode(&body)?;
    if !is_openvulcan_feedback_id(&envelope.feedback.id) {
        return Err(OpenVulcanInboxError::InvalidAnswer);
    }
    Ok(envelope.feedback)
}

/// Lists the caller's feedback, newest first.
/// 列出调用者的反馈，按从新到旧排列。
pub async fn list_openvulcan_feedback(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    cursor: Option<&str>,
) -> Result<OpenVulcanFeedbackPage, OpenVulcanInboxError> {
    let query: Vec<(&str, &str)> = checked_cursor(cursor)?
        .map(|value| vec![("cursor", value)])
        .unwrap_or_default();
    let url = route_url(region_of(secret)?, "feedback", &query)?;
    let (_, body) = send_authorized(http, secret, reqwest::Method::GET, url, None).await?;
    decode(&body)
}

/// Reads one of the caller's feedback items.
/// 读取调用者的一条反馈。
pub async fn read_openvulcan_feedback(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    feedback_id: &str,
) -> Result<OpenVulcanFeedbackDetail, OpenVulcanInboxError> {
    if !is_openvulcan_feedback_id(feedback_id) {
        return Err(OpenVulcanInboxError::InvalidArgument(
            "the feedback id is malformed",
        ));
    }
    let url = route_url(region_of(secret)?, &format!("feedback/{feedback_id}"), &[])?;
    let (_, body) = send_authorized(http, secret, reqwest::Method::GET, url, None).await?;
    let envelope: DetailEnvelope = decode(&body)?;
    Ok(envelope.feedback)
}

/// Links one Vulcan Code pull request to the caller's feedback item.
/// 将一个 Vulcan Code 拉取请求关联到调用者的反馈。
pub async fn link_openvulcan_feedback_pr(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    feedback_id: &str,
    pr_url: &str,
) -> Result<OpenVulcanFeedbackLinks, OpenVulcanInboxError> {
    if !is_openvulcan_feedback_id(feedback_id) {
        return Err(OpenVulcanInboxError::InvalidArgument(
            "the feedback id is malformed",
        ));
    }
    if !is_openvulcan_feedback_pr_url(pr_url) {
        return Err(OpenVulcanInboxError::InvalidArgument(
            "only pull requests of OpenVulcan/vulcan-code can be linked",
        ));
    }
    let url = route_url(
        region_of(secret)?,
        &format!("feedback/{feedback_id}/links"),
        &[],
    )?;
    let request = serde_json::to_vec(&serde_json::json!({"kind": "github_pr", "url": pr_url}))
        .map_err(|_| OpenVulcanInboxError::InvalidArgument("the link could not be encoded"))?;
    let (_, body) =
        send_authorized(http, secret, reqwest::Method::POST, url, Some(request)).await?;
    decode(&body)
}

/// Permanently deletes one of the caller's feedback items with its replies, links and messages.
/// 永久删除调用者的一条反馈及其回复、链接与消息。
pub async fn delete_openvulcan_feedback(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    feedback_id: &str,
) -> Result<(), OpenVulcanInboxError> {
    if !is_openvulcan_feedback_id(feedback_id) {
        return Err(OpenVulcanInboxError::InvalidArgument(
            "the feedback id is malformed",
        ));
    }
    let url = route_url(
        region_of(secret)?,
        &format!("feedback/{feedback_id}/delete"),
        &[],
    )?;
    send_authorized(
        http,
        secret,
        reqwest::Method::POST,
        url,
        Some(b"{}".to_vec()),
    )
    .await?;
    Ok(())
}

/// Reads the region's announcements without any credential.
/// 不带任何凭据读取该区域的公告。
///
/// # Parameters
/// 参数
/// `etag` is the ETag of the cached document, sent as `If-None-Match`.
/// `etag` 是缓存文档的 ETag，作为 `If-None-Match` 发送。
pub async fn request_openvulcan_announcements(
    http: &reqwest::Client,
    region: OpenVulcanRegion,
    etag: Option<&str>,
) -> Result<OpenVulcanAnnouncementsFetch, OpenVulcanInboxError> {
    let url = route_url(region, "announcements", &[])?;
    let mut request = http.get(url).timeout(INBOX_REQUEST_TIMEOUT);
    if let Some(etag) = etag.filter(|value| !value.is_empty()) {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let response = request
        .send()
        .await
        .map_err(|_| OpenVulcanInboxError::Transport)?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(OpenVulcanAnnouncementsFetch::NotModified);
    }
    if !response.status().is_success() {
        return Err(rejection(response).await);
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = read_limited(response, ANNOUNCEMENTS_MAX_RESPONSE_BYTES).await?;
    Ok(OpenVulcanAnnouncementsFetch::Current {
        etag,
        document: decode(&body)?,
    })
}

/// Lists the caller's personal messages, newest first.
/// 列出调用者的个人消息，按从新到旧排列。
pub async fn list_openvulcan_messages(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    cursor: Option<&str>,
    unread_only: bool,
) -> Result<OpenVulcanMessagePage, OpenVulcanInboxError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(cursor) = checked_cursor(cursor)? {
        query.push(("cursor", cursor));
    }
    if unread_only {
        query.push(("unread", "true"));
    }
    let url = route_url(region_of(secret)?, "messages", &query)?;
    let (_, body) = send_authorized(http, secret, reqwest::Method::GET, url, None).await?;
    decode(&body)
}

/// Marks personal messages read.
/// 将个人消息标记为已读。
pub async fn mark_openvulcan_messages_read(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    selection: &OpenVulcanMarkRead,
) -> Result<OpenVulcanMarkReadResult, OpenVulcanInboxError> {
    let request = match selection {
        OpenVulcanMarkRead::Ids(ids) => {
            if ids.is_empty()
                || ids.len() > OPENVULCAN_MARK_READ_MAX_IDS
                || !ids.iter().all(|id| is_openvulcan_message_id(id))
            {
                return Err(OpenVulcanInboxError::InvalidArgument(
                    "the message ids are empty, too many or malformed",
                ));
            }
            serde_json::json!({ "ids": ids })
        }
        OpenVulcanMarkRead::AllBefore(id) => {
            if !is_openvulcan_message_id(id) {
                return Err(OpenVulcanInboxError::InvalidArgument(
                    "the message id is malformed",
                ));
            }
            serde_json::json!({ "all_before": id })
        }
    };
    let request = serde_json::to_vec(&request)
        .map_err(|_| OpenVulcanInboxError::InvalidArgument("the selection could not be encoded"))?;
    let url = route_url(region_of(secret)?, "messages/read", &[])?;
    let (_, body) =
        send_authorized(http, secret, reqwest::Method::POST, url, Some(request)).await?;
    decode(&body)
}

/// Reads the unread message summary from the account route.
/// 从 account 路由读取未读消息摘要。
///
/// # Returns
/// 返回值
/// `None` when the deployment does not serve the message center yet.
/// 部署尚未提供消息中心时返回 `None`。
pub async fn request_openvulcan_message_summary(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
) -> Result<Option<OpenVulcanMessageSummary>, OpenVulcanInboxError> {
    let url = route_url(region_of(secret)?, "account", &[])?;
    let (_, body) = send_authorized(http, secret, reqwest::Method::GET, url, None).await?;
    let account: AccountMessages = decode(&body)?;
    if Some(account.authorization_id.as_str()) != secret.provider_device_id() {
        return Err(OpenVulcanInboxError::InvalidAnswer);
    }
    Ok(account.messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_ids_require_their_prefix_and_base32_body() {
        assert!(is_openvulcan_feedback_id("fb_abcdefghijklmnopqrstuvwxyz"));
        assert!(is_openvulcan_message_id("msg_234567abcdefghijklmnopqrst"));
        assert!(!is_openvulcan_feedback_id("fb_abcdefghijklmnopqrstuvwxy1"));
        assert!(!is_openvulcan_feedback_id("fb_ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
        assert!(!is_openvulcan_feedback_id("fb_abc/../../account"));
        assert!(!is_openvulcan_message_id("fb_abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn only_vulcan_code_pull_requests_can_be_linked() {
        assert!(is_openvulcan_feedback_pr_url(
            "https://github.com/OpenVulcan/vulcan-code/pull/42"
        ));
        for rejected in [
            "https://github.com/OpenVulcan/vulcan-code/pull/0",
            "https://github.com/OpenVulcan/vulcan-code/pull/",
            "https://github.com/OpenVulcan/vulcan-code/pull/42/files",
            "https://github.com/OpenVulcan/vulcan-code/pull/42?x=1",
            "https://github.com/other/vulcan-code/pull/42",
            "http://github.com/OpenVulcan/vulcan-code/pull/42",
        ] {
            assert!(!is_openvulcan_feedback_pr_url(rejected), "{rejected}");
        }
    }

    #[test]
    fn answers_decode_the_documented_shapes() {
        let detail: DetailEnvelope = decode(
            br#"{"feedback":{"id":"fb_abcdefghijklmnopqrstuvwxyz","kind":"bug","status":"triaged",
            "title":"t","created_at":"2026-11-07T00:00:00Z","updated_at":"2026-11-07T00:00:00Z",
            "reply_count":1,"body":"b","allow_contact":true,"attachments_purged":false,
            "replies":[{"body":"r","created_at":"2026-11-07T00:00:00Z"}]}}"#,
        )
        .unwrap();
        assert_eq!(detail.feedback.replies.len(), 1);
        assert!(detail.feedback.links.is_empty());
        let message: OpenVulcanPersonalMessage = decode(
            br#"{"id":"msg_abcdefghijklmnopqrstuvwxyz","kind":"feedback_reply","title":"t","body":"b",
            "created_at":"2026-11-07T00:00:00Z","read_at":null,"ref":{"feedback_id":"fb_abcdefghijklmnopqrstuvwxyz"}}"#,
        )
        .unwrap();
        assert!(message.reference.feedback_id.is_some());
        let encoded = serde_json::to_value(&message).unwrap();
        assert!(encoded.get("ref").is_some());
    }
}
