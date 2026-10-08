//! Session-owned answer mode commands, durable receipts and the global default.
//! 会话自有回答模式命令、可靠回执与全局默认值。

use serde::{Deserialize, Serialize};
use vulcan_session_core::v1::config::{AnswerMode, AnswerModePreference};

/// Compare-and-set request shared by GUI, WebUI and TUI.
/// GUI、WebUI 和 TUI 共用的比较交换请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionAnswerModeUpdateRequest {
    /// Exact existing session; drafts submit their mode with first-turn settings.
    /// 精确的已有会话；草稿随首轮设置提交模式。
    pub session_id: String,
    /// Desired mode for the next business request preparation.
    /// 下一次业务请求准备时的期望模式。
    pub mode: AnswerMode,
    /// Last observed mode revision, independent from stream revisions.
    /// 最近观察到的模式修订，独立于流式修订。
    pub expected_revision: u64,
}

/// Durable answer mode result; conflicts return current authority without overwriting it.
/// 可靠回答模式结果；冲突返回当前权威状态且不覆盖它。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionAnswerModeUpdateResponse {
    /// Exact session owner of this response.
    /// 本响应的精确会话归属。
    pub session_id: String,
    /// History generation at the serialized mutation boundary.
    /// 串行修改边界处的历史代次。
    pub window_generation: u64,
    /// Document revision whose durable publication completed before returning.
    /// 返回前已完成可靠发布的文档修订。
    pub revision: u64,
    /// Current and prepared answer mode captured by the same writer.
    /// 同一写入器捕获的当前与准备回答模式。
    pub answer_mode: AnswerModePreference,
    /// True when the request revision was stale and no mode was changed.
    /// 请求修订过期且未修改模式时为真。
    pub conflict: bool,
}

/// Saves the answer mode copied into new conversations.
/// 保存复制到新会话的回答模式。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptAnswerModeSaveRequest {
    /// Global default for conversations created after this save.
    /// 此次保存之后创建的会话所用的全局默认值。
    pub mode: AnswerMode,
}
