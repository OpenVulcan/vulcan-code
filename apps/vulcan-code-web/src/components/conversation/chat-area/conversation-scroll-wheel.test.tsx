// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { useConversationScrollController } from "./conversation-scroll-controller";

// Verify that a wheel gesture owns the viewport before either content observer sees the native scroll.
// 验证在任一内容观察器看到原生滚动之前，滚轮操作已经接管视口。
it.each(["streaming text", "tool card"] as const)("preserves a small upward wheel scroll during %s updates", async (update) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Deliver resize notifications at an explicitly chosen point in the browser event sequence.
  // 在浏览器事件序列的指定时点派发尺寸变化通知。
  const resizes = new Set<() => void>();
  vi.stubGlobal("ResizeObserver", class {
    // Retain the callback until observe registers it.
    // 保存回调，直到 observe 将其注册。
    // Parameters: callback handles a resized content subtree.
    // 参数：callback 处理内容子树的尺寸变化。
    constructor(private callback: () => void) {}
    // Register the callback without delivering a notification; returns nothing.
    // 注册回调但不派发通知，无返回值。
    observe() { resizes.add(this.callback); }
    // Remove the callback on teardown; returns nothing.
    // 清理时移除回调，无返回值。
    disconnect() { resizes.delete(this.callback); }
  });
  // Mirror the transcript's measured scroll range and observe changes to follow ownership.
  // 模拟会话正文测量得到的滚动范围，并观察跟随状态的切换。
  let contentHeight = 1_000;
  const onFollowTailChange = vi.fn();

  // Mount the production controller on a transcript; takes no props and returns the scrollport.
  // 将生产控制器挂载到会话正文，不接收属性并返回滚动区。
  function Harness() {
    // Bind both native wheel events and React scroll events to this viewport.
    // 将原生滚轮事件和 React 滚动事件同时绑定到此视口。
    const scrollRef = React.useRef<HTMLDivElement>(null);
    const controller = useConversationScrollController({
      active: true, surfaceId: "wheel", scrollRef, holdRunningTailHeight: true, onFollowTailChange,
    });
    return <div data-conversation-surface-id="wheel" onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, {
        scrollHeight: { get: () => contentHeight },
        clientHeight: { value: 300 },
      });
    }}>
      <div className="vk-transcript-inner">
        <div className="vk-timeline">
          <section style={update === "tool card" ? { overflowY: "auto" } : undefined}>Initial content</section>
        </div>
      </div>
    </div>;
  }

  // Isolate the mounted transcript so its event handlers and observers can be removed after the test.
  // 隔离挂载的会话正文，以便测试结束后移除其事件处理器和观察器。
  const host = document.createElement("div");
  const root = createRoot(host);
  document.body.append(host);
  try {
    await React.act(async () => root.render(<Harness />));
    // Locate the owning viewport and the descendant where the user points the mouse.
    // 定位所属视口和鼠标指向的子内容。
    const scrollport = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    const leaf = scrollport.querySelector<HTMLElement>("section")!;
    // Deliver each queued content-resize callback; takes no parameters and returns nothing.
    // 派发每个已注册的内容尺寸变化回调，不接收参数，无返回值。
    function notifyResize() {
      for (const callback of resizes) callback();
    }
    // Send a vertical wheel gesture through the content to the native viewport listener.
    // 通过子内容向原生视口监听器派发纵向滚轮操作。
    // Parameters: deltaY sets direction and distance; returns nothing.
    // 参数：deltaY 指定方向和距离，无返回值。
    function wheel(deltaY: number) {
      leaf.dispatchEvent(new WheelEvent("wheel", { bubbles: true, deltaY }));
    }
    // Compute the current scroll limit; takes no parameters and returns the bottom position.
    // 计算当前滚动上限，不接收参数，返回底部位置。
    const bottom = () => scrollport.scrollHeight - scrollport.clientHeight;
    expect(scrollport.scrollTop).toBe(bottom());

    if (update === "tool card") {
      Object.defineProperties(leaf, { scrollHeight: { value: 100 }, clientHeight: { value: 50 } });
      leaf.scrollTop = 10;
      wheel(-2);
      expect(onFollowTailChange).not.toHaveBeenCalledWith(false);
      leaf.scrollTop = 0;
    }

    wheel(-2);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);
    // A resize can arrive before the browser has applied the wheel delta, while the viewport is still at the bottom.
    // 浏览器应用滚轮位移之前就可能收到尺寸变化通知，此时视口仍处于底部。
    notifyResize();
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);

    // Move the viewport by just two pixels and let the real MutationObserver run before dispatching scroll.
    // 只向上移动视口两个像素，并让真实 MutationObserver 先于 scroll 事件运行。
    const readingTop = bottom() - 2;
    scrollport.scrollTop = readingTop;
    if (update === "streaming text") {
      (leaf.firstChild as Text).appendData(" streamed");
    } else {
      leaf.replaceChildren(document.createElement("pre"));
    }
    await Promise.resolve();
    expect(scrollport.scrollTop).toBe(readingTop);
    contentHeight += 8;
    notifyResize();
    scrollport.dispatchEvent(new Event("scroll"));
    expect(scrollport.scrollTop).toBe(readingTop);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);

    // Moving down within the old near-bottom range stays paused until the viewport reaches the bottom.
    // 在原有的近底范围内向下滚动仍保持暂停，直到视口抵达底部。
    scrollport.scrollTop = bottom() - 2;
    scrollport.dispatchEvent(new Event("scroll"));
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);
    scrollport.scrollTop = bottom();
    scrollport.dispatchEvent(new Event("scroll"));
    expect(onFollowTailChange).toHaveBeenLastCalledWith(true);

    contentHeight += 50;
    notifyResize();
    expect(scrollport.scrollTop).toBe(bottom());
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
