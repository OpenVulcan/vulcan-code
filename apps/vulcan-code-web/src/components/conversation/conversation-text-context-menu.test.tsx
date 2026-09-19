// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ConversationTextContextMenu } from "./conversation-text-context-menu";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// host, root, and timelineRef own one real transcript context menu per test.
// host、root 与 timelineRef 为每个测试持有一份真实的会话右键菜单。
let host: HTMLDivElement;
let root: Root;
let timelineRef: React.RefObject<HTMLDivElement | null>;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  timelineRef = React.createRef<HTMLDivElement>();
});

afterEach(async () => {
  await act(async () => root.unmount());
  window.getSelection()?.removeAllRanges();
  host.remove();
  vi.unstubAllGlobals();
});

// mountMenu renders the transcript and an adjacent composer-shaped text outside its selection scope.
// mountMenu 渲染会话转录及一段位于选区范围外的输入区形态文本。
async function mountMenu(onCopyText: (text: string) => Promise<void> | void): Promise<void> {
  await act(async () => root.render(
    <ConversationTextContextMenu enabled timelineRef={timelineRef} onCopyText={onCopyText}>
      <div>
        <div ref={timelineRef}><span>Selected transcript</span><span> second line</span></div>
        <div data-composer-text>Composer draft</div>
      </div>
    </ConversationTextContextMenu>,
  ));
}

// openMenu triggers the production right-click path after a caller sets the DOM selection.
// openMenu 在调用方设置 DOM 选区后触发生产右键路径。
async function openMenu(): Promise<void> {
  await act(async () => timelineRef.current!.querySelector("span")!.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, button: 2, clientX: 20, clientY: 20 }),
  ));
}

it("copies only text selected inside the visible transcript", async () => {
  const copy = vi.fn(async (_text: string) => {});
  await mountMenu(copy);
  const range = document.createRange();
  range.selectNodeContents(timelineRef.current!.querySelector("span")!);
  window.getSelection()!.addRange(range);
  await openMenu();

  const copyItem = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((item) => item.textContent === "conversation.contextMenu.copy")!;
  expect(copyItem.getAttribute("data-disabled")).toBeNull();
  await act(async () => copyItem.click());
  expect(copy).toHaveBeenCalledExactlyOnceWith("Selected transcript");
});

it("selects the transcript without selecting the adjacent composer text", async () => {
  await mountMenu(vi.fn());
  await openMenu();
  const items = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  expect(items.find((item) => item.textContent === "conversation.contextMenu.copy")?.hasAttribute("data-disabled")).toBe(true);
  await act(async () => items.find((item) => item.textContent === "conversation.contextMenu.selectAll")!.click());
  expect(window.getSelection()?.toString()).toContain("Selected transcript second line");
  expect(window.getSelection()?.toString()).not.toContain("Composer draft");
});
