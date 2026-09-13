import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { WebuiApp } from "../../src/app/WebuiApp";
import { calls } from "./runtime";

// failure injects one declared feature render error without replacing the real workspace shell.
// failure 注入一处明确的功能渲染错误，不替换真实工作台外壳。
const failure = vi.hoisted(() => ({ agents: false }));
vi.mock("@/components/features/agents/AgentsArea", async (importOriginal) => {
  // original retains all real agent controls outside this specific failure branch.
  // original 在此特定故障分支之外保留全部真实智能体控件。
  const original = await importOriginal<typeof import("../../src/components/features/agents/AgentsArea")>();
  return { ...original, AgentCategoryMenu: (props: React.ComponentProps<typeof original.AgentCategoryMenu>) => {
    if (failure.agents) throw new Error("Synthetic embedded feature failure");
    return <original.AgentCategoryMenu {...props} />;
  } };
});

it.each([
  { area: "设置", selector: ".settings-feature-content" },
  { area: "智能体", selector: ".agent-management-surface" },
  { area: "记忆", selector: ".vmm-memory-page" },
  { area: "知识库", selector: ".knowledge-page" },
  { area: "技能", selector: ".agent-management-surface" },
  { area: "插件", selector: ".skill-management-surface" },
  { area: "侧栏", selector: ".workspace-panel-shell" },
  { area: "页面故障", selector: ".workspace-panel-shell" },
])("navigates the real $area page without service discovery", async ({ area, selector }) => {
  // storage belongs only to this jsdom test and follows the native Storage methods consumed by the workbench.
  // storage 仅属于此 jsdom 测试，并遵循工作台消费的原生 Storage 方法。
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key), clear: () => storage.clear(), key: (index: number) => [...storage.keys()][index] ?? null, get length() { return storage.size; } });
  // Observers and media queries are deterministic browser facilities, not production component mocks.
  // 观察器与媒体查询是确定性的浏览器设施，不是生产组件替身。
  // resizeListeners expose only browser measurements, leaving the real shell's policy and callbacks intact.
  // resizeListeners 仅提供浏览器测量，保留真实外壳的策略及回调。
  const resizeListeners = new Map<Element, ResizeObserverCallback>();
  vi.stubGlobal("ResizeObserver", class {
    // callback receives a synthetic browser observation; targets belong to this observer instance.
    // callback 接收合成的浏览器观察结果；targets 归属于当前观察器实例。
    private readonly targets = new Set<Element>();
    constructor(private readonly callback: ResizeObserverCallback) {}
    // observe registers an element; returns nothing.
    // observe 注册元素，不返回值。
    observe(element: Element) { this.targets.add(element); resizeListeners.set(element, this.callback); }
    // unobserve removes one owned element; returns nothing.
    // unobserve 移除一个所属元素，不返回值。
    unobserve(element: Element) { this.targets.delete(element); resizeListeners.delete(element); }
    // disconnect releases all owned observations; returns nothing.
    // disconnect 释放所有所属观察，不返回值。
    disconnect() { for (const element of this.targets) resizeListeners.delete(element); this.targets.clear(); }
  });
  vi.stubGlobal("IntersectionObserver", class { observe() {} unobserve() {} disconnect() {} });
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  // fetchSpy and socketSpy fail every attempted real service transport.
  // fetchSpy 与 socketSpy 使每次尝试的真实服务传输失败。
  const fetchSpy = vi.fn(async () => { throw new Error("UI_AUDIT_NETWORK_FORBIDDEN"); });
  const socketSpy = vi.fn(() => { throw new Error("UI_AUDIT_SOCKET_FORBIDDEN"); });
  vi.stubGlobal("fetch", fetchSpy); vi.stubGlobal("WebSocket", socketSpy);
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  // container mounts actual production components, including the real settings navigation and dialogs.
  // container 挂载实际生产组件，包括真实设置导航及弹窗。
  const container = document.createElement("div"); document.body.appendChild(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<WebuiApp />));
    await vi.waitFor(async () => {
      await act(async () => undefined);
      expect(container.querySelectorAll("button").length).toBeGreaterThan(5);
    }, { timeout: 10000 });
    expect(calls.some((call) => call.method === "workbench.snapshot" && call.status === "read")).toBe(true);
    expect(fetchSpy).not.toHaveBeenCalled(); expect(socketSpy).not.toHaveBeenCalled();
    expect(container.querySelectorAll("button").length).toBeGreaterThan(5);
    if (area === "页面故障") {
      vi.spyOn(console, "error").mockImplementation(() => {});
      failure.agents = true;
      // navigate uses actual sidebar actions; a failed category must not unmount these controls.
      // navigate 使用实际侧栏动作；分类加载失败不得卸载这些控件。
      const navigate = async (label: string) => {
        const button = [...container.querySelectorAll("button")].find((item) => item.textContent === label);
        expect(button).toBeTruthy();
        await act(async () => button!.click());
      };
      await navigate("智能体");
      await vi.waitFor(async () => {
        await act(async () => undefined);
        expect(container.querySelector('[role="alert"]')?.textContent).toContain("Synthetic embedded feature failure");
      });
      await navigate("知识库");
      await vi.waitFor(async () => {
        await act(async () => undefined);
        expect(container.querySelector(".knowledge-page")).not.toBeNull();
      });
      expect(container.querySelector('[role="alert"]')).toBeNull();
      expect(fetchSpy).not.toHaveBeenCalled(); expect(socketSpy).not.toHaveBeenCalled();
      return;
    }
    if (area === "侧栏") {
      // resize sends geometry to the currently registered production observer, including after effect recreation.
      // resize 向当前注册的生产观察器传入尺寸，包含副作用重新注册后的观察器。
      const resize = async (width: number) => {
        const element = container.querySelector(".workspace-panel-shell")!;
        expect(resizeListeners.has(element)).toBe(true);
        const observer = new ResizeObserver(() => undefined);
        await act(async () => resizeListeners.get(element)!([{ target: element, contentRect: new DOMRect(0, 0, width, 700), borderBoxSize: [], contentBoxSize: [], devicePixelContentBoxSize: [] }], observer));
      };
      // trigger is the actual console-header control, including its manual-toggle callback.
      // trigger 是真实控制台标题栏按钮，包含其手动切换回调。
      const trigger = container.querySelector<HTMLButtonElement>('button[aria-label="切换侧栏"]')!;
      await resize(760);
      expect(trigger.getAttribute("aria-expanded")).toBe("false");
      await act(async () => trigger.click());
      await resize(760);
      expect(trigger.getAttribute("aria-expanded")).toBe("true");
      await resize(730);
      expect(trigger.getAttribute("aria-expanded")).toBe("false");
      await resize(1200);
      expect(trigger.getAttribute("aria-expanded")).toBe("true");
      await act(async () => trigger.click());
      await resize(1300);
      expect(trigger.getAttribute("aria-expanded")).toBe("false");
      expect(fetchSpy).not.toHaveBeenCalled(); expect(socketSpy).not.toHaveBeenCalled();
      return;
    }
    // navigation follows the same visible workbench controls used in the browser audit.
    // navigation 遵循浏览器验收使用的同一可见工作台控件。
    const navigation = area === "设置" ? container.querySelector<HTMLButtonElement>('button[aria-label="打开设置"]') : [...container.querySelectorAll("button")].find((button) => button.textContent === area);
    expect(navigation).toBeTruthy();
    await act(async () => navigation!.click());
    await vi.waitFor(async () => {
      await act(async () => undefined);
      expect(container.querySelector(selector)?.textContent?.length).toBeGreaterThan(5);
    }, { timeout: 10000 });
    if (area === "设置") {
      // leaves are the actual production settings registry projected by its sidebar.
      // leaves 是生产设置注册表经自身侧栏投影的真实叶节点。
      const leaves = [...container.querySelectorAll<HTMLButtonElement>(".settings-subarea-button")];
      expect(leaves).toHaveLength(25);
      for (const leaf of leaves) {
        await act(async () => leaf.click());
        await vi.waitFor(async () => {
          await act(async () => undefined);
          expect(container.querySelector(".settings-feature-content")?.textContent?.length, leaf.textContent ?? "").toBeGreaterThan(5);
        });
      }
    }
    if (area === "智能体") {
      // Open the real editor through its public button, without executing a save or filesystem action.
      // 通过公开按钮打开真实编辑器，不执行保存或文件系统操作。
      await act(async () => container.querySelector<HTMLButtonElement>('button[aria-label="新增智能体"]')!.click());
      expect(document.querySelector('[role="dialog"] .agent-editor-form')).not.toBeNull();
    }
    expect(fetchSpy).not.toHaveBeenCalled(); expect(socketSpy).not.toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals();
    failure.agents = false; vi.restoreAllMocks();
    delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
  }
}, 15000);
