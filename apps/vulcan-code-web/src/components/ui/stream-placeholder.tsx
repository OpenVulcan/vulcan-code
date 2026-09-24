import * as React from "react";

import { cn } from "@/lib/utils";
import { useUiText } from "@/components/ui/ui-text";

// StreamPlaceholderShape names the silhouette a placeholder imitates so the real content lands in the same footprint.
// StreamPlaceholderShape 指定占位所模仿的轮廓，使真实内容落在相同位置。
export type StreamPlaceholderShape = "lines" | "rows" | "fields" | "tiles";

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
}

// StreamPlaceholder shows a quiet shimmering silhouette of the content being loaded, so the swap to real content
// reads as the same layout filling in rather than a jump from a sentence to a page.
// StreamPlaceholder 显示正在加载内容的安静流光轮廓，使切换到真实内容时像是同一布局被填满，而不是从一句话跳成一页。
export function StreamPlaceholder({ label, shape = "lines", count, decorative, className, ...props }: StreamPlaceholderProps) {
  const text = useUiText();
  const total = count ?? (shape === "lines" ? 3 : shape === "tiles" ? 3 : 4);
  const items = Array.from({ length: total }, (_, index) => index);
  return (
    <div
      {...(decorative ? { "aria-hidden": true } : { role: "status", "aria-live": "polite", "aria-busy": true })}
      data-slot="stream-placeholder"
      data-shape={shape}
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
