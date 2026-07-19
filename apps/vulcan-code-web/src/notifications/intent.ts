import type { ConversationRunSettledEvent } from "@/types";
import type { NotificationIntentV1 } from "@/notifications/types";

// NOTIFICATION_INTENT_TTL_MS limits delayed local or future remote delivery to one day.
// NOTIFICATION_INTENT_TTL_MS 将延迟的本地或未来远程投递限制在一天内。
const NOTIFICATION_INTENT_TTL_MS = 24 * 60 * 60 * 1000;

// createConversationNotificationIntent converts one settled lifecycle event into a platform-neutral intent.
// createConversationNotificationIntent 将一个终态生命周期事件转换为与平台无关的通知意图。
export function createConversationNotificationIntent(
  // event is the authoritative platform-neutral service event.
  // event 是权威且与平台无关的服务事件。
  event: ConversationRunSettledEvent,
  // conversationTitle is included only when the current privacy preference permits it.
  // conversationTitle 仅在当前隐私偏好允许时包含。
  conversationTitle: string | null,
): NotificationIntentV1 {
  // category maps the strict service outcome to the independently configurable notification class.
  // category 将严格服务结果映射为可独立配置的通知类别。
  const category = event.outcome === "completed" ? "conversation_completed" : "conversation_failed";
  // normalizedTitle excludes blank titles from privacy-bearing intent arguments.
  // normalizedTitle 从携带隐私信息的意图参数中排除空白标题。
  const normalizedTitle = conversationTitle?.trim() || null;
  // occurredAtMs preserves the service-owned event time as the single expiration source.
  // occurredAtMs 将服务端事件时间保留为唯一过期来源。
  const occurredAtMs = Number(event.occurredAt);
  if (!Number.isSafeInteger(occurredAtMs) || occurredAtMs < 0) {
    throw new Error("Conversation run settlement occurredAt must be a non-negative safe integer.");
  }
  return {
    version: 1,
    intentId: `notification:${event.eventId}`,
    sourceEventId: event.eventId,
    category,
    conversationId: event.conversationId,
    projectId: event.projectId ?? null,
    dedupeKey: `conversation-run:${event.eventId}`,
    collapseKey: `conversation:${event.conversationId}`,
    priority: event.outcome === "failed" ? "high" : "normal",
    expiresAt: String(occurredAtMs + NOTIFICATION_INTENT_TTL_MS),
    route: {
      kind: "conversation",
      projectId: event.projectId ?? null,
      conversationId: event.conversationId,
    },
    privacyLevel: normalizedTitle === null ? "generic" : "conversation_title",
    localizationKey: event.outcome === "completed" ? "notifications.completed" : "notifications.failed",
    localizationArgs: normalizedTitle === null ? {} : { conversationTitle: normalizedTitle },
  };
}
