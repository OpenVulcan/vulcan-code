//! Session-owned HTML answer preference commands and durable receipts.
//! 会话自有 HTML 回答偏好命令与可靠回执。

use serde::{Deserialize, Serialize};
use vulcan_session_core::v1::config::HtmlAnswerMode;

/// Compare-and-set request shared by GUI, WebUI and TUI.
/// GUI、WebUI 和 TUI 共用的比较交换请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionHtmlAnswerModeUpdateRequest {
    /// Exact existing session; drafts submit their preference with first-turn settings.
    /// 精确的已有会话；草稿随首轮设置提交偏好。
    pub session_id: String,
    /// Desired preference for the next business request preparation.
    /// 下一次业务请求准备时的期望偏好。
    pub enabled: bool,
    /// Last observed preference revision, independent from stream revisions.
    /// 最近观察到的偏好修订，独立于流式修订。
    pub expected_revision: u64,
}

/// Durable preference result; conflicts return current authority without overwriting it.
/// 可靠偏好结果；冲突返回当前权威状态且不覆盖它。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionHtmlAnswerModeUpdateResponse {
    /// Exact session owner of this response.
    /// 本响应的精确会话归属。
    pub session_id: String,
    /// History generation at the serialized mutation boundary.
    /// 串行修改边界处的历史代次。
    pub window_generation: u64,
    /// Document revision whose durable publication completed before returning.
    /// 返回前已完成可靠发布的文档修订。
    pub revision: u64,
    /// Current desired and prepared preference captured by the same writer.
    /// 同一写入器捕获的当前期望与准备偏好。
    pub html_answers: HtmlAnswerMode,
    /// True when the request revision was stale and no preference was changed.
    /// 请求修订过期且未修改偏好时为真。
    pub conflict: bool,
}

/// Exact immutable source identity supplied by the accepted conversation projection.
/// 已接纳会话投影提供的精确不可变源码身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlAnswerSourceIdentity {
    /// Canonical UUID of the owning session.
    /// 所属会话的规范 UUID。
    pub session_id: uuid::Uuid,
    /// Explicit indexed parent for an embedded task, matching hydration ownership.
    /// 嵌入式任务的显式已索引父会话，与加载归属一致。
    #[serde(default)]
    pub parent_session_id: Option<uuid::Uuid>,
    /// Persisted history generation, invalidated by edit or replacement.
    /// 持久历史代次，编辑或替换后失效。
    pub window_generation: u64,
    /// Exact event sequence containing the original assistant text.
    /// 包含原始助手正文的精确事件序号。
    pub event_seq: u64,
    /// Exact semantic revision whose offsets were projected.
    /// 被投影偏移所属的精确语义修订。
    pub event_revision: u64,
    /// Full Core-derived SHA-256 identity; never a model-assigned identifier.
    /// Core 派生的完整 SHA-256 身份；不是模型分配的标识。
    pub artifact_id: String,
}

/// Authenticated client request to mint one temporary, result-scoped viewer link.
/// 已鉴权客户端申请一个临时且限定结果的查看链接。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlAnswerViewLinkRequest {
    /// Client lifetime identity used for bounded issuance and explicit revocation.
    /// 用于有界签发及显式撤销的客户端生命周期身份。
    pub client_id: uuid::Uuid,
    /// Source selected by the accepted Core projection.
    /// 已接纳 Core 投影选定的源码。
    pub source: HtmlAnswerSourceIdentity,
    /// Shared-catalog locale selected by this client.
    /// 当前客户端选择的共享目录语言。
    pub locale: String,
}

/// Secret-bearing link receipt; intentionally has no Debug implementation.
/// 携带秘密的链接回执；刻意不实现 Debug。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlAnswerViewLinkResponse {
    /// Non-secret handle used by the authenticated client to revoke its own grant.
    /// 已鉴权客户端用于撤销自身授权的非秘密句柄。
    pub grant_id: uuid::Uuid,
    /// Trusted Core viewer address with a single-use secret only in its fragment.
    /// 可信 Core 查看地址，单次秘密只位于片段中。
    pub viewer_url: String,
    /// Lifetime from issuance, in seconds; history stores neither this link nor its secret.
    /// 签发起的有效秒数；历史既不保存该链接也不保存秘密。
    pub expires_in_seconds: u64,
}

/// Authenticated revocation of a client's pending ticket or active viewer lease.
/// 已鉴权撤销客户端的待兑换票据或活动查看租约。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HtmlAnswerRevokeRequest {
    /// Client that originally requested the grant.
    /// 最初申请该授权的客户端。
    pub client_id: uuid::Uuid,
    /// Non-secret grant identity returned by link issuance.
    /// 链接签发返回的非秘密授权身份。
    pub grant_id: uuid::Uuid,
}

/// Stable rejection category shared by native RPC and the credential-free viewer shell.
/// 原生 RPC 与无通用凭据查看器外壳共用的稳定拒绝分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlAnswerViewError {
    /// Malformed source, origin, or protocol request.
    /// 源码身份、来源或协议请求格式不合法。
    InvalidRequest,
    /// The source or hosting service is unavailable.
    /// 源码或宿主服务不可用。
    Unavailable,
    /// Stored generation, revision or content no longer matches the selected result.
    /// 存储代次、修订或内容已不匹配所选结果。
    StaleSource,
    /// The grant expired, was consumed, or was revoked.
    /// 授权已过期、已消费或已撤销。
    Expired,
    /// A bounded ticket or lease table has reached capacity.
    /// 有界票据或租约表已达到容量。
    Capacity,
    /// This client is issuing links too rapidly.
    /// 当前客户端签发链接过快。
    RateLimited,
    /// The owning Core listener has stopped.
    /// 所属 Core 监听器已停止。
    Closed,
}
