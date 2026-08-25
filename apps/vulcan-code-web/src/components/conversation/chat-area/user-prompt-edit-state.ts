import React from "react";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import type { ConversationMessage, UserPromptEditDraftSummary } from "@/types";

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
  open: (message: ConversationMessage, visiblePrompt: string) => Promise<void>;
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
  // validationVersion invalidates delayed backend draft responses after close or a newer edit selection.
  // validationVersion 在关闭弹窗或选择更新编辑项后使延迟到达的后端草稿响应失效。
  const validationVersion = React.useRef(0);

  const close = React.useCallback(() => {
    validationVersion.current += 1;
    setDraft(null);
  }, []);

  const open = React.useCallback(async (message: ConversationMessage, _visiblePrompt: string) => {
    const requestVersion = validationVersion.current + 1;
    validationVersion.current = requestVersion;
    setDraft(null);
    // Backend validation must complete before the dialog becomes submittable; otherwise a stale
    // edit affordance can race the validation request and submit a prompt that is no longer editable.
    // 后端校验必须先于可提交弹窗完成；否则陈旧编辑入口可能与校验请求竞态并提交已不可编辑的提示词。
    const nextDraft = await options.onGetUserPromptEditDraft(message.conversationId, message.id);
    if (validationVersion.current !== requestVersion) {
      return;
    }
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
