//! Runtime foundation for provider transport, MCP runtime integration, and host-owned tool orchestration.
//! 面向 Provider 传输、MCP 运行时集成与宿主自有工具编排的运行时基础模块。

pub mod ai_tool_router;
pub mod config;
pub mod context_injection_runtime;
pub mod context_request_mode;
pub mod conversation_hooks;
pub mod events;
pub mod execution_models;
pub mod execution_preparation;
pub mod executor;
pub mod extension_generation;
pub mod prompt_compaction;
pub mod prompt_composer_runtime;
pub mod provider;
pub mod provider_attachment;
pub mod provider_cache_policy;
pub mod provider_native_tools;
pub mod provider_prompt_layers;
pub mod provider_prompt_loader;
pub mod provider_protocol_anthropic_client;
pub mod provider_protocol_anthropic_decoders;
pub mod provider_protocol_bridge;
pub mod provider_protocol_cohere;
pub mod provider_protocol_content;
pub mod provider_protocol_gemini;
pub mod provider_protocol_openai_compatible;
pub mod provider_protocol_tool_definition;
pub mod provider_reasoning;
pub mod provider_request_bridge_conversation;
pub mod provider_request_bridge_messages;
pub mod provider_request_bridge_records;
pub mod provider_request_bridge_tools;
pub mod provider_request_controller_config;
pub mod provider_request_preview;
pub mod provider_request_snapshot_builder;
pub mod provider_request_snapshot_preview;
pub mod provider_request_snapshot_support;
pub mod provider_request_telemetry;
pub mod provider_request_turn_snapshot;
pub mod provider_runtime_context;
pub mod provider_runtime_dispatch;
pub mod provider_runtime_inputs;
pub mod provider_stream;
pub mod provider_stream_projection;
pub mod provider_system_update_lowering;
pub mod provider_transport;
pub mod provider_transport_adapter_support;
pub mod provider_transport_anthropic;
pub mod provider_transport_anthropic_adapter;
pub mod provider_transport_anthropic_http;
pub mod provider_transport_azure;
pub mod provider_transport_azure_adapter;
pub mod provider_transport_azure_http;
pub mod provider_transport_cohere;
pub mod provider_transport_cohere_adapter;
pub mod provider_transport_cohere_http;
pub mod provider_transport_common_adapter;
pub mod provider_transport_entry_inputs;
pub mod provider_transport_google;
pub mod provider_transport_google_adapter;
pub mod provider_transport_google_http;
pub mod provider_transport_http_support;
pub mod provider_transport_live_delta;
pub mod provider_transport_openai;
pub mod provider_transport_openai_adapter;
pub mod provider_transport_openai_compatible_http;
pub mod provider_transport_openai_responses;
pub mod provider_transport_openai_responses_adapter;
pub mod provider_transport_openai_responses_http;
pub mod provider_transport_openrouter;
pub mod provider_transport_openrouter_adapter;
pub mod provider_transport_request;
pub mod provider_transport_request_budget;
pub mod provider_transport_retry;
pub mod provider_transport_router;
pub mod provider_transport_stream_executor;
pub mod provider_transport_stream_runtime;
pub mod provider_transport_tool_context;
pub mod provider_transport_turn_request;
pub mod provider_transport_wire_capture;
pub mod provider_visible_agent_messages;
pub mod question_runtime;
pub mod runtime;
pub mod runtime_request_capture;
pub mod runtime_stream;
pub mod runtime_stream_bootstrap;
pub mod runtime_stream_bootstrap_core;
pub mod runtime_stream_bootstrap_resources;
pub mod runtime_stream_delta;
pub mod runtime_stream_driver;
pub mod runtime_stream_driver_loop;
pub mod runtime_stream_finalize;
pub mod runtime_stream_intervention;
pub mod runtime_stream_launch;
pub mod runtime_stream_launch_payload;
pub mod runtime_stream_preparation;
pub mod runtime_stream_preparation_failure;
pub mod runtime_stream_preparation_resources;
pub mod runtime_stream_setup;
pub mod session_goal_prompt;
pub mod task_runtime;
pub mod token_budget_runtime;
pub(crate) mod tool_request_argument_assembler;
pub mod tool_request_stream;
pub mod tool_runtime_bridge;
pub mod vmm_hooks;
pub mod vulcan_plugin_hooks;

pub use config::{RuntimeSkillKind, ensure_runtime_defaults};
pub use events::ExecutionStreamEvent;
pub use execution_models::{
    ExecutionKnowledgeAccessContext, ExecutionPreparation, ExecutionPreparationRequest,
    ExecutionPromptPurpose, ExecutionProviderVisibleAgentMessage, ExecutionRunReport,
    ExecutionSessionGoalContext,
};
pub use extension_generation::ExtensionGeneration;
pub use runtime::RuntimeFoundation;
pub use runtime_stream::{
    stream_runtime_run, stream_runtime_run_with_intervention,
    stream_runtime_run_with_intervention_and_plugins,
};
pub use runtime_stream_intervention::{RuntimePromptIntervention, RuntimePromptInterventionError};

#[cfg(test)]
mod runtime_boundary_guard;
