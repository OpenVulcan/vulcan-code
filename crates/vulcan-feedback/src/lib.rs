//! Feedback sent from Vulcan Code to the OpenVulcan user center.
//! 从 Vulcan Code 发送到 OpenVulcan 用户中心的反馈。
//!
//! Collection, filtering and encoding happen here, before anything leaves the machine. The service
//! kernel gathers the inputs, the interface only shows the prepared preview, and submission sends the
//! exact bytes whose digest the user previewed.
//! 采集、过滤与编码都在这里完成，发生在任何内容离开本机之前。服务内核负责收集输入，界面只展示准备好的
//! 预览，提交时发送的正是用户预览过、摘要一致的字节。

pub mod limits;
mod log_excerpt;
mod model;
mod prepare;
mod redact;

pub use log_excerpt::{read_recent_log_text, select_log_lines};
pub use model::{
    FeedbackAttachments, FeedbackClientInfo, FeedbackCreateRequest, FeedbackErrorAttachment,
    FeedbackKind, FeedbackRedactionSummary, FeedbackSource,
};
pub use prepare::{FeedbackDraft, PreparedFeedback, prepare_feedback};
pub use redact::{
    HIT_PATH, HIT_SECRET, HIT_URL, HIT_USER, KnownSecret, RedactionHits, Redactor, RedactorInputs,
};

/// Stable reason a draft cannot be prepared; the interface maps it to a catalog message.
/// 草稿无法准备的稳定原因；界面将其映射为目录文案。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackErrorCode {
    /// The title is empty.
    /// 标题为空。
    TitleRequired,
    /// The title exceeds its limit.
    /// 标题超出上限。
    TitleTooLong,
    /// The description is empty.
    /// 描述为空。
    BodyRequired,
    /// The description exceeds its limit.
    /// 描述超出上限。
    BodyTooLong,
    /// A generated field is malformed.
    /// 某个生成字段格式错误。
    InvalidField,
    /// The encoded request exceeds the body limit.
    /// 编码后的请求超出请求体上限。
    RequestTooLarge,
    /// The filter could not run.
    /// 过滤器无法运行。
    FilterFailed,
}

impl FeedbackErrorCode {
    /// Wire spelling shared with the interface.
    /// 与界面共享的线协议拼写。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TitleRequired => "title_required",
            Self::TitleTooLong => "title_too_long",
            Self::BodyRequired => "body_required",
            Self::BodyTooLong => "body_too_long",
            Self::InvalidField => "invalid_field",
            Self::RequestTooLarge => "request_too_large",
            Self::FilterFailed => "filter_failed",
        }
    }
}

/// Coded preparation failure with an English diagnostic detail.
/// 带英文诊断细节的编码化准备失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackError {
    code: FeedbackErrorCode,
    detail: String,
}

impl FeedbackError {
    /// Creates an error with a stable code and an English detail.
    /// 以稳定错误码和英文细节创建错误。
    pub fn new(code: FeedbackErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// Creates a filter failure.
    /// 创建过滤失败错误。
    pub(crate) fn filter(detail: impl Into<String>) -> Self {
        Self::new(FeedbackErrorCode::FilterFailed, detail)
    }

    /// Stable code.
    /// 稳定错误码。
    pub const fn code(&self) -> FeedbackErrorCode {
        self.code
    }

    /// English diagnostic detail.
    /// 英文诊断细节。
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl std::fmt::Display for FeedbackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.detail)
    }
}

impl std::error::Error for FeedbackError {}
