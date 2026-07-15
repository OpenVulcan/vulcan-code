//! Provider request latency trace state.
//! Provider 请求延迟追踪状态。

use super::*;

/// Tracks request-local latency markers and emits each first-event trace exactly once.
/// 跟踪请求局部延迟标记，并确保每类首事件追踪只发送一次。
pub(super) struct StreamRequestTraceState {
    /// Monotonic wall-clock sample captured when the stream task started.
    /// 流任务启动时采集的单调墙钟时间样本。
    stream_task_started_at: i128,
    /// Wall-clock sample captured when the current provider request started.
    /// 当前 Provider 请求启动时采集的墙钟时间样本。
    provider_request_started_at: Option<i128>,
    /// Whether the first event after the current provider request was emitted.
    /// 当前 Provider 请求后的首事件是否已经发送。
    first_event_emitted: bool,
    /// Whether the first user-visible delta for the current provider request was emitted.
    /// 当前 Provider 请求的首个用户可见增量是否已经发送。
    first_visible_delta_emitted: bool,
}

impl StreamRequestTraceState {
    /// Creates trace state for one agent stream task.
    /// 为单个 Agent 流任务创建追踪状态。
    ///
    /// # Parameters
    /// # 参数
    /// `stream_task_started_at` is the wall-clock sample captured at task entry.
    /// `stream_task_started_at` 是任务入口处采集的墙钟时间样本。
    ///
    /// # Returns
    /// # 返回值
    /// A state machine with no active provider request and no emitted markers.
    /// 返回尚无活动 Provider 请求且未发送任何标记的状态机。
    pub(super) fn new(stream_task_started_at: i128) -> Self {
        Self {
            stream_task_started_at,
            provider_request_started_at: None,
            first_event_emitted: false,
            first_visible_delta_emitted: false,
        }
    }

    /// Starts latency tracking for a new physical provider request.
    /// 为新的真实 Provider 请求启动延迟追踪。
    ///
    /// # Parameters
    /// # 参数
    /// `started_at` is the wall-clock sample captured at the request boundary.
    /// `started_at` 是在请求边界处采集的墙钟时间样本。
    ///
    /// # Returns
    /// # 返回值
    /// This method returns no value and resets both first-event markers.
    /// 此方法无返回值，并重置两个首事件标记。
    pub(super) fn begin_provider_request(&mut self, started_at: i128) {
        self.provider_request_started_at = Some(started_at);
        self.first_event_emitted = false;
        self.first_visible_delta_emitted = false;
    }

    /// Formats the elapsed time since the active provider request from a shared observation timestamp.
    /// 使用共享观测时间戳格式化活动 Provider 请求启动后的已用时间。
    ///
    /// # Parameters
    /// # 参数
    /// `observed_at` is the timestamp shared by every trace sink for one event.
    /// `observed_at` 是单个事件的所有追踪接收端共享的时间戳。
    ///
    /// # Returns
    /// # 返回值
    /// Milliseconds as text, or `<unknown>` before a request-start event is observed.
    /// 返回毫秒文本；在观察到请求启动事件前返回 `<unknown>`。
    pub(super) fn provider_request_elapsed_label_at(&self, observed_at: i128) -> String {
        self.elapsed_since_provider_request_label(observed_at)
    }

    /// Emits the first content event even when a provider request boundary has not arrived yet.
    /// 即使 Provider 请求边界尚未到达，也发送首个内容事件。
    ///
    /// # Parameters
    /// # 参数
    /// `session_id` identifies the streamed session.
    /// `session_id` 标识正在流式处理的会话。
    /// `event_label` is the stable trace label for the content event.
    /// `event_label` 是内容事件的稳定追踪标签。
    ///
    /// # Returns
    /// # 返回值
    /// `true` when this call emitted the first-event marker, otherwise `false`.
    /// 本次调用发送首事件标记时返回 `true`，否则返回 `false`。
    pub(super) fn emit_first_content_event(
        &mut self,
        session_id: SessionId,
        event_label: &str,
    ) -> bool {
        self.emit_first_event(session_id, event_label)
    }

    /// Emits the first runtime event only after a provider request boundary is active.
    /// 仅在 Provider 请求边界已激活后发送首个运行时事件。
    ///
    /// # Parameters
    /// # 参数
    /// `session_id` identifies the streamed session.
    /// `session_id` 标识正在流式处理的会话。
    /// `event_label` is the stable trace label for the runtime event.
    /// `event_label` 是运行时事件的稳定追踪标签。
    ///
    /// # Returns
    /// # 返回值
    /// `true` when this call emitted the first-event marker, otherwise `false`.
    /// 本次调用发送首事件标记时返回 `true`，否则返回 `false`。
    pub(super) fn emit_first_runtime_event(
        &mut self,
        session_id: SessionId,
        event_label: &str,
    ) -> bool {
        if self.provider_request_started_at.is_none() {
            return false;
        }
        self.emit_first_event(session_id, event_label)
    }

    /// Emits the first user-visible delta marker for the current provider request.
    /// 发送当前 Provider 请求的首个用户可见增量标记。
    ///
    /// # Parameters
    /// # 参数
    /// `session_id` identifies the streamed session.
    /// `session_id` 标识正在流式处理的会话。
    /// `kind` distinguishes thinking output from assistant output.
    /// `kind` 用于区分思考输出和助手输出。
    /// `character_count` is the Unicode scalar count carried by the first delta.
    /// `character_count` 是首个增量携带的 Unicode 标量数量。
    ///
    /// # Returns
    /// # 返回值
    /// `true` when this call emitted the visible-delta marker, otherwise `false`.
    /// 本次调用发送可见增量标记时返回 `true`，否则返回 `false`。
    pub(super) fn emit_first_visible_delta(
        &mut self,
        session_id: SessionId,
        kind: &str,
        character_count: usize,
    ) -> bool {
        if self.first_visible_delta_emitted {
            return false;
        }
        self.first_visible_delta_emitted = true;
        // A single observation timestamp keeps both trace sinks numerically consistent.
        // 单次观测时间戳确保两个追踪接收端的数值完全一致。
        let observed_at = current_unix_millis_value();
        // The shared detail payload prevents trace sinks from drifting in formatting or values.
        // 共享详情载荷用于防止两个追踪接收端的格式或数值发生漂移。
        let detail = format!(
            "session={session_id} kind={kind} elapsed_ms_since_request={} chars={character_count}",
            self.elapsed_since_provider_request_label(observed_at)
        );
        webui_after_precheck_trace("after_precheck_34_first_visible_delta", detail.clone());
        webui_trace("stream_first_visible_delta", detail);
        true
    }

    /// Emits the shared first-event marker when it has not been emitted for the active request.
    /// 在活动请求尚未发送首事件标记时发送共享标记。
    ///
    /// # Parameters
    /// # 参数
    /// `session_id` identifies the streamed session.
    /// `session_id` 标识正在流式处理的会话。
    /// `event_label` is the stable event label written to both trace sinks.
    /// `event_label` 是写入两个追踪接收端的稳定事件标签。
    ///
    /// # Returns
    /// # 返回值
    /// `true` when the marker was emitted, otherwise `false`.
    /// 标记已发送时返回 `true`，否则返回 `false`。
    fn emit_first_event(&mut self, session_id: SessionId, event_label: &str) -> bool {
        if self.first_event_emitted {
            return false;
        }
        self.first_event_emitted = true;
        // A single observation timestamp keeps both trace sinks numerically consistent.
        // 单次观测时间戳确保两个追踪接收端的数值完全一致。
        let observed_at = current_unix_millis_value();
        // The shared detail payload prevents trace sinks from drifting in formatting or values.
        // 共享详情载荷用于防止两个追踪接收端的格式或数值发生漂移。
        let detail = format!(
            "session={session_id} event={event_label} elapsed_ms_since_request={} elapsed_ms_since_task={}",
            self.elapsed_since_provider_request_label(observed_at),
            observed_at.saturating_sub(self.stream_task_started_at)
        );
        webui_after_precheck_trace(
            "after_precheck_33_first_event_after_request",
            detail.clone(),
        );
        webui_trace("stream_first_event_after_request", detail);
        true
    }

    /// Formats provider-request elapsed time from one shared observation timestamp.
    /// 使用一个共享观测时间戳格式化 Provider 请求已用时间。
    ///
    /// # Parameters
    /// # 参数
    /// `observed_at` is the wall-clock sample used by every sink for the same event.
    /// `observed_at` 是同一事件的所有接收端共同使用的墙钟时间样本。
    ///
    /// # Returns
    /// # 返回值
    /// Milliseconds as text, or `<unknown>` when no request-start event has been observed.
    /// 返回毫秒文本；尚未观察到请求启动事件时返回 `<unknown>`。
    fn elapsed_since_provider_request_label(&self, observed_at: i128) -> String {
        self.provider_request_started_at
            .map(|started_at| observed_at.saturating_sub(started_at).to_string())
            .unwrap_or_else(|| String::from("<unknown>"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that one request emits each marker once and a new request resets both markers.
    /// 验证单个请求只发送一次各类标记，并且新请求会重置两个标记。
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
    fn request_boundary_resets_first_event_markers() {
        // The deterministic identifier keeps trace output stable across test runs.
        // 确定性的标识符确保各次测试运行的追踪输出稳定。
        let session_id = Uuid::nil();
        // The state starts before any provider request boundary is observed.
        // 该状态在观察到任何 Provider 请求边界前启动。
        let mut state = StreamRequestTraceState::new(100);

        state.begin_provider_request(120);
        assert!(state.emit_first_content_event(session_id, "thinking_delta"));
        assert!(!state.emit_first_content_event(session_id, "output_delta"));
        assert!(state.emit_first_visible_delta(session_id, "thinking", 3));
        assert!(!state.emit_first_visible_delta(session_id, "output", 4));

        state.begin_provider_request(200);
        assert!(state.emit_first_content_event(session_id, "output_delta"));
        assert!(state.emit_first_visible_delta(session_id, "output", 4));
    }

    /// Verifies the established distinction between early content and early runtime events.
    /// 验证既有语义中提前到达的内容事件与运行时事件之间的区别。
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
    fn runtime_event_requires_request_boundary_but_content_event_does_not() {
        // The deterministic identifier keeps trace output stable across test runs.
        // 确定性的标识符确保各次测试运行的追踪输出稳定。
        let session_id = Uuid::nil();
        // The state intentionally has no active provider request.
        // 该状态刻意不包含活动 Provider 请求。
        let mut state = StreamRequestTraceState::new(100);

        assert!(!state.emit_first_runtime_event(session_id, "runtime.ToolStarted"));
        assert!(state.emit_first_content_event(session_id, "thinking_delta"));
    }
}
