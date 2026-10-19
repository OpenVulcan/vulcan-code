import * as React from "react";
import { cx } from "../behavior/core";

// Empty is the placeholder shown where a list or page has nothing to show yet.
// Empty 是列表或页面暂无内容时显示的占位区。
export function Empty({ className, variant = "muted", ...props }: React.ComponentProps<"div"> & { variant?: "muted" | "outlined" }) {
  return <div data-slot="empty" data-variant={variant} className={cx("vk-empty", className)} {...props} />;
}

// EmptyHeader groups the media, title and description.
// EmptyHeader 组合图标、标题与说明。
export function EmptyHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="empty-header" className={cx("vk-empty-header", className)} {...props} />;
}

// EmptyMedia holds an illustration or an icon tile.
// EmptyMedia 承载插图或图标方块。
export function EmptyMedia({ className, variant = "default", ...props }: React.ComponentProps<"div"> & { variant?: "default" | "icon" | null }) {
  return <div data-slot="empty-icon" data-variant={variant ?? "default"} className={cx("vk-empty-media", className)} {...props} />;
}

// EmptyTitle states what is missing.
// EmptyTitle 说明缺少的内容。
export function EmptyTitle({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="empty-title" className={cx("vk-empty-title", className)} {...props} />;
}

// EmptyDescription explains how to fill the space.
// EmptyDescription 说明如何填充该区域。
export function EmptyDescription({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="empty-description" className={cx("vk-empty-description", className)} {...props} />;
}

// EmptyContent holds the follow-up actions.
// EmptyContent 承载后续操作。
export function EmptyContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="empty-content" className={cx("vk-empty-content", className)} {...props} />;
}
