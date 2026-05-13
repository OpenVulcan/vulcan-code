//! # vulcan-mcp-runtime
//! 宿主自有的 MCP 运行时。
//!
//! This crate exposes host-owned MCP client transports used by VulcanCode.
//! 本 crate 暴露供 VulcanCode 使用的宿主自有 MCP 客户端传输。

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

pub mod auth;
pub mod error;
pub mod http;
mod http_support;
pub mod launch;
mod rmcp_support;
pub mod sse;
pub mod stdio;
pub mod transport;

pub use auth::{McpAuthChallenge, McpAuthStatus, McpOAuthSession, McpTransportOAuthSpec};
pub use error::McpRuntimeError;
pub use http::HttpTransport;
pub use launch::McpTransportLaunchSpec;
pub use rmcp::model::{GetPromptResult, Prompt, PromptArgument, ReadResourceResult, Resource};
pub use sse::SseTransport;
pub use stdio::StdioTransport;
pub use transport::{McpToolSchema, McpTransport, McpTransportEvent};
