use super::*;

/// Builds the compact label used by one subagent dispatch row from its agent name only.
/// 仅根据代理名称构建单条子代理调度行使用的紧凑标签。
pub fn subagent_navigation_task_label(state: &SubagentTaskPanelState) -> String {
    let label = state.subagent_label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    String::from("智能体")
}

/// Converts one explicit execution-status string into the canonical subagent run state.
/// 将一条明确的执行状态字符串转换为规范子代理运行状态。
pub fn subagent_run_state_from_execution_status(
    status: Option<&str>,
) -> Option<SubagentTaskRunState> {
    match status.map(str::trim).filter(|value| !value.is_empty())? {
        "created" | "running" | "waiting_children" | "waiting_followup" | "compacting" => {
            Some(SubagentTaskRunState::Running)
        }
        "completed" | "archived" => Some(SubagentTaskRunState::Completed),
        "stopped" => Some(SubagentTaskRunState::Stopped),
        "failed" => Some(SubagentTaskRunState::Failed),
        _ => None,
    }
}
