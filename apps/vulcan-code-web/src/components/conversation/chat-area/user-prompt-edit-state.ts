import React from "react";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import type { UserPromptEditDraftSummary } from "@/types";

// UseUserPromptEditStateOptions carries backend handlers for prompt replay editing.
// UseUserPromptEditStateOptions 承载提示词重放编辑所需的后端处理器。
export interface UseUserPromptEditStateOptions {
  // Handler that loads one backend-validated prompt edit draft.
  // 加载单个后端校验提示词编辑草稿的处理器。
  onGetUserPromptEditDraft: ChatAreaProps["onGetUserPromptEditDraft"];
  // Handler that submits one edited user prompt.
  // 提交单条已编辑用户提示词的处理器。
  onSubmitUserPromptEdit: ChatAreaProps["onSubmitUserPromptEdit"];
}

// UserPromptEditState is the controlled dialog state consumed by ChatArea.
// UserPromptEditState 是 ChatArea 消费的受控弹窗状态。
export interface UserPromptEditState {
  // Backend-validated edit draft currently being edited.
  // 当前正在编辑的后端校验草稿。
  draft: UserPromptEditDraftSummary | null;
  // Controlled prompt text.
  // 受控提示词文本。
  text: string;
  // Whether a submit request is currently in flight.
  // 当前是否正在提交请求。
  saving: boolean;
  // Handler that opens the edit dialog for one user message.
  // 为单条用户消息打开编辑弹窗的处理器。
  open: (conversationId: string, messageId: string) => Promise<void>;
  // Handler that closes the edit dialog.
  // 关闭编辑弹窗的处理器。
  close: () => void;
  // Handler that updates the controlled prompt text.
  // 更新受控提示词文本的处理器。
  setText: (text: string) => void;
  // Handler that submits the current edit.
  // 提交当前编辑的处理器。
  submit: () => Promise<void>;
}

// useUserPromptEditState owns the replay-edit dialog state and submit lifecycle.
// useUserPromptEditState 持有重放编辑弹窗状态与提交生命周期。
export function useUserPromptEditState(options: UseUserPromptEditStateOptions): UserPromptEditState {
  const [draft, setDraft] = React.useState<UserPromptEditDraftSummary | null>(null);
  const [text, setText] = React.useState("");
  const [saving, setSaving] = React.useState(false);

  const close = React.useCallback(() => {
    setDraft(null);
  }, []);

  const open = React.useCallback(async (conversationId: string, messageId: string) => {
    const nextDraft = await options.onGetUserPromptEditDraft(conversationId, messageId);
    if (!nextDraft) {
      return;
    }
    setDraft(nextDraft);
    setText(nextDraft.prompt);
  }, [options]);

  const submit = React.useCallback(async () => {
    if (!draft) {
      return;
    }
    setSaving(true);
    try {
      const ok = await options.onSubmitUserPromptEdit({
        conversationId: draft.conversationId,
        messageId: draft.messageId,
        prompt: text,
        attachments: draft.attachments ?? [],
      });
      if (ok) {
        setDraft(null);
      }
    } finally {
      setSaving(false);
    }
  }, [draft, options, text]);

  return {
    close,
    draft,
    open,
    saving,
    setText,
    submit,
    text,
  };
}
