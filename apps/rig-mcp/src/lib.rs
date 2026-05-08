//! # rig-mcp
//!
//! Model Context Protocol bridge for [`rig-compose`](https://crates.io/crates/rig-compose)
//! tool registries, backed by the official [`rmcp`] SDK.
//!
//! Skills cannot tell an [`McpTool`] apart from a `rig_compose::LocalTool`
//! — both implement the same [`Tool`](rig_compose::tool::Tool) trait.
//! The same registry can be exposed as an MCP server via [`serve_stdio`]
//! and consumed by another process via [`StdioTransport::spawn`], with
//! all spec-level concerns (JSON-RPC framing, capability handshakes,
//! protocol-version negotiation) delegated to `rmcp`.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::panic_in_result_fn,
    )
)]

pub mod stdio;
pub mod transport;

pub use stdio::{StdioTransport, serve_stdio};
pub use transport::{LoopbackTransport, McpTool, McpTransport};
