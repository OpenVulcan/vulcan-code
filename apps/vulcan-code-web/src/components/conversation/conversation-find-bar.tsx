import React from "react";
import { workbenchShortcutAction } from "@/lib/workbench-shortcuts";
import { ChevronDown, ChevronUp, Search, X } from "lucide-react";

import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { IconButton } from "@/ui-kit/primitives/button";
import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from "@/ui-kit/primitives/input-group";
import "@/ui-kit/styles/features/conversation-find.css";

import {
  CONVERSATION_FIND_HIGHLIGHT,
  CONVERSATION_FIND_MATCH_LIMIT,
  clearConversationFind,
  findTextRanges,
  paintConversationFind,
} from "./conversation-find";

// CONVERSATION_FIND_DELAY_MS lets typing settle before searching, and coalesces transcript updates while a reply streams.
// CONVERSATION_FIND_DELAY_MS 让输入稳定后再搜索，并在回答流式输出时合并转录更新。
const CONVERSATION_FIND_DELAY_MS = 150;

// ConversationFindBar searches the open conversation's transcript. Ctrl+F (Cmd+F) opens it or returns to it,
// Enter and Shift+Enter step through the matches, and Escape closes it and gives focus back. A view with its own
// search (such as a file preview) marks its root with data-find-scope and renders a scoped bar inside it: Ctrl+F
// pressed in that root belongs to that bar alone.
// ConversationFindBar 搜索当前会话的转录。Ctrl+F（Cmd+F）打开或回到它，Enter 与 Shift+Enter 在匹配之间移动，
// Escape 关闭并归还焦点。拥有独立搜索的视图（如文件预览）以 data-find-scope 标记根元素并在其中渲染限定范围的查找栏：
// 在该根元素内按下的 Ctrl+F 只属于那个查找栏。
// A bar given onGotoLine also opens on Ctrl+G (Cmd+G) with ":" typed: ":42" and Enter goes to line 42.
// 提供 onGotoLine 的查找栏也会在 Ctrl+G（Cmd+G）时打开并预填 ":"：输入 ":42" 后按 Enter 跳到第 42 行。
export function ConversationFindBar(props: {
  // scrollRef is the transcript scrollport the search covers.
  // scrollRef 是搜索覆盖的转录滚动视口。
  scrollRef: React.RefObject<HTMLElement | null>;
  // onReveal scrolls a match into view through the transcript's scroll owner.
  // onReveal 通过转录的滚动所有者将匹配滚动到可见位置。
  onReveal: (range: Range) => void;
  // scoped bars answer Ctrl+F only from inside the data-find-scope root around scrollRef.
  // scoped 查找栏只响应在 scrollRef 外围 data-find-scope 根元素内按下的 Ctrl+F。
  scoped?: boolean;
  // label names what is searched; it defaults to the conversation.
  // label 说明搜索的对象；默认为会话。
  label?: TranslationKey;
  // highlight is the CSS highlight name of the matches (see CONVERSATION_FIND_HIGHLIGHT).
  // highlight 是匹配项的 CSS 高亮名称（见 CONVERSATION_FIND_HIGHLIGHT）。
  highlight?: string;
  // onGotoLine goes to a line number typed after ":"; without it the bar only searches.
  // onGotoLine 跳到 ":" 后输入的行号；未提供时查找栏只做搜索。
  onGotoLine?: (line: number) => void;
}) {
  const { t } = useI18n();
  const { scrollRef, onReveal, scoped = false, highlight = CONVERSATION_FIND_HIGHLIGHT } = props;
  const label = t(props.label ?? "conversation.find.label");
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  // result holds the matches of the last search and the one the reader is on.
  // result 保存最近一次搜索的匹配及读者所在的那一个。
  const [result, setResult] = React.useState<{ ranges: Range[]; current: number }>({ ranges: [], current: -1 });
  // latestResult lets keys pressed faster than React renders each step from the previous step.
  // latestResult 让快于 React 渲染的连续按键都从上一步继续移动。
  const latestResult = React.useRef(result);
  latestResult.current = result;
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  // returnFocus is where focus goes back to when the bar closes.
  // returnFocus 是查找栏关闭时焦点返回的位置。
  const returnFocus = React.useRef<HTMLElement | null>(null);
  const openRef = React.useRef(open);
  openRef.current = open;
  // focusRequest asks the rendered input to take focus: Ctrl+F selects its text, Ctrl+G puts the caret after ":".
  // focusRequest 请求渲染后的输入框获得焦点：Ctrl+F 选中其文本，Ctrl+G 将光标置于 ":" 之后。
  const [focusRequest, setFocusRequest] = React.useState<{ caretAtEnd: boolean } | null>(null);
  const canGotoLine = Boolean(props.onGotoLine);
  // lineQuery is the query when it asks for a line (":" and digits) rather than text.
  // lineQuery 表示查询请求的是行号（":" 加数字）而不是文本。
  const lineQuery = canGotoLine && /^:\d*$/.test(query.trim());

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const action = workbenchShortcutAction(event);
      if (event.defaultPrevented || !(action === "find" || (action === "gotoLine" && canGotoLine))) return;
      const port = scrollRef.current;
      if (!port?.isConnected || (typeof port.checkVisibility === "function" && !port.checkVisibility())) return;
      const target = event.target instanceof Element ? event.target : null;
      // Dialogs and other text fields keep Ctrl+F; the composer and the transcript hand it to the bar.
      // 对话框与其他文本字段保留 Ctrl+F；输入区与转录将其交给查找栏。
      if (target?.closest("[role='dialog'], [role='alertdialog']")) return;
      const scope = target?.closest("[data-find-scope]") ?? null;
      if (scoped ? !scope?.contains(port) : scope && !scope.contains(port)) return;
      const chat = port.closest(".vk-chat");
      if (!scoped && target?.closest("input, textarea, select, [contenteditable='true']") && !chat?.contains(target)) return;
      event.preventDefault();
      if (!openRef.current) {
        returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        const selected = window.getSelection()?.toString().trim() ?? "";
        if (action === "find" && selected && !selected.includes("\n") && selected.length <= 200) setQuery(selected);
        setOpen(true);
      }
      if (action === "gotoLine") setQuery(":");
      setFocusRequest({ caretAtEnd: action === "gotoLine" });
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [canGotoLine, scoped, scrollRef]);

  // The bar takes focus once its input exists and holds the text the shortcut put there.
  // 查找栏在输入框出现并含有快捷键填入的文本后获得焦点。
  React.useLayoutEffect(() => {
    const input = inputRef.current;
    if (!open || !focusRequest || !input) return;
    input.focus();
    if (focusRequest.caretAtEnd) input.setSelectionRange(input.value.length, input.value.length);
    else input.select();
  }, [focusRequest, open]);

  // search runs the query again; a fresh query starts at the first match at or below the top of the view, while a
  // re-run after the transcript changed stays on the same match number.
  // search 重新执行查询；新查询从视图顶部及以下的首个匹配开始，转录变化后的重跑则停留在相同的匹配序号。
  const search = React.useCallback((fresh: boolean) => {
    const port = scrollRef.current;
    const ranges = port && !lineQuery ? findTextRanges(port, query) : [];
    setResult((previous) => {
      if (!fresh) return { ranges, current: ranges.length === 0 ? -1 : Math.min(Math.max(previous.current, 0), ranges.length - 1) };
      const top = port?.getBoundingClientRect().top ?? 0;
      const firstInView = ranges.findIndex((range) => range.getBoundingClientRect().bottom >= top);
      return { ranges, current: ranges.length === 0 ? -1 : Math.max(firstInView, 0) };
    });
  }, [lineQuery, query, scrollRef]);

  React.useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(() => search(true), CONVERSATION_FIND_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [open, search]);

  // A streaming reply or a fold changes the text under the matches, so the search follows the transcript.
  // 流式回答或折叠会改变匹配所在的文本，因此搜索跟随转录更新。
  React.useEffect(() => {
    const port = scrollRef.current;
    if (!open || !query.trim() || !port) return;
    let timer = 0;
    const observer = new MutationObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => search(false), CONVERSATION_FIND_DELAY_MS);
    });
    observer.observe(port, { characterData: true, childList: true, subtree: true });
    return () => {
      observer.disconnect();
      window.clearTimeout(timer);
    };
  }, [open, query, scrollRef, search]);

  React.useEffect(() => {
    if (!open) return;
    paintConversationFind(result.ranges, result.current, highlight);
  }, [highlight, open, result]);

  // The highlights belong to the open bar only.
  // 高亮只属于打开的查找栏。
  React.useEffect(() => {
    if (!open) clearConversationFind(highlight);
    return () => clearConversationFind(highlight);
  }, [highlight, open]);

  // step moves to the next or previous match, wrapping at either end, and brings it into view.
  // step 移动到下一个或上一个匹配，在两端循环，并将其滚动到可见位置。
  const step = (direction: 1 | -1) => {
    const { ranges, current } = latestResult.current;
    if (ranges.length === 0) return;
    const next = (current + direction + ranges.length) % ranges.length;
    latestResult.current = { ranges, current: next };
    setResult(latestResult.current);
    onReveal(ranges[next]);
  };

  // A new search reveals its starting match unless it is already on screen.
  // 新搜索会显示其起始匹配，除非它已在屏幕上。
  const revealedQuery = React.useRef("");
  React.useEffect(() => {
    const range = result.ranges[result.current];
    if (!open || !range || revealedQuery.current === query) return;
    revealedQuery.current = query;
    const port = scrollRef.current;
    if (!port) return;
    const view = port.getBoundingClientRect();
    const box = range.getBoundingClientRect();
    const dock = Number.parseFloat(getComputedStyle(port).getPropertyValue("--composer-dock-height")) || 0;
    if (box.top < view.top || box.bottom > view.bottom - dock) onReveal(range);
  }, [onReveal, open, query, result, scrollRef]);

  const close = () => {
    setOpen(false);
    setResult({ ranges: [], current: -1 });
    revealedQuery.current = "";
    const target = returnFocus.current;
    returnFocus.current = null;
    if (target?.isConnected) target.focus();
  };

  if (!open) return null;
  const total = result.ranges.length;
  const totalLabel = total >= CONVERSATION_FIND_MATCH_LIMIT ? `${CONVERSATION_FIND_MATCH_LIMIT}+` : String(total);
  const lineNumber = lineQuery ? Number(query.trim().slice(1)) : 0;
  const status = lineQuery
    ? lineNumber > 0 ? formatMessage(t("workspaceFiles.gotoLine"), { line: String(lineNumber) }) : t("workspaceFiles.gotoLineEmpty")
    : !query.trim()
    ? ""
    : total === 0
      ? t("conversation.find.none")
      : formatMessage(t("conversation.find.count"), { current: String(result.current + 1), total: totalLabel });
  return (
    <div aria-label={label} className="vk-find-bar" role="search">
      <InputGroup>
        <InputGroupAddon>
          <Search aria-hidden="true" />
        </InputGroupAddon>
        <InputGroupInput
          aria-label={label}
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.nativeEvent.isComposing) {
              event.preventDefault();
              if (!lineQuery) step(event.shiftKey ? -1 : 1);
              else if (lineNumber > 0) {
                props.onGotoLine?.(lineNumber);
                close();
              }
            } else if (event.key === "Escape") {
              event.preventDefault();
              close();
            }
          }}
          placeholder={label}
          ref={inputRef}
          spellCheck={false}
          value={query}
        />
        <InputGroupAddon align="inline-end">
          <InputGroupText aria-live="polite">{status}</InputGroupText>
        </InputGroupAddon>
      </InputGroup>
      <IconButton aria-label={t("conversation.find.previous")} disabled={total === 0} onClick={() => step(-1)} title={t("conversation.find.previous")} type="button">
        <ChevronUp />
      </IconButton>
      <IconButton aria-label={t("conversation.find.next")} disabled={total === 0} onClick={() => step(1)} title={t("conversation.find.next")} type="button">
        <ChevronDown />
      </IconButton>
      <IconButton aria-label={t("conversation.find.close")} onClick={close} title={t("conversation.find.close")} type="button">
        <X />
      </IconButton>
    </div>
  );
}
