//! Runtime streaming entrypoint for provider execution and final report assembly.
//! Provider 执行与最终报告组装的运行时流式入口模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::execution_preparation::prepare_execution;
use crate::agent_runtime::provider_runtime_dispatch::execute_provider_prompt_stream;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_delta::{
    RuntimeStreamDeltaState, process_provider_stream_delta,
};
use crate::agent_runtime::runtime_stream_finalize::{
    build_failure_terminal_events, build_success_terminal_events,
};
use crate::agent_runtime::runtime_stream_setup::{
    bootstrap_runtime_stream, build_runtime_start_events,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use async_stream::stream;
use futures_util::{Stream, StreamExt};

/// Streams one real provider run and yields preparation, thinking, output, and final-report events incrementally.
/// 以增量方式流式执行一次真实 Provider 调用，并依次产出准备、思考、输出与最终报告事件。
pub fn stream_runtime_run(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: ExecutionPreparationRequest,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let foundation = RuntimeFoundation::build(
            &config,
            &agent_profile_catalog,
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
        );
        let preparation = match prepare_execution(
            &foundation,
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
            &config,
            &request,
        ) {
            Ok(preparation) => preparation,
            Err(error) => {
                let report = ExecutionRunReport::failure(
                    request.session_id,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    error,
                );
                yield ExecutionStreamEvent::Finished(report);
                return;
            }
        };

        let bootstrap = bootstrap_runtime_stream(
            &foundation,
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
            &request,
            preparation,
        )
        .await;
        let preparation = bootstrap.preparation;
        let active_agent = bootstrap.active_agent;
        let tools_manager = bootstrap.tools_manager;
        let resolved_tool_definitions = bootstrap.resolved_tool_definitions;
        let discovery_events = bootstrap.discovery_events;
        let discovery_rows = bootstrap.discovery_rows;
        let discovery_errors = bootstrap.discovery_errors;
        let warnings = bootstrap.warnings;
        let system_preamble = bootstrap.system_preamble;
        let request_history = bootstrap.request_history;
        let core_snapshot = bootstrap.core_snapshot;
        let mut stream_state = RuntimeStreamDeltaState::new(bootstrap.core_controller);

        for event in build_runtime_start_events(
            request.session_id,
            &preparation,
            discovery_events,
        ) {
            yield event;
        }

        let mut provider_stream = execute_provider_prompt_stream(
            &config,
            &preparation,
            active_agent.as_ref(),
            &system_preamble,
            request.prompt.trim(),
            request_history,
            core_snapshot,
            tools_manager,
            resolved_tool_definitions,
        );

        while let Some(item) = provider_stream.next().await {
            for event in process_provider_stream_delta(request.session_id, &mut stream_state, item) {
                yield event;
            }
            if stream_state.has_terminal_error() {
                break;
            }
        }

        if let Some(error) = stream_state.take_terminal_error() {
            for event in build_failure_terminal_events(
                request.session_id,
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
            request.session_id,
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
