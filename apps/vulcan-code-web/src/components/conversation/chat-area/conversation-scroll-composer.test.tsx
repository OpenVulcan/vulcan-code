// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { ConversationScrollSnapshot } from "../chat-area-scroll";
import { useConversationScrollController, type ConversationScrollController } from "./conversation-scroll-controller";

// Exercise submission and leaf tool replacement before the browser delivers its queued scroll event.
// 覆盖提交和叶子工具替换发生在浏览器派发排队滚动事件之前的顺序。
it.each([
  { source: "composer", nextBottom: 776 },
  { source: "composer", nextBottom: 1100 },
  { source: "tool replacement", nextBottom: 823 },
  { source: "tool replacement", nextBottom: 1100 },
])("preserves tail ownership across $source ending at $nextBottom", async ({ source, nextBottom }) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Hold correction frames so native scroll events can arrive before the deferred restore.
  // 暂存校正帧，让原生滚动事件先于延迟恢复到达。
  const frames = new Map<number, FrameRequestCallback>();
  // Allocate distinct handles so cancellation mirrors the browser frame queue.
  // 分配不同句柄，使取消行为对应浏览器帧队列。
  let frameId = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++frameId, callback);
    return frameId;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
  // Keep the saved surface and controller observable through their production interfaces.
  // 通过生产接口观察已保存界面和控制器。
  const saved = new Map<string, ConversationScrollSnapshot>();
  // Capture the mounted controller without adding a second scroll owner.
  // 捕获已挂载控制器，不增加第二个滚动所有者。
  const control: { current: ConversationScrollController | null } = { current: null };
  // Record every ownership transition, including a transient loss repaired by a later resize.
  // 记录每次所有权变化，包括被后续尺寸变化修复的短暂失锁。
  const onFollowTailChange = vi.fn();
  // Mount the real controller with deterministic native geometry; returns one scrollport.
  // 使用确定的原生几何挂载真实控制器；返回一个滚动视口。
  function Harness() {
    // Share the exact mounted surface with the controller.
    // 与控制器共享精确挂载的界面。
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    // Delegate scroll ownership and persistence to the production hook.
    // 将滚动所有权和持久化委派给生产钩子。
    const controller = useConversationScrollController({
      active: true, surfaceId: "conversation", scrollRef,
      readSnapshot: (id) => saved.get(id) ?? null,
      writeSnapshot: (id, snapshot) => { saved.set(id, snapshot); },
      onFollowTailChange,
    });
    React.useLayoutEffect(() => { control.current = controller; }, [controller]);
    return <div data-conversation-surface-id="conversation" onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, {
        scrollHeight: { configurable: true, value: 1323 },
        clientHeight: { configurable: true, value: 500 },
      });
    }}><div className="vk-transcript-inner" /></div>;
  }
  // Isolate this controller's lifecycle and pending frame queue.
  // 隔离此控制器的生命周期与待执行帧队列。
  const host = document.createElement("div");
  document.body.append(host);
  // Own and dispose the React surface for this scenario.
  // 为此场景拥有并释放 React 界面。
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<Harness />));
    // Require the same materialized surface and API used by ChatArea.
    // 要求具备 ChatArea 所使用的相同实体界面与接口。
    const element = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    // Use the initialized controller rather than a second scrolling implementation.
    // 使用已初始化控制器，而非第二套滚动实现。
    const controller = control.current!;
    expect(element.scrollTop).toBe(823);
    // Growing and shrinking the draft preserves the tail through each composer-height commit.
    // 草稿增高和缩短时，每次输入框高度提交都应保持锁底。
    for (const bottom of [950, 823]) {
      controller.captureAndScheduleLayoutRestore();
      Object.defineProperty(element, "scrollHeight", { configurable: true, value: bottom + 500 });
      element.scrollTop = Math.min(element.scrollTop, bottom);
      controller.scheduleStoredRestore();
      element.dispatchEvent(new Event("scroll"));
      for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
      expect(element.scrollTop).toBe(bottom);
      expect(saved.get("conversation")?.followTail).toBe(true);
    }
    controller.captureAndScheduleLayoutRestore();
    // Model the browser clamp caused by clearing a multiline composer, before its scroll event is delivered.
    // 模拟清空多行输入框导致的浏览器位置截断，此时滚动事件尚未派发。
    Object.defineProperty(element, "scrollHeight", { configurable: true, value: 1135 });
    element.scrollTop = 635;
    // Insert the submitted prompt before the native event, so the old clamp no longer equals the current maximum.
    // 在原生事件之前插入已发送提示词，使旧截断位置不再等于当前最大位置。
    Object.defineProperty(element, "scrollHeight", { configurable: true, value: nextBottom + 500 });
    if (source === "composer") {
      controller.scheduleStoredRestore();
    } else {
      // Leaf tool subscriptions can replace DOM without committing ChatArea or changing final content height.
      // 叶子工具订阅可以替换 DOM，而不提交 ChatArea，也不改变最终内容高度。
      // Deliver the real mutation microtask before the native scroll event; no resize callback is emitted.
      // 在原生滚动事件前派发真实修改微任务；此处不产生尺寸回调。
      element.querySelector(".vk-transcript-inner")!.replaceChildren(document.createElement("div"));
      await Promise.resolve();
    }
    element.dispatchEvent(new Event("scroll"));
    expect(saved.get("conversation")?.followTail).toBe(true);
    expect(onFollowTailChange).not.toHaveBeenCalledWith(false);
    for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
    expect(element.scrollTop).toBe(nextBottom);

    // Reader movement must still cancel a scheduled correction instead of being pulled back to the bottom.
    // 阅读者移动仍必须取消待执行校正，不能被拉回底部。
    controller.scheduleStoredRestore();
    element.scrollTop = nextBottom - 200;
    element.dispatchEvent(new Event("scroll"));
    expect(saved.get("conversation")?.followTail).toBe(false);
    expect(frames.size).toBe(0);
    // Typing and submitting while reading history must preserve that explicit reader ownership.
    // 阅读历史时输入及发送必须保留明确的阅读位置所有权。
    controller.captureAndScheduleLayoutRestore();
    Object.defineProperty(element, "scrollHeight", { configurable: true, value: nextBottom + 800 });
    controller.scheduleStoredRestore();
    // Tool replacement while reading history must not let the mutation observer reacquire the viewport.
    // 阅读历史时替换工具，不得让修改观察器重新取得视口所有权。
    element.querySelector(".vk-transcript-inner")!.replaceChildren(document.createElement("div"));
    await Promise.resolve();
    for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
    expect(element.scrollTop).toBe(nextBottom - 200);
    expect(saved.get("conversation")?.followTail).toBe(false);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
