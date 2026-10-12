// Usage values arrive as exact decimal strings; these helpers format them through BigInt so no value
// ever passes through a floating-point Number.
// 用量值以精确十进制字符串到达；这些辅助函数经由 BigInt 格式化，任何值都不经过浮点 Number。

// MICRO_POINTS_PER_POINT is the server's fixed unit: one point equals one million micro-points.
// MICRO_POINTS_PER_POINT 是服务端固定单位：一积分等于一百万微积分。
const MICRO_POINTS_PER_POINT = 1_000_000n;
// MICRO_DIGITS is the number of fractional digits one micro-point adds to a point.
// MICRO_DIGITS 是微积分为积分带来的小数位数。
const MICRO_DIGITS = 6;

// parseCount returns value as a BigInt, or null when it is not a non-negative integer string.
// parseCount 将 value 转为 BigInt；不是非负整数字符串时返回 null。
function parseCount(value: string): bigint | null {
  return /^\d+$/.test(value) ? BigInt(value) : null;
}

// formatCount groups an exact integer string for locale, returning the raw text when it is not one.
// formatCount 按 locale 为精确整数字符串分组；不是整数时原样返回。
export function formatCount(value: string, locale: string): string {
  const parsed = parseCount(value);
  return parsed === null ? value : parsed.toLocaleString(locale);
}

// formatMicroPoints converts exact micro-points to points for locale, keeping every significant digit.
// formatMicroPoints 将精确微积分按 locale 转为积分，保留全部有效数字。
export function formatMicroPoints(value: string, locale: string): string {
  const parsed = parseCount(value);
  if (parsed === null) return value;
  const whole = (parsed / MICRO_POINTS_PER_POINT).toLocaleString(locale);
  const fraction = (parsed % MICRO_POINTS_PER_POINT).toString().padStart(MICRO_DIGITS, "0").replace(/0+$/, "");
  if (!fraction) return whole;
  const separator = new Intl.NumberFormat(locale).formatToParts(1.5).find((part) => part.type === "decimal")?.value ?? ".";
  return `${whole}${separator}${fraction}`;
}

// pointsToMicro converts an exact "whole.micro" point string to micro-points, or null when it is not one.
// pointsToMicro 将精确的“整数.微积分”积分字符串转为微积分；格式不符时返回 null。
export function pointsToMicro(value: string | undefined): bigint | null {
  const match = value ? /^(\d+)(?:\.(\d{1,6}))?$/.exec(value) : null;
  if (!match) return null;
  return BigInt(match[1]) * MICRO_POINTS_PER_POINT + BigInt((match[2] ?? "").padEnd(MICRO_DIGITS, "0"));
}

// formatPoints groups an exact point string for locale, returning the raw text when it is not one.
// formatPoints 按 locale 为精确积分字符串分组；格式不符时原样返回。
export function formatPoints(value: string, locale: string): string {
  const micro = pointsToMicro(value);
  return micro === null ? value : formatMicroPoints(micro.toString(), locale);
}

// remainingShare returns remaining/total as a 0–100 percentage for a progress bar, or null when unknown.
// remainingShare 以 0–100 百分比返回剩余/总量，用于进度条；无法确定时返回 null。
export function remainingShare(remaining: string | undefined, total: string | undefined): number | null {
  const left = pointsToMicro(remaining);
  const whole = pointsToMicro(total);
  if (left === null || whole === null || whole === 0n) return null;
  // Per-mille precision keeps the division in BigInt; only the small quotient becomes a Number.
  // 千分位精度使除法保留在 BigInt 中；只有很小的商转为 Number。
  const permille = (left > whole ? whole : left) * 1000n / whole;
  return Number(permille) / 10;
}

// formatExpiryDate shows a date-only expiry in the viewer's local time, or the raw text when it is not a date.
// formatExpiryDate 以查看者本地时间显示到期日期；不是日期时原样返回。
export function formatExpiryDate(value: string, locale: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale, { year: "numeric", month: "2-digit", day: "2-digit" }).format(date);
}

// sumCounts adds exact integer strings; values that are not integers are skipped.
// sumCounts 累加精确整数字符串；非整数值被跳过。
export function sumCounts(values: readonly string[]): string {
  return values.reduce((total, value) => total + (parseCount(value) ?? 0n), 0n).toString();
}

// formatUsageHour shows a UTC hour start in the viewer's local time, or the raw text when it is not a date.
// formatUsageHour 以查看者本地时间显示 UTC 小时起点；不是日期时原样返回。
export function formatUsageHour(value: string, locale: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }).format(date);
}
