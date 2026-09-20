// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  listCoreBrowserDialogs,
  listCoreBrowserDownloads,
  listCoreBrowserFileChoosers,
  listCoreBrowserPages,
  listCoreBrowserSessions,
  openCoreBrowserStream,
  requestCoreBrowser,
  type CoreBrowserDownload,
  type CoreBrowserLease,
  type CoreBrowserPage,
  type CoreBrowserSession,
  type CoreBrowserStreamCallbacks,
} from "@/service/core-browser-service";
import { BrowserArea, coreBrowserAddress } from "./BrowserArea";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US" }) }));
vi.mock("@/service/core-browser-service", () => ({
  getCoreBrowserRuntimeStatus: vi.fn(async () => ({ ready: true, canInstall: true, diagnostic: null, installation: null })),
  requestCoreBrowser: vi.fn(),
  listCoreBrowserPages: vi.fn(),
  listCoreBrowserDownloads: vi.fn(),
  listCoreBrowserDialogs: vi.fn(),
  listCoreBrowserFileChoosers: vi.fn(),
  listCoreBrowserSessions: vi.fn(),
  fetchCoreBrowserDownload: vi.fn(),
  openCoreBrowserStream: vi.fn(),
}));

// host contains the mounted browser panel for this isolated projection test.
// host 保存本次隔离投影测试挂载的浏览器面板。
let host: HTMLDivElement;
// root owns the React tree and its polling cleanup.
// root 持有 React 组件树及其轮询清理。
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.clearAllMocks();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.mocked(listCoreBrowserDialogs).mockResolvedValue([]);
  vi.mocked(listCoreBrowserFileChoosers).mockResolvedValue([]);
  vi.mocked(listCoreBrowserDownloads).mockResolvedValue([]);
  vi.mocked(openCoreBrowserStream).mockResolvedValue(() => {});
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// browserTabs reads the rendered browser choices without opening a projection implicitly.
// browserTabs 读取可选浏览器列表，不隐式打开投影。
function browserTabs(): HTMLButtonElement[] {
  return [...host.querySelectorAll<HTMLButtonElement>('[role="tablist"][aria-label="Active AI browsers"] [role="tab"]')];
}

// selectBrowser makes the explicit viewer choice required by the new disconnected default.
// selectBrowser 执行新默认未连接状态下所需的明确观察选择。
// index is the Core list position; returns after the click has settled.
// index 是 Core 列表位置；在点击完成后返回。
async function selectBrowser(index = 0): Promise<void> {
  await vi.waitFor(() => expect(browserTabs().length).toBeGreaterThan(index));
  await act(async () => browserTabs()[index].click());
}

it("normalizes browser addresses before Core navigation", () => {
  expect(coreBrowserAddress("www.baidu.com")).toBe("https://www.baidu.com/");
  expect(coreBrowserAddress("localhost:5173/test")).toBe("http://localhost:5173/test");
  expect(coreBrowserAddress("http://127.0.0.1:61449/")).toBe("http://127.0.0.1:61449/");
  expect(() => coreBrowserAddress("file:///private.txt")).toThrow();
});

// A crashed host must remain selectable and show its blocked state instead of vanishing.
// 崩溃宿主必须保持可选择并展示阻塞状态，而不是消失。
it("keeps a crashed Playwright browser visible with an explicit diagnostic", async () => {
  const session: CoreBrowserSession = {
    sessionId: "crashed-browser", ownerSessionId: "conversation-a", status: "crashed",
    profileMode: "ephemeral", currentUrl: null, leaseState: "blocked",
    blockedReason: "automation.close outcome unknown", automationState: "detached",
    pageGeneration: "1", viewportGeneration: "1", controlEpoch: "2", hostGeneration: "1",
    viewportSize: null, pageCount: 0, requiresExplicitReopen: true,
  };
  vi.mocked(listCoreBrowserSessions).mockResolvedValue([session]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);
  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  expect(browserTabs()[0].textContent).toContain("Crashed");
  expect(host.textContent).toContain("The Playwright browser host exited unexpectedly");
  expect(host.textContent).toContain("automation.close outcome unknown");
  expect(host.textContent).not.toContain("Connecting view");
  expect([...host.querySelectorAll('button')].some(button => button.textContent === 'Take control')).toBe(false);
});

it("sends a bare domain through the address bar as an HTTPS Core navigation", async () => {
  // session is a running Core browser owned by the visible conversation.
  // session 是归属当前可见会话的运行中 Core 浏览器。
  const session: CoreBrowserSession = {
    sessionId: "browser-a", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: "https://old.example/", leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 1, requiresExplicitReopen: false,
  };
  // page is the existing target whose address bar will be edited by the user.
  // page 是用户即将修改地址的现有页面目标。
  const page: CoreBrowserPage = {
    pageId: "page-a", targetId: "target-a", url: "https://old.example/",
    openerPageId: null, hostGeneration: 1,
  };
  // heldSession is the Core-confirmed lease projection after acquisition.
  // heldSession 是取得租约后 Core 确认的控制权投影。
  const heldSession: CoreBrowserSession = { ...session, leaseState: "held", controlEpoch: "2" };
  // held tracks the actual transition observed by Core list polling.
  // held 跟踪 Core 列表轮询能观察到的真实授权转换。
  let held = false;
  vi.mocked(listCoreBrowserSessions).mockImplementation(async () => [held ? heldSession : session]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([page]);
  vi.mocked(requestCoreBrowser).mockImplementation((_owner, method) => {
    if (method === "browser.lease.acquire") {
      held = true;
      return Promise.resolve({ session: heldSession, leaseSecret: "lease-a", heartbeatIntervalMs: 5000 } as never);
    }
    if (method === "browser.session.navigate") {
      return Promise.resolve({ ...session, currentUrl: "https://www.baidu.com/", pageGeneration: "2" } as never);
    }
    return Promise.resolve(undefined as never);
  });

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  await vi.waitFor(() => expect(host.querySelector('[data-browser-page-id="page-a"]')).not.toBeNull());
  const acquire = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes("Take control"))!;
  await act(async () => acquire.click());
  await vi.waitFor(() => expect(host.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(false));
  const address = host.querySelector<HTMLInputElement>('input[aria-label="Page URL"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(address, "www.baidu.com");
    address.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => address.closest("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  await vi.waitFor(() => expect(vi.mocked(requestCoreBrowser)).toHaveBeenCalledWith(
    "conversation-a", "browser.session.navigate", expect.objectContaining({
      sessionId: "browser-a", pageId: "page-a", url: "https://www.baidu.com/",
    }),
  ));
});

it("ignores a previous browser's page response after selecting another browser", async () => {
  // sessionA and sessionB belong to one conversation but have distinct Core browser identities.
  // sessionA 与 sessionB 属于同一会话，但拥有不同的 Core 浏览器身份。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 1, requiresExplicitReopen: false,
  };
  const sessionB: CoreBrowserSession = { ...sessionA, sessionId: "bbbbbbbb-browser" };
  // resolveOldPages holds A's deliberately late HTTP response until B is visible.
  // resolveOldPages 暂存 A 的迟到 HTTP 响应，直至 B 已显示。
  let resolveOldPages!: (pages: CoreBrowserPage[]) => void;
  // oldPages models an in-flight poll that cannot be cancelled by the HTTP client.
  // oldPages 模拟 HTTP 客户端无法取消的在途轮询。
  const oldPages = new Promise<CoreBrowserPage[]>((resolve) => { resolveOldPages = resolve; });
  // resolveOldDownloads holds A's delayed download metadata until B has loaded.
  // resolveOldDownloads 暂存 A 的下载元数据，直至 B 已加载。
  let resolveOldDownloads!: (downloads: CoreBrowserDownload[]) => void;
  // oldDownloads verifies that non-page metadata follows the same ownership rule.
  // oldDownloads 验证非页面元数据遵循相同的归属规则。
  const oldDownloads = new Promise<CoreBrowserDownload[]>((resolve) => { resolveOldDownloads = resolve; });
  // pageB is the only page that should remain visible after selection changes.
  // pageB 是切换选择后唯一应保持可见的页面。
  const pageB: CoreBrowserPage = {
    pageId: "page-b", targetId: "target-b", url: "https://browser-b.example/",
    openerPageId: null, hostGeneration: 1,
  };
  vi.mocked(listCoreBrowserSessions).mockResolvedValue([sessionA, sessionB]);
  vi.mocked(listCoreBrowserPages).mockImplementation((_owner, sessionId) =>
    sessionId === sessionA.sessionId ? oldPages : Promise.resolve([pageB]));
  vi.mocked(listCoreBrowserDownloads).mockImplementation((_owner, sessionId) =>
    sessionId === sessionA.sessionId ? oldDownloads : Promise.resolve([{
      downloadId: "download-b", sessionId: sessionB.sessionId, pageId: pageB.pageId,
      fileName: "browser-b.txt", status: "completed", byteLength: 1, sha256: null,
    }]));

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  expect(vi.mocked(listCoreBrowserPages)).not.toHaveBeenCalled();
  await selectBrowser(0);
  await vi.waitFor(() => expect(vi.mocked(listCoreBrowserPages)).toHaveBeenCalledWith("conversation-a", sessionA.sessionId));
  // tabs are rendered in the Core list order, so the second tab selects B.
  // tabs 按 Core 列表顺序渲染，因此第二个标签选择 B。
  await selectBrowser(1);
  await vi.waitFor(() => expect(host.textContent).toContain("browser-b.example"));
  await vi.waitFor(() => expect(host.textContent).toContain("browser-b.txt"));
  await act(async () => resolveOldPages([{
    pageId: "page-a", targetId: "target-a", url: "https://browser-a.example/",
    openerPageId: null, hostGeneration: 1,
  }]));
  await act(async () => resolveOldDownloads([{
    downloadId: "download-a", sessionId: sessionA.sessionId, pageId: "page-a",
    fileName: "browser-a.txt", status: "completed", byteLength: 1, sha256: null,
  }]));
  expect(host.textContent).toContain("browser-b.example");
  expect(host.textContent).toContain("browser-b.txt");
  expect(host.textContent).not.toContain("browser-a.example");
  expect(host.textContent).not.toContain("browser-a.txt");
});

it("keeps the newer browser list when an older refresh finishes last", async () => {
  // sessionA and sessionB are distinct list entries in the same conversation.
  // sessionA 与 sessionB 是同一会话内的不同列表项。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  const sessionB: CoreBrowserSession = { ...sessionA, sessionId: "bbbbbbbb-browser" };
  // resolveOldList completes the first manual refresh after the newer one has arrived.
  // resolveOldList 在更新的刷新结果到达之后完成第一次手动刷新。
  let resolveOldList!: (sessions: CoreBrowserSession[]) => void;
  // oldList models an out-of-order HTTP response from the same Core owner.
  // oldList 模拟同一 Core 所有者的乱序 HTTP 响应。
  const oldList = new Promise<CoreBrowserSession[]>((resolve) => { resolveOldList = resolve; });
  vi.mocked(listCoreBrowserSessions)
    .mockResolvedValueOnce([sessionA])
    .mockImplementationOnce(() => oldList)
    .mockResolvedValueOnce([sessionA, sessionB]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await vi.waitFor(() => expect(browserTabs()).toHaveLength(1));
  // refresh is the panel's explicit list request, independent of the page stream.
  // refresh 是面板显式发出的列表请求，与页面流无关。
  const refresh = host.querySelector<HTMLButtonElement>('[aria-label="Refresh browser list"]')!;
  await act(async () => { refresh.click(); refresh.click(); });
  await vi.waitFor(() => expect(browserTabs()).toHaveLength(2));
  await act(async () => resolveOldList([sessionA]));
  expect(browserTabs()).toHaveLength(2);
});

it("does not display an older refresh failure after a newer browser list succeeds", async () => {
  // sessionA and sessionB make a stale-error regression visible in the browser tabs.
  // sessionA 与 sessionB 使过期错误回归能从浏览器页签中观察到。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  const sessionB: CoreBrowserSession = { ...sessionA, sessionId: "bbbbbbbb-browser" };
  // rejectOldList delays the first manual refresh failure until the second one has succeeded.
  // rejectOldList 将首次手动刷新的失败延迟至第二次刷新成功之后。
  let rejectOldList!: (failure: Error) => void;
  // oldList represents the older Core request whose failure must be ignored.
  // oldList 代表必须忽略其失败的较早 Core 请求。
  const oldList = new Promise<CoreBrowserSession[]>((_resolve, reject) => { rejectOldList = reject; });
  vi.mocked(listCoreBrowserSessions)
    .mockResolvedValueOnce([sessionA])
    .mockImplementationOnce(() => oldList)
    .mockResolvedValueOnce([sessionA, sessionB]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await vi.waitFor(() => expect(browserTabs()).toHaveLength(1));
  // Refresh twice to establish a newer accepted result before the old request fails.
  // 连续刷新两次，使旧请求失败前已经存在更新的有效结果。
  const refresh = host.querySelector<HTMLButtonElement>('[aria-label="Refresh browser list"]')!;
  await act(async () => { refresh.click(); refresh.click(); });
  await vi.waitFor(() => expect(browserTabs()).toHaveLength(2));
  await act(async () => rejectOldList(new Error("stale list failure")));
  expect(host.textContent).not.toContain("stale list failure");
  expect(browserTabs()).toHaveLength(2);
});

it("keeps a newer browser-list failure visible when an older success arrives last", async () => {
  // sessionA is the accepted list before two overlapping refreshes.
  // sessionA 是两次重叠刷新之前已经接受的列表内容。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  // sessionB exposes an incorrectly accepted response as an extra browser tab.
  // sessionB 通过额外浏览器页签暴露错误接受的旧响应。
  const sessionB: CoreBrowserSession = { ...sessionA, sessionId: "bbbbbbbb-browser" };
  // resolveOldList lets the older successful request finish after the newer failure.
  // resolveOldList 让较早的成功请求在较新请求失败后完成。
  let resolveOldList!: (sessions: CoreBrowserSession[]) => void;
  // oldList represents the request that must not hide the newer failure.
  // oldList 代表不得掩盖较新失败的旧请求。
  const oldList = new Promise<CoreBrowserSession[]>((resolve) => { resolveOldList = resolve; });
  vi.mocked(listCoreBrowserSessions)
    .mockResolvedValueOnce([sessionA])
    .mockImplementationOnce(() => oldList)
    .mockRejectedValueOnce(new Error("newer list failure"))
    .mockResolvedValueOnce([sessionA, sessionB]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await vi.waitFor(() => expect(browserTabs()).toHaveLength(1));
  // refresh issues requests in the same order as the Core list mock.
  // refresh 按 Core 列表模拟请求的顺序发出调用。
  const refresh = host.querySelector<HTMLButtonElement>('[aria-label="Refresh browser list"]')!;
  await act(async () => { refresh.click(); refresh.click(); });
  await vi.waitFor(() => expect(host.querySelector('[role="alert"]')?.textContent).toContain("newer list failure"));
  await act(async () => resolveOldList([sessionA, sessionB]));
  expect(browserTabs()).toHaveLength(1);
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("newer list failure");
  await act(async () => refresh.click());
  await vi.waitFor(() => expect(browserTabs()).toHaveLength(2));
  expect(host.querySelector('[role="alert"]')).toBeNull();
});

it("clears the previous conversation's browser projection before the new owner responds", async () => {
  // sessionA and sessionB have different trusted VulcanCode conversation owners.
  // sessionA 与 sessionB 具有不同的可信 VulcanCode 会话归属。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 1, requiresExplicitReopen: false,
  };
  // sessionB is the only browser the second conversation may see.
  // sessionB 是第二个会话唯一允许看到的浏览器。
  const sessionB: CoreBrowserSession = { ...sessionA, sessionId: "bbbbbbbb-browser", ownerSessionId: "conversation-b" };
  // resolveOldList releases an in-flight request from the previous owner after switching.
  // resolveOldList 在切换后放行前一归属尚在途的请求。
  let resolveOldList!: (sessions: CoreBrowserSession[]) => void;
  // oldList is the delayed request that must not republish conversation A.
  // oldList 是不得重新发布会话 A 的迟到请求。
  const oldList = new Promise<CoreBrowserSession[]>((resolve) => { resolveOldList = resolve; });
  // resolveNewList controls when the new owner's projection becomes available.
  // resolveNewList 控制新归属投影何时可用。
  let resolveNewList!: (sessions: CoreBrowserSession[]) => void;
  // newList remains pending while the panel must show no previous-owner metadata.
  // newList 保持未完成时面板不得显示旧归属的元数据。
  const newList = new Promise<CoreBrowserSession[]>((resolve) => { resolveNewList = resolve; });
  vi.mocked(listCoreBrowserSessions).mockImplementation((owner) =>
    owner === "conversation-b" ? newList : Promise.resolve([sessionA]));
  vi.mocked(listCoreBrowserSessions).mockResolvedValueOnce([sessionA]);
  vi.mocked(listCoreBrowserSessions).mockImplementationOnce(() => oldList);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([{
    pageId: "page-a", targetId: "target-a", url: "https://conversation-a.example/",
    openerPageId: null, hostGeneration: 1,
  }]);

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  await vi.waitFor(() => expect(host.textContent).toContain("conversation-a.example"));
  // refresh starts the previous owner's second request before the prop changes.
  // refresh 在属性切换前启动旧归属的第二次请求。
  const refresh = host.querySelector<HTMLButtonElement>('[aria-label="Refresh browser list"]')!;
  await act(async () => refresh.click());
  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-b" />));
  expect(browserTabs()).toHaveLength(0);
  expect(host.textContent).not.toContain("conversation-a.example");
  await act(async () => resolveOldList([sessionA]));
  expect(browserTabs()).toHaveLength(0);
  await act(async () => resolveNewList([sessionB]));
  await vi.waitFor(() => expect(browserTabs()[0]?.title).toBe(sessionB.sessionId));
  expect(browserTabs()[0].title).not.toBe(sessionA.sessionId);
});

it("releases a control grant that arrives after its conversation is replaced", async () => {
  // sessionA is the Core browser whose lease acquisition remains in flight.
  // sessionA 是控制权申请仍在途的 Core 浏览器。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  // resolveLease lets Core grant the old request after the UI has switched owners.
  // resolveLease 让 Core 在界面切换归属后才批准旧请求。
  let resolveLease!: (lease: CoreBrowserLease) => void;
  // pendingLease models a non-cancellable acquire request.
  // pendingLease 模拟无法取消的控制权申请。
  const pendingLease = new Promise<CoreBrowserLease>((resolve) => { resolveLease = resolve; });
  vi.mocked(listCoreBrowserSessions).mockImplementation((owner) =>
    Promise.resolve(owner === "conversation-a" ? [sessionA] : []));
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);
  vi.mocked(requestCoreBrowser).mockImplementation((_owner, method) =>
    method === "browser.lease.acquire" ? pendingLease as Promise<never> : Promise.resolve(undefined as never));

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  // acquire starts the exact old-owner request before the React prop changes.
  // acquire 在 React 属性变化前启动精确的旧归属请求。
  const acquire = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes("Take control")) as HTMLButtonElement;
  await act(async () => acquire.click());
  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-b" />));
  await act(async () => resolveLease({ session: sessionA, leaseSecret: "old-owner-secret", heartbeatIntervalMs: 5000 }));
  await vi.waitFor(() => expect(vi.mocked(requestCoreBrowser)).toHaveBeenCalledWith(
    "conversation-a", "browser.lease.release", { sessionId: sessionA.sessionId, leaseSecret: "old-owner-secret" },
  ));
  expect(browserTabs()).toHaveLength(0);
  expect(host.textContent).not.toContain("Release control");
});

it("releases a control grant that arrives after the browser panel unmounts", async () => {
  // session is the exact browser whose grant arrives after its view disappears.
  // session 是界面消失后授权才返回的精确浏览器。
  const session: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  // finishAcquire resolves the request after the React panel is removed.
  // finishAcquire 在 React 面板卸载之后结束控制权申请。
  let finishAcquire!: (lease: CoreBrowserLease) => void;
  // pendingAcquire models an already-issued Core request that cannot be cancelled locally.
  // pendingAcquire 模拟本地无法取消、已经发出的 Core 请求。
  const pendingAcquire = new Promise<CoreBrowserLease>((resolve) => { finishAcquire = resolve; });
  vi.mocked(listCoreBrowserSessions).mockResolvedValue([session]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);
  vi.mocked(requestCoreBrowser).mockImplementation((_owner, method) =>
    method === "browser.lease.acquire" ? pendingAcquire as Promise<never> : Promise.resolve(undefined as never));

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  const acquire = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes("Take control")) as HTMLButtonElement;
  await act(async () => acquire.click());
  await act(async () => root.render(<></>));
  await act(async () => finishAcquire({ session, leaseSecret: "late-unmount-secret", heartbeatIntervalMs: 5000 }));
  await vi.waitFor(() => expect(vi.mocked(requestCoreBrowser)).toHaveBeenCalledWith(
    "conversation-a", "browser.lease.release", { sessionId: session.sessionId, leaseSecret: "late-unmount-secret" },
  ));
  expect(host.textContent).toBe("");
});

it("releases an already-held browser grant under its original owner on conversation switch", async () => {
  // freeSession is the browser projection before control acquisition.
  // freeSession 是取得控制权之前的浏览器投影。
  const freeSession: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  // heldSession represents Core's newer control epoch after the grant.
  // heldSession 表示 Core 授权后的较新控制代次。
  const heldSession: CoreBrowserSession = { ...freeSession, leaseState: "held", controlEpoch: "2" };
  // held tracks which projection the mocked Core currently publishes.
  // held 跟踪模拟 Core 当前发布的投影。
  let held = false;
  vi.mocked(listCoreBrowserSessions).mockImplementation((owner) =>
    Promise.resolve(owner === "conversation-a" ? [held ? heldSession : freeSession] : []));
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);
  vi.mocked(requestCoreBrowser).mockImplementation((_owner, method) => {
    if (method === "browser.lease.acquire") {
      held = true;
      return Promise.resolve({ session: heldSession, leaseSecret: "held-owner-secret", heartbeatIntervalMs: 5000 } as never);
    }
    if (method === "browser.lease.release") held = false;
    return Promise.resolve(undefined as never);
  });

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  const acquire = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes("Take control")) as HTMLButtonElement;
  await act(async () => acquire.click());
  await vi.waitFor(() => expect(host.textContent).toContain("Release control"));
  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-b" />));
  await vi.waitFor(() => expect(vi.mocked(requestCoreBrowser)).toHaveBeenCalledWith(
    "conversation-a", "browser.lease.release", { sessionId: freeSession.sessionId, leaseSecret: "held-owner-secret" },
  ));
  expect(browserTabs()).toHaveLength(0);
  expect(host.textContent).not.toContain("Release control");
});

it("shares one pending lease release across rapid browser-tab choices", async () => {
  // sessionA is selected and controlled before the two competing tab clicks.
  // sessionA 在两个相互竞争的页签点击前已选中并取得控制权。
  const sessionA: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 0, requiresExplicitReopen: false,
  };
  // sessionB and sessionC are the two later selections under the same owner.
  // sessionB 和 sessionC 是同一归属下先后点击的两个目标。
  const sessionB: CoreBrowserSession = { ...sessionA, sessionId: "bbbbbbbb-browser" };
  const sessionC: CoreBrowserSession = { ...sessionA, sessionId: "cccccccc-browser" };
  // heldA is the actual Core projection returned with the acquired grant.
  // heldA 是取得授权后 Core 返回的真实形态投影。
  const heldA: CoreBrowserSession = { ...sessionA, leaseState: "held", controlEpoch: "2" };
  // finishRelease controls the single Core release shared by both tab choices.
  // finishRelease 控制两个页签选择共用的唯一 Core 释放请求。
  let finishRelease!: () => void;
  // releasePending holds the release until both tab clicks have been made.
  // releasePending 让授权释放等到两个页签均已点击后才完成。
  const releasePending = new Promise<void>((resolve) => { finishRelease = resolve; });
  vi.mocked(listCoreBrowserSessions).mockResolvedValue([sessionA, sessionB, sessionC]);
  vi.mocked(listCoreBrowserPages).mockResolvedValue([]);
  vi.mocked(requestCoreBrowser).mockImplementation((_owner, method) => {
    if (method === "browser.lease.acquire") {
      return Promise.resolve({ session: heldA, leaseSecret: "rapid-switch-secret", heartbeatIntervalMs: 5000 } as never);
    }
    if (method === "browser.lease.release") return releasePending as Promise<never>;
    return Promise.resolve(undefined as never);
  });

  await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
  await selectBrowser();
  const acquire = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes("Take control")) as HTMLButtonElement;
  await act(async () => acquire.click());
  await vi.waitFor(() => expect(host.textContent).toContain("Release control"));
  const tabs = browserTabs();
  await act(async () => { tabs[1].click(); tabs[2].click(); });
  expect(vi.mocked(requestCoreBrowser).mock.calls.filter(([, method]) => method === "browser.lease.release")).toHaveLength(1);
  await act(async () => finishRelease());
  await vi.waitFor(() => expect(tabs[2].getAttribute("aria-selected")).toBe("true"));
  expect(tabs[1].getAttribute("aria-selected")).toBe("false");
});

it("sends a held key's release to the previous page when the user changes page tabs", async () => {
  // freeSession is the initial projection before the test UI acquires control.
  // freeSession 是测试界面取得控制权之前的初始投影。
  const freeSession: CoreBrowserSession = {
    sessionId: "aaaaaaaa-browser", ownerSessionId: "conversation-a", status: "running",
    profileMode: "ephemeral", currentUrl: null, leaseState: "free", blockedReason: null,
    automationState: "detached", pageGeneration: "1", viewportGeneration: "1",
    viewportSize: { width: 1280, height: 800 }, controlEpoch: "1", hostGeneration: "1",
    pageCount: 2, requiresExplicitReopen: false,
  };
  // heldSession is the Core projection after the lease changes epochs.
  // heldSession 是租约改变代次后 Core 发布的投影。
  const heldSession: CoreBrowserSession = { ...freeSession, leaseState: "held", controlEpoch: "2" };
  // pages provide two targets without changing the browser generation.
  // pages 提供两个页面目标，且不改变浏览器代次。
  const pages: CoreBrowserPage[] = [
    { pageId: "page-a", targetId: "target-a", url: "https://a.example/", openerPageId: null, hostGeneration: 1 },
    { pageId: "page-b", targetId: "target-b", url: "https://b.example/", openerPageId: null, hostGeneration: 1 },
  ];
  // callbacks let the test publish one real-shaped Core frame for page A.
  // callbacks 使测试可以为页面 A 发布一帧符合真实结构的 Core 画面。
  let callbacks!: CoreBrowserStreamCallbacks;
  // held selects the projection returned by subsequent Core list polls.
  // held 决定后续 Core 列表轮询返回哪种投影。
  let held = false;
  vi.mocked(listCoreBrowserSessions).mockImplementation(() => Promise.resolve([held ? heldSession : freeSession]));
  vi.mocked(listCoreBrowserPages).mockResolvedValue(pages);
  vi.mocked(openCoreBrowserStream).mockImplementation(async (_owner, _session, pageId, nextCallbacks) => {
    if (pageId === "page-a") callbacks = nextCallbacks;
    return () => {};
  });
  vi.mocked(requestCoreBrowser).mockImplementation((_owner, method) => {
    if (method === "browser.lease.acquire") {
      held = true;
      return Promise.resolve({ session: heldSession, leaseSecret: "page-switch-secret", heartbeatIntervalMs: 5000 } as never);
    }
    return Promise.resolve({ outcome: "applied" } as never);
  });
  // previousCreate and previousRevoke restore jsdom's URL implementation after frame rendering.
  // previousCreate 与 previousRevoke 在画面渲染后恢复 jsdom 的 URL 实现。
  const previousCreate = URL.createObjectURL;
  const previousRevoke = URL.revokeObjectURL;
  URL.createObjectURL = vi.fn(() => "blob:page-a-frame");
  URL.revokeObjectURL = vi.fn();
  try {
    await act(async () => root.render(<BrowserArea ownerSessionId="conversation-a" />));
    await selectBrowser();
    await vi.waitFor(() => expect(host.textContent).toContain("a.example"));
    const acquire = [...host.querySelectorAll("button")].find((button) => button.textContent?.includes("Take control")) as HTMLButtonElement;
    await act(async () => acquire.click());
    await vi.waitFor(() => expect(host.textContent).toContain("Release control"));
    await act(async () => callbacks.onFrame({
      sequence: "1", pageId: "page-a", pageGeneration: "1", viewportGeneration: "1",
      width: 1280, height: 800, capturedAt: "2026-09-19T00:00:00Z", mimeType: "image/png",
      image: new Uint8Array([1, 2, 3]),
    }));
    const image = host.querySelector('[role="img"][aria-label="Live Core browser page"]') as HTMLDivElement;
    await act(async () => image.dispatchEvent(new KeyboardEvent("keydown", { key: "Shift", bubbles: true })));
    const pageB = [...host.querySelectorAll('[role="tablist"][aria-label="Browser pages"] [role="tab"]')]
      .find((tab) => tab.textContent?.includes("b.example")) as HTMLButtonElement;
    await act(async () => pageB.click());
    await vi.waitFor(() => expect(vi.mocked(requestCoreBrowser)).toHaveBeenCalledWith(
      "conversation-a", "browser.input.dispatch", expect.objectContaining({
        sessionId: freeSession.sessionId, pageId: "page-a", kind: "key",
        payload: { type: "up", key: "Shift" },
      }),
    ));
  } finally {
    URL.createObjectURL = previousCreate;
    URL.revokeObjectURL = previousRevoke;
  }
});
