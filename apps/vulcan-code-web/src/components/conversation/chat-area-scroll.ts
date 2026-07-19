import type { ConversationMessage } from "@/types";

// STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT limits tail-follow signatures to messages that can affect the viewport tail.
// STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT 将尾部追随签名限制在会影响视口尾部的消息范围。
const STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT = 8;

// ConversationTimelineUserScrollDirection describes the latest user-driven scroll direction.
// ConversationTimelineUserScrollDirection 描述最近一次用户驱动滚动的方向。
export type ConversationTimelineUserScrollDirection = "up" | "down" | "unknown";

// ConversationScrollSnapshot stores one transcript surface scroll restoration point.
// ConversationScrollSnapshot 保存单个转录界面的滚动恢复点。
export interface ConversationScrollSnapshot {
  // First visible row anchor captured with this scroll point.
  // 与该滚动点一起捕获的首个可见行锚点。
  anchor?: ConversationViewportAnchorSnapshot | null;
  // Distance from the visual viewport bottom to the transcript bottom.
  // 从视觉视口底部到转录底部的距离。
  distanceFromBottom: number;
  // Whether this surface should continue following newly appended content.
  // 该界面是否应继续跟随新追加的内容。
  wasAtBottom: boolean;
}

// ConversationViewportAnchorSnapshot stores one visible row anchor before layout changes.
// ConversationViewportAnchorSnapshot 保存布局变化前的一条可见行锚点。
export interface ConversationViewportAnchorSnapshot {
  // Stable row key captured from the measured timeline row.
  // 从已测量时间线行捕获的稳定行键。
  rowKey: string;
  // Row top offset relative to the scroll viewport top.
  // 行顶部相对滚动视口顶部的偏移。
  offsetTop: number;
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

// captureConversationViewportAnchor captures the first visible virtualized timeline row.
// captureConversationViewportAnchor 捕获第一条可见的虚拟时间线行。
export function captureConversationViewportAnchor(element: HTMLElement): ConversationViewportAnchorSnapshot | null {
  const viewportRect = element.getBoundingClientRect();
  const rows = element.querySelectorAll<HTMLElement>("[data-conversation-row-key]");
  for (const row of rows) {
    const rowKey = row.dataset.conversationRowKey;
    if (!rowKey) {
      continue;
    }
    const rowRect = row.getBoundingClientRect();
    if (rowRect.bottom < viewportRect.top || rowRect.top > viewportRect.bottom) {
      continue;
    }
    return {
      offsetTop: rowRect.top - viewportRect.top,
      rowKey,
    };
  }
  return null;
}

// restoreConversationViewportAnchor restores a captured row to the same visual offset.
// restoreConversationViewportAnchor 将捕获的行恢复到相同视觉偏移。
export function restoreConversationViewportAnchor(
  element: HTMLElement,
  anchor: ConversationViewportAnchorSnapshot,
): boolean {
  const viewportRect = element.getBoundingClientRect();
  const rows = element.querySelectorAll<HTMLElement>("[data-conversation-row-key]");
  for (const row of rows) {
    if (row.dataset.conversationRowKey !== anchor.rowKey) {
      continue;
    }
    const rowRect = row.getBoundingClientRect();
    element.scrollTop += rowRect.top - viewportRect.top - anchor.offsetTop;
    return true;
  }
  return false;
}

// restoreConversationScrollSnapshot restores one complete viewport snapshot with deterministic fallback order.
// restoreConversationScrollSnapshot 按确定回退顺序恢复一个完整视口快照。
// Parameters: element is the owning scroll viewport; snapshot carries tail-follow, anchor, and bottom-distance state.
// 参数：element 是所属滚动视口；snapshot 携带尾随、锚点与底部距离状态。
// Returns: nothing; the function restores the tail, then the anchor, then the saved bottom distance as fallback.
// 返回值：无；函数依次恢复尾部、锚点，最后以已保存底部距离回退。
export function restoreConversationScrollSnapshot(
  element: HTMLElement,
  snapshot: ConversationScrollSnapshot,
): void {
  if (snapshot.wasAtBottom) {
    element.scrollTop = element.scrollHeight;
    return;
  }
  if (snapshot.anchor && restoreConversationViewportAnchor(element, snapshot.anchor)) {
    return;
  }
  conversationScrollToDistanceFromBottom(element, snapshot.distanceFromBottom);
}

// buildConversationStreamSignature returns a structural signature without reading cumulative stream bodies.
// buildConversationStreamSignature 返回不读取累计流正文的结构签名。
export function buildConversationStreamSignature(messages: ConversationMessage[]): string {
  return messages
    .slice(-STREAM_SIGNATURE_TAIL_MESSAGE_LIMIT)
    .map((message) => `${message.id}:${message.recordKind ?? ""}:${message.role}`)
    .join("|");
}

// conversationViewportAllowsTailFollow returns whether the timeline is at or near the bottom.
// conversationViewportAllowsTailFollow 返回时间线是否位于底部或接近底部。
export function conversationViewportAllowsTailFollow(element: HTMLElement): boolean {
  const distanceFromBottom = conversationDistanceFromBottom(element);
  return element.scrollHeight <= element.clientHeight + 24 || distanceFromBottom <= 24;
}
