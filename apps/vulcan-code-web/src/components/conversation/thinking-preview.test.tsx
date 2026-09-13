// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { ThinkingPreview } from "./thinking-preview";

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
