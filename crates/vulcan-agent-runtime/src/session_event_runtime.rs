//! Shared session event reducers for background conversation state updates.
//! 用于后台会话状态更新的共享会话事件归约器。

use serde_json::json;
use std::collections::BTreeSet;
use vulcan_core::types::ConversationAttachment;
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMessageKind, SessionMessageRecordKind,
    SessionRuntimeStateEntry, SessionTurnStats, SessionTurnStatsKind, SessionTurnStatsStatus,
    SessionTurnUsageSegment, SessionTurnUsageSegmentKind,
};

use crate::{
    ExecutionRunReport,
    agent_runtime::events::{RuntimeEvent, RuntimeEventKind},
    conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
        SessionConversationState, append_record, append_record_body_delta,
        append_session_conversation_message, append_user_text_message,
    },
    skills::{SessionDerivedRecord, SessionDerivedSkillChain, SummarySkillInput},
    turn_stats::{
        build_session_turn_stats_from_state_with_segments,
        merge_explicit_usage_segment_into_turn_stats, merge_usage_segment_into_turn_stats,
    },
};

/// Local identifiers for records created while one streaming run is active.
/// 单次流式运行活跃期间创建的本地记录标识。
#[derive(Debug, Clone, Default)]
pub struct SessionStreamingRecordIds {
    /// Collapsed thinking record that receives thinking deltas.
    /// 接收思考增量的默认折叠思考记录。
    pub thinking_record_id: Option<String>,
    /// Visible assistant output record that receives final-output deltas.
    /// 接收正式输出增量的可见助手输出记录。
    pub output_record_id: Option<String>,
}

/// Builds the stable transcript record id used by legacy session projections.
/// 构建旧会话投影使用的稳定转录记录标识。
pub fn build_session_record_message_id(
    session_id: SessionId,
    created_at: &str,
    record_index: usize,
) -> String {
    format!("{session_id}-{created_at}-{record_index}")
}

mod checkpoint;
mod event_projection;
mod open_tool_records;
mod streaming_records;
mod terminal_report;
mod tool_display_payloads;
mod tool_event_records;
mod user_prompt;

pub use checkpoint::*;
pub use event_projection::*;
pub use streaming_records::*;
pub use terminal_report::*;
pub use user_prompt::*;

use open_tool_records::*;
use tool_display_payloads::*;
use tool_event_records::*;

#[cfg(test)]
mod tests;
