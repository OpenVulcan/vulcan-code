// Read-only data views of a declarative plugin page, as authored in a `kind: settings` layout and validated by Core.
// 声明式插件页面的只读数据视图，编写于 `kind: settings` 版面并由 Core 校验。
// The authoritative shape is docs/contracts/plugin-settings-layout.schema.json, generated from the Rust types.
// 权威结构为由 Rust 类型生成的 docs/contracts/plugin-settings-layout.schema.json。

import type { PluginSettingsOption } from "./plugin-settings-layout";

// PluginDataFormat is one host value format; absent or null values render as an em dash.
// PluginDataFormat 是一种宿主数值格式；缺失或 null 值显示为破折号。
export type PluginDataFormat =
  | "text" | "code" | "number" | "compact" | "percent" | "ratio" | "duration_ms"
  | "date" | "month" | "timestamp_ms" | "date_unix_sec" | "bar";

// PluginDataWebHints are the presentation hints this WebUI reads.
// PluginDataWebHints 是本 WebUI 读取的呈现提示。
export interface PluginDataWebHints {
  hidden?: boolean;
  variant?: "cards" | "tiles" | "bars" | "line";
  columns?: number;
}

// PluginDataTuiHints are the terminal presentation hints; the WebUI carries them but never reads them.
// PluginDataTuiHints 是终端呈现提示；WebUI 只携带而从不读取。
export interface PluginDataTuiHints {
  hidden?: boolean;
  variant?: "sparkline" | "table";
}

// PluginDataArgument is one action input property chosen from fixed options.
// PluginDataArgument 是一个从固定选项中选择的 Action 输入属性。
export interface PluginDataArgument {
  name: string;
  label_message_key: string;
  options: PluginSettingsOption[];
  value_format?: "plain" | "duration_ms" | "days";
  default: string | number;
}

// PluginDataSource binds a data section to one read action.
// PluginDataSource 将数据区块绑定到一个读取 Action。
export interface PluginDataSource {
  action: string;
  arguments?: PluginDataArgument[];
  refreshable?: boolean;
  show_loaded_at?: boolean;
  available_path?: string;
  error_path?: string;
  unavailable_message_key?: string;
  refresh_topics?: string[];
}

// PluginDataMetric is one key figure: a path value or the ratio of two paths.
// PluginDataMetric 是一个关键数字：路径值或两个路径的比值。
export interface PluginDataMetric {
  label_message_key: string;
  path?: string;
  ratio?: { numerator: string; denominator: string };
  format?: PluginDataFormat;
}

// PluginDataChartPeriod is one selectable chart series.
// PluginDataChartPeriod 是一个可选图表序列。
export interface PluginDataChartPeriod {
  id: string;
  label_message_key: string;
  path: string;
  label_paths: string[];
  label_format?: PluginDataFormat;
}

// PluginDataColumn is one table column relative to a row.
// PluginDataColumn 是相对于单行的一个表格列。
export interface PluginDataColumn {
  path: string;
  label_message_key: string;
  format?: PluginDataFormat;
}

// PluginDataBlock is one block of a data section.
// PluginDataBlock 是数据区块中的一个数据块。
export type PluginDataBlock =
  | {
    type: "metrics"; title_message_key?: string; help_message_key?: string; source_path?: string; empty_message_key?: string;
    items: PluginDataMetric[]; web?: PluginDataWebHints; tui?: PluginDataTuiHints;
  }
  | {
    type: "chart"; title_message_key?: string; value_label_message_key: string; periods: PluginDataChartPeriod[]; value_path: string;
    value_format?: PluginDataFormat; detail?: { path: string; format: PluginDataFormat }; limit?: number; empty_message_key: string;
    web?: PluginDataWebHints; tui?: PluginDataTuiHints;
  }
  | {
    type: "table"; path: string; title_message_key?: string; count_message_key?: string; rank?: boolean; columns: PluginDataColumn[];
    empty_message_key: string; web?: PluginDataWebHints; tui?: PluginDataTuiHints;
  };

// DEFAULT_CHART_LIMIT mirrors the Rust serde default for an omitted chart `limit`.
// DEFAULT_CHART_LIMIT 对应 Rust 端省略图表 `limit` 时的 serde 默认值。
export const DEFAULT_CHART_LIMIT = 14;

// EMPTY_VALUE is shown for absent or null values on every surface.
// EMPTY_VALUE 是所有界面上缺失或 null 值的显示文本。
export const EMPTY_VALUE = "—";

// readPath follows one validated dotted path; any missing step yields undefined.
// readPath 跟随一个已校验的点分路径；任一步缺失时返回 undefined。
export function readPath(value: unknown, path: string): unknown {
  let current = value;
  for (const segment of path.split(".")) {
    if (current === null || typeof current !== "object" || Array.isArray(current)) return undefined;
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

// readNumber returns a finite numeric path value, or null.
// readNumber 返回有限数值路径值，否则返回 null。
export function readNumber(value: unknown, path: string): number | null {
  const found = readPath(value, path);
  return typeof found === "number" && Number.isFinite(found) ? found : null;
}

// readRows returns the object rows of an array path, skipping malformed entries.
// readRows 返回数组路径中的对象行，跳过格式错误的条目。
export function readRows(value: unknown, path: string): Record<string, unknown>[] {
  const found = readPath(value, path);
  return Array.isArray(found) ? found.filter((row): row is Record<string, unknown> => row !== null && typeof row === "object" && !Array.isArray(row)) : [];
}

// isoCalendar parses YYYY-MM-DD or YYYY-MM as a UTC date, rejecting undocumented shapes instead of guessing.
// isoCalendar 将 YYYY-MM-DD 或 YYYY-MM 解析为 UTC 日期，拒绝未声明格式而非猜测。
function isoCalendar(value: string, monthOnly: boolean): Date | null {
  if (!(monthOnly ? /^\d{4}-\d{2}$/ : /^\d{4}-\d{2}-\d{2}$/).test(value)) return null;
  const date = new Date(`${value}${monthOnly ? "-01" : ""}T00:00:00Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, monthOnly ? 7 : 10) === value ? date : null;
}

// formatDuration renders milliseconds as compact milliseconds, seconds, or minutes and seconds.
// formatDuration 将毫秒渲染为紧凑的毫秒、秒或分秒文本。
function formatDuration(value: number, locale: string): string {
  const unit = (amount: number, name: string, digits = 0) =>
    new Intl.NumberFormat(locale, { style: "unit", unit: name, unitDisplay: "short", minimumFractionDigits: digits, maximumFractionDigits: digits }).format(amount);
  if (value < 1000) return unit(Math.round(value), "millisecond");
  if (value < 60_000) return unit(value / 1000, "second", 1);
  return `${unit(Math.floor(value / 60_000), "minute")} ${unit(Math.floor((value % 60_000) / 1000), "second")}`;
}

// formatDataValue renders one value through a host format; `bar` and `ratio` are composed by their blocks.
// formatDataValue 按宿主格式渲染一个值；`bar` 与 `ratio` 由所属数据块组合显示。
export function formatDataValue(value: unknown, format: PluginDataFormat, locale: string): string {
  if (value === undefined || value === null || value === "") return EMPTY_VALUE;
  if (format === "text" || format === "code") return typeof value === "object" ? JSON.stringify(value) : String(value);
  if (format === "date" || format === "month") {
    const date = typeof value === "string" ? isoCalendar(value, format === "month") : null;
    if (date === null) return String(value);
    return new Intl.DateTimeFormat(locale, format === "month"
      ? { year: "numeric", month: "short", timeZone: "UTC" }
      : { month: "numeric", day: "numeric", timeZone: "UTC" }).format(date);
  }
  if (typeof value !== "number" || !Number.isFinite(value)) return String(value);
  switch (format) {
    case "compact": return new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 }).format(value);
    case "percent": return new Intl.NumberFormat(locale, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value / 100);
    case "ratio": return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value)}×`;
    case "duration_ms": return formatDuration(value, locale);
    case "timestamp_ms":
      return value > 0 ? new Intl.DateTimeFormat(locale, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }).format(new Date(value)) : EMPTY_VALUE;
    case "date_unix_sec":
      return new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric" }).format(new Date(value * 1000));
    default: return new Intl.NumberFormat(locale).format(value);
  }
}

// metricValue resolves one metric, dividing ratio metrics by a denominator of at least one.
// metricValue 解析一个指标；比值指标的分母至少按一计算。
export function metricValue(source: unknown, metric: PluginDataMetric, locale: string): string {
  if (metric.ratio) {
    const numerator = readNumber(source, metric.ratio.numerator);
    const denominator = readNumber(source, metric.ratio.denominator);
    return numerator === null ? EMPTY_VALUE : formatDataValue(numerator / Math.max(1, denominator ?? 0), "ratio", locale);
  }
  return formatDataValue(readPath(source, metric.path ?? ""), metric.format ?? "number", locale);
}

// interpolateCount replaces the `{{count}}` placeholder Core requires in table count messages.
// interpolateCount 替换 Core 要求表格计数文案包含的 `{{count}}` 占位符。
export function interpolateCount(template: string, count: string): string {
  return template.replaceAll("{{count}}", count);
}
