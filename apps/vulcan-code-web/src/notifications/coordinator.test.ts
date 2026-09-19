import { describe, expect, it, vi } from "vitest";
import { NotificationCoordinator } from "@/notifications/coordinator";
import { defaultNotificationPreferences } from "@/notifications/preferences";
import type { NotificationRouteStorage } from "@/notifications/route-storage";
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

// MemoryRouteStorage models one page's sessionStorage across a real reload.
// MemoryRouteStorage 模拟真实重载前后同一页面的 sessionStorage。
class MemoryRouteStorage implements NotificationRouteStorage {
  // values stores exact string values by their session storage keys.
  // values 按会话存储键保存精确的字符串值。
  private readonly values = new Map<string, string>();

  // getItem reads one value or null when no document was saved.
  // getItem 读取一项值，未保存文档时返回 null。
  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  // setItem replaces one complete route document.
  // setItem 替换一份完整的路由文档。
  setItem(key: string, value: string): void {
    this.values.set(key, value);
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
    notificationClientId: "client-1",
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
      "client-1",
      null,
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
    shell.activate("notification:run-1");
    expect(routeHandler).toHaveBeenCalledWith({
      kind: "conversation",
      projectId: "project-1",
      conversationId: "conversation-1",
    });
  });

  it("suppresses a settlement event owned by another client", async () => {
    // endpoint records whether a foreign broadcast reached local presentation.
    // endpoint 记录外来广播是否到达本地展示边界。
    const endpoint = new RecordingEndpoint();
    // coordinator owns a different client identity from the event fixture.
    // coordinator 持有与事件夹具不同的客户端身份。
    const coordinator = new NotificationCoordinator(
      new NotificationTestShell(),
      [endpoint],
      defaultNotificationPreferences,
      () => ({ conversationViewActive: false, visibleConversationId: null }),
      "client-2",
      null,
    );

    expect(await coordinator.handleConversationRunSettled(
      testSettledEvent(),
      "Docs",
      () => ({ title: "Vulcan Code", body: "Completed" }),
    )).toEqual([
      { endpointKind: "local", status: "suppressed", reason: "foreign_client" },
    ]);
    expect(endpoint.intents).toHaveLength(0);
  });

  it("restores click routing after a same-client reload without exposing it to another client", async () => {
    // storage survives the first coordinator while each renderer has its own shell.
    // storage 在首个协调器结束后保留，而每个渲染器拥有独立的壳。
    const storage = new MemoryRouteStorage();
    // endpoint records the first renderer's single delivered notification.
    // endpoint 记录首个渲染器投递的一条通知。
    const endpoint = new RecordingEndpoint();
    // preferences permit delivery without asking the operating system in this unit test.
    // preferences 在本单元测试中允许投递，且不申请系统权限。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    // readPresentation reports that no transcript is already visible.
    // readPresentation 报告当前没有正在显示的转录。
    const readPresentation = () => ({ conversationViewActive: false, visibleConversationId: null });
    // first is the coordinator that originally received the service event.
    // first 是最初收到服务事件的协调器。
    const first = new NotificationCoordinator(new NotificationTestShell(), [endpoint],
      () => preferences, readPresentation, "client-1", storage);
    await first.handleConversationRunSettled(testSettledEvent(), "Docs",
      () => ({ title: "Vulcan Code", body: "Completed" }));
    expect(endpoint.intents).toHaveLength(1);

    // reloadedShell receives the native click after the original renderer is gone.
    // reloadedShell 在原渲染器消失后收到原生通知点击。
    const reloadedShell = new NotificationTestShell();
    // reloaded restores only the original client identity and its click map.
    // reloaded 仅恢复原客户端身份及其点击映射。
    const reloaded = new NotificationCoordinator(reloadedShell, [], () => preferences,
      readPresentation, "client-1", storage);
    // opened records the exact conversation opened after the reload.
    // opened 记录重载后打开的精确会话。
    const opened = vi.fn();
    await reloaded.start();
    // The native click may arrive before React installs its navigation handler after reload.
    // 重载后，原生点击可能先于 React 安装导航处理器到达。
    reloadedShell.activate("notification:run-1");
    expect(opened).not.toHaveBeenCalled();
    reloaded.setRouteHandler(opened);
    expect(opened).toHaveBeenCalledExactlyOnceWith({
      kind: "conversation", projectId: "project-1", conversationId: "conversation-1",
    });

    // foreignShell simulates a copied tab with its own new client identity.
    // foreignShell 模拟带有全新客户端身份的复制标签页。
    const foreignShell = new NotificationTestShell();
    // foreign may see copied sessionStorage bytes but cannot restore the old owner's route.
    // foreign 即使看见复制的 sessionStorage 字节，也不能恢复旧所有者的路由。
    const foreign = new NotificationCoordinator(foreignShell, [], () => preferences,
      readPresentation, "client-2", storage);
    // foreignOpened must remain empty when the old native ID is activated.
    // foreignOpened 在旧原生 ID 被激活时必须保持为空。
    const foreignOpened = vi.fn();
    foreign.setRouteHandler(foreignOpened);
    await foreign.start();
    foreignShell.activate("notification:run-1");
    expect(foreignOpened).not.toHaveBeenCalled();
  });
});
