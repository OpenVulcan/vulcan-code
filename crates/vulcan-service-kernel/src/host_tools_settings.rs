//! Host tool settings service logic.
//! 宿主工具设置服务逻辑。

use serde::Deserialize;
use vulcan_agent_runtime::provider_config::{self, ProviderConfigurationFile};
use vulcan_provider_config::config::{PersistedRtkRouteMode, PersistedRtkToolConfiguration};
use vulcan_service_api::{
    HostToolsSettingsSummary, RtkRoutingSettingsSummary, ServiceEventEmitter, WorkbenchSnapshot,
};

use crate::{SharedServiceBridgeState, emit_workbench_event, refresh_settings_projection_locked};

/// RTK routing settings save request sent by the WebUI settings page.
/// WebUI 设置页发送的 RTK 路由设置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RtkRoutingSettingsSaveRequest {
    /// Whether RTK routing should be enabled when the bundled executable is present.
    /// 当随包 RTK 可执行文件存在时是否启用路由。
    enabled: bool,
    /// Route mode requested by the WebUI.
    /// WebUI 请求的路由模式。
    route_mode: String,
    /// Selected RTK route names.
    /// 已选择的 RTK 路由名称。
    #[serde(default)]
    enabled_routes: Vec<String>,
    /// Explicitly disabled RTK route names.
    /// 显式禁用的 RTK 路由名称。
    #[serde(default)]
    disabled_routes: Vec<String>,
    /// Whether RTK telemetry should be disabled for routed invocations.
    /// 路由调用中是否禁用 RTK 遥测。
    #[serde(default = "default_rtk_telemetry_disabled")]
    telemetry_disabled: bool,
}

/// Saves RTK routing settings from the WebUI settings page.
/// 从 WebUI 设置页保存 RTK 路由设置。
pub(crate) async fn service_save_rtk_routing_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: RtkRoutingSettingsSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let route_mode = persisted_rtk_route_mode(&request.route_mode)?;
    let enabled_routes = normalize_rtk_route_names(request.enabled_routes)?;
    let disabled_routes = normalize_rtk_route_names(request.disabled_routes)?;
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.host_tools.rtk = PersistedRtkToolConfiguration {
            enabled: request.enabled,
            route_mode,
            enabled_routes,
            disabled_routes,
            telemetry_disabled: request.telemetry_disabled,
        };
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存 RTK 路由设置失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        refresh_settings_projection_locked(&mut guard, &next_configuration, None);
        guard.workbench.clone()
    };
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Builds the WebUI host-tool settings snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI 宿主工具设置快照。
pub(crate) fn host_tools_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> HostToolsSettingsSummary {
    HostToolsSettingsSummary {
        rtk: rtk_routing_settings(&provider_configuration.host_tools.rtk),
    }
}

/// Builds the WebUI RTK routing settings snapshot from persisted configuration.
/// 从持久化配置构建 WebUI RTK 路由设置快照。
fn rtk_routing_settings(
    configuration: &PersistedRtkToolConfiguration,
) -> RtkRoutingSettingsSummary {
    RtkRoutingSettingsSummary {
        enabled: configuration.enabled,
        route_mode: api_rtk_route_mode(configuration.route_mode),
        enabled_routes: configuration.enabled_routes.iter().cloned().collect(),
        disabled_routes: configuration.disabled_routes.iter().cloned().collect(),
        telemetry_disabled: configuration.telemetry_disabled,
    }
}

/// Converts a persisted RTK route mode into the WebUI API shape.
/// 将持久化 RTK 路由模式转换为 WebUI API 形状。
fn api_rtk_route_mode(mode: PersistedRtkRouteMode) -> String {
    match mode {
        PersistedRtkRouteMode::Disabled => String::from("disabled"),
        PersistedRtkRouteMode::Selected => String::from("selected"),
        PersistedRtkRouteMode::All => String::from("all"),
    }
}

/// Converts a WebUI route mode string into the persisted RTK route mode.
/// 将 WebUI 路由模式字符串转换为持久化 RTK 路由模式。
fn persisted_rtk_route_mode(mode: &str) -> Result<PersistedRtkRouteMode, String> {
    match mode {
        "disabled" => Ok(PersistedRtkRouteMode::Disabled),
        "selected" => Ok(PersistedRtkRouteMode::Selected),
        "all" => Ok(PersistedRtkRouteMode::All),
        other => Err(format!("未知 RTK 路由模式：{other}")),
    }
}

/// Normalizes and validates RTK route names before persistence.
/// 持久化前规范化并校验 RTK 路由名称。
fn normalize_rtk_route_names(
    routes: Vec<String>,
) -> Result<std::collections::BTreeSet<String>, String> {
    let mut normalized = std::collections::BTreeSet::new();
    for route in routes {
        let route = route.trim().to_ascii_lowercase();
        if route.is_empty() {
            continue;
        }
        if !route
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '_' || value == '-')
        {
            return Err(format!(
                "RTK 路由名称 `{route}` 只能包含字母、数字、下划线和连字符。"
            ));
        }
        normalized.insert(route);
    }
    Ok(normalized)
}

/// Returns the default RTK telemetry-disabled state for save requests.
/// 返回保存请求默认使用的 RTK 遥测禁用状态。
fn default_rtk_telemetry_disabled() -> bool {
    true
}
