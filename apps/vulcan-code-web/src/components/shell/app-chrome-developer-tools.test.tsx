// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { GlobalActions } from "./app-chrome";

// shell is the exact host capability exposed to the production menu during each test.
// shell 是每个测试中暴露给生产菜单的精确宿主能力。
const shell = vi.hoisted(() => ({
  openDeveloperTools: undefined as undefined | (() => Promise<void>),
}));
vi.mock("@/shell", () => ({ createShellClient: () => shell }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// host and root own one real Radix menu and its portal per test.
// host 与 root 在每个测试中持有一个真实 Radix 菜单及其浮层。
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  shell.openDeveloperTools = undefined;
  vi.unstubAllGlobals();
});

// openMenu renders the global action menu and activates its real keyboard trigger.
// openMenu 渲染全局操作菜单，并激活其真实键盘触发器。
async function openMenu(): Promise<void> {
  await act(async () => root.render(
    <GlobalActions
      startup={{ ready: true, phase: "ready", steps: [], recentLogs: [] }}
      theme="light"
      locale="zh-CN"
      onChangeLocale={() => {}}
      onToggleTheme={() => {}}
    />,
  ));
  await act(async () => host.querySelector<HTMLButtonElement>("button")!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
}

it("opens the workbench developer console from the desktop app menu", async () => {
  const openDeveloperTools = vi.fn(async () => {});
  shell.openDeveloperTools = openDeveloperTools;
  await openMenu();

  const entry = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((item) => item.textContent === "appChrome.developerTools");
  expect(entry).toBeDefined();
  await act(async () => entry!.click());
  expect(openDeveloperTools).toHaveBeenCalledOnce();
});

it("does not offer a nonfunctional developer console in a browser client", async () => {
  await openMenu();
  expect(document.body.textContent).not.toContain("appChrome.developerTools");
});
