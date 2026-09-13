"use client"

import * as React from "react"
import { HoverCard as HoverCardPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import {
  FLOATING_MOTION_CLASS_NAME,
  FLOATING_SURFACE_CLASS_NAME,
  floatingContentWidthClassName,
  type FloatingContentWidth,
} from "@/components/ui/floating-surface"

// HoverCard exposes the shared interactive hover-card root.
// HoverCard 暴露共享的可交互悬浮卡片根节点。
function HoverCard({
  ...props
}: React.ComponentProps<typeof HoverCardPrimitive.Root>) {
  return <HoverCardPrimitive.Root data-slot="hover-card" {...props} />
}

// HoverCardTrigger binds one interactive element to its hover card.
// HoverCardTrigger 将一个交互元素绑定到对应悬浮卡片。
function HoverCardTrigger({
  ...props
}: React.ComponentProps<typeof HoverCardPrimitive.Trigger>) {
  return <HoverCardPrimitive.Trigger data-slot="hover-card-trigger" {...props} />
}

// HoverCardContent renders hoverable card content in a shared portal.
// HoverCardContent 在共享门户中渲染可驻留的卡片内容。
// Parameters: props choose semantic width and native positioning options. Returns: wrapped content within the viewport width.
// 参数：props 选择语义宽度及原生定位选项。返回值：在视口宽度内换行的内容。
function HoverCardContent({
  className,
  align = "center",
  sideOffset = 4,
  width = "md",
  ...props
}: React.ComponentProps<typeof HoverCardPrimitive.Content> & {
  // Semantic width strategy for the hover-card surface.
  // HoverCard 表面的语义宽度策略。
  width?: Exclude<FloatingContentWidth, "trigger">
}) {
  return (
    <HoverCardPrimitive.Portal>
      <HoverCardPrimitive.Content
        align={align}
        className={cn(
          "z-50 origin-(--radix-hover-card-content-transform-origin) p-3 text-sm",
          FLOATING_SURFACE_CLASS_NAME,
          FLOATING_MOTION_CLASS_NAME,
          floatingContentWidthClassName(width),
          "min-w-0 [overflow-wrap:anywhere]",
          className,
        )}
        data-slot="hover-card-content"
        sideOffset={sideOffset}
        {...props}
      />
    </HoverCardPrimitive.Portal>
  )
}

export { HoverCard, HoverCardContent, HoverCardTrigger }
