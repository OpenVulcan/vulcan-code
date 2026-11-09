// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { TimeSeriesLineChart } from "./plugin-host-components";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));

// Resize the same mounted chart and verify its geometry and label density follow the panel.
// 调整同一已挂载图表的容器宽度，验证绘图范围及标签密度随面板变化。
it("fits the SVG plotting area and hourly labels to the panel width", async () => {
  // updateWidth delivers a width measurement to the chart's observer.
  // updateWidth 向图表观察器传递宽度测量结果。
  let updateWidth: (width: number) => void;
  // observe and disconnect stand in for the browser observer's lifecycle methods.
  // observe 与 disconnect 替代浏览器观察器的生命周期方法。
  const observe = vi.fn();
  const disconnect = vi.fn();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class {
    // Capture the observer callback and report one measured content-box width at a time.
    // 捕获观察器回调，每次报告一个实测内容区宽度。
    constructor(callback: (entries: { contentRect: { width: number } }[]) => void) {
      updateWidth = (width) => callback([{ contentRect: { width } }]);
    }
    observe = observe;
    disconnect = disconnect;
  });
  // host and root own the real chart for both width measurements.
  // host 和 root 持有进行两次宽度测量的真实图表。
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () => root.render(<TimeSeriesLineChart ariaLabel="fixture" series={[{
      id: "model", label: "Model", color: "currentColor",
      points: Array.from({ length: 5 }, (_, index) => ({ key: String(index), label: `0${index}:00`, value: 0 })),
    }]} />));
    await act(async () => updateWidth(1280));
    expect(host.querySelector("svg")?.getAttribute("viewBox")?.split(" ")[2]).toBe("1280");
    expect(host.querySelector("svg")?.textContent).toContain("02:00");
    await act(async () => updateWidth(180));
    expect(host.querySelector("svg")?.getAttribute("viewBox")?.split(" ")[2]).toBe("180");
    expect(host.querySelector("svg")?.textContent).toContain("00:00");
    expect(host.querySelector("svg")?.textContent).toContain("04:00");
    expect(host.querySelector("svg")?.textContent).not.toContain("02:00");
    expect(observe).toHaveBeenCalledOnce();
  } finally {
    await act(async () => root.unmount());
    vi.unstubAllGlobals();
  }
  expect(disconnect).toHaveBeenCalledOnce();
});
