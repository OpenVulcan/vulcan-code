"use client"

import * as React from "react"
import { ContextMenu as ContextMenuPrimitive } from "radix-ui"
import { CheckIcon, ChevronRightIcon, MinusIcon } from "lucide-react"

import { cn } from "@/lib/utils"
import {
  FLOATING_MOTION_CLASS_NAME,
  FLOATING_SURFACE_CLASS_NAME,
  MENU_CHECKABLE_ITEM_CLASS_NAME,
  MENU_ITEM_CLASS_NAME,
  MENU_LABEL_CLASS_NAME,
  MENU_SEPARATOR_CLASS_NAME,
  MENU_SHORTCUT_CLASS_NAME,
  floatingContentWidthClassName,
  type FloatingContentWidth,
} from "@/components/ui/floating-surface"

// ContextMenu renders the root provider for one right-click menu.
// ContextMenu 渲染单个右键菜单的根提供器。
function ContextMenu({
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Root>) {
  return <ContextMenuPrimitive.Root data-slot="context-menu" {...props} />
}

// ContextMenuTrigger renders the element that opens the right-click menu.
// ContextMenuTrigger 渲染打开右键菜单的触发元素。
function ContextMenuTrigger({
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Trigger>) {
  return <ContextMenuPrimitive.Trigger data-slot="context-menu-trigger" {...props} />
}

// ContextMenuGroup groups related right-click menu items.
// ContextMenuGroup 对相关右键菜单项进行分组。
function ContextMenuGroup({
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Group>) {
  return <ContextMenuPrimitive.Group data-slot="context-menu-group" {...props} />
}

// ContextMenuPortal renders right-click menu content outside the local DOM stack.
// ContextMenuPortal 将右键菜单内容渲染到局部 DOM 层级之外。
function ContextMenuPortal({
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Portal>) {
  return <ContextMenuPrimitive.Portal data-slot="context-menu-portal" {...props} />
}

// ContextMenuSub renders one nested right-click submenu.
// ContextMenuSub 渲染一个嵌套右键子菜单。
function ContextMenuSub({
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Sub>) {
  return <ContextMenuPrimitive.Sub data-slot="context-menu-sub" {...props} />
}

// ContextMenuRadioGroup groups mutually exclusive right-click menu items.
// ContextMenuRadioGroup 对互斥右键菜单项进行分组。
function ContextMenuRadioGroup({
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.RadioGroup>) {
  return <ContextMenuPrimitive.RadioGroup data-slot="context-menu-radio-group" {...props} />
}

// ContextMenuSubTrigger renders the row that opens a nested submenu.
// ContextMenuSubTrigger 渲染打开嵌套子菜单的菜单行。
function ContextMenuSubTrigger({
  className,
  inset,
  children,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.SubTrigger> & {
  inset?: boolean
}) {
  return (
    <ContextMenuPrimitive.SubTrigger
      data-inset={inset}
      data-slot="context-menu-sub-trigger"
      className={cn(
        MENU_ITEM_CLASS_NAME,
        "data-[state=open]:bg-(--vcn-fill-hover) data-[state=open]:text-foreground",
        className,
      )}
      {...props}
    >
      {children}
      <ChevronRightIcon className="ml-auto size-4" />
    </ContextMenuPrimitive.SubTrigger>
  )
}

// ContextMenuSubContent renders the floating body of a nested submenu.
// ContextMenuSubContent 渲染嵌套子菜单的浮动主体。
// Parameters: props preserve native submenu options and collision padding. Returns: a portaled, scrollable submenu.
// 参数：props 保留原生子菜单选项及碰撞留白。返回值：门户中的可滚动子菜单。
function ContextMenuSubContent({
  className,
  collisionPadding = 8,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.SubContent>) {
  return (
    <ContextMenuPrimitive.Portal>
    <ContextMenuPrimitive.SubContent
      data-slot="context-menu-sub-content"
      collisionPadding={collisionPadding}
      className={cn(
        "z-50 max-h-(--radix-context-menu-content-available-height) origin-(--radix-context-menu-content-transform-origin) overflow-x-hidden overflow-y-auto overscroll-contain p-1 data-[state=closed]:overflow-hidden",
        FLOATING_SURFACE_CLASS_NAME,
        FLOATING_MOTION_CLASS_NAME,
        "min-w-[min(8rem,var(--radix-context-menu-content-available-width))] max-w-(--radix-context-menu-content-available-width)",
        className,
      )}
      {...props}
    />
    </ContextMenuPrimitive.Portal>
  )
}

// ContextMenuContent renders the floating body of the right-click menu.
// ContextMenuContent 渲染右键菜单的浮动主体。
function ContextMenuContent({
  className,
  width = "content",
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Content> & {
  // Semantic width strategy for the right-click menu surface.
  // 右键菜单表面的语义宽度策略。
  width?: Exclude<FloatingContentWidth, "trigger">
}) {
  return (
    <ContextMenuPrimitive.Portal>
      <ContextMenuPrimitive.Content
        data-slot="context-menu-content"
        className={cn(
          "z-50 max-h-(--radix-context-menu-content-available-height) origin-(--radix-context-menu-content-transform-origin) overflow-x-hidden overflow-y-auto p-1 data-[state=closed]:overflow-hidden",
          FLOATING_SURFACE_CLASS_NAME,
          FLOATING_MOTION_CLASS_NAME,
          floatingContentWidthClassName(width),
          className,
        )}
        {...props}
      />
    </ContextMenuPrimitive.Portal>
  )
}

// ContextMenuItem renders one selectable right-click menu command.
// ContextMenuItem 渲染单个可选择的右键菜单命令。
function ContextMenuItem({
  className,
  inset,
  variant = "default",
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Item> & {
  inset?: boolean
  variant?: "default" | "destructive"
}) {
  return (
    <ContextMenuPrimitive.Item
      data-inset={inset}
      data-slot="context-menu-item"
      data-variant={variant}
      className={cn(
        "group/context-menu-item data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 data-[variant=destructive]:focus:text-destructive dark:data-[variant=destructive]:focus:bg-destructive/20",
        MENU_ITEM_CLASS_NAME,
        className,
      )}
      {...props}
    />
  )
}

// ContextMenuCheckboxItem renders one checkable right-click menu command.
// ContextMenuCheckboxItem 渲染单个可勾选的右键菜单命令。
// Parameters: props supply native check state and actions. Returns: a row with distinct full and mixed indicators.
// 参数：props 提供原生选择状态及操作。返回值：具有不同全选和部分选择指示的菜单行。
function ContextMenuCheckboxItem({
  className,
  children,
  checked,
  inset,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.CheckboxItem> & {
  inset?: boolean
}) {
  return (
    <ContextMenuPrimitive.CheckboxItem
      data-slot="context-menu-checkbox-item"
      data-inset={inset}
      className={cn(
        MENU_CHECKABLE_ITEM_CLASS_NAME,
        className,
      )}
      checked={checked}
      {...props}
    >
      <span className="pointer-events-none absolute right-2 flex items-center justify-center text-primary">
        <ContextMenuPrimitive.ItemIndicator className="group/menu-check-indicator">
          <CheckIcon className="group-data-[state=indeterminate]/menu-check-indicator:hidden" />
          <MinusIcon className="hidden group-data-[state=indeterminate]/menu-check-indicator:block" />
        </ContextMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </ContextMenuPrimitive.CheckboxItem>
  )
}

// ContextMenuRadioItem renders one radio-style right-click menu command.
// ContextMenuRadioItem 渲染单个单选样式的右键菜单命令。
function ContextMenuRadioItem({
  className,
  children,
  inset,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.RadioItem> & {
  inset?: boolean
}) {
  return (
    <ContextMenuPrimitive.RadioItem
      data-slot="context-menu-radio-item"
      data-inset={inset}
      className={cn(
        MENU_CHECKABLE_ITEM_CLASS_NAME,
        className,
      )}
      {...props}
    >
      <span className="pointer-events-none absolute right-2 flex items-center justify-center text-primary">
        <ContextMenuPrimitive.ItemIndicator>
          <CheckIcon />
        </ContextMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </ContextMenuPrimitive.RadioItem>
  )
}

// ContextMenuLabel renders a non-interactive right-click menu label.
// ContextMenuLabel 渲染不可交互的右键菜单标签。
function ContextMenuLabel({
  className,
  inset,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Label> & {
  inset?: boolean
}) {
  return (
    <ContextMenuPrimitive.Label
      data-inset={inset}
      data-slot="context-menu-label"
      className={cn(MENU_LABEL_CLASS_NAME, className)}
      {...props}
    />
  )
}

// ContextMenuSeparator renders a visual separator inside the menu.
// ContextMenuSeparator 渲染菜单内部的视觉分隔线。
function ContextMenuSeparator({
  className,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Separator>) {
  return (
    <ContextMenuPrimitive.Separator
      data-slot="context-menu-separator"
      className={cn(MENU_SEPARATOR_CLASS_NAME, className)}
      {...props}
    />
  )
}

// ContextMenuShortcut renders trailing shortcut text for a menu item.
// ContextMenuShortcut 渲染菜单项末尾的快捷键文本。
function ContextMenuShortcut({
  className,
  ...props
}: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="context-menu-shortcut"
      className={cn(MENU_SHORTCUT_CLASS_NAME, className)}
      {...props}
    />
  )
}

export {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuCheckboxItem,
  ContextMenuRadioItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuGroup,
  ContextMenuPortal,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuRadioGroup,
}
