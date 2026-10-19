import React from "react";
import {
  CONVERSATION_PROMPT_NAVIGATOR_ACTIVE_LINE_RATIO,
  resolveActivePromptNavigatorIndex,
  type ConversationPromptNavigatorEntry,
} from "@/components/conversation/conversation-prompt-navigator-model";
import { useI18n } from "@/i18n";

// CONVERSATION_PROMPT_NAVIGATOR_CARD_MARGIN_PX keeps the hover card inside the navigator overlay bounds.
// CONVERSATION_PROMPT_NAVIGATOR_CARD_MARGIN_PX 使悬停卡片保持在导航覆盖层边界内。
const CONVERSATION_PROMPT_NAVIGATOR_CARD_MARGIN_PX = 8;

// ConversationPromptNavigatorProps describes one edge-pinned prompt rail bound to a transcript scrollport.
// ConversationPromptNavigatorProps 描述一个绑定转录滚动视口的贴边提示词轨道。
export interface ConversationPromptNavigatorProps {
  // Prompt entries in transcript order.
  // 按转录顺序排列的提示词条目。
  entries: readonly ConversationPromptNavigatorEntry[];
  // Transcript scrollport that contains every rendered turn shell.
  // 包含全部已渲染 Turn 外壳的转录滚动视口。
  scrollRef: React.RefObject<HTMLElement | null>;
  // Moves the transcript to one rendered turn shell through the scroll controller.
  // 通过滚动控制器将转录移动到一个已渲染 Turn 外壳。
  onRevealAnchor: (anchor: HTMLElement) => void;
}

// ConversationPromptNavigatorHover identifies the tick whose preview card is visible.
// ConversationPromptNavigatorHover 标识当前显示预览卡片的刻度。
interface ConversationPromptNavigatorHover {
  // Hovered or focused entry key.
  // 被悬停或聚焦的条目键。
  rowKey: string;
  // Tick center relative to the navigator overlay top edge.
  // 刻度中心相对导航覆盖层顶部的位置。
  anchorY: number;
}

// collectTurnShells maps rendered turn keys to their shell elements without selector escaping.
// collectTurnShells 在不做选择器转义的前提下将已渲染 Turn 键映射到外壳元素。
// Parameters: scrollElement is the transcript scrollport.
// 参数：scrollElement 是转录滚动视口。
// Returns: turn key to shell element.
// 返回值：Turn 键到外壳元素的映射。
function collectTurnShells(scrollElement: HTMLElement): Map<string, HTMLElement> {
  const shells = new Map<string, HTMLElement>();
  for (const node of scrollElement.querySelectorAll<HTMLElement>("[data-conversation-turn-shell]")) {
    const key = node.dataset.conversationTurnShell;
    if (key) {
      shells.set(key, node);
    }
  }
  return shells;
}

// ConversationPromptNavigator renders one tick per user prompt, expands into a comb on hover, and jumps on click.
// ConversationPromptNavigator 为每个用户提示词渲染一个刻度，悬停时展开为梳齿，点击后跳转。
// Parameters: props contains ordered entries, the transcript scrollport, and the controller reveal callback.
// 参数：props 包含有序条目、转录滚动视口与控制器定位回调。
// Returns: the navigator overlay.
// 返回值：导航覆盖层。
export const ConversationPromptNavigator = React.memo(function ConversationPromptNavigator(props: ConversationPromptNavigatorProps) {
  const { t } = useI18n();
  const { entries, onRevealAnchor, scrollRef } = props;
  // entriesRef lets scroll measurement read the newest entries without resubscribing on content-only updates.
  // entriesRef 让滚动测量读取最新条目，而不必在纯内容更新时重新订阅。
  const entriesRef = React.useRef(entries);
  entriesRef.current = entries;
  // rowKeySignature changes only when prompts are added, removed, or reordered, which is when positions must be re-measured.
  // rowKeySignature 仅在提示词增删或重排时变化，此时才需要重新测量位置。
  const rowKeySignature = entries.map((entry) => entry.rowKey).join("\n");
  const rootRef = React.useRef<HTMLDivElement | null>(null);
  const listRef = React.useRef<HTMLOListElement | null>(null);
  const cardRef = React.useRef<HTMLDivElement | null>(null);
  const [activeKey, setActiveKey] = React.useState<string | null>(null);
  const [hover, setHover] = React.useState<ConversationPromptNavigatorHover | null>(null);
  // pinnedKeyRef keeps a clicked prompt current until the reader scrolls again after the jump settles.
  // pinnedKeyRef 在跳转结束后阅读者再次滚动前，保持被点击的提示词为当前项。
  const pinnedKeyRef = React.useRef<string | null>(null);
  // pinSettledRef records that the jump's native scroll has ended, so the next scroll releases the pin.
  // pinSettledRef 记录跳转的原生滚动已结束，下一次滚动即释放固定。
  const pinSettledRef = React.useRef(false);

  React.useEffect(() => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    let frame: number | null = null;
    const measure = (): void => {
      frame = null;
      if (pinnedKeyRef.current) {
        return;
      }
      const shells = collectTurnShells(element);
      const activeLine = element.getBoundingClientRect().top + element.clientHeight * CONVERSATION_PROMPT_NAVIGATOR_ACTIVE_LINE_RATIO;
      const current = entriesRef.current;
      const index = resolveActivePromptNavigatorIndex(
        current.length,
        (position) => shells.get(current[position].rowKey)?.getBoundingClientRect().top ?? Number.POSITIVE_INFINITY,
        activeLine,
      );
      setActiveKey(index >= 0 ? current[index].rowKey : null);
    };
    const schedule = (): void => {
      if (pinnedKeyRef.current) {
        if (!pinSettledRef.current) {
          return;
        }
        pinnedKeyRef.current = null;
        pinSettledRef.current = false;
      }
      if (frame === null) {
        frame = window.requestAnimationFrame(measure);
      }
    };
    const settle = (): void => {
      if (pinnedKeyRef.current) {
        pinSettledRef.current = true;
      }
    };
    schedule();
    element.addEventListener("scroll", schedule, { passive: true });
    element.addEventListener("scrollend", settle);
    return () => {
      element.removeEventListener("scroll", schedule);
      element.removeEventListener("scrollend", settle);
      if (frame !== null) {
        window.cancelAnimationFrame(frame);
      }
    };
  }, [rowKeySignature, scrollRef]);

  // Keep the current tick inside the rail's own hidden-scrollbar viewport.
  // 让当前刻度保持在轨道自身无滚动条视口之内。
  React.useEffect(() => {
    const list = listRef.current;
    if (!list || !activeKey) {
      return;
    }
    const item = Array.from(list.children).find(
      (child): child is HTMLElement => child instanceof HTMLElement && child.dataset.promptNavigatorKey === activeKey,
    );
    if (!item) {
      return;
    }
    const itemTop = item.offsetTop;
    const itemBottom = itemTop + item.offsetHeight;
    if (itemTop < list.scrollTop || itemBottom > list.scrollTop + list.clientHeight) {
      list.scrollTop = itemTop - (list.clientHeight - item.offsetHeight) / 2;
    }
  }, [activeKey, rowKeySignature]);

  // Clamp the preview card inside the overlay after its content height is known.
  // 在卡片内容高度确定后将预览卡片约束在覆盖层内部。
  React.useLayoutEffect(() => {
    const card = cardRef.current;
    const root = rootRef.current;
    if (!card || !root || !hover) {
      return;
    }
    const maxTop = root.clientHeight - card.offsetHeight - CONVERSATION_PROMPT_NAVIGATOR_CARD_MARGIN_PX;
    const centeredTop = hover.anchorY - card.offsetHeight / 2;
    card.style.top = `${Math.max(CONVERSATION_PROMPT_NAVIGATOR_CARD_MARGIN_PX, Math.min(maxTop, centeredTop))}px`;
  }, [entries, hover]);

  const showCard = React.useCallback((rowKey: string, tick: HTMLElement): void => {
    const root = rootRef.current;
    if (!root) {
      return;
    }
    const tickRect = tick.getBoundingClientRect();
    setHover({ rowKey, anchorY: tickRect.top + tickRect.height / 2 - root.getBoundingClientRect().top });
  }, []);

  const hideCard = React.useCallback((): void => setHover(null), []);

  const navigate = React.useCallback((rowKey: string): void => {
    const element = scrollRef.current;
    const shell = element ? collectTurnShells(element).get(rowKey) : undefined;
    if (!shell) {
      return;
    }
    pinnedKeyRef.current = rowKey;
    pinSettledRef.current = false;
    setActiveKey(rowKey);
    onRevealAnchor(shell);
  }, [onRevealAnchor, scrollRef]);

  const handleKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLButtonElement>): void => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") {
      return;
    }
    const item = event.currentTarget.closest("li");
    const sibling = event.key === "ArrowUp" ? item?.previousElementSibling : item?.nextElementSibling;
    const next = sibling?.querySelector("button");
    if (next instanceof HTMLButtonElement) {
      event.preventDefault();
      next.focus();
    }
  }, []);

  // hoveredEntry is resolved from the live entry list so streaming replies refresh an open card.
  // hoveredEntry 从实时条目列表解析，使流式回复可以刷新已打开的卡片。
  const hoveredEntry = hover ? entries.find((entry) => entry.rowKey === hover.rowKey) ?? null : null;
  // focusableKey is the single tab stop of the rail.
  // focusableKey 是轨道唯一的 Tab 停靠点。
  const focusableKey = activeKey && entries.some((entry) => entry.rowKey === activeKey) ? activeKey : entries[0]?.rowKey;
  const hoveredPreview = hoveredEntry?.replyPreview
    || (hoveredEntry?.running ? t("conversation.promptNavigator.replyPending") : t("conversation.promptNavigator.replyEmpty"));

  return (
    <div className="vk-prompt-nav" ref={rootRef}>
      <nav
        aria-label={t("conversation.promptNavigator.label")}
        className="vk-prompt-nav-rail"
        onMouseLeave={hideCard}
      >
        <ol className="vk-prompt-nav-list" onScroll={hideCard} ref={listRef}>
          {entries.map((entry) => {
            const label = entry.prompt || t("conversation.promptNavigator.untitled");
            return (
              <li data-prompt-navigator-key={entry.rowKey} key={entry.rowKey}>
                <button
                  aria-current={entry.rowKey === activeKey ? "true" : undefined}
                  aria-label={label}
                  className="vk-prompt-nav-tick"
                  onBlur={hideCard}
                  onClick={() => navigate(entry.rowKey)}
                  onFocus={(event) => showCard(entry.rowKey, event.currentTarget)}
                  onKeyDown={handleKeyDown}
                  onMouseEnter={(event) => showCard(entry.rowKey, event.currentTarget)}
                  tabIndex={entry.rowKey === focusableKey ? 0 : -1}
                  type="button"
                >
                  <span aria-hidden="true" className="vk-prompt-nav-line" />
                </button>
              </li>
            );
          })}
        </ol>
      </nav>
      {hoveredEntry ? (
        <div aria-hidden="true" className="vk-prompt-nav-card" ref={cardRef}>
          <div className="vk-prompt-nav-title">
            {hoveredEntry.prompt || t("conversation.promptNavigator.untitled")}
          </div>
          <div className="vk-prompt-nav-preview" data-empty={hoveredEntry.replyPreview ? undefined : "true"}>
            {hoveredPreview}
          </div>
        </div>
      ) : null}
    </div>
  );
});
