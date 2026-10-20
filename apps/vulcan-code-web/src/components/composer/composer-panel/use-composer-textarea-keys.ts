import React from "react";

import { useComposerPathMention, type ComposerPathMention } from "./use-composer-path-mention";
import { usePromptRecall } from "./use-composer-prompt-recall";

// useComposerTextareaKeys routes the composer textarea's keys: an open path list takes them first, then prompt
// recall gets the arrows unless the slash menu is open, and everything left goes to the composer's own handler.
// useComposerTextareaKeys 为输入区文本框分派按键：打开的路径列表优先，其次在 slash 菜单未打开时由提示词召回处理方向键，
// 其余按键交给输入区自身的处理器。
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
  onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
}): { pathMention: ComposerPathMention; handleKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void } {
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
  return { pathMention, handleKeyDown };
}
