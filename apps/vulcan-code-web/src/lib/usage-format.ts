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
// Returns the seconds label used by every usage statistic surface.
// 返回所有用量统计界面共用的秒数文本。
export function formatDurationSeconds(milliseconds: number): string {
  const normalized = Number.isFinite(milliseconds) ? Math.max(0, milliseconds) : 0;
  return `${(normalized / 1000).toFixed(1)} s`;
}
