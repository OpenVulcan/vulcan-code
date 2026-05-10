//! Runtime foundation for rig-core, rig-compose, and rig-mcp integration.
//! 面向 rig-core、rig-compose 与 rig-mcp 集成的运行时基础模块。

pub mod config;
pub mod events;
pub mod executor;
pub mod provider;
pub mod rig_tools;
pub mod runtime;

pub use config::{RuntimeSkillKind, ensure_runtime_defaults};
pub use events::ExecutionStreamEvent;
pub use executor::{
    ExecutionContextMessage, ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
    stream_runtime_run,
};
pub use runtime::RuntimeFoundation;
