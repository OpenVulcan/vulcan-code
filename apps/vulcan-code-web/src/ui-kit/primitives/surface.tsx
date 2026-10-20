import * as React from "react";
import { cx } from "../behavior/core";
import { Slot } from "../behavior/slot";

// SurfaceVariant is the elevation and emphasis of a panel.
// SurfaceVariant 是面板的层级与强调程度。
export type SurfaceVariant = "default" | "muted" | "outlined" | "interactive" | "selected";

// SurfaceProps are div attributes plus the variant and asChild.
// SurfaceProps 是 div 属性加上变体与 asChild。
export interface SurfaceProps extends React.ComponentProps<"div"> {
  variant?: SurfaceVariant | null;
  asChild?: boolean;
}

// surfaceVariants returns the kit classes for a surface variant (kept for callers that style other elements).
// surfaceVariants 返回指定表面变体的 kit 类名（为给其他元素套用样式的调用方保留）。
export function surfaceVariants(options: { variant?: SurfaceVariant | null } = {}): string {
  return `vk-surface vk-surface-${options.variant ?? "default"}`;
}

// Surface renders a panel at one of the shared elevations.
// Surface 以共享层级之一渲染面板。
export function Surface({ asChild = false, className, variant = "default", ...props }: SurfaceProps) {
  const Comp = asChild ? Slot : "div";
  return (
    <Comp
      className={cx(surfaceVariants({ variant }), className)}
      data-slot="surface"
      data-variant={variant ?? "default"}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}
