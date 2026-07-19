import type {
  NotificationIntentV1,
  NotificationPreferencesV1,
  NotificationPresentationContext,
} from "@/notifications/types";
import type {
  ShellNotificationCapability,
  ShellNotificationPermission,
  ShellNotificationPresentationState,
} from "@/shell";

// LocalNotificationPolicyDecision describes a pure endpoint policy result.
// LocalNotificationPolicyDecision 描述纯本地端点策略结果。
export interface LocalNotificationPolicyDecision {
  // allowed indicates whether the endpoint may show a system notification.
  // allowed 表示端点是否可以展示系统通知。
  allowed: boolean;
  // reason is a stable diagnostic code.
  // reason 是稳定的诊断代码。
  reason: string;
}

// decideLocalNotification evaluates preferences, capability, permission, focus, and exact transcript context.
// decideLocalNotification 评估偏好、能力、权限、焦点与精确转录上下文。
export function decideLocalNotification(
  // intent identifies the lifecycle category and affected conversation.
  // intent 标识生命周期类别与受影响会话。
  intent: NotificationIntentV1,
  // preferences contains current per-client local choices.
  // preferences 包含当前客户端本地选择。
  preferences: NotificationPreferencesV1,
  // presentation identifies the exact transcript surface currently shown.
  // presentation 标识当前真实展示的转录界面。
  presentation: NotificationPresentationContext,
  // shellState contains current host focus and visibility facts.
  // shellState 包含当前宿主焦点与可见性事实。
  shellState: ShellNotificationPresentationState,
  // capability reports whether the current shell can show notifications.
  // capability 报告当前壳是否能够展示通知。
  capability: ShellNotificationCapability,
  // permission is the current shell notification authorization state.
  // permission 是当前壳通知授权状态。
  permission: ShellNotificationPermission,
  // nowMs is the client clock used only to reject expired delivery.
  // nowMs 是仅用于拒绝过期投递的客户端时钟。
  nowMs: number = Date.now(),
): LocalNotificationPolicyDecision {
  if (!preferences.channels.local.enabled) {
    return { allowed: false, reason: "local_channel_disabled" };
  }
  // expiresAtMs is derived from the service occurrence time by the versioned intent builder.
  // expiresAtMs 由版本化意图构建器根据服务端发生时间派生。
  const expiresAtMs = Number(intent.expiresAt);
  if (!Number.isSafeInteger(expiresAtMs) || expiresAtMs <= nowMs) {
    return { allowed: false, reason: "intent_expired" };
  }
  if (intent.category === "conversation_completed" && !preferences.categories.conversationCompleted) {
    return { allowed: false, reason: "completion_category_disabled" };
  }
  if (intent.category === "conversation_failed" && !preferences.categories.conversationFailed) {
    return { allowed: false, reason: "failure_category_disabled" };
  }
  if (!capability.supported) {
    return { allowed: false, reason: "shell_unsupported" };
  }
  if (permission !== "granted") {
    return { allowed: false, reason: permission === "unsupported" ? "shell_unsupported" : "permission_denied" };
  }
  // The notification is redundant only when this client is foregrounded, focused, and showing the exact conversation.
  // 仅当当前客户端位于前台、拥有焦点且正展示精确会话时，通知才是冗余的。
  const exactConversationIsVisible = shellState.foreground
    && shellState.focused
    && shellState.visible
    && presentation.conversationViewActive
    && presentation.visibleConversationId === intent.conversationId;
  if (exactConversationIsVisible) {
    return { allowed: false, reason: "exact_conversation_visible" };
  }
  return { allowed: true, reason: "eligible" };
}
