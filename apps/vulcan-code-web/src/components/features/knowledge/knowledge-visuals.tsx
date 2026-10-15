import { Bot, Cog, FolderTree, Layers, Sparkles, TextSearch, UserRound, type LucideIcon } from "lucide-react";
import type { TranslationKey } from "@/i18n";
import type { KnowledgeIndexMode } from "@/types/knowledge-base";
import { KNOWLEDGE_MODE_COPY } from "./knowledge-mode";

// Each index mode keeps one recognizable glyph across the rail, header and policy cards.
// 每种索引模式在知识库栏、页眉及策略卡片中保持同一可识别图标。
export const KNOWLEDGE_MODE_ICONS: Readonly<Record<KnowledgeIndexMode, LucideIcon>> = {
  browse: FolderTree, bm25: TextSearch, vector: Sparkles, hybrid: Layers,
};

// Actor glyphs distinguish AI tool calls, human restores and system imports in history.
// 执行者图标在历史中区分 AI 工具调用、人工还原及系统导入。
const ACTOR_ICONS: Readonly<Record<string, LucideIcon>> = { ai: Bot, user: UserRound, system: Cog };

// Actor labels are catalog keys; unknown kinds fall back to the system label without guessing ownership.
// 执行者标签为目录键；未知种类回退为系统标签，不推测归属。
const ACTOR_LABELS: Readonly<Record<string, TranslationKey>> = {
  ai: "knowledge.history.actorKind.ai", user: "knowledge.history.actorKind.user", system: "knowledge.history.actorKind.system",
};

// Replaces `{name}` placeholders in one catalog message; returns the filled display text.
// 替换一条目录消息中的 `{name}` 占位符；返回填充后的展示文本。
// Parameters: template is a translated message; values maps placeholder names to display values.
// 参数：template 为已翻译消息；values 将占位符名称映射为展示值。
export function fillKnowledgeText(template: string, values: Readonly<Record<string, string | number>>): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) => Object.hasOwn(values, name) ? String(values[name]) : match);
}

// Renders one compact mode badge whose tone is fixed by the declared mode.
// 渲染一个紧凑模式徽章，色调由声明模式固定。
// Parameters: mode is the library-owned index mode; t resolves its label. Returns a non-interactive badge.
// 参数：mode 为库拥有的索引模式；t 解析其标签。返回非交互徽章。
export function KnowledgeModeBadge({ mode, t }: { mode: KnowledgeIndexMode; t: (key: TranslationKey) => string }) {
  // The glyph mirrors the mode card in the policy dialog.
  // 图标与策略对话框中的模式卡片一致。
  const Icon = KNOWLEDGE_MODE_ICONS[mode];
  return <span className="knowledge-badge" data-mode={mode}><Icon size={11} aria-hidden="true" />{t(KNOWLEDGE_MODE_COPY[mode].label)}</span>;
}

// Returns the catalog key for one actor kind.
// 返回一种执行者种类的目录键。
// Parameters: kind is the immutable history actor kind. Returns its label key.
// 参数：kind 为不可变历史执行者种类。返回其标签键。
export function knowledgeActorLabel(kind: string): TranslationKey {
  return Object.hasOwn(ACTOR_LABELS, kind) ? ACTOR_LABELS[kind] : ACTOR_LABELS.system;
}

// Renders the circular history marker for one actor kind.
// 渲染一种执行者种类的圆形历史标记。
// Parameters: kind is the immutable actor kind; label names it for assistive technology. Returns a decorative marker.
// 参数：kind 为不可变执行者种类；label 为辅助技术命名。返回装饰性标记。
export function KnowledgeActorMarker({ kind, label }: { kind: string; label: string }) {
  // Unknown kinds share the neutral system glyph.
  // 未知种类共享中性系统图标。
  const Icon = Object.hasOwn(ACTOR_ICONS, kind) ? ACTOR_ICONS[kind] : Cog;
  return <span className="knowledge-timeline-marker" data-actor={Object.hasOwn(ACTOR_ICONS, kind) ? kind : "system"} title={label}><Icon size={15} aria-hidden="true" /></span>;
}
