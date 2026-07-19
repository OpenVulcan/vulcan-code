import { describe, expect, it, vi } from "vitest";
import { NotificationCoordinator } from "@/notifications/coordinator";
import { defaultNotificationPreferences } from "@/notifications/preferences";
import { NotificationTestShell } from "@/notifications/test-support/notification-test-shell";
import type {
  NotificationDeliveryEndpoint,
  NotificationDeliveryResult,
  NotificationIntentV1,
} from "@/notifications/types";
import type { ConversationRunSettledEvent } from "@/types";

// RecordingEndpoint captures every intent delivered by the coordinator.
// RecordingEndpoint 捕获协调器投递的每个意图。
class RecordingEndpoint implements NotificationDeliveryEndpoint {
  // kind identifies the current local test endpoint.
  // kind 标识当前本地测试端点。
  readonly kind = "local" as const;
  // intents stores delivered intent snapshots.
  // intents 保存已投递意图快照。
  readonly intents: NotificationIntentV1[] = [];

  // deliver records one intent and returns a deterministic success result.
  // deliver 记录一个意图并返回确定的成功结果。
  async deliver(intent: NotificationIntentV1): Promise<NotificationDeliveryResult> {
    this.intents.push(intent);
    return { endpointKind: this.kind, status: "delivered", reason: "delivered" };
  }
}

// testSettledEvent builds one exact service event for coordinator tests.
// testSettledEvent 为协调器测试构建一个精确服务事件。
function testSettledEvent(): ConversationRunSettledEvent {
  return {
    eventId: "event-1",
    conversationId: "conversation-1",
    projectId: "project-1",
    runId: "run-1",
    outcome: "completed",
    occurredAt: "1000",
  };
}

describe("NotificationCoordinator", () => {
  it("retries activation subscription after a transient startup failure", async () => {
    // shell fails exactly one activation subscription attempt.
    // shell 精确失败一次激活订阅尝试。
    const shell = new NotificationTestShell();
    shell.failNextActivationSubscription = true;
    // coordinator has no delivery endpoints because this test isolates activation startup.
    // coordinator 不含投递端点，因为本测试隔离激活启动。
    const coordinator = new NotificationCoordinator(shell, []);
    await expect(coordinator.start()).rejects.toThrow("transient activation failure");
    await expect(coordinator.start()).resolves.toBeUndefined();
    expect(shell.activationSubscriptionAttempts).toBe(2);
  });

  it("deduplicates repeated service events and routes browser activation", async () => {
    // shell exposes activation without relying on browser globals.
    // shell 在不依赖浏览器全局对象的情况下暴露激活能力。
    const shell = new NotificationTestShell();
    // endpoint records the unique dispatch count.
    // endpoint 记录唯一分发次数。
    const endpoint = new RecordingEndpoint();
    // preferences enables local delivery and title privacy for intent verification.
    // preferences 开启本地投递与标题隐私，用于意图验证。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    preferences.privacy.showConversationTitle = true;
    // coordinator uses injected client state readers for deterministic isolation.
    // coordinator 使用注入的客户端状态读取器实现确定性隔离。
    const coordinator = new NotificationCoordinator(
      shell,
      [endpoint],
      () => preferences,
      () => ({ conversationViewActive: false, visibleConversationId: null }),
    );
    // routeHandler records the platform-neutral route activated by the shell.
    // routeHandler 记录壳激活的与平台无关路由。
    const routeHandler = vi.fn();
    coordinator.setRouteHandler(routeHandler);
    await coordinator.start();
    // resolveText is not invoked by the recording endpoint but satisfies the endpoint contract.
    // resolveText 不会被记录端点调用，但满足端点契约。
    const resolveText = () => ({ title: "Vulcan Code", body: "Completed" });
    expect(await coordinator.handleConversationRunSettled(testSettledEvent(), "Docs", resolveText)).toEqual([
      { endpointKind: "local", status: "delivered", reason: "delivered" },
    ]);
    expect(await coordinator.handleConversationRunSettled(testSettledEvent(), "Docs", resolveText)).toEqual([
      { endpointKind: "local", status: "suppressed", reason: "duplicate_event" },
    ]);
    expect(endpoint.intents).toHaveLength(1);
    shell.activate("notification:event-1");
    expect(routeHandler).toHaveBeenCalledWith({
      kind: "conversation",
      projectId: "project-1",
      conversationId: "conversation-1",
    });
  });
});
