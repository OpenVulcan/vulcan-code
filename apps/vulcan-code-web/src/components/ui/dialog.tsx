"use client"

import * as React from "react"
import { Dialog as DialogPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { useUiText } from "@/components/ui/ui-text"
import { XIcon } from "lucide-react"

function Dialog({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Root>) {
  return <DialogPrimitive.Root data-slot="dialog" {...props} />
}

function DialogTrigger({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Trigger>) {
  return <DialogPrimitive.Trigger data-slot="dialog-trigger" {...props} />
}

function DialogPortal({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Portal>) {
  return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />
}

function DialogClose({
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Close>) {
  return <DialogPrimitive.Close data-slot="dialog-close" {...props} />
}

function DialogOverlay({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Overlay>) {
  return (
    <DialogPrimitive.Overlay
      data-slot="dialog-overlay"
      className={cn(
        "fixed inset-0 isolate z-50 bg-(--vcn-overlay) supports-[backdrop-filter]:backdrop-blur-(--vcn-overlay-blur) duration-(--vcn-duration-base) ease-(--vcn-ease-out) data-closed:duration-(--vcn-duration-fast) data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0",
        className
      )}
      {...props}
    />
  )
}

// DialogContentSize defines the supported surface widths for standard and specialized dialogs.
// DialogContentSize 定义标准弹窗与特殊弹窗支持的表面宽度。
type DialogContentSize = "compact" | "sm" | "md" | "lg" | "xl" | "viewer" | "fullscreen"

// DialogContentLayout defines padding and spacing owned by the shared dialog surface.
// DialogContentLayout 定义由共享弹窗表面负责的内边距与间距。
type DialogContentLayout = "form" | "document" | "viewer"

// DialogContentScrollMode defines which dialog layer owns vertical overflow.
// DialogContentScrollMode 定义由哪一层弹窗结构负责垂直溢出。
type DialogContentScrollMode = "body-scroll" | "content-scroll" | "fixed-footer"

// dialogContentSizeClassNames maps semantic dialog sizes to mergeable width recipes.
// dialogContentSizeClassNames 将语义弹窗尺寸映射为可正确合并的宽度配方。
const dialogContentSizeClassNames: Record<DialogContentSize, string> = {
  compact: "sm:max-w-sm",
  sm: "sm:max-w-md",
  md: "sm:max-w-xl",
  lg: "sm:max-w-2xl",
  xl: "sm:max-w-4xl",
  viewer: "h-[min(860px,calc(100dvh-2rem))] sm:max-w-[min(1180px,calc(100vw-2rem))]",
  fullscreen: "inset-4 h-auto max-w-none translate-x-0 translate-y-0",
}

// DialogContentProps exposes semantic surface options without requiring page-level visual overrides.
// DialogContentProps 暴露语义化表面选项，避免页面级视觉覆盖。
interface DialogContentProps extends React.ComponentProps<typeof DialogPrimitive.Content> {
  // Whether the standard top-right close action is rendered.
  // 是否渲染标准的右上角关闭操作。
  showCloseButton?: boolean
  // Semantic width of the dialog surface.
  // 弹窗表面的语义宽度。
  size?: DialogContentSize
  // Semantic content spacing of the dialog surface.
  // 弹窗表面的语义内容间距。
  layout?: DialogContentLayout
  // Optional overflow ownership used by bounded-height dialogs.
  // 有界高度弹窗使用的可选溢出归属。
  scrollMode?: DialogContentScrollMode
}

// DialogContent renders the shared modal surface with semantic size, layout, and overflow contracts.
// DialogContent 使用语义尺寸、布局与溢出契约渲染共享模态表面。
// Parameters: props select declared surface recipes and native dialog attributes. Returns: the accessible portal and close action.
// 参数：props 选择声明的表面配方及原生弹窗属性。返回值：无障碍门户及关闭操作。
function DialogContent({
  className,
  children,
  showCloseButton = true,
  size = "compact",
  layout = "form",
  scrollMode,
  ...props
}: DialogContentProps) {
  // text follows the application locale or the explicit standalone primitive defaults.
  // text 遵循应用语言或明确约定的基础组件独立默认值。
  const text = useUiText()
  return (
    <DialogPortal>
      <DialogOverlay />
      <DialogPrimitive.Content
        data-slot="dialog-content"
        data-layout={layout}
        data-scroll-mode={scrollMode}
        data-size={size}
        className={cn(
          "fixed top-1/2 left-1/2 z-50 grid min-w-0 w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 rounded-xl bg-popover text-sm text-popover-foreground ring-1 ring-border shadow-(--vcn-shadow-modal) duration-(--vcn-duration-base) ease-(--vcn-ease-out) data-closed:duration-(--vcn-duration-fast) data-closed:ease-(--vcn-ease-in) outline-none data-[layout=document]:gap-0 data-[layout=document]:p-0 data-[layout=form]:gap-4 data-[layout=form]:p-4 data-[layout=viewer]:gap-0 data-[layout=viewer]:p-0 data-[scroll-mode=body-scroll]:max-h-[calc(100dvh-2rem)] data-[scroll-mode=body-scroll]:overflow-y-auto data-[scroll-mode=content-scroll]:max-h-[calc(100dvh-2rem)] data-[scroll-mode=content-scroll]:overflow-hidden data-[scroll-mode=fixed-footer]:max-h-[calc(100dvh-2rem)] data-[scroll-mode=fixed-footer]:overflow-hidden data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
          dialogContentSizeClassNames[size],
          "[overflow-wrap:anywhere]",
          className
        )}
        {...props}
      >
        {children}
        {showCloseButton && (
          <DialogPrimitive.Close data-slot="dialog-close" asChild>
            <Button
              variant="ghost"
              className="absolute top-2 right-2"
              size="icon-sm"
            >
              <XIcon
              />
              <span className="sr-only">{text.close}</span>
            </Button>
          </DialogPrimitive.Close>
        )}
      </DialogPrimitive.Content>
    </DialogPortal>
  )
}

function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="dialog-header"
      className={cn("flex flex-col gap-2", className)}
      {...props}
    />
  )
}

// DialogFooter renders caller actions and an optional localized primitive-owned close button.
// DialogFooter 渲染调用方操作及可选的本地化基础关闭按钮。
// Parameters: props are native layout attributes and the close-action flag. Returns: the shared action footer.
// 参数：props 是原生布局属性与关闭操作开关。返回值：共享操作底栏。
function DialogFooter({
  className,
  showCloseButton = false,
  children,
  ...props
}: React.ComponentProps<"div"> & {
  showCloseButton?: boolean
}) {
  // text is supplied independently from the caller's custom footer actions.
  // text 独立于调用方自定义底栏操作提供。
  const text = useUiText()
  return (
    <div
      data-slot="dialog-footer"
      className={cn(
        "-mx-4 -mb-4 flex flex-col-reverse gap-2 rounded-b-xl border-t bg-muted/50 p-4 sm:flex-row sm:justify-end",
        className
      )}
      {...props}
    >
      {children}
      {showCloseButton && (
        <DialogPrimitive.Close asChild>
          <Button variant="outline">{text.close}</Button>
        </DialogPrimitive.Close>
      )}
    </div>
  )
}

function DialogTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Title>) {
  return (
    <DialogPrimitive.Title
      data-slot="dialog-title"
      className={cn(
        "font-heading text-base leading-none font-medium",
        className
      )}
      {...props}
    />
  )
}

function DialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      data-slot="dialog-description"
      className={cn(
        "text-sm text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
        className
      )}
      {...props}
    />
  )
}

export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
  dialogContentSizeClassNames,
}
