import * as React from "react"
import { Dialog as SheetPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { useUiText } from "@/components/ui/ui-text"
import { XIcon } from "lucide-react"

function Sheet({ ...props }: React.ComponentProps<typeof SheetPrimitive.Root>) {
  return <SheetPrimitive.Root data-slot="sheet" {...props} />
}

function SheetTrigger({
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Trigger>) {
  return <SheetPrimitive.Trigger data-slot="sheet-trigger" {...props} />
}

function SheetClose({
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Close>) {
  return <SheetPrimitive.Close data-slot="sheet-close" {...props} />
}

function SheetPortal({
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Portal>) {
  return <SheetPrimitive.Portal data-slot="sheet-portal" {...props} />
}

function SheetOverlay({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Overlay>) {
  return (
    <SheetPrimitive.Overlay
      data-slot="sheet-overlay"
      className={cn(
        "fixed inset-0 z-50 bg-(--vcn-overlay) supports-[backdrop-filter]:backdrop-blur-(--vcn-overlay-blur) duration-(--vcn-duration-base) ease-(--vcn-ease-out) data-closed:duration-(--vcn-duration-fast) data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0",
        className
      )}
      {...props}
    />
  )
}

// SheetContentSize defines the supported widths for side-mounted sheets.
// SheetContentSize 定义侧边抽屉支持的宽度。
type SheetContentSize = "sm" | "md" | "lg"

// SheetContentProps exposes controlled direction, width, and close-button behavior.
// SheetContentProps 暴露受控的方向、宽度与关闭按钮行为。
interface SheetContentProps extends React.ComponentProps<typeof SheetPrimitive.Content> {
  // Edge from which the sheet enters.
  // 抽屉进入的边缘方向。
  side?: "top" | "right" | "bottom" | "left"
  // Whether the standard close action is rendered.
  // 是否渲染标准关闭操作。
  showCloseButton?: boolean
  // Semantic width used by left and right sheets.
  // 左右侧抽屉使用的语义宽度。
  size?: SheetContentSize
}

// SheetContent renders a shared edge-mounted surface with semantic sizing.
// SheetContent 使用语义尺寸渲染共享的边缘抽屉表面。
// Parameters: props choose a declared side and size and forward native dialog options. Returns: a bounded portal surface.
// 参数：props 选择声明的方向和尺寸并转发原生弹窗选项。返回值：有界门户表面。
function SheetContent({
  className,
  children,
  side = "right",
  showCloseButton = true,
  size = "sm",
  ...props
}: SheetContentProps) {
  // text uses the same standard close label as dialogs, including standalone English defaults.
  // text 使用与弹窗相同的标准关闭文案，包括独立模式英文默认值。
  const text = useUiText()
  return (
    <SheetPortal>
      <SheetOverlay />
      <SheetPrimitive.Content
        data-slot="sheet-content"
        data-side={side}
        data-size={size}
        className={cn(
          // Bound the default sheet so long record details remain scrollable in every direction.
          // 限制默认抽屉高度，使长记录详情在所有进入方向均可滚动。
          "min-w-0 max-h-dvh overflow-y-auto overscroll-contain [overflow-wrap:anywhere]",
          "fixed z-50 flex flex-col gap-4 bg-popover bg-clip-padding text-sm text-popover-foreground shadow-(--vcn-shadow-modal) transition duration-(--vcn-duration-slow) ease-(--vcn-ease-out) data-[side=bottom]:inset-x-0 data-[side=bottom]:bottom-0 data-[side=bottom]:h-auto data-[side=bottom]:border-t data-[side=left]:inset-y-0 data-[side=left]:left-0 data-[side=left]:h-full data-[side=left]:w-3/4 data-[side=left]:border-r data-[side=right]:inset-y-0 data-[side=right]:right-0 data-[side=right]:h-full data-[side=right]:w-3/4 data-[side=right]:border-l data-[side=top]:inset-x-0 data-[side=top]:top-0 data-[side=top]:h-auto data-[side=top]:border-b data-[side=left]:data-[size=sm]:sm:max-w-sm data-[side=right]:data-[size=sm]:sm:max-w-sm data-[side=left]:data-[size=md]:sm:max-w-xl data-[side=right]:data-[size=md]:sm:max-w-xl data-[side=left]:data-[size=lg]:sm:max-w-2xl data-[side=right]:data-[size=lg]:sm:max-w-2xl data-open:animate-in data-open:fade-in-0 data-[side=bottom]:data-open:slide-in-from-bottom-10 data-[side=left]:data-open:slide-in-from-left-10 data-[side=right]:data-open:slide-in-from-right-10 data-[side=top]:data-open:slide-in-from-top-10 data-closed:animate-out data-closed:fade-out-0 data-[side=bottom]:data-closed:slide-out-to-bottom-10 data-[side=left]:data-closed:slide-out-to-left-10 data-[side=right]:data-closed:slide-out-to-right-10 data-[side=top]:data-closed:slide-out-to-top-10",
          className
        )}
        {...props}
      >
        {children}
        {showCloseButton && (
          <SheetPrimitive.Close data-slot="sheet-close" asChild>
            <Button
              variant="ghost"
              className="absolute top-3 right-3"
              size="icon-sm"
            >
              <XIcon
              />
              <span className="sr-only">{text.close}</span>
            </Button>
          </SheetPrimitive.Close>
        )}
      </SheetPrimitive.Content>
    </SheetPortal>
  )
}

// SheetHeader reserves close-button space while keeping the heading outside flex compression.
// SheetHeader 预留关闭按钮空间，避免标题被弹性布局压缩。
// Parameters: props are native div attributes. Returns: the shared heading container.
// 参数：props 是原生 div 属性。返回值：共享标题容器。
function SheetHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="sheet-header"
      className={cn("flex min-w-0 shrink-0 flex-col gap-0.5 p-4 pr-12", className)}
      {...props}
    />
  )
}

// SheetFooter keeps native action controls at their intended height inside a scrollable sheet.
// SheetFooter 在可滚动抽屉中保持原生操作控件的预期高度。
// Parameters: props are native div attributes. Returns: the shared action container.
// 参数：props 是原生 div 属性。返回值：共享操作容器。
function SheetFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="sheet-footer"
      className={cn("mt-auto flex min-w-0 shrink-0 flex-col gap-2 p-4", className)}
      {...props}
    />
  )
}

function SheetTitle({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Title>) {
  return (
    <SheetPrimitive.Title
      data-slot="sheet-title"
      className={cn(
        "font-heading text-base font-medium text-foreground",
        className
      )}
      {...props}
    />
  )
}

function SheetDescription({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Description>) {
  return (
    <SheetPrimitive.Description
      data-slot="sheet-description"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    />
  )
}

export {
  Sheet,
  SheetTrigger,
  SheetClose,
  SheetContent,
  SheetHeader,
  SheetFooter,
  SheetTitle,
  SheetDescription,
}
