import React from "react";
import { CONVERSATION_SCROLLBAR_POSITION_EVENT, CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, type ConversationScrollbarPosition } from "./conversation-layout-events";

// The minimum thumb size keeps long transcripts easy to grab.
// 最小滑块高度使长会话仍易于抓取。
const MIN_THUMB_HEIGHT_PX = 24;

// Arrow keys move by a small fixed distance; Page Up and Page Down use the viewport height.
// 方向键移动一小段固定距离；翻页键按视口高度移动。
const KEY_SCROLL_STEP_PX = 40;

// ScrollbarDrag holds the pointer and its initial offset within the thumb.
// ScrollbarDrag 保存指针及其在滑块中的初始偏移。
interface ScrollbarDrag {
  // Captured pointer that owns this gesture.
  // 持有本次手势的已捕获指针。
  pointerId: number;
  // Distance between the pointer and the thumb's top edge, in CSS pixels.
  // 指针距滑块顶部的 CSS 像素距离。
  offset: number;
  // Hold the thumb height steady during the gesture, even as content loads.
  // 即使内容陆续加载，手势期间仍保持滑块高度稳定。
  thumbHeight: number;
  // Latest position on the track.
  // 轨道上的最新进度。
  progress: number;
}

// ConversationTranscriptFrameProps binds the native viewport and its visible scrollbar.
// ConversationTranscriptFrameProps 绑定原生视口与可见滚动条。
interface ConversationTranscriptFrameProps {
  // Native scrollport used by the transcript controller.
  // 转录控制器使用的原生滚动视口。
  scrollRef: React.RefObject<HTMLElement | null>;
  // Accessible name of the controlled conversation.
  // 受控会话的无障碍名称。
  label: string;
  // The transcript with its existing context menu and scroll owner.
  // 带现有菜单和滚动所有者的转录。
  children: React.ReactNode;
}

// ConversationTranscriptFrame places a pointer-capturing scrollbar beside the native scrolling content.
// ConversationTranscriptFrame 在原生滚动内容旁放置可捕获指针的滚动条。
// Parameters: props supplies the exact native viewport, its accessible name, and the transcript.
// 参数：props 提供精确原生视口、无障碍名称及转录内容。
// Returns: the transcript frame and its accessible vertical scrollbar.
// 返回值：转录外框及其无障碍垂直滚动条。
export function ConversationTranscriptFrame({ scrollRef, label, children }: ConversationTranscriptFrameProps) {
  // Refs keep scroll updates local to the track and thumb.
  // 引用让滚动更新只作用于轨道和滑块。
  const trackRef = React.useRef<HTMLDivElement>(null);
  const thumbRef = React.useRef<HTMLDivElement>(null);
  const dragRef = React.useRef<ScrollbarDrag | null>(null);
  const viewportId = React.useId();

  // Paint the thumb from actual scroll geometry, or from the held pointer while dragging.
  // 按真实滚动几何绘制滑块，拖动时则以已捕获指针为准。
  const syncThumb = React.useCallback(() => {
    const scrollport = scrollRef.current;
    const track = trackRef.current;
    const thumb = thumbRef.current;
    if (!scrollport || !track || !thumb) return;

    // Keep the track measurable even when it has no overflow to display.
    // 即使没有可展示的溢出，轨道也保持可测量。
    const viewportHeight = scrollport.clientHeight;
    const scrollHeight = scrollport.scrollHeight;
    const maxScrollTop = scrollHeight - viewportHeight;
    const trackHeight = track.clientHeight;
    const drag = dragRef.current;
    const visible = viewportHeight > 0 && (maxScrollTop > 0 || drag !== null);
    track.toggleAttribute("data-scrollable", visible);
    if (!visible) return;

    // Reuse the grabbed dimensions until release so the pointer keeps the same contact point.
    // 松手前复用抓取时的尺寸，使指针始终落在同一抓取点。
    const height = Math.min(trackHeight, drag?.thumbHeight
      ?? Math.max(MIN_THUMB_HEIGHT_PX, trackHeight * viewportHeight / scrollHeight));
    const progress = drag?.progress ?? Math.max(0, Math.min(1, scrollport.scrollTop / maxScrollTop));
    thumb.style.height = `${height}px`;
    thumb.style.transform = `translateY(${progress * (trackHeight - height)}px)`;
    track.setAttribute("aria-valuenow", String(Math.round(progress * 100)));
  }, [scrollRef]);

  React.useLayoutEffect(() => {
    // Observe both the viewport and content so resize and streamed output move the thumb.
    // 同时观察视口与内容，使调整尺寸和流式输出同步滑块。
    const scrollport = scrollRef.current;
    if (!scrollport) return;
    scrollport.id = viewportId;
    const inner = scrollport.querySelector(".vk-transcript-inner");
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(syncThumb);
    observer?.observe(scrollport);
    if (inner) observer?.observe(inner);
    scrollport.addEventListener("scroll", syncThumb, { passive: true });
    scrollport.addEventListener(CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, syncThumb);
    syncThumb();
    return () => {
      observer?.disconnect();
      scrollport.removeEventListener("scroll", syncThumb);
      scrollport.removeEventListener(CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, syncThumb);
    };
  }, [scrollRef, syncThumb, viewportId]);

  // Route writes through the existing scroll controller so reader and tail ownership stay consistent.
  // 通过既有滚动控制器写入位置，保持阅读位置与尾部所有权一致。
  function moveTo(progress: number, dragging: boolean): void {
    scrollRef.current?.dispatchEvent(new CustomEvent<ConversationScrollbarPosition>(
      CONVERSATION_SCROLLBAR_POSITION_EVENT, { detail: { progress, dragging } },
    ));
    syncThumb();
  }

  // Translate the pointer into a track position, including coordinates outside the scrollbar.
  // 将指针换算为轨道进度，包含滚动条外的坐标。
  function updateDrag(event: React.PointerEvent<HTMLDivElement>, dragging: boolean): void {
    const drag = dragRef.current;
    if (!drag || event.pointerId !== drag.pointerId) return;
    // Subtract the grab offset to preserve where the user picked up the thumb.
    // 扣除抓取偏移，保留用户抓住滑块的位置。
    const track = event.currentTarget;
    const rect = track.getBoundingClientRect();
    const travel = rect.height - drag.thumbHeight;
    if (travel > 0) drag.progress = Math.max(0, Math.min(1, (event.clientY - rect.top - drag.offset) / travel));
    if (!dragging) dragRef.current = null;
    moveTo(drag.progress, dragging);
  }

  // A track click centers the thumb; grabbing the thumb itself preserves the exact contact point.
  // 点击轨道时将滑块居中；直接抓取滑块时保留精确接触点。
  function startDrag(event: React.PointerEvent<HTMLDivElement>): void {
    if (event.button !== 0 || dragRef.current) return;
    const track = event.currentTarget;
    const thumb = thumbRef.current;
    if (!thumb || !track.hasAttribute("data-scrollable")) return;
    const rect = thumb.getBoundingClientRect();
    dragRef.current = {
      pointerId: event.pointerId,
      offset: event.target === thumb ? event.clientY - rect.top : rect.height / 2,
      thumbHeight: rect.height,
      progress: 0,
    };
    track.setPointerCapture(event.pointerId);
    track.focus({ preventScroll: true });
    event.preventDefault();
    updateDrag(event, true);
  }

  // Release ownership after a cancellation without using the cancellation event's coordinates.
  // 取消时释放所有权，并舍弃取消事件的坐标。
  function cancelDrag(event: React.PointerEvent<HTMLDivElement>): void {
    const drag = dragRef.current;
    if (!drag || event.pointerId !== drag.pointerId) return;
    dragRef.current = null;
    moveTo(drag.progress, false);
  }

  // Keep standard scrollbar navigation available when the track has keyboard focus.
  // 轨道获得焦点时支持标准滚动条键盘导航。
  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>): void {
    const scrollport = scrollRef.current;
    if (!scrollport || dragRef.current) return;
    const max = scrollport.scrollHeight - scrollport.clientHeight;
    if (max <= 0) return;
    let next = scrollport.scrollTop;
    switch (event.key) {
      case "ArrowUp": next -= KEY_SCROLL_STEP_PX; break;
      case "ArrowDown": next += KEY_SCROLL_STEP_PX; break;
      case "PageUp": next -= scrollport.clientHeight; break;
      case "PageDown": next += scrollport.clientHeight; break;
      case "Home": next = 0; break;
      case "End": next = max; break;
      default: return;
    }
    event.preventDefault();
    moveTo(Math.max(0, Math.min(1, next / max)), false);
  }

  return <div className="vk-transcript-frame">
    {children}
    <div ref={trackRef} className="vk-transcript-scrollbar" role="scrollbar"
      aria-label={label} aria-controls={viewportId} aria-orientation="vertical" aria-valuenow={0}
      tabIndex={0} onPointerDown={startDrag} onPointerMove={(event) => updateDrag(event, true)}
      onPointerUp={(event) => updateDrag(event, false)} onPointerCancel={cancelDrag}
      onLostPointerCapture={cancelDrag} onKeyDown={handleKeyDown}>
      <div className="vk-transcript-scrollbar-thumb" ref={thumbRef} />
    </div>
  </div>;
}
