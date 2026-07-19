import type { NotificationPresentationContext } from "@/notifications/types";

// inactiveNotificationPresentationContext returns the canonical non-conversation surface state.
// inactiveNotificationPresentationContext 返回规范的非会话界面状态。
export function inactiveNotificationPresentationContext(): NotificationPresentationContext {
  return { conversationViewActive: false, visibleConversationId: null };
}

// currentPresentationContext stores the latest exact transcript surface for this client only.
// currentPresentationContext 仅为当前客户端保存最近的精确转录界面。
let currentPresentationContext = inactiveNotificationPresentationContext();

// setNotificationPresentationContext replaces the current client presentation snapshot.
// setNotificationPresentationContext 替换当前客户端界面快照。
export function setNotificationPresentationContext(context: NotificationPresentationContext): void {
  currentPresentationContext = context;
}

// getNotificationPresentationContext returns the latest client presentation snapshot.
// getNotificationPresentationContext 返回当前客户端最近的界面快照。
export function getNotificationPresentationContext(): NotificationPresentationContext {
  return currentPresentationContext;
}

// resetNotificationPresentationContext clears presentation state when the owning surface unmounts.
// resetNotificationPresentationContext 在所属界面卸载时清空界面状态。
export function resetNotificationPresentationContext(): void {
  currentPresentationContext = inactiveNotificationPresentationContext();
}
