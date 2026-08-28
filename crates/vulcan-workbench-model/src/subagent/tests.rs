use super::*;
use serde_json::json;
use vulcan_context::SessionMessageKind;

mod metadata;

/// Builds one compact test panel state with the requested lifecycle state.
/// 构建一条带指定生命周期状态的紧凑测试面板状态。
fn test_subagent_state(
    parent_session_id: SessionId,
    task_id: SessionId,
    run_state: SubagentTaskRunState,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id,
        public_task_id: task_id.to_string(),
        parent_session_id,
        subagent_label: String::from("worker"),
        description: None,
        run_state,
        result_text: String::new(),
        conversation_state: None,
    }
}
