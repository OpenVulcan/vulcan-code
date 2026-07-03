//! Host tool settings service logic.
//! 宿主工具设置服务逻辑。

use std::{
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use tokio::{process::Command as TokioCommand, time::timeout};
use vulcan_agent_runtime::provider_config::{self, ProviderConfigurationFile};
use vulcan_provider_config::config::{PersistedRtkRouteMode, PersistedRtkToolConfiguration};
use vulcan_runtime_assets::resolve_bundled_rtk_executable_path;
use vulcan_service_api::{
    HostToolsSettingsSummary, NativeToolSummary, RtkGainCommandStats, RtkGainDailyStats,
    RtkGainMonthlyStats, RtkGainStatsResponse, RtkGainStatsSummary, RtkGainWeeklyStats,
    RtkRoutingSettingsSummary, ServiceEventEmitter, WorkbenchSnapshot,
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

/// Loads global RTK gain statistics through the bundled executable.
/// 通过随包可执行文件加载全局 RTK gain 统计。
pub(crate) async fn service_get_rtk_gain_stats() -> Result<RtkGainStatsResponse, String> {
    let generated_at_ms = current_unix_time_ms();
    let Some(rtk_path) = resolve_bundled_rtk_executable_path() else {
        return Ok(RtkGainStatsResponse::unavailable(
            generated_at_ms,
            "bundled RTK executable was not found",
        ));
    };

    let mut command = TokioCommand::new(&rtk_path);
    command
        .args(["gain", "--all", "--format", "json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = match timeout(Duration::from_secs(5), command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Ok(RtkGainStatsResponse::unavailable(
                generated_at_ms,
                format!("failed to run RTK gain statistics: {error}"),
            ));
        }
        Err(_) => {
            return Ok(RtkGainStatsResponse::unavailable(
                generated_at_ms,
                "RTK gain statistics timed out",
            ));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Ok(RtkGainStatsResponse::unavailable(
            generated_at_ms,
            if detail.is_empty() {
                format!("RTK gain exited with status {}", output.status)
            } else {
                format!("RTK gain exited with status {}: {detail}", output.status)
            },
        ));
    }

    let export: RtkGainStatsExport = match serde_json::from_str(&stdout) {
        Ok(export) => export,
        Err(error) => {
            return Ok(RtkGainStatsResponse::unavailable(
                generated_at_ms,
                format!("failed to parse RTK gain statistics JSON: {error}"),
            ));
        }
    };
    Ok(RtkGainStatsResponse {
        available: true,
        generated_at_ms,
        summary: export.summary,
        daily: export.daily,
        weekly: export.weekly,
        monthly: export.monthly,
        by_command: load_rtk_gain_command_stats(&rtk_path).await,
        error: None,
    })
}

/// Builds the WebUI host-tool settings snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI 宿主工具设置快照。
pub(crate) fn host_tools_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> HostToolsSettingsSummary {
    host_tools_settings_with_native_tools(provider_configuration, Vec::new())
}

/// Builds the WebUI host-tool settings snapshot with runtime native tool rows.
/// 使用运行时原生工具行构建 WebUI 宿主工具设置快照。
pub(crate) fn host_tools_settings_with_native_tools(
    provider_configuration: &ProviderConfigurationFile,
    native_tools: Vec<NativeToolSummary>,
) -> HostToolsSettingsSummary {
    HostToolsSettingsSummary {
        rtk: rtk_routing_settings(&provider_configuration.host_tools.rtk),
        native_tools,
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

/// RTK gain JSON payload returned by `rtk gain --all --format json`.
/// `rtk gain --all --format json` 返回的 RTK gain JSON 载荷。
#[derive(Debug, Clone, Deserialize)]
struct RtkGainStatsExport {
    /// Global RTK gain summary.
    /// RTK gain 全局统计摘要。
    #[serde(default)]
    summary: RtkGainStatsSummary,
    /// Daily RTK gain points.
    /// 按天聚合的 RTK gain 数据点。
    #[serde(default)]
    daily: Vec<RtkGainDailyStats>,
    /// Weekly RTK gain points.
    /// 按周聚合的 RTK gain 数据点。
    #[serde(default)]
    weekly: Vec<RtkGainWeeklyStats>,
    /// Monthly RTK gain points.
    /// 按月聚合的 RTK gain 数据点。
    #[serde(default)]
    monthly: Vec<RtkGainMonthlyStats>,
}

/// Loads command-level RTK gain rows from the standard dashboard output.
/// 从标准仪表盘输出加载 RTK gain 命令级明细行。
async fn load_rtk_gain_command_stats(rtk_path: &std::path::Path) -> Vec<RtkGainCommandStats> {
    let mut command = TokioCommand::new(rtk_path);
    command
        .arg("gain")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let output = match timeout(Duration::from_secs(5), command.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_rtk_gain_command_stats(&stdout)
}

/// Parses the By Command section emitted by `rtk gain`.
/// 解析 `rtk gain` 输出的 By Command 区块。
fn parse_rtk_gain_command_stats(stdout: &str) -> Vec<RtkGainCommandStats> {
    let mut in_section = false;
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed == "By Command" {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if trimmed.is_empty() {
            break;
        }
        if trimmed.starts_with('─') || trimmed.starts_with('#') {
            continue;
        }
        if let Some(row) = parse_rtk_gain_command_stats_row(trimmed) {
            rows.push(row);
        }
    }
    rows
}

/// Parses one command-level RTK gain dashboard row.
/// 解析单条 RTK gain 命令级仪表盘行。
fn parse_rtk_gain_command_stats_row(row: &str) -> Option<RtkGainCommandStats> {
    let (_, after_index) = row.split_once(".  ")?;
    let cells: Vec<&str> = after_index.split_whitespace().collect();
    if cells.len() < 6 {
        return None;
    }
    let metric_start = cells.len().checked_sub(5)?;
    let command = cells[..metric_start].join(" ");
    let count = cells[metric_start].parse::<u64>().ok()?;
    let saved_tokens = parse_rtk_compact_count(cells[metric_start + 1])?;
    let avg_savings_pct = cells[metric_start + 2]
        .trim_end_matches('%')
        .parse::<f64>()
        .ok()?;
    let avg_time_ms = parse_rtk_duration_ms(cells[metric_start + 3])?;
    Some(RtkGainCommandStats {
        command,
        count,
        saved_tokens,
        avg_savings_pct,
        avg_time_ms,
    })
}

/// Parses RTK compact count strings such as `3.8K` and `800`.
/// 解析 RTK 的紧凑计数字符串，例如 `3.8K` 与 `800`。
fn parse_rtk_compact_count(value: &str) -> Option<u64> {
    let normalized = value.trim();
    let (number, multiplier) = if let Some(prefix) = normalized.strip_suffix('K') {
        (prefix, 1_000.0)
    } else if let Some(prefix) = normalized.strip_suffix('M') {
        (prefix, 1_000_000.0)
    } else {
        (normalized, 1.0)
    };
    let parsed = number.parse::<f64>().ok()?;
    Some((parsed * multiplier).round() as u64)
}

/// Parses RTK duration strings such as `105ms`, `1.1s`, and `2m3s`.
/// 解析 RTK 耗时字符串，例如 `105ms`、`1.1s` 与 `2m3s`。
fn parse_rtk_duration_ms(value: &str) -> Option<u64> {
    let normalized = value.trim();
    if let Some(prefix) = normalized.strip_suffix("ms") {
        return prefix.parse::<u64>().ok();
    }
    if let Some(prefix) = normalized.strip_suffix('s') {
        if let Some((minutes, seconds)) = prefix.split_once('m') {
            let minutes = minutes.parse::<u64>().ok()?;
            let seconds = seconds.parse::<u64>().ok()?;
            return Some(minutes * 60_000 + seconds * 1_000);
        }
        let seconds = prefix.parse::<f64>().ok()?;
        return Some((seconds * 1_000.0).round() as u64);
    }
    None
}

/// Returns the current Unix time in milliseconds for cache labels.
/// 返回用于缓存标签的当前 Unix 毫秒时间。
fn current_unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies RTK dashboard command rows are parsed into structured WebUI rows.
    /// 验证 RTK 仪表盘命令行会被解析为结构化 WebUI 行。
    #[test]
    fn parses_rtk_gain_command_stats_rows() {
        let stdout = r#"
RTK Token Savings (Global Scope)

By Command
───────────────────────────────────────────────────────────────────────
  #  Command                   Count  Saved    Avg%    Time  Impact
───────────────────────────────────────────────────────────────────────
 1.  rtk find                     10   3.8K   50.2%    20ms  ██████████
 2.  rtk git status                2     13   24.1%   107ms  ░░░░░░░░░░
 3.  rtk long command na...        1    800   80.0%    1.1s  ██░░░░░░░░
───────────────────────────────────────────────────────────────────────
"#;
        let rows = parse_rtk_gain_command_stats(stdout);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].command, "rtk find");
        assert_eq!(rows[0].count, 10);
        assert_eq!(rows[0].saved_tokens, 3_800);
        assert_eq!(rows[0].avg_savings_pct, 50.2);
        assert_eq!(rows[0].avg_time_ms, 20);
        assert_eq!(rows[2].command, "rtk long command na...");
        assert_eq!(rows[2].avg_time_ms, 1_100);
    }
}
