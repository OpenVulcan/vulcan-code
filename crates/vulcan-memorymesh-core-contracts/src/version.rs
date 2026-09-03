//! Version and capability contracts.
//! 版本与能力契约。

use serde::{Deserialize, Serialize};

/// Defines the stable Rust API version.
/// 定义稳定的 Rust API 版本。
pub const CORE_API_VERSION: u32 = 1;

/// Defines the stable C ABI version.
/// 定义稳定的 C ABI 版本。
pub const CORE_ABI_VERSION: u32 = 29;

/// Defines the resource manifest format version.
/// 定义资源清单格式版本。
pub const RESOURCE_FORMAT_VERSION: u32 = 1;

/// Defines the binary export schema version.
/// 定义二进制导出 Schema 版本。
pub const EXPORT_SCHEMA_VERSION: u32 = 8;

/// Describes one public Core capability set.
/// 描述一组公共 Core 能力。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Reports the Rust API version.
    /// 报告 Rust API 版本。
    pub api_version: u32,
    /// Reports the C ABI version.
    /// 报告 C ABI 版本。
    pub abi_version: u32,
    /// Reports the resource format version.
    /// 报告资源格式版本。
    pub resource_format_version: u32,
    /// Reports the binary export schema version.
    /// 报告二进制导出 Schema 版本。
    pub export_schema_version: u32,
    /// Reports whether step and resume commands are supported.
    /// 报告是否支持 step 与 resume 命令。
    pub command_effect_resume: bool,
    /// Reports whether uncertain storage outcomes are supported.
    /// 报告是否支持存储结果不确定语义。
    pub uncertain_storage_result: bool,
}

impl Default for Capabilities {
    /// Builds the capability set implemented by this release.
    /// 构造当前版本实现的能力集合。
    fn default() -> Self {
        Self {
            api_version: CORE_API_VERSION,
            abi_version: CORE_ABI_VERSION,
            resource_format_version: RESOURCE_FORMAT_VERSION,
            export_schema_version: EXPORT_SCHEMA_VERSION,
            command_effect_resume: true,
            uncertain_storage_result: true,
        }
    }
}
