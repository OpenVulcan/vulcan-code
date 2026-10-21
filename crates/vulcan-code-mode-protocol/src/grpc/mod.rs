// Vendored from openai/codex (codex-rs/code-mode-protocol/src/grpc/mod.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
#![allow(clippy::double_must_use)] // Emitted by async_trait in the generated tonic service.

// VULCAN MODIFICATION: upstream also selected a Bazel-generated module under `cfg(codex_bazel)`;
// Vulcan builds with Cargo only. The proto package name stays `codex.code_mode.v1`.
tonic::include_proto!("codex.code_mode.v1");

pub const MAX_IDENTIFIER_BYTES: usize = 256;

/// Decimal nanoseconds a completed result was buffered before a wait or terminate retrieved it.
/// Zero means immediate delivery. Hosts omit this metadata if no completed result was retrieved.
pub const RESULT_BUFFERED_FOR_NS_METADATA: &str = "codex-code-mode-result-buffered-for-ns";
