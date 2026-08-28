//! Shared session event reducers for background conversation state updates.
//! 用于后台会话状态更新的共享会话事件归约器。

use serde_json::json;
use std::collections::BTreeSet;
use vulcan_workbench_model::{
    SessionId, SessionMessageKind, SessionMessageRecordKind, SessionRuntimeStateEntry,
    SessionTurnStats, SessionTurnStatsKind, SessionTurnStatsStatus, SessionTurnUsageGroupKind,
    SessionTurnUsageSegment, SessionTurnUsageSegmentKind,
};

use crate::{
    ExecutionRunReport,
    conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
        SessionConversationState, append_record, append_session_conversation_message,
    },
    skills::{SessionDerivedRecord, SessionDerivedSkillChain, SummarySkillInput},
    turn_stats::{
        build_session_turn_stats_from_state_with_segments, merge_usage_segment_into_turn_stats,
    },
};

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
mod open_tool_records;
mod terminal_report;

pub use checkpoint::*;
pub use terminal_report::*;

use open_tool_records::*;

#[cfg(test)]
mod terminal_report_tests;
