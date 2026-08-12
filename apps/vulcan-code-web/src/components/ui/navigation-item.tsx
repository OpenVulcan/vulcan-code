import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

// navigationItemVariants defines shared hover, selected, focus, and disabled navigation states.
// navigationItemVariants 定义导航项统一的悬停、选中、焦点及禁用状态。
const navigationItemVariants = cva(
  "min-w-0 rounded-md text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/35 disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      selected: {
        false: "text-muted-foreground hover:bg-accent/55 hover:text-accent-foreground",
        true: "bg-accent text-accent-foreground",
      },
      density: {
        compact: "min-h-7 px-2 py-1",
        default: "min-h-8 px-2.5 py-1.5",
      },
    },
    defaultVariants: {
      selected: false,
      density: "default",
    },
  },
)

// NavItemProps describes a selectable navigation destination.
// NavItemProps 描述一个可选择的导航目标。
export interface NavItemProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof navigationItemVariants> {}

// NavItem renders a navigation destination without owning route state.
// NavItem 渲染导航目标，但不持有路由状态。
// Parameters: props supplies selected state, density, content, and native button behavior.
// 参数：props 提供选中状态、密度、内容及原生按钮行为。
// Returns a button with stable navigation state visuals.
// 返回具有稳定导航状态视觉的按钮。
export function NavItem({ className, density, selected, type = "button", ...props }: NavItemProps) {
  return (
    <button
      aria-current={selected ? "page" : undefined}
      className={cn(navigationItemVariants({ density, selected }), className)}
      data-selected={selected ? "true" : "false"}
      data-slot="nav-item"
      type={type}
      {...props}
    />
  )
}

// TreeRowProps describes one hierarchical navigation row and its indentation depth.
// TreeRowProps 描述一个层级导航行及其缩进深度。
export interface TreeRowProps extends NavItemProps {
  // Zero-based tree depth used only for visual indentation.
  // 仅用于视觉缩进的零基树深度。
  depth?: number
}

// TreeRow renders a hierarchical row while keeping expansion and selection state external.
// TreeRow 渲染层级行，同时将展开与选择状态保留给调用方。
// Parameters: props supplies depth, selected state, content, and native button behavior.
// 参数：props 提供层级、选中状态、内容及原生按钮行为。
// Returns a navigation button indented by the explicit tree depth.
// 返回按照明确树层级缩进的导航按钮。
export function TreeRow({ className, depth = 0, style, ...props }: TreeRowProps) {
  return (
    <NavItem
      className={cn("flex w-full items-center gap-2", className)}
      data-depth={depth}
      data-slot="tree-row"
      role="treeitem"
      style={{ paddingInlineStart: `${8 + depth * 16}px`, ...style }}
      {...props}
    />
  )
}

export { navigationItemVariants }
