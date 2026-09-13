import type { TranslationKey } from "@/i18n"

// Integral units retain the exact native millisecond value without a repeating decimal tail.
// 整数单位保留准确原生毫秒值，避免无限循环小数尾数。
const durationUnits = [
  [86400000, "memory.policy.days"], [3600000, "memory.policy.hours"],
  [60000, "memory.policy.minutes"], [1000, "memory.policy.seconds"],
  [1, "memory.policy.milliseconds"],
] as const satisfies ReadonlyArray<readonly [number, TranslationKey]>

/** Formats a valid positive native policy duration using its largest exact integral unit.
 * 使用最大准确整数单位格式化有效的原生正数策略时长。
 * durationMs is integer milliseconds; locale selects digit grouping; translate supplies unit labels.
 * durationMs 为整数毫秒；locale 决定数字分组；translate 提供单位文案。
 * Returns a localized exact duration, retaining milliseconds when larger units cannot express it exactly.
 * 返回本地化准确时长；更大单位无法准确表示时保留毫秒。
 */
export function formatNativeDuration(durationMs: number, locale: string, translate: (key: TranslationKey) => string): string {
  // unit always exists for the validated integer contract because its final factor is one.
  // 有效整数契约始终有可用单位，因为最后一个因子为一。
  const unit = durationUnits.find(([factor]) => durationMs % factor === 0)!
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(durationMs / unit[0])} ${translate(unit[1])}`
}
