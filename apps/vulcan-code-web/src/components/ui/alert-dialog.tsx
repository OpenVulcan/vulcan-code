import * as React from "react"
import { AlertDialog as AlertDialogPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"

// AlertDialog renders the shared modal confirmation root and forwards Radix properties.
// AlertDialog 渲染共享模态确认根节点并转发 Radix 属性。
function AlertDialog({
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Root>) {
  return <AlertDialogPrimitive.Root data-slot="alert-dialog" {...props} />
}

// AlertDialogTrigger renders the control that opens an alert dialog.
// AlertDialogTrigger 渲染打开警告确认框的控制器。
function AlertDialogTrigger({
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Trigger>) {
  return (
    <AlertDialogPrimitive.Trigger data-slot="alert-dialog-trigger" {...props} />
  )
}

// AlertDialogPortal mounts alert-dialog content outside the current stacking context.
// AlertDialogPortal 将警告确认内容挂载到当前层叠上下文之外。
function AlertDialogPortal({
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Portal>) {
  return (
    <AlertDialogPrimitive.Portal data-slot="alert-dialog-portal" {...props} />
  )
}

// AlertDialogOverlay renders the modal backdrop and blocks background interaction.
// AlertDialogOverlay 渲染模态背景并阻止背景交互。
function AlertDialogOverlay({
  className,
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Overlay>) {
  return (
    <AlertDialogPrimitive.Overlay
      data-slot="alert-dialog-overlay"
      className={cn(
        "fixed inset-0 z-50 bg-(--vcn-overlay) duration-100 data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0",
        className
      )}
      {...props}
    />
  )
}

// AlertDialogContent renders the focused confirmation surface and accessible content.
// AlertDialogContent 渲染聚焦的确认界面及无障碍内容。
// AlertDialogContentSize defines the supported widths for confirmation surfaces.
// AlertDialogContentSize 定义确认弹窗表面支持的宽度。
type AlertDialogContentSize = "compact" | "sm" | "md"

// AlertDialogContentProps exposes semantic confirmation sizing.
// AlertDialogContentProps 暴露语义化确认弹窗尺寸。
interface AlertDialogContentProps extends React.ComponentProps<typeof AlertDialogPrimitive.Content> {
  // Semantic width of the confirmation surface.
  // 确认弹窗表面的语义宽度。
  size?: AlertDialogContentSize
  // Optional layer class applied to both the modal backdrop and content surface.
  // 可选层级样式，同时应用于模态遮罩与内容表面。
  layerClassName?: string
}

// AlertDialogContent renders the shared confirmation surface at a semantic width.
// AlertDialogContent 使用语义宽度渲染共享确认弹窗表面。
// Parameters: props select size, layer and native confirmation attributes. Returns: the bounded modal confirmation surface.
// 参数：props 选择尺寸、层级和原生确认属性。返回值：有界模态确认表面。
function AlertDialogContent({
  className,
  layerClassName,
  size = "compact",
  ...props
}: AlertDialogContentProps) {
  return (
    <AlertDialogPortal>
      <AlertDialogOverlay className={layerClassName} />
      <AlertDialogPrimitive.Content
        data-slot="alert-dialog-content"
        data-size={size}
        className={cn(
          "group/alert-dialog-content fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 gap-4 rounded-xl bg-popover p-4 text-popover-foreground ring-1 ring-foreground/10 duration-100 outline-none data-[size=compact]:sm:max-w-sm data-[size=sm]:sm:max-w-md data-[size=md]:sm:max-w-lg data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
          layerClassName,
          "min-w-0 [overflow-wrap:anywhere]",
          className
        )}
        {...props}
      />
    </AlertDialogPortal>
  )
}

// AlertDialogHeader groups the confirmation title, description, and optional media.
// AlertDialogHeader 组合确认标题、说明和可选媒体。
function AlertDialogHeader({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-dialog-header"
      className={cn(
        "grid grid-rows-[auto_1fr] place-items-center gap-1.5 text-center has-data-[slot=alert-dialog-media]:grid-rows-[auto_auto_1fr] has-data-[slot=alert-dialog-media]:gap-x-4 sm:group-data-[size=default]/alert-dialog-content:place-items-start sm:group-data-[size=default]/alert-dialog-content:text-left sm:group-data-[size=default]/alert-dialog-content:has-data-[slot=alert-dialog-media]:grid-rows-[auto_1fr]",
        className
      )}
      {...props}
    />
  )
}

// AlertDialogFooter aligns cancel and confirmation actions for safe navigation.
// AlertDialogFooter 对齐取消与确认操作以支持安全导航。
function AlertDialogFooter({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-dialog-footer"
      className={cn(
        "-mx-4 -mb-4 flex flex-col-reverse gap-2 rounded-b-xl border-t bg-muted/50 p-4 group-data-[size=sm]/alert-dialog-content:grid group-data-[size=sm]/alert-dialog-content:grid-cols-2 sm:flex-row sm:justify-end",
        className
      )}
      {...props}
    />
  )
}

// AlertDialogMedia renders optional status artwork inside an alert dialog.
// AlertDialogMedia 渲染警告确认框中的可选状态图形。
function AlertDialogMedia({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-dialog-media"
      className={cn(
        "mb-2 inline-flex size-10 items-center justify-center rounded-md bg-muted sm:group-data-[size=default]/alert-dialog-content:row-span-2 *:[svg:not([class*='size-'])]:size-6",
        className
      )}
      {...props}
    />
  )
}

// AlertDialogTitle renders the accessible confirmation heading.
// AlertDialogTitle 渲染无障碍确认标题。
function AlertDialogTitle({
  className,
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Title>) {
  return (
    <AlertDialogPrimitive.Title
      data-slot="alert-dialog-title"
      className={cn(
        "font-heading text-base font-medium sm:group-data-[size=default]/alert-dialog-content:group-has-data-[slot=alert-dialog-media]/alert-dialog-content:col-start-2",
        className
      )}
      {...props}
    />
  )
}

// AlertDialogDescription renders the consequence and recovery explanation.
// AlertDialogDescription 渲染操作后果与恢复说明。
function AlertDialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Description>) {
  return (
    <AlertDialogPrimitive.Description
      data-slot="alert-dialog-description"
      className={cn(
        "text-sm text-balance text-muted-foreground md:text-pretty *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
        className
      )}
      {...props}
    />
  )
}

// AlertDialogAction renders the final confirmation using shared button variants.
// AlertDialogAction 使用共享按钮变体渲染最终确认操作。
function AlertDialogAction({
  className,
  variant = "default",
  size = "default",
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Action> &
  Pick<React.ComponentProps<typeof Button>, "variant" | "size">) {
  return (
    <Button variant={variant} size={size} asChild>
      <AlertDialogPrimitive.Action
        data-slot="alert-dialog-action"
        className={cn(className)}
        {...props}
      />
    </Button>
  )
}

// AlertDialogCancel renders the safe dismissal action and closes the dialog.
// AlertDialogCancel 渲染安全取消操作并关闭确认框。
function AlertDialogCancel({
  className,
  variant = "outline",
  size = "default",
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Cancel> &
  Pick<React.ComponentProps<typeof Button>, "variant" | "size">) {
  return (
    <Button variant={variant} size={size} asChild>
      <AlertDialogPrimitive.Cancel
        data-slot="alert-dialog-cancel"
        className={cn(className)}
        {...props}
      />
    </Button>
  )
}

export {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogOverlay,
  AlertDialogPortal,
  AlertDialogTitle,
  AlertDialogTrigger,
}
