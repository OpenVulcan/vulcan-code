// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { formatBytes } from "@/lib/display-format";
import { ToolRequestCard } from "./tool-call-card";
import type { ToolCardState } from "./tool-call";

// Exercise both read and write cards at the same compact byte-format boundaries.
// 验证读取与书写卡片在相同的字节格式切换边界保持宽度。
it.each(["file_read", "file_write"])("keeps the widest live metadata until the %s request ends", (toolName) => {
  // Simulated layout widths let jsdom report the fractional dimensions of each byte label.
  // 模拟布局宽度，使 jsdom 能报告每个字节计量的实际小数宽度形式。
  let measuredWidth = 96;
  const widths = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockImplementation(() => new DOMRect(0, 0, measuredWidth, 14));
  const observed = new Map<Element, () => void>();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // The test controls when the browser would report a metadata resize.
  // 由测试控制浏览器何时报告元信息列尺寸变化。
  vi.stubGlobal("ResizeObserver", class {
    // Remember the callback to deliver when a measured element changes.
    // 保存被测元素变化时需要触发的回调。
    constructor(private readonly callback: () => void) {}
    // Register the observed element so the test can report its new width.
    // 登记被观察的元素，以便测试报告其新宽度。
    observe(element: Element) { observed.set(element, this.callback); }
    // Discard only this observer's pending measurements when the card settles or is replaced.
    // 卡片结束或被替换时，只清理本观察器的待报告测量。
    disconnect() {
      for (const [element, callback] of observed) {
        if (callback === this.callback) observed.delete(element);
      }
    }
  });
  const host = document.createElement("div");
  const root = createRoot(host);

  // Render one update and report its measured size to the metadata observer.
  // 渲染一次更新，并将测量尺寸报告给元信息列观察器。
  const render = (receivedBytes: number, width: number, requestStreamId = "first", state: ToolCardState = "running") => {
    measuredWidth = width;
    act(() => root.render(
      <I18nProvider initialLocale="en-US">
        <ToolRequestCard onOpenChange={() => undefined} open={false} phase="receiving" preview={null}
          receivedBytes={receivedBytes} requestStreamId={requestStreamId} state={state} toolName={toolName} />
      </I18nProvider>,
    ));
    const meta = host.querySelector<HTMLElement>(".vk-tool-meta")!;
    observed.get(meta)?.();
    expect(meta.querySelector(".vk-tool-request")?.textContent).toContain(formatBytes(receivedBytes));
    return meta.style.getPropertyValue("--vk-tool-meta-min-w");
  };

  try {
    expect(render(1946, 106.25)).toBe("106.25px"); // 1.9 KiB
    expect(render(2048, 98)).toBe("106.25px"); // 2 KiB
    expect(render(10_752, 113)).toBe("113px"); // 10.5 KiB
    expect(render(1_048_576, 98)).toBe("113px"); // 1 MiB
    expect(render(96, 82, "second")).toBe("82px");
    expect(render(96, 35, "second", "stopped")).toBe("");
    expect(observed.size).toBe(0);
  } finally {
    act(() => root.unmount());
    widths.mockRestore();
    vi.unstubAllGlobals();
  }
});
