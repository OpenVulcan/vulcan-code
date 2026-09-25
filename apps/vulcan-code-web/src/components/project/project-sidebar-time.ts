import { formatMessage, readLocaleMessage, type LocaleCode, type TranslationKey } from "@/i18n";
import type { ConversationSummary } from "@/types";

// formatConversationRelativeTime returns compact dynamic time text for sidebar rows.
// formatConversationRelativeTime 为侧栏行返回紧凑的动态时间文本。
export function formatConversationRelativeTime(
  conversation: ConversationSummary,
  nowMs: number,
  locale: LocaleCode,
): string {
  const updatedAtMs = Number.parseInt(conversation.lastUpdatedAt ?? "", 10);
  if (Number.isFinite(updatedAtMs) && updatedAtMs > 0) {
    return formatCompactRelativeTime(nowMs - updatedAtMs, locale);
  }
  return compactPersistedRelativeLabel(conversation.lastUpdatedLabel || conversation.status, locale);
}

// formatCompactRelativeTime converts one elapsed duration into the compact tree label.
// formatCompactRelativeTime 将经过时长转换为紧凑树标签。
function formatCompactRelativeTime(elapsedMs: number, locale: LocaleCode): string {
  const safeElapsedMs = Math.max(0, elapsedMs);
  const minuteMs = 60_000;
  const hourMs = 60 * minuteMs;
  const dayMs = 24 * hourMs;
  const weekMs = 7 * dayMs;
  const monthMs = 30 * dayMs;
  const yearMs = 365 * dayMs;
  if (safeElapsedMs < minuteMs) {
    return readLocaleMessage(locale, "projectSidebar.time.justNow");
  }
  if (safeElapsedMs < hourMs) {
    return compactUnitLabel(locale, "projectSidebar.time.minutes", Math.max(1, Math.floor(safeElapsedMs / minuteMs)));
  }
  if (safeElapsedMs < dayMs) {
    return compactUnitLabel(locale, "projectSidebar.time.hours", Math.max(1, Math.floor(safeElapsedMs / hourMs)));
  }
  if (safeElapsedMs < weekMs) {
    return compactUnitLabel(locale, "projectSidebar.time.days", Math.max(1, Math.floor(safeElapsedMs / dayMs)));
  }
  if (safeElapsedMs < monthMs) {
    return compactUnitLabel(locale, "projectSidebar.time.weeks", Math.max(1, Math.floor(safeElapsedMs / weekMs)));
  }
  if (safeElapsedMs < yearMs) {
    return compactUnitLabel(locale, "projectSidebar.time.months", Math.max(1, Math.floor(safeElapsedMs / monthMs)));
  }
  return compactUnitLabel(locale, "projectSidebar.time.years", Math.max(1, Math.floor(safeElapsedMs / yearMs)));
}

// compactUnitLabel fills one compact unit template such as `{value}m` in the active language.
// compactUnitLabel 以当前语言填充一个紧凑单位模板，例如 `{value}m`。
function compactUnitLabel(locale: LocaleCode, key: TranslationKey, value: number): string {
  return formatMessage(readLocaleMessage(locale, key), { value });
}

// compactPersistedRelativeLabel compresses older static labels when no raw timestamp exists.
// compactPersistedRelativeLabel 在没有原始时间戳时压缩旧的静态标签。
function compactPersistedRelativeLabel(label: string, locale: LocaleCode): string {
  const trimmed = label.trim();
  if (!trimmed) {
    return "";
  }
  if (/^(刚刚|now)$/i.test(trimmed)) {
    return readLocaleMessage(locale, "projectSidebar.time.justNow");
  }
  const match = trimmed.match(/(\d+)\s*(分|分钟|minute|minutes|min|m|时|小时|hour|hours|h|天|日|day|days|d|周|星期|week|weeks|w|月|month|months|M|年|year|years|Y)/i);
  if (!match) {
    return trimmed;
  }
  const value = Number.parseInt(match[1], 10);
  const rawUnit = match[2];
  const unit = rawUnit.toLowerCase();
  if (rawUnit === "M" || unit === "月" || unit === "month" || unit === "months") {
    return compactUnitLabel(locale, "projectSidebar.time.months", value);
  }
  if (rawUnit === "Y" || unit === "年" || unit === "year" || unit === "years" || unit === "y") {
    return compactUnitLabel(locale, "projectSidebar.time.years", value);
  }
  if (unit === "分" || unit === "分钟" || unit === "minute" || unit === "minutes" || unit === "min" || unit === "m") {
    return compactUnitLabel(locale, "projectSidebar.time.minutes", value);
  }
  if (unit === "时" || unit === "小时" || unit === "hour" || unit === "hours" || unit === "h") {
    return compactUnitLabel(locale, "projectSidebar.time.hours", value);
  }
  if (unit === "天" || unit === "日" || unit === "day" || unit === "days" || unit === "d") {
    return compactUnitLabel(locale, "projectSidebar.time.days", value);
  }
  if (unit === "周" || unit === "星期" || unit === "week" || unit === "weeks" || unit === "w") {
    return compactUnitLabel(locale, "projectSidebar.time.weeks", value);
  }
  return trimmed;
}
