// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT is fired before shared shell geometry changes.
// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT 在共享外壳几何布局变化前触发。
export const CONVERSATION_LAYOUT_WILL_CHANGE_EVENT = "vulcan:conversation-layout-will-change";

// CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT marks user fold actions that should preserve the clicked viewport.
// CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT 标记需要保留点击视口的用户折叠操作。
export const CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT = "vulcan:conversation-manual-viewport-interaction";

// dispatchConversationLayoutWillChange notifies the active transcript to preserve its viewport.
// dispatchConversationLayoutWillChange 通知当前转录界面保留视口位置。
export function dispatchConversationLayoutWillChange(): void {
  window.dispatchEvent(new Event(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT));
}

// dispatchConversationManualViewportInteraction tells transcript auto-follow to yield to a user fold action.
// dispatchConversationManualViewportInteraction 通知转录自动追随让位给用户折叠操作。
export function dispatchConversationManualViewportInteraction(): void {
  window.dispatchEvent(new Event(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT));
}
