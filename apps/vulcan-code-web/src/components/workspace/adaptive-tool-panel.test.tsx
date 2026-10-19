// @vitest-environment jsdom
import React, { act, Profiler, type ProfilerOnRenderCallback } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { AdaptiveToolPanel } from "@/components/workspace/adaptive-tool-panel";

// resizeCallbacks stores active synthetic observer deliveries for deterministic layout tests.
// resizeCallbacks 保存活动的合成观察器回调，用于确定性的布局测试。
const resizeCallbacks = new Set<ResizeObserverCallback>();

// TestResizeObserver exposes explicit delivery while preserving the browser observer lifecycle.
// TestResizeObserver 暴露显式投递能力，同时保留浏览器观察器生命周期。
class TestResizeObserver implements ResizeObserver {
  // callback owns the observer-specific delivery target.
  // callback 持有当前观察器专属的投递目标。
  private readonly callback: ResizeObserverCallback;

  // constructor registers one callback until disconnect releases it.
  // constructor 注册一个回调，直到 disconnect 将其释放。
  // Parameters: callback receives synthetic resize entries. Returns a test observer instance.
  // 参数：callback 接收合成尺寸事件。返回测试观察器实例。
  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    resizeCallbacks.add(callback);
  }

  // disconnect releases this observer from later synthetic deliveries.
  // disconnect 使当前观察器不再接收后续合成投递。
  // Parameters: none. Returns nothing.
  // 参数：无。无返回值。
  disconnect(): void {
    resizeCallbacks.delete(this.callback);
  }

  // observe accepts the target because geometry is supplied by getBoundingClientRect.
  // observe 接受目标节点，几何数据由 getBoundingClientRect 提供。
  // Parameters: target is the observed element and options is the native box selection. Returns nothing.
  // 参数：target 是被观察元素，options 是原生盒模型选项。无返回值。
  observe(_target: Element, _options?: ResizeObserverOptions): void {}

  // unobserve is a no-op because each test observer owns one measured target.
  // unobserve 无需操作，因为每个测试观察器只持有一个测量目标。
  // Parameters: target is the element released by React cleanup. Returns nothing.
  // 参数：target 是 React 清理时释放的元素。无返回值。
  unobserve(_target: Element): void {}
}

// deliverResizeObservers emits one delivery cycle to every active observer.
// deliverResizeObservers 向每个活动观察器发送一次投递周期。
// Parameters: none. Returns nothing after every callback has run.
// 参数：无。所有回调执行完成后无返回值。
function deliverResizeObservers(): void {
  for (const callback of [...resizeCallbacks]) {
    callback([], {} as ResizeObserver);
  }
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", TestResizeObserver);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    bottom: 40,
    height: 40,
    left: 0,
    right: 320,
    toJSON: () => ({}),
    top: 0,
    width: 320,
    x: 0,
    y: 0,
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  resizeCallbacks.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("does not recommit when ResizeObserver repeats an unchanged panel size", async () => {
  let commitCount = 0;
  // countCommit records every committed adaptive-panel render without changing its layout.
  // countCommit 记录自适应面板的每次提交，但不改变其布局。
  // Parameters: React supplies profiler timing details; only the commit occurrence is needed. Returns nothing.
  // 参数：React 提供性能计时详情；测试只需要提交发生这一事实。无返回值。
  const countCommit: ProfilerOnRenderCallback = () => {
    commitCount += 1;
  };

  await act(async () => root.render(
    <Profiler id="adaptive-tool-panel" onRender={countCommit}>
      <AdaptiveToolPanel expanded panel={<div>工具</div>} side="right">
        <main>会话</main>
      </AdaptiveToolPanel>
    </Profiler>,
  ));

  const settledCommitCount = commitCount;
  await act(async () => deliverResizeObservers());
  await act(async () => deliverResizeObservers());

  expect(settledCommitCount).toBeGreaterThan(1);
  expect(commitCount).toBe(settledCommitCount);
  expect(host.querySelector<HTMLElement>(".vk-tpanel-slot")?.style.width).toBe("320px");
  expect(host.querySelector<HTMLElement>(".vk-tpanel-slot")?.style.height).toBe("40px");
});
