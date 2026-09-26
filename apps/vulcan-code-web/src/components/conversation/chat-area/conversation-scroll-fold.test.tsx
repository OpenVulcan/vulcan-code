// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { ConversationScrollSnapshot } from "../chat-area-scroll";
import { dispatchConversationLayoutSettled, dispatchConversationLayoutWillChange, dispatchConversationManualViewportInteraction } from "../conversation-layout-events";
import { useConversationScrollController } from "./conversation-scroll-controller";

// Exercise real fold events and native resize/clamp ordering without replacing the scroll owner.
// 使用真实折叠事件和原生尺寸变化、截断顺序进行验证，不替换滚动所有者。
it.each(["manual", "automatic", "announced layout"])("preserves bottom ownership across %s fold animations", async (mode) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Frames remain queued until the test delivers the browser's native scroll event.
  // 帧保持排队，直到测试派发浏览器原生滚动事件。
  const frames = new Map<number, FrameRequestCallback>();
  let frameId = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { frames.set(++frameId, callback); return frameId; });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
  // Observer callbacks reproduce each intermediate animation measurement.
  // 观察器回调重现动画的每次中间测量。
  const resizes = new Set<() => void>();
  vi.stubGlobal("ResizeObserver", class {
    // Register one observer callback; accepts a notification and returns the mock observer.
    // 注册一个观察器回调；接收通知函数并返回模拟观察器。
    constructor(private callback: () => void) {}
    // Attach this callback to the test geometry; no return value.
    // 将此回调挂接到测试几何；无返回值。
    observe() { resizes.add(this.callback); }
    // Remove this observer's callback; no return value.
    // 移除此观察器的回调；无返回值。
    disconnect() { resizes.delete(this.callback); }
  });
  // Persist the same snapshots that ChatArea consumes and record even transient ownership loss.
  // 持久化 ChatArea 消费的同一份快照，并记录短暂的所有权丢失。
  const saved = new Map<string, ConversationScrollSnapshot>();
  const ownership = vi.fn();
  // Mount the actual hook around a focusable tool toggle; returns its owned scrollport.
  // 在可聚焦工具按钮外挂载真实钩子；返回其拥有的滚动视口。
  function Harness() {
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    const controller = useConversationScrollController({
      active: true, surfaceId: "fold-surface", scrollRef,
      readSnapshot: (id) => saved.get(id) ?? null,
      writeSnapshot: (id, snapshot) => { saved.set(id, snapshot); },
      onFollowTailChange: ownership,
    });
    return <div data-conversation-surface-id="fold-surface" onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, {
        scrollHeight: { configurable: true, value: 1500 },
        clientHeight: { configurable: true, value: 500 },
      });
    }}><div className="conversation-scroll-content"><button type="button">Tool</button></div></div>;
  }
  // Host and root own this test's mounted lifecycle.
  // 宿主和根节点拥有本测试的挂载生命周期。
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<Harness />));
    const element = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    host.querySelector("button")!.focus();
    expect(element.scrollTop).toBe(1000);
    // Each sequence expands and collapses the same tool, including interrupted animation targets.
    // 每组序列展开并收起同一个工具，包含动画目标被中途改变的情况。
    for (const bottoms of [[1100, 1350, 1600], [1400, 900, 600], [450, 1250]]) {
      if (mode === "manual") dispatchConversationManualViewportInteraction();
      else if (mode === "announced layout") dispatchConversationLayoutWillChange();
      expect(saved.get("fold-surface")?.followTail).toBe(true);
      for (const bottom of bottoms) {
        Object.defineProperty(element, "scrollHeight", { configurable: true, value: bottom + 500 });
        element.scrollTop = Math.min(element.scrollTop, bottom);
        // Native clamping may notify before ResizeObserver, while expansion notifies after it.
        // 原生截断通知可能先于 ResizeObserver，而展开通知可能后于它。
        element.dispatchEvent(new Event("scroll"));
        for (const notify of resizes) notify();
        for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
        expect(element.scrollTop).toBe(bottom);
        expect(saved.get("fold-surface")?.followTail).toBe(true);
      }
      dispatchConversationLayoutSettled("fold-surface");
      for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
    }
    expect(ownership).not.toHaveBeenCalledWith(false);
    // Real reader scrolling during a fold must invalidate the pending animation restore.
    // 折叠过程中的真实阅读滚动必须使待执行动画恢复失效。
    dispatchConversationManualViewportInteraction();
    element.scrollTop -= 250;
    element.dispatchEvent(new Event("scroll"));
    expect(saved.get("fold-surface")?.followTail).toBe(false);
    expect(frames.size).toBe(0);
    // Subsequent expansion preserves the reader's position instead of forcing bottom ownership.
    // 后续展开保留阅读位置，而不是强制接管锁底。
    const readerTop = element.scrollTop;
    dispatchConversationManualViewportInteraction();
    Object.defineProperty(element, "scrollHeight", { configurable: true, value: 2200 });
    for (const notify of resizes) notify();
    dispatchConversationLayoutSettled("fold-surface");
    for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
    expect(element.scrollTop).toBe(readerTop);
    expect(saved.get("fold-surface")?.followTail).toBe(false);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
