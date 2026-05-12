//! Runtime foundation for provider transport, MCP runtime integration, and host-owned tool orchestration.
//! 面向 Provider 传输、MCP 运行时集成与宿主自有工具编排的运行时基础模块。

pub mod config;
pub mod events;
pub mod execution_models;
pub mod execution_preparation;
pub mod executor;
pub mod mcp_runtime_projection;
pub mod provider;
pub mod provider_prompt_loader;
pub mod provider_protocol_bridge;
pub mod provider_request_bridge;
pub mod provider_request_snapshot;
pub mod provider_runtime_context;
pub mod provider_runtime_dispatch;
pub mod provider_runtime_inputs;
pub mod provider_stream;
pub mod provider_stream_projection;
pub mod provider_transport;
pub mod provider_transport_anthropic;
pub mod provider_transport_common;
pub mod provider_transport_openai;
pub mod provider_transport_openai_responses;
pub mod provider_transport_openrouter;
pub mod provider_transport_request;
pub mod provider_transport_router;
pub mod runtime;
pub mod runtime_stream;
pub mod runtime_stream_delta;
pub mod runtime_stream_driver;
pub mod runtime_stream_finalize;
pub mod runtime_stream_setup;
pub mod tool_runtime_bridge;

pub use config::{RuntimeSkillKind, ensure_runtime_defaults};
pub use events::ExecutionStreamEvent;
pub use execution_models::{ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport};
pub use runtime::RuntimeFoundation;
pub use runtime_stream::stream_runtime_run;
