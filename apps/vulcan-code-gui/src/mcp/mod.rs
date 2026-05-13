//! MCP configuration and registry glue for the compose runtime foundation.
//! 面向 compose 运行时基础架构的 MCP 配置与注册表胶水层。

pub mod config;
pub mod registry;
pub mod runtime;

pub use runtime::{
    McpRuntimeAccess, McpRuntimeDiscoveryReport, McpRuntimeHandle, McpRuntimeOverview,
    McpRuntimeToolAllowlist, McpRuntimeToolBinding, bootstrap_mcp_runtime,
};
