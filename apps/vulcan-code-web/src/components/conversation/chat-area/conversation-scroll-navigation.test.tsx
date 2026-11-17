// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import {
  CONVERSATION_SCROLLBAR_POSITION_EVENT,
  CONVERSATION_TURN_WINDOW_LAYOUT_EVENT,
  type ConversationScrollbarPosition,
  type ConversationTurnWindowLayoutDetail,
} from "../conversation-layout-events";
import { useConversationScrollController } from "./conversation-scroll-controller";

// Report composer geometry in a child layout effect, before the parent initializes the incoming viewport.
// 在父组件初始化新视口前，通过子组件布局副作用报告输入框几何。
// Parameters: onLayout captures geometry; returns no visible content.
// 参数：onLayout 捕获几何；返回空可见内容。
function ComposerLayoutProbe({ onLayout }: { onLayout: () => void }) {
  React.useLayoutEffect(() => { onLayout(); });
  return null;
}

// Verify that a held scrollbar position wins over turn anchoring when the scroll range grows.
// 验证滚动范围增大时，拖住的滚动条进度优先于回合锚点。
it("keeps the held track position as turns resize and reaches the new bottom", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Provide a mutable content height to model successive turn mounts.
  // 提供可变内容高度以模拟回合陆续挂载。
  let contentHeight = 3_000;
  const onFollowTailChange = vi.fn();
  // Mount the production controller with deterministic viewport geometry.
  // 以确定的视口几何挂载生产控制器。
  function Harness() {
    // Share the scrollport between layout notifications and the scroll owner.
    // 让布局通知与滚动所有者共用视口。
    const scrollRef = React.useRef<HTMLDivElement>(null);
    const controller = useConversationScrollController({
      active: true, surfaceId: "drag", scrollRef, onFollowTailChange,
    });
    return <div data-conversation-surface-id="drag" onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, {
        scrollHeight: { get: () => contentHeight },
        clientHeight: { value: 500 },
      });
    }} />;
  }
  // Isolate the controller and dispatch the same messages as the scrollbar and turn window.
  // 隔离控制器，并发送与滚动条和回合视口相同的消息。
  const host = document.createElement("div");
  const root = createRoot(host);
  document.body.append(host);
  try {
    await React.act(async () => root.render(<Harness />));
    const scrollport = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    // Send one normalized pointer position, including its ownership state.
    // 发送一次归一化指针进度及其所有权状态。
    const moveThumb = (progress: number, dragging: boolean) => scrollport.dispatchEvent(
      new CustomEvent<ConversationScrollbarPosition>(CONVERSATION_SCROLLBAR_POSITION_EVENT, {
        detail: { progress, dragging },
      }),
    );
    await React.act(async () => { moveThumb(0.5, true); });
    expect(scrollport.scrollTop).toBe(1_250);
    contentHeight = 6_500;
    scrollport.dispatchEvent(new CustomEvent<ConversationTurnWindowLayoutDetail>(
      CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, { detail: { anchor: null } },
    ));
    expect(scrollport.scrollTop).toBe(3_000);
    await React.act(async () => { moveThumb(1, true); });
    expect(scrollport.scrollTop).toBe(6_000);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);
    await React.act(async () => { moveThumb(1, false); });
    expect(scrollport.scrollTop).toBe(6_000);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(true);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});

// Verify upward wheel intent releases tail ownership before the native scroll event lands, so
// queued geometry corrections cannot pull the reader back to the bottom.
// 验证向上滚轮意图先于原生滚动事件释放尾部所有权，使排队几何校正无法把阅读者拉回底部。
it("releases the tail on upward wheel intent before the scroll event lands", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // height models transcript growth while the reader's scroll event is still queued.
  // height 模拟阅读者滚动事件仍在排队时的转录增长。
  let height = 6_000;
  const onFollowTailChange = vi.fn();
  function Harness() {
    // Share the scrollport between native listeners and the scroll owner.
    // 让原生监听器与滚动所有者共用视口。
    const scrollRef = React.useRef<HTMLDivElement>(null);
    const controller = useConversationScrollController({
      active: true, surfaceId: "wheel", scrollRef, onFollowTailChange,
    });
    return <div data-conversation-surface-id="wheel" onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, {
        scrollHeight: { configurable: true, get: () => height },
        clientHeight: { configurable: true, value: 500 },
      });
    }}>
      <div className="vk-transcript-inner"><div className="vk-timeline">Latest reply</div></div>
    </div>;
  }
  const host = document.createElement("div");
  const root = createRoot(host);
  document.body.append(host);
  try {
    await React.act(async () => root.render(<Harness />));
    const scrollport = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    expect(scrollport.scrollTop).toBe(5_500);
    // The wheel event alone must release ownership before any native scroll event is dispatched.
    // 滚轮事件本身必须在任何原生滚动事件派发前释放所有权。
    await React.act(async () => {
      scrollport.dispatchEvent(new WheelEvent("wheel", { deltaY: -120, bubbles: true, cancelable: true }));
    });
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);
    // Growth while the scroll event is still queued must not pull the viewport back to the tail.
    // 滚动事件仍在排队期间的增长不得把视口拉回尾部。
    await React.act(async () => {
      scrollport.scrollTop = 5_200;
      scrollport.dispatchEvent(new Event("scroll"));
    });
    height = 8_000;
    await React.act(async () => {
      scrollport.dispatchEvent(new CustomEvent<ConversationTurnWindowLayoutDetail>(
        CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, { detail: { anchor: null } },
      ));
    });
    expect(scrollport.scrollTop).toBe(5_200);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});

// Verify navigation to the latest content on reused scrollports and after a loading gap.
// 验证复用滚动视口和经过加载间隙时，切换会话都会定位到最新内容。
it.each([false, true])("starts each incoming session at the tail (loading gap: %s)", async (loadingGap) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // height represents the provisional transcript and its later measured result.
  // height 表示转录的占位高度和后续测量结果。
  let height = 6_000;
  // onFollowTailChange observes reader ownership and the reset on navigation.
  // onFollowTailChange 观察阅读状态以及切换时的复位。
  const onFollowTailChange = vi.fn();
  // Mount one production scroll controller with an incoming composer layout effect.
  // 挂载实际滚动控制器，并触发新界面输入框的布局副作用。
  // id selects the session and ready selects loading or the mounted scrollport.
  // id 选择会话，ready 决定返回加载占位还是已挂载的滚动视口。
  function Harness({ id, ready }: { id: string; ready: boolean }) {
    // scrollRef is the actual scrollport shared with the controller.
    // scrollRef 是与控制器共用的实际滚动视口。
    const scrollRef = React.useRef<HTMLDivElement>(null);
    // controller applies entry and layout corrections to that scrollport.
    // controller 对此视口执行进入会话及布局校正。
    const controller = useConversationScrollController({
      active: ready, surfaceId: id, scrollRef, onFollowTailChange, holdRunningTailHeight: true,
    });
    return ready ? <div data-conversation-surface-id={id} onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, {
        scrollHeight: { configurable: true, get: () => height },
        clientHeight: { configurable: true, value: 500 },
      });
    }}>
      <div className="vk-transcript-inner"><div className="vk-timeline">Latest reply</div></div>
      <ComposerLayoutProbe onLayout={controller.captureAndScheduleLayoutRestore} />
    </div> : null;
  }
  // host and root isolate the mounted transcript between test cases.
  // host 和 root 隔离不同用例中挂载的转录。
  const host = document.createElement("div");
  const root = createRoot(host);
  document.body.append(host);
  try {
    await React.act(async () => root.render(<Harness id="a" ready />));
    // scrollport holds the latest mounted transcript for each assertion.
    // scrollport 在每次断言时返回最新挂载的转录滚动视口。
    const scrollport = () => host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    expect(scrollport().scrollTop).toBe(5_500);
    await React.act(async () => {
      scrollport().scrollTop = 320;
      scrollport().dispatchEvent(new Event("scroll"));
    });
    expect(onFollowTailChange).toHaveBeenLastCalledWith(false);

    height = 1_400;
    if (loadingGap) await React.act(async () => root.render(<Harness id="b" ready={false} />));
    await React.act(async () => root.render(<Harness id="b" ready />));
    expect(scrollport().scrollTop).toBe(900);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(true);

    height = 2_000;
    if (loadingGap) await React.act(async () => root.render(<Harness id="a" ready={false} />));
    await React.act(async () => root.render(<Harness id="a" ready />));
    expect(scrollport().scrollTop).toBe(1_500);

    height = 7_000;
    await React.act(async () => {
      scrollport().dispatchEvent(new CustomEvent<ConversationTurnWindowLayoutDetail>(
        CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, { detail: { anchor: null } },
      ));
    });
    expect(scrollport().scrollTop).toBe(6_500);
    expect(onFollowTailChange).toHaveBeenLastCalledWith(true);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
