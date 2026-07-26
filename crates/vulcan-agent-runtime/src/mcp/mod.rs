//! MCP configuration and registry glue for the compose runtime foundation.
//! 面向 compose 运行时基础架构的 MCP 配置与注册表胶水层。

pub mod config;
mod managed_codebase_memory;
mod plugin;
pub mod registry;
pub mod runtime;

pub use config::{
    HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY, McpServerConfig, McpServerOAuthConfig,
    McpServerSource, McpServerTransportConfig, McpTransportKind, runtime_mcp_server_configs,
};
pub use managed_codebase_memory::{
    MANAGED_CODEBASE_MEMORY_CONTRACT, MANAGED_CODEBASE_MEMORY_PRODUCT,
    MANAGED_CODEBASE_MEMORY_READ_ONLY_TOOLS, MANAGED_CODEBASE_MEMORY_RESERVED_ARGUMENTS,
    MANAGED_CODEBASE_MEMORY_SERVER_ID, ManagedCodebaseMemoryAuthorization,
    ManagedCodebaseMemoryExecutionContext, ManagedCodebaseMemoryProjectRoute,
    inject_managed_codebase_memory_context, is_managed_codebase_memory_read_only_tool,
    managed_codebase_memory_public_tool_id, model_visible_managed_codebase_memory_schema,
};
pub use plugin::{
    PluginMcpSecretStore, plugin_mcp_server_configs, plugin_mcp_server_configs_for_descriptors,
};
pub use runtime::{
    McpRuntimeAccess, McpRuntimeDiscoveryReport, McpRuntimeOverview, McpRuntimeServerState,
    McpRuntimeServerStatus, McpRuntimeToolAllowlist, McpRuntimeToolBinding,
    bootstrap_mcp_runtime_access, configured_mcp_runtime_access,
    remove_plugin_mcp_runtime_partition, replace_managed_codebase_memory_mcp_runtime_partition,
    replace_plugin_mcp_runtime_partition, replace_user_mcp_runtime_partition,
    validate_plugin_mcp_runtime_partition,
};
