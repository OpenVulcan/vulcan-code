import React from "react";

import { TransientReasoningStatusLine } from "@/components/conversation/conversation-live-reasoning";
import { ProviderRetryCountdownLine } from "@/components/conversation/conversation-provider-retry-card";
import {
  useConversationProviderRetryStatus,
  useConversationTransientReasoningStatus,
} from "@/state/conversation-live-stream-context";

/** Renders the transcript-tail status line of one conversation from the live stream store.
 * 从实时流存储渲染单个会话的转录尾部状态行。
 *
 * The changing content (reasoning token count, retry deadline) is subscribed here, so its updates redraw only this
 * line and never the transcript or ChatArea. A pending retry wait outranks the reasoning status.
 * 变化中的内容（思考 token 数、重试截止时间）在此订阅，因此其更新只重绘这一行，绝不重绘转录或 ChatArea。
 * 等待中的重试优先于思考状态。
 *
 * @param props.conversationId the exact conversation whose tail status is shown.
 *                             显示其尾部状态的精确会话。
 * @returns the active status line, or nothing when neither status is present.
 *          当前状态行；两种状态都不存在时不渲染任何内容。 */
export const ConversationTailStatus = React.memo(function ConversationTailStatus(props: { conversationId: string }) {
  const providerRetryStatus = useConversationProviderRetryStatus(props.conversationId);
  const transientReasoningStatus = useConversationTransientReasoningStatus(props.conversationId);
  if (providerRetryStatus) return <ProviderRetryCountdownLine countdown={providerRetryStatus} />;
  if (transientReasoningStatus) return <TransientReasoningStatusLine status={transientReasoningStatus} />;
  return null;
});
