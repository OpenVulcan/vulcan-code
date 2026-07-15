//! Root stream content delta handling.
//! 根流内容增量处理。

use super::*;

/// Applies one typed provider content delta to latency tracing and transcript projection.
/// 将单个强类型 Provider 内容增量应用到延迟追踪与转录投影。
///
/// # Parameters
/// # 参数
/// `app` emits the projected workbench message delta.
/// `app` 发送投影后的工作台消息增量。
/// `task_state` owns retained workspace and transcript state.
/// `task_state` 持有保留工作区与转录状态。
/// `request_trace_state` owns first-event and first-visible-delta markers.
/// `request_trace_state` 持有首事件与首个可见增量标记。
/// `record_ids` owns independent thinking and output streaming record identifiers.
/// `record_ids` 持有相互独立的思考与输出流式记录标识。
/// `session_id` identifies the root provider-stream session.
/// `session_id` 标识根 Provider 流会话。
/// `kind` selects the exact content and record semantics.
/// `kind` 选择精确的内容与记录语义。
/// `delta` is the provider content fragment to append.
/// `delta` 是需要追加的 Provider 内容片段。
///
/// # Returns
/// # 返回值
/// This function returns no value; projection errors are logged and stream polling continues.
/// 此函数无返回值；投影错误会被记录，流轮询继续执行。
pub(super) fn handle_content_delta_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    request_trace_state: &mut StreamRequestTraceState,
    record_ids: &mut SessionStreamingRecordIds,
    session_id: SessionId,
    kind: StreamContentKind,
    delta: String,
) {
    request_trace_state.emit_first_content_event(session_id, kind.trace_event_label());
    request_trace_state.emit_first_visible_delta(
        session_id,
        kind.visible_delta_label(),
        delta.chars().count(),
    );
    if let Err(error) =
        append_stream_content_delta(app, task_state, session_id, record_ids, kind, &delta)
    {
        eprintln!("[webui-service] {}: {error}", kind.append_failure_context());
    }
}
