// @vitest-environment jsdom
import * as React from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { ConversationScrollSnapshot } from "../chat-area-scroll";
import {
  CONVERSATION_SCROLLBAR_POSITION_EVENT,
  CONVERSATION_TURN_WINDOW_LAYOUT_EVENT,
  type ConversationScrollbarPosition,
  type ConversationTurnWindowLayoutDetail,
} from "../conversation-layout-events";
import { useConversationScrollController } from "./conversation-scroll-controller";

// Report composer geometry in a child layout effect, before the parent restores the selected surface.
// 在子组件布局副作用中报告输入框几何，早于父组件恢复选中界面的位置。
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
      readSnapshot: () => null, writeSnapshot: () => {},
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

// Exercise the production scroll owner across a loading gap and a return to a previously read window.
// 覆盖生产滚动所有者跨加载间隙后返回已阅读窗口的行为。
it("retains the reader position across loading and cached session remounts", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Saved reader positions belong to separate surfaces and must never be replaced by loading geometry.
  // 保存的阅读位置分别属于独立界面，绝不能被加载占位几何覆盖。
  const saved = new Map<string, ConversationScrollSnapshot>([
    ["a", { anchorKey: null, anchorTop: 0, scrollTop: 320, followTail: false }],
    ["b", { anchorKey: null, anchorTop: 0, scrollTop: 740, followTail: false }],
  ]);
  const readSnapshot = (id: string) => saved.get(id) ?? null;
  const writeSnapshot = (id: string, snapshot: ConversationScrollSnapshot) => { saved.set(id, snapshot); };
  const onFollowTailChange = vi.fn();
  // Harness mounts the scrollport only after hydration; supplied geometry represents retained history pages.
  // Harness 仅在加载完成后挂载滚动视口；指定几何代表保留下来的历史分页。
  // Parameters select the surface and loading state; returns the active scrollport or a loading placeholder.
  // 参数选择界面和加载状态；返回活动滚动视口或加载占位。
  function Harness(props: { id: string; ready: boolean }) {
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    const controller = useConversationScrollController({ active: props.ready, surfaceId: props.id, scrollRef, readSnapshot, writeSnapshot, onFollowTailChange });
    return props.ready ? <div data-conversation-surface-id={props.id} onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (element) Object.defineProperties(element, { scrollHeight: { configurable: true, value: 3000 }, clientHeight: { configurable: true, value: 500 } });
    }}><div className="vk-transcript-inner" /></div> : <div>载入</div>;
  }
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  try {
    await React.act(async () => root.render(<Harness id="a" ready />));
    expect(host.querySelector<HTMLElement>("[data-conversation-surface-id]")?.scrollTop).toBe(320);
    await React.act(async () => root.render(<Harness id="b" ready={false} />));
    expect(saved.get("a")).toMatchObject({ scrollTop: 320, followTail: false });
    expect(saved.get("b")).toMatchObject({ scrollTop: 740, followTail: false });
    await React.act(async () => root.render(<Harness id="b" ready />));
    expect(host.querySelector<HTMLElement>("[data-conversation-surface-id]")?.scrollTop).toBe(740);
    await React.act(async () => root.render(<Harness id="a" ready />));
    expect(host.querySelector<HTMLElement>("[data-conversation-surface-id]")?.scrollTop).toBe(320);
    expect(saved.get("a")?.followTail).toBe(false);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});

// Reproduce returning to the middle of a tall turn when the first render has only its estimated shell.
// 复现返回长回合中段时，首帧仅包含该回合估算外壳的情况。
it("restores a deep reading offset after its placeholder becomes a real turn", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // The saved offset is deeper than the provisional height but fits the mounted body.
  // 已保存的偏移超过占位高度，但位于真实正文内。
  const anchorKey = "long-turn";
  const turnTop = 960;
  const placeholderHeight = 480;
  const savedOffset = 3_000;
  const bodyHeight = 4_000;
  let saved: ConversationScrollSnapshot = {
    anchorKey, anchorTop: -savedOffset, scrollTop: turnTop + savedOffset, followTail: false,
  };
  // Mount the production owner against deterministic scroll geometry.
  // 以确定的滚动几何挂载生产滚动所有者。
  function Harness() {
    // scrollRef owns the browser scrollport while rowHeight models the body swap.
    // scrollRef 持有浏览器滚动视口，rowHeight 模拟正文切换。
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    const controller = useConversationScrollController({
      active: true, surfaceId: "reader", scrollRef,
      onFollowTailChange: () => {},
      readSnapshot: () => saved,
      writeSnapshot: (_id, snapshot) => { saved = snapshot; },
    });
    return <div data-conversation-surface-id="reader" onScroll={controller.handleScroll} ref={(element) => {
      scrollRef.current = element;
      if (!element) return;
      Object.defineProperties(element, { scrollHeight: { value: 8_000 }, clientHeight: { value: 600 } });
      vi.spyOn(element, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 800, 600));
      // The row rect changes with scrollTop just as it does in the browser.
      // 回合矩形随 scrollTop 变化，与浏览器行为一致。
      const row = element.querySelector<HTMLElement>("[data-conversation-row-key]")!;
      vi.spyOn(row, "getBoundingClientRect").mockImplementation(() => new DOMRect(
        0, turnTop - element.scrollTop, 800,
        row.hasAttribute("data-conversation-turn-placeholder") ? placeholderHeight : bodyHeight,
      ));
    }}><div className="vk-transcript-inner">
      <section data-conversation-row-key={anchorKey} data-conversation-turn-placeholder="" />
    </div></div>;
  }
  // Isolate the DOM and notify the controller exactly as the body-window hook does.
  // 隔离 DOM，并以正文视口钩子相同的方式通知控制器。
  const host = document.createElement("div");
  const root = createRoot(host);
  document.body.append(host);
  try {
    await React.act(async () => root.render(<Harness />));
    const scrollport = host.querySelector<HTMLElement>("[data-conversation-surface-id]")!;
    const row = host.querySelector<HTMLElement>("[data-conversation-row-key]")!;
    expect(scrollport.scrollTop).toBeGreaterThanOrEqual(turnTop);
    expect(scrollport.scrollTop).toBeLessThan(turnTop + placeholderHeight);
    // The browser can deliver the provisional scroll before the body observer fires.
    // 浏览器可能在正文观察器触发之前派发占位滚动事件。
    await React.act(async () => scrollport.dispatchEvent(new Event("scroll")));
    expect(saved.anchorTop).toBe(-savedOffset);
    const provisionalTop = row.getBoundingClientRect().top;
    row.removeAttribute("data-conversation-turn-placeholder");
    scrollport.dispatchEvent(new CustomEvent<ConversationTurnWindowLayoutDetail>(
      CONVERSATION_TURN_WINDOW_LAYOUT_EVENT,
      { detail: { anchor: { rowKey: anchorKey, offsetTop: provisionalTop } } },
    ));
    expect(scrollport.scrollTop).toBe(turnTop + savedOffset);
    expect(saved.anchorTop).toBe(-savedOffset);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});

// Reproduce the real composer-before-parent effect order when returning from a tail-owned short conversation.
// 复现从跟随底部的短会话返回时，真实输入框副作用先于父组件执行的顺序。
it("does not let the incoming composer overwrite a saved reader position before restoration", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Saved positions cover both reader ownership and explicit tail ownership.
  // 已存位置同时覆盖阅读位置所有权和显式尾部所有权。
  const saved = new Map<string, ConversationScrollSnapshot>([
    ["long", { anchorKey: null, anchorTop: 0, scrollTop: 876, followTail: false }],
    ["short", { anchorKey: null, anchorTop: 0, scrollTop: 0, followTail: true }],
  ]);
  // Read one surface snapshot by id; returns null for an unseen surface.
  // 按标识读取单个界面快照；未见界面返回空值。
  const readSnapshot = (id: string) => saved.get(id) ?? null;
  // Store the exact snapshot for the given surface; returns nothing.
  // 保存指定界面的精确快照；无返回值。
  const writeSnapshot = (id: string, snapshot: ConversationScrollSnapshot) => { saved.set(id, snapshot); };
  // Observe tail ownership notifications without changing layout.
  // 观察尾部所有权通知，且不改变布局。
  const onFollowTailChange = vi.fn();
  // Mount keyed scrollports alongside the composer, matching the production child layout ordering.
  // 按键挂载滚动视口及输入框，匹配生产子组件的布局执行顺序。
  // Parameters: id selects the conversation geometry; returns its scrollport and composer probe.
  // 参数：id 选择会话几何；返回其滚动视口和输入框探针。
  function Harness({ id }: { id: string }) {
    // Share the mounted scrollport with the production controller.
    // 与生产控制器共享已挂载滚动视口。
    const scrollRef = React.useRef<HTMLDivElement | null>(null);
    // Use the actual controller so child effects can expose its initialization boundary.
    // 使用真实控制器，使子组件副作用能够暴露其初始化边界。
    const controller = useConversationScrollController({ active: true, surfaceId: id, scrollRef, readSnapshot, writeSnapshot, onFollowTailChange });
    return <>
      <div key={id} data-conversation-surface-id={id} ref={(element) => {
        scrollRef.current = element;
        if (element) Object.defineProperties(element, { scrollHeight: { configurable: true, value: id === "long" ? 3340 : 1192 }, clientHeight: { configurable: true, value: 1192 } });
      }}><div className="vk-transcript-inner" /></div>
      <ComposerLayoutProbe onLayout={controller.captureAndScheduleLayoutRestore} />
    </>;
  }
  // Host and root isolate the production hook lifecycle from other tests.
  // 宿主与根节点将生产钩子生命周期与其他测试隔离。
  const host = document.createElement("div");
  const root = createRoot(host);
  document.body.append(host);
  try {
    await React.act(async () => root.render(<Harness id="short" />));
    await React.act(async () => root.render(<Harness id="long" />));
    expect(host.querySelector<HTMLElement>("[data-conversation-surface-id]")?.scrollTop).toBe(876);
    expect(saved.get("long")).toMatchObject({ scrollTop: 876, followTail: false });
    await React.act(async () => root.render(<Harness id="short" />));
    await React.act(async () => root.render(<Harness id="long" />));
    expect(host.querySelector<HTMLElement>("[data-conversation-surface-id]")?.scrollTop).toBe(876);
  } finally {
    await React.act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
