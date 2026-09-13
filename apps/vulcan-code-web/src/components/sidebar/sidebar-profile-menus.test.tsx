// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
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

it("removes clipped header actions in collapsed mode and keeps the real user-menu settings action keyboard reachable", async () => {
  const select = vi.fn();
  const settings = vi.fn();
  // render updates the actual sidebar preference and returns after its context commits.
  // render 更新真实侧栏偏好，在上下文提交后返回。
  async function render(open: boolean) {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><SidebarProvider open={open}>
      <SidebarBrandMenu activeArea="settings" currentLabel="设置" onSelectArea={select} />
      <SidebarUserMenu onOpenSettings={settings} />
    </SidebarProvider></TooltipProvider></I18nProvider>));
  }
  await render(true);
  const back = host.querySelector<HTMLButtonElement>('.sidebar-brand-settings-button')!;
  await act(async () => back.click());
  expect(select).toHaveBeenCalledExactlyOnceWith("chat");
  await render(false);
  expect(host.querySelector('.sidebar-brand-settings-button')).toBeNull();
  const trigger = host.querySelector<HTMLButtonElement>('.sidebar-user-button')!;
  await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  const menu = document.querySelector('[role="menu"]')!;
  expect(menu).not.toBeNull();
  expect(menu.textContent).not.toContain("退出");
  expect(menu.textContent).not.toContain("账号");
  const item = menu.querySelector<HTMLElement>('[role="menuitem"]')!;
  await act(async () => { item.focus(); item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  expect(settings).toHaveBeenCalledTimes(1);
});
