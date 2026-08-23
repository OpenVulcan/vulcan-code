//! Shared subagent task-session projection helpers reused by WebUI and service.
//! 供 WebUI 与服务复用的共享子代理 task-session 投影辅助逻辑。

mod metadata;
mod models;
mod navigation;
mod panel_state;
mod parsing;
mod store;

#[cfg(test)]
mod tests;

use crate::SessionId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context::{
    SessionConversationRecord, SessionConversationState, SessionMessageEntry,
    SessionMessageRecordKind,
};
use vulcan_session_core::{
    ParentSessionRef, SessionContextDocument, SessionNodeId, SessionNodeStore, SessionStoreError,
};
use vulcan_subagent_core::{
    CLOSE_AGENT_TOOL, LEGACY_TASK_SESSION_ID_KEY, SEND_MESSAGE_TOOL, STOPPED_SUBAGENT_TASK_IDS_KEY,
    SUBAGENT_TASK_ID_KEY, SUBAGENT_TASK_KIND, is_readable_subagent_task_kind,
};
use vulcan_task_session::{
    SegmentedTaskSessionStore, TaskSessionMetadata, TaskSessionMirrorSummary, TaskSessionStatus,
    TaskSessionStore,
};

use store::parent_session_id_from_task_session_parent;

pub use metadata::*;
pub use models::*;
pub use navigation::*;
pub use panel_state::*;
pub use parsing::*;
pub use store::*;
