"use client"

import * as React from "react"
import { DropdownMenu as DropdownMenuPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"
import { CheckIcon, ChevronRightIcon, MinusIcon } from "lucide-react"
import {
  FLOATING_MOTION_CLASS_NAME,
  FLOATING_SURFACE_CLASS_NAME,
  MENU_CHECKABLE_ITEM_CLASS_NAME,
  MENU_CONTENT_PADDING_CLASS_NAME,
  MENU_ITEM_CLASS_NAME,
  MENU_LABEL_CLASS_NAME,
  MENU_SEPARATOR_CLASS_NAME,
  MENU_SHORTCUT_CLASS_NAME,
  floatingContentWidthClassName,
  type FloatingContentWidth,
} from "@/components/ui/floating-surface"

function DropdownMenu({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Root>) {
  return <DropdownMenuPrimitive.Root data-slot="dropdown-menu" {...props} />
}

function DropdownMenuPortal({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Portal>) {
  return (
    <DropdownMenuPrimitive.Portal data-slot="dropdown-menu-portal" {...props} />
  )
}

function DropdownMenuTrigger({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Trigger>) {
  return (
    <DropdownMenuPrimitive.Trigger
      data-slot="dropdown-menu-trigger"
      {...props}
    />
  )
}

// DropdownMenuContent renders shared menu content and avoids restoring focus to a trigger after pointer dismissal.
// DropdownMenuContent 渲染共享菜单内容，并避免指针点击外部关闭后把焦点强制还给触发按钮。
//
// Parameters:
// 参数：接收 Radix 下拉菜单内容属性、定位参数及可选外部交互回调。
// Returns the portal-mounted menu content while preserving keyboard focus restoration.
// 返回挂载到 Portal 的菜单内容，同时保留键盘关闭时的焦点恢复。
function DropdownMenuContent({
  className,
  align = "start",
  sideOffset = 4,
  width = "trigger",
  onCloseAutoFocus,
  onInteractOutside,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Content> & {
  // Semantic width strategy for the menu surface.
  // 菜单表面的语义宽度策略。
  width?: FloatingContentWidth
}) {
  // interactedOutsideRef distinguishes pointer dismissal from keyboard-driven menu closure.
  // interactedOutsideRef 区分指针点击外部关闭与键盘驱动的菜单关闭。
  const interactedOutsideRef = React.useRef(false)
  // handleInteractOutside records only unhandled outside interactions that will dismiss the menu.
  // handleInteractOutside 仅记录未被调用方阻止、将实际关闭菜单的外部交互。
  const handleInteractOutside = React.useCallback<
    NonNullable<React.ComponentProps<typeof DropdownMenuPrimitive.Content>["onInteractOutside"]>
  >((event) => {
    onInteractOutside?.(event)
    interactedOutsideRef.current = !event.defaultPrevented
  }, [onInteractOutside])
  // handleCloseAutoFocus suppresses pointer-dismiss focus return while retaining Radix keyboard behavior.
  // handleCloseAutoFocus 阻止指针关闭后的焦点回送，同时保留 Radix 的键盘行为。
  const handleCloseAutoFocus = React.useCallback<
    NonNullable<React.ComponentProps<typeof DropdownMenuPrimitive.Content>["onCloseAutoFocus"]>
  >((event) => {
    // interactedOutside is consumed once so later keyboard closures cannot inherit stale pointer state.
    // interactedOutside 只消费一次，避免后续键盘关闭继承陈旧的指针状态。
    const interactedOutside = interactedOutsideRef.current
    interactedOutsideRef.current = false
    onCloseAutoFocus?.(event)
    if (interactedOutside && !event.defaultPrevented) {
      event.preventDefault()
    }
  }, [onCloseAutoFocus])

  return (
    <DropdownMenuPrimitive.Portal>
      <DropdownMenuPrimitive.Content
        data-slot="dropdown-menu-content"
        sideOffset={sideOffset}
        align={align}
        onCloseAutoFocus={handleCloseAutoFocus}
        onInteractOutside={handleInteractOutside}
        className={cn(
          "z-50 max-h-(--radix-dropdown-menu-content-available-height) origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto data-[state=closed]:overflow-hidden",
          MENU_CONTENT_PADDING_CLASS_NAME,
          FLOATING_SURFACE_CLASS_NAME,
          FLOATING_MOTION_CLASS_NAME,
          floatingContentWidthClassName(width),
          className,
        )}
        {...props}
      />
    </DropdownMenuPrimitive.Portal>
  )
}

function DropdownMenuGroup({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Group>) {
  return (
    <DropdownMenuPrimitive.Group data-slot="dropdown-menu-group" {...props} />
  )
}

function DropdownMenuItem({
  className,
  inset,
  variant = "default",
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Item> & {
  inset?: boolean
  variant?: "default" | "destructive"
}) {
  return (
    <DropdownMenuPrimitive.Item
      data-slot="dropdown-menu-item"
      data-inset={inset}
      data-variant={variant}
      className={cn(
        "group/dropdown-menu-item data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 data-[variant=destructive]:focus:text-destructive dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:*:[svg]:text-destructive",
        MENU_ITEM_CLASS_NAME,
        className
      )}
      {...props}
    />
  )
}

// DropdownMenuCheckboxItem preserves native checkable menu state, including a distinct mixed indicator.
// DropdownMenuCheckboxItem 保留原生可选菜单状态，包括独立的部分选择指示。
// Parameters: props supply native check state and actions. Returns: the accessible checkable menu row.
// 参数：props 提供原生选择状态及操作。返回值：无障碍可选菜单行。
function DropdownMenuCheckboxItem({
  className,
  children,
  checked,
  inset,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.CheckboxItem> & {
  inset?: boolean
}) {
  return (
    <DropdownMenuPrimitive.CheckboxItem
      data-slot="dropdown-menu-checkbox-item"
      data-inset={inset}
      className={cn(
        MENU_CHECKABLE_ITEM_CLASS_NAME,
        className
      )}
      checked={checked}
      {...props}
    >
      <span
        className="pointer-events-none absolute right-2 flex items-center justify-center text-primary"
        data-slot="dropdown-menu-checkbox-item-indicator"
      >
        <DropdownMenuPrimitive.ItemIndicator className="group/menu-check-indicator">
          <CheckIcon className="group-data-[state=indeterminate]/menu-check-indicator:hidden" />
          <MinusIcon className="hidden group-data-[state=indeterminate]/menu-check-indicator:block" />
        </DropdownMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </DropdownMenuPrimitive.CheckboxItem>
  )
}

function DropdownMenuRadioGroup({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.RadioGroup>) {
  return (
    <DropdownMenuPrimitive.RadioGroup
      data-slot="dropdown-menu-radio-group"
      {...props}
    />
  )
}

function DropdownMenuRadioItem({
  className,
  children,
  inset,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.RadioItem> & {
  inset?: boolean
}) {
  return (
    <DropdownMenuPrimitive.RadioItem
      data-slot="dropdown-menu-radio-item"
      data-inset={inset}
      className={cn(
        MENU_CHECKABLE_ITEM_CLASS_NAME,
        className
      )}
      {...props}
    >
      <span
        className="pointer-events-none absolute right-2 flex items-center justify-center text-primary"
        data-slot="dropdown-menu-radio-item-indicator"
      >
        <DropdownMenuPrimitive.ItemIndicator>
          <CheckIcon
          />
        </DropdownMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </DropdownMenuPrimitive.RadioItem>
  )
}

function DropdownMenuLabel({
  className,
  inset,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Label> & {
  inset?: boolean
}) {
  return (
    <DropdownMenuPrimitive.Label
      data-slot="dropdown-menu-label"
      data-inset={inset}
      className={cn(
        MENU_LABEL_CLASS_NAME,
        className
      )}
      {...props}
    />
  )
}

function DropdownMenuSeparator({
  className,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Separator>) {
  return (
    <DropdownMenuPrimitive.Separator
      data-slot="dropdown-menu-separator"
      className={cn(MENU_SEPARATOR_CLASS_NAME, className)}
      {...props}
    />
  )
}

function DropdownMenuShortcut({
  className,
  ...props
}: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="dropdown-menu-shortcut"
      className={cn(
        MENU_SHORTCUT_CLASS_NAME,
        "group-focus/dropdown-menu-item:text-foreground",
        className
      )}
      {...props}
    />
  )
}

function DropdownMenuSub({
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.Sub>) {
  return <DropdownMenuPrimitive.Sub data-slot="dropdown-menu-sub" {...props} />
}

function DropdownMenuSubTrigger({
  className,
  inset,
  children,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.SubTrigger> & {
  inset?: boolean
}) {
  return (
    <DropdownMenuPrimitive.SubTrigger
      data-slot="dropdown-menu-sub-trigger"
      data-inset={inset}
      className={cn(
        MENU_ITEM_CLASS_NAME,
        "data-open:bg-(--vcn-fill-hover) data-open:text-foreground",
        className
      )}
      {...props}
    >
      {children}
      <ChevronRightIcon className="ml-auto size-3.5" />
    </DropdownMenuPrimitive.SubTrigger>
  )
}

// DropdownMenuSubContent portals dynamic submenus outside transformed parent menus before viewport positioning.
// DropdownMenuSubContent 将动态二级菜单挂载到变换后的父菜单之外，再按视口定位。
// Parameters: props retain the Radix submenu contract. Returns: one themed, keyboard-managed submenu surface.
// 参数：props 保留 Radix 二级菜单契约。返回值：一个主题化且由键盘管理的二级菜单表面。
function DropdownMenuSubContent({
  className,
  collisionPadding = 8,
  ...props
}: React.ComponentProps<typeof DropdownMenuPrimitive.SubContent>) {
  return (
    <DropdownMenuPrimitive.Portal>
    <DropdownMenuPrimitive.SubContent
      data-slot="dropdown-menu-sub-content"
      collisionPadding={collisionPadding}
      className={cn(
        "z-50 max-h-(--radix-dropdown-menu-content-available-height) origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto overscroll-contain data-[state=closed]:overflow-hidden",
        MENU_CONTENT_PADDING_CLASS_NAME,
        FLOATING_SURFACE_CLASS_NAME,
        FLOATING_MOTION_CLASS_NAME,
        "min-w-[min(8rem,var(--radix-dropdown-menu-content-available-width))] max-w-(--radix-dropdown-menu-content-available-width)",
        className,
      )}
      {...props}
    />
    </DropdownMenuPrimitive.Portal>
  )
}

export {
  DropdownMenu,
  DropdownMenuPortal,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuItem,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
}
