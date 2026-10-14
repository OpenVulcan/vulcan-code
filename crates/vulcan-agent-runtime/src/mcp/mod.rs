//! MCP configuration and registry glue for the compose runtime foundation.
//! 面向 compose 运行时基础架构的 MCP 配置与注册表胶水层。

pub mod config;
mod oauth_credentials;
mod plugin;
pub mod registry;
pub mod runtime;

pub use config::{
    McpServerConfig, McpServerOAuthConfig, McpServerSource, McpServerTransportConfig,
    McpTransportKind, runtime_mcp_server_configs,
};
pub use oauth_credentials::{install_mcp_oauth_credential_backend, mcp_oauth_credential_backend};
// Re-exported so hosts drive OAuth through this crate without depending on the transport crate.
// 重新导出，使宿主通过本 crate 驱动 OAuth，而无需依赖传输 crate。
pub use plugin::{
    PluginMcpSecretStore, PreparedPluginMcpConfigs, PreparedPredecessorPluginMcpConfigs,
    PreparedPublishedPluginMcpConfigs, PreparedTargetPluginMcpConfigs,
    candidate_global_mcp_server_configs, candidate_local_mcp_server_configs,
    plugin_conversation_mcp_server_config, plugin_mcp_server_configs,
    plugin_mcp_server_configs_for_descriptors, predecessor_global_mcp_server_configs,
    published_global_mcp_server_configs, target_global_mcp_server_configs,
};
pub use runtime::{
    CandidatePluginMcpObservation, CandidatePluginMcpRuntime, McpRuntimeAccess,
    McpRuntimeDiscoveryReport, McpRuntimeOverview, McpRuntimeOwnedPartitionKey,
    McpRuntimeOwnedStdioPartition, McpRuntimeServerState, McpRuntimeServerStatus,
    McpRuntimeToolAllowlist, McpRuntimeToolBinding, PredecessorPluginMcpHandoffGuard,
    PredecessorPluginMcpRuntime, PreparedPublishedPluginMcpPublication,
    PublishedPluginMcpObservation, PublishedPluginMcpPublicationGuard, PublishedPluginMcpRuntime,
    RetainedPluginMcpReplacement, TargetPluginMcpHandoffGuard, TargetPluginMcpRuntime,
    bootstrap_mcp_runtime_access, bootstrap_mcp_runtime_access_with_published,
    configured_mcp_runtime_access, remove_plugin_mcp_runtime_partition,
    replace_plugin_mcp_runtime_partition, replace_user_mcp_runtime_partition,
    validate_plugin_mcp_runtime_partition,
};
pub use vulcan_mcp_runtime::{
    McpAuthChallenge, McpAuthStatus, McpOAuthCredentialBackend, McpOAuthCredentialRecord,
    McpOAuthMemoryCredentialBackend, McpOAuthSession, McpOAuthStoredCredentials, McpRuntimeError,
    clear_mcp_oauth_credentials,
};
