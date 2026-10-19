// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { SidebarBrandMenu, SidebarUserMenu } from "./sidebar-profile-menus";

// host and root isolate sidebar navigation without mounting a service connection.
// host 与 root 隔离侧栏导航，不挂载服务连接。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("keeps the panel header a plain title and the rail user menu's settings action keyboard reachable", async () => {
  const settings = vi.fn();
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><SidebarProvider>
    <SidebarBrandMenu currentLabel="设置" />
    <SidebarUserMenu onOpenSettings={settings} />
  </SidebarProvider></TooltipProvider></I18nProvider>));
  // Area switching now lives on the activity rail, so the panel header carries no buttons.
  // 区域切换已移至活动轨道，面板标题不再包含按钮。
  expect(host.querySelector(".sidebar-brand-static")?.textContent).toContain("设置");
  expect(host.querySelector(".sidebar-brand-static button")).toBeNull();
  const trigger = host.querySelector<HTMLButtonElement>(".sidebar-user-button")!;
  await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  const menu = document.querySelector('[role="menu"]')!;
  expect(menu).not.toBeNull();
  expect(menu.textContent).not.toContain("退出");
  expect(menu.textContent).not.toContain("账号");
  const item = menu.querySelector<HTMLElement>('[role="menuitem"]')!;
  await act(async () => { item.focus(); item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  expect(settings).toHaveBeenCalledTimes(1);
});
