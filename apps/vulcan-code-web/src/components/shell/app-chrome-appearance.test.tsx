// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage, type TranslationKey } from "@/i18n";
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

// clickItem clicks the open menu row with the given role whose text starts with the catalog message for key.
// clickItem 点击已打开菜单中指定角色且文本以 key 对应目录文案开头的行。
async function clickItem(role: string, key: TranslationKey): Promise<void> {
  const label = readLocaleMessage("en-US", key);
  const item = [...document.querySelectorAll<HTMLElement>(`[role="${role}"]`)].find((row) => row.textContent?.startsWith(label));
  expect(item, `${role} ${key}`).toBeDefined();
  await act(async () => item!.click());
}

it("applies a theme picked from the app menu even when this client refuses to store it", async () => {
  const storage = { getItem: vi.fn(() => null), setItem: vi.fn(() => { throw new Error("blocked"); }) };
  vi.stubGlobal("localStorage", storage);
  await render(<GlobalActions startup={{ ready: true, phase: "ready", steps: [], recentLogs: [] }} locale="en-US" onChangeLocale={() => {}} />);
  expect(storage.setItem).not.toHaveBeenCalled();
  await openTrigger();
  await clickItem("menuitem", "appChrome.theme");
  await clickItem("menuitemradio", "appChrome.theme.command");
  expect(document.documentElement.dataset.vkTheme).toBe("command");
  expect(document.documentElement.dataset.vkScheme).toBe("dark");
  expect(toast.error).toHaveBeenCalledOnce();
});

it("sets the text tier and the reduced-motion switch from the app menu", async () => {
  await render(<GlobalActions startup={{ ready: true, phase: "ready", steps: [], recentLogs: [] }} locale="en-US" onChangeLocale={() => {}} />);
  await openTrigger();
  await clickItem("menuitem", "appChrome.text");
  await clickItem("menuitemradio", "appChrome.text.l");
  expect(document.documentElement.dataset.vkText).toBe("l");
  await openTrigger();
  await clickItem("menuitemcheckbox", "appChrome.motion.reduce");
  expect(document.documentElement.dataset.vkMotion).toBe("reduce");
});

it("steps the text tier with Ctrl+= and Ctrl+-, stops at either end, and restores the default with Ctrl+0", async () => {
  await render(<GlobalActions startup={{ ready: true, phase: "ready", steps: [], recentLogs: [] }} locale="en-US" onChangeLocale={() => {}} />);
  const press = async (key: string) => {
    const event = new KeyboardEvent("keydown", { key, ctrlKey: true, bubbles: true, cancelable: true });
    await act(async () => { document.body.dispatchEvent(event); });
    return event.defaultPrevented;
  };
  const tier = () => document.documentElement.dataset.vkText;
  expect(await press("=")).toBe(true);
  expect(tier()).toBe("m");
  for (let index = 0; index < 5; index += 1) await press("-");
  expect(tier()).toBe("xs");
  await press("0");
  expect(tier()).toBe(DEFAULT_APPEARANCE.text);
  expect(await press("k")).toBe(false);
});

it("switches the conversation reading width from the title strip", async () => {
  await render(<ReadingWidthMenu />);
  await openTrigger();
  expect(document.documentElement.dataset.vkWidth).toBe("standard");
  expect(document.querySelectorAll('[role="menuitemradio"]')).toHaveLength(5);
  expect(document.querySelectorAll('[role="menuitemradio"][aria-checked="true"]')).toHaveLength(1);
  await clickItem("menuitemradio", "appChrome.width.full");
  expect(document.documentElement.dataset.vkWidth).toBe("full");
});
