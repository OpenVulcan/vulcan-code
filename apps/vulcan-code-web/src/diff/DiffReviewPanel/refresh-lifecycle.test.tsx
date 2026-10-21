// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { DiffReviewPanel } from "./panel";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN", t: (key: string) => key }) }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn() } }));

// The empty panel still owns a real refresh action and must retain its lock until that request settles.
// 空面板仍持有真实刷新动作，必须保留锁直到该请求结束。
it("deduplicates refresh and releases its lock after a rejected host request", async () => {
  // host and root contain synthetic empty diff state only.
  // host 与 root 仅包含合成的空差异状态。
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // rejectRefresh drives the host failure without calling Git.
  // rejectRefresh 驱动宿主失败，不调用 Git。
  let rejectRefresh: (error: Error) => void = () => { throw new Error("Refresh not started"); };
  const refresh = vi.fn(() => new Promise<void>((_resolve, reject) => { rejectRefresh = reject; }));
  try {
    await act(async () => root.render(<TooltipProvider><DiffReviewPanel snapshot={null} onRefresh={refresh} /></TooltipProvider>));
    // button is the public empty-panel refresh control, not an internal handler.
    // button 是公开的空面板刷新控件，而不是内部处理器。
    const button = [...host.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent?.includes("diff.refresh"))!;
    expect(button).toBeTruthy();
    await act(async () => { button.click(); button.click(); });
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(async () => rejectRefresh(new Error("Synthetic Git read failure")));
    expect(button.disabled).toBe(false);
    refresh.mockResolvedValueOnce(undefined);
    await act(async () => button.click());
    expect(refresh).toHaveBeenCalledTimes(2);
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
