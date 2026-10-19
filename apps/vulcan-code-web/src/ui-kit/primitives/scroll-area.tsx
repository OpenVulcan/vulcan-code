import * as React from "react";
import { cx } from "../behavior/core";

// ScrollArea is a scrolling region with the kit's thin scrollbars; the viewport child keeps the earlier
// slot name so code that scrolls the viewport still finds it.
// ScrollArea 是使用 kit 细滚动条的滚动区域；视口子元素保留早期槽位名，使滚动视口的代码仍能找到它。
export function ScrollArea({ className, children, ...props }: React.ComponentProps<"div">) {
  return (
    <div data-slot="scroll-area" className={cx("vk-scroll-area", className)} {...props}>
      <div data-slot="scroll-area-viewport" className="vk-scroll-area-viewport">
        {children}
      </div>
    </div>
  );
}

// ScrollBar is kept for API compatibility; native scrollbars are styled by the kit.
// ScrollBar 为兼容 API 而保留；原生滚动条由 kit 统一样式。
export function ScrollBar(_props: { orientation?: "vertical" | "horizontal"; className?: string }) {
  return null;
}
