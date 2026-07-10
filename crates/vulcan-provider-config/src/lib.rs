//! Shared provider configuration, catalog, and capability resolution surface.
//! 共享的 Provider 配置、目录与能力解析表面。

pub mod ai_tool_configuration;
mod atomic_file;
pub mod capabilities;
pub mod catalog;
pub mod config;

/// Transitional alias that preserves the long-lived internal `provider_config` module path.
/// 保留长期存在的内部 `provider_config` 模块路径的过渡别名。
pub mod provider_config {
    pub use crate::config::*;
}

/// Transitional alias that preserves the long-lived internal `provider_capabilities` module path.
/// 保留长期存在的内部 `provider_capabilities` 模块路径的过渡别名。
pub mod provider_capabilities {
    pub use crate::capabilities::*;
}

pub use crate::catalog::ai_capability_provider_catalog;
pub use vulcan_runtime_assets as runtime_assets;
