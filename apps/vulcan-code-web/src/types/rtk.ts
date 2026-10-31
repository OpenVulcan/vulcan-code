// RtkRouteMode selects which RTK routes may replace a model-issued shell command.
// RtkRouteMode 选择哪些 RTK 路由可以替换模型发出的 shell 命令。
export type RtkRouteMode = "disabled" | "selected" | "all";

// RtkSettings is the persisted host-native RTK configuration.
// RtkSettings 是持久化的宿主原生 RTK 配置。
export interface RtkSettings {
  // Whether model-issued shell commands are routed through RTK.
  // 是否将模型发出的 shell 命令经 RTK 路由。
  enabled: boolean;
  // Which routes may apply.
  // 哪些路由可以生效。
  routeMode: RtkRouteMode;
  // Allowlist used in `selected` mode.
  // `selected` 模式使用的允许列表。
  enabledRoutes: string[];
  // Denylist used in `all` mode.
  // `all` 模式使用的拒绝列表。
  disabledRoutes: string[];
  // Whether RTK telemetry is disabled for host-launched processes.
  // 是否为宿主启动的进程关闭 RTK 遥测。
  telemetryDisabled: boolean;
  // Whether Git commit-summary context is compressed through RTK.
  // 是否经 RTK 压缩 Git 提交总结上下文。
  gitCommitContextEnabled: boolean;
}

// RtkSettingsSnapshot is the saved settings with the offered route catalog.
// RtkSettingsSnapshot 是已保存设置及可选路由目录。
export interface RtkSettingsSnapshot {
  settings: RtkSettings;
  // Curated routes; also the `selected`-mode default.
  // 精选路由；同时也是 `selected` 模式的默认值。
  curatedRoutes: string[];
  // Further routes offered for selection.
  // 其余可选路由。
  extendedRoutes: string[];
}

// RtkStatisticsSummary is the `rtk gain` totals.
// RtkStatisticsSummary 是 `rtk gain` 的汇总。
export interface RtkStatisticsSummary {
  totalCommands: number;
  totalInput: number;
  totalOutput: number;
  // Estimated tokens saved; negative when compression grew the output.
  // 估算节省的 Token 数；压缩使输出变大时为负。
  totalSaved: number;
  avgSavingsPct: number;
  oversizedExcluded: number;
}

// RtkStatisticsPeriod is one day, week, or month of savings.
// RtkStatisticsPeriod 是节省统计的一天、一周或一个月。
export interface RtkStatisticsPeriod {
  // `YYYY-MM-DD`, or `YYYY-MM` for a month.
  // `YYYY-MM-DD`；月份为 `YYYY-MM`。
  start: string;
  // Inclusive week end; null for days and months.
  // 含当日的周终点；天与月份时为 null。
  end: string | null;
  commands: number;
  savedTokens: number;
  savingsPct: number;
}

// RtkCommandStatistics is one row of the by-command ranking.
// RtkCommandStatistics 是按命令排名中的一行。
export interface RtkCommandStatistics {
  command: string;
  count: number;
  savedTokens: number;
  savingsPct: number;
  avgTimeMs: number;
}

// RtkStatisticsSnapshot carries the statistics, or the reason they are unavailable.
// RtkStatisticsSnapshot 携带统计数据，或其不可用的原因。
export interface RtkStatisticsSnapshot {
  available: boolean;
  // English diagnostic when unavailable.
  // 不可用时的英文诊断。
  error: string | null;
  summary: RtkStatisticsSummary;
  daily: RtkStatisticsPeriod[];
  weekly: RtkStatisticsPeriod[];
  monthly: RtkStatisticsPeriod[];
  byCommand: RtkCommandStatistics[];
}
