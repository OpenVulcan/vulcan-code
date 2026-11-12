//! Host-native RTK settings and token-saving statistics for the WebUI and TUI pages.
//! 供 WebUI 与 TUI 页面使用的宿主原生 RTK 设置与 Token 节省统计。
//!
//! The `rtk gain` JSON export and by-command table belong to the upstream RTK release locked in
//! `scripts/build-inputs.lock.json` `tools.rtk`; re-read both whenever that release changes.
//! `rtk gain` 的 JSON 导出与按命令表归属于 `scripts/build-inputs.lock.json` `tools.rtk`
//! 锁定的上游 RTK 发行版；该发行版变化时必须重新核对两者。

use std::path::Path;

use serde::Deserialize;
use vulcan_host_tools::rtk::{
    RTK_STATISTICS_TIMEOUT, resolve_rtk_executable, rtk_invocation_environment, run_rtk_capture,
    run_rtk_command_ranking,
};
use vulcan_provider_config::provider_config::{
    DEFAULT_RTK_ENABLED_ROUTES, PersistedRtkConfiguration, RTK_EXTENDED_ROUTES, RtkRouteMode,
};
use vulcan_service_api::{
    RtkCommandStatistics, RtkRouteModeSetting, RtkSettings, RtkSettingsSaveRequest,
    RtkSettingsSnapshot, RtkStatisticsPeriod, RtkStatisticsSnapshot, RtkStatisticsSummary,
};

use crate::SharedServiceBridgeState;
use crate::service_state_access::{
    mutate_provider_configuration, provider_configuration_from_state,
};

/// Reason reported when this host ships no RTK executable.
/// 本机未随包提供 RTK 可执行文件时报告的原因。
const RTK_NOT_BUNDLED: &str = "RTK is not bundled for this host.";

/// Heading that opens the by-command ranking in `rtk gain --text`.
/// `rtk gain --text` 中按命令排名的起始标题。
const BY_COMMAND_HEADING: &str = "By Command";

/// Converts the persisted settings into their wire form.
/// 将持久化设置转换为线上形态。
fn settings_from_config(config: &PersistedRtkConfiguration) -> RtkSettings {
    RtkSettings {
        enabled: config.enabled,
        route_mode: match config.route_mode {
            RtkRouteMode::Disabled => RtkRouteModeSetting::Disabled,
            RtkRouteMode::Selected => RtkRouteModeSetting::Selected,
            RtkRouteMode::All => RtkRouteModeSetting::All,
        },
        enabled_routes: config.enabled_routes.clone(),
        disabled_routes: config.disabled_routes.clone(),
        telemetry_disabled: config.telemetry_disabled,
        git_commit_context_enabled: config.git_commit_context_enabled,
    }
}

/// Converts wire settings into the persisted form.
/// 将线上设置转换为持久化形态。
fn config_from_settings(settings: RtkSettings) -> PersistedRtkConfiguration {
    PersistedRtkConfiguration {
        enabled: settings.enabled,
        route_mode: match settings.route_mode {
            RtkRouteModeSetting::Disabled => RtkRouteMode::Disabled,
            RtkRouteModeSetting::Selected => RtkRouteMode::Selected,
            RtkRouteModeSetting::All => RtkRouteMode::All,
        },
        enabled_routes: settings.enabled_routes,
        disabled_routes: settings.disabled_routes,
        telemetry_disabled: settings.telemetry_disabled,
        git_commit_context_enabled: settings.git_commit_context_enabled,
    }
}

/// Converts one static route catalog into owned strings.
/// 将一个静态路由目录转换为拥有所有权的字符串。
fn route_catalog(routes: &[&str]) -> Vec<String> {
    routes.iter().map(|route| (*route).to_string()).collect()
}

/// Runs one RTK command with no project working directory.
/// 在无项目工作目录的情况下运行一条 RTK 命令。
///
/// # Parameters
/// 参数
/// `executable` is the bundled RTK, `arguments` follow it, and `telemetry_disabled` is the saved setting.
/// `executable` 是随包 RTK，`arguments` 跟在其后，`telemetry_disabled` 是已保存设置。
async fn run_rtk(
    executable: &Path,
    arguments: &[&str],
    telemetry_disabled: bool,
    timeout: std::time::Duration,
) -> Result<String, String> {
    // RTK statistics are global, so the executable's own directory is a stable neutral cwd.
    // RTK 统计是全局的，因此可执行文件所在目录是稳定的中性工作目录。
    let cwd = executable.parent().unwrap_or(executable);
    run_rtk_capture(
        executable,
        arguments,
        cwd,
        &rtk_invocation_environment(telemetry_disabled),
        timeout,
    )
    .await
}

/// Builds the settings snapshot from the persisted settings.
/// 由持久化设置构建设置快照。
fn settings_snapshot(config: &PersistedRtkConfiguration) -> RtkSettingsSnapshot {
    RtkSettingsSnapshot {
        settings: settings_from_config(config),
        curated_routes: route_catalog(DEFAULT_RTK_ENABLED_ROUTES),
        extended_routes: route_catalog(RTK_EXTENDED_ROUTES),
    }
}

/// Returns the saved RTK settings and the offered route catalog.
/// 返回已保存的 RTK 设置与可选路由目录。
pub(crate) fn service_rtk_settings(
    state: SharedServiceBridgeState,
) -> Result<RtkSettingsSnapshot, String> {
    let configuration = provider_configuration_from_state(&state)?;
    Ok(settings_snapshot(&configuration.rtk))
}

/// Saves the complete RTK settings; the next turn applies them.
/// 保存完整 RTK 设置；下一回合生效。
///
/// # Returns
/// 返回值
/// The refreshed snapshot, or an error naming the first invalid route.
/// 刷新后的快照；路由无效时返回指明首个无效路由的错误。
pub(crate) async fn service_save_rtk_settings(
    state: SharedServiceBridgeState,
    request: RtkSettingsSaveRequest,
) -> Result<RtkSettingsSnapshot, String> {
    let config = config_from_settings(request.settings).normalized()?;
    mutate_provider_configuration(
        &state,
        "Failed to save the RTK settings",
        |configuration| {
            configuration.rtk = config;
            Ok(())
        },
        |_, configuration| settings_snapshot(&configuration.rtk),
    )
    .await
}

/// Builds an unavailable statistics snapshot carrying `reason`.
/// 构建携带 `reason` 的不可用统计快照。
fn unavailable_statistics(reason: String) -> RtkStatisticsSnapshot {
    RtkStatisticsSnapshot {
        available: false,
        error: Some(reason),
        summary: RtkStatisticsSummary::default(),
        daily: Vec::new(),
        weekly: Vec::new(),
        monthly: Vec::new(),
        by_command: Vec::new(),
    }
}

/// Loads RTK token-saving statistics from `rtk gain`.
/// 从 `rtk gain` 读取 RTK Token 节省统计。
///
/// # Returns
/// 返回值
/// The statistics, or an unavailable snapshot with the reason; RTK failures are not RPC errors.
/// 统计数据；失败时返回带原因的不可用快照，RTK 失败不作为 RPC 错误。
pub(crate) async fn service_rtk_statistics(
    state: SharedServiceBridgeState,
) -> Result<RtkStatisticsSnapshot, String> {
    let telemetry_disabled = provider_configuration_from_state(&state)?
        .rtk
        .telemetry_disabled;
    let Some(executable) = resolve_rtk_executable() else {
        return Ok(unavailable_statistics(String::from(RTK_NOT_BUNDLED)));
    };
    Ok(load_statistics(&executable, telemetry_disabled).await)
}

/// Runs both `rtk gain` exports and merges them into one snapshot.
/// 运行两种 `rtk gain` 导出并合并为一个快照。
async fn load_statistics(executable: &Path, telemetry_disabled: bool) -> RtkStatisticsSnapshot {
    let export = match run_rtk(
        executable,
        &["gain", "--all", "--format", "json"],
        telemetry_disabled,
        RTK_STATISTICS_TIMEOUT,
    )
    .await
    .and_then(|stdout| parse_gain_export(&stdout))
    {
        Ok(export) => export,
        Err(reason) => return unavailable_statistics(reason),
    };
    // The JSON export has no command rows, so the ranking comes from the text table; a failure
    // there leaves the ranking empty while the totals stay valid.
    // JSON 导出不含命令行，排名来自文本表；文本表失败时排名为空，汇总仍然有效。
    let by_command = run_rtk_command_ranking(executable, telemetry_disabled)
        .await
        .map(|stdout| parse_gain_by_command(&stdout))
        .unwrap_or_default();
    RtkStatisticsSnapshot {
        available: true,
        error: None,
        summary: export.summary.into_summary(),
        daily: export.daily.into_iter().map(GainDay::into_period).collect(),
        weekly: export
            .weekly
            .into_iter()
            .map(GainWeek::into_period)
            .collect(),
        monthly: export
            .monthly
            .into_iter()
            .map(GainMonth::into_period)
            .collect(),
        by_command,
    }
}

/// Clears RTK's statistics history, then returns the now-empty statistics.
/// 清空 RTK 统计历史，然后返回已清空的统计。
///
/// # Returns
/// 返回值
/// The refreshed statistics, or an error when RTK is missing or the reset fails.
/// 刷新后的统计；RTK 缺失或重置失败时返回错误。
pub(crate) async fn service_clear_rtk_statistics(
    state: SharedServiceBridgeState,
) -> Result<RtkStatisticsSnapshot, String> {
    let telemetry_disabled = provider_configuration_from_state(&state)?
        .rtk
        .telemetry_disabled;
    let executable = resolve_rtk_executable().ok_or_else(|| String::from(RTK_NOT_BUNDLED))?;
    run_rtk(
        &executable,
        &["gain", "--reset", "--yes"],
        telemetry_disabled,
        RTK_STATISTICS_TIMEOUT,
    )
    .await
    .map_err(|reason| format!("Failed to clear the RTK statistics: {reason}"))?;
    Ok(load_statistics(&executable, telemetry_disabled).await)
}

/// The fields of `rtk gain --all --format json` the pages show; other fields are ignored.
/// 页面展示的 `rtk gain --all --format json` 字段；其余字段忽略。
#[derive(Debug, Deserialize)]
struct GainExport {
    summary: GainSummary,
    #[serde(default)]
    daily: Vec<GainDay>,
    #[serde(default)]
    weekly: Vec<GainWeek>,
    #[serde(default)]
    monthly: Vec<GainMonth>,
}

/// The `summary` object of the gain export.
/// gain 导出的 `summary` 对象。
#[derive(Debug, Deserialize)]
struct GainSummary {
    total_commands: u64,
    total_input: u64,
    total_output: u64,
    total_saved: i64,
    avg_savings_pct: f64,
    #[serde(default)]
    oversized_excluded: u64,
}

impl GainSummary {
    /// Converts the summary into its wire form.
    /// 将汇总转换为线上形态。
    fn into_summary(self) -> RtkStatisticsSummary {
        RtkStatisticsSummary {
            total_commands: self.total_commands,
            total_input: self.total_input,
            total_output: self.total_output,
            total_saved: self.total_saved,
            avg_savings_pct: self.avg_savings_pct,
            oversized_excluded: self.oversized_excluded,
        }
    }
}

/// One `daily` entry of the gain export.
/// gain 导出的一个 `daily` 条目。
#[derive(Debug, Deserialize)]
struct GainDay {
    date: String,
    commands: u64,
    saved_tokens: i64,
    savings_pct: f64,
}

/// One `weekly` entry of the gain export.
/// gain 导出的一个 `weekly` 条目。
#[derive(Debug, Deserialize)]
struct GainWeek {
    week_start: String,
    week_end: String,
    commands: u64,
    saved_tokens: i64,
    savings_pct: f64,
}

/// One `monthly` entry of the gain export.
/// gain 导出的一个 `monthly` 条目。
#[derive(Debug, Deserialize)]
struct GainMonth {
    month: String,
    commands: u64,
    saved_tokens: i64,
    savings_pct: f64,
}

impl GainDay {
    /// Converts the entry into a wire period.
    /// 将条目转换为线上周期。
    fn into_period(self) -> RtkStatisticsPeriod {
        RtkStatisticsPeriod {
            start: self.date,
            end: None,
            commands: self.commands,
            saved_tokens: self.saved_tokens,
            savings_pct: self.savings_pct,
        }
    }
}

impl GainWeek {
    /// Converts the entry into a wire period.
    /// 将条目转换为线上周期。
    fn into_period(self) -> RtkStatisticsPeriod {
        RtkStatisticsPeriod {
            start: self.week_start,
            end: Some(self.week_end),
            commands: self.commands,
            saved_tokens: self.saved_tokens,
            savings_pct: self.savings_pct,
        }
    }
}

impl GainMonth {
    /// Converts the entry into a wire period.
    /// 将条目转换为线上周期。
    fn into_period(self) -> RtkStatisticsPeriod {
        RtkStatisticsPeriod {
            start: self.month,
            end: None,
            commands: self.commands,
            saved_tokens: self.saved_tokens,
            savings_pct: self.savings_pct,
        }
    }
}

/// Decodes the gain JSON export.
/// 解码 gain JSON 导出。
fn parse_gain_export(stdout: &str) -> Result<GainExport, String> {
    serde_json::from_str(stdout)
        .map_err(|error| format!("rtk gain returned unreadable JSON: {error}"))
}

/// Parses the by-command ranking of `rtk gain --text`.
/// 解析 `rtk gain --text` 的按命令排名。
///
/// Rows read `N.  command  count  saved  pct%  time  [impact]`; the ranking ends at the first blank
/// line after its heading, and rows that do not match are not ranking rows.
/// 行格式为 `N.  命令  次数  节省  百分比%  耗时  [影响条]`；排名在标题后的首个空行结束，
/// 不匹配的行不属于排名。
fn parse_gain_by_command(stdout: &str) -> Vec<RtkCommandStatistics> {
    let mut rows = Vec::new();
    let mut in_section = false;
    for line in stdout.lines() {
        if !in_section {
            in_section = line.trim() == BY_COMMAND_HEADING;
            continue;
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(row) = parse_command_row(line) {
            rows.push(row);
        }
    }
    rows
}

/// Parses one ranking row, or returns `None` for separators and column headings.
/// 解析一行排名；分隔线与列标题返回 `None`。
fn parse_command_row(line: &str) -> Option<RtkCommandStatistics> {
    let (rank, rest) = line.trim_start().split_once('.')?;
    if rank.is_empty() || !rank.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut tokens: Vec<&str> = rest.split_whitespace().collect();
    // RTK 1.1 appends an impact bar made only of block characters.
    // RTK 1.1 追加一个仅由方块字符组成的影响条。
    if tokens.last().is_some_and(|token| {
        token
            .chars()
            .all(|character| matches!(character, '█' | '░'))
    }) {
        tokens.pop();
    }
    let split = tokens.len().checked_sub(4).filter(|split| *split > 0)?;
    let (command, columns) = tokens.split_at(split);
    Some(RtkCommandStatistics {
        command: command.join(" "),
        count: columns[0].parse().ok()?,
        saved_tokens: parse_compact_count(columns[1])?,
        savings_pct: columns[2].strip_suffix('%')?.parse().ok()?,
        avg_time_ms: parse_duration_ms(columns[3])?,
    })
}

/// Expands a compact count such as `206`, `763.2K`, `13.8M` or `-1.1G`.
/// 展开 `206`、`763.2K`、`13.8M` 或 `-1.1G` 这类紧凑计数。
fn parse_compact_count(text: &str) -> Option<i64> {
    let (number, multiplier) = match text.chars().last()? {
        'K' => (&text[..text.len() - 1], 1e3),
        'M' => (&text[..text.len() - 1], 1e6),
        'G' => (&text[..text.len() - 1], 1e9),
        _ => (text, 1.0),
    };
    let value = number.parse::<f64>().ok()? * multiplier;
    value.is_finite().then(|| value.round() as i64)
}

/// Parses a duration such as `48ms`, `8.7s`, `6m5s` or `1h2m` into milliseconds.
/// 将 `48ms`、`8.7s`、`6m5s` 或 `1h2m` 这类时长解析为毫秒。
fn parse_duration_ms(text: &str) -> Option<u64> {
    let mut total = 0.0;
    let mut rest = text;
    while !rest.is_empty() {
        let number_end = rest
            .find(|character: char| !(character.is_ascii_digit() || character == '.'))
            .filter(|end| *end > 0)?;
        let amount: f64 = rest[..number_end].parse().ok()?;
        rest = &rest[number_end..];
        let unit_end = rest
            .find(|character: char| !character.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let scale = match &rest[..unit_end] {
            "ms" => 1.0,
            "s" => 1_000.0,
            "m" => 60_000.0,
            "h" => 3_600_000.0,
            _ => return None,
        };
        total += amount * scale;
        rest = &rest[unit_end..];
    }
    (!text.is_empty()).then(|| total.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ranking rows keep multi-word commands, drop the impact bar, and expand compact cells.
    /// 排名行保留多词命令、去掉影响条并展开紧凑单元格。
    #[test]
    fn gain_ranking_rows_parse_every_column() {
        let stdout = "Total commands:    5\n\nBy Command\n\
            ────────\n  #  Command   Count   Saved  Total%    Time  Impact    \n────────\n \
            1.  rtk rg                     5953   13.8M   56.7%    48ms  ██████████\n \
            2.  rtk go test -p 1 -tim...      3    5.0M  100.0%  13m16s  ████░░░░░░\n\
            10.  rtk git diff        2     206   -3.5%    1.2s\n────────\n\nRecall\n 1.  ignored  1  1  1%  1ms\n";
        let rows = parse_gain_by_command(stdout);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].command, "rtk rg");
        assert_eq!(rows[0].count, 5953);
        assert_eq!(rows[0].saved_tokens, 13_800_000);
        assert_eq!(rows[0].avg_time_ms, 48);
        assert_eq!(rows[1].command, "rtk go test -p 1 -tim...");
        assert_eq!(rows[1].avg_time_ms, 796_000);
        assert_eq!(rows[2].savings_pct, -3.5);
        assert_eq!(rows[2].avg_time_ms, 1_200);
    }

    /// Unknown units and malformed cells are rejected instead of guessed.
    /// 未知单位与格式错误的单元格被拒绝而不是猜测。
    #[test]
    fn gain_cells_reject_unknown_shapes() {
        assert_eq!(parse_duration_ms("6m5s"), Some(365_000));
        assert_eq!(parse_duration_ms("1h2m"), Some(3_720_000));
        for invalid in ["", "5", "5d", "ms", "1.2.3s"] {
            assert_eq!(parse_duration_ms(invalid), None, "`{invalid}`");
        }
        assert_eq!(parse_compact_count("763.2K"), Some(763_200));
        assert_eq!(parse_compact_count("-1.1G"), Some(-1_100_000_000));
        assert_eq!(parse_compact_count("K"), None);
        assert!(parse_command_row(" 1.  rtk ls  x  1  1%  1ms").is_none());
    }

    /// The export keeps RTK's totals and maps weekly ranges to start and end.
    /// 导出保留 RTK 的汇总，并将每周范围映射为起点与终点。
    #[test]
    fn gain_export_maps_periods() {
        let export = parse_gain_export(
            r#"{"summary":{"total_commands":2,"total_input":10,"total_output":4,"total_saved":6,
            "avg_savings_pct":60.0,"total_time_ms":5,"avg_time_ms":2,"oversized_excluded":1},
            "daily":[{"date":"2026-10-20","commands":2,"input_tokens":10,"output_tokens":4,
            "saved_tokens":6,"savings_pct":60.0,"total_time_ms":5,"avg_time_ms":2}],
            "weekly":[{"week_start":"2026-10-19","week_end":"2026-10-25","commands":2,
            "input_tokens":10,"output_tokens":4,"saved_tokens":6,"savings_pct":60.0,
            "total_time_ms":5,"avg_time_ms":2}],"monthly":[]}"#,
        )
        .expect("gain export");
        let summary = export.summary.into_summary();
        assert_eq!(summary.total_saved, 6);
        assert_eq!(summary.oversized_excluded, 1);
        let week = export
            .weekly
            .into_iter()
            .map(GainWeek::into_period)
            .next()
            .expect("one week");
        assert_eq!(week.start, "2026-10-19");
        assert_eq!(week.end.as_deref(), Some("2026-10-25"));
        assert!(parse_gain_export("not json").is_err());
    }

    /// Wire settings and persisted settings convert both ways without loss.
    /// 线上设置与持久化设置双向转换无损。
    #[test]
    fn rtk_settings_round_trip() {
        let config = PersistedRtkConfiguration {
            route_mode: RtkRouteMode::All,
            disabled_routes: vec![String::from("docker")],
            telemetry_disabled: false,
            ..PersistedRtkConfiguration::default()
        };
        assert_eq!(config_from_settings(settings_from_config(&config)), config);
    }
}
