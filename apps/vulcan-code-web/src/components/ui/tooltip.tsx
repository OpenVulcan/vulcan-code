import * as React from "react"
import { Tooltip as TooltipPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// pointerFocusSuppression retains pointer-origin focus restoration only until the next keyboard navigation.
// pointerFocusSuppression 仅在下一次键盘导航前保留指针来源的焦点恢复抑制。
let pointerFocusSuppression = new WeakSet<HTMLElement>()

// suppressTooltipFocusAfterPointer marks one trigger so restored focus will not reopen its tooltip.
// suppressTooltipFocusAfterPointer 标记一个触发器，使恢复焦点不会重新打开它的 tooltip。
function suppressTooltipFocusAfterPointer(element: HTMLElement) {
  pointerFocusSuppression.add(element)
}

// tooltipFocusShouldBeSuppressed returns whether focus should be hidden from Radix Tooltip.
// tooltipFocusShouldBeSuppressed 返回当前焦点是否应对 Radix Tooltip 隐藏。
function tooltipFocusShouldBeSuppressed(element: HTMLElement) {
  return pointerFocusSuppression.has(element)
}

// TooltipProvider shares timing and releases pointer suppression when users deliberately navigate with the keyboard.
// TooltipProvider 共享提示时序，并在用户主动键盘导航时解除指针抑制。
// Parameters: props are the Radix provider contract. Returns: the existing provider with one cleaned-up document listener.
// 参数：props 是 Radix 提供者契约。返回值：具有可清理文档监听器的既有提供者。
function TooltipProvider({
  delayDuration = 400,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  React.useEffect(() => {
    // releaseForNavigation runs before focus moves, while Escape preserves a menu's quiet focus restoration.
    // releaseForNavigation 在焦点移动前执行，同时 Escape 保留菜单关闭时不重新弹出提示的焦点恢复。
    const releaseForNavigation = (event: KeyboardEvent) => {
      if (["Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) {
        pointerFocusSuppression = new WeakSet<HTMLElement>()
      }
    }
    document.addEventListener("keydown", releaseForNavigation, true)
    return () => document.removeEventListener("keydown", releaseForNavigation, true)
  }, [])
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

// TooltipContent wraps explanatory text without expanding its intrinsic flex width.
// TooltipContent 换行显示说明文本，避免内在弹性宽度撑开浮层。
// Parameters: props are native tooltip attributes with an optional arrow flag. Returns: the portaled description.
// 参数：props 是原生提示属性及可选箭头开关。返回值：门户中的说明内容。
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
          // Anywhere wrapping also lowers the intrinsic width of anonymous flex text.
          // 任意位置换行同时降低匿名弹性文本的内在宽度。
          "min-w-0 [overflow-wrap:anywhere]",
          "z-50 inline-flex w-fit max-w-72 origin-(--radix-tooltip-content-transform-origin) items-center gap-1.5 whitespace-normal rounded-md bg-foreground px-2.5 py-1.5 text-left text-xs leading-4 text-background has-data-[slot=kbd]:pr-1.5 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 **:data-[slot=kbd]:relative **:data-[slot=kbd]:isolate **:data-[slot=kbd]:z-50 **:data-[slot=kbd]:rounded-sm data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-95 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
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
