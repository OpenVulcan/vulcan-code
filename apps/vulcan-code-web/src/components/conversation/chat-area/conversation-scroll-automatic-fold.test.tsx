// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { ConversationScrollSnapshot } from "../chat-area-scroll";
import { dispatchConversationAutomaticFold } from "../conversation-layout-events";
import { useConversationScrollController } from "./conversation-scroll-controller";

// A followed tail must keep its page height when a folded tool batch is replaced by a shorter one:
// the automatic fold fills the shrinkage with the tail reserve and the next batch consumes it.
// 锁底尾部在折叠的工具批次被更短批次替换时必须保持页面高度：自动折叠用尾部预留填补收缩，下一批次消耗预留。
it.each([
  "announced fold",
  "unannounced running tool replacement",
])("keeps the followed tail visible and steady through an %s", async (change) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Hold animation callbacks so a correction queued for the next frame cannot pass a before-paint assertion.
  // 暂停动画回调，避免下一帧的校正通过当前绘制前的断言。
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", () => {});
  // resizes delivers the content observer notification the browser sends after each geometry change.
  // resizes 派发浏览器在每次几何变化后发出的内容观察器通知。
  const resizes = new Set<() => void>();
  vi.stubGlobal("ResizeObserver", class {
    // Keeps the observer callback; accepts it and returns the mock observer.
    // 保存观察器回调；接收回调并返回模拟观察器。
    constructor(private callback: () => void) {}
    // Registers the callback; no return value.
    // 注册回调；无返回值。
    observe() { resizes.add(this.callback); }
    // Removes the callback; no return value.
    // 移除回调；无返回值。
    disconnect() { resizes.delete(this.callback); }
  });
  // contentHeight is the synthetic timeline height; the viewport is 500px tall and starts at the bottom.
  // contentHeight 是合成时间线高度；视口高 500px 且从底部开始。
  let contentHeight = 2000;
  const saved = new Map<string, ConversationScrollSnapshot>();
  // Harness mounts the real controller over a transcript with a timeline and a tail reserve.
  // Harness 在带有时间线与尾部预留的转录上挂载真实控制器。
  function Harness() {
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    const controller = useConversationScrollController({
      active: true, surfaceId: "auto-fold", scrollRef,
      holdRunningTailHeight: change === "unannounced running tool replacement",
      readSnapshot: (id) => saved.get(id) ?? null,
      writeSnapshot: (id, snapshot) => { saved.set(id, snapshot); },
      onFollowTailChange: () => {},
    });
    return <div data-conversation-surface-id="auto-fold" onScroll={controller.handleScroll} ref={scrollRef}>
      <div className="vk-transcript-inner"><div className="vk-timeline"><section data-tool-run="" /></div><div className="vk-transcript-tail" /></div>
    </div>;
  }
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<Harness />));
    const element = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    const timeline = element.querySelector<HTMLElement>(".vk-timeline")!;
    const reserve = element.querySelector<HTMLElement>(".vk-transcript-tail")!;
    // Geometry follows the synthetic heights: the reserve's inline height is the controller's output.
    // 几何跟随合成高度：预留的内联高度即控制器的输出。
    const reserveHeight = () => Number.parseFloat(reserve.style.height || "0");
    Object.defineProperty(timeline, "offsetHeight", { configurable: true, get: () => contentHeight });
    Object.defineProperty(reserve, "offsetHeight", { configurable: true, get: reserveHeight });
    Object.defineProperty(element, "clientHeight", { configurable: true, value: 500 });
    Object.defineProperty(element, "scrollHeight", { configurable: true, get: () => contentHeight + reserveHeight() });
    element.scrollTop = element.scrollHeight - element.clientHeight;
    const pageHeight = () => contentHeight + reserveHeight();
    const notify = () => { for (const callback of resizes) callback(); };
    notify();
    const before = pageHeight();

    // A tall running replacement can exceed the manual fold budget without sending a fold event.
    // 活动工具替换的收缩量可超过手动折叠预算，且无需发出折叠事件。
    const shrink = change === "announced fold" ? 160 : 350;
    if (change === "announced fold") dispatchConversationAutomaticFold(timeline.querySelector("[data-tool-run]")!);
    contentHeight -= shrink;
    element.scrollTop = element.scrollHeight - element.clientHeight;
    notify();
    expect(pageHeight()).toBe(before);
    expect(element.scrollTop).toBe(before - element.clientHeight);
    // The fold window closes before the next batch arrives, so the batch is measured as ordinary followed growth.
    // 折叠窗口在下一批次到达前关闭，使该批次按普通锁底增长计量。
    await React.act(async () => { await new Promise((resolve) => setTimeout(resolve, 150)); });

    // The next two-tool batch appears and grows by less than the fold took: the page still keeps its height.
    // 下一批两个工具出现，增长少于折叠收缩的量：页面仍保持高度。
    contentHeight += 90;
    notify();
    expect(pageHeight()).toBe(before);

    // Content beyond the earlier height consumes the rest of the reserve, so the page grows only by the excess.
    // 超出原高度的内容消耗剩余预留，使页面只按超出部分增长。
    contentHeight += shrink - 90 + 30;
    notify();
    expect(reserveHeight()).toBe(0);
    expect(pageHeight()).toBe(before + 30);

    // A leaf assistant text update has no ChatArea commit. Resize delivery alone must follow it before paint.
    // 助手正文的叶子更新不提交 ChatArea。仅派发尺寸变化也必须在绘制前跟随。
    contentHeight += 80;
    notify();
    expect(element.scrollTop).toBe(pageHeight() - element.clientHeight);

    // Reader scroll releases the tail, even while automatic output continues.
    // 即使自动输出仍在继续，阅读者上滚也会释放锁底。
    const readingTop = element.scrollTop - 300;
    element.scrollTop = readingTop;
    element.dispatchEvent(new Event("scroll"));
    contentHeight += 60;
    notify();
    expect(element.scrollTop).toBe(readingTop);
    expect(saved.get("auto-fold")?.followTail).toBe(false);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
