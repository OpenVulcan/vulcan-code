// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { DEFAULT_APPEARANCE, setAppearance } from "@/ui-kit/appearance";
import { toast } from "@/ui-kit/primitives/toast";
import { GlobalActions, ReadingWidthMenu } from "./app-chrome";

vi.mock("@/host-shell", () => ({ createShellClient: () => ({}) }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn() } }));

// host and root own one menu render and its portal per test.
// host 与 root 在每个测试中持有一次菜单渲染及其浮层。
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(toast.error).mockClear();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  for (const [key, value] of Object.entries(DEFAULT_APPEARANCE)) setAppearance(key as keyof typeof DEFAULT_APPEARANCE, value as never);
});

// render mounts one chrome element inside the real translation provider.
// render 在真实翻译上下文中挂载一个 chrome 元素。
async function render(element: React.ReactElement): Promise<void> {
  await act(async () => root.render(<I18nProvider initialLocale="en-US">{element}</I18nProvider>));
}

// openTrigger activates the rendered menu trigger with the keyboard.
// openTrigger 用键盘激活已渲染的菜单触发器。
async function openTrigger(): Promise<void> {
  await act(async () => host.querySelector<HTMLButtonElement>("button")!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
}

// clickItem clicks the open menu row with the given role whose text starts with label.
// clickItem 点击已打开菜单中指定角色且文本以 label 开头的行。
async function clickItem(role: string, label: string): Promise<void> {
  const item = [...document.querySelectorAll<HTMLElement>(`[role="${role}"]`)].find((row) => row.textContent?.startsWith(label));
  expect(item, `${role} "${label}"`).toBeDefined();
  await act(async () => item!.click());
}

it("applies a theme picked from the app menu even when this client refuses to store it", async () => {
  const storage = { getItem: vi.fn(() => null), setItem: vi.fn(() => { throw new Error("blocked"); }) };
  vi.stubGlobal("localStorage", storage);
  await render(<GlobalActions startup={{ ready: true, phase: "ready", steps: [], recentLogs: [] }} locale="en-US" onChangeLocale={() => {}} />);
  expect(storage.setItem).not.toHaveBeenCalled();
  await openTrigger();
  await clickItem("menuitem", "Theme");
  await clickItem("menuitemradio", "Command");
  expect(document.documentElement.dataset.vkTheme).toBe("command");
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  expect(toast.error).toHaveBeenCalledOnce();
});

it("sets the text tier and the reduced-motion switch from the app menu", async () => {
  await render(<GlobalActions startup={{ ready: true, phase: "ready", steps: [], recentLogs: [] }} locale="en-US" onChangeLocale={() => {}} />);
  await openTrigger();
  await clickItem("menuitem", "Text size");
  await clickItem("menuitemradio", "Large");
  expect(document.documentElement.dataset.vkText).toBe("l");
  await openTrigger();
  await clickItem("menuitemcheckbox", "Reduce motion");
  expect(document.documentElement.dataset.vkMotion).toBe("reduce");
});

it("switches the conversation reading width from the title strip", async () => {
  await render(<ReadingWidthMenu />);
  await openTrigger();
  expect(document.documentElement.dataset.vkWidth).toBe("standard");
  expect(document.querySelectorAll('[role="menuitemradio"]')).toHaveLength(5);
  expect(document.querySelectorAll('[role="menuitemradio"][aria-checked="true"]')).toHaveLength(1);
  await clickItem("menuitemradio", "Full width");
  expect(document.documentElement.dataset.vkWidth).toBe("full");
});
