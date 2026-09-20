// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { useBrowserFrameRate } from "./use-browser-frame-rate";

// Verify rolling received-frame FPS, idle decay and identity reset with a deterministic clock.
// 使用确定时钟验证接收帧的滚动 FPS、静止归零和身份重置。
it("counts frames per second, expires idle samples and resets on a different page", async () => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // host and root own only this hook's disposable component tree.
  // host 和 root 仅持有本钩子的可丢弃组件树。
  const host = document.createElement("div");
  const root = createRoot(host);
  // record captures the hook callback used by accepted stream frames.
  // record 保存已接受画面流帧调用的钩子回调。
  let record: () => void;
  // Meter exposes the measured value for a supplied page identity; returns a test readout.
  // Meter 为传入的页面身份暴露测量值；返回测试读数。
  function Meter({ page }: { page: string }) {
    const { fps, recordFrame } = useBrowserFrameRate("owner", "browser", page);
    record = recordFrame;
    return <span>{fps}</span>;
  }
  try {
    await act(async () => root.render(<Meter page="a" />));
    await act(async () => {
      for (let index = 0; index < 20; index++) { record(); vi.advanceTimersByTime(50); }
    });
    expect(host.textContent).toBe("10");
    await act(async () => vi.advanceTimersByTime(2000));
    expect(host.textContent).toBe("0");
    await act(async () => { record(); vi.advanceTimersByTime(500); });
    expect(host.textContent).toBe("0.5");
    await act(async () => root.render(<Meter page="b" />));
    expect(host.textContent).toBe("0");
  } finally {
    await act(async () => root.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});
