//! Vulcan Code MCP endpoint: lets external AI clients drive sessions and run granted media models.
//! Vulcan Code MCP 端点：让外部 AI 客户端驱动会话并运行已授权的媒体模型。
//!
//! The crate owns the protocol, authentication, tool catalog, waiting, and media jobs; the service
//! kernel supplies the operations through [`McpServiceBackend`], and the service host mounts
//! [`McpServer::handle_post`] on `/mcp`.
//! 本 crate 持有协议、认证、工具目录、等待与媒体任务；服务内核通过 [`McpServiceBackend`] 提供操作，
//! 服务宿主把 [`McpServer::handle_post`] 挂载到 `/mcp`。

use std::time::Duration;

mod auth;
mod backend;
mod media_jobs;
mod server;
mod tools;

#[cfg(test)]
mod tests;

pub use auth::{IssuedClientToken, issue_client_token, token_sha256_hex};
pub use backend::*;
pub use server::{McpClientUsage, McpHttpResponse, McpServer};
pub use tools::MCP_MEDIA_CAPABILITIES;

/// MCP protocol revisions this server speaks, newest first; the first is the default answer.
/// 本服务器支持的 MCP 协议修订，最新在前；第一项是默认应答。
pub const MCP_SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

/// Seconds a waiting tool blocks when the caller does not say.
/// 调用方未指定时等待类工具阻塞的秒数。
pub const MCP_DEFAULT_WAIT_SECONDS: u64 = 45;

/// Longest wait one tool call may request.
/// 单次工具调用可请求的最长等待秒数。
pub const MCP_MAX_WAIT_SECONDS: u64 = 600;

/// Interval between live status checks while waiting.
/// 等待期间实时状态检查的间隔。
pub const MCP_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Events or sessions returned when the caller does not say.
/// 调用方未指定时返回的事件或会话数。
pub const MCP_DEFAULT_READ_LIMIT: usize = 50;

/// Most events or sessions one call returns.
/// 单次调用返回的最多事件或会话数。
pub const MCP_MAX_READ_LIMIT: usize = 200;

/// Most non-reply activity entries a wait result lists.
/// 等待结果列出的最多非回复活动条目数。
pub const MCP_WAIT_ACTIVITY_LIMIT: usize = 30;

/// Characters of tool output kept in a transcript preview.
/// 转录预览中保留的工具输出字符数。
pub const MCP_TOOL_OUTPUT_PREVIEW_CHARS: usize = 2_000;

/// Largest generated image returned inline; larger ones are returned by path only.
/// 内联返回的最大生成图片；更大的只返回路径。
pub const MCP_INLINE_IMAGE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Media jobs one client may run at once.
/// 一个客户端可同时运行的媒体任务数。
pub const MCP_MAX_RUNNING_MEDIA_JOBS_PER_CLIENT: usize = 4;

/// Media jobs remembered per client, finished ones evicted oldest first.
/// 每个客户端记住的媒体任务数，已结束的按最早优先淘汰。
pub const MCP_MAX_RETAINED_MEDIA_JOBS_PER_CLIENT: usize = 32;
