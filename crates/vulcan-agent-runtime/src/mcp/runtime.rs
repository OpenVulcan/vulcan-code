//! Long-lived MCP runtime manager that keeps server connections hot for tool registration.
//! 用于保持服务器长连接并支撑工具注册的长期 MCP 运行时管理器。

mod diagnostics;
mod handle;
mod snapshot;
mod tools;

#[cfg(test)]
mod tests;

use crate::mcp::config::McpServerConfig;
use crate::mcp::registry::{McpServerRegistry, configured_mcp_registry_overview};
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::{allocate_unique_public_tool_name, build_prefixed_public_tool_name};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{RwLock as AsyncRwLock, watch};
use vulcan_mcp_runtime::{
    McpAuthChallenge, McpAuthStatus, McpOAuthSession, McpTransport, McpTransportEvent,
};

use diagnostics::*;
use handle::McpRuntimeHandle;
use snapshot::*;
use tools::*;

pub use diagnostics::{
    McpRuntimeDiscoveryReport, McpRuntimeEventProjection, McpRuntimeOverview,
    McpRuntimeServerDiagnostic, McpRuntimeServerStatus, McpRuntimeToolAllowlist,
};
pub use handle::bootstrap_mcp_runtime_access;
pub use tools::{McpRuntimeAccess, McpRuntimeToolBinding, configured_mcp_runtime_access};
