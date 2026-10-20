import React from "react";
import { createPortal } from "react-dom";
import { requestComposerCommand } from "@/components/composer/composer-external-commands";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";

// quoteAsMarkdown turns selected transcript text into a Markdown block quote followed by an empty line, so
// the user's own note starts on a fresh paragraph below it.
// quoteAsMarkdown 将选中的会话文本转为 Markdown 引用块并在其后留一个空行，使用户自己的标注从引用下方的新段落开始。
export function quoteAsMarkdown(text: string): string {
  const lines = text.replace(/\r\n?/g, "\n").replace(/^\n+|\s+$/g, "").split("\n");
  return `${lines.map((line) => (line.trim() ? `> ${line}` : ">")).join("\n")}\n\n`;
}

// quoteInComposer appends `text` as a block quote to the mounted composer's draft without sending it.
// quoteInComposer 将 `text` 作为引用块追加到已挂载输入区的草稿中，但不发送。
export function quoteInComposer(text: string): void {
  if (!text.trim()) return;
  requestComposerCommand({ kind: "insert", text: quoteAsMarkdown(text) });
}

// SelectionQuoteAnchor is the selected text and the viewport point the quote action floats at.
// SelectionQuoteAnchor 是已选文本以及引用操作悬浮所在的视口坐标。
interface SelectionQuoteAnchor {
  text: string;
  // top is the viewport y of the action's bottom edge, or its top edge when `below` is set.
  // top 是操作下边缘的视口 y；设置 `below` 时为其上边缘。
  top: number;
  // left is the viewport x of the action's horizontal center.
  // left 是操作水平中心的视口 x。
  left: number;
  // below places the action under the selection when there is no room above it.
  // below 在选区上方没有空间时把操作放在选区下方。
  below: boolean;
}

// SELECTION_QUOTE_CLEARANCE_PX is the room above a selection the action needs before it flips below.
// SELECTION_QUOTE_CLEARANCE_PX 是操作翻转到下方之前在选区上方所需的空间。
const SELECTION_QUOTE_CLEARANCE_PX = 40;

// readTimelineSelection returns the quote anchor for a non-empty selection fully inside the visible part of
// the timeline, or null for anything else.
// readTimelineSelection 为完全位于时间线可见部分内的非空选区返回引用锚点，其余情况返回 null。
function readTimelineSelection(
  timeline: HTMLElement | null,
  viewport: HTMLElement | null,
): SelectionQuoteAnchor | null {
  const selection = window.getSelection();
  if (!timeline || !selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  if (!selection.anchorNode || !selection.focusNode) return null;
  if (!timeline.contains(selection.anchorNode) || !timeline.contains(selection.focusNode)) return null;
  const text = selection.toString();
  if (!text.trim()) return null;
  const rect = selection.getRangeAt(0).getBoundingClientRect();
  const bounds = (viewport ?? timeline).getBoundingClientRect();
  // A selection scrolled out of the transcript keeps no floating action over unrelated content.
  // 已滚出会话区域的选区不在无关内容上保留悬浮操作。
  if (rect.bottom <= bounds.top || rect.top >= bounds.bottom) return null;
  const below = rect.top - bounds.top < SELECTION_QUOTE_CLEARANCE_PX;
  const center = rect.left + rect.width / 2;
  return {
    text,
    top: below ? rect.bottom : rect.top,
    left: Math.min(Math.max(center, bounds.left), bounds.right),
    below,
  };
}

// ConversationSelectionQuoteProps names the transcript whose selections can be quoted.
// ConversationSelectionQuoteProps 指定其选区可被引用的会话转录。
interface ConversationSelectionQuoteProps {
  // timelineRef is the transcript content; selections that reach outside it are ignored.
  // timelineRef 是会话内容；超出其范围的选区会被忽略。
  timelineRef: React.RefObject<HTMLElement | null>;
  // scrollRef is the scrolling transcript viewport that bounds and moves the action.
  // scrollRef 是限定并带动操作位置的会话滚动视口。
  scrollRef: React.RefObject<HTMLElement | null>;
  // onQuote receives the selected text when the user chooses the action.
  // onQuote 在用户选择该操作时接收已选文本。
  onQuote: (text: string) => void;
}

// ConversationSelectionQuote floats a quote action above text selected in the transcript.
// ConversationSelectionQuote 在会话中选中的文本上方悬浮一个引用操作。
export function ConversationSelectionQuote(props: ConversationSelectionQuoteProps) {
  const { t } = useI18n();
  const [anchor, setAnchor] = React.useState<SelectionQuoteAnchor | null>(null);

  React.useEffect(() => {
    // frame coalesces the reads triggered by one gesture into the next paint.
    // frame 将同一手势触发的多次读取合并到下一次绘制。
    let frame = 0;
    const read = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        setAnchor(readTimelineSelection(props.timelineRef.current, props.scrollRef.current));
      });
    };
    // A selection is offered only once the pointer or keyboard gesture ends, never while it is dragged,
    // but it disappears as soon as the selection collapses.
    // 只在指针或键盘手势结束后提供选区操作，拖动过程中不显示；选区一旦折叠立即消失。
    const hideWhenCollapsed = () => {
      if (window.getSelection()?.isCollapsed ?? true) setAnchor(null);
    };
    const scroller = props.scrollRef.current;
    document.addEventListener("pointerup", read);
    document.addEventListener("keyup", read);
    document.addEventListener("selectionchange", hideWhenCollapsed);
    window.addEventListener("resize", read);
    scroller?.addEventListener("scroll", read, { passive: true });
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("pointerup", read);
      document.removeEventListener("keyup", read);
      document.removeEventListener("selectionchange", hideWhenCollapsed);
      window.removeEventListener("resize", read);
      scroller?.removeEventListener("scroll", read);
    };
  }, [props.timelineRef, props.scrollRef]);

  if (!anchor) return null;
  return createPortal(
    <div
      className="vk-selection-quote"
      data-placement={anchor.below ? "below" : "above"}
      style={{ top: anchor.top, left: anchor.left }}
    >
      <Button
        size="xs"
        variant="outline"
        // Keeping the pointer press from moving focus keeps the selection the action is about.
        // 阻止指针按下转移焦点，从而保留该操作所针对的选区。
        onPointerDown={(event) => event.preventDefault()}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => {
          props.onQuote(anchor.text);
          window.getSelection()?.removeAllRanges();
          setAnchor(null);
        }}
      >
        {t("conversation.contextMenu.quote")}
      </Button>
    </div>,
    document.body,
  );
}
