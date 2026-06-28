use super::*;

/// Describes host-owned tool settings projected to WebUI clients.
/// 描述投影给 WebUI 客户端的宿主自有工具设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostToolsSettingsSummary {
    /// RTK command routing settings.
    /// RTK 命令路由设置。
    #[serde(default)]
    pub rtk: RtkRoutingSettingsSummary,
}

/// Describes RTK command-routing settings projected to WebUI clients.
/// 描述投影给 WebUI 客户端的 RTK 命令路由设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RtkRoutingSettingsSummary {
    /// Whether RTK routing is enabled when the bundled executable is present.
    /// 当随包 RTK 可执行文件存在时是否启用路由。
    pub enabled: bool,
    /// Current routing mode.
    /// 当前路由模式。
    pub route_mode: String,
    /// Selected RTK route names.
    /// 已选择的 RTK 路由名称。
    pub enabled_routes: Vec<String>,
    /// Explicitly disabled RTK route names.
    /// 显式禁用的 RTK 路由名称。
    pub disabled_routes: Vec<String>,
    /// Whether RTK telemetry is disabled for routed invocations.
    /// 路由调用中是否禁用 RTK 遥测。
    pub telemetry_disabled: bool,
}

impl Default for RtkRoutingSettingsSummary {
    /// Builds the default RTK routing settings summary.
    /// 构建默认 RTK 路由设置摘要。
    fn default() -> Self {
        Self {
            enabled: true,
            route_mode: String::from("selected"),
            enabled_routes: Vec::new(),
            disabled_routes: Vec::new(),
            telemetry_disabled: true,
        }
    }
}
