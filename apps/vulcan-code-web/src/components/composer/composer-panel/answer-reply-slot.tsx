import React from "react";
import { AnswerReplyContext } from "@/components/conversation/answer-components/answer-reply";

// LazyAnswerReplyTray loads only after the reader picks an option or comments on an answer component.
// LazyAnswerReplyTray 仅在读者于回答组件上选择选项或批注之后加载。
const LazyAnswerReplyTray = React.lazy(async () => ({
  default: (await import("@/components/conversation/answer-components/answer-reply-tray")).AnswerReplyTray,
}));

// noAnswerReplies keeps composers outside a conversation surface on the same subscription path.
// noAnswerReplies 让会话界面之外的输入区走同一条订阅路径。
const noAnswerReplies = () => () => {};

// ComposerAnswerReplySlot shows the reply tray while the reader has decisions or comments on answer components, and
// appends the summarized reply to the draft; the reader still sends it.
// ComposerAnswerReplySlot 在读者于回答组件上有决策或批注时显示回复条，并把汇总回复追加到草稿；仍由读者自行发送。
export function ComposerAnswerReplySlot(props: {
  // Whether draft mutation is temporarily excluded, such as during a queued prompt replacement.
  // 是否暂时禁止修改草稿，例如排队提示词替换期间。
  disabled: boolean;
  // Draft setter of the active composer owner.
  // 当前输入区归属的草稿设置器。
  setDraftInput: React.Dispatch<React.SetStateAction<string>>;
  // Prompt field focused after insertion.
  // 插入后聚焦的提示词输入框。
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
}) {
  const store = React.useContext(AnswerReplyContext);
  const count = React.useSyncExternalStore(store?.subscribe ?? noAnswerReplies, () => store?.size() ?? 0);
  const { setDraftInput, textareaRef } = props;
  const insert = React.useCallback((text: string) => {
    setDraftInput((current) => current.trim() ? `${current.trimEnd()}\n\n${text}` : text);
    window.requestAnimationFrame(() => textareaRef.current?.focus({ preventScroll: true }));
  }, [setDraftInput, textareaRef]);
  if (!store || count === 0 || props.disabled) return null;
  return (
    <React.Suspense fallback={null}>
      <LazyAnswerReplyTray onInsert={insert} store={store} />
    </React.Suspense>
  );
}
