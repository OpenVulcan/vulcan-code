//! Runtime streaming entrypoint for provider execution and final report assembly.
//! Provider 执行与最终报告组装的运行时流式入口模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::execution_preparation::prepare_execution;
use crate::agent_runtime::mcp_runtime_projection::collect_enabled_mcp_runtime_events;
use crate::agent_runtime::provider_request_bridge::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_snapshot::{
    build_core_controller_config, build_core_provider_request_snapshot,
    build_provider_request_preview_markdown,
};
use crate::agent_runtime::provider_runtime_context::{
    build_runtime_system_preamble, resolve_runtime_max_turns,
};
use crate::agent_runtime::provider_runtime_dispatch::execute_provider_prompt_stream;
use crate::agent_runtime::provider_runtime_inputs::resolve_provider_additional_params;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_stream_projection::{
    is_provider_tool_boundary_runtime_event, normalize_assistant_output,
    normalize_optional_stream_output, normalize_streamed_thinking_output,
    reset_current_thinking_phase_tracking, runtime_event_to_core_event,
    take_current_thinking_phase_completed_event,
};
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::conversation_messages_to_rig_history;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use async_stream::stream;
use futures_util::{Stream, StreamExt};
use std::path::PathBuf;
use vulcan_core::controller::VulcanCoreController;

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
        let mut preparation = match prepare_execution(
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

        let active_agent = foundation.active_agent_definition().cloned();
        let active_agent_tool_id = active_agent
            .as_ref()
            .map(|agent| agent.id.clone())
            .unwrap_or_else(|| preparation.agent_id.clone());
        let tools_manager = build_vulcan_tools_manager(
            foundation.native_tool_catalog().clone(),
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
            (!preparation.workspace_path.is_empty())
                .then(|| PathBuf::from(preparation.workspace_path.clone())),
            foundation.resolved_native_tools_for_agent(&active_agent_tool_id),
            foundation.resolved_luaskills_tools_for_agent(&active_agent_tool_id),
            foundation.resolved_mcp_tools_for_agent(&active_agent_tool_id),
        );
        let resolved_tools = tools_manager.resolve_for_execution_mode(&preparation.execution_mode);
        let mut runtime_events = Vec::new();
        let mut warnings = preparation.warnings.clone();
        let mut thinking_raw_output = String::new();
        let mut thinking_visible_output = String::new();
        let mut assistant_output = String::new();
        let mut thinking_completed_emitted = false;

        let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
            collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
        warnings.extend(discovery_warnings);

        let system_preamble = build_runtime_system_preamble(&preparation);
        let request_history = conversation_messages_to_rig_history(&request.conversation_state.messages);
        let core_snapshot = build_core_conversation_snapshot(&request.conversation_state);
        let mut core_controller = VulcanCoreController::new(
            build_core_controller_config(
                &preparation,
                &system_preamble,
                resolve_runtime_max_turns(active_agent.as_ref()),
                resolve_provider_additional_params(&preparation.model_blueprint),
            ),
            core_snapshot.clone(),
        );
        match build_core_provider_request_snapshot(
            &preparation,
            &request.conversation_state,
            &system_preamble,
            request.prompt.trim(),
            &resolved_tools.definitions,
            resolve_runtime_max_turns(active_agent.as_ref()),
        )
        .await
        {
            Ok(request_snapshot) => {
                preparation.provider_request_preview_markdown =
                    build_provider_request_preview_markdown(&preparation, &request_snapshot);
                preparation.provider_request_snapshot_json =
                    serde_json::to_value(&request_snapshot).ok();
            }
            Err(error) => {
                preparation.provider_request_preview_markdown = format!(
                    "# 最后一次真实 API 请求快照\n\n请求快照构建失败：{}",
                    error
                );
                preparation.provider_request_snapshot_json = None;
                warnings.push(format!("请求快照构建失败：{error}"));
            }
        }

        yield ExecutionStreamEvent::Prepared(preparation.clone());

        let context_event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "执行上下文",
            format!(
                "已进入真实执行阶段，目标模型为 `{}`，工作区为 `{}`。",
                preparation.model_blueprint.model_name, preparation.workspace_path
            ),
        );
        runtime_events.push(context_event.clone());
        yield ExecutionStreamEvent::RuntimeEvent {
            session_id: request.session_id,
            event: context_event,
        };

        for event in discovery_events {
            runtime_events.push(event.clone());
            yield ExecutionStreamEvent::RuntimeEvent {
                session_id: request.session_id,
                event,
            };
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
            resolved_tools.definitions,
        );

        let mut terminal_error = None;
        let mut final_conversation_state = None;
        let mut latest_request_snapshot = None;

        while let Some(item) = provider_stream.next().await {
            match item {
                Ok(ProviderStreamDelta::RequestSnapshot(snapshot)) => {
                    latest_request_snapshot = Some(snapshot);
                }
                Ok(ProviderStreamDelta::Thinking(delta)) => {
                    core_controller.record_thinking(delta.clone());
                    thinking_raw_output.push_str(&delta);
                    let normalized_visible_output =
                        normalize_streamed_thinking_output(&thinking_raw_output);
                    let visible_delta = normalized_visible_output
                        .strip_prefix(&thinking_visible_output)
                        .unwrap_or(normalized_visible_output.as_str())
                        .to_string();
                    thinking_visible_output = normalized_visible_output;

                    if !visible_delta.is_empty() {
                        yield ExecutionStreamEvent::ThinkingDelta {
                            session_id: request.session_id,
                            delta: visible_delta,
                        };
                    }
                }
                Ok(ProviderStreamDelta::Output(delta)) => {
                    core_controller.record_output(delta.clone());
                    if let Some(thinking_completed_event) =
                        take_current_thinking_phase_completed_event(
                            request.session_id,
                            &thinking_visible_output,
                            &mut thinking_completed_emitted,
                        )
                    {
                        yield thinking_completed_event;
                    }
                    assistant_output.push_str(&delta);
                    yield ExecutionStreamEvent::OutputDelta {
                        session_id: request.session_id,
                        delta,
                    };
                }
                Ok(ProviderStreamDelta::RuntimeEvent(event)) => {
                    if is_provider_tool_boundary_runtime_event(&event) {
                        if let Some(thinking_completed_event) =
                            take_current_thinking_phase_completed_event(
                                request.session_id,
                                &thinking_visible_output,
                                &mut thinking_completed_emitted,
                            )
                        {
                            yield thinking_completed_event;
                        }
                        reset_current_thinking_phase_tracking(
                            &mut thinking_raw_output,
                            &mut thinking_visible_output,
                            &mut thinking_completed_emitted,
                        );
                    }
                    if let Some(core_event) = runtime_event_to_core_event(&event) {
                        core_controller.push_event(core_event);
                    }
                    runtime_events.push(event.clone());
                    yield ExecutionStreamEvent::RuntimeEvent {
                        session_id: request.session_id,
                        event,
                    };
                }
                Ok(ProviderStreamDelta::FinalConversationState(conversation_state)) => {
                    final_conversation_state = Some(conversation_state);
                }
                Err(error) => {
                    terminal_error = Some(error);
                    break;
                }
            }
        }

        if let Some(error) = terminal_error {
            if let Some(thinking_completed_event) = take_current_thinking_phase_completed_event(
                request.session_id,
                &thinking_visible_output,
                &mut thinking_completed_emitted,
            ) {
                yield thinking_completed_event;
            }
            if let Some(request_snapshot) = latest_request_snapshot {
                preparation.provider_request_preview_markdown =
                    build_provider_request_preview_markdown(&preparation, &request_snapshot);
                preparation.provider_request_snapshot_json =
                    serde_json::to_value(&request_snapshot).ok();
            }
            core_controller.mark_failed(error.clone());
            let failed_event = RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "真实执行失败",
                error.clone(),
            );
            runtime_events.push(failed_event.clone());
            yield ExecutionStreamEvent::RuntimeEvent {
                session_id: request.session_id,
                event: failed_event,
            };
            let report = ExecutionRunReport::failure(
                request.session_id,
                Some(preparation),
                discovery_rows,
                discovery_errors,
                warnings,
                core_controller.events().to_vec(),
                error,
            );
            yield ExecutionStreamEvent::Finished(report);
            return;
        }

        if let Some(thinking_completed_event) = take_current_thinking_phase_completed_event(
            request.session_id,
            &thinking_visible_output,
            &mut thinking_completed_emitted,
        ) {
            yield thinking_completed_event;
        }

        if let Some(request_snapshot) = latest_request_snapshot {
            preparation.provider_request_preview_markdown =
                build_provider_request_preview_markdown(&preparation, &request_snapshot);
            preparation.provider_request_snapshot_json =
                serde_json::to_value(&request_snapshot).ok();
        }

        let normalized_output = normalize_assistant_output(assistant_output);
        let output_completed_event = RuntimeEvent::new(
            RuntimeEventKind::TokenDelta,
            "模型输出完成",
            format!(
                "真实 Provider 已返回 {} 个字符的正式输出内容。",
                normalized_output.chars().count()
            ),
        );
        runtime_events.push(output_completed_event.clone());
        yield ExecutionStreamEvent::RuntimeEvent {
            session_id: request.session_id,
            event: output_completed_event,
        };

        let report = ExecutionRunReport::success(
            request.session_id,
            preparation,
            normalize_optional_stream_output(thinking_visible_output),
            normalized_output,
            final_conversation_state,
            discovery_rows,
            discovery_errors,
            warnings,
            core_controller.events().to_vec(),
        );
        yield ExecutionStreamEvent::Finished(report);
    }
}
