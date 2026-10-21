// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { CommandPalette, recentPaletteConversations, type PaletteConversation } from "./command-palette";

let host: HTMLDivElement;
let root: Root;
// frames holds animation-frame callbacks until a test flushes them, so they run after React commits as in a browser.
// frames 暂存动画帧回调直到测试刷新，使其像浏览器中一样在 React 提交之后运行。
let frames: FrameRequestCallback[] = [];
beforeEach(() => {
  frames = [];
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = () => {};
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Harness owns the palette's open state the way the workbench does, behind a button that opens it.
// Harness 以工作台的方式持有面板的打开状态，并提供打开它的按钮。
function Harness(props: { conversations?: PaletteConversation[]; openConversationId?: string | null }) {
  const [open, setOpen] = React.useState(false);
  return (
    <I18nProvider initialLocale="en-US">
      <button onClick={() => setOpen(true)} type="button">open</button>
      <CommandPalette
        areas={[]}
        conversations={props.conversations ?? []}
        onOpenChange={setOpen}
        onOpenConversation={() => {}}
        openConversationId={props.openConversationId ?? null}
        onOpenSessions={() => {}}
        onOpenSettingsPage={() => {}}
        open={open}
        slashCommands={[]}
      />
    </I18nProvider>
  );
}

it("opens the keyboard shortcut list from the palette and returns focus to the opener on close", async () => {
  await act(async () => root.render(<Harness />));
  const opener = host.querySelector("button")!;
  opener.focus();
  await act(async () => opener.click());
  const item = document.querySelector<HTMLElement>('[cmdk-item][data-value="keyboard-shortcuts"]');
  expect(item).not.toBeNull();
  await act(async () => item!.click());
  await act(async () => { for (const callback of frames.splice(0)) callback(0); });
  const dialog = document.querySelector<HTMLElement>('[role="dialog"]');
  expect(dialog).not.toBeNull();
  expect(document.querySelector("[cmdk-root]")).toBeNull();
  const title = dialog!.querySelector("h2");
  expect(dialog!.getAttribute("aria-labelledby")).toBe(title?.id);
  // Each group heading sits one level under the dialog title, and every row pairs an action with key caps.
  // 每个分组标题比对话框标题低一级，每行都把一项操作与键帽配对。
  const groups = dialog!.querySelectorAll(".vk-shortcuts-group > h3");
  expect(groups.length).toBeGreaterThan(0);
  for (const row of dialog!.querySelectorAll(".vk-shortcuts > div")) {
    expect(row.querySelector("dt")?.textContent).toBeTruthy();
    expect(row.querySelectorAll("dd > kbd").length).toBeGreaterThan(0);
  }
  await act(async () => dialog!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(opener);
});

// session builds one palette session with the given last activity.
// session 构建一个具有指定最后活动时间的面板会话。
function session(id: string, updatedAtMs: number): PaletteConversation {
  return { id, title: `Session ${id}`, projectName: null, updatedAtMs };
}

it("leads with the most recently active other sessions until something is typed", async () => {
  const conversations = [session("a", 10), session("b", 30), session("open", 40), session("c", 20), session("unknown", 0)];
  await act(async () => root.render(<Harness conversations={conversations} openConversationId="open" />));
  await act(async () => host.querySelector("button")!.click());
  const values = () => [...document.querySelectorAll<HTMLElement>("[cmdk-item]")].map((item) => item.dataset.value);
  expect(values().slice(0, 3)).toEqual(["recent:b", "recent:c", "recent:a"]);
  expect(values()).not.toContain("recent:open");
  expect(values()).not.toContain("recent:unknown");
  const input = document.querySelector<HTMLInputElement>("[cmdk-input]")!;
  const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => { setValue.call(input, "Session"); input.dispatchEvent(new Event("input", { bubbles: true })); });
  expect(values().some((value) => value?.startsWith("recent:"))).toBe(false);
  expect(values()).toContain("conversation:open");
});

it("keeps at most a few recent sessions", () => {
  const many = Array.from({ length: 20 }, (_, index) => session(String(index), index + 1));
  const recent = recentPaletteConversations(many, null);
  expect(recent.length).toBeLessThan(many.length);
  expect(recent[0].id).toBe("19");
});
