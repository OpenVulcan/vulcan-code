//! Host settings and public AI connection persistence.
//! 宿主设置与公共 AI 连接持久化。

mod atomic_file;
pub mod config;

/// Preserves the internal host-settings module path used by existing callers.
/// 保留现有调用方使用的内部宿主设置模块路径。
pub mod provider_config {
    pub use crate::config::*;
}

pub use vulcan_runtime_assets as runtime_assets;
