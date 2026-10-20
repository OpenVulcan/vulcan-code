import * as React from "react";
import { cx } from "../behavior/core";
import { useUiText } from "./ui-text";

// Streaming placeholders: instead of a "Loading…" sentence later swapped for a full page, a silhouette of
// the content shimmers in place. It appears after a short beat, so fast loads never flash it.
// 流式占位：不再显示一句「正在加载…」再整体替换，而是在原位显示内容轮廓的流光；短暂停顿后才出现，快速加载不会闪烁。

// StreamPlaceholderShape picks the silhouette that resembles the content being loaded.
// StreamPlaceholderShape 选择与待加载内容相似的轮廓。
export type StreamPlaceholderShape = "lines" | "rows" | "fields" | "tiles" | "cards";

// FILL_ITEM_LIMIT caps how many silhouette items a filling placeholder renders.
// FILL_ITEM_LIMIT 限制填充式占位最多渲染的轮廓项数。
const FILL_ITEM_LIMIT = 48;

// LINE_WIDTHS vary line lengths so the silhouette reads like text.
// LINE_WIDTHS 让各行长度不同，使轮廓读起来像文字。
const LINE_WIDTHS = ["92%", "76%", "84%", "58%", "70%", "64%"];

// ShimmerBlock is one shimmering bar.
// ShimmerBlock 是一条流光条。
export function ShimmerBlock({ className, ...props }: React.ComponentProps<"span">) {
  return <span aria-hidden="true" data-slot="shimmer-block" className={cx("vk-skeleton", className)} {...props} />;
}

// StreamPlaceholderProps choose the shape, count and accessibility of a placeholder.
// StreamPlaceholderProps 选择占位的形状、数量与可访问性。
export interface StreamPlaceholderProps extends Omit<React.ComponentProps<"div">, "children"> {
  label?: string;
  shape?: StreamPlaceholderShape;
  count?: number;
  decorative?: boolean;
  fill?: boolean;
}

// visibleBottom returns the bottom edge of the nearest scrolling ancestor, or the viewport.
// visibleBottom 返回最近滚动祖先的底边，否则返回视口底边。
function visibleBottom(element: HTMLElement): number {
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (style.overflowY === "auto" || style.overflowY === "scroll") {
      return parent.getBoundingClientRect().bottom - (Number.parseFloat(style.paddingBottom) || 0);
    }
  }
  return window.innerHeight;
}

// useFillCount measures how many items fill the visible space below the placeholder.
// useFillCount 测量占位下方可见空间能容纳的轮廓项数量。
function useFillCount(ref: React.RefObject<HTMLDivElement | null>, enabled: boolean, minimum: number): number | null {
  const [fillCount, setFillCount] = React.useState<number | null>(null);
  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!enabled || !element) {
      setFillCount(null);
      return;
    }
    const measure = () => {
      const item = element.querySelector<HTMLElement>(":scope > :not(.vk-sr-only)");
      const itemHeight = item?.getBoundingClientRect().height ?? 0;
      if (itemHeight <= 0) return;
      const style = getComputedStyle(element);
      const gap = Number.parseFloat(style.rowGap) || 0;
      const columns = Math.max(1, style.gridTemplateColumns.split(" ").filter((track) => track && track !== "none").length);
      const frame = ["padding-top", "padding-bottom", "border-top-width", "border-bottom-width"]
        .reduce((total, property) => total + (Number.parseFloat(style.getPropertyValue(property)) || 0), 0);
      const available = visibleBottom(element) - element.getBoundingClientRect().top - frame;
      const rows = Math.floor((available + gap) / (itemHeight + gap));
      setFillCount(Math.min(FILL_ITEM_LIMIT, Math.max(minimum, rows * columns)));
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [enabled, minimum, ref]);
  return fillCount;
}

// StreamPlaceholder renders the content silhouette.
// StreamPlaceholder 渲染内容轮廓。
export function StreamPlaceholder({ label, shape = "lines", count, decorative, fill, className, ...props }: StreamPlaceholderProps) {
  const text = useUiText();
  const ref = React.useRef<HTMLDivElement>(null);
  const baseCount = count ?? (shape === "lines" || shape === "tiles" || shape === "cards" ? 3 : 4);
  const total = useFillCount(ref, fill === true, baseCount) ?? baseCount;
  const items = Array.from({ length: total }, (_, index) => index);
  return (
    <div
      {...(decorative ? { "aria-hidden": true } : { role: "status", "aria-live": "polite", "aria-busy": true })}
      ref={ref}
      data-slot="stream-placeholder"
      data-shape={shape}
      data-fill={fill ? "true" : undefined}
      className={cx("vk-stream", className)}
      {...props}
    >
      {decorative ? null : <span className="vk-sr-only">{label ?? text.loading}</span>}
      {items.map((index) => {
        if (shape === "rows") {
          return (
            <span className="vk-stream-row" key={index}>
              <ShimmerBlock data-shape="icon" />
              <span className="vk-stream-copy">
                <ShimmerBlock style={{ width: LINE_WIDTHS[(index + 1) % LINE_WIDTHS.length] }} />
                <ShimmerBlock data-shape="minor" style={{ width: LINE_WIDTHS[(index + 3) % LINE_WIDTHS.length] }} />
              </span>
              <ShimmerBlock data-shape="pill" />
            </span>
          );
        }
        if (shape === "fields") {
          return (
            <span className="vk-stream-field" key={index}>
              <ShimmerBlock data-shape="minor" style={{ width: `${28 + ((index * 17) % 30)}%` }} />
              <ShimmerBlock data-shape="control" />
            </span>
          );
        }
        if (shape === "cards") {
          return (
            <span className="vk-stream-card" key={index}>
              <span className="vk-stream-card-head">
                <ShimmerBlock data-shape="tile" />
                <span className="vk-stream-copy">
                  <ShimmerBlock style={{ width: `${30 + ((index * 13) % 24)}%` }} />
                  <ShimmerBlock data-shape="minor" style={{ width: LINE_WIDTHS[(index + 3) % LINE_WIDTHS.length] }} />
                </span>
              </span>
              <ShimmerBlock data-shape="minor" style={{ width: LINE_WIDTHS[index % LINE_WIDTHS.length] }} />
              <ShimmerBlock data-shape="minor" style={{ width: LINE_WIDTHS[(index + 2) % LINE_WIDTHS.length] }} />
            </span>
          );
        }
        if (shape === "tiles") {
          return (
            <span className="vk-stream-tile" key={index}>
              <ShimmerBlock data-shape="minor" style={{ width: "50%" }} />
              <ShimmerBlock data-shape="value" style={{ width: "75%" }} />
            </span>
          );
        }
        return <ShimmerBlock key={index} style={{ width: LINE_WIDTHS[index % LINE_WIDTHS.length] }} />;
      })}
    </div>
  );
}

// InlineStreamPlaceholder is a one-line silhouette that sits inside running text.
// InlineStreamPlaceholder 是嵌入行内文字的单行轮廓。
export function InlineStreamPlaceholder({
  label,
  decorative,
  className,
  ...props
}: Omit<React.ComponentProps<"span">, "children"> & { label?: string; decorative?: boolean }) {
  const text = useUiText();
  return (
    <span
      {...(decorative ? { "aria-hidden": true } : { role: "status", "aria-live": "polite", "aria-busy": true })}
      data-slot="inline-stream-placeholder"
      className={cx("vk-stream-inline", className)}
      {...props}
    >
      {decorative ? null : <span className="vk-sr-only">{label ?? text.loading}</span>}
      <ShimmerBlock data-shape="minor" style={{ width: "100%" }} />
    </span>
  );
}
