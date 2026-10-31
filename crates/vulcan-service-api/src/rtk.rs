use super::*;

/// Which RTK routes may replace a model-issued shell command.
/// 哪些 RTK 路由可以替换模型发出的 shell 命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtkRouteModeSetting {
    /// No rewrite is applied.
    /// 不应用任何改写。
    Disabled,
    /// Only routes in `enabledRoutes` apply.
    /// 只应用 `enabledRoutes` 中的路由。
    Selected,
    /// Every route except those in `disabledRoutes` applies.
    /// 应用除 `disabledRoutes` 之外的全部路由。
    All,
}

/// Persisted RTK settings exchanged with the WebUI and TUI settings pages.
/// 与 WebUI 和 TUI 设置页交换的持久化 RTK 设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkSettings {
    /// Whether model-issued shell commands are routed through RTK.
    /// 是否将模型发出的 shell 命令经 RTK 路由。
    pub enabled: bool,
    /// Which routes may apply.
    /// 哪些路由可以生效。
    pub route_mode: RtkRouteModeSetting,
    /// Allowlist used in `selected` mode.
    /// `selected` 模式使用的允许列表。
    pub enabled_routes: Vec<String>,
    /// Denylist used in `all` mode.
    /// `all` 模式使用的拒绝列表。
    pub disabled_routes: Vec<String>,
    /// Whether RTK telemetry is disabled for host-launched processes.
    /// 是否为宿主启动的进程关闭 RTK 遥测。
    pub telemetry_disabled: bool,
    /// Whether Git commit-summary context is compressed through RTK.
    /// 是否经 RTK 压缩 Git 提交总结上下文。
    pub git_commit_context_enabled: bool,
}

/// RTK settings page snapshot: the saved settings and the offered route catalog.
/// RTK 设置页快照：已保存设置与可选路由目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkSettingsSnapshot {
    /// Saved settings.
    /// 已保存设置。
    pub settings: RtkSettings,
    /// Routes shown in the curated group; also the `selected`-mode default.
    /// 精选分组中显示的路由；同时也是 `selected` 模式的默认值。
    pub curated_routes: Vec<String>,
    /// Routes shown in the extended group.
    /// 扩展分组中显示的路由。
    pub extended_routes: Vec<String>,
}

/// Replaces the persisted RTK settings.
/// 替换持久化 RTK 设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkSettingsSaveRequest {
    /// Complete settings to save.
    /// 要保存的完整设置。
    pub settings: RtkSettings,
}

/// Token-saving totals reported by `rtk gain`.
/// `rtk gain` 报告的 Token 节省汇总。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkStatisticsSummary {
    /// Commands included in the totals.
    /// 计入汇总的命令数。
    pub total_commands: u64,
    /// Estimated tokens of raw output.
    /// 原始输出的估算 Token 数。
    pub total_input: u64,
    /// Estimated tokens of compressed output.
    /// 压缩后输出的估算 Token 数。
    pub total_output: u64,
    /// Estimated tokens saved; negative when compression grew the output.
    /// 估算节省的 Token 数；压缩使输出变大时为负。
    pub total_saved: i64,
    /// Overall savings percentage.
    /// 总体节省百分比。
    pub avg_savings_pct: f64,
    /// Oversized outputs left out of the totals.
    /// 未计入汇总的超大输出数。
    pub oversized_excluded: u64,
}

/// One day, week, or month of RTK savings.
/// RTK 节省的一天、一周或一个月。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkStatisticsPeriod {
    /// Period start as `YYYY-MM-DD`, or `YYYY-MM` for a month.
    /// 周期起点：`YYYY-MM-DD`，月份为 `YYYY-MM`。
    pub start: String,
    /// Inclusive period end as `YYYY-MM-DD`; absent for days and months.
    /// 含当日的周期终点 `YYYY-MM-DD`；天与月份时缺省。
    pub end: Option<String>,
    /// Commands in the period.
    /// 周期内命令数。
    pub commands: u64,
    /// Estimated tokens saved in the period; may be negative.
    /// 周期内估算节省的 Token 数；可能为负。
    pub saved_tokens: i64,
    /// Savings percentage in the period.
    /// 周期内节省百分比。
    pub savings_pct: f64,
}

/// One row of the `rtk gain` by-command ranking.
/// `rtk gain` 按命令排名中的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkCommandStatistics {
    /// Command text as RTK prints it, possibly truncated by RTK.
    /// RTK 打印的命令文本，可能已被 RTK 截断。
    pub command: String,
    /// Times the command ran.
    /// 命令运行次数。
    pub count: u64,
    /// Estimated tokens saved; may be negative.
    /// 估算节省的 Token 数；可能为负。
    pub saved_tokens: i64,
    /// Weighted savings percentage; may be negative.
    /// 加权节省百分比；可能为负。
    pub savings_pct: f64,
    /// Average run time in milliseconds.
    /// 平均运行时长（毫秒）。
    pub avg_time_ms: u64,
}

/// RTK statistics page snapshot; an unavailable result carries the reason instead of data.
/// RTK 统计页快照；不可用结果携带原因而非数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RtkStatisticsSnapshot {
    /// Whether RTK returned statistics.
    /// RTK 是否返回了统计数据。
    pub available: bool,
    /// English reason when statistics are unavailable.
    /// 统计不可用时的英文原因。
    pub error: Option<String>,
    /// Overall totals.
    /// 总体汇总。
    pub summary: RtkStatisticsSummary,
    /// Daily periods, oldest first.
    /// 按天周期，最早的在前。
    pub daily: Vec<RtkStatisticsPeriod>,
    /// Weekly periods, oldest first.
    /// 按周周期，最早的在前。
    pub weekly: Vec<RtkStatisticsPeriod>,
    /// Monthly periods, oldest first.
    /// 按月周期，最早的在前。
    pub monthly: Vec<RtkStatisticsPeriod>,
    /// Top commands as ranked by RTK.
    /// RTK 排名的头部命令。
    pub by_command: Vec<RtkCommandStatistics>,
}
