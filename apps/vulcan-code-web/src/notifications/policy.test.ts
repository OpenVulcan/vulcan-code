import { describe, expect, it } from "vitest";
import { decideLocalNotification } from "@/notifications/policy";
import { defaultNotificationPreferences } from "@/notifications/preferences";
import type { NotificationIntentV1, NotificationPresentationContext } from "@/notifications/types";
import type { ShellNotificationPresentationState } from "@/shell";

// testIntent builds one complete notification intent for pure policy tests.
// testIntent 为纯策略测试构建一个完整通知意图。
function testIntent(
  // category selects the lifecycle preference branch under test.
  // category 选择待测试的生命周期偏好分支。
  category: NotificationIntentV1["category"] = "conversation_completed",
): NotificationIntentV1 {
  return {
    version: 1,
    intentId: "notification:event-1",
    sourceEventId: "event-1",
    category,
    conversationId: "conversation-1",
    projectId: "project-1",
    dedupeKey: "conversation-run:event-1",
    collapseKey: "conversation:conversation-1",
    priority: category === "conversation_failed" ? "high" : "normal",
    expiresAt: "9999999999999",
    route: { kind: "conversation", projectId: "project-1", conversationId: "conversation-1" },
    privacyLevel: "generic",
    localizationKey: category === "conversation_failed" ? "notifications.failed" : "notifications.completed",
    localizationArgs: {},
  };
}

// focusedShellState returns the foreground facts required for exact-conversation suppression.
// focusedShellState 返回精确会话抑制所需的前台事实。
function focusedShellState(): ShellNotificationPresentationState {
  return { foreground: true, focused: true, visible: true };
}

// exactPresentation returns a context that displays the intent conversation.
// exactPresentation 返回展示意图会话的界面上下文。
function exactPresentation(): NotificationPresentationContext {
  return { conversationViewActive: true, visibleConversationId: "conversation-1" };
}

describe("decideLocalNotification", () => {
  it("suppresses the exact focused conversation", () => {
    // preferences enables the local channel so focus policy is reached.
    // preferences 开启本地通道，使测试进入焦点策略。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      exactPresentation(),
      focusedShellState(),
      { shellKind: "browser", supported: true, activationSupported: true },
      "granted",
    )).toEqual({ allowed: false, reason: "exact_conversation_visible" });
  });

  it("allows another conversation, another feature, or a background shell", () => {
    // preferences enables the local channel for all eligible matrix cases.
    // preferences 为所有符合条件的矩阵场景开启本地通道。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    // capability is the supported browser shell fact shared by the matrix.
    // capability 是矩阵共享的浏览器壳支持事实。
    const capability = { shellKind: "browser", supported: true, activationSupported: true } as const;
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      { conversationViewActive: true, visibleConversationId: "conversation-2" },
      focusedShellState(),
      capability,
      "granted",
    ).allowed).toBe(true);
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      { conversationViewActive: false, visibleConversationId: null },
      focusedShellState(),
      capability,
      "granted",
    ).allowed).toBe(true);
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      exactPresentation(),
      { foreground: false, focused: false, visible: false },
      capability,
      "granted",
    ).allowed).toBe(true);
  });

  it("distinguishes disabled categories, denied permission, and unsupported shells", () => {
    // preferences starts with local delivery enabled and then exercises exact category switches.
    // preferences 初始开启本地投递，随后测试精确类别开关。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    preferences.categories.conversationCompleted = false;
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      exactPresentation(),
      focusedShellState(),
      { shellKind: "browser", supported: true, activationSupported: true },
      "granted",
    ).reason).toBe("completion_category_disabled");
    preferences.categories.conversationCompleted = true;
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      exactPresentation(),
      focusedShellState(),
      { shellKind: "browser", supported: true, activationSupported: true },
      "denied",
    ).reason).toBe("permission_denied");
    expect(decideLocalNotification(
      testIntent(),
      preferences,
      exactPresentation(),
      focusedShellState(),
      { shellKind: "browser", supported: false, activationSupported: false },
      "unsupported",
    ).reason).toBe("shell_unsupported");
  });

  it("suppresses expired or malformed intents using the explicit policy clock", () => {
    // preferences enables the local channel so expiration policy is reached.
    // preferences 开启本地通道，使测试进入过期策略。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    // intent expires immediately before the explicit policy clock.
    // intent 在显式策略时钟之前立即过期。
    const intent = testIntent();
    intent.expiresAt = "999";
    expect(decideLocalNotification(
      intent,
      preferences,
      exactPresentation(),
      focusedShellState(),
      { shellKind: "browser", supported: true, activationSupported: true },
      "granted",
      1000,
    )).toEqual({ allowed: false, reason: "intent_expired" });
    intent.expiresAt = "invalid";
    expect(decideLocalNotification(
      intent,
      preferences,
      exactPresentation(),
      focusedShellState(),
      { shellKind: "browser", supported: true, activationSupported: true },
      "granted",
      1000,
    )).toEqual({ allowed: false, reason: "intent_expired" });
  });
});
