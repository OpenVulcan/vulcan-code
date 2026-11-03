// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

type Store = typeof import("./file-window-store");

// loadStore imports a fresh copy of the store, which reads localStorage as a new page would.
// loadStore 导入一份全新的存储模块，它像新页面一样读取 localStorage。
async function loadStore(): Promise<Store> {
  vi.resetModules();
  return import("./file-window-store");
}

// windowsOf reads the windows saved for a session, the state a reload starts from.
// windowsOf 读取为会话保存的窗口，即重新加载时的起始状态。
function windowsOf(sessionId: string): unknown[] {
  const saved = JSON.parse(window.localStorage.getItem("vulcan.file-windows") ?? "null") as { sessions: Record<string, unknown[]> } | null;
  return saved?.sessions[sessionId] ?? [];
}

// jsdom here offers no localStorage, so each test gets an empty in-memory one.
// 此处的 jsdom 不提供 localStorage，因此每个测试使用一个空的内存实现。
beforeEach(() => {
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  });
});

afterEach(() => vi.unstubAllGlobals());

it("opens one window per file and reuses the lowest free slot", async () => {
  const store = await loadStore();
  store.openFileWindow("s1", "/a/report.pdf");
  store.openFileWindow("s1", "/a/chart.png");
  store.openFileWindow("s1", "/a/report.pdf");
  expect(windowsOf("s1").map((entry) => (entry as { id: string }).id)).toEqual(["file-window-1", "file-window-2"]);
  store.closeFileWindow("s1", "file-window-1");
  store.openFileWindow("s1", "/a/deck.pptx");
  expect(windowsOf("s1")).toEqual([
    { id: "file-window-2", path: "/a/chart.png", minimized: false },
    { id: "file-window-1", path: "/a/deck.pptx", minimized: false },
  ]);
});

it("keeps sessions apart and restores minimized windows after a reload", async () => {
  const store = await loadStore();
  store.openFileWindow("s1", "/a/report.pdf");
  store.openFileWindow("s2", "/b/budget.xlsx");
  store.minimizeFileWindow("s1", "file-window-1");
  const reloaded = await loadStore();
  expect(windowsOf("s1")).toEqual([{ id: "file-window-1", path: "/a/report.pdf", minimized: true }]);
  reloaded.openFileWindow("s1", "/a/report.pdf");
  expect(windowsOf("s1")).toEqual([{ id: "file-window-1", path: "/a/report.pdf", minimized: false }]);
  reloaded.closeFileWindow("s2", "file-window-1");
  expect(JSON.parse(window.localStorage.getItem("vulcan.file-windows") ?? "{}").sessions).not.toHaveProperty("s2");
});

it("drops malformed saved windows instead of guessing", async () => {
  window.localStorage.setItem("vulcan.file-windows", JSON.stringify({ version: 1, sessions: { s1: [{ id: "file-window-1", path: 3 }] } }));
  const store = await loadStore();
  store.openFileWindow("s1", "/a/report.pdf");
  expect(windowsOf("s1")).toEqual([{ id: "file-window-1", path: "/a/report.pdf", minimized: false }]);
});
