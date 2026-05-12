//! Runtime streaming finalization helpers for request snapshots, output events, and terminal reports.
//! 面向请求快照、输出事件与终态报告的运行时流式收尾辅助模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionRunReport};
use crate::agent_runtime::provider_stream_projection::{
    normalize_assistant_output, normalize_optional_stream_output,
};
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
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

/// Builds the complete failure event batch emitted after one runtime stream terminates with a terminal provider error.
/// 构建在运行时流以终态 Provider 错误结束后发出的完整失败事件批次。
pub(crate) fn build_failure_terminal_events(
    session_id: SessionId,
    mut preparation: ExecutionPreparation,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    stream_state: &mut RuntimeStreamDeltaState,
    error: String,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::new();
    if let Some(thinking_completed_event) = stream_state.complete_pending_thinking(session_id) {
        events.push(thinking_completed_event);
    }
    apply_latest_request_snapshot(&mut preparation, stream_state.latest_request_snapshot());
    stream_state.controller_mut().mark_failed(error.clone());
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_failed_runtime_event(&error),
    });
    events.push(ExecutionStreamEvent::Finished(
        build_failure_execution_report(
            session_id,
            preparation,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            stream_state.controller().events().to_vec(),
            error,
        ),
    ));
    events
}

/// Builds the complete success event batch emitted after one runtime stream finishes without terminal provider errors.
/// 构建在运行时流无终态 Provider 错误完成后发出的完整成功事件批次。
pub(crate) fn build_success_terminal_events(
    session_id: SessionId,
    mut preparation: ExecutionPreparation,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    stream_state: &mut RuntimeStreamDeltaState,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::new();
    if let Some(thinking_completed_event) = stream_state.complete_pending_thinking(session_id) {
        events.push(thinking_completed_event);
    }

    apply_latest_request_snapshot(&mut preparation, stream_state.latest_request_snapshot());

    let normalized_output = normalize_assistant_output(stream_state.take_assistant_output());
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_output_completed_event(&normalized_output),
    });
    events.push(ExecutionStreamEvent::Finished(
        build_success_execution_report(
            session_id,
            preparation,
            normalize_optional_stream_output(stream_state.thinking_visible_output().to_string()),
            normalized_output,
            stream_state.take_final_conversation_state(),
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            stream_state.controller().events().to_vec(),
        ),
    ));
    events
}
