import React from "react";

// AskAnchorBounds stores viewport offsets for the active conversation panel.
// AskAnchorBounds 保存当前会话面板相对视口的左右偏移。
interface AskAnchorBounds {
  // Left viewport offset in CSS pixels.
  // 视口左侧偏移，单位为 CSS 像素。
  left: number;
  // Right viewport offset in CSS pixels.
  // 视口右侧偏移，单位为 CSS 像素。
  right: number;
}

// useAskAnchorStyle keeps a fixed session panel horizontally aligned with the active conversation panel.
// useAskAnchorStyle 保持固定定位的会话面板与当前会话面板横向对齐。
// requestId re-measures for each request and stops measuring when null; returns the overlay style.
// requestId 为每个请求重新测量，为 null 时停止测量；返回覆盖层样式。
export function useAskAnchorStyle(requestId: string | null): React.CSSProperties {
  // anchorBounds stores the last measured panel offsets.
  // anchorBounds 保存最近一次测量到的面板偏移。
  const [anchorBounds, setAnchorBounds] = React.useState<AskAnchorBounds | null>(null);

  React.useLayoutEffect(() => {
    if (!requestId) {
      setAnchorBounds(null);
      return;
    }
    let frameId: number | null = null;
    let resizeObserver: ResizeObserver | null = null;
    // scheduleMeasure batches layout reads behind the browser frame boundary.
    // scheduleMeasure 将布局读取合并到浏览器帧边界之后执行。
    const scheduleMeasure = () => {
      if (frameId !== null) {
        return;
      }
      frameId = window.requestAnimationFrame(() => {
        frameId = null;
        const nextBounds = readAskAnchorBounds();
        setAnchorBounds((current) => (
          askAnchorBoundsEqual(current, nextBounds) ? current : nextBounds
        ));
      });
    };
    const anchorElement = askAnchorElement();
    scheduleMeasure();
    if (anchorElement && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(scheduleMeasure);
      resizeObserver.observe(anchorElement);
      const shellElement = anchorElement.closest(".vk-stage");
      if (shellElement instanceof HTMLElement) {
        resizeObserver.observe(shellElement);
      }
    }
    window.addEventListener("resize", scheduleMeasure);
    return () => {
      if (frameId !== null) {
        window.cancelAnimationFrame(frameId);
      }
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [requestId]);

  return anchorBounds
    ? { left: anchorBounds.left, right: anchorBounds.right }
    : { left: 0, right: 0 };
}

// askAnchorElement returns the current main conversation panel used as the positioning anchor.
// askAnchorElement 返回用于定位的当前主会话面板。
function askAnchorElement(): HTMLElement | null {
  if (typeof document === "undefined") {
    return null;
  }
  return document.querySelector<HTMLElement>(".vk-stage-main");
}

// readAskAnchorBounds reads viewport offsets for the current conversation panel.
// readAskAnchorBounds 读取当前会话面板相对视口的左右偏移。
function readAskAnchorBounds(): AskAnchorBounds | null {
  const anchorElement = askAnchorElement();
  if (!anchorElement || typeof window === "undefined") {
    return null;
  }
  const rect = anchorElement.getBoundingClientRect();
  if (rect.width <= 0) {
    return null;
  }
  return {
    left: Math.max(0, Math.round(rect.left)),
    right: Math.max(0, Math.round(window.innerWidth - rect.right)),
  };
}

// askAnchorBoundsEqual compares nullable panel bounds without triggering redundant renders.
// askAnchorBoundsEqual 比较可空面板边界，避免触发冗余渲染。
function askAnchorBoundsEqual(left: AskAnchorBounds | null, right: AskAnchorBounds | null): boolean {
  if (left === right) {
    return true;
  }
  if (!left || !right) {
    return false;
  }
  return left.left === right.left && left.right === right.right;
}
