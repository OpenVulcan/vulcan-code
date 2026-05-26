//! Runtime stream preparation failure helpers for terminal report projection.
//! 面向终态报告投影的运行时流准备失败辅助模块。

use crate::agent_runtime::execution_models::ExecutionRunReport;
use crate::model::SessionId;

/// Builds the terminal execution report emitted when runtime preparation fails before streaming begins.
/// 构建在流式执行开始前运行时准备失败时发出的终态执行报告。
pub(crate) fn build_preparation_failure_report(
    session_id: SessionId,
    error: impl Into<String>,
) -> ExecutionRunReport {
    ExecutionRunReport::failure(
        session_id,
        None,
        0,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        error,
    )
}

#[cfg(test)]
mod tests {
    //! Preparation-failure projection tests for runtime launch preparation.
    //! 面向运行时启动准备流程的准备失败投影测试。

    use super::*;
    use uuid::Uuid;

    /// Preserves the session identifier and terminal error while leaving optional success payloads empty.
    /// 保留会话标识与终态错误，同时确保可选成功载荷保持为空。
    #[test]
    fn preparation_failure_report_contains_only_terminal_error_payload() {
        let session_id = Uuid::nil();
        let report = build_preparation_failure_report(session_id, "runtime preparation failed");

        assert_eq!(report.session_id, session_id);
        assert_eq!(report.error.as_deref(), Some("runtime preparation failed"));
        assert!(report.preparation.is_none());
        assert!(report.assistant_output.is_none());
        assert!(report.thinking_output.is_none());
        assert!(report.conversation_state.is_none());
        assert!(report.controller_events.is_empty());
        assert!(report.mcp_discovery_rows.is_empty());
        assert!(report.mcp_discovery_errors.is_empty());
        assert!(report.warnings.is_empty());
    }
}
