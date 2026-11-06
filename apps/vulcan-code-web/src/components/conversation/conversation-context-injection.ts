import type { TranslationKey } from "@/i18n-messages";
import { shellJobStatusLabel } from "@/components/workspace/session-activity/session-activity-format";
import type { ShellJobNoticeFacts } from "./shell-job-notice";

// ContextInjectionCategory names the user-facing family of one injected prompt.
// ContextInjectionCategory 表示一条注入提示词面向用户的类别。
export type ContextInjectionCategory =
  | "goal" | "shellJob" | "plan" | "tools" | "permission" | "model" | "answer" | "guidance" | "base" | "injection" | "other";

// ContextInjectionEntry is one injected prompt shown inside a context-injection card.
// ContextInjectionEntry 是上下文注入卡片中显示的一条注入提示词。
export interface ContextInjectionEntry {
  context_key: string;
  category: ContextInjectionCategory;
  operation: "upsert" | "remove";
  content: string;
  // Optional heading that names what the entry is about, shown instead of the operation.
  // 可选标题，指明条目的对象，替代操作名显示。
  label?: string;
}

// ContextInjectionPage groups a card's entries of one category, in the order the category first appears.
// ContextInjectionPage 按类别首次出现的顺序归组卡片中同一类别的条目。
export interface ContextInjectionPage {
  category: ContextInjectionCategory;
  entries: ContextInjectionEntry[];
}

type Translation = (key: TranslationKey) => string;

// contextInjectionCategoryLabel localizes one category name.
// contextInjectionCategoryLabel 本地化一个类别名称。
export function contextInjectionCategoryLabel(category: ContextInjectionCategory, t: Translation): string {
  return t(`conversation.contextInjection.category.${category}`);
}

// contextInjectionOperationLabel localizes whether an entry sent or withdrew its instructions.
// contextInjectionOperationLabel 本地化条目是发送还是撤回指令。
export function contextInjectionOperationLabel(operation: ContextInjectionEntry["operation"], t: Translation): string {
  return t(operation === "remove" ? "conversation.contextInjection.remove" : "conversation.contextInjection.upsert");
}

// contextInjectionEntryMarkdown shows one entry's operation over the exact text the model received, fenced
// longer than any backtick run inside it so the text renders verbatim.
// contextInjectionEntryMarkdown 显示一个条目的操作及模型收到的原文；围栏长于原文中的任何反引号序列，使原文逐字显示。
export function contextInjectionEntryMarkdown(entry: ContextInjectionEntry, t: Translation): string {
  const longestRun = Math.max(0, ...(entry.content.match(/`{3,}/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(Math.max(3, longestRun + 1));
  const heading = entry.label ?? contextInjectionOperationLabel(entry.operation, t);
  return `**${heading}**\n\n${fence}text\n${entry.content}\n${fence}`;
}

// shellJobNoticeLabel names a finished background command like its command card: the purpose (or the command's first
// line), the job id, and how it ended with its exit code.
// shellJobNoticeLabel 像命令卡一样命名已结束的后台命令：用途（或命令首行）、任务标识，以及结束方式与退出码。
export function shellJobNoticeLabel(notice: ShellJobNoticeFacts, t: Translation): string {
  const name = notice.purpose.trim() || notice.command.trim().split("\n")[0]!;
  const parts = [name, notice.jobId, shellJobStatusLabel(notice.status, t)];
  if (notice.exitCode !== null) parts.push(t("toolCard.exitCode").replace("{code}", String(notice.exitCode)));
  return parts.filter((part) => part.length > 0).join(" · ");
}

// contextInjectionPages splits a card's entries into one page per category.
// contextInjectionPages 将卡片条目按类别拆分为每类一页。
export function contextInjectionPages(entries: readonly ContextInjectionEntry[]): ContextInjectionPage[] {
  const pages = new Map<ContextInjectionCategory, ContextInjectionEntry[]>();
  for (const entry of entries) pages.set(entry.category, [...(pages.get(entry.category) ?? []), entry]);
  return [...pages].map(([category, grouped]) => ({ category, entries: grouped }));
}

// readContextInjectionEntries returns the entries a context-injection notice carries, or null for any other block.
// readContextInjectionEntries 返回上下文注入提示携带的条目；其他块返回 null。
export function readContextInjectionEntries(metadata: unknown): ContextInjectionEntry[] | null {
  const record = metadata as { notice_kind?: unknown; context_entries?: unknown } | null | undefined;
  if (record?.notice_kind !== "context_injection" || !Array.isArray(record.context_entries)) return null;
  return record.context_entries as ContextInjectionEntry[];
}
