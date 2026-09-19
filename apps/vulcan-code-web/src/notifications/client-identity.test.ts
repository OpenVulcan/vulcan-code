import { describe, expect, it } from "vitest";
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
});
