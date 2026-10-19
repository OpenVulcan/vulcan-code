import React from "react";

// Anchored positioning for menus, popovers, tooltips and selects: place on the requested side, flip
// when that side lacks room, then shift along the edge to stay inside the viewport padding.
// 菜单、弹出层、提示与选择框的锚定定位：放在请求的一侧，空间不足时翻转，再沿边平移以保持在视口留白内。

// FloatingSide and FloatingAlign follow the Radix vocabulary the existing call sites use.
// FloatingSide 与 FloatingAlign 沿用现有调用处使用的 Radix 词汇。
export type FloatingSide = "top" | "right" | "bottom" | "left";
export type FloatingAlign = "start" | "center" | "end";

// FloatingOptions configures one anchored layer.
// FloatingOptions 配置一个锚定层。
export interface FloatingOptions {
  side?: FloatingSide;
  align?: FloatingAlign;
  sideOffset?: number;
  alignOffset?: number;
  collisionPadding?: number;
  // avoidCollisions false keeps the requested side even when the opposite side has more room.
  // avoidCollisions 为 false 时即使对侧空间更大也保持请求的一侧。
  avoidCollisions?: boolean;
}

// AnchorLike is an element or a fixed point (context menus open at the pointer).
// AnchorLike 是元素或固定点（右键菜单在指针处打开）。
export type AnchorLike = HTMLElement | { x: number; y: number } | null;

// FloatingPlacement is the computed result applied to the floating element.
// FloatingPlacement 是应用到浮动元素上的计算结果。
export interface FloatingPlacement {
  side: FloatingSide;
  align: FloatingAlign;
}

function anchorRect(anchor: AnchorLike): DOMRect | null {
  if (!anchor) return null;
  if (anchor instanceof HTMLElement) return anchor.getBoundingClientRect();
  return new DOMRect(anchor.x, anchor.y, 0, 0);
}

const opposite: Record<FloatingSide, FloatingSide> = { top: "bottom", bottom: "top", left: "right", right: "left" };

// computePosition returns the top-left corner for the floating box and the side actually used.
// computePosition 返回浮动框的左上角坐标及实际使用的一侧。
export function computePosition(
  anchor: DOMRect,
  floating: { width: number; height: number },
  viewport: { width: number; height: number },
  options: FloatingOptions,
): { x: number; y: number; side: FloatingSide; availableWidth: number; availableHeight: number } {
  const pad = options.collisionPadding ?? 8;
  const gap = options.sideOffset ?? 4;
  const alignOffset = options.alignOffset ?? 0;
  const align = options.align ?? "center";
  let side = options.side ?? "bottom";
  const room = (s: FloatingSide) =>
    s === "bottom" ? viewport.height - anchor.bottom - gap - pad
      : s === "top" ? anchor.top - gap - pad
        : s === "right" ? viewport.width - anchor.right - gap - pad
          : anchor.left - gap - pad;
  const need = (s: FloatingSide) => (s === "top" || s === "bottom" ? floating.height : floating.width);
  if (options.avoidCollisions !== false && room(side) < need(side) && room(opposite[side]) > room(side)) side = opposite[side];
  const vertical = side === "top" || side === "bottom";
  let x: number;
  let y: number;
  if (vertical) {
    y = side === "bottom" ? anchor.bottom + gap : anchor.top - gap - floating.height;
    x = align === "start" ? anchor.left + alignOffset
      : align === "end" ? anchor.right - floating.width - alignOffset
        : anchor.left + anchor.width / 2 - floating.width / 2;
  } else {
    x = side === "right" ? anchor.right + gap : anchor.left - gap - floating.width;
    y = align === "start" ? anchor.top + alignOffset
      : align === "end" ? anchor.bottom - floating.height - alignOffset
        : anchor.top + anchor.height / 2 - floating.height / 2;
  }
  x = Math.min(Math.max(x, pad), Math.max(pad, viewport.width - floating.width - pad));
  y = Math.min(Math.max(y, pad), Math.max(pad, viewport.height - floating.height - pad));
  return {
    x,
    y,
    side,
    availableWidth: Math.max(0, vertical ? viewport.width - pad * 2 : room(side)),
    availableHeight: Math.max(0, vertical ? room(side) : viewport.height - pad * 2),
  };
}

// useFloating positions `floating` against `anchor` while open and keeps it attached on scroll and resize.
// useFloating 在打开期间将 `floating` 相对 `anchor` 定位，并在滚动与尺寸变化时保持贴附。
export function useFloating(
  open: boolean,
  anchor: () => AnchorLike,
  floating: React.RefObject<HTMLElement | null>,
  options: FloatingOptions,
): FloatingPlacement {
  const [placement, setPlacement] = React.useState<FloatingPlacement>({ side: options.side ?? "bottom", align: options.align ?? "center" });
  const latest = React.useRef({ anchor, options });
  latest.current = { anchor, options };
  React.useLayoutEffect(() => {
    if (!open) return;
    let frame = 0;
    const update = () => {
      const element = floating.current;
      const rect = anchorRect(latest.current.anchor());
      if (!element || !rect) return;
      const opts = latest.current.options;
      const result = computePosition(
        rect,
        { width: element.offsetWidth, height: element.offsetHeight },
        { width: window.innerWidth, height: window.innerHeight },
        opts,
      );
      element.style.position = "fixed";
      element.style.left = `${Math.round(result.x)}px`;
      element.style.top = `${Math.round(result.y)}px`;
      element.style.setProperty("--vk-anchor-width", `${rect.width}px`);
      element.style.setProperty("--vk-available-height", `${result.availableHeight}px`);
      element.style.setProperty("--vk-available-width", `${result.availableWidth}px`);
      setPlacement((previous) => (previous.side === result.side && previous.align === (opts.align ?? "center")
        ? previous
        : { side: result.side, align: opts.align ?? "center" }));
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener("resize", schedule);
    window.addEventListener("scroll", schedule, true);
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(schedule) : null;
    if (floating.current) observer?.observe(floating.current);
    const anchorElement = latest.current.anchor();
    if (anchorElement instanceof HTMLElement) observer?.observe(anchorElement);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
      observer?.disconnect();
    };
  }, [open, floating]);
  return placement;
}
