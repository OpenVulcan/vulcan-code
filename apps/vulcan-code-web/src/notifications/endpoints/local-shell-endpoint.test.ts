import { describe, expect, it } from "vitest";
import { LocalShellNotificationEndpoint } from "@/notifications/endpoints/local-shell-endpoint";
import { createConversationNotificationIntent } from "@/notifications/intent";
import { defaultNotificationPreferences } from "@/notifications/preferences";
import { NotificationTestShell } from "@/notifications/test-support/notification-test-shell";

describe("LocalShellNotificationEndpoint", () => {
  it("delivers in background and suppresses denied or exact focused contexts", async () => {
    // shell provides controlled endpoint facts.
    // shell 提供受控端点事实。
    const shell = new NotificationTestShell();
    shell.presentationState = { foreground: true, focused: false, visible: true };
    // endpoint is bound to the controlled shell.
    // endpoint 绑定到受控壳。
    const endpoint = new LocalShellNotificationEndpoint(shell);
    // preferences enables the current local channel.
    // preferences 开启当前本地通道。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    // intent is the exact completed conversation request under test.
    // intent 是待测试的精确会话完成请求。
    const intent = createConversationNotificationIntent({
      eventId: "event-1",
      conversationId: "conversation-1",
      projectId: null,
      runId: "run-1",
      outcome: "completed",
      occurredAt: String(Date.now()),
    }, null);
    // resolveText provides deterministic localized presentation text.
    // resolveText 提供确定的本地化展示文案。
    const resolveText = () => ({ title: "Vulcan Code", body: "Completed" });
    expect((await endpoint.deliver(
      intent,
      preferences,
      { conversationViewActive: true, visibleConversationId: "conversation-1" },
      resolveText,
    )).status).toBe("delivered");
    shell.presentationState = { foreground: true, focused: true, visible: true };
    expect((await endpoint.deliver(
      intent,
      preferences,
      { conversationViewActive: true, visibleConversationId: "conversation-1" },
      resolveText,
    )).reason).toBe("exact_conversation_visible");
    shell.permission = "denied";
    expect((await endpoint.deliver(
      intent,
      preferences,
      { conversationViewActive: false, visibleConversationId: null },
      resolveText,
    )).status).toBe("permissionDenied");
    expect(shell.requests).toHaveLength(1);
  });
});
