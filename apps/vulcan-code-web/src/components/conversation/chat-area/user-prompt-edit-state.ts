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

  const open = React.useCallback(async (message: ConversationMessage, visiblePrompt: string) => {
    const requestVersion = validationVersion.current + 1;
    validationVersion.current = requestVersion;
    // optimisticDraft opens the dialog synchronously from the already-rendered authoritative row.
    // optimisticDraft 直接从已经渲染的权威消息行同步打开弹窗。
    const optimisticDraft: UserPromptEditDraftSummary = {
      conversationId: message.conversationId,
      messageId: message.id,
      prompt: visiblePrompt,
      attachments: message.attachments ?? [],
      running: false,
    };
    setDraft(optimisticDraft);
    setText(visiblePrompt);
    const nextDraft = await options.onGetUserPromptEditDraft(message.conversationId, message.id);
    if (validationVersion.current !== requestVersion) {
      return;
    }
    if (!nextDraft) {
      setDraft(null);
      return;
    }
    setDraft((currentDraft) => (
      currentDraft?.conversationId === message.conversationId && currentDraft.messageId === message.id
        ? nextDraft
        : currentDraft
    ));
    // Preserve text already changed by the user while the backend validation request was in flight.
    // 后端校验请求进行期间保留用户已经修改的文本。
    setText((currentText) => currentText === visiblePrompt ? nextDraft.prompt : currentText);
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
