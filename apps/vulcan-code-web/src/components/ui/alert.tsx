import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"
import { surfaceVariants } from "@/components/ui/surface"

// alertVariants defines the shared visual treatments for alert severity variants.
// alertVariants 定义不同警告级别的共享视觉样式。
const alertVariants = cva(
  "group/alert relative grid w-full gap-0.5 rounded-lg px-3 py-2.5 text-left text-sm has-data-[slot=alert-action]:relative has-data-[slot=alert-action]:pr-18 has-[>svg]:grid-cols-[auto_1fr] has-[>svg]:gap-x-2 *:[svg]:row-span-2 *:[svg]:translate-y-0.5 *:[svg]:text-current *:[svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "text-card-foreground",
        warning: "border border-[color-mix(in_oklch,var(--vcn-content-warning)_25%,transparent)] bg-[var(--vcn-status-warning-background)] text-[var(--vcn-content-warning)]",
        destructive:
          "border border-destructive/25 bg-destructive/6 text-destructive *:data-[slot=alert-description]:text-destructive/90 *:[svg]:text-current",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

// Alert renders a themed status container and forwards native div properties.
// Alert 渲染主题化状态容器并转发原生 div 属性。
function Alert({
  className,
  variant,
  ...props
}: React.ComponentProps<"div"> & VariantProps<typeof alertVariants>) {
  return (
    <div
      data-slot="alert"
      role="alert"
      className={cn(variant === "default" && surfaceVariants({ variant: "outlined" }), alertVariants({ variant }), className)}
      {...props}
    />
  )
}

// AlertTitle renders the concise heading for an alert.
// AlertTitle 渲染警告信息的简短标题。
function AlertTitle({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-title"
      className={cn(
        "font-medium group-has-[>svg]/alert:col-start-2 [&_a]:underline [&_a]:underline-offset-3 [&_a]:hover:text-foreground",
        className
      )}
      {...props}
    />
  )
}

// AlertDescription renders the actionable body content for an alert.
// AlertDescription 渲染警告信息的可操作正文内容。
function AlertDescription({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-description"
      className={cn(
        "text-sm text-balance text-muted-foreground md:text-pretty [&_a]:underline [&_a]:underline-offset-3 [&_a]:hover:text-foreground [&_p:not(:last-child)]:mb-4",
        className
      )}
      {...props}
    />
  )
}

// AlertAction renders the action area aligned with an alert message.
// AlertAction 渲染与警告信息对齐的操作区域。
function AlertAction({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-action"
      className={cn("absolute top-2 right-2", className)}
      {...props}
    />
  )
}

export { Alert, AlertTitle, AlertDescription, AlertAction }
