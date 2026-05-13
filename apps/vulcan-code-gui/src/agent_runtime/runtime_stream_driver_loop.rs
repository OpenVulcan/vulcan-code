//! Runtime stream provider-loop driver for one prepared launch payload.
//! 面向单个已准备启动载荷的运行时流 Provider 循环驱动模块。

use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::provider_runtime_dispatch::execute_provider_prompt_stream;
use crate::agent_runtime::runtime_stream_delta::process_provider_stream_delta;
use crate::agent_runtime::runtime_stream_finalize::{
    build_failure_terminal_events, build_success_terminal_events,
};
use crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch;
use crate::provider_config::ProviderConfigurationFile;
use async_stream::stream;
use futures_util::{Stream, StreamExt};

/// Drives one prepared runtime-stream launch payload and yields provider delta and terminal events in order.
/// 驱动一份已准备完成的运行时流启动载荷，并按顺序产出 Provider 增量与终态事件。
pub(crate) fn drive_runtime_stream_provider_loop(
    config: ProviderConfigurationFile,
    launch: RuntimeStreamLaunch,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let session_id = launch.session_id;
        let effective_prompt = launch.effective_prompt;
        let preparation = launch.preparation;
        let active_agent = launch.active_agent;
        let task_runtime = launch.task_runtime;
        let tools_manager = launch.tools_manager;
        let resolved_tool_definitions = launch.resolved_tool_definitions;
        let discovery_rows = launch.discovery_rows;
        let discovery_errors = launch.discovery_errors;
        let warnings = launch.warnings;
        let system_preamble = launch.system_preamble;
        let request_history = launch.request_history;
        let core_snapshot = launch.core_snapshot;
        let mut stream_state = launch.stream_state;

        let mut provider_stream = execute_provider_prompt_stream(
            &config,
            &preparation,
            active_agent.as_ref(),
            &system_preamble,
            &effective_prompt,
            request_history,
            core_snapshot,
            task_runtime,
            tools_manager,
            resolved_tool_definitions,
        );

        while let Some(item) = provider_stream.next().await {
            for event in process_provider_stream_delta(session_id, &mut stream_state, item) {
                yield event;
            }
            if stream_state.has_terminal_error() {
                break;
            }
        }

        if let Some(error) = stream_state.take_terminal_error() {
            for event in build_failure_terminal_events(
                session_id,
                preparation,
                discovery_rows,
                discovery_errors,
                warnings,
                &mut stream_state,
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
        ) {
            yield event;
        }
    }
}
