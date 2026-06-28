use super::*;

/// Persisted host-owned tool configuration.
/// 宿主自有工具的持久化配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedHostToolsConfiguration {
    /// RTK command-routing configuration.
    /// RTK 命令路由配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedRtkToolConfiguration::is_default"
    )]
    pub rtk: PersistedRtkToolConfiguration,
}

impl Default for PersistedHostToolsConfiguration {
    /// Builds the default host-tool configuration.
    /// 构建默认宿主工具配置。
    fn default() -> Self {
        Self {
            rtk: PersistedRtkToolConfiguration::default(),
        }
    }
}

impl PersistedHostToolsConfiguration {
    /// Returns whether every host-tool setting is still at its default value.
    /// 返回全部宿主工具设置是否仍为默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Persisted RTK routing mode.
/// 持久化 RTK 路由模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersistedRtkRouteMode {
    /// RTK routing is disabled.
    /// 禁用 RTK 路由。
    Disabled,
    /// Only selected stable RTK routes are enabled.
    /// 仅启用已选择的稳定 RTK 路由。
    Selected,
    /// Every RTK rewrite route is allowed unless explicitly denied.
    /// 允许全部 RTK rewrite 路由，除非被显式拒绝。
    All,
}

impl Default for PersistedRtkRouteMode {
    /// Builds the default RTK route mode.
    /// 构建默认 RTK 路由模式。
    fn default() -> Self {
        Self::Selected
    }
}

/// Persisted RTK host-tool configuration.
/// 持久化 RTK 宿主工具配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedRtkToolConfiguration {
    /// Whether RTK routing is enabled when the bundled executable is present.
    /// 当随包可执行文件存在时是否启用 RTK 路由。
    #[serde(default = "default_rtk_enabled")]
    pub enabled: bool,
    /// Routing mode used after `rtk rewrite` returns one candidate command.
    /// `rtk rewrite` 返回候选命令后使用的路由模式。
    #[serde(default, skip_serializing_if = "PersistedRtkRouteMode::is_selected")]
    pub route_mode: PersistedRtkRouteMode,
    /// Explicitly enabled RTK route names used by selected mode.
    /// 精选模式下显式启用的 RTK 路由名称集合。
    #[serde(
        default = "default_rtk_enabled_routes",
        skip_serializing_if = "is_default_rtk_enabled_routes"
    )]
    pub enabled_routes: BTreeSet<String>,
    /// Explicitly disabled RTK route names applied after allow-mode resolution.
    /// 在允许模式解析后应用的显式禁用 RTK 路由名称集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_routes: BTreeSet<String>,
    /// Whether RTK telemetry should be disabled for host-routed invocations.
    /// 宿主路由调用中是否禁用 RTK 遥测。
    #[serde(default = "default_rtk_telemetry_disabled")]
    pub telemetry_disabled: bool,
}

impl Default for PersistedRtkToolConfiguration {
    /// Builds the default RTK host-tool configuration.
    /// 构建默认 RTK 宿主工具配置。
    fn default() -> Self {
        Self {
            enabled: default_rtk_enabled(),
            route_mode: PersistedRtkRouteMode::default(),
            enabled_routes: default_rtk_enabled_routes(),
            disabled_routes: BTreeSet::new(),
            telemetry_disabled: default_rtk_telemetry_disabled(),
        }
    }
}

impl PersistedRtkToolConfiguration {
    /// Returns whether the RTK host-tool configuration is still at its default value.
    /// 返回 RTK 宿主工具配置是否仍为默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl PersistedRtkRouteMode {
    /// Returns whether this route mode is the selected-route default.
    /// 返回该路由模式是否为精选路由默认值。
    pub fn is_selected(&self) -> bool {
        matches!(self, Self::Selected)
    }
}

/// Returns the default RTK enabled state.
/// 返回默认 RTK 启用状态。
fn default_rtk_enabled() -> bool {
    true
}

/// Returns the default RTK telemetry-disabled state.
/// 返回默认 RTK 遥测禁用状态。
fn default_rtk_telemetry_disabled() -> bool {
    true
}

/// Returns the default selected RTK routes.
/// 返回默认精选 RTK 路由集合。
fn default_rtk_enabled_routes() -> BTreeSet<String> {
    [
        "cargo", "find", "git", "go", "grep", "jest", "ls", "mypy", "npm", "npx", "pnpm", "pytest",
        "read", "ruff", "tree", "tsc", "vitest", "wc",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Returns whether one route set matches the default selected RTK routes.
/// 返回某个路由集合是否匹配默认精选 RTK 路由。
fn is_default_rtk_enabled_routes(routes: &BTreeSet<String>) -> bool {
    routes == &default_rtk_enabled_routes()
}
