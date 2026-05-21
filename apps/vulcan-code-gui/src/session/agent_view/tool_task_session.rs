//! Dedicated task-session tool transcript projection helpers.
//! 专属 task-session 工具转录投影辅助逻辑。

use super::AgentViewBuildOptions;
use super::tool_shared::{
    build_base_tool_attributes, build_tool_directive_block, task_session_payload,
};
use crate::model::SessionMessageEntry;
use vulcan_agent_view::{
    AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
};

/// Builds one task-session tool transcript block with the task routing attributes required by the GUI and transcript renderer.
/// 构建一条携带任务路由属性的 task-session 工具转录区块，供 GUI 与转录渲染器使用。
pub(crate) fn build_task_session_tool_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) -> AgentViewDirectiveBlock {
    let mut attributes =
        build_base_tool_attributes(message, AgentViewToolPresentationKind::TaskSession, options);
    if let Some(task_session_id) = task_session_payload(message)
        .and_then(|payload| {
            payload
                .get("task_session_id")
                .or_else(|| payload.get("task_id"))
        })
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        attributes.insert(String::from("task-session-id"), task_session_id.to_string());
    }
    if let Some(task_execution_status) = task_session_payload(message)
        .and_then(|payload| payload.get("execution_status"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        attributes.insert(
            String::from("task-session-execution-status"),
            task_execution_status.to_string(),
        );
    }

    build_tool_directive_block(kind, attributes)
}
