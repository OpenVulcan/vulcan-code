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
use crate::tooling::{allocate_unique_public_tool_name, build_prefixed_public_tool_name};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, RwLock};
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
    McpRuntimeServerDiagnostic, McpRuntimeServerInspection, McpRuntimeServerState,
    McpRuntimeServerStatus, McpRuntimeToolAllowlist, McpRuntimeToolInspection,
};
pub use handle::{
    CandidatePluginMcpObservation, CandidatePluginMcpRuntime, LegacyPluginMcpReplacement,
    PredecessorPluginMcpHandoffGuard, PredecessorPluginMcpRuntime,
    PreparedPublishedPluginMcpPublication, PublishedPluginMcpObservation,
    PublishedPluginMcpPublicationGuard, PublishedPluginMcpRuntime, TargetPluginMcpHandoffGuard,
    TargetPluginMcpRuntime, bootstrap_mcp_runtime_access,
    bootstrap_mcp_runtime_access_with_published, remove_plugin_mcp_runtime_partition,
    replace_plugin_mcp_runtime_partition, replace_user_mcp_runtime_partition,
    validate_plugin_mcp_runtime_partition,
};
pub use tools::{
    McpRuntimeAccess, McpRuntimeOwnedPartitionKey, McpRuntimeOwnedStdioPartition,
    McpRuntimeToolBinding, configured_mcp_runtime_access,
};
