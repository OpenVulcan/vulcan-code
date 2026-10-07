//! Provider request phase event handling.
//! Provider 请求阶段事件处理。

use super::*;

/// Request lifecycle marker emitted by the Agent runtime stream.
/// Agent 运行时流发送的请求生命周期标记。
#[derive(Clone, Copy)]
pub(super) enum RequestPhaseStreamEvent {
    /// The canonical structured request snapshot is ready.
    /// 规范结构化请求快照已经就绪。
    RequestReady,
    /// A new physical provider request has started.
    /// 新的真实 Provider 请求已经启动。
    RequestStarted,
}

/// Applies one request lifecycle marker to latency tracing and retained session phase state.
/// 将单个请求生命周期标记应用到延迟追踪与保留会话阶段状态。
///
/// # Parameters
/// # 参数
/// `app` emits the resulting workbench phase delta.
/// `app` 发送生成的工作台阶段增量。
/// `task_state` owns the retained session phase state.
/// `task_state` 持有保留的会话阶段状态。
/// `request_trace_state` owns physical-request timing and first-event markers.
/// `request_trace_state` 持有真实请求计时与首事件标记。
/// `session_id` identifies the provider-stream session.
/// `session_id` 标识 Provider 流会话。
/// `active_stream_task_id` proves the exact stream allowed to commit the phase.
/// `active_stream_task_id` 证明获准提交阶段的精确流。
/// `stream_task_started_at` is the task-entry timestamp used by latency traces.
/// `stream_task_started_at` 是延迟追踪使用的任务入口时间戳。
/// `event` selects the exact request lifecycle marker to apply.
/// `event` 选择需要应用的精确请求生命周期标记。
///
/// # Returns
/// # 返回值
/// This function returns no value; phase persistence errors are logged and stream polling continues.
/// 此函数无返回值；阶段持久化错误会被记录，流轮询继续执行。
pub(super) fn handle_request_phase_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    request_trace_state: &mut StreamRequestTraceState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    stream_task_started_at: i128,
    event: RequestPhaseStreamEvent,
) {
    // One timestamp keeps paired trace sinks numerically consistent for the same event.
    // 单个时间戳确保同一事件的成对追踪接收端数值一致。
    let observed_at = current_unix_millis_value();
    match event {
        RequestPhaseStreamEvent::RequestReady => {
            // The shared detail prevents the two request-ready trace sinks from drifting.
            // 共享详情用于防止两个请求就绪追踪接收端发生漂移。
            let detail = format!(
                "session={session_id} elapsed_ms_since_task={}",
                observed_at.saturating_sub(stream_task_started_at)
            );
            webui_after_precheck_trace("after_precheck_30_request_ready", detail.clone());
            webui_trace("stream_provider_request_ready", detail);
        }
        RequestPhaseStreamEvent::RequestStarted => {
            request_trace_state.begin_provider_request(observed_at);
            webui_after_precheck_trace(
                "after_precheck_32_request_started",
                format!(
                    "session={session_id} elapsed_ms_since_task={}",
                    observed_at.saturating_sub(stream_task_started_at)
                ),
            );
            webui_trace("stream_request_started", format!("session={session_id}"));
        }
    }

    match apply_active_stream_session_phase(
        app,
        task_state,
        session_id,
        active_stream_task_id,
        request_phase_session_phase(event),
    ) {
        Ok(outcome) => webui_trace(
            "stream_request_phase_commit",
            format!("session={session_id} task={active_stream_task_id} outcome={outcome:?}"),
        ),
        Err(error) => {
            tracing::warn!(
                "[webui-service] {}: {error}",
                request_phase_failure_context(event)
            );
        }
    }
}

/// Returns the retained session phase required by one request lifecycle marker.
/// 返回单个请求生命周期标记要求的保留会话阶段。
///
/// # Parameters
/// # 参数
/// `event` is the request lifecycle marker being projected.
/// `event` 是正在投影的请求生命周期标记。
///
/// # Returns
/// # 返回值
/// Every request lifecycle marker maps to `SessionRunPhase::Requesting`.
/// 每个请求生命周期标记都映射到 `SessionRunPhase::Requesting`。
fn request_phase_session_phase(_event: RequestPhaseStreamEvent) -> SessionRunPhase {
    SessionRunPhase::Requesting
}

/// Returns the stable failure context for one request lifecycle marker.
/// 返回单个请求生命周期标记对应的稳定失败上下文。
///
/// # Parameters
/// # 参数
/// `event` is the request lifecycle marker whose phase update failed.
/// `event` 是阶段更新失败的请求生命周期标记。
///
/// # Returns
/// # 返回值
/// The existing diagnostic prefix associated with the marker.
/// 返回与该标记关联的既有诊断前缀。
fn request_phase_failure_context(event: RequestPhaseStreamEvent) -> &'static str {
    match event {
        RequestPhaseStreamEvent::RequestReady => "apply request-ready phase failed",
        RequestPhaseStreamEvent::RequestStarted => "apply request-started failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that every request lifecycle marker retains the requesting phase.
    /// 验证每个请求生命周期标记都会保留请求阶段。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn request_lifecycle_markers_share_requesting_phase() {
        for event in [
            RequestPhaseStreamEvent::RequestReady,
            RequestPhaseStreamEvent::RequestStarted,
        ] {
            assert_eq!(
                request_phase_session_phase(event),
                SessionRunPhase::Requesting
            );
        }
    }

    /// Verifies that request lifecycle markers preserve distinct diagnostic contexts.
    /// 验证请求生命周期标记会保留不同的诊断上下文。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn request_lifecycle_markers_preserve_failure_contexts() {
        assert_eq!(
            request_phase_failure_context(RequestPhaseStreamEvent::RequestReady),
            "apply request-ready phase failed"
        );
        assert_eq!(
            request_phase_failure_context(RequestPhaseStreamEvent::RequestStarted),
            "apply request-started failed"
        );
    }
}
