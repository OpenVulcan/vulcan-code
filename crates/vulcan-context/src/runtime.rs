//! Canonical runtime-event log models shared by persistence and provider adapters.
//! 供持久化与 Provider 适配器共享的规范运行时事件日志模型。

#[path = "runtime/events.rs"]
mod events;
#[path = "runtime/projection.rs"]
mod projection;

pub use events::*;
pub use projection::{
    conversation_state_from_runtime_log_lines, runtime_log_lines_from_conversation_state,
};

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
