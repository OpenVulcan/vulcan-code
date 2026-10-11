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

// PERSISTED_RELATIVE_UNIT_KEYS maps the backend relative-label units (`5 min ago`, `2 mo ago`) to compact templates.
// PERSISTED_RELATIVE_UNIT_KEYS 将后端相对时间标签的单位（`5 min ago`、`2 mo ago`）映射到紧凑模板。
const PERSISTED_RELATIVE_UNIT_KEYS: Record<string, TranslationKey> = {
  min: "projectSidebar.time.minutes",
  h: "projectSidebar.time.hours",
  d: "projectSidebar.time.days",
  w: "projectSidebar.time.weeks",
  mo: "projectSidebar.time.months",
};

// compactPersistedRelativeLabel compresses the backend relative label when no raw timestamp exists.
// compactPersistedRelativeLabel 在没有原始时间戳时压缩后端相对时间标签。
function compactPersistedRelativeLabel(label: string, locale: LocaleCode): string {
  const trimmed = label.trim();
  if (!trimmed) {
    return "";
  }
  if (/^just now$/i.test(trimmed)) {
    return readLocaleMessage(locale, "projectSidebar.time.justNow");
  }
  const match = trimmed.match(/^(\d+)\s*(min|h|d|w|mo)\s+ago$/i);
  const key = match ? PERSISTED_RELATIVE_UNIT_KEYS[match[2].toLowerCase()] : undefined;
  if (!match || !key) {
    return trimmed;
  }
  return compactUnitLabel(locale, key, Number.parseInt(match[1], 10));
}
