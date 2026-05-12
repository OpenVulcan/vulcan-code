//! Runtime streaming finalization helpers for request snapshots, output events, and terminal reports.
//! 面向请求快照、输出事件与终态报告的运行时流式收尾辅助模块。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionRunReport};
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use vulcan_core::types::{
    CoreEvent as CoreControllerEvent, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

/// Applies the latest request snapshot to the mutable preparation object when one is available.
/// 当存在最新请求快照时，将其应用到可变执行准备对象上。
pub(crate) fn apply_latest_request_snapshot(
    preparation: &mut ExecutionPreparation,
    request_snapshot: Option<&CoreProviderRequestSnapshot>,
) {
    let Some(request_snapshot) = request_snapshot else {
        return;
    };

    preparation.provider_request_preview_markdown =
        crate::agent_runtime::provider_request_snapshot::build_provider_request_preview_markdown(
            preparation,
            request_snapshot,
        );
    preparation.provider_request_snapshot_json = serde_json::to_value(request_snapshot).ok();
}

/// Builds the standard failed runtime event emitted when provider execution terminates with one error.
/// 构建当 Provider 执行以单条错误终止时发出的标准失败运行时事件。
pub(crate) fn build_failed_runtime_event(error: &str) -> RuntimeEvent {
    RuntimeEvent::new(RuntimeEventKind::Failed, "真实执行失败", error.to_string())
}

/// Builds the standard output-completed event emitted after successful provider output normalization.
/// 构建在成功规范化 Provider 输出之后发出的标准输出完成事件。
pub(crate) fn build_output_completed_event(normalized_output: &str) -> RuntimeEvent {
    RuntimeEvent::new(
        RuntimeEventKind::TokenDelta,
        "模型输出完成",
        format!(
            "真实 Provider 已返回 {} 个字符的正式输出内容。",
            normalized_output.chars().count()
        ),
    )
}

/// Builds one failure execution report for a terminated runtime stream.
/// 为已经终止的运行时流构建一份失败执行报告。
pub(crate) fn build_failure_execution_report(
    session_id: SessionId,
    preparation: ExecutionPreparation,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    controller_events: Vec<CoreControllerEvent>,
    error: impl Into<String>,
) -> ExecutionRunReport {
    ExecutionRunReport::failure(
        session_id,
        Some(preparation),
        mcp_discovery_rows,
        mcp_discovery_errors,
        warnings,
        controller_events,
        error,
    )
}

/// Builds one success execution report for a completed runtime stream.
/// 为已经完成的运行时流构建一份成功执行报告。
pub(crate) fn build_success_execution_report(
    session_id: SessionId,
    preparation: ExecutionPreparation,
    thinking_output: Option<String>,
    assistant_output: String,
    conversation_state: Option<SessionConversationState>,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    controller_events: Vec<CoreControllerEvent>,
) -> ExecutionRunReport {
    ExecutionRunReport::success(
        session_id,
        preparation,
        thinking_output,
        assistant_output,
        conversation_state,
        mcp_discovery_rows,
        mcp_discovery_errors,
        warnings,
        controller_events,
    )
}
