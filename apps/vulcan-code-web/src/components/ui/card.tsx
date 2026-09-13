import * as React from "react"

import { surfaceVariants, type SurfaceVariant } from "@/components/ui/surface"
import { cn } from "@/lib/utils"

// Card renders the shared compact or default content surface.
// Card 渲染共享的紧凑或默认内容表面。
function Card({
  className,
  size = "default",
  variant = "default",
  ...props
}: React.ComponentProps<"div"> & {
  size?: "default" | "sm"
  variant?: SurfaceVariant
}) {
  return (
    <div
      data-slot="card"
      data-size={size}
      data-variant={variant}
      className={cn(
        surfaceVariants({ variant }),
        "group/card flex min-w-0 flex-col gap-(--card-spacing) overflow-hidden py-(--card-spacing) text-sm text-card-foreground [overflow-wrap:anywhere] [--card-spacing:--spacing(4)] has-data-[slot=card-footer]:pb-0 has-[>img:first-child]:pt-0 data-[size=sm]:[--card-spacing:--spacing(3)] data-[size=sm]:has-data-[slot=card-footer]:pb-0 *:[img:first-child]:rounded-t-lg *:[img:last-child]:rounded-b-lg",
        className,
      )}
      {...props}
    />
  )
}

// CardHeader renders the shared card identity header.
// CardHeader 渲染共享卡片身份标题区。
function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        "group/card-header @container/card-header grid min-w-0 auto-rows-min items-start gap-1 rounded-t-lg px-(--card-spacing) has-data-[slot=card-action]:grid-cols-[minmax(0,1fr)_auto] has-data-[slot=card-description]:grid-rows-[auto_auto] *:min-w-0 [.border-b]:pb-(--card-spacing)",
        className,
      )}
      {...props}
    />
  )
}

// CardTitle renders the shared card title.
// CardTitle 渲染共享卡片标题。
function CardTitle({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-title"
      className={cn(
        "font-heading text-base leading-snug font-medium group-data-[size=sm]/card:text-sm",
        className,
      )}
      {...props}
    />
  )
}

// CardDescription renders the shared secondary description.
// CardDescription 渲染共享次要说明。
function CardDescription({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-description"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    />
  )
}

// CardAction renders the shared header action slot.
// CardAction 渲染共享标题操作槽。
function CardAction({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-action"
      className={cn(
        "col-start-2 row-span-2 row-start-1 self-start justify-self-end",
        className,
      )}
      {...props}
    />
  )
}

// CardContent renders the shared card body.
// CardContent 渲染共享卡片主体。
function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-content"
      className={cn("px-(--card-spacing)", className)}
      {...props}
    />
  )
}

// CardFooter renders the shared separated action footer.
// CardFooter 渲染共享的分隔操作页脚。
function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-footer"
      className={cn(
        "flex items-center rounded-b-lg border-t bg-muted/50 p-(--card-spacing)",
        className,
      )}
      {...props}
    />
  )
}

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardAction,
  CardDescription,
  CardContent,
}
