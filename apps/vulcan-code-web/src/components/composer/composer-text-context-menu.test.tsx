// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Textarea } from "@/components/ui/textarea";
import { ComposerTextContextMenu } from "./composer-text-context-menu";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
// desktopClipboardBridge selects the native path without mounting an entire Electron preload.
// desktopClipboardBridge 在无需挂载完整 Electron 预加载器的情况下选择原生路径。
const desktopClipboardBridge = vi.hoisted(() => ({
  current: null as null | {
    readClipboardText: () => Promise<string>;
    writeClipboardText: (value: string) => Promise<void>;
    pasteClipboard?: () => Promise<void>;
  },
}));
vi.mock("@/host-shell/electron-bridge", () => ({ readElectronDesktopBridge: () => desktopClipboardBridge.current }));

// host and root own one controlled composer field for each test.
// host 与 root 为每项测试持有一个受控会话输入框。
let host: HTMLDivElement;
let root: Root;
// originalClipboard restores the browser clipboard property after each case.
// originalClipboard 在每个用例结束后恢复浏览器剪贴板属性。
let originalClipboard: PropertyDescriptor | undefined;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  originalClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
  desktopClipboardBridge.current = null;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  if (originalClipboard) Object.defineProperty(navigator, "clipboard", originalClipboard);
  else Reflect.deleteProperty(navigator, "clipboard");
  vi.unstubAllGlobals();
});

// ControlledComposer keeps the test input on the same state path as a React-controlled draft.
// ControlledComposer 让测试输入框沿用 React 受控草稿的状态路径。
function ControlledComposer({ initial }: { initial: string }) {
  // value is the current draft text edited by menu actions.
  // value 是由菜单操作编辑的当前草稿文本。
  const [value, setValue] = React.useState(initial);
  // fieldRef identifies the exact composer field and its selection.
  // fieldRef 标识精确的会话输入框及其选区。
  const fieldRef = React.useRef<HTMLTextAreaElement | null>(null);
  return (
    <ComposerTextContextMenu
      disabled={false}
      enabled
      onChange={setValue}
      ownerKey="conversation-one"
      textareaRef={fieldRef}
      value={value}
    >
      <Textarea aria-label="composer" onChange={(event) => setValue(event.currentTarget.value)} ref={fieldRef} value={value} />
    </ComposerTextContextMenu>
  );
}

// installClipboard provides the browser clipboard contract used by an older Electron shell.
// installClipboard 提供旧版 Electron 壳使用的网页剪贴板契约。
function installClipboard(text: string, writeText = vi.fn(async (_value: string) => {})) {
  // readText returns the clipboard payload for a menu Paste action.
  // readText 返回菜单“粘贴”操作所需的剪贴板内容。
  const readText = vi.fn(async () => text);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { readText, writeText },
  });
  return { readText, writeText };
}

// openMenu reproduces a right-click on the real input after setting its text selection.
// openMenu 在设置输入框文本选区后复现对实际输入框的右键操作。
async function openMenu(start: number, end: number): Promise<HTMLTextAreaElement> {
  // field is the live controlled textarea, not a synthetic draft object.
  // field 是实时受控输入框，而不是模拟的草稿对象。
  const field = host.querySelector("textarea")!;
  field.focus();
  field.setSelectionRange(start, end);
  await act(async () => field.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, button: 2, clientX: 20, clientY: 20 }),
  ));
  return field;
}

// choose selects one visible right-click action by its stable translation key.
// choose 根据稳定翻译键选择一项可见的右键操作。
async function choose(key: string): Promise<void> {
  // item is the actual Radix menu item rendered into the document portal.
  // item 是渲染到文档门户中的实际 Radix 菜单项。
  const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((entry) => entry.textContent === key)!;
  expect(item).toBeTruthy();
  await act(async () => item.click());
}

it("shows four actions and copies only the selected composer text", async () => {
  const clipboard = installClipboard("");
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(0, 5);
  expect([...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].map((item) => item.textContent)).toEqual([
    "composer.contextMenu.copy",
    "composer.contextMenu.paste",
    "composer.contextMenu.cut",
    "composer.contextMenu.selectAll",
  ]);
  await choose("composer.contextMenu.copy");
  expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith("alpha");
  expect(field.value).toBe("alpha beta");
  expect(document.activeElement).toBe(field);
});

it("keeps focus and the selection in the composer while the menu is open", async () => {
  installClipboard("");
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(6, 10);
  expect(document.querySelector('[role="menu"]')).toBeTruthy();
  expect(document.activeElement).toBe(field);
  expect([field.selectionStart, field.selectionEnd]).toEqual([6, 10]);
  // item is hovered by mouse; Radix would normally move focus onto it.
  // item 被鼠标悬停；Radix 默认会把焦点移到该菜单项上。
  const item = document.querySelector<HTMLElement>('[role="menuitem"]')!;
  await act(async () => item.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse" })));
  expect(document.activeElement).toBe(field);
  expect(document.querySelector('[role="menu"]')).toBeTruthy();
});

it("hands focus to the menu when opened by keyboard so arrow keys and Enter drive it", async () => {
  installClipboard("NEW");
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = host.querySelector("textarea")!;
  field.focus();
  field.setSelectionRange(0, 5);
  // Shift+F10 precedes the keyboard contextmenu event with no pointer press in between.
  // Shift+F10 先于键盘触发的 contextmenu 事件到达，其间没有指针按下。
  await act(async () => {
    field.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "F10", shiftKey: true }));
    field.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, button: 0, clientX: 20, clientY: 20 }));
  });
  // focusedLabel reads the row Radix currently keeps focused.
  // focusedLabel 读取 Radix 当前保持聚焦的菜单行。
  const focusedLabel = () => (document.activeElement as HTMLElement | null)?.getAttribute("role") === "menuitem"
    ? document.activeElement!.textContent
    : null;
  await act(async () => { await vi.waitFor(() => expect(focusedLabel()).toBe("composer.contextMenu.copy")); });
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "ArrowDown" })));
  await act(async () => { await vi.waitFor(() => expect(focusedLabel()).toBe("composer.contextMenu.paste")); });
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })));
  await act(async () => { await vi.waitFor(() => expect(field.value).toBe("NEW beta")); });
  expect(document.activeElement).toBe(field);
});

it("cuts only after clipboard write succeeds and restores the controlled caret", async () => {
  const clipboard = installClipboard("");
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(6, 10);
  await choose("composer.contextMenu.cut");
  expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith("beta");
  expect(field.value).toBe("alpha ");
  expect([field.selectionStart, field.selectionEnd]).toEqual([6, 6]);
});

it("pastes into the saved selection through controlled state", async () => {
  const clipboard = installClipboard("NEW");
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(0, 5);
  await choose("composer.contextMenu.paste");
  expect(clipboard.readText).toHaveBeenCalledOnce();
  expect(field.value).toBe("NEW beta");
  expect([field.selectionStart, field.selectionEnd]).toEqual([3, 3]);
});

it("selects the whole composer field without selecting surrounding page text", async () => {
  installClipboard("");
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(2, 2);
  await choose("composer.contextMenu.selectAll");
  expect(document.activeElement).toBe(field);
  expect([field.selectionStart, field.selectionEnd]).toEqual([0, 10]);
});

it("keeps selected text when the clipboard rejects Cut", async () => {
  installClipboard("", vi.fn(async () => { throw new Error("Clipboard denied"); }));
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(0, 5);
  await choose("composer.contextMenu.cut");
  expect(field.value).toBe("alpha beta");
});

it("uses the desktop text bridge when native Paste is unavailable", async () => {
  // nativeRead supplies the text returned by the owning desktop process.
  // nativeRead 提供所属桌面进程返回的文本。
  const nativeRead = vi.fn(async () => "desktop");
  // nativeWrite observes text written through the owning desktop process.
  // nativeWrite 观察通过所属桌面进程写入的文本。
  const nativeWrite = vi.fn(async (_value: string) => {});
  desktopClipboardBridge.current = { readClipboardText: nativeRead, writeClipboardText: nativeWrite };
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(0, 5);
  await choose("composer.contextMenu.paste");
  expect(nativeRead).toHaveBeenCalledOnce();
  expect(field.value).toBe("desktop beta");
  await openMenu(0, 7);
  await choose("composer.contextMenu.copy");
  expect(nativeWrite).toHaveBeenCalledExactlyOnceWith("desktop");
});

it("routes Paste through Electron's page command when available", async () => {
  // pasteTarget records the focused element when the host command runs; Electron pastes there.
  // pasteTarget 记录宿主命令执行时的焦点元素；Electron 会粘贴到该元素。
  let pasteTarget: { element: Element | null; start: number | null; end: number | null } | null = null;
  // nativePaste represents the host command that emits the normal page paste event.
  // nativePaste 代表发出正常页面粘贴事件的宿主命令。
  const nativePaste = vi.fn(async () => {
    const focused = document.activeElement as HTMLTextAreaElement | null;
    pasteTarget = { element: focused, start: focused?.selectionStart ?? null, end: focused?.selectionEnd ?? null };
  });
  desktopClipboardBridge.current = {
    readClipboardText: vi.fn(async () => "text fallback must not run"),
    writeClipboardText: vi.fn(async (_value: string) => {}),
    pasteClipboard: nativePaste,
  };
  await act(async () => root.render(<ControlledComposer initial="alpha beta" />));
  const field = await openMenu(0, 5);
  await choose("composer.contextMenu.paste");
  expect(nativePaste).toHaveBeenCalledOnce();
  expect(pasteTarget).toEqual({ element: field, start: 0, end: 5 });
});
