//! Native host-tool settings projection.
//! 原生宿主工具设置投影。

use vulcan_agent_runtime::provider_config::ProviderConfigurationFile;
use vulcan_service_api::{HostToolsSettingsSummary, NativeToolSummary};

/// Builds the WebUI native-tool settings snapshot from shared configuration.
/// 从共享配置构建 WebUI 原生工具设置快照。
///
/// # Parameters
/// 参数
/// `provider_configuration` is retained as the stable projection boundary.
/// `provider_configuration` 作为稳定投影边界保留。
///
/// # Returns
/// 返回值
/// Returns an empty native-tool collection until runtime rows are supplied.
/// 在运行时行被提供前返回空的原生工具集合。
pub(crate) fn host_tools_settings(
    _provider_configuration: &ProviderConfigurationFile,
) -> HostToolsSettingsSummary {
    HostToolsSettingsSummary::default()
}

/// Builds the WebUI native-tool settings snapshot with runtime rows.
/// 使用运行时行构建 WebUI 原生工具设置快照。
///
/// # Parameters
/// 参数
/// `provider_configuration` preserves the shared projection signature and `native_tools` contains exact runtime descriptors.
/// `provider_configuration` 保持共享投影签名，`native_tools` 包含精确运行时描述符。
///
/// # Returns
/// 返回值
/// Returns the supplied runtime-native tool descriptors.
/// 返回提供的运行时原生工具描述符。
pub(crate) fn host_tools_settings_with_native_tools(
    _provider_configuration: &ProviderConfigurationFile,
    native_tools: Vec<NativeToolSummary>,
) -> HostToolsSettingsSummary {
    HostToolsSettingsSummary { native_tools }
}
