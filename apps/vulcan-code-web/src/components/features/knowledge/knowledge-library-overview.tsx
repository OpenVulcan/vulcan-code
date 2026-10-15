import type * as React from "react";
import { FileText, TriangleAlert } from "lucide-react";
import type { TranslationKey } from "@/i18n";
import { formatDateTime } from "@/lib/display-format";
import { formatKnowledgeIndexStatus } from "@/lib/knowledge-index-status";
import type { KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { fillKnowledgeText, KnowledgeModeBadge } from "./knowledge-visuals";

// Shared display inputs for one registry summary; no mutation callback crosses this boundary.
// 一个注册表摘要的共享展示输入；变更回调不跨越此边界。
interface KnowledgeLibraryDisplayProps {
  // Exact current registry summary.
  // 精确当前注册表摘要。
  library: KnowledgeLibrarySummary;
  // Active-locale catalog lookup.
  // 当前语言目录查找。
  t: (key: TranslationKey) => string;
}

// Returns whether a registry entry can open its content, history and index views.
// 返回注册表条目是否可打开内容、历史及索引视图。
// Parameters: library is one registry summary. Returns true only for an active library with readable settings.
// 参数：library 为一个注册表摘要。仅活动且设置可读的库返回 true。
export function knowledgeLibraryReadable(library: KnowledgeLibrarySummary): library is KnowledgeLibrarySummary & { settings: NonNullable<KnowledgeLibrarySummary["settings"]> } {
  return library.lifecycleState === "active" && library.settings !== null;
}

// Renders the derived-index status as a toned badge; unknown states render nothing rather than a guess.
// 将派生索引状态渲染为带色调徽章；未知状态不渲染，绝不猜测。
// Parameters: props supplies the summary and translator. Returns a badge or null.
// 参数：props 提供摘要及翻译器。返回徽章或空值。
export function KnowledgeIndexStatusBadge({ library, t }: KnowledgeLibraryDisplayProps) {
  // The shared formatter owns the known status vocabulary.
  // 共享格式化器拥有已知状态词汇。
  const label = formatKnowledgeIndexStatus(library.indexStatus, t);
  if (label === "-") return null;
  return <span className="knowledge-badge" data-status={library.indexStatus}><span className="knowledge-dot" data-status={library.indexStatus} />{label}</span>;
}

// Renders one selectable rail card summarizing mode, size and index health.
// 渲染一个可选择的栏卡片，概括模式、规模及索引健康。
// Parameters: selected/disabled control state; onSelect selects this exact ID. Returns a pressed-state button.
// 参数：selected 及 disabled 控制状态；onSelect 选择此精确身份。返回带按下状态的按钮。
export function KnowledgeLibraryCard({ library, t, selected, disabled, onSelect }: KnowledgeLibraryDisplayProps & { selected: boolean; disabled: boolean; onSelect: () => void }) {
  // Unreadable libraries show their diagnosis badge instead of a mode.
  // 不可读库显示诊断徽章而非模式。
  const readable = knowledgeLibraryReadable(library);
  return <button type="button" className="knowledge-library-card" aria-pressed={selected} disabled={disabled} onClick={onSelect}>
    <span className="knowledge-library-card-head">
      <strong>{library.name}</strong>
      {readable ? <KnowledgeModeBadge mode={library.settings.indexMode} t={t} /> : <span className="knowledge-badge" data-tone="danger"><TriangleAlert size={11} aria-hidden="true" />{t("knowledge.unavailable")}</span>}
    </span>
    {library.description !== "" ? <span className="knowledge-library-card-description">{library.description}</span> : null}
    {readable ? <span className="knowledge-library-card-meta">
      <span><FileText size={12} aria-hidden="true" />{fillKnowledgeText(t("knowledge.documentCount"), { count: library.documentCount })}</span>
      {formatKnowledgeIndexStatus(library.indexStatus, t) !== "-" ? <span><span className="knowledge-dot" data-status={library.indexStatus} />{formatKnowledgeIndexStatus(library.indexStatus, t)}</span> : null}
    </span> : null}
  </button>;
}

// Renders the selected library's identity, lifecycle actions and headline statistics.
// 渲染选定库的身份、生命周期动作及主要统计。
// Parameters: locale formats timestamps; actions are parent-owned lifecycle controls. Returns the detail header.
// 参数：locale 格式化时间戳；actions 为父组件拥有的生命周期控件。返回详情页眉。
export function KnowledgeLibraryHero({ library, t, locale, actions }: KnowledgeLibraryDisplayProps & { locale: string; actions: React.ReactNode }) {
  // Statistics are shown only for readable libraries; diagnostics replace them otherwise.
  // 仅可读库显示统计；否则由诊断替代。
  const readable = knowledgeLibraryReadable(library);
  // Each statistic is a direct registry field, never a client-side estimate.
  // 每项统计均为注册表直接字段，绝非客户端估算。
  const stats: { key: string; label: TranslationKey; value: React.ReactNode; tone?: string }[] = [
    { key: "documents", label: "knowledge.stats.documents", value: library.documentCount.toLocaleString(locale) },
    { key: "chunks", label: "knowledge.stats.chunks", value: library.chunkCount.toLocaleString(locale) },
    { key: "summaries", label: "knowledge.stats.pendingSummaries", value: library.pendingSummaryCount.toLocaleString(locale), tone: library.pendingSummaryCount > 0 ? "warning" : undefined },
    { key: "changes", label: "knowledge.stats.changes", value: library.headChangeSeq.toLocaleString(locale) },
    { key: "last", label: "knowledge.stats.lastChange", value: library.lastChangeAtMs === null ? <small>{t("knowledge.stats.noChanges")}</small> : <small>{formatDateTime(library.lastChangeAtMs, locale)}</small> },
  ];
  return <header className="knowledge-hero">
    <div className="knowledge-hero-head">
      <div className="knowledge-hero-title">
        <div><h3>{library.name}</h3>{readable ? <><KnowledgeModeBadge mode={library.settings.indexMode} t={t} /><KnowledgeIndexStatusBadge library={library} t={t} /></> : <span className="knowledge-badge" data-tone="danger"><TriangleAlert size={11} aria-hidden="true" />{t("knowledge.unavailable")}</span>}</div>
        {library.description !== "" ? <p>{library.description}</p> : null}
      </div>
      <div className="knowledge-actions">{actions}</div>
    </div>
    {readable ? <dl className="knowledge-stats">{stats.map((stat) => <div className="knowledge-stat" data-tone={stat.tone} key={stat.key}><dt>{t(stat.label)}</dt><dd>{stat.value}</dd></div>)}</dl> : null}
  </header>;
}
