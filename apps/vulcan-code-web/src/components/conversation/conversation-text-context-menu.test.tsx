// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ConversationTextContextMenu } from "./conversation-text-context-menu";
import { WorkspaceFileOpenContext } from "@/components/workspace/workspace-file-open-request";
import { fileReferenceData } from "@/components/workspace/workspace-file-actions";

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
// onCopyText receives clipboard writes; enabled selects the desktop menu instead of native browser handling;
// onQuoteText receives text quoted into the composer.
// onCopyText 接收剪贴板写入；enabled 选择桌面菜单，而非浏览器原生处理；onQuoteText 接收引用到输入区的文本。
async function mountMenu(
  onCopyText: (text: string) => Promise<void> | void,
  enabled = true,
  onQuoteText: (text: string) => void = () => {},
): Promise<void> {
  await act(async () => root.render(
    <ConversationTextContextMenu enabled={enabled} timelineRef={timelineRef} onCopyText={onCopyText} onQuoteText={onQuoteText}>
      <div>
        <div ref={timelineRef}>
          <span>Selected transcript</span><span> second line</span>
          <a data-link="web" href="https://example.com/page?q=1#section"><svg aria-hidden="true"><circle /></svg>Website</a>
          <a data-link="file" href="file:///C:/my%20files/page.html#section">Local page</a>
          <a data-link="empty" href="">Filtered link</a>
        </div>
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

it("quotes only text selected inside the visible transcript", async () => {
  const quote = vi.fn((_text: string) => {});
  await mountMenu(vi.fn(), true, quote);
  const range = document.createRange();
  range.selectNodeContents(timelineRef.current!.querySelector("span")!);
  window.getSelection()!.addRange(range);
  await openMenu();

  const quoteItem = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((item) => item.textContent === "conversation.contextMenu.quote")!;
  expect(quoteItem.getAttribute("data-disabled")).toBeNull();
  await act(async () => quoteItem.click());
  expect(quote).toHaveBeenCalledExactlyOnceWith("Selected transcript");
});

it("selects the transcript without selecting the adjacent composer text", async () => {
  await mountMenu(vi.fn());
  await openMenu();
  const items = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  expect(items.find((item) => item.textContent === "conversation.contextMenu.copy")?.hasAttribute("data-disabled")).toBe(true);
  expect(items.find((item) => item.textContent === "conversation.contextMenu.quote")?.hasAttribute("data-disabled")).toBe(true);
  await act(async () => items.find((item) => item.textContent === "conversation.contextMenu.selectAll")!.click());
  expect(window.getSelection()?.toString()).toContain("Selected transcript second line");
  expect(window.getSelection()?.toString()).not.toContain("Composer draft");
});

it.each([
  ['[data-link="web"] circle', "https://example.com/page?q=1#section"],
  ['[data-link="file"]', "file:///C:/my%20files/page.html#section"],
])("copies the actual address when right-clicking %s", async (selector, expected) => {
  // copy observes the production clipboard callback without modifying the system clipboard.
  // copy 观察生产剪贴板回调，不修改系统剪贴板。
  const copy = vi.fn(async (_text: string) => {});
  await mountMenu(copy);
  await act(async () => timelineRef.current!.querySelector(selector)!.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 }),
  ));
  // item locates the action by meaning rather than its position alongside existing text actions.
  // item 按语义而非与既有文本操作的相对位置定位菜单项。
  const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((entry) => entry.textContent === "conversation.contextMenu.copyLinkAddress")!;
  expect(item).toBeDefined();
  await act(async () => item.click());
  expect(copy).toHaveBeenCalledExactlyOnceWith(expected);
  await openMenu();
  expect(document.body.textContent).not.toContain("conversation.contextMenu.copyLinkAddress");
});

it("does not offer copying for an empty sanitized link", async () => {
  await mountMenu(vi.fn());
  await act(async () => timelineRef.current!.querySelector('[data-link="empty"]')!.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 }),
  ));
  expect(document.querySelector('[role="menu"]')).not.toBeNull();
  expect(document.body.textContent).not.toContain("conversation.contextMenu.copyLinkAddress");
});

it("leaves the web client's native link context menu untouched", async () => {
  await mountMenu(vi.fn(), false);
  // event retains its default action so the browser can provide Copy link address itself.
  // event 保留默认动作，浏览器可自行提供复制链接地址。
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 });
  await act(async () => timelineRef.current!.querySelector('[data-link="web"]')!.dispatchEvent(event));
  expect(event.defaultPrevented).toBe(false);
  expect(document.querySelector('[role="menu"]')).toBeNull();
});

// mountFileReference renders one file reference inside the workbench's file opener, with or without the desktop's
// local file bridge.
// mountFileReference 在工作台文件打开动作中渲染一个文件引用，可选择是否提供桌面本地文件桥。
async function mountFileReference(open: (...request: unknown[]) => void, copy: (text: string) => void, desktop: boolean): Promise<void> {
  if (desktop) {
    const bridge = { inspect: vi.fn(), openWithDefaultApp: vi.fn(), showInFolder: vi.fn() };
    vi.stubGlobal("vulcanDesktop", { localFile: bridge });
  }
  await act(async () => root.render(
    <WorkspaceFileOpenContext.Provider value={open}>
      <ConversationTextContextMenu enabled timelineRef={timelineRef} onCopyText={copy} onQuoteText={() => {}}>
        <div ref={timelineRef}>
          <code {...fileReferenceData("docs/design.psd", { start: 3, end: 4 })}>docs/design.psd:3-4</code>
        </div>
      </ConversationTextContextMenu>
    </WorkspaceFileOpenContext.Provider>,
  ));
  await act(async () => timelineRef.current!.querySelector("code")!.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 }),
  ));
}

// menuItem finds one open menu item by its label key.
// menuItem 按文案键查找已打开菜单中的一项。
function menuItem(label: string): HTMLElement | undefined {
  return [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((entry) => entry.textContent === label);
}

it("offers file actions on a file reference and hands OS actions to the file opener", async () => {
  const open = vi.fn();
  const copy = vi.fn();
  await mountFileReference(open, copy, true);
  await act(async () => menuItem("fileActions.openWithDefaultApp")!.click());
  expect(open).toHaveBeenCalledExactlyOnceWith("docs/design.psd", { start: 3, end: 4 }, "open");

  await act(async () => timelineRef.current!.querySelector("code")!.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 }),
  ));
  await act(async () => menuItem("fileActions.copyPath")!.click());
  expect(copy).toHaveBeenCalledExactlyOnceWith("docs/design.psd");
});

it("offers no OS file actions without the desktop bridge", async () => {
  await mountFileReference(vi.fn(), vi.fn(), false);
  expect(menuItem("fileActions.open")).toBeDefined();
  expect(menuItem("fileActions.copyPath")).toBeDefined();
  expect(menuItem("fileActions.openWithDefaultApp")).toBeUndefined();
  expect(menuItem("fileActions.showInFolder")).toBeUndefined();
});
