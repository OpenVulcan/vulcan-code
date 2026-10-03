import * as React from "react";

import { cn } from "@/lib/utils";
import { useUiText } from "@/components/ui/ui-text";

// StreamPlaceholderShape names the silhouette a placeholder imitates so the real content lands in the same footprint.
// StreamPlaceholderShape 指定占位所模仿的轮廓，使真实内容落在相同位置。
export type StreamPlaceholderShape = "lines" | "rows" | "fields" | "tiles" | "cards";

// FILL_ITEM_LIMIT bounds how many items a filling placeholder may render on very tall viewports.
// FILL_ITEM_LIMIT 限制填充式占位在超高视口中最多渲染的条目数。
const FILL_ITEM_LIMIT = 48;

// LINE_WIDTHS gives text placeholders a natural ragged edge instead of identical bars.
// LINE_WIDTHS 让文本占位呈现自然的参差边缘，而不是等长色条。
const LINE_WIDTHS = ["92%", "76%", "84%", "58%", "70%", "64%"];

// ShimmerBlock renders one decorative shimmering bar; size and radius come from the caller.
// ShimmerBlock 渲染一个装饰性的流光色块；尺寸与圆角由调用方决定。
export function ShimmerBlock({ className, ...props }: React.ComponentProps<"span">) {
  return <span aria-hidden="true" data-slot="shimmer-block" className={cn("vcn-shimmer block h-3 rounded-md", className)} {...props} />;
}

// StreamPlaceholderProps configures one loading silhouette.
// StreamPlaceholderProps 配置一个加载轮廓。
export interface StreamPlaceholderProps extends Omit<React.ComponentProps<"div">, "children"> {
  // Accessible status text; it stays readable by assistive technology while the bars are decorative.
  // 可访问状态文案；色块为装饰，辅助技术仍可读取该文案。
  label?: string;
  // Silhouette to imitate.
  // 要模仿的轮廓。
  shape?: StreamPlaceholderShape;
  // Number of lines, rows, fields or tiles.
  // 行、列表项、字段或卡片的数量。
  count?: number;
  // Decorative placeholders sit beside another one that already announces the loading status.
  // 装饰性占位与另一个已播报加载状态的占位并列，自身不再播报。
  decorative?: boolean;
  // fill stands in for content that will occupy the rest of the page: the silhouette grows to the visible height of
  // its scroll container, and count becomes the minimum.
  // fill 用于代替将占满页面剩余部分的内容：轮廓增长到所在滚动容器的可见高度，count 变为最少数量。
  fill?: boolean;
}

// visibleBottom returns the lower edge of the nearest scrolling ancestor's content box, or of the viewport.
// visibleBottom 返回最近滚动祖先内容框的下边缘；没有滚动祖先时返回视口下边缘。
function visibleBottom(element: HTMLElement): number {
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (style.overflowY === "auto" || style.overflowY === "scroll") {
      return parent.getBoundingClientRect().bottom - (Number.parseFloat(style.paddingBottom) || 0);
    }
  }
  return window.innerHeight;
}

// useFillCount measures one rendered item and returns how many items cover the visible height below the placeholder.
// Returns null when filling is off or layout is unavailable (for example in jsdom), so the fixed count applies.
// useFillCount 测量一个已渲染条目，返回铺满占位下方可见高度所需的条目数。
// 未启用填充或无法取得布局（例如 jsdom）时返回 null，此时使用固定数量。
function useFillCount(ref: React.RefObject<HTMLDivElement | null>, enabled: boolean, minimum: number): number | null {
  const [fillCount, setFillCount] = React.useState<number | null>(null);
  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!enabled || !element) {
      setFillCount(null);
      return;
    }
    const measure = () => {
      const item = element.querySelector<HTMLElement>(":scope > :not(.sr-only)");
      const itemHeight = item?.getBoundingClientRect().height ?? 0;
      if (itemHeight <= 0) return;
      const style = getComputedStyle(element);
      const gap = Number.parseFloat(style.rowGap) || 0;
      const columns = Math.max(1, style.gridTemplateColumns.split(" ").filter((track) => track && track !== "none").length);
      // frame is the placeholder's own padding and border, which surround the items without holding any.
      // frame 是占位自身的内边距与边框，它们包围条目但不容纳条目。
      const frame = ["padding-top", "padding-bottom", "border-top-width", "border-bottom-width"]
        .reduce((total, property) => total + (Number.parseFloat(style.getPropertyValue(property)) || 0), 0);
      const available = visibleBottom(element) - element.getBoundingClientRect().top - frame;
      // Whole rows only, so the silhouette never pushes its container into scrolling.
      // 只计算完整行，使轮廓不会把所在容器撑出滚动条。
      const rows = Math.floor((available + gap) / (itemHeight + gap));
      setFillCount(Math.min(FILL_ITEM_LIMIT, Math.max(minimum, rows * columns)));
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [enabled, minimum, ref]);
  return fillCount;
}

// StreamPlaceholder shows a quiet shimmering silhouette of the content being loaded, so the swap to real content
// reads as the same layout filling in rather than a jump from a sentence to a page.
// StreamPlaceholder 显示正在加载内容的安静流光轮廓，使切换到真实内容时像是同一布局被填满，而不是从一句话跳成一页。
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
      className={cn("vcn-stream-placeholder", className)}
      {...props}
    >
      {decorative ? null : <span className="sr-only">{label ?? text.loading}</span>}
      {items.map((index) => {
        if (shape === "rows") {
          return (
            <span className="vcn-stream-placeholder-row" key={index}>
              <ShimmerBlock className="size-4 rounded-[5px]" />
              <span className="vcn-stream-placeholder-copy">
                <ShimmerBlock style={{ width: LINE_WIDTHS[(index + 1) % LINE_WIDTHS.length] }} />
                <ShimmerBlock className="h-2.5 opacity-70" style={{ width: LINE_WIDTHS[(index + 3) % LINE_WIDTHS.length] }} />
              </span>
              <ShimmerBlock className="h-5 w-12 rounded-full" />
            </span>
          );
        }
        if (shape === "fields") {
          return (
            <span className="vcn-stream-placeholder-field" key={index}>
              <ShimmerBlock className="h-2.5" style={{ width: `${28 + ((index * 17) % 30)}%` }} />
              <ShimmerBlock className="h-8 w-full rounded-lg" />
            </span>
          );
        }
        if (shape === "cards") {
          return (
            <span className="vcn-stream-placeholder-card" key={index}>
              <span className="vcn-stream-placeholder-card-head">
                <ShimmerBlock className="size-8 rounded-lg" />
                <span className="vcn-stream-placeholder-copy">
                  <ShimmerBlock style={{ width: `${30 + ((index * 13) % 24)}%` }} />
                  <ShimmerBlock className="h-2.5 opacity-70" style={{ width: LINE_WIDTHS[(index + 3) % LINE_WIDTHS.length] }} />
                </span>
              </span>
              <ShimmerBlock className="h-2.5" style={{ width: LINE_WIDTHS[index % LINE_WIDTHS.length] }} />
              <ShimmerBlock className="h-2.5" style={{ width: LINE_WIDTHS[(index + 2) % LINE_WIDTHS.length] }} />
            </span>
          );
        }
        if (shape === "tiles") {
          return (
            <span className="vcn-stream-placeholder-tile" key={index}>
              <ShimmerBlock className="h-2.5 w-1/2" />
              <ShimmerBlock className="h-5 w-3/4" />
            </span>
          );
        }
        return (
          <ShimmerBlock
            key={index}
            style={{ width: LINE_WIDTHS[index % LINE_WIDTHS.length] }}
          />
        );
      })}
    </div>
  );
}

// InlineStreamPlaceholder replaces a short loading sentence inside running text, such as a footer hint or a subtitle.
// InlineStreamPlaceholder 替换行内的简短加载语句，例如底栏提示或副标题。
export function InlineStreamPlaceholder({ label, decorative, className, ...props }: Omit<React.ComponentProps<"span">, "children"> & { label?: string; decorative?: boolean }) {
  const text = useUiText();
  return (
    <span
      {...(decorative ? { "aria-hidden": true } : { role: "status", "aria-live": "polite", "aria-busy": true })}
      data-slot="inline-stream-placeholder"
      className={cn("vcn-stream-inline", className)}
      {...props}
    >
      {decorative ? null : <span className="sr-only">{label ?? text.loading}</span>}
      <ShimmerBlock className="h-2.5 w-full" />
    </span>
  );
}
