import { describe, expect, it } from "vitest";
import {
  NOTIFICATION_PREFERENCES_STORAGE_KEY,
  defaultNotificationPreferences,
  loadNotificationPreferences,
  saveNotificationPreferences,
} from "@/notifications/preferences";

// MemoryStorage implements the exact storage methods used by preference persistence.
// MemoryStorage 实现偏好持久化使用的精确存储方法。
class MemoryStorage {
  // values stores serialized documents by exact local key.
  // values 按精确本地键保存序列化文档。
  private readonly values = new Map<string, string>();

  // getItem returns the serialized value or null when absent.
  // getItem 返回序列化值，不存在时返回 null。
  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  // setItem replaces one serialized value.
  // setItem 替换一个序列化值。
  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

describe("notification preferences", () => {
  it("persists and reloads a complete V1 document", () => {
    // storage isolates this test from browser state.
    // storage 将本测试与浏览器状态隔离。
    const storage = new MemoryStorage();
    // preferences is the complete V1 document modified by the test.
    // preferences 是本测试修改的完整 V1 文档。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    preferences.privacy.showConversationTitle = true;
    saveNotificationPreferences(preferences, storage);
    expect(loadNotificationPreferences(storage)).toEqual(preferences);
  });

  it("rejects malformed, unversioned, and future documents", () => {
    // storage isolates invalid schema cases by exact replacement.
    // storage 通过精确替换隔离无效结构场景。
    const storage = new MemoryStorage();
    storage.setItem(NOTIFICATION_PREFERENCES_STORAGE_KEY, JSON.stringify({ notificationsEnabled: true }));
    expect(loadNotificationPreferences(storage)).toEqual(defaultNotificationPreferences());
    storage.setItem(NOTIFICATION_PREFERENCES_STORAGE_KEY, JSON.stringify({ version: 2 }));
    expect(loadNotificationPreferences(storage)).toEqual(defaultNotificationPreferences());
    storage.setItem(NOTIFICATION_PREFERENCES_STORAGE_KEY, "{");
    expect(loadNotificationPreferences(storage)).toEqual(defaultNotificationPreferences());
  });
});
