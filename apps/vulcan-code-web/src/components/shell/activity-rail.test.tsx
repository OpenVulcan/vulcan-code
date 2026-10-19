// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { ActivityRail, type ActivityRailEntry } from "./activity-rail";

// host and root isolate the rail without mounting a service connection.
// host 与 root 隔离轨道，不挂载服务连接。
let host: HTMLDivElement;
let root: Root;
// store is an in-memory localStorage so pin persistence is observable.
// store 是内存版 localStorage，用于观察固定状态的持久化。
let store: Map<string, string>;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  store = new Map();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
  });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// renderRail mounts the rail with the given entries and conversation projection.
// renderRail 使用给定入口与会话投影挂载轨道。
async function renderRail(props: {
  entries: ActivityRailEntry[];
  sessionsActive: boolean;
  sessionTree?: React.ReactNode;
  onSelectSessions?: () => void;
  defaultOpen?: boolean;
}) {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><SidebarProvider defaultOpen={props.defaultOpen ?? true}>
    <ActivityRail
      entries={props.entries}
      onSelectSessions={props.onSelectSessions ?? vi.fn()}
      onSelectSettings={vi.fn()}
      selectedConversationId={null}
      sessionTree={props.sessionTree ?? null}
      sessionsActive={props.sessionsActive}
      settingsActive={false}
    />
  </SidebarProvider></TooltipProvider></I18nProvider>));
}

it("keeps dynamic plugin pages in the overflow menu until the user pins them", async () => {
  const openPlugin = vi.fn();
  // entries mirrors the layout: a built-in area plus one manifest-generated plugin page.
  // entries 与布局一致：一个内置区域加一个由清单生成的插件页面。
  await renderRail({
    sessionsActive: true,
    entries: [
      { id: "skills", label: "插件", icon: null, active: false, onSelect: vi.fn() },
      { id: "plugin:demo:stats", label: "统计", icon: null, active: false, parentId: "skills", onSelect: openPlugin },
    ],
  });
  const railLabels = () => [...host.querySelectorAll<HTMLButtonElement>(".activity-rail-button")].map((button) => button.getAttribute("aria-label"));
  expect(railLabels()).toContain("插件");
  expect(railLabels()).not.toContain("统计");

  const more = host.querySelector<HTMLButtonElement>('button[aria-label="更多"]')!;
  await act(async () => { more.focus(); more.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
  const checkbox = [...document.querySelectorAll<HTMLElement>('[role="menuitemcheckbox"]')].find((item) => item.textContent?.includes("统计"))!;
  expect(checkbox.getAttribute("aria-checked")).toBe("false");
  await act(async () => { checkbox.focus(); checkbox.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });

  // Pinning is an explicit user action, so it is the only time the layout is written.
  // 固定是用户的明确操作，因此只有此时才写入布局。
  expect(JSON.parse(store.get("vulcan.gui.activity-rail-pins.v1") ?? "[]")).toContain("plugin:demo:stats");
  expect(railLabels()).toContain("统计");
  await act(async () => host.querySelector<HTMLButtonElement>('.activity-rail-button[aria-label="统计"]')!.click());
  expect(openPlugin).toHaveBeenCalledTimes(1);
});

it("shows the session panel's own tree in the collapsed session area and reopens the panel from its footer", async () => {
  // sessionTree stands in for the panel's ProjectSessionTree; the flyout must host it unchanged.
  // sessionTree 代表面板的 ProjectSessionTree；浮层必须原样承载它。
  await renderRail({
    sessionsActive: true,
    defaultOpen: false,
    entries: [{ id: "skills", label: "插件", icon: null, active: false, onSelect: vi.fn() }],
    sessionTree: <div data-testid="session-tree">Alpha</div>,
  });
  // The session area is open with its panel collapsed, so focusing the sessions button opens the flyout.
  // 处于会话区域且面板已折叠，因此聚焦会话按钮即打开浮层。
  await act(async () => host.querySelector<HTMLButtonElement>('.activity-rail-button[aria-label="工作台"]')!.focus());
  const flyout = () => host.querySelector<HTMLElement>(".activity-rail-flyout");
  expect(flyout()!.querySelector('[data-testid="session-tree"]')).not.toBeNull();
  const footer = flyout()!.querySelector<HTMLButtonElement>(".activity-rail-flyout-footer button")!;
  await act(async () => footer.click());
  expect(flyout()).toBeNull();
  // Reopening the panel ends the collapsed state, so the button no longer offers the flyout.
  // 重新展开面板后不再处于折叠状态，按钮也不再提供浮层。
  await act(async () => host.querySelector<HTMLButtonElement>('.activity-rail-button[aria-label="工作台"]')!.focus());
  expect(flyout()).toBeNull();
});

it("navigates without a flyout from a feature page", async () => {
  const selectSessions = vi.fn();
  await renderRail({
    sessionsActive: false,
    defaultOpen: false,
    entries: [{ id: "skills", label: "插件", icon: null, active: true, onSelect: vi.fn() }],
    sessionTree: <div data-testid="session-tree">Alpha</div>,
    onSelectSessions: selectSessions,
  });
  // Outside the session area the sessions button only navigates; the flyout waits until the user is there.
  // 在会话区域之外，会话按钮只负责导航；浮层要等用户进入会话区域后才出现。
  const button = host.querySelector<HTMLButtonElement>('.activity-rail-button[aria-label="工作台"]')!;
  await act(async () => button.focus());
  expect(host.querySelector(".activity-rail-flyout")).toBeNull();
  await act(async () => button.click());
  expect(selectSessions).toHaveBeenCalledTimes(1);
});

it("does not open the flyout while the session panel itself is visible", async () => {
  await renderRail({
    sessionsActive: true,
    entries: [{ id: "skills", label: "插件", icon: null, active: false, onSelect: vi.fn() }],
    sessionTree: <div data-testid="session-tree">Alpha</div>,
  });
  await act(async () => host.querySelector<HTMLButtonElement>('.activity-rail-button[aria-label="工作台"]')!.focus());
  expect(host.querySelector(".activity-rail-flyout")).toBeNull();
});
