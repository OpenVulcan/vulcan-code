import React from "react";

import { useComposerPathMention, type ComposerPathMention } from "./use-composer-path-mention";
import { usePromptRecall } from "./use-composer-prompt-recall";

// ComposerListboxIds names the slash and path lists so the textarea can point at them.
// ComposerListboxIds 为 slash 与路径列表命名，使文本框可以指向它们。
export interface ComposerListboxIds {
  slash: string;
  path: string;
}

// composerListOptionId names one row of a composer list; the lists render the same ids on their rows.
// composerListOptionId 为输入区列表的一行命名；列表在其行上渲染相同的 id。
export function composerListOptionId(listboxId: string, index: number): string {
  return `${listboxId}-${index}`;
}

// useComposerTextareaKeys routes the composer textarea's keys: an open path list takes them first, then prompt
// recall gets the arrows unless the slash menu is open, and everything left goes to the composer's own handler.
// It also tells assistive technology which list the arrows move through and which row is active, since focus
// stays in the textarea while they move.
// useComposerTextareaKeys 为输入区文本框分派按键：打开的路径列表优先，其次在 slash 菜单未打开时由提示词召回处理方向键，
// 其余按键交给输入区自身的处理器。由于方向键移动时焦点始终留在文本框中，它还会告知辅助技术方向键正在哪个列表中移动、
// 哪一行处于活动状态。
export function useComposerTextareaKeys(options: {
  input: string;
  setInput: (value: string) => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  // recallScope groups recalled prompts; slotKey changes when the composer switches drafts.
  // recallScope 为召回的提示词分组；slotKey 在输入区切换草稿时改变。
  recallScope: string;
  slotKey: string;
  workspacePath: string | null;
  projectId: string | null;
  conversationId: string | null;
  slashPanelVisible: boolean;
  // slashActiveIndex is the highlighted slash row.
  // slashActiveIndex 是高亮的 slash 行。
  slashActiveIndex: number;
  onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
}): {
  pathMention: ComposerPathMention;
  handleKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  listboxIds: ComposerListboxIds;
  textareaListbox: Pick<React.AriaAttributes, "aria-autocomplete" | "aria-controls" | "aria-activedescendant">;
} {
  const { input, setInput, textareaRef } = options;
  const recallPrompt = usePromptRecall({ scope: options.recallScope, slotKey: options.slotKey, input, setInput, textareaRef });
  const pathMention = useComposerPathMention({
    input,
    setInput,
    textareaRef,
    workspacePath: options.workspacePath,
    projectId: options.projectId,
    conversationId: options.conversationId,
  });
  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (pathMention.handleKeyDown(event)) return;
    if (!options.slashPanelVisible && recallPrompt(event)) return;
    options.onKeyDown(event);
  };
  const baseId = React.useId();
  const listboxIds = React.useMemo(() => ({ slash: `${baseId}slash`, path: `${baseId}path` }), [baseId]);
  // The path list takes the keys first, so it is the one announced when both lists are open.
  // 路径列表优先接收按键，因此两个列表同时打开时以它为准。
  const active = pathMention.visible
    ? { list: listboxIds.path, index: pathMention.highlightedIndex }
    : options.slashPanelVisible ? { list: listboxIds.slash, index: options.slashActiveIndex } : null;
  const textareaListbox = {
    "aria-autocomplete": "list" as const,
    "aria-controls": active?.list,
    "aria-activedescendant": active ? composerListOptionId(active.list, active.index) : undefined,
  };
  return { pathMention, handleKeyDown, listboxIds, textareaListbox };
}
