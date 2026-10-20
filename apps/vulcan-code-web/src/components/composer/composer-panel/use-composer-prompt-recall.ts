import React from "react";

// Prompt recall: ArrowUp in an empty composer brings back prompts sent earlier in the same project during this
// app session, newest first, and ArrowDown walks forward again until the empty draft returns. History lives only
// in memory, so a reload starts it over and nothing about it is persisted.
// 提示词召回：在空输入区按 ArrowUp 依次（从新到旧）取回本次应用运行期间同一项目中发送过的提示词，ArrowDown
// 再向新的方向回走，直到回到空草稿。历史只保存在内存中，重新加载即重新开始，不做任何持久化。

// PROMPT_HISTORY_LIMIT bounds the prompts kept per project.
// PROMPT_HISTORY_LIMIT 限制每个项目保留的提示词数量。
const PROMPT_HISTORY_LIMIT = 50;

// histories maps a project scope to its sent prompts, oldest first.
// histories 将项目范围映射到其已发送提示词，按从旧到新排列。
const histories = new Map<string, string[]>();

// recordSentPrompt appends one accepted prompt to its project's history; a repeat moves to the newest place.
// recordSentPrompt 将一条已被接受的提示词追加到项目历史；重复的提示词移到最新位置。
export function recordSentPrompt(scope: string, prompt: string): void {
  if (!prompt.trim()) return;
  const entries = (histories.get(scope) ?? []).filter((entry) => entry !== prompt);
  entries.push(prompt);
  histories.set(scope, entries.slice(-PROMPT_HISTORY_LIMIT));
}

// sentPrompts returns a project's history, oldest first.
// sentPrompts 返回项目历史，按从旧到新排列。
export function sentPrompts(scope: string): readonly string[] {
  return histories.get(scope) ?? [];
}

// clearSentPrompts empties every history; tests use it to start from a known state.
// clearSentPrompts 清空全部历史；测试用它从已知状态开始。
export function clearSentPrompts(): void {
  histories.clear();
}

// usePromptRecall returns a keydown handler that reports whether it consumed the key.
// usePromptRecall 返回一个键盘处理器，并报告是否消费了该按键。
export function usePromptRecall(options: {
  // scope names the project whose history is walked.
  // scope 指定被浏览历史所属的项目。
  scope: string;
  // slotKey changes when the composer switches drafts, which ends any recall in progress.
  // slotKey 在输入区切换草稿时改变，随之结束进行中的召回。
  slotKey: string;
  input: string;
  setInput: (value: string) => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
}): (event: React.KeyboardEvent<HTMLTextAreaElement>) => boolean {
  // position is the recalled entry counted back from the newest (0), or null when nothing is recalled.
  // position 是从最新条目（0）往回计数的召回位置；未召回时为 null。
  const position = React.useRef<number | null>(null);
  // shown is the text recall last placed in the composer; any edit ends the recall.
  // shown 是召回最近放入输入区的文本；任何编辑都会结束召回。
  const shown = React.useRef<string | null>(null);
  React.useLayoutEffect(() => {
    position.current = null;
    shown.current = null;
  }, [options.slotKey, options.scope]);
  const latest = React.useRef(options);
  latest.current = options;
  return React.useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return false;
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.nativeEvent.isComposing) return false;
    const { input, scope, setInput, textareaRef } = latest.current;
    const recalling = position.current !== null && input === shown.current;
    if (!recalling) {
      position.current = null;
      shown.current = null;
      if (event.key === "ArrowDown" || input.trim() !== "") return false;
    }
    const target = event.currentTarget;
    // Arrows keep moving the caret between lines; only the first or last line hands over to recall.
    // 方向键仍在行间移动光标；只有首行或末行才交给召回。
    if (event.key === "ArrowUp" && input.slice(0, target.selectionStart).includes("\n")) return false;
    if (event.key === "ArrowDown" && input.slice(target.selectionEnd).includes("\n")) return false;
    const entries = sentPrompts(scope);
    const next = event.key === "ArrowUp" ? (position.current ?? -1) + 1 : (position.current ?? 0) - 1;
    if (next >= entries.length) return false;
    event.preventDefault();
    const text = next < 0 ? "" : entries[entries.length - 1 - next];
    position.current = next < 0 ? null : next;
    shown.current = next < 0 ? null : text;
    setInput(text);
    // The caret goes to the end once the recalled text has rendered.
    // 召回文本渲染后，光标移到末尾。
    window.requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (textarea) textarea.setSelectionRange(text.length, text.length);
    });
    return true;
  }, []);
}
