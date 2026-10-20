import * as React from "react";
import { cx } from "../behavior/core";
import { surfaceVariants, type SurfaceVariant } from "./surface";

// Card is a surface with a header, content and optional footer; size "sm" tightens its spacing.
// Card 是带标题区、内容区与可选底栏的表面；size 为 "sm" 时收紧间距。
export function Card({
  className,
  size = "default",
  variant = "default",
  ...props
}: React.ComponentProps<"div"> & { size?: "default" | "sm"; variant?: SurfaceVariant }) {
  return (
    <div
      data-slot="card"
      data-size={size}
      data-variant={variant}
      className={cx(surfaceVariants({ variant }), "vk-panel-card", className)}
      {...props}
    />
  );
}

// CardHeader holds the title, description and an optional trailing action.
// CardHeader 承载标题、说明与可选的尾部操作。
export function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-header" className={cx("vk-panel-card-header", className)} {...props} />;
}

// CardTitle is the card heading.
// CardTitle 是卡片标题。
export function CardTitle({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-title" className={cx("vk-panel-card-title", className)} {...props} />;
}

// CardDescription is secondary text under the title.
// CardDescription 是标题下方的次要文字。
export function CardDescription({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-description" className={cx("vk-panel-card-description", className)} {...props} />;
}

// CardAction sits at the header's trailing edge.
// CardAction 位于标题区尾部。
export function CardAction({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-action" className={cx("vk-panel-card-action", className)} {...props} />;
}

// CardContent is the card body.
// CardContent 是卡片正文。
export function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-content" className={cx("vk-panel-card-content", className)} {...props} />;
}

// CardFooter is a sunken action strip at the bottom of the card.
// CardFooter 是卡片底部下沉的操作条。
export function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-footer" className={cx("vk-panel-card-footer", className)} {...props} />;
}
