import React from "react";
import { nextStreamingThinkingTailFollow } from "@/components/conversation/streaming-thinking-scroll";

// StreamingThinkingViewportOptions controls whether a mounted thinking surface participates in live tail following.
// StreamingThinkingViewportOptions 控制已挂载思考视图是否参与实时尾部跟随。
export interface StreamingThinkingViewportOptions {
  // enabled is true only while the surface owns an active or terminal live render session.
  // enabled 仅在视图持有活动或终态实时渲染会话时为真。
  enabled: boolean;
}

// StreamingThinkingViewportBindings contains refs and event handlers shared by inline and popup thinking surfaces.
// StreamingThinkingViewportBindings 包含内联与弹窗思考视图共享的引用和事件处理器。
export interface StreamingThinkingViewportBindings {
  // bodyRef points to the scrollable viewport that owns the user position.
  // bodyRef 指向持有用户滚动位置的可滚动视口。
  bodyRef: React.MutableRefObject<HTMLDivElement | null>;
  // contentRef points to the changing document whose geometry drives tail following.
  // contentRef 指向几何变化会驱动尾部跟随的动态文档。
  contentRef: React.MutableRefObject<HTMLDivElement | null>;
  // handleBodyScroll updates whether the user remains pinned near the tail.
  // handleBodyScroll 更新用户是否仍锁定在尾部附近。
  handleBodyScroll: () => void;
  // handleBodyWheel records explicit wheel intent before the browser emits its scroll event.
  // handleBodyWheel 在浏览器发出滚动事件前记录明确的滚轮意图。
  handleBodyWheel: (event: React.WheelEvent<HTMLDivElement>) => void;
  // handleBodyPointerDown records pointer-driven scrolling as explicit user interaction.
  // handleBodyPointerDown 将指针驱动的滚动记录为明确的用户交互。
  handleBodyPointerDown: () => void;
}

// useStreamingThinkingViewport shares bounded, frame-coalesced tail following across thinking surfaces.
// useStreamingThinkingViewport 在思考视图之间共享受限且按帧合并的尾部跟随逻辑。
// Parameters: options declares whether the mounted surface currently owns a live render session.
// 参数：options 声明当前挂载视图是否持有实时渲染会话。
// Returns: refs and event handlers that bind the shared viewport behavior to a thinking surface.
// 返回值：绑定到思考视图的共享视口引用与事件处理器。
export function useStreamingThinkingViewport(
  options: StreamingThinkingViewportOptions,
): StreamingThinkingViewportBindings {
  // bodyRef stores the scrollable element for the current thinking surface.
  // bodyRef 保存当前思考视图的可滚动元素。
  const bodyRef = React.useRef<HTMLDivElement | null>(null);
  // contentRef stores the changing inner document without observing cumulative props.
  // contentRef 保存变化中的内部文档，不直接观察累计 props。
  const contentRef = React.useRef<HTMLDivElement | null>(null);
  // followTailRef records whether automatic updates may continue following the latest content.
  // followTailRef 记录自动更新是否可以继续跟随最新内容。
  const followTailRef = React.useRef(true);
  // tailFrameRef coalesces geometry-driven scroll writes into one animation frame.
  // tailFrameRef 将几何变化驱动的滚动写入合并到一个动画帧。
  const tailFrameRef = React.useRef<number | null>(null);
  // manualScrollPointerRef records an active pointer gesture that may move the viewport intentionally.
  // manualScrollPointerRef 记录可能主动移动视口的活跃指针手势。
  const manualScrollPointerRef = React.useRef(false);
  // manualScrollUntilRef keeps wheel intent active until asynchronous scroll events settle.
  // manualScrollUntilRef 保持滚轮意图，直到异步滚动事件稳定。
  const manualScrollUntilRef = React.useRef(0);

  // scheduleTailFollow writes scrollTop at most once per frame while the user remains tail-pinned.
  // scheduleTailFollow 仅在用户锁定尾部时每帧最多写入一次 scrollTop。
  const scheduleTailFollow = React.useCallback(() => {
    if (!options.enabled || tailFrameRef.current !== null) {
      return;
    }
    tailFrameRef.current = window.requestAnimationFrame(() => {
      tailFrameRef.current = null;
      // element is read at execution time because the surface may unmount before the frame runs.
      // element 在执行时读取，因为视图可能在动画帧运行前卸载。
      const element = bodyRef.current;
      if (!element || !followTailRef.current) {
        return;
      }
      element.scrollTop = element.scrollHeight;
    });
  }, [options.enabled]);

  React.useEffect(() => {
    // content is the exact inner document whose height changes during live Markdown updates.
    // content 是实时 Markdown 更新期间高度发生变化的精确内部文档。
    const content = contentRef.current;
    if (!options.enabled || !content) {
      return;
    }
    // observer reacts to real geometry changes instead of every cumulative body prop.
    // observer 根据真实几何变化反应，而不是监听每次累计正文 props。
    const observer = new ResizeObserver(scheduleTailFollow);
    observer.observe(content);
    scheduleTailFollow();
    return () => {
      observer.disconnect();
      if (tailFrameRef.current !== null) {
        window.cancelAnimationFrame(tailFrameRef.current);
        tailFrameRef.current = null;
      }
    };
  }, [options.enabled, scheduleTailFollow]);

  React.useEffect(() => {
    // clearPointerInteraction ends pointer scrolling even when release occurs outside the viewport.
    // clearPointerInteraction 即使在视口外释放，也会结束指针滚动状态。
    if (!options.enabled) {
      return;
    }
    const clearPointerInteraction = () => {
      manualScrollPointerRef.current = false;
    };
    window.addEventListener("pointerup", clearPointerInteraction);
    window.addEventListener("pointercancel", clearPointerInteraction);
    return () => {
      window.removeEventListener("pointerup", clearPointerInteraction);
      window.removeEventListener("pointercancel", clearPointerInteraction);
    };
  }, [options.enabled]);

  // handleBodyScroll updates tail-follow intent from the viewport's measured distance to its bottom.
  // handleBodyScroll 根据视口测量出的底部距离更新尾部跟随意图。
  const handleBodyScroll = React.useCallback(() => {
    if (!options.enabled) {
      return;
    }
    // element owns the local scroll range for either the inline box or the popup.
    // element 持有内联框或弹窗的局部滚动范围。
    const element = bodyRef.current;
    if (!element) {
      return;
    }
    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    const manualInteraction = manualScrollPointerRef.current
      || Date.now() <= manualScrollUntilRef.current;
    followTailRef.current = nextStreamingThinkingTailFollow(
      followTailRef.current,
      distanceFromBottom,
      manualInteraction,
    );
  }, [options.enabled]);

  // handleBodyWheel records explicit wheel intent before the browser dispatches scroll.
  // handleBodyWheel 在浏览器派发滚动事件前记录明确的滚轮意图。
  const handleBodyWheel = React.useCallback((event: React.WheelEvent<HTMLDivElement>) => {
    if (!options.enabled) {
      return;
    }
    manualScrollUntilRef.current = Date.now() + 250;
    if (event.deltaY < 0) {
      followTailRef.current = false;
    }
  }, [options.enabled]);

  // handleBodyPointerDown marks scrollbar, touch, pen, and drag scrolling as manual interaction.
  // handleBodyPointerDown 将滚动条、触摸、触控笔和拖动滚动标记为手动交互。
  const handleBodyPointerDown = React.useCallback(() => {
    if (options.enabled) {
      manualScrollPointerRef.current = true;
    }
  }, [options.enabled]);

  return {
    bodyRef,
    contentRef,
    handleBodyScroll,
    handleBodyWheel,
    handleBodyPointerDown,
  };
}
