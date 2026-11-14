import { describe, expect, it, vi } from "vitest";
import {
  createClientOwnedActionId,
  notificationClientInstanceId,
  resolveNotificationClientInstanceId,
} from "@/notifications/client-identity";
import { resolveBrowserServiceClientId } from "@/service/browser-service-discovery";

describe("client-owned action identity", () => {
  it("carries the loaded WebUI client id in every unique action id", () => {
    // first and second are independent actions initiated by the same loaded client.
    // first 与 second 是同一个已加载客户端发起的两个独立动作。
    const first = createClientOwnedActionId();
    const second = createClientOwnedActionId();
    expect(first).toMatch(
      /^vcn1:[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}:[0-9a-f-]{36}$/,
    );
    expect(first.split(":")[1]).toBe(notificationClientInstanceId);
    expect(second.split(":")[1]).toBe(notificationClientInstanceId);
    expect(second).not.toBe(first);
  });

  it("uses the same owner for durable actions and the service event socket", () => {
    expect(resolveBrowserServiceClientId()).toBe(notificationClientInstanceId);
  });

  it("keeps one client owner across reloads of the same session-scoped surface", () => {
    // values models the sessionStorage namespace owned by one tab or Electron window.
    // values 模拟一个标签页或 Electron 窗口拥有的 sessionStorage 命名空间。
    const values = new Map<string, string>();
    // storage records and restores the exact value written by the identity resolver.
    // storage 记录并恢复身份解析器写入的精确值。
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    };
    // firstLoad and reloaded represent two module lifetimes sharing one client surface.
    // firstLoad 与 reloaded 表示共享同一客户端界面的两次模块生命周期。
    const firstLoad = resolveNotificationClientInstanceId(storage, false);
    const reloaded = resolveNotificationClientInstanceId(storage, true);
    expect(reloaded).toBe(firstLoad);
  });

  it("replaces a copied sessionStorage owner on a new tab navigation", () => {
    // values models a newly opened tab that inherited another tab's sessionStorage value.
    // values 模拟继承了另一标签页 sessionStorage 值的新标签页。
    const values = new Map<string, string>();
    // storage records the identity after each separate document navigation.
    // storage 记录每次独立文档导航后的客户端身份。
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    };
    const firstTab = resolveNotificationClientInstanceId(storage, false);
    const copiedTab = resolveNotificationClientInstanceId(storage, false);
    expect(copiedTab).not.toBe(firstTab);
    expect(resolveNotificationClientInstanceId(storage, true)).toBe(copiedTab);
  });

  it("replaces a malformed stored owner instead of exposing it to Core", () => {
    // values starts with an invalid external value that must never enter a durable action id.
    // values 初始包含一个无效外部值，该值不得进入可靠动作标识。
    const values = new Map<string, string>([["vulcan.notifications.client-instance.v1", "spoofed"]]);
    // storage exposes the malformed value through the same minimal session contract.
    // storage 通过同一个最小会话契约暴露格式错误的值。
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    };
    expect(resolveNotificationClientInstanceId(storage)).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });

  it("reuses the pending recovery action across reload until its exact acknowledgement", async () => {
    // values retains browser session storage while the page module is recreated.
    // values 在页面模块重新创建期间保留浏览器会话存储。
    const values = new Map<string, string>();
    // storage provides the same tab's data to both document lifetimes.
    // storage 向两次文档生命周期提供同一个标签页的数据。
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    };
    // scope identifies a blocked switch's source and exact target.
    // scope 标识被阻止切换的来源和精确目标。
    const scope = 'reasoning-recovery:["source","deepseek","account","deepseek-flash"]';
    try {
      vi.stubGlobal("sessionStorage", storage);
      vi.spyOn(performance, "getEntriesByType").mockReturnValue([
        { type: "navigate" } as PerformanceNavigationTiming,
      ]);
      vi.resetModules();
      // first represents the original page before the recovery response is lost.
      // first 表示恢复响应丢失之前的原页面。
      const first = await import("@/notifications/client-identity");
      // actionId is saved before the first recovery request.
      // actionId 在首次恢复请求之前保存。
      const actionId = first.retainClientOwnedRetryActionId(scope);
      vi.mocked(performance.getEntriesByType).mockReturnValue([
        { type: "reload" } as PerformanceNavigationTiming,
      ]);
      vi.resetModules();
      // reloaded has a new module instance and restores the original tab owner.
      // reloaded 是恢复原标签页所有者的新模块实例。
      const reloaded = await import("@/notifications/client-identity");
      expect(reloaded.retainClientOwnedRetryActionId(scope)).toBe(actionId);
      expect(reloaded.retainClientOwnedRetryActionId(`${scope}:other-route`)).not.toBe(actionId);
      reloaded.completeClientOwnedRetryAction(scope, reloaded.createClientOwnedActionId());
      expect(reloaded.retainClientOwnedRetryActionId(scope)).toBe(actionId);
      reloaded.completeClientOwnedRetryAction(scope, actionId);
      expect(reloaded.retainClientOwnedRetryActionId(scope)).not.toBe(actionId);
    } finally {
      vi.restoreAllMocks();
      vi.unstubAllGlobals();
      vi.resetModules();
    }
  });
});
