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

// ComposerAnswerReplySlot connects the composer to the conversation's answer replies while its draft can take them,
// and shows the reply tray while the reader has decisions or comments. Inserting appends the summarized reply to the
// draft; sending appends it and asks the composer to send the draft through its normal send path.
// ComposerAnswerReplySlot 在草稿可接收时把输入区连接到会话的回答回复，并在读者有决策或批注时显示回复条。插入会把汇总
// 回复追加到草稿；发送会追加后请输入区经正常发送路径发送草稿。
export function ComposerAnswerReplySlot(props: {
  // Whether draft mutation is temporarily excluded, such as during a queued prompt replacement.
  // 是否暂时禁止修改草稿，例如排队提示词替换期间。
  disabled: boolean;
  // Current draft text; a pending send waits for the render that carries the appended reply.
  // 当前草稿文字；待发送的回复等到携带它的那次渲染。
  draftInput: string;
  // The composer's normal send path, with every send barrier and the running-turn queue.
  // 输入区的正常发送路径，包含全部发送屏障与运行中轮次的排队。
  send: () => unknown;
  // Draft setter of the active composer owner.
  // 当前输入区归属的草稿设置器。
  setDraftInput: React.Dispatch<React.SetStateAction<string>>;
  // Prompt field focused after insertion.
  // 插入后聚焦的提示词输入框。
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
}) {
  const store = React.useContext(AnswerReplyContext);
  const count = React.useSyncExternalStore(store?.subscribe ?? noAnswerReplies, () => store?.size() ?? 0);
  const { disabled, draftInput, send, setDraftInput, textareaRef } = props;
  // sendPending marks a reply appended for direct sending until the draft carrying it is sent.
  // sendPending 标记为直接发送而追加的回复，直到携带它的草稿被发送。
  const sendPending = React.useRef(false);
  React.useEffect(() => {
    if (!sendPending.current || !draftInput.trim()) return;
    sendPending.current = false;
    void send();
  }, [draftInput, send]);
  React.useEffect(() => {
    if (!store || disabled) return undefined;
    const append = (text: string) => setDraftInput((current) => current.trim() ? `${current.trimEnd()}\n\n${text}` : text);
    return store.connectComposer({
      insert: (text) => {
        append(text);
        window.requestAnimationFrame(() => textareaRef.current?.focus({ preventScroll: true }));
      },
      send: (text) => {
        sendPending.current = true;
        append(text);
      },
    });
  }, [disabled, setDraftInput, store, textareaRef]);
  if (!store || count === 0 || disabled) return null;
  return (
    <React.Suspense fallback={null}>
      <LazyAnswerReplyTray store={store} />
    </React.Suspense>
  );
}
