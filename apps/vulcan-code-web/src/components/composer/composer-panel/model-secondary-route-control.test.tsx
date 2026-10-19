// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import type { ProviderModelSecondaryRouteSummary } from "@/types";
import { ModelSecondaryRouteControl } from "./model-secondary-route-control";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
// deferred exposes one controllable service response without contacting a provider.
// deferred 暴露一个可控制的服务响应，不联系供应商。
// No parameters; returns the promise and its explicit completion callbacks.
// 无参数；返回 Promise 及其明确结束回调。
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}
// route creates a complete declared route summary for an exact provider/model owner.
// route 为精确供应商和模型归属创建完整已声明路由摘要。
// id names the endpoint, modelId identifies its owner; returns a synthetic route.
// id 命名端点，modelId 标识归属；返回合成路由。
function route(id: string, modelId = "one"): ProviderModelSecondaryRouteSummary {
  return { providerId: "provider", modelId, routeId: id, tag: "tag", providerName: "Provider",
    endpointName: `${id} endpoint`, label: id, subtitle: `${id} details` };
}
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
// trigger locates the single actual menu trigger in the renderer root.
// trigger 在渲染器根节点中定位唯一真实菜单触发器。
// No parameters; returns the semantic trigger button.
// 无参数；返回语义触发按钮。
function trigger() { return host.querySelector<HTMLButtonElement>('[aria-label="Select secondary provider"]')!; }
// key dispatches keyboard input and lets Radix finish its deferred roving focus update.
// key 分派键盘输入，并等待 Radix 完成延迟的游标焦点更新。
// element is the focused control, value is a key; returns after its effects settle.
// element 是焦点控件，value 是按键；在其副作用结束后返回。
async function key(element: Element, value: string) {
  await act(async () => { element.dispatchEvent(new KeyboardEvent("keydown", { key: value, bubbles: true, cancelable: true })); await new Promise((resolve) => setTimeout(resolve, 1)); });
}
// openMenu follows the production keyboard-open path without mocking the shared menu.
// openMenu 沿生产键盘打开路径执行，不模拟共享菜单。
// No parameters; returns when the portaled menu is open.
// 无参数；在传送门菜单打开后返回。
async function openMenu() { await key(trigger(), "Enter"); }
// item resolves a discovered endpoint through its visible primary label.
// item 根据可见主要标签解析已发现端点。
// label is the exact label; returns its semantic menu item.
// label 是精确标签；返回语义菜单项。
function item(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')]
    .find((button) => button.querySelector(".model-picker-row-title")?.textContent?.startsWith(label))!;
}

it("ignores an old forced refresh after switching model and preserves the new loading state", async () => {
  const oldRefresh = deferred<ProviderModelSecondaryRouteSummary[]>();
  const nextLoad = deferred<ProviderModelSecondaryRouteSummary[]>();
  const list = vi.fn().mockResolvedValueOnce([route("old")]).mockReturnValueOnce(oldRefresh.promise).mockReturnValueOnce(nextLoad.promise);
  const select = vi.fn();
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={null} onListRoutes={list} onSelectRoute={select} />));
  await openMenu();
  await act(async () => document.querySelector<HTMLButtonElement>('[aria-label="Refresh secondary providers"]')!.click());
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="two" selectedRoute={null} onListRoutes={list} onSelectRoute={select} />));
  expect(item("old")).toBeUndefined();
  await act(async () => oldRefresh.resolve([route("stale")]));
  expect(item("stale")).toBeUndefined();
  expect(document.querySelector('[role="status"]')?.textContent).toBe("Loading secondary providers...");
  expect(toast.success).not.toHaveBeenCalled();
  await act(async () => nextLoad.resolve([route("new", "two")]));
  expect(item("new")).toBeDefined();
  expect(list).toHaveBeenLastCalledWith("provider", "two", false);
});

it("invalidates forced refresh when closed and reopens with only the latest request", async () => {
  const oldRefresh = deferred<ProviderModelSecondaryRouteSummary[]>();
  const list = vi.fn().mockResolvedValueOnce([route("old")]).mockReturnValueOnce(oldRefresh.promise).mockResolvedValueOnce([route("new")]);
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={null} onListRoutes={list} onSelectRoute={vi.fn()} />));
  await openMenu();
  const refresh = document.querySelector<HTMLButtonElement>('[aria-label="Refresh secondary providers"]')!;
  await act(async () => { refresh.click(); refresh.click(); });
  expect(list).toHaveBeenCalledTimes(2);
  await key(document.querySelector('[role="menu"]')!, "Escape");
  await openMenu();
  await act(async () => oldRefresh.reject(new Error("obsolete failure")));
  expect(item("new")).toBeDefined();
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(toast.error).not.toHaveBeenCalled();
});

it.each(["false", "reject"])("rolls back a %s selection result and allows an unchanged retry", async (failure) => {
  const write = deferred<boolean>();
  const select = vi.fn().mockReturnValueOnce(write.promise).mockResolvedValueOnce(true);
  const list = vi.fn().mockResolvedValue([route("selected"), route("next")]);
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={route("selected")} onListRoutes={list} onSelectRoute={select} />));
  await openMenu();
  const next = item("next");
  await act(async () => { next.click(); next.click(); });
  expect(select).toHaveBeenCalledTimes(1);
  expect(trigger().disabled).toBe(true);
  expect(trigger().textContent).toBe("next");
  await act(async () => { if (failure === "reject") write.reject(new Error("write failure")); else write.resolve(false); });
  expect(trigger().disabled).toBe(false);
  expect(trigger().textContent).toBe("selected");
  await openMenu();
  expect(document.querySelector('[role="alert"]')).not.toBeNull();
  await act(async () => item("next").click());
  expect(select).toHaveBeenCalledTimes(2);
  expect(select).toHaveBeenLastCalledWith("provider", "one", { routeId: "next", tag: "tag", providerName: "Provider", endpointName: "next endpoint" });
});

it("keeps a new model selection pending when the former model write fails", async () => {
  const oldWrite = deferred<boolean>(); const nextWrite = deferred<boolean>();
  const select = vi.fn().mockReturnValueOnce(oldWrite.promise).mockReturnValueOnce(nextWrite.promise);
  const list = vi.fn().mockResolvedValueOnce([route("old")]).mockResolvedValueOnce([route("new", "two")]);
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={null} onListRoutes={list} onSelectRoute={select} />));
  await openMenu(); await act(async () => item("old").click());
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="two" selectedRoute={null} onListRoutes={list} onSelectRoute={select} />));
  await openMenu(); await act(async () => item("new").click());
  await act(async () => oldWrite.reject(new Error("obsolete write")));
  expect(trigger().textContent).toBe("new"); expect(trigger().disabled).toBe(true);
  expect(toast.error).not.toHaveBeenCalled();
  await act(async () => nextWrite.resolve(true));
  expect(trigger().disabled).toBe(false); expect(trigger().textContent).toBe("new");
});

it("supports search, arrow and enter selection, tab to refresh, and distinct failed/empty states", async () => {
  const select = vi.fn().mockResolvedValue(true);
  const list = vi.fn().mockRejectedValueOnce(new Error("discovery unavailable")).mockResolvedValueOnce([]).mockResolvedValueOnce([route("alpha"), route("beta")]);
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={null} onListRoutes={list} onSelectRoute={select} />));
  await openMenu();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("discovery unavailable");
  expect(document.body.textContent).not.toContain("No secondary providers are available");
  const search = document.querySelector<HTMLInputElement>('[aria-label="Search secondary providers"]')!;
  expect(document.activeElement).toBe(search);
  await act(async () => search.focus());
  await key(search, "Tab");
  const refresh = document.querySelector<HTMLButtonElement>('[aria-label="Refresh secondary providers"]')!;
  expect(document.activeElement).toBe(refresh);
  // Diagnostics must be reachable within one complete focus cycle, independent of selector-engine ordering.
  // 诊断详情必须在一轮完整焦点循环内可达，不依赖选择器引擎的枚举顺序。
  const summary = document.querySelector("summary")!;
  for (let step = 0; step < 4 && document.activeElement !== summary; step += 1) await key(document.activeElement!, "Tab");
  expect(document.activeElement).toBe(summary);
  await act(async () => refresh.click());
  expect(document.body.textContent).toContain("No secondary providers are available for this model");
  await act(async () => refresh.click());
  await act(async () => {
    search.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(search, "beta endpoint");
    search.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(item("alpha")).toBeUndefined(); expect(item("beta")).toBeDefined();
  await key(search, "ArrowUp");
  expect(document.activeElement).toBe(item("beta"));
  await key(item("beta"), "Enter");
  expect(select).toHaveBeenCalledTimes(1);
  expect(trigger().textContent).toBe("beta");
});

it("retains known routes when a same-owner refresh fails and does not reload for callback identity alone", async () => {
  const list = vi.fn().mockResolvedValue([route("known")]);
  const select = vi.fn();
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={null} onListRoutes={list} onSelectRoute={select} />));
  await openMenu();
  const nextList = vi.fn().mockRejectedValue(new Error("refresh unavailable"));
  await act(async () => root.render(<ModelSecondaryRouteControl providerId="provider" modelId="one" selectedRoute={null} onListRoutes={nextList} onSelectRoute={select} />));
  expect(nextList).not.toHaveBeenCalled();
  await act(async () => document.querySelector<HTMLButtonElement>('[aria-label="Refresh secondary providers"]')!.click());
  expect(nextList).toHaveBeenCalledWith("provider", "one", true);
  expect(item("known")).toBeDefined();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("refresh unavailable");
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Refresh secondary providers"]')!.disabled).toBe(false);
});
