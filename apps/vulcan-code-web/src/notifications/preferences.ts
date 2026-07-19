import type { NotificationPreferencesV1 } from "@/notifications/types";

// NOTIFICATION_PREFERENCES_STORAGE_KEY isolates device-local choices from future account synchronization.
// NOTIFICATION_PREFERENCES_STORAGE_KEY 将设备本地选择与未来账号同步隔离。
export const NOTIFICATION_PREFERENCES_STORAGE_KEY = "vulcan.notification-preferences.local.v1";

// defaultNotificationPreferences returns a fresh safe default preference document.
// defaultNotificationPreferences 返回一份全新的安全默认偏好文档。
export function defaultNotificationPreferences(): NotificationPreferencesV1 {
  return {
    version: 1,
    channels: { local: { enabled: false } },
    categories: {
      conversationCompleted: true,
      conversationFailed: true,
    },
    privacy: { showConversationTitle: false },
  };
}

// loadNotificationPreferences validates one local document and falls back to safe defaults when invalid.
// loadNotificationPreferences 校验本地文档，并在无效时回退到安全默认值。
export function loadNotificationPreferences(storage: Pick<Storage, "getItem"> = window.localStorage): NotificationPreferencesV1 {
  try {
    // serialized stores the exact device-local preference document.
    // serialized 保存精确的设备本地偏好文档。
    const serialized = storage.getItem(NOTIFICATION_PREFERENCES_STORAGE_KEY);
    if (serialized === null) {
      return defaultNotificationPreferences();
    }
    // candidate remains unknown until every V1 field passes strict validation.
    // candidate 在全部 V1 字段通过严格校验前保持未知类型。
    const candidate: unknown = JSON.parse(serialized);
    if (!isNotificationPreferencesV1(candidate)) {
      return defaultNotificationPreferences();
    }
    return candidate;
  } catch {
    return defaultNotificationPreferences();
  }
}

// saveNotificationPreferences writes one validated V1 document to device-local storage.
// saveNotificationPreferences 将一份已校验的 V1 文档写入设备本地存储。
export function saveNotificationPreferences(
  // preferences is the complete replacement preference document.
  // preferences 是完整替换用的偏好文档。
  preferences: NotificationPreferencesV1,
  // storage is the device-local storage boundary.
  // storage 是设备本地存储边界。
  storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
  storage.setItem(NOTIFICATION_PREFERENCES_STORAGE_KEY, JSON.stringify(preferences));
}

// isNotificationPreferencesV1 performs exact runtime validation for the current local schema.
// isNotificationPreferencesV1 对当前本地结构执行精确运行时校验。
function isNotificationPreferencesV1(value: unknown): value is NotificationPreferencesV1 {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  // candidate exposes only the known V1 object positions after the top-level object check.
  // candidate 在顶层对象检查后只暴露已知 V1 对象位置。
  const candidate = value as Record<string, unknown>;
  // channels is validated as an object before reading the local channel.
  // channels 在读取本地通道前先作为对象校验。
  const channels = candidate.channels;
  // categories is validated as an object before reading category switches.
  // categories 在读取类别开关前先作为对象校验。
  const categories = candidate.categories;
  // privacy is validated as an object before reading privacy switches.
  // privacy 在读取隐私开关前先作为对象校验。
  const privacy = candidate.privacy;
  if (typeof channels !== "object" || channels === null
    || typeof categories !== "object" || categories === null
    || typeof privacy !== "object" || privacy === null) {
    return false;
  }
  // local is the only current delivery channel document.
  // local 是当前唯一的投递通道文档。
  const local = (channels as Record<string, unknown>).local;
  return candidate.version === 1
    && typeof local === "object"
    && local !== null
    && typeof (local as Record<string, unknown>).enabled === "boolean"
    && typeof (categories as Record<string, unknown>).conversationCompleted === "boolean"
    && typeof (categories as Record<string, unknown>).conversationFailed === "boolean"
    && typeof (privacy as Record<string, unknown>).showConversationTitle === "boolean";
}
