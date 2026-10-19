import * as React from "react";
import { cx } from "../behavior/core";

// SeparatorProps follow the Radix separator: decorative separators are hidden from assistive technology.
// SeparatorProps 沿用 Radix 分隔线：装饰性分隔线对辅助技术隐藏。
export interface SeparatorProps extends React.ComponentProps<"div"> {
  orientation?: "horizontal" | "vertical";
  decorative?: boolean;
}

// Separator renders a hairline divider.
// Separator 渲染一条细分隔线。
export function Separator({ className, orientation = "horizontal", decorative = true, ...props }: SeparatorProps) {
  return (
    <div
      data-slot="separator"
      data-orientation={orientation}
      role={decorative ? "none" : "separator"}
      aria-orientation={decorative || orientation === "horizontal" ? undefined : orientation}
      className={cx("vk-sep", className)}
      {...props}
    />
  );
}
