// Decimal token units keep goal usage compact and provider-independent.
// 十进制 Token 单位让目标用量保持紧凑且不依赖 Provider。
const GOAL_TOKEN_UNITS = [
  { divisor: 1, suffix: "" },
  { divisor: 1_000, suffix: "K" },
  { divisor: 1_000_000, suffix: "M" },
  { divisor: 1_000_000_000, suffix: "B" },
] as const;

// normalizeGoalTokenCount converts non-finite, negative, or fractional input into a safe displayed token integer.
// normalizeGoalTokenCount 将非有限、负数或小数输入转换为安全展示的 Token 整数。
function normalizeGoalTokenCount(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.trunc(value));
}

// formatCompactGoalTokenCount renders one token count with fixed K, M, and B suffixes and at most two decimals.
// formatCompactGoalTokenCount 使用固定 K、M、B 后缀和最多两位小数渲染 Token 数。
export function formatCompactGoalTokenCount(value: number): string {
  // normalizedValue is the exact nonnegative integer represented by the compact label.
  // normalizedValue 是紧凑标签表示的精确非负整数。
  const normalizedValue = normalizeGoalTokenCount(value);
  // unitIndex selects the largest supported unit whose divisor does not exceed the value.
  // unitIndex 选择除数不超过当前数值的最大受支持单位。
  let unitIndex = 0;
  for (let index = 1; index < GOAL_TOKEN_UNITS.length; index += 1) {
    if (normalizedValue < GOAL_TOKEN_UNITS[index].divisor) {
      break;
    }
    unitIndex = index;
  }

  // roundedValue is recalculated from the source value when rounding promotes it to the next unit.
  // roundedValue 在四舍五入晋升到下一单位时会基于原始数值重新计算。
  let roundedValue = roundCompactGoalTokenValue(
    normalizedValue / GOAL_TOKEN_UNITS[unitIndex].divisor,
  );
  while (roundedValue >= 1_000 && unitIndex < GOAL_TOKEN_UNITS.length - 1) {
    unitIndex += 1;
    roundedValue = roundCompactGoalTokenValue(
      normalizedValue / GOAL_TOKEN_UNITS[unitIndex].divisor,
    );
  }

  return `${trimCompactGoalTokenDecimals(roundedValue)}${GOAL_TOKEN_UNITS[unitIndex].suffix}`;
}

// formatFullGoalTokenCount renders the exact token integer with locale-aware separators for Tooltip details.
// formatFullGoalTokenCount 使用区域化分隔符渲染精确 Token 整数，供 Tooltip 明细使用。
export function formatFullGoalTokenCount(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(
    normalizeGoalTokenCount(value),
  );
}

// roundCompactGoalTokenValue rounds one scaled compact value to at most two decimal places.
// roundCompactGoalTokenValue 将缩放后的紧凑数值四舍五入到最多两位小数。
function roundCompactGoalTokenValue(value: number): number {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}

// trimCompactGoalTokenDecimals removes insignificant zeroes from one rounded compact value.
// trimCompactGoalTokenDecimals 移除紧凑数值中无意义的末尾零。
function trimCompactGoalTokenDecimals(value: number): string {
  return value.toFixed(2).replace(/\.?0+$/, "");
}
