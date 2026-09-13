// @vitest-environment jsdom
import React, { act, Suspense } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { RenderErrorBoundary } from "./render-error-boundary";
import { BootErrorScreen } from "./BootStartupScreen";

// host and root isolate React import failures from the real application window.
// host 与 root 将 React 导入失败与真实应用窗口隔离。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.spyOn(console, "error").mockImplementation(() => {});
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it("contains a rejected feature chunk and leaves sibling navigation available", async () => {
  // LazyFailure represents an actual rejected React.lazy import, not a manually selected error state.
  // LazyFailure 表示实际被拒绝的 React.lazy 导入，而非手动选择的错误状态。
  const LazyFailure = React.lazy(async () => { throw new Error("合成模块无法加载"); });
  const navigate = vi.fn();
  await act(async () => root.render(<><button onClick={navigate}>导航</button><RenderErrorBoundary resetKey="failed" fallback={(error) => <p role="alert">{error.message}</p>}><Suspense fallback="加载"><LazyFailure /></Suspense></RenderErrorBoundary></>));
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("合成模块无法加载");
  await act(async () => host.querySelector("button")!.click());
  expect(navigate).toHaveBeenCalledTimes(1);
  await act(async () => root.render(<><button onClick={navigate}>导航</button><RenderErrorBoundary resetKey="healthy" fallback={(error) => <p role="alert">{error.message}</p>}><p>正常页面</p></RenderErrorBoundary></>));
  expect(host.querySelector('[role="alert"]')).toBeNull();
  expect(host.textContent).toContain("正常页面");
});

it("preserves healthy component state when navigation identity changes", async () => {
  // Draft owns local input state to detect accidental remounts of an otherwise healthy feature.
  // Draft 持有本地输入状态，用于检测正常功能是否被意外重新挂载。
  // Returns a real stateful control without accepting parameters.
  // 返回真实有状态控件，不接收参数。
  function Draft() { const [count, setCount] = React.useState(0); return <button onClick={() => setCount(count + 1)}>草稿 {count}</button>; }
  await act(async () => root.render(<RenderErrorBoundary resetKey="one" fallback={() => null}><Draft /></RenderErrorBoundary>));
  await act(async () => host.querySelector("button")!.click());
  await act(async () => root.render(<RenderErrorBoundary resetKey="two" fallback={() => null}><Draft /></RenderErrorBoundary>));
  expect(host.textContent).toBe("草稿 1");
});

it("exposes explicit localized boot recovery without reloading automatically", async () => {
  const reload = vi.fn();
  await act(async () => root.render(<BootErrorScreen error="合成启动失败" uiLocale="zh-CN" onReload={reload} />));
  expect(reload).not.toHaveBeenCalled();
  expect(host.querySelector("button")?.textContent).toBe("重新加载应用");
  await act(async () => host.querySelector("button")!.click());
  expect(reload).toHaveBeenCalledTimes(1);
});
