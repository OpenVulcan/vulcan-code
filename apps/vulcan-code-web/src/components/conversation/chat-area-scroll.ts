import type { ConversationMessage } from "@/types";

// STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT limits tail-follow signatures to messages that can affect the viewport tail.
// STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT 将尾部追随签名限制在会影响视口尾部的消息范围。
const STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT = 8;

// ConversationScrollSnapshot stores one transcript surface scroll restoration point.
// ConversationScrollSnapshot 保存单个转录界面的滚动恢复点。
export interface ConversationScrollSnapshot {
  // Distance from the visual viewport bottom to the transcript bottom.
  // 从视觉视口底部到转录底部的距离。
  distanceFromBottom: number;
  // Whether this surface should continue following newly appended content.
  // 该界面是否应继续跟随新追加的内容。
  wasAtBottom: boolean;
}

// conversationDistanceFromBottom returns the scroll metric used by the sticky-footer transcript.
// conversationDistanceFromBottom 返回粘性底部转录布局使用的滚动距离指标。
export function conversationDistanceFromBottom(element: HTMLElement): number {
  return Math.max(0, element.scrollHeight - element.scrollTop - element.clientHeight);
}

// conversationScrollToDistanceFromBottom restores a visual bottom distance in the current layout.
// conversationScrollToDistanceFromBottom 在当前布局中恢复视觉底部距离。
export function conversationScrollToDistanceFromBottom(element: HTMLElement, distanceFromBottom: number): void {
  const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
  element.scrollTop = Math.max(0, Math.min(maxScrollTop, element.scrollHeight - element.clientHeight - distanceFromBottom));
}

// buildConversationStreamSignature returns a compact content signature for tail-follow decisions.
// buildConversationStreamSignature 返回用于尾部追随判定的紧凑内容签名。
export function buildConversationStreamSignature(messages: ConversationMessage[]): string {
  return messages
    .slice(-STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT)
    .map((message) => {
      const bodyTail = message.body.slice(-48);
      return `${message.id}:${message.recordKind ?? ""}:${message.body.length}:${bodyTail}`;
    })
    .join("|");
}

// conversationViewportAllowsTailFollow returns whether the timeline is at or near the bottom.
// conversationViewportAllowsTailFollow 返回时间线是否位于底部或接近底部。
export function conversationViewportAllowsTailFollow(element: HTMLElement): boolean {
  const distanceFromBottom = conversationDistanceFromBottom(element);
  return element.scrollHeight <= element.clientHeight + 24 || distanceFromBottom <= 24;
}
