use serde::{Deserialize, Serialize};

/// Process-local lifecycle status for one subagent execution.
/// 单次子代理执行的进程内生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSessionStatus {
    /// The child execution has been created but not started.
    /// 子代理执行已创建但尚未启动。
    Created,
    /// The child execution is currently running.
    /// 子代理执行正在运行。
    Running,
    /// The child execution is waiting for nested child tasks.
    /// 子代理执行正在等待嵌套子任务。
    WaitingChildren,
    /// The child execution is waiting for a follow-up prompt.
    /// 子代理执行正在等待后续提示词。
    WaitingFollowup,
    /// The child execution completed successfully.
    /// 子代理执行已成功完成。
    Completed,
    /// The child execution failed.
    /// 子代理执行已失败。
    Failed,
    /// The child execution was stopped.
    /// 子代理执行已停止。
    Stopped,
    /// The child execution is compacting its request context.
    /// 子代理执行正在压缩请求上下文。
    Compacting,
    /// The process-local child record was archived from the active registry.
    /// 进程内子代理记录已从活动注册表归档。
    Archived,
}

impl TaskSessionStatus {
    /// Returns whether this status still represents active work.
    /// 返回该状态是否仍代表活动工作。
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Created
                | Self::Running
                | Self::WaitingChildren
                | Self::WaitingFollowup
                | Self::Compacting
        )
    }

    /// Returns whether this status represents a settled execution result.
    /// 返回该状态是否代表已落定的执行结果。
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Stopped)
    }
}

#[cfg(test)]
mod tests {
    use super::TaskSessionStatus;

    /// Verifies completed, failed, and stopped statuses share the terminal contract.
    /// 验证完成、失败和停止状态共享同一终态契约。
    #[test]
    fn task_session_status_terminal_flags_cover_all_settled_results() {
        assert!(TaskSessionStatus::Completed.is_terminal());
        assert!(TaskSessionStatus::Failed.is_terminal());
        assert!(TaskSessionStatus::Stopped.is_terminal());
    }

    /// Verifies active and archival statuses are not execution-result terminals.
    /// 验证活动状态和归档状态不是执行结果终态。
    #[test]
    fn task_session_status_terminal_flags_exclude_active_and_archived_states() {
        assert!(!TaskSessionStatus::Created.is_terminal());
        assert!(!TaskSessionStatus::Running.is_terminal());
        assert!(!TaskSessionStatus::WaitingChildren.is_terminal());
        assert!(!TaskSessionStatus::WaitingFollowup.is_terminal());
        assert!(!TaskSessionStatus::Compacting.is_terminal());
        assert!(!TaskSessionStatus::Archived.is_terminal());
    }
}
