import * as React from "react";
import { cx } from "../behavior/core";
import { Slot } from "../behavior/slot";

// BadgeVariant is the tone of a small status label.
// BadgeVariant 是小型状态标签的色调。
export type BadgeVariant = "default" | "secondary" | "destructive" | "success" | "warning" | "outline" | "ghost" | "link";

// BADGE_TONES maps each variant onto a kit tone; variants without a tone use the neutral badge.
// BADGE_TONES 将各变体映射到 kit 色调；没有色调的变体使用中性徽标。
const BADGE_TONES: Partial<Record<BadgeVariant, string>> = {
  default: "signal",
  destructive: "bad",
  success: "ok",
  warning: "warn",
};

// Badge renders a compact status or count label.
// Badge 渲染紧凑的状态或计数标签。
export function Badge({
  className,
  variant = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"span"> & { variant?: BadgeVariant | null; asChild?: boolean }) {
  const Comp = asChild ? Slot : "span";
  const resolved = variant ?? "default";
  return (
    <Comp
      data-slot="badge"
      data-variant={resolved}
      data-tone={BADGE_TONES[resolved]}
      className={cx("vk-badge", className)}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}
