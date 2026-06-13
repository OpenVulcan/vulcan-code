import type { LocaleCode } from "@/i18n";
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
  const chinese = locale === "zh-CN";
  if (safeElapsedMs < minuteMs) {
    return chinese ? "刚刚" : "now";
  }
  if (safeElapsedMs < hourMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / minuteMs));
    return chinese ? `${value}分` : `${value}m`;
  }
  if (safeElapsedMs < dayMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / hourMs));
    return chinese ? `${value}时` : `${value}h`;
  }
  if (safeElapsedMs < weekMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / dayMs));
    return chinese ? `${value}天` : `${value}d`;
  }
  if (safeElapsedMs < monthMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / weekMs));
    return chinese ? `${value}周` : `${value}w`;
  }
  if (safeElapsedMs < yearMs) {
    const value = Math.max(1, Math.floor(safeElapsedMs / monthMs));
    return `${value}M`;
  }
  const value = Math.max(1, Math.floor(safeElapsedMs / yearMs));
  return `${value}Y`;
}

// compactPersistedRelativeLabel compresses older static labels when no raw timestamp exists.
// compactPersistedRelativeLabel 在没有原始时间戳时压缩旧的静态标签。
function compactPersistedRelativeLabel(label: string, locale: LocaleCode): string {
  const trimmed = label.trim();
  if (!trimmed) {
    return "";
  }
  if (/^(刚刚|now)$/i.test(trimmed)) {
    return locale === "zh-CN" ? "刚刚" : "now";
  }
  const match = trimmed.match(/(\d+)\s*(分|分钟|minute|minutes|min|m|时|小时|hour|hours|h|天|日|day|days|d|周|星期|week|weeks|w|月|month|months|M|年|year|years|Y)/i);
  if (!match) {
    return trimmed;
  }
  const value = Number.parseInt(match[1], 10);
  const rawUnit = match[2];
  const unit = rawUnit.toLowerCase();
  const chinese = locale === "zh-CN";
  if (rawUnit === "M" || unit === "月" || unit === "month" || unit === "months") {
    return `${value}M`;
  }
  if (rawUnit === "Y" || unit === "年" || unit === "year" || unit === "years" || unit === "y") {
    return `${value}Y`;
  }
  if (unit === "分" || unit === "分钟" || unit === "minute" || unit === "minutes" || unit === "min" || unit === "m") {
    return chinese ? `${value}分` : `${value}m`;
  }
  if (unit === "时" || unit === "小时" || unit === "hour" || unit === "hours" || unit === "h") {
    return chinese ? `${value}时` : `${value}h`;
  }
  if (unit === "天" || unit === "日" || unit === "day" || unit === "days" || unit === "d") {
    return chinese ? `${value}天` : `${value}d`;
  }
  if (unit === "周" || unit === "星期" || unit === "week" || unit === "weeks" || unit === "w") {
    return chinese ? `${value}周` : `${value}w`;
  }
  return trimmed;
}
