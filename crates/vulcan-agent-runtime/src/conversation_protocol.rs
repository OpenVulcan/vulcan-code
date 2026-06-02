//! Thin GUI adapter for shared canonical conversation protocol and compaction rewrite helpers.
//! 面向图形界面的共享规范会话协议与压缩改写辅助薄适配层。

use crate::context_budget::ContextCompactionPolicy;
use crate::tooling::change_set::compact_change_set_host_result;

pub use vulcan_context::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    append_record, append_record_body_delta, append_user_text_message,
    collect_host_visible_attachments, conversation_state_from_session_messages,
    conversation_state_to_context_records, conversation_state_to_session_messages,
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
