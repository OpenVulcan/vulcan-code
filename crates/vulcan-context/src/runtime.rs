//! Canonical runtime-event log models shared by persistence and provider adapters.
//! 供持久化与 Provider 适配器共享的规范运行时事件日志模型。

#[path = "runtime/codec.rs"]
mod codec;
#[path = "runtime/events.rs"]
mod events;
#[path = "runtime/identity.rs"]
mod identity;
#[path = "runtime/projection.rs"]
mod projection;
#[path = "runtime/projection_error.rs"]
mod projection_error;
#[path = "runtime/replay.rs"]
mod replay;
#[path = "runtime/replay_error.rs"]
mod replay_error;

pub use codec::{RuntimeLogEventLineDecodeError, decode_runtime_log_event_line_json};
pub use events::*;
pub use projection::runtime_log_lines_from_conversation_state;
pub use projection_error::RuntimeLogProjectionError;
pub use replay::conversation_state_from_runtime_log_lines;
pub use replay_error::RuntimeLogReplayError;

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
