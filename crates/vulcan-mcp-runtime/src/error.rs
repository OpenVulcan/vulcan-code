//! Shared MCP runtime error type.
//! 共享 MCP 运行时错误类型。

use std::error::Error;
use std::fmt;

/// Host-owned MCP runtime error surfaced across all client transports.
/// 跨所有客户端传输对外暴露的宿主自有 MCP 运行时错误。
#[derive(Debug, Clone)]
pub enum McpRuntimeError {
    /// Invalid caller input.
    /// 调用方输入非法。
    InvalidArgument(String),
    /// Transport or remote-side execution failed.
    /// 传输层或远端执行失败。
    Transport(String),
}

impl McpRuntimeError {
    /// Build one invalid-argument error from a human-readable message.
    /// 根据人类可读消息构建单个非法参数错误。
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }

    /// Build one transport/runtime error from a human-readable message.
    /// 根据人类可读消息构建单个传输/运行时错误。
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport(message.into())
    }
}

impl fmt::Display for McpRuntimeError {
    /// Render the runtime error as one display string.
    /// 将运行时错误渲染为单条展示字符串。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(message) | Self::Transport(message) => {
                write!(formatter, "{}", message)
            }
        }
    }
}

impl Error for McpRuntimeError {}
