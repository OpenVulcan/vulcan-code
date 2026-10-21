// Vendored from openai/codex (codex-rs/code-mode-runtime/src/lib.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
mod cell_actor;
mod runtime;
mod service;
mod session_runtime;
mod v8_init;

pub(crate) type TaskFailureHandler = std::sync::Arc<dyn Fn(String) + Send + Sync>;

pub use service::InProcessCodeModeSession;
pub use v8_init::V8JitMode;
pub use v8_init::initialize_v8;
pub use vulcan_code_mode_protocol::*;
