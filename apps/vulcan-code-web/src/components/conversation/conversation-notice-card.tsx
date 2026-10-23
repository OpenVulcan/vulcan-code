// Inline notice card. Runtime notices (memory recall, prompt injections, display notes) and legacy text-only
// tool blocks use the tool-card frame and open in place, never in a dialog; a prompt-injection card pages
// through its categories like the memory search card pages through memories.
// 内联提示卡。运行时提示（记忆召回、提示词注入、显示提示）与旧版纯文本工具块使用工具卡框架并就地展开，
// 绝不使用弹窗；提示词注入卡像记忆检索卡翻阅记忆一样按类别分页。
import * as React from "react";
import { ChevronLeft, ChevronRight, type LucideIcon } from "lucide-react";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { ToolFrame } from "@/conversation/tools/tool-card";
import type { ToolCardState } from "@/conversation/tools/tool-call";
import { MarkdownResult } from "@/conversation/tools/views/result-views";
import type { ConversationBlock } from "./conversation-model";
import { normalizeToolDisplayState } from "./conversation-tool-metadata";
import {
  contextInjectionCategoryLabel,
  contextInjectionEntryMarkdown,
  contextInjectionPages,
  readContextInjectionEntries,
  type ContextInjectionEntry,
} from "./conversation-context-injection";

// noticeCardState maps a block status onto the tool-card lifecycle; a notice without one is settled.
// noticeCardState 将块状态映射为工具卡生命周期；没有状态的提示视为已完成。
function noticeCardState(status: string | undefined): ToolCardState {
  const state = status ? normalizeToolDisplayState(status) : undefined;
  return state === "failed" ? "failed" : state === "running" ? "running" : state === "stopped" ? "stopped" : "done";
}

/** Renders one notice or legacy tool block as an inline expandable card.
 * 将单个提示或旧版工具块渲染为可就地展开的卡片。 */
export function ConversationNoticeCard(props: {
  // Block shown by the card.
  // 卡片展示的块。
  block: ConversationBlock;
  // Head icon.
  // 标题图标。
  icon: LucideIcon;
  // Localized head title.
  // 已本地化的标题。
  title: string;
  // WebUI-session expansion memory shared with tool cards.
  // 与工具卡共享的当前 WebUI 会话展开记忆。
  expandedIds: Set<string>;
  // Records one expansion state change.
  // 记录一次展开状态变化。
  onExpandedChange: (id: string, expanded: boolean) => void;
}) {
  const cardKey = `note:${props.block.id}`;
  // localOpen applies a click immediately while the surface memory updates as a transition.
  // localOpen 立即应用点击，界面记忆则以过渡任务更新。
  const [localOpen, setLocalOpen] = React.useState<boolean | null>(null);
  const open = localOpen ?? props.expandedIds.has(cardKey);
  const { onExpandedChange } = props;
  const handleOpenChange = React.useCallback((next: boolean) => {
    setLocalOpen(next);
    React.startTransition(() => onExpandedChange(cardKey, next));
  }, [cardKey, onExpandedChange]);
  const entries = readContextInjectionEntries(props.block.rawMetadata);
  const body = entries && entries.length > 0
    ? <ContextInjectionPages entries={entries} />
    : props.block.body.trim() ? <MarkdownResult text={props.block.body} /> : null;
  return (
    <ToolFrame
      family="other"
      icon={props.icon}
      meta={props.block.summary ? <span>{props.block.summary}</span> : null}
      onOpenChange={handleOpenChange}
      open={open}
      state={noticeCardState(props.block.status)}
      verb={props.title}
    >
      {body}
    </ToolFrame>
  );
}

// ContextInjectionPages shows one category per page; a card with several categories gets a pager.
// ContextInjectionPages 每页显示一个类别；含多个类别的卡片提供翻页。
function ContextInjectionPages(props: { entries: readonly ContextInjectionEntry[] }) {
  const { t } = useI18n();
  const pages = React.useMemo(() => contextInjectionPages(props.entries), [props.entries]);
  const [index, setIndex] = React.useState(0);
  const page = pages[Math.min(index, pages.length - 1)]!;
  return (
    <div className="vk-memory-page">
      <div className="vk-memory-page-head">
        <span className="vk-memory-page-id">{contextInjectionCategoryLabel(page.category, t)}</span>
        {pages.length > 1 ? (
          <span className="vk-memory-pager">
            <Button aria-label={t("conversation.contextInjection.previous")} disabled={index <= 0}
              onClick={() => setIndex((value) => Math.max(0, value - 1))} size="icon-xs" type="button" variant="ghost">
              <ChevronLeft size={12} />
            </Button>
            <span>{index + 1} / {pages.length}</span>
            <Button aria-label={t("conversation.contextInjection.next")} disabled={index >= pages.length - 1}
              onClick={() => setIndex((value) => Math.min(pages.length - 1, value + 1))} size="icon-xs" type="button" variant="ghost">
              <ChevronRight size={12} />
            </Button>
          </span>
        ) : null}
      </div>
      <MarkdownResult text={page.entries.map((entry) => contextInjectionEntryMarkdown(entry, t)).join("\n\n")} />
    </div>
  );
}
