import * as React from "react";
import {
  GeneratedMediaStrip,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import { useConversationToolRequestProgress } from "@/state/conversation-live-stream-context";
import {
  liveToolRequestProgress,
  preferredSubagentTaskDisplayBlock,
  toolExecutionDisplayState,
  toolExecutionMediaAttachments,
  toolExecutionName,
  toolRequestProgress,
  toolRequestStreamId,
} from "./conversation-tool-execution-model";
import type { ToolExecutionConversationBlock } from "./conversation-tool-execution-types";
import { fixedV1ToolEvent } from "./tool-presentations/presentation-value-readers";
import { ToolCallCard, ToolRequestCard } from "@/conversation/tools/tool-call-card";
import { ToolCardSessionContext } from "@/conversation/tools/tool-card";

export type {
  ToolExecutionConversationBlock,
  ToolExecutionConversationBlockKind,
  ToolExecutionTraceCallback,
} from "./conversation-tool-execution-types";
export { toolExecutionDisplayState } from "./conversation-tool-execution-model";

// ToolSequenceGroupedContext is true inside a tool run folded behind a sequence head. Cards there start closed on
// every mount and ignore the expansion they had as standalone cards, so opening the run never reopens its cards.
// ToolSequenceGroupedContext 在折叠到序列标题之后的工具序列内为 true。其中的卡片每次挂载都从收起开始，且不沿用
// 作为独立卡片时的展开状态，因此展开序列不会重新展开其中的卡片。
export const ToolSequenceGroupedContext = React.createContext(false);

// ToolExecutionBlock renders one tool row: the card of a durable call, or of a request the model is still writing,
// followed by any media the call generated.
// ToolExecutionBlock 渲染单条工具行：可靠调用或模型仍在书写的请求所对应的卡片，以及该调用生成的媒体。
export function ToolExecutionBlock(props: {
  // Tool-related blocks merged into one row.
  // 合并成单条记录的工具相关块。
  blocks: ToolExecutionConversationBlock[];
  // Conversation id whose live request store feeds a request still being written.
  // 其实时请求存储为仍在书写的请求提供数据的会话标识。
  conversationId: string | null;
  // WebUI-session expansion memory for tool cards.
  // 工具卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Records one tool card expansion state change.
  // 记录一次工具卡片展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  const primaryBlock = preferredSubagentTaskDisplayBlock(props.blocks)
    ?? props.blocks.find((block) => block.kind === "tool-result")
    ?? props.blocks.find((block) => block.kind === "command")
    ?? props.blocks.find((block) => block.kind === "tool-call")
    ?? props.blocks[0];
  const displayState = toolExecutionDisplayState(props.blocks, props.sessionRunning);
  // retainedRequestProgress supplies the durable Started or Ready identity used to bind the leaf store.
  // retainedRequestProgress 提供用于绑定叶子存储的持久 Started 或 Ready 身份。
  const retainedRequestProgress = toolRequestProgress(props.blocks);
  // liveRequestProgress changes only this card subtree while argument tokens stream.
  // liveRequestProgress 在参数 Token 流期间仅改变当前卡片子树。
  const liveRequestProgress = useConversationToolRequestProgress(props.conversationId, toolRequestStreamId(props.blocks));
  const requestProgress = retainedRequestProgress && liveRequestProgress
    ? liveToolRequestProgress(liveRequestProgress)
    : retainedRequestProgress;
  const v1ToolEvent = fixedV1ToolEvent(props.blocks);
  const requestToolName = primaryBlock?.toolName ?? toolExecutionName(props.blocks);
  const mediaAttachments = React.useMemo(() => toolExecutionMediaAttachments(props.blocks), [props.blocks]);
  // cardKey keys expansion memory by the request stream, which a request keeps when it becomes a durable call, so a
  // card opened while the model writes stays open.
  // cardKey 以请求流作为展开记忆的键；请求成为可靠调用后仍保留该标识，因此模型书写时展开的卡片会保持展开。
  const cardKey = v1ToolEvent
    ? `tool:${v1ToolEvent.request_stream_id}`
    : requestProgress && requestToolName ? `tool:${requestProgress.requestStreamId}` : null;
  // localOpen applies a click immediately while the surface memory updates as a transition.
  // localOpen 立即应用点击，界面记忆则以过渡任务更新。
  const [localOpen, setLocalOpen] = React.useState<boolean | null>(null);
  const grouped = React.useContext(ToolSequenceGroupedContext);
  const open = localOpen ?? (!grouped && cardKey !== null && props.expandedToolPresentationIds.has(cardKey));
  const { onToolPresentationExpandedChange } = props;
  const handleOpenChange = React.useCallback((next: boolean) => {
    if (!cardKey) return;
    setLocalOpen(next);
    React.startTransition(() => onToolPresentationExpandedChange(cardKey, next));
  }, [cardKey, onToolPresentationExpandedChange]);
  const card = v1ToolEvent && cardKey ? (
    <ToolCardSessionContext.Provider value={props.conversationId}>
      <ToolCallCard event={v1ToolEvent} onOpenChange={handleOpenChange} open={open} />
    </ToolCardSessionContext.Provider>
  ) : requestProgress && requestToolName && cardKey ? (
    <ToolRequestCard
      onOpenChange={handleOpenChange}
      open={open}
      phase={requestProgress.state}
      preview={requestProgress.preview}
      receivedBytes={requestProgress.receivedBytes}
      requestStreamId={requestProgress.requestStreamId}
      state={displayState === "completed" ? "done" : displayState}
      toolName={requestToolName}
    />
  ) : null;
  if (!card && mediaAttachments.length === 0) return null;
  return (
    <div className="vk-tool-stack">
      {card}
      {mediaAttachments.length > 0 ? (
        <GeneratedMediaStrip
          attachments={mediaAttachments}
          onOpenDetail={() => {
            const firstAttachment = mediaAttachments[0];
            if (firstAttachment) props.onOpenMedia(firstAttachment);
          }}
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
    </div>
  );
}
