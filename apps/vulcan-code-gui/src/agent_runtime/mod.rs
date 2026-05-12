//! Runtime foundation for provider transport, MCP runtime integration, and host-owned tool orchestration.
//! 面向 Provider 传输、MCP 运行时集成与宿主自有工具编排的运行时基础模块。

pub mod config;
pub mod events;
pub mod executor;
pub mod provider;
pub mod provider_protocol_bridge;
pub mod provider_request_bridge;
pub mod provider_transport;
pub mod runtime;
pub mod tool_runtime_bridge;

pub use config::{RuntimeSkillKind, ensure_runtime_defaults};
pub use events::ExecutionStreamEvent;
pub use executor::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport, stream_runtime_run,
};
pub use runtime::RuntimeFoundation;
