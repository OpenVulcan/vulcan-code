import * as React from "react"
import { Popover as PopoverPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import {
  FLOATING_MOTION_CLASS_NAME,
  FLOATING_SURFACE_CLASS_NAME,
  floatingContentWidthClassName,
  type FloatingContentWidth,
} from "@/components/ui/floating-surface"

// Popover renders the shared Radix popover root.
// Popover 渲染共享 Radix 浮层根节点。
function Popover({
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Root>) {
  return <PopoverPrimitive.Root data-slot="popover" {...props} />
}

// PopoverTrigger renders the control that opens or closes a popover.
// PopoverTrigger 渲染打开或关闭浮层的控制器。
function PopoverTrigger({
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Trigger>) {
  return <PopoverPrimitive.Trigger data-slot="popover-trigger" {...props} />
}

// PopoverContent renders positioned popover content inside a portal.
// PopoverContent 在 Portal 中渲染定位后的浮层内容。
function PopoverContent({
  className,
  align = "center",
  sideOffset = 4,
  width = "lg",
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Content> & {
  // Semantic width strategy for the popover surface.
  // Popover 表面的语义宽度策略。
  width?: Exclude<FloatingContentWidth, "trigger">
}) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        data-slot="popover-content"
        align={align}
        sideOffset={sideOffset}
        className={cn(
          "z-50 flex origin-(--radix-popover-content-transform-origin) flex-col gap-2.5 p-2.5 text-sm",
          FLOATING_SURFACE_CLASS_NAME,
          FLOATING_MOTION_CLASS_NAME,
          floatingContentWidthClassName(width),
          className
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  )
}

// PopoverAnchor provides an explicit positioning anchor for popover content.
// PopoverAnchor 为浮层内容提供显式定位锚点。
function PopoverAnchor({
  ...props
}: React.ComponentProps<typeof PopoverPrimitive.Anchor>) {
  return <PopoverPrimitive.Anchor data-slot="popover-anchor" {...props} />
}

// PopoverHeader groups the title and description of a popover.
// PopoverHeader 组合浮层的标题与说明。
function PopoverHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="popover-header"
      className={cn("flex flex-col gap-0.5 text-sm", className)}
      {...props}
    />
  )
}

// PopoverTitle renders a semantic heading and forwards native heading attributes.
// PopoverTitle 渲染语义标题并转发原生标题属性。
// Parameters: props are native h2 attributes. Returns: the styled h2 heading.
// 参数：props 是原生 h2 属性。返回值：带共享样式的 h2 标题。
function PopoverTitle({ className, ...props }: React.ComponentProps<"h2">) {
  return (
    <h2
      data-slot="popover-title"
      className={cn("font-medium", className)}
      {...props}
    />
  )
}

// PopoverDescription renders supporting guidance inside a popover.
// PopoverDescription 渲染浮层中的辅助说明。
function PopoverDescription({
  className,
  ...props
}: React.ComponentProps<"p">) {
  return (
    <p
      data-slot="popover-description"
      className={cn("text-muted-foreground", className)}
      {...props}
    />
  )
}

export {
  Popover,
  PopoverAnchor,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
}
