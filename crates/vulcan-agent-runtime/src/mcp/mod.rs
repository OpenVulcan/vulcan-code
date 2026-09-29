//! MCP configuration and registry glue for the compose runtime foundation.
//! 面向 compose 运行时基础架构的 MCP 配置与注册表胶水层。

pub mod config;
mod plugin;
pub mod registry;
pub mod runtime;

pub use config::{
    McpServerConfig, McpServerOAuthConfig, McpServerSource, McpServerTransportConfig,
    McpTransportKind, runtime_mcp_server_configs,
};
pub use plugin::{
    PluginMcpSecretStore, PreparedPluginMcpConfigs, candidate_global_mcp_server_configs,
    candidate_local_mcp_server_configs, plugin_conversation_mcp_server_config,
    plugin_mcp_server_configs, plugin_mcp_server_configs_for_descriptors,
};
pub use runtime::{
    CandidatePluginMcpObservation, CandidatePluginMcpRuntime, McpRuntimeAccess,
    McpRuntimeDiscoveryReport, McpRuntimeOverview, McpRuntimeOwnedPartitionKey,
    McpRuntimeOwnedStdioPartition, McpRuntimeServerState, McpRuntimeServerStatus,
    McpRuntimeToolAllowlist, McpRuntimeToolBinding, bootstrap_mcp_runtime_access,
    configured_mcp_runtime_access, remove_plugin_mcp_runtime_partition,
    replace_plugin_mcp_runtime_partition, replace_user_mcp_runtime_partition,
    validate_plugin_mcp_runtime_partition,
};
