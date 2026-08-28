//! Strongly typed V1 composer-draft service contracts.
//! 强类型 V1 输入区草稿服务契约。

use serde::{Deserialize, Serialize};

use crate::{ComposerAttachmentSummary, SessionResourceSummaryV1};

/// Requests the authoritative composer draft for one materialized V1 session.
/// 请求一个已物化 V1 会话的权威输入区草稿。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionDraftReadV1Request {
    /// Exact materialized session identity.
    /// 精确的已物化会话身份。
    pub session_id: String,
}

/// Replaces the authoritative composer draft for one materialized V1 session.
/// 替换一个已物化 V1 会话的权威输入区草稿。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionDraftWriteV1Request {
    /// Exact materialized session identity.
    /// 精确的已物化会话身份。
    pub session_id: String,
    /// Raw user-authored draft text including intentional whitespace.
    /// 包含用户有意输入空白的原始草稿正文。
    pub text: String,
    /// Ordered staged or project-resource attachment references.
    /// 有序的暂存附件或项目资源附件引用。
    pub attachments: Vec<ComposerAttachmentSummary>,
}

/// Returns one authoritative V1 draft with browser-safe ordered resource metadata.
/// 返回一份带浏览器安全有序资源元数据的权威 V1 草稿。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionDraftV1Response {
    /// Exact materialized session identity.
    /// 精确的已物化会话身份。
    pub session_id: String,
    /// Raw persisted draft text.
    /// 已持久化的原始草稿正文。
    pub text: String,
    /// Ordered resource projections matching metadata input state exactly.
    /// 与 metadata 输入状态精确匹配的有序资源投影。
    pub resources: Vec<SessionResourceSummaryV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies draft requests reject caller-owned project or path hints.
    /// 验证草稿请求会拒绝调用方持有的项目或路径提示。
    #[test]
    fn draft_requests_reject_unknown_ownership_fields() {
        let error = serde_json::from_value::<SessionDraftReadV1Request>(serde_json::json!({
            "sessionId": "session-a",
            "projectId": "must-not-be-client-owned"
        }))
        .expect_err("unknown ownership fields must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }
}
