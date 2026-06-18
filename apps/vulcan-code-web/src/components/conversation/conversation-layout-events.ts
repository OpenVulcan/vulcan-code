// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT is fired before shared shell geometry changes.
// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT 在共享外壳几何布局变化前触发。
export const CONVERSATION_LAYOUT_WILL_CHANGE_EVENT = "vulcan:conversation-layout-will-change";

// dispatchConversationLayoutWillChange notifies the active transcript to preserve its viewport.
// dispatchConversationLayoutWillChange 通知当前转录界面保留视口位置。
export function dispatchConversationLayoutWillChange(): void {
  window.dispatchEvent(new Event(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT));
}
