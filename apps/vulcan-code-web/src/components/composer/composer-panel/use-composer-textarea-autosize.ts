import React from "react";

/**
 * Grows the composer textarea with its content. The height follows the text's scroll height, and the
 * stylesheet's `max-height` (the one place that sets how many extra lines it may grow) caps it, after which
 * the field scrolls. The height is measured again when the text changes and when the field's width changes,
 * since wrapping depends on the width.
 * 让输入框随内容增高。高度跟随文本的滚动高度，由样式表的 `max-height` 限制（可额外增高的行数只在那里设定），
 * 超出后输入框内部滚动。文本变化或输入框宽度变化时重新测量，因为折行取决于宽度。
 */
export function useComposerTextareaAutosize(textareaRef: React.RefObject<HTMLTextAreaElement | null>, value: string): void {
  React.useLayoutEffect(() => {
    fitTextareaHeight(textareaRef.current);
  }, [textareaRef, value]);

  React.useEffect(() => {
    const element = textareaRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    let width = element.clientWidth;
    // Only width changes rewrap the text; height changes come from this hook itself.
    // 只有宽度变化会改变折行；高度变化来自本 hook 自身。
    const observer = new ResizeObserver(() => {
      if (element.clientWidth === width) return;
      width = element.clientWidth;
      fitTextareaHeight(element);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [textareaRef]);
}

// fitTextareaHeight resets the height so the scroll height reflects only the text, then applies it.
// fitTextareaHeight 先重置高度，使滚动高度只反映文本，再应用该高度。
function fitTextareaHeight(element: HTMLTextAreaElement | null): void {
  if (!element) return;
  element.style.height = "auto";
  element.style.height = `${element.scrollHeight}px`;
}
