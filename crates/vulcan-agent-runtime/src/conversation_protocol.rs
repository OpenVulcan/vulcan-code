//! Runtime facade for the shared canonical conversation protocol and compaction rewrite helpers.
//! 面向运行时的共享规范会话协议与压缩改写辅助门面。

use crate::context_budget::ContextCompactionPolicy;
use crate::tooling::change_set::compact_change_set_host_result;

pub use vulcan_context::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    append_assistant_text_message, append_record, append_record_body_delta,
    append_user_text_message, collect_host_visible_attachments,
    conversation_state_from_session_messages, conversation_state_to_context_records,
    conversation_state_to_session_messages, merge_session_messages_with_conversation_state_records,
};

/// Rewrites the active canonical window using one compaction summary plus the retained recent tail.
/// 使用一条压缩摘要与保留的最近尾部消息改写当前活动规范窗口。
pub fn compact_conversation_with_summary(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_kind: crate::model::SessionMessageKind,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    policy: ContextCompactionPolicy,
) {
    vulcan_context_compaction::compact_conversation_with_summary(
        state,
        summary_message_id,
        summary_kind,
        summary_body,
        updated_at,
        policy,
    );
}

/// Rewrites the active canonical window using one compaction summary and the planner source boundary.
/// 使用一条压缩摘要与规划器源边界改写当前活动规范窗口。
///
/// # Parameters
/// # 参数
/// `state` is the canonical conversation window to rewrite.
/// `state` 是需要改写的规范会话窗口。
/// `summary_message_id` is the hidden summary message identifier inserted as the new context anchor.
/// `summary_message_id` 是作为新上下文锚点插入的隐藏摘要消息标识。
/// `summary_kind` controls the synthetic summary message role.
/// `summary_kind` 控制合成摘要消息角色。
/// `summary_body` is the durable continuation summary.
/// `summary_body` 是持久续写摘要。
/// `updated_at` is written to the continuation metadata.
/// `updated_at` 会写入 continuation 元数据。
/// `source_until_message_id` is the inclusive source boundary covered by the summary.
/// `source_until_message_id` 是摘要覆盖的闭区间源边界。
/// `policy` is the fallback tail policy when the boundary is unavailable.
/// `policy` 是边界不可用时的回退尾部策略。
pub fn compact_conversation_with_summary_until(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_kind: crate::model::SessionMessageKind,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    source_until_message_id: Option<&str>,
    policy: ContextCompactionPolicy,
) {
    vulcan_context_compaction::compact_conversation_with_summary_until(
        state,
        summary_message_id,
        summary_kind,
        summary_body,
        updated_at,
        source_until_message_id,
        policy,
    );
}

/// Compacts canonical records after summary creation so old tool details stop inflating persistence.
/// 在摘要创建后压缩规范记录，避免旧工具细节继续放大持久化体积。
pub fn compact_conversation_records_after_summary(
    state: &mut SessionConversationState,
    summary_record_id: &str,
    policy: ContextCompactionPolicy,
) {
    vulcan_context_compaction::compact_conversation_records_after_summary(
        state,
        summary_record_id,
        policy,
        compact_change_set_host_result,
    );
}
