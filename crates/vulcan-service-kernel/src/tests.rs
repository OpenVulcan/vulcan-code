use super::*;
use crate::git::{
    WebuiWorkspaceDiffMode, run_webui_git_command, webui_diff_hunk_scoped_unified_diff,
    webui_unified_diff_file_summaries, webui_unified_diff_line_count, webui_unified_diff_stats,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use vulcan_agent_runtime::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload,
    conversation_state_to_compaction_source_messages,
};
use vulcan_workbench_model::SessionTurnUsageSegmentKind;

pub(crate) mod test_support;

use test_support::*;

mod active_session_state_tests;
mod capability_metadata_tests;

/// Verifies application shutdown drains tasks before VMM and releases MCP last.
/// 验证应用关闭会先排空任务、再关闭 VMM，并最后释放 MCP。
#[test]
fn runtime_shutdown_order_is_tasks_then_managed_memory_then_mcp() {
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let task_events = Arc::clone(&events);
    let memory_events = Arc::clone(&events);
    let mcp_events = Arc::clone(&events);
    let stopped_id = Uuid::new_v4();

    let stopped = ordered_runtime_shutdown(
        move || {
            task_events.lock().expect("task events").push("tasks");
            Ok(vec![stopped_id])
        },
        move || {
            memory_events.lock().expect("memory events").push("vmm");
            Ok(())
        },
        move || {
            mcp_events.lock().expect("mcp events").push("mcp");
            Ok(())
        },
    )
    .expect("ordered shutdown");

    assert_eq!(stopped, vec![stopped_id]);
    assert_eq!(
        *events.lock().expect("final events"),
        vec!["tasks", "vmm", "mcp"]
    );
}
mod conversation_selection_tests;
mod conversation_submission_activation_tests;
mod conversation_submission_preflight_tests;
mod conversation_submission_queue_tests;
mod detached_subagent_tests;
mod goal_recovery_tests;
mod goal_todolist_tests;
mod hot_path_scan_guard_tests;
mod mcp_settings_tests;
mod pending_question_projection_tests;
mod prompt_runtime_state_tests;
mod rpc_diff_tests;
mod session_kernel_tests;
mod settings_projection_tests;
mod stop_task_mirror_tests;
mod subagent_fixed_model_tests;
mod subagent_navigation_tests;
