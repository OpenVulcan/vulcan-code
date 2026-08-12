import * as React from "react"
import { Tooltip as TooltipPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// POINTER_FOCUS_SUPPRESSION_MS keeps pointer-activated triggers from reopening tooltips after focus restoration.
// POINTER_FOCUS_SUPPRESSION_MS 防止指针点击过的触发器在焦点恢复后重新弹出 tooltip。
const POINTER_FOCUS_SUPPRESSION_MS = 10 * 60 * 1000

// pointerFocusSuppressionUntil stores the focus-suppression deadline for pointer-activated trigger elements.
// pointerFocusSuppressionUntil 保存指针激活触发器元素的焦点抑制截止时间。
const pointerFocusSuppressionUntil = new WeakMap<HTMLElement, number>()

// suppressTooltipFocusAfterPointer marks one trigger so restored focus will not reopen its tooltip.
// suppressTooltipFocusAfterPointer 标记一个触发器，使恢复焦点不会重新打开它的 tooltip。
function suppressTooltipFocusAfterPointer(element: HTMLElement) {
  pointerFocusSuppressionUntil.set(element, Date.now() + POINTER_FOCUS_SUPPRESSION_MS)
}

// tooltipFocusShouldBeSuppressed returns whether focus should be hidden from Radix Tooltip.
// tooltipFocusShouldBeSuppressed 返回当前焦点是否应对 Radix Tooltip 隐藏。
function tooltipFocusShouldBeSuppressed(element: HTMLElement) {
  const suppressionDeadline = pointerFocusSuppressionUntil.get(element)
  if (!suppressionDeadline) {
    return false
  }
  if (Date.now() > suppressionDeadline) {
    pointerFocusSuppressionUntil.delete(element)
    return false
  }
  return true
}

function TooltipProvider({
  delayDuration = 400,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return (
    <TooltipPrimitive.Provider
      data-slot="tooltip-provider"
      delayDuration={delayDuration}
      {...props}
    />
  )
}

function Tooltip({
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Root>) {
  return <TooltipPrimitive.Root data-slot="tooltip" {...props} />
}

function TooltipTrigger({
  onFocusCapture,
  onPointerDownCapture,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Trigger>) {
  return (
    <TooltipPrimitive.Trigger
      data-slot="tooltip-trigger"
      {...props}
      onFocusCapture={(event) => {
        onFocusCapture?.(event)
        if (
          !event.defaultPrevented
          && event.currentTarget instanceof HTMLElement
          && tooltipFocusShouldBeSuppressed(event.currentTarget)
        ) {
          event.stopPropagation()
        }
      }}
      onPointerDownCapture={(event) => {
        onPointerDownCapture?.(event)
        if (!event.defaultPrevented && event.currentTarget instanceof HTMLElement) {
          suppressTooltipFocusAfterPointer(event.currentTarget)
        }
      }}
    />
  )
}

function TooltipContent({
  className,
  showArrow = true,
  sideOffset = 6,
  children,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Content> & {
  // Whether to render the small tooltip arrow marker.
  // 是否渲染 tooltip 的小箭头标记。
  showArrow?: boolean
}) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        sideOffset={sideOffset}
        className={cn(
          "z-50 inline-flex w-fit max-w-72 origin-(--radix-tooltip-content-transform-origin) items-center gap-1.5 whitespace-normal break-words rounded-md bg-foreground px-2.5 py-1.5 text-left text-xs leading-4 text-background has-data-[slot=kbd]:pr-1.5 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 **:data-[slot=kbd]:relative **:data-[slot=kbd]:isolate **:data-[slot=kbd]:z-50 **:data-[slot=kbd]:rounded-sm data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-95 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
          className
        )}
        {...props}
      >
        {children}
        {showArrow ? (
          <TooltipPrimitive.Arrow className="z-50 size-2.5 translate-y-[calc(-50%_-_2px)] rotate-45 rounded-[2px] bg-foreground fill-foreground" />
        ) : null}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  )
}

export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger }
