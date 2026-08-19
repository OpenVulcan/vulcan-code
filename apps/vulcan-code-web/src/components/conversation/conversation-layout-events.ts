// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT is fired before shared shell geometry changes.
// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT 在共享外壳几何布局变化前触发。
export const CONVERSATION_LAYOUT_WILL_CHANGE_EVENT = "vulcan:conversation-layout-will-change";

// CONVERSATION_LAYOUT_SETTLED_EVENT is fired after an animated transcript geometry transaction stabilizes.
// CONVERSATION_LAYOUT_SETTLED_EVENT 在会话动画几何事务稳定后触发。
export const CONVERSATION_LAYOUT_SETTLED_EVENT = "vulcan:conversation-layout-settled";

// CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT marks user fold actions that should preserve the clicked viewport.
// CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT 标记需要保留点击视口的用户折叠操作。
export const CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT = "vulcan:conversation-manual-viewport-interaction";

// ConversationManualViewportInteractionDetail identifies the exact transcript owning a manual fold action.
// ConversationManualViewportInteractionDetail 标识拥有手动折叠操作的精确转录界面。
export interface ConversationManualViewportInteractionDetail {
  // Stable conversation or child-task surface id resolved from the focused control.
  // 从聚焦控件解析出的稳定会话或子任务界面标识。
  surfaceId: string | null;
}

// ConversationLayoutSettledDetail identifies the exact transcript whose animation stabilized.
// ConversationLayoutSettledDetail 标识动画已经稳定的精确转录界面。
export interface ConversationLayoutSettledDetail {
  // Stable conversation or child-task surface id owning the completed geometry transaction.
  // 持有已完成几何事务的稳定会话或子任务界面标识。
  surfaceId: string | null;
}

// dispatchConversationLayoutWillChange notifies the active transcript to preserve its viewport.
// dispatchConversationLayoutWillChange 通知当前转录界面保留视口位置。
export function dispatchConversationLayoutWillChange(): void {
  window.dispatchEvent(new Event(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT));
}

// dispatchConversationLayoutSettled closes the active transcript geometry transaction.
// dispatchConversationLayoutSettled 结束当前活动会话几何事务。
// Parameters: surfaceId identifies the transcript whose animated geometry stabilized.
// 参数：surfaceId 标识动画几何已经稳定的转录界面。
// Returns: nothing.
// 返回值：无。
export function dispatchConversationLayoutSettled(surfaceId: string | null): void {
  window.dispatchEvent(new CustomEvent<ConversationLayoutSettledDetail>(
    CONVERSATION_LAYOUT_SETTLED_EVENT,
    { detail: { surfaceId } },
  ));
}

// dispatchConversationManualViewportInteraction tells transcript auto-follow to yield to a user fold action.
// dispatchConversationManualViewportInteraction 通知转录自动追随让位给用户折叠操作。
// Parameters: none; the active control resolves its exact owning transcript from the committed DOM.
// 参数：无；活动控件通过已提交 DOM 解析其所属的精确转录界面。
// Returns: nothing.
// 返回值：无。
export function dispatchConversationManualViewportInteraction(): void {
  // activeElement is the concrete control that initiated the synchronous fold handler.
  // activeElement 是发起同步折叠处理的具体控件。
  const activeElement = document.activeElement instanceof Element ? document.activeElement : null;
  // surfaceId prevents a child transcript action from changing the main transcript's ownership ledger.
  // surfaceId 防止子转录操作改变主转录的所有权账本。
  const surfaceId = activeElement
    ?.closest<HTMLElement>("[data-conversation-surface-id]")
    ?.dataset.conversationSurfaceId ?? null;
  window.dispatchEvent(new CustomEvent<ConversationManualViewportInteractionDetail>(
    CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT,
    { detail: { surfaceId } },
  ));
}
