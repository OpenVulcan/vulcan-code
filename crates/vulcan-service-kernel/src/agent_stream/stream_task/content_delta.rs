//! Root stream content delta handling.
//! 根流内容增量处理。

use super::*;

/// Records latency markers for one content delta without mutating the legacy transcript.
/// 为一条内容增量记录延迟标记，但不修改旧转录。
///
/// # Parameters
/// 参数
/// `request_trace_state` owns first-event markers, while the remaining values identify the
/// exact V1 semantic fragment already retained by the session owner.
/// `request_trace_state` 持有首事件标记，其余值标识已经由会话所有者保留的精确 V1 语义片段。
///
/// # Returns
/// 返回值
/// Returns no value after updating only non-persistent request diagnostics.
/// 仅更新非持久化请求诊断后不返回值。
pub(super) fn trace_content_delta_stream_event(
    request_trace_state: &mut StreamRequestTraceState,
    session_id: SessionId,
    kind: StreamContentKind,
    delta: &str,
) {
    request_trace_state.emit_first_content_event(session_id, kind.trace_event_label());
    request_trace_state.emit_first_visible_delta(
        session_id,
        kind.visible_delta_label(),
        delta.chars().count(),
    );
}
