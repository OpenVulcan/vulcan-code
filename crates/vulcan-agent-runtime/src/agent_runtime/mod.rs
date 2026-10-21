//! Runtime foundation for provider transport, MCP runtime integration, and host-owned tool orchestration.
//! 面向 Provider 传输、MCP 运行时集成与宿主自有工具编排的运行时基础模块。

pub(crate) mod code_mode;
pub mod config;
pub mod context_injection_runtime;
pub mod context_request_mode;
pub mod conversation_hooks;
pub mod events;
pub mod execution_models;
pub mod execution_preparation;
pub mod extension_generation;
pub(crate) mod markdown_memory_hooks;
pub mod prompt_compaction;
pub mod prompt_composer_runtime;
pub mod provider;
pub mod provider_attachment;
pub mod provider_continuation;
pub mod provider_prompt_loader;
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
pub mod provider_runtime_context;
pub mod provider_runtime_dispatch;
pub mod provider_runtime_inputs;
pub mod provider_stream;
pub mod provider_stream_projection;
pub mod provider_system_update_lowering;
pub mod provider_tool_event_format;
pub mod provider_transport;
pub mod provider_transport_entry_inputs;
pub mod provider_transport_live_delta;
pub mod provider_transport_request;
pub mod provider_transport_request_budget;
pub mod provider_transport_retry;
pub mod provider_transport_stream_executor;
pub mod provider_transport_stream_runtime;
pub mod provider_transport_tool_context;
pub mod provider_transport_vulcan;
pub mod provider_v1_bridge;
pub mod provider_visible_agent_messages;
pub mod question_runtime;
pub mod runtime;
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
pub mod tool_permission;
pub(crate) mod tool_request_argument_assembler;
pub mod tool_request_stream;
pub(crate) mod tool_result_media;
pub mod tool_runtime_bridge;
pub mod vmm_hooks;
pub mod vulcan_plugin_hooks;

pub use code_mode::release_code_mode_session;
pub use config::{RuntimeSkillKind, ensure_runtime_defaults};
pub use events::{ExecutionStreamEvent, TransientReasoningStatus, TransientReasoningStatusKind};
pub use execution_models::{
    DedicatedExecutionProfile, ExecutionKnowledgeAccessContext, ExecutionPreparation,
    ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionProviderVisibleAgentMessage,
    ExecutionRunReport, ExecutionSessionGoalContext,
};
pub use extension_generation::ExtensionGeneration;
pub use runtime::RuntimeFoundation;
pub use runtime_stream::{
    stream_runtime_run, stream_runtime_run_with_intervention,
    stream_runtime_run_with_intervention_and_plugins,
    stream_runtime_run_with_intervention_plugins_and_public_ai,
};
pub use runtime_stream_intervention::{RuntimePromptIntervention, RuntimePromptInterventionError};
