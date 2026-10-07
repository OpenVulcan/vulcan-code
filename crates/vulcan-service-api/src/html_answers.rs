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
