// TOKEN_COUNT_UNITS lists the compact token suffixes from smallest to largest; values below the first unit stay exact.
// TOKEN_COUNT_UNITS 按从小到大列出紧凑 Token 单位；低于首个单位的数值保持精确显示。
const TOKEN_COUNT_UNITS: ReadonlyArray<readonly [number, string]> = [
  [1_000, "K"],
  [1_000_000, "M"],
  [1_000_000_000, "B"],
];

// formatTokenCount renders token statistics: exact below 1000, otherwise one decimal with K, M, or B.
// formatTokenCount 渲染 Token 统计：1000 以下显示精确值，否则以 K、M、B 保留一位小数。
//
// Parameters:
// 参数：value 是 Token 数量；非有限值与负数按 0 处理。
// Returns the compact label; a zero decimal is dropped (1000 -> "1K") and rounding carries into the next unit (999_950 -> "1M").
// 返回紧凑文本；零小数会省略（1000 -> "1K"），进位会升到下一单位（999_950 -> "1M"）。
export function formatTokenCount(value: number): string {
  const normalized = Number.isFinite(value) ? Math.max(0, Math.round(value)) : 0;
  if (normalized < TOKEN_COUNT_UNITS[0][0]) {
    return String(normalized);
  }
  for (let index = 0; index < TOKEN_COUNT_UNITS.length; index += 1) {
    const [unit, suffix] = TOKEN_COUNT_UNITS[index];
    const scaled = Math.round((normalized / unit) * 10) / 10;
    if (scaled < 1_000 || index === TOKEN_COUNT_UNITS.length - 1) {
      return `${scaled.toFixed(1).replace(/\.0$/u, "")}${suffix}`;
    }
  }
  return String(normalized);
}

// formatDurationSeconds renders a millisecond duration as seconds with one decimal, e.g. 3009 -> "3.0 s".
// formatDurationSeconds 将毫秒时长渲染为保留一位小数的秒数，例如 3009 -> "3.0 s"。
//
// Parameters:
// 参数：milliseconds 是时长毫秒数；非有限值与负数按 0 处理。
// Returns the seconds label shared by every measured-duration display (usage stats, request timings, timeouts).
// 返回所有实测时长显示（用量统计、请求耗时、超时）共用的秒数文本。
export function formatDurationSeconds(milliseconds: number): string {
  const normalized = Number.isFinite(milliseconds) ? Math.max(0, milliseconds) : 0;
  return `${(normalized / 1000).toFixed(1)} s`;
}

// tokenUnitLabel returns the token unit word written after compact counts and speeds in the active locale.
// tokenUnitLabel 返回当前语言下写在紧凑数量与速度之后的 Token 单位词。
//
// Parameters:
// 参数：locale 是当前界面语言。
// Returns "Token" for Chinese and "tokens" for other locales.
// 中文返回 "Token"，其他语言返回 "tokens"。
export function tokenUnitLabel(locale: string): string {
  return locale.startsWith("zh") ? "Token" : "tokens";
}

// formatTokenSpeedValue renders the numeric part of an output speed with one decimal, keeping tiny rates distinct from zero.
// formatTokenSpeedValue 以一位小数渲染输出速度的数值部分，并让极小速率与零区分。
//
// Parameters:
// 参数：tokensPerSecond 是每秒 Token 数；非有限值与负数按 0 处理。
// Returns "<0.1" for positive rates below 0.1, otherwise the one-decimal value.
// 正数且低于 0.1 时返回 "<0.1"，否则返回一位小数数值。
export function formatTokenSpeedValue(tokensPerSecond: number): string {
  const normalized = Number.isFinite(tokensPerSecond) ? Math.max(0, tokensPerSecond) : 0;
  return normalized > 0 && normalized < 0.1 ? "<0.1" : normalized.toFixed(1);
}

// formatTokenSpeed renders a full output speed label such as "343.8 Token/s" or "343.8 tokens/s".
// formatTokenSpeed 渲染完整输出速度文本，例如 "343.8 Token/s" 或 "343.8 tokens/s"。
//
// Parameters:
// 参数：tokensPerSecond 是每秒 Token 数，locale 是当前界面语言。
// Returns the speed with the locale token unit per second.
// 返回带当前语言 Token 单位的每秒速度文本。
export function formatTokenSpeed(tokensPerSecond: number, locale: string): string {
  return `${formatTokenSpeedValue(tokensPerSecond)} ${tokenUnitLabel(locale)}/s`;
}
