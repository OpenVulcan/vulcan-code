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
    /// Runtime-registered native tool rows available for agent policy editing.
    /// 可用于智能体策略编辑的运行时已注册原生工具行。
    #[serde(default)]
    pub native_tools: Vec<NativeToolSummary>,
}

/// Describes one runtime-registered native host tool.
/// 描述单个运行时已注册原生宿主工具。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NativeToolSummary {
    /// Stable native tool identifier.
    /// 稳定原生工具标识。
    pub id: String,
    /// Human-readable native tool name.
    /// 人类可读原生工具名称。
    pub name: String,
    /// Human-readable native tool description.
    /// 人类可读原生工具描述。
    pub description: String,
    /// Whether this native tool is normally visible to AI agents.
    /// 该原生工具通常是否对 AI 代理可见。
    pub ai_visible: bool,
    /// Whether this native tool may run while Plan mode is active.
    /// 该原生工具是否允许在计划模式激活时运行。
    pub available_in_plan: bool,
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

/// Describes one RTK gain statistics response returned to WebUI clients.
/// 描述返回给 WebUI 客户端的一次 RTK gain 统计响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RtkGainStatsResponse {
    /// Whether the bundled RTK executable was available and produced statistics.
    /// 随包 RTK 可执行文件是否可用并成功产出统计数据。
    pub available: bool,
    /// Unix timestamp in milliseconds when the statistics were generated.
    /// 统计数据生成时的 Unix 毫秒时间戳。
    pub generated_at_ms: u64,
    /// Global RTK gain summary.
    /// RTK gain 全局统计摘要。
    pub summary: RtkGainStatsSummary,
    /// Daily RTK gain points.
    /// 按天聚合的 RTK gain 数据点。
    #[serde(default)]
    pub daily: Vec<RtkGainDailyStats>,
    /// Weekly RTK gain points.
    /// 按周聚合的 RTK gain 数据点。
    #[serde(default)]
    pub weekly: Vec<RtkGainWeeklyStats>,
    /// Monthly RTK gain points.
    /// 按月聚合的 RTK gain 数据点。
    #[serde(default)]
    pub monthly: Vec<RtkGainMonthlyStats>,
    /// Top command-level RTK gain rows from the standard dashboard.
    /// 标准仪表盘中的 RTK gain 命令级 Top 明细行。
    #[serde(default)]
    pub by_command: Vec<RtkGainCommandStats>,
    /// Optional unavailable or parsing error text.
    /// 可选的不可用或解析错误说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RtkGainStatsResponse {
    /// Builds an unavailable RTK gain statistics response.
    /// 构建不可用的 RTK gain 统计响应。
    pub fn unavailable(generated_at_ms: u64, error: impl Into<String>) -> Self {
        Self {
            available: false,
            generated_at_ms,
            summary: RtkGainStatsSummary::default(),
            daily: Vec::new(),
            weekly: Vec::new(),
            monthly: Vec::new(),
            by_command: Vec::new(),
            error: Some(error.into()),
        }
    }
}

/// Describes aggregate RTK token-savings statistics.
/// 描述 RTK Token 节省的聚合统计。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct RtkGainStatsSummary {
    /// Total routed commands recorded by RTK.
    /// RTK 记录的路由命令总数。
    pub total_commands: u64,
    /// Estimated original input tokens before RTK compression.
    /// RTK 压缩前估算的原始输入 Token 数。
    pub total_input: u64,
    /// Estimated output tokens after RTK compression.
    /// RTK 压缩后估算的输出 Token 数。
    pub total_output: u64,
    /// Estimated tokens saved by RTK compression.
    /// RTK 压缩估算节省的 Token 数。
    pub total_saved: u64,
    /// Average token-savings percentage.
    /// 平均 Token 节省百分比。
    pub avg_savings_pct: f64,
    /// Total command execution time in milliseconds.
    /// 命令总执行耗时毫秒数。
    pub total_time_ms: u64,
    /// Average command execution time in milliseconds.
    /// 命令平均执行耗时毫秒数。
    pub avg_time_ms: u64,
}

/// Describes one RTK gain command-level statistics row.
/// 描述单条 RTK gain 命令级统计明细。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RtkGainCommandStats {
    /// Display command recorded by RTK.
    /// RTK 记录的展示命令。
    pub command: String,
    /// Number of command executions.
    /// 命令执行次数。
    pub count: u64,
    /// Estimated tokens saved by this command group.
    /// 此命令组估算节省的 Token 数。
    pub saved_tokens: u64,
    /// Average token-savings percentage for this command group.
    /// 此命令组的平均 Token 节省百分比。
    pub avg_savings_pct: f64,
    /// Average command execution time in milliseconds.
    /// 命令平均执行耗时毫秒数。
    pub avg_time_ms: u64,
}

/// Describes one daily RTK gain statistics point.
/// 描述单个按天聚合的 RTK gain 统计点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct RtkGainDailyStats {
    /// ISO calendar date for this point.
    /// 此统计点对应的 ISO 日历日期。
    pub date: String,
    /// Commands recorded in this period.
    /// 此周期内记录的命令数。
    pub commands: u64,
    /// Estimated original input tokens before RTK compression.
    /// RTK 压缩前估算的原始输入 Token 数。
    pub input_tokens: u64,
    /// Estimated output tokens after RTK compression.
    /// RTK 压缩后估算的输出 Token 数。
    pub output_tokens: u64,
    /// Estimated tokens saved by RTK compression.
    /// RTK 压缩估算节省的 Token 数。
    pub saved_tokens: u64,
    /// Token-savings percentage for this period.
    /// 此周期的 Token 节省百分比。
    pub savings_pct: f64,
    /// Total command execution time in milliseconds.
    /// 命令总执行耗时毫秒数。
    pub total_time_ms: u64,
    /// Average command execution time in milliseconds.
    /// 命令平均执行耗时毫秒数。
    pub avg_time_ms: u64,
}

/// Describes one weekly RTK gain statistics point.
/// 描述单个按周聚合的 RTK gain 统计点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct RtkGainWeeklyStats {
    /// ISO calendar date for the start of this week.
    /// 此周起始日对应的 ISO 日历日期。
    pub week_start: String,
    /// ISO calendar date for the end of this week.
    /// 此周结束日对应的 ISO 日历日期。
    pub week_end: String,
    /// Commands recorded in this period.
    /// 此周期内记录的命令数。
    pub commands: u64,
    /// Estimated original input tokens before RTK compression.
    /// RTK 压缩前估算的原始输入 Token 数。
    pub input_tokens: u64,
    /// Estimated output tokens after RTK compression.
    /// RTK 压缩后估算的输出 Token 数。
    pub output_tokens: u64,
    /// Estimated tokens saved by RTK compression.
    /// RTK 压缩估算节省的 Token 数。
    pub saved_tokens: u64,
    /// Token-savings percentage for this period.
    /// 此周期的 Token 节省百分比。
    pub savings_pct: f64,
    /// Total command execution time in milliseconds.
    /// 命令总执行耗时毫秒数。
    pub total_time_ms: u64,
    /// Average command execution time in milliseconds.
    /// 命令平均执行耗时毫秒数。
    pub avg_time_ms: u64,
}

/// Describes one monthly RTK gain statistics point.
/// 描述单个按月聚合的 RTK gain 统计点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct RtkGainMonthlyStats {
    /// ISO year-month label for this point.
    /// 此统计点对应的 ISO 年月标签。
    pub month: String,
    /// Commands recorded in this period.
    /// 此周期内记录的命令数。
    pub commands: u64,
    /// Estimated original input tokens before RTK compression.
    /// RTK 压缩前估算的原始输入 Token 数。
    pub input_tokens: u64,
    /// Estimated output tokens after RTK compression.
    /// RTK 压缩后估算的输出 Token 数。
    pub output_tokens: u64,
    /// Estimated tokens saved by RTK compression.
    /// RTK 压缩估算节省的 Token 数。
    pub saved_tokens: u64,
    /// Token-savings percentage for this period.
    /// 此周期的 Token 节省百分比。
    pub savings_pct: f64,
    /// Total command execution time in milliseconds.
    /// 命令总执行耗时毫秒数。
    pub total_time_ms: u64,
    /// Average command execution time in milliseconds.
    /// 命令平均执行耗时毫秒数。
    pub avg_time_ms: u64,
}
