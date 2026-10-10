//! Dedicated-session (专职助手) memory-lock requests shared by WebUI and TUI.
//! WebUI 与 TUI 共用的专职会话（专职助手）记忆锁定请求。

use serde::{Deserialize, Serialize};
use vulcan_session_core::v1::dedicated::DedicatedSessionProfile;
use vulcan_session_core::v1::ids::Seq;

/// Locks or unlocks the memory of one dedicated session between turns.
/// 在回合之间锁定或解锁一个专职会话的记忆。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DedicatedMemoryLockRequest {
    /// Exact existing dedicated session.
    /// 精确的已有专职会话。
    pub session_id: String,
    /// Desired lock state; repeating the current state is an idempotent no-op.
    /// 期望的锁定状态；重复当前状态为幂等空操作。
    pub locked: bool,
}

/// Durable lock state written by the unique session writer.
/// 由唯一会话写入器写入的可靠锁定状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DedicatedMemoryLockResponse {
    /// Exact session owner of this response.
    /// 本响应的精确会话归属。
    pub session_id: String,
    /// History generation at the serialized mutation boundary.
    /// 串行修改边界处的历史代次。
    pub window_generation: u64,
    /// Document revision whose durable publication completed before returning.
    /// 返回前已完成可靠发布的文档修订。
    pub revision: u64,
    /// Seq of the appended lock/unlock note; `None` when the state was already as requested.
    /// 追加的锁定/解锁通知 seq；状态已符合请求时为 `None`。
    pub note_seq: Option<Seq>,
    /// Profile including the updated locked spans.
    /// 包含更新后锁定区间的配置。
    pub dedicated: DedicatedSessionProfile,
}

/// Drafts or revises a dedicated-session base prompt with the model selected in the composer.
/// 使用输入区所选模型起草或修订专职会话基础提示词。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DedicatedPromptDraftRequest {
    /// Exact provider of the selected model.
    /// 所选模型的精确供应商。
    pub provider_id: String,
    /// Exact credential connection of the selected model.
    /// 所选模型的精确凭据连接。
    pub connection_id: String,
    /// Exact selected model.
    /// 精确的所选模型。
    pub model_id: String,
    /// What the dedicated assistant must do, in the user's words.
    /// 用户描述的专职助手职责。
    pub requirement: String,
    /// Prompt to revise; absent when drafting a new one.
    /// 待修订的提示词；起草新提示词时为空。
    #[serde(default)]
    pub current_prompt: Option<String>,
    /// Additional instructions for the revision.
    /// 修订时的补充说明。
    #[serde(default)]
    pub revision_request: Option<String>,
}

/// Generated base prompt; the client shows it for review before creating the session.
/// 生成的基础提示词；客户端在创建会话前展示供用户审阅。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DedicatedPromptDraftResponse {
    /// Prompt text within the dedicated base prompt limit.
    /// 不超过专职基础提示词上限的提示词文本。
    pub prompt: String,
    /// Provider that produced the prompt.
    /// 生成提示词的供应商。
    pub provider_id: String,
    /// Model that produced the prompt.
    /// 生成提示词的模型。
    pub model_id: String,
}
