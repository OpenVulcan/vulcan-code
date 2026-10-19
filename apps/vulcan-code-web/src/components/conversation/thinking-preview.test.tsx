// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { StreamingThinkingOverlay } from "./conversation-live-reasoning";
import { ThinkingPopupContext, ThinkingPreview } from "./thinking-preview";

// Verify delayed hover, cancellation, immediate clicks, and stale-timer cleanup through real DOM events.
// 通过真实 DOM 事件验证延迟悬停、取消、立即点击与过期定时器清理。
it("waits 500ms for hover while clicks bypass and cancel the pending delay", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // host and root own the isolated trigger; the actual Radix content renders into its portal.
  // host 与 root 持有隔离入口；真实 Radix 内容渲染到自身 Portal。
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<I18nProvider initialLocale="zh-CN"><ThinkingPreview compact>完整思考</ThinkingPreview></I18nProvider>));
    // trigger is the actual user-facing button and must exist after mounting.
    // trigger 是实际面向用户的按钮，挂载后必须存在。
    const trigger = host.querySelector<HTMLButtonElement>("button");
    if (!trigger) throw new Error("Missing thinking trigger");
    await React.act(async () => trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })));
    await React.act(async () => vi.advanceTimersByTime(499));
    expect(document.querySelector(".thinking-preview-surface")).toBeNull();
    await React.act(async () => vi.advanceTimersByTime(1));
    expect(document.querySelector(".thinking-preview-body")?.textContent).toBe("完整思考");
    await React.act(async () => trigger.dispatchEvent(new MouseEvent("mouseout", { bubbles: true })));
    await React.act(async () => vi.advanceTimersByTime(180));
    expect(document.querySelector(".thinking-preview-surface")).toBeNull();

    await React.act(async () => trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })));
    await React.act(async () => vi.advanceTimersByTime(200));
    await React.act(async () => trigger.dispatchEvent(new MouseEvent("mouseout", { bubbles: true })));
    await React.act(async () => vi.advanceTimersByTime(500));
    expect(document.querySelector(".thinking-preview-surface")).toBeNull();

    await React.act(async () => trigger.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })));
    await React.act(async () => trigger.click());
    expect(document.querySelector(".thinking-preview-body")?.textContent).toBe("完整思考");
    await React.act(async () => trigger.click());
    await React.act(async () => vi.advanceTimersByTime(600));
    expect(document.querySelector(".thinking-preview-surface")).toBeNull();
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

// Verify the established live popup alone owns streaming content, while the completed preview remains interactive.
// 验证既有实时弹窗独占流式内容，而完成后的预览仍可交互。
it("does not open a second preview while the live thinking popup owns the stream", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // The observer stub keeps this test focused on popup ownership instead of geometry.
  // 观察器桩使此测试聚焦弹窗所有权，而不是几何测量。
  vi.stubGlobal("ResizeObserver", class {
    // observe accepts the content node without producing layout callbacks.
    // observe 接收内容节点，但不产生布局回调。
    observe() {}
    // disconnect releases the inert observer after unmount.
    // disconnect 在卸载后释放无操作观察器。
    disconnect() {}
  });
  // host and root isolate the actual popup-mode presentation lifecycle.
  // host 与 root 隔离真实弹窗模式下的展示生命周期。
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <StreamingThinkingOverlay
            block={{ id: "live-reasoning", kind: "thinking", title: "思考过程", summary: "正在思考", body: "实时思考", status: "streaming" }}
            conversationId="test-conversation"
            leaving={false}
          />
          <ThinkingPopupContext.Provider value>
            <ThinkingPreview streaming>实时思考</ThinkingPreview>
          </ThinkingPopupContext.Provider>
        </TooltipProvider>
      </I18nProvider>,
    ));
    expect(document.querySelectorAll(".streaming-thinking-overlay")).toHaveLength(1);
    expect(host.querySelector(".thinking-preview-indicator")).not.toBeNull();
    expect(host.querySelector("button.thinking-preview-trigger")).toBeNull();
    expect(document.querySelector(".thinking-preview-surface")).toBeNull();
    await React.act(async () => vi.advanceTimersByTime(600));
    expect(document.querySelector(".thinking-preview-surface")).toBeNull();

    await React.act(async () => root.render(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ThinkingPopupContext.Provider value>
            <ThinkingPreview>已完成思考</ThinkingPreview>
          </ThinkingPopupContext.Provider>
        </TooltipProvider>
      </I18nProvider>,
    ));
    const trigger = host.querySelector<HTMLButtonElement>("button.thinking-preview-trigger");
    if (!trigger) throw new Error("Missing completed thinking trigger");
    await React.act(async () => trigger.click());
    expect(document.querySelector(".thinking-preview-body")?.textContent).toBe("已完成思考");
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});
