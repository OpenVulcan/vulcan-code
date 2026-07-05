//! Runtime stream provider-loop driver for one prepared launch payload.
//! 面向单个已准备启动载荷的运行时流 Provider 循环驱动模块。

use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::provider_runtime_dispatch::execute_provider_prompt_stream;
use crate::agent_runtime::runtime_stream_delta::process_provider_stream_delta;
use crate::agent_runtime::runtime_stream_finalize::{
    build_failure_terminal_events, build_success_terminal_events,
};
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch;
use crate::model::SessionId;
use crate::provider_config::ProviderConfigurationFile;
use async_stream::stream;
use futures_util::{Stream, StreamExt};
use std::time::Duration;

/// Workbench-facing stream delta flush cadence used to avoid repainting the full workbench for every provider token.
/// 面向工作台的流式增量刷新节奏，用于避免每个 Provider token 都触发整窗重绘。
const RUNTIME_STREAM_WORKBENCH_DELTA_FLUSH_INTERVAL: Duration = Duration::from_millis(50);

/// Accumulates adjacent visible stream deltas while preserving non-delta event ordering.
/// 在保留非增量事件顺序的同时，合并相邻的可见流式增量。
#[derive(Default)]
struct ExecutionStreamDeltaCoalescer {
    /// Pending visible thinking delta for the current flush window.
    /// 当前刷新窗口内等待发出的可见思考增量。
    pending_thinking_delta: Option<(SessionId, String)>,
    /// Pending visible output delta for the current flush window.
    /// 当前刷新窗口内等待发出的可见输出增量。
    pending_output_delta: Option<(SessionId, String)>,
}

impl ExecutionStreamDeltaCoalescer {
    /// Returns whether no visible delta is waiting for the next workbench flush.
    /// 返回当前是否没有等待下一次工作台刷新的可见增量。
    fn is_empty(&self) -> bool {
        self.pending_thinking_delta.is_none() && self.pending_output_delta.is_none()
    }

    /// Adds one execution event and returns any ordered events that must be emitted before it.
    /// 添加一个执行事件，并返回在它之前必须按顺序发出的事件集合。
    fn push_event(&mut self, event: ExecutionStreamEvent) -> Vec<ExecutionStreamEvent> {
        match event {
            ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                if self.pending_output_delta.is_none()
                    && let Some((pending_session_id, pending_delta)) =
                        self.pending_thinking_delta.as_mut()
                    && *pending_session_id == session_id
                {
                    pending_delta.push_str(&delta);
                    return Vec::new();
                }

                let flushed_events = self.flush();
                self.pending_thinking_delta = Some((session_id, delta));
                flushed_events
            }
            ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                if self.pending_thinking_delta.is_none()
                    && let Some((pending_session_id, pending_delta)) =
                        self.pending_output_delta.as_mut()
                    && *pending_session_id == session_id
                {
                    pending_delta.push_str(&delta);
                    return Vec::new();
                }

                let flushed_events = self.flush();
                self.pending_output_delta = Some((session_id, delta));
                flushed_events
            }
            other_event => {
                let mut flushed_events = self.flush();
                flushed_events.push(other_event);
                flushed_events
            }
        }
    }

    /// Drains all pending visible deltas as ordered execution events.
    /// 将所有等待发出的可见增量按顺序提取为执行事件。
    fn flush(&mut self) -> Vec<ExecutionStreamEvent> {
        let mut events = Vec::new();

        if let Some((session_id, delta)) = self.pending_thinking_delta.take() {
            events.push(ExecutionStreamEvent::ThinkingDelta { session_id, delta });
        }

        if let Some((session_id, delta)) = self.pending_output_delta.take() {
            events.push(ExecutionStreamEvent::OutputDelta { session_id, delta });
        }

        events
    }
}

/// Drives one prepared runtime-stream launch payload and yields provider delta and terminal events in order.
/// 驱动一份已准备完成的运行时流启动载荷，并按顺序产出 Provider 增量与终态事件。
pub(crate) fn drive_runtime_stream_provider_loop(
    config: ProviderConfigurationFile,
    launch: RuntimeStreamLaunch,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let session_id = launch.session_id;
        let task_execution_context = launch.task_execution_context;
        let effective_prompt = launch.effective_prompt;
        let preparation = launch.preparation;
        let active_agent = launch.active_agent;
        let task_runtime = launch.task_runtime;
        let token_budget_runtime = launch.token_budget_runtime;
        let tools_manager = launch.tools_manager;
        let resolved_tool_definitions = launch.resolved_tool_definitions;
        let discovery_rows = launch.discovery_rows;
        let discovery_errors = launch.discovery_errors;
        let warnings = launch.warnings;
        let system_preamble = launch.system_preamble;
        let request_history = launch.request_history;
        let core_snapshot = launch.core_snapshot;
        let mut stream_state = launch.stream_state;
        let _task_execution_context_guard = if let Some(context) = task_execution_context {
            match task_runtime.install_execution_context(session_id, context) {
                Ok(guard) => Some(guard),
                Err(error) => {
                    for event in build_failure_terminal_events(
                        session_id,
                        preparation,
                        discovery_rows,
                        discovery_errors,
                        warnings,
                        &mut stream_state,
                        token_budget_runtime.estimator(),
                        error,
                    ) {
                        yield event;
                    }
                    return;
                }
            }
        } else {
            None
        };

        let mut provider_stream = execute_provider_prompt_stream(
            &config,
            &preparation,
            active_agent.as_ref(),
            &system_preamble,
            &effective_prompt,
            request_history,
            core_snapshot,
            task_runtime,
            token_budget_runtime.clone(),
            tools_manager,
            resolved_tool_definitions,
            prompt_intervention,
        );

        let mut delta_coalescer = ExecutionStreamDeltaCoalescer::default();
        let mut next_delta_flush_at =
            tokio::time::Instant::now() + RUNTIME_STREAM_WORKBENCH_DELTA_FLUSH_INTERVAL;

        loop {
            let item = if delta_coalescer.is_empty() {
                provider_stream.next().await
            } else {
                match tokio::time::timeout_at(next_delta_flush_at, provider_stream.next()).await {
                    Ok(item) => item,
                    Err(_) => {
                        for event in delta_coalescer.flush() {
                            yield event;
                        }
                        next_delta_flush_at = tokio::time::Instant::now()
                            + RUNTIME_STREAM_WORKBENCH_DELTA_FLUSH_INTERVAL;
                        continue;
                    }
                }
            };

            let Some(item) = item else {
                break;
            };

            for event in process_provider_stream_delta(session_id, &mut stream_state, item) {
                let was_empty = delta_coalescer.is_empty();
                let flushed_events = delta_coalescer.push_event(event);
                let starts_new_flush_window =
                    (was_empty || !flushed_events.is_empty()) && !delta_coalescer.is_empty();

                for flushed_event in flushed_events {
                    yield flushed_event;
                }

                if starts_new_flush_window {
                    next_delta_flush_at = tokio::time::Instant::now()
                        + RUNTIME_STREAM_WORKBENCH_DELTA_FLUSH_INTERVAL;
                }
            }

            if !delta_coalescer.is_empty() && tokio::time::Instant::now() >= next_delta_flush_at {
                for event in delta_coalescer.flush() {
                    yield event;
                }
                next_delta_flush_at =
                    tokio::time::Instant::now() + RUNTIME_STREAM_WORKBENCH_DELTA_FLUSH_INTERVAL;
            }

            if stream_state.has_terminal_error() {
                break;
            }
        }

        for event in delta_coalescer.flush() {
            yield event;
        }

        if let Some(error) = stream_state.take_terminal_error() {
            for event in build_failure_terminal_events(
                session_id,
                preparation,
                discovery_rows,
                discovery_errors,
                warnings,
                &mut stream_state,
                token_budget_runtime.estimator(),
                error,
            ) {
                yield event;
            }
            return;
        }

        for event in build_success_terminal_events(
            session_id,
            preparation,
            discovery_rows,
            discovery_errors,
            warnings,
            &mut stream_state,
            token_budget_runtime.estimator(),
        ) {
            yield event;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Runtime stream driver-loop tests for workbench-facing delta coalescing.
    //! 面向工作台增量合并的运行时流驱动循环测试。

    use super::*;
    use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
    use uuid::Uuid;

    /// Verifies adjacent output deltas for the same session are merged into one workbench event.
    /// 验证同一会话的相邻输出增量会合并为一个工作台事件。
    #[test]
    fn coalescer_merges_adjacent_output_deltas_for_same_session() {
        let session_id = Uuid::nil();
        let mut coalescer = ExecutionStreamDeltaCoalescer::default();

        assert!(
            coalescer
                .push_event(ExecutionStreamEvent::OutputDelta {
                    session_id,
                    delta: String::from("hello"),
                })
                .is_empty()
        );
        assert!(
            coalescer
                .push_event(ExecutionStreamEvent::OutputDelta {
                    session_id,
                    delta: String::from(" world"),
                })
                .is_empty()
        );

        let flushed_events = coalescer.flush();
        assert_eq!(flushed_events.len(), 1);
        assert!(matches!(
            &flushed_events[0],
            ExecutionStreamEvent::OutputDelta { delta, .. } if delta == "hello world"
        ));
    }

    /// Verifies non-delta events force pending text to flush before the structural event is emitted.
    /// 验证非增量事件会先冲刷待发文本，再按顺序发出结构化事件。
    #[test]
    fn coalescer_flushes_pending_output_before_runtime_event() {
        let session_id = Uuid::nil();
        let mut coalescer = ExecutionStreamDeltaCoalescer::default();

        assert!(
            coalescer
                .push_event(ExecutionStreamEvent::OutputDelta {
                    session_id,
                    delta: String::from("partial"),
                })
                .is_empty()
        );

        let flushed_events = coalescer.push_event(ExecutionStreamEvent::RuntimeEvent {
            session_id,
            event: RuntimeEvent::new(RuntimeEventKind::ToolStarted, "tool", "started"),
        });

        assert_eq!(flushed_events.len(), 2);
        assert!(matches!(
            &flushed_events[0],
            ExecutionStreamEvent::OutputDelta { delta, .. } if delta == "partial"
        ));
        assert!(matches!(
            &flushed_events[1],
            ExecutionStreamEvent::RuntimeEvent { event, .. }
                if event.kind == RuntimeEventKind::ToolStarted
        ));
    }
}
