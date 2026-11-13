import type { ConversationViewportAnchorSnapshot } from "./chat-area-scroll";

// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT is fired before shared shell geometry changes.
// CONVERSATION_LAYOUT_WILL_CHANGE_EVENT 在共享外壳几何布局变化前触发。
export const CONVERSATION_LAYOUT_WILL_CHANGE_EVENT = "vulcan:conversation-layout-will-change";

// CONVERSATION_LAYOUT_SETTLED_EVENT is fired after an animated transcript geometry transaction stabilizes.
// CONVERSATION_LAYOUT_SETTLED_EVENT 在会话动画几何事务稳定后触发。
export const CONVERSATION_LAYOUT_SETTLED_EVENT = "vulcan:conversation-layout-settled";

// CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT marks user fold actions that should preserve the clicked viewport.
// CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT 标记需要保留点击视口的用户折叠操作。
export const CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT = "vulcan:conversation-manual-viewport-interaction";

// CONVERSATION_AUTOMATIC_FOLD_EVENT marks a fold the transcript starts by itself, such as finished tools
// leaving a folded tool run, so a followed tail keeps its place until new content refills the space.
// CONVERSATION_AUTOMATIC_FOLD_EVENT 标记转录自行发起的折叠（例如已完成工具收回折叠组），
// 使锁底尾部保持原位，直到新内容填回空间。
export const CONVERSATION_AUTOMATIC_FOLD_EVENT = "vulcan:conversation-automatic-fold";

// A turn body swap can change estimated heights above the reader's viewport.
// 回合正文切换可能改变阅读视口上方的估算高度。
export const CONVERSATION_TURN_WINDOW_LAYOUT_EVENT = "vulcan:conversation-turn-window-layout";

// The scrollbar sends its track position to the transcript scroll owner.
// 滚动条将轨道位置传给转录滚动所有者。
export const CONVERSATION_SCROLLBAR_POSITION_EVENT = "vulcan:conversation-scrollbar-position";

// ConversationScrollbarPosition describes one pointer or keyboard scroll action.
// ConversationScrollbarPosition 描述一次指针或键盘滚动操作。
export interface ConversationScrollbarPosition {
  // Progress along the thumb's usable track, clamped to [0, 1].
  // 滑块在可用轨道上的进度，限制在 [0, 1]。
  progress: number;
  // Whether the pointer still owns the thumb.
  // 指针是否仍控制滑块。
  dragging: boolean;
}

// ConversationTurnWindowLayoutDetail carries the pre-swap anchor to the scroll owner.
// ConversationTurnWindowLayoutDetail 将切换前的锚点传给滚动所有者。
export interface ConversationTurnWindowLayoutDetail {
  // The shell and offset observed immediately before the body swap.
  // 正文切换前立即观察到的外壳及偏移。
  anchor: ConversationViewportAnchorSnapshot | null;
}

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

// dispatchConversationAutomaticFold announces a self-started fold before its content shrinks.
// dispatchConversationAutomaticFold 在自行发起的折叠收缩内容之前发出通知。
// Parameters: owner is any committed element inside the folding transcript.
// 参数：owner 是正在折叠的转录中任一已提交元素。
// Returns: nothing.
// 返回值：无。
export function dispatchConversationAutomaticFold(owner: Element): void {
  // surfaceId comes from the folding element itself, never from focus, because no control was used.
  // surfaceId 取自折叠元素本身而非焦点，因为此次折叠没有经过任何控件。
  const surfaceId = owner.closest<HTMLElement>("[data-conversation-surface-id]")?.dataset.conversationSurfaceId ?? null;
  window.dispatchEvent(new CustomEvent<ConversationManualViewportInteractionDetail>(
    CONVERSATION_AUTOMATIC_FOLD_EVENT,
    { detail: { surfaceId } },
  ));
}

// dispatchConversationManualViewportInteraction captures existing scroll ownership before a user fold action.
// dispatchConversationManualViewportInteraction 在用户折叠操作前捕获既有滚动所有权。
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
