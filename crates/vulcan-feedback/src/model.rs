//! Wire shapes sent to `POST /api/v1/vulcancode/feedback`.
//! 发送到 `POST /api/v1/vulcancode/feedback` 的线协议结构。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the user is sending.
/// 用户发送的内容类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackKind {
    /// An idea or request.
    /// 想法或需求。
    Suggestion,
    /// A problem report.
    /// 问题报告。
    Bug,
}

/// Which entry produced the draft; used only for counting on the server.
/// 草稿来自哪个入口；服务端只用于统计。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSource {
    /// The account menu or settings entry.
    /// 账户菜单或设置入口。
    Manual,
    /// The "report this problem" button on a session error card.
    /// 会话错误卡上的“报告此问题”按钮。
    ErrorCard,
    /// A draft written by the AI `feedback` tool and reviewed by the user.
    /// AI `feedback` 工具起草、由用户检查的草稿。
    AiDraft,
}

/// Closed client environment description.
/// 封闭的客户端环境说明。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackClientInfo {
    /// Product version from `product.json`.
    /// 来自 `product.json` 的产品版本。
    pub version: String,
    /// Install layout epoch from `product.json`.
    /// 来自 `product.json` 的安装布局批次。
    pub install_epoch: u32,
    /// `debug` or `release`.
    /// `debug` 或 `release`。
    pub channel: String,
    /// Operating system family.
    /// 操作系统类别。
    pub os: String,
    /// CPU architecture.
    /// CPU 架构。
    pub arch: String,
    /// Interface locale.
    /// 界面语言。
    pub locale: String,
}

/// Structured facts of one provider failure; every text field is filtered before sending.
/// 一次供应商失败的结构化事实；每个文本字段发送前都经过过滤。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackErrorAttachment {
    /// Stable error category.
    /// 稳定错误分类。
    pub category: String,
    /// Optional finer category.
    /// 可选的细分类。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    /// Execution stage that failed.
    /// 失败的执行阶段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Upstream HTTP status.
    /// 上游 HTTP 状态码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// Provider identifier.
    /// 供应商标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Model identifier.
    /// 模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Upstream error code.
    /// 上游错误码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<String>,
    /// Upstream request identifier.
    /// 上游请求编号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Whether the failure was retryable.
    /// 失败是否可重试。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// Filtered human-readable message.
    /// 过滤后的可读消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Optional evidence attached to a feedback item.
/// 反馈附带的可选证据。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackAttachments {
    /// Entry that produced the draft.
    /// 草稿来源入口。
    pub source: FeedbackSource,
    /// Provider failure facts.
    /// 供应商失败事实。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FeedbackErrorAttachment>,
    /// Filtered service log lines joined by newlines.
    /// 以换行连接的过滤后服务日志行。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_excerpt: Option<String>,
}

/// Counts of what filtering replaced, without any original text.
/// 过滤替换内容的计数，不含任何原文。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRedactionSummary {
    /// Identity of the bundled rule set that ran.
    /// 实际运行的随包规则集标识。
    pub rules_version: String,
    /// Replacement count per category.
    /// 每个类别的替换次数。
    pub hits: BTreeMap<String, u32>,
}

/// Complete create request; the encoded bytes of this value are what the user previews.
/// 完整的创建请求；该值的编码字节就是用户预览的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackCreateRequest {
    /// Idempotency key generated once per draft.
    /// 每个草稿生成一次的幂等键。
    pub client_request_id: String,
    /// Suggestion or problem.
    /// 建议或问题。
    pub kind: FeedbackKind,
    /// Filtered title.
    /// 过滤后的标题。
    pub title: String,
    /// Filtered description.
    /// 过滤后的描述。
    pub body: String,
    /// Whether maintainers may reply through account messages.
    /// 是否允许维护者通过账号消息回复。
    pub allow_contact: bool,
    /// Client environment; absent when the user unticks it.
    /// 客户端环境；用户取消勾选时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<FeedbackClientInfo>,
    /// Attached evidence.
    /// 附带证据。
    pub attachments: FeedbackAttachments,
    /// Filtering summary.
    /// 过滤摘要。
    pub redaction: FeedbackRedactionSummary,
}
