import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

// surfaceVariants defines the approved visual roles for ordinary content containers.
// surfaceVariants 定义普通内容容器允许使用的统一视觉角色。
const surfaceVariants = cva(
  "rounded-lg text-foreground",
  {
    variants: {
      variant: {
        default: "bg-card ring-1 ring-foreground/10",
        muted: "bg-muted/45",
        outlined: "border border-border bg-card",
        interactive: "bg-card ring-1 ring-foreground/10 transition-colors hover:bg-accent/40 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/35",
        selected: "bg-accent/55 ring-1 ring-ring/45",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
)

// SurfaceVariant identifies one approved ordinary content-surface treatment.
// SurfaceVariant 标识一种允许使用的普通内容表面样式。
export type SurfaceVariant = NonNullable<VariantProps<typeof surfaceVariants>["variant"]>

// SurfaceProps describes a semantic content surface and its native element properties.
// SurfaceProps 描述语义内容表面及其原生元素属性。
export interface SurfaceProps extends React.ComponentProps<"div">, VariantProps<typeof surfaceVariants> {
  // asChild applies the surface treatment to the supplied child element.
  // asChild 将表面样式应用到传入的子元素。
  asChild?: boolean
}

// Surface renders the shared visual foundation for ordinary product content.
// Surface 为普通产品内容渲染统一视觉基础。
// Parameters: props selects the semantic variant, optional child slot, and native div attributes.
// 参数：props 选择语义样式、可选子元素插槽及原生 div 属性。
// Returns a styled div or slotted child without adding business behavior.
// 返回带样式的 div 或插槽子元素，不增加业务行为。
export function Surface({ asChild = false, className, variant = "default", ...props }: SurfaceProps) {
  // Comp preserves caller-owned semantics while centralizing only the visual surface.
  // Comp 保留调用方拥有的语义，仅集中管理视觉表面。
  const Comp = asChild ? Slot.Root : "div"

  return (
    <Comp
      className={cn(surfaceVariants({ variant }), className)}
      data-slot="surface"
      data-variant={variant}
      {...props}
    />
  )
}

export { surfaceVariants }
