// @vitest-environment jsdom
import { act, useEffect, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ElectronLocalBrowserBridge, ElectronLocalBrowserSnapshot } from "@/host-shell/electron-bridge";
import { openWorkspaceContextTab, emptyWorkspaceContextTabState, workspaceContextTabStateCache } from "@/components/workspace/workspace-shell/context-tabs";
import { useCloseWorkspaceContextTab } from "@/components/workspace/workspace-shell/use-close-workspace-context-tab";
import { AlertDialog, AlertDialogContent, AlertDialogDescription, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { PanelTab } from "@/ui-kit/primitives/panel-tab";
import { toast } from "@/ui-kit/primitives/toast";
import { ClientBrowserArea } from "./client-browser-area";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key);
  return { ...actual, useI18n: () => ({ locale: "zh-CN", setLocale: () => undefined, t }) };
});

// host and root own the isolated React fixture; no real browser or user profile is touched.
// host 和 root 持有隔离 React 用例；不接触真实浏览器或用户配置。
let host: HTMLDivElement;
let root: Root;
// snapshot and publish represent authoritative native events, including redirects and history changes.
// snapshot 和 publish 代表权威原生事件，包括重定向与历史变化。
let snapshot: ElectronLocalBrowserSnapshot;
let publish: (next: ElectronLocalBrowserSnapshot) => void;
// bridge records the exact preload contract used by the component.
// bridge 记录组件使用的精确预加载契约。
let bridge: ElectronLocalBrowserBridge;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class {
    // observe and disconnect are inert because test rectangles remain fixed.
    // observe 和 disconnect 为空操作，因为测试矩形保持不变。
    observe() {}
    disconnect() {}
  });
  snapshot = { ownerSessionId: "owner", activeId: "a", tabs: [
    { id: "a", title: "A", url: "https://a.example/", faviconUrl: null, loading: false, canGoBack: false, canGoForward: false },
    { id: "b", title: "B", url: "https://b.example/", faviconUrl: null, loading: false, canGoBack: true, canGoForward: false },
  ] };
  bridge = {
    initialize: vi.fn(async () => {}),
    closeOwner: vi.fn(async () => true),
    setOwner: vi.fn(async () => snapshot), snapshot: vi.fn(async () => snapshot),
    open: vi.fn(async () => "c"), navigate: vi.fn(async () => {}),
    select: vi.fn(async () => {}), close: vi.fn(async () => {}),
    history: vi.fn(async () => {}), move: vi.fn(async () => {}),
    openDeveloperTools: vi.fn(async () => {}), openExternal: vi.fn(async () => {}),
    openExternalUrl: vi.fn(async () => {}),
    viewport: vi.fn(async () => {}),
    subscribeChanged: (handler) => { publish = handler; return () => {}; },
  };
  Object.defineProperty(window, "vulcanDesktop", { configurable: true, value: { localBrowser: bridge } });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0, y: 0, left: 0, top: 0, width: 700, height: 600, right: 700, bottom: 600, toJSON: () => ({}),
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  workspaceContextTabStateCache.clear();
  Reflect.deleteProperty(window, "vulcanDesktop");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

// mount renders the current conversation and waits for the native-owner handshake.
// mount 渲染当前会话，并等待原生归属握手。
// Returns when all initial bridge promises have settled.
// 初始桥 Promise 完成后返回。
async function mount(): Promise<void> {
  await act(async () => root.render(<ClientBrowserArea ownerSessionId="owner" />));
}

it("hides the native viewport under an application dialog and restores it after dismissal", async () => {
  // render keeps the actual browser mounted while the shared application dialog enters and exits.
  // render 在共享应用弹窗进入与退出期间保持真实浏览器组件挂载。
  const render = (open: boolean) => root.render(<>
    <ClientBrowserArea ownerSessionId="owner" />
    <AlertDialog open={open}>
      <AlertDialogContent>
        <AlertDialogTitle>Fixture</AlertDialogTitle>
        <AlertDialogDescription>Application dialog fixture</AlertDialogDescription>
      </AlertDialogContent>
    </AlertDialog>
  </>);
  await act(async () => render(false));
  await vi.waitFor(() => expect(bridge.viewport).toHaveBeenLastCalledWith(expect.objectContaining({ visible: true })));
  await act(async () => render(true));
  expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
  await vi.waitFor(() => expect(bridge.viewport).toHaveBeenLastCalledWith(expect.objectContaining({ visible: false })));
  await act(async () => render(false));
  await vi.waitFor(() => expect(bridge.viewport).toHaveBeenLastCalledWith(expect.objectContaining({ visible: true })));
});

// CloseTabFixture renders the workspace close action with the shared tab control.
// CloseTabFixture 用共享页签控件呈现工作区关闭动作。
// owner identifies the active route; its tab state is restored from the same cache as the workspace.
// owner 标识当前路由，页签状态从与工作区相同的缓存恢复。
function CloseTabFixture({ owner }: { owner: string }) {
  // entry pairs the visible tabs with their originating route, even during navigation.
  // entry 将可见页签与其所属路由绑定，切换路由期间也保持这一归属。
  const [entry, setEntry] = useState({ cacheKey: owner, state: emptyWorkspaceContextTabState() });
  useEffect(() => {
    // Read the saved route or open a local browser tab for this fixture.
    // 读取已保存的路由状态，首次进入时为用例打开本地浏览器页签。
    const state = workspaceContextTabStateCache.get(owner)
      ?? openWorkspaceContextTab(emptyWorkspaceContextTabState(), "local-browser", (key) => key);
    workspaceContextTabStateCache.set(owner, state);
    setEntry({ cacheKey: owner, state });
  }, [owner]);
  // closeTab is the production handler invoked by the panel's close control.
  // closeTab 是面板关闭控件调用的生产处理函数。
  const closeTab = useCloseWorkspaceContextTab(entry, owner, owner, setEntry);
  return <div role="tablist">{entry.state.tabs.map((tab) => (
    <PanelTab key={tab.id} active label={owner} title={owner} closeLabel={`Close ${owner}`}
      onSelect={vi.fn()} onClose={() => closeTab(tab.id)} />
  ))}</div>;
}

// browserCloseButton returns the close control for the requested fixture owner, if present.
// browserCloseButton 返回指定用例会话的关闭控件，控件不存在时返回空值。
function browserCloseButton(owner: string): HTMLButtonElement | null {
  return host.querySelector<HTMLButtonElement>(`button[aria-label="Close ${owner}"]`);
}

it("closes the local browser directly and waits for native destruction", async () => {
  // finish delays native destruction to exercise repeated clicks while closure is in flight.
  // finish 暂缓原生销毁，用于检验关闭期间的连续点击。
  let finish!: (closed: boolean) => void;
  vi.mocked(bridge.closeOwner).mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<CloseTabFixture owner="owner" />));
  await act(async () => { browserCloseButton("owner")!.click(); browserCloseButton("owner")!.click(); });
  expect(bridge.closeOwner).toHaveBeenCalledExactlyOnceWith("owner", true);
  expect(bridge.snapshot).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(browserCloseButton("owner")).not.toBeNull();
  await act(async () => finish(true));
  expect(browserCloseButton("owner")).toBeNull();
});

it("keeps the browser tab open when native closure fails and lets the user retry", async () => {
  // report captures the workspace notification without mounting an application-wide toast host.
  // report 捕获工作区通知，无需挂载全局通知容器。
  const report = vi.spyOn(toast, "error").mockImplementation(() => "");
  vi.mocked(bridge.closeOwner).mockRejectedValueOnce(new Error("native close failed"));
  await act(async () => root.render(<CloseTabFixture owner="owner" />));
  await act(async () => browserCloseButton("owner")!.click());
  expect(report).toHaveBeenCalledWith("native close failed");
  expect(browserCloseButton("owner")).not.toBeNull();
  await act(async () => browserCloseButton("owner")!.click());
  expect(bridge.closeOwner).toHaveBeenCalledTimes(2);
  expect(browserCloseButton("owner")).toBeNull();
});

it("only removes the originating conversation's tab after switching conversations", async () => {
  // finish allows navigation before the originating native close operation completes.
  // finish 让用例在发起会话的原生关闭完成前切换会话。
  let finish!: (closed: boolean) => void;
  vi.mocked(bridge.closeOwner).mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<CloseTabFixture owner="owner" />));
  await act(async () => browserCloseButton("owner")!.click());
  await act(async () => root.render(<CloseTabFixture owner="other" />));
  await act(async () => finish(true));
  expect(browserCloseButton("other")).not.toBeNull();
  await act(async () => root.render(<CloseTabFixture owner="owner" />));
  expect(browserCloseButton("owner")).toBeNull();
});

// addressInput locates the address bar; returns its actual DOM input.
// addressInput 定位地址栏；返回真实 DOM 输入框。
function addressInput(): HTMLInputElement {
  return host.querySelector<HTMLInputElement>('[aria-label="本地浏览地址"]')!;
}

// editAddress dispatches the native input event React observes; value is a user's unfinished draft.
// editAddress 派发 React 监听的原生输入事件；value 是用户尚未提交的草稿。
// Returns after React applies the edit.
// React 应用编辑后返回。
async function editAddress(value: string): Promise<void> {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(addressInput(), value);
    addressInput().dispatchEvent(new Event("input", { bubbles: true }));
  });
}

it("updates a focused address on redirect and does not replace an actual unsubmitted draft", async () => {
  await mount();
  addressInput().focus();
  snapshot.tabs[0] = { ...snapshot.tabs[0], url: "https://a.example/redirect" };
  await act(async () => publish({ ...snapshot }));
  expect(addressInput().value).toBe("https://a.example/redirect");
  await editAddress("https://next.example/");
  snapshot.tabs[0] = { ...snapshot.tabs[0], url: "https://a.example/#state" };
  await act(async () => publish({ ...snapshot }));
  expect(addressInput().value).toBe("https://next.example/");
  await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(bridge.navigate).toHaveBeenCalledWith("a", "https://next.example/");
  snapshot.tabs[0] = { ...snapshot.tabs[0], url: "https://next.example/final" };
  await act(async () => publish({ ...snapshot }));
  expect(addressInput().value).toBe("https://next.example/final");
});

// Native paths reach the host verbatim on both open and navigate; only the host may canonicalize them.
// 原生路径在打开与导航时原样到达宿主；仅由宿主进行规范化。
it.each([String.raw`C:\Users\lingeasy\.vulcan-debug\projects\vulcan_project\pelican-bicycle.html`,
  "C:/my files/page.html", "file:///C:/my%20files/page.html"])("passes local address %s to the desktop host", async (address) => {
  await mount();
  await editAddress(address);
  await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(bridge.navigate).toHaveBeenCalledWith("a", address);
  snapshot = { ownerSessionId: "owner", activeId: null, tabs: [] };
  await act(async () => publish(snapshot));
  await editAddress(address);
  await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(bridge.open).toHaveBeenCalledWith(address);
});

it("resets drafts when switching tabs and uses native history availability", async () => {
  await mount();
  await editAddress("unsubmitted.example");
  expect(host.querySelector<HTMLButtonElement>('[aria-label="后退"]')!.disabled).toBe(true);
  snapshot.activeId = "b";
  await act(async () => publish({ ...snapshot }));
  expect(addressInput().value).toBe("https://b.example/");
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="后退"]')!.click());
  expect(bridge.history).toHaveBeenCalledWith("b", "back");
  snapshot.tabs[1] = { ...snapshot.tabs[1], loading: true };
  await act(async () => publish({ ...snapshot }));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="停止加载"]')!.click());
  expect(bridge.history).toHaveBeenLastCalledWith("b", "stop");
});

it("supports keyboard tab switching and reordering without navigation", async () => {
  await mount();
  // first is the tab button, not its independent close button.
  // first 是标签按钮，不是其独立关闭按钮。
  const first = host.querySelector<HTMLButtonElement>('[role="tab"]')!;
  await act(async () => first.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
  expect(bridge.select).toHaveBeenCalledWith("b");
  await act(async () => first.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", altKey: true, bubbles: true })));
  expect(bridge.move).toHaveBeenCalledWith("a", "b");
  expect(bridge.navigate).not.toHaveBeenCalled();
});

it("moves a dragged tab to its drop destination", async () => {
  await mount();
  // tabs are native draggable wrappers around the tab and close buttons.
  // tabs 是包裹标签与关闭按钮的原生可拖动容器。
  const tabs = host.querySelectorAll('[draggable="true"]');
  // transfer supplies only the browser drag API used by the handler.
  // transfer 仅提供处理器使用的浏览器拖动 API。
  const transfer = { setData: vi.fn(), effectAllowed: "", dropEffect: "" };
  // start contains no external data and establishes the internal drag identity.
  // start 不含外部数据，并建立内部拖动身份。
  const start = new Event("dragstart", { bubbles: true });
  Object.defineProperty(start, "dataTransfer", { value: transfer });
  await act(async () => tabs[0].dispatchEvent(start));
  await act(async () => tabs[1].dispatchEvent(new Event("drop", { bubbles: true, cancelable: true })));
  expect(bridge.move).toHaveBeenCalledWith("a", "b");
});

it("sends and clears a real responsive viewport through the native bridge", async () => {
  await mount();
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="自由尺寸"]')!.click());
  await act(async () => new Promise((resolve) => setTimeout(resolve, 40)));
  expect(bridge.viewport).toHaveBeenCalledWith(expect.objectContaining({ deviceSize: { width: 393, height: 852 } }));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="退出自由尺寸"]')!.click());
  await act(async () => new Promise((resolve) => setTimeout(resolve, 40)));
  expect(bridge.viewport).toHaveBeenLastCalledWith(expect.objectContaining({ deviceSize: null }));
});

it("opens page developer tools from the menu while hiding the native view behind the menu", async () => {
  await mount();
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="浏览器菜单"]')!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true })));
  await act(async () => new Promise((resolve) => setTimeout(resolve, 40)));
  expect(bridge.viewport).toHaveBeenLastCalledWith(expect.objectContaining({ visible: false }));
  // toolsItem belongs to the current page's menu, independently of the workbench console entry.
  // toolsItem 属于当前页面菜单，与工作台控制台入口独立。
  const toolsItem = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
    .find((item) => item.textContent?.includes("打开调试工具"))!;
  await act(async () => toolsItem.click());
  expect(bridge.openDeveloperTools).toHaveBeenCalledWith("a");
  await act(async () => new Promise((resolve) => setTimeout(resolve, 40)));
  expect(bridge.viewport).toHaveBeenLastCalledWith(expect.objectContaining({ visible: true }));
});
