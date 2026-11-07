// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/ui-kit/shell/sidebar";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { SidebarBrandMenu, SidebarUserMenu } from "./sidebar-profile-menus";

// openExternalUrl is replaced so the GitHub entries can be observed without a host shell.
// 替换 openExternalUrl，使 GitHub 入口无需宿主壳即可被观察。
const openExternalUrl = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>(async () => undefined));
vi.mock("@/service/project-actions-service", () => ({ openExternalUrl }));

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
  expect(host.querySelector(".vk-brand")?.textContent).toContain("设置");
  expect(host.querySelector(".vk-brand button")).toBeNull();
  const trigger = host.querySelector<HTMLButtonElement>(".vk-rail-btn")!;
  await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  const menu = document.querySelector('[role="menu"]')!;
  expect(menu).not.toBeNull();
  expect(menu.textContent).not.toContain("退出");
  expect(menu.textContent).not.toContain("账号");
  const item = menu.querySelector<HTMLElement>('[role="menuitem"]')!;
  await act(async () => { item.focus(); item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  expect(settings).toHaveBeenCalledTimes(1);
});

it("opens the GitHub issue and pull request pages from the rail user menu without an account", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><TooltipProvider><SidebarProvider>
    <SidebarUserMenu onOpenSettings={() => undefined} />
  </SidebarProvider></TooltipProvider></I18nProvider>));
  // select opens the menu and activates the item that carries the given icon.
  // select 打开菜单并激活带有指定图标的条目。
  const select = async (icon: string) => {
    const trigger = host.querySelector<HTMLButtonElement>(".vk-rail-btn")!;
    await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
    const item = document.querySelector(`[role="menuitem"] svg.${icon}`)?.closest<HTMLElement>(`[role="menuitem"]`) as HTMLElement;
    expect(item).toBeDefined();
    await act(async () => { item.focus(); item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  };
  await select("lucide-circle-dot");
  await select("lucide-git-pull-request");
  expect(openExternalUrl.mock.calls).toEqual([
    ["https://github.com/OpenVulcan/vulcan-code/issues/new"],
    ["https://github.com/OpenVulcan/vulcan-code/compare"],
  ]);
});
