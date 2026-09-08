// FloatingContentWidth defines the approved width strategies for menus and lightweight overlays.
// FloatingContentWidth 定义菜单与轻量浮层允许使用的宽度策略。
export type FloatingContentWidth = "trigger" | "content" | "sm" | "md" | "lg"

// FLOATING_SURFACE_CLASS_NAME provides one visual surface for every lightweight overlay.
// FLOATING_SURFACE_CLASS_NAME 为所有轻量浮层提供统一的视觉表面。
export const FLOATING_SURFACE_CLASS_NAME =
  "rounded-lg bg-popover text-popover-foreground shadow-(--vcn-shadow-floating) ring-1 ring-border max-w-[calc(100vw-1rem)] outline-hidden"

// FLOATING_MOTION_CLASS_NAME provides shared directional entry and exit motion.
// FLOATING_MOTION_CLASS_NAME 提供统一的定向进入与退出动效。
export const FLOATING_MOTION_CLASS_NAME =
  "duration-100 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95"

// MENU_ITEM_CLASS_NAME provides one density and interaction language for ordinary menu rows.
// MENU_ITEM_CLASS_NAME 为普通菜单行提供统一的密度与交互语言。
export const MENU_ITEM_CLASS_NAME =
  "relative flex cursor-default items-center gap-1.5 rounded-md px-1.5 py-1 text-sm outline-hidden select-none focus:bg-accent focus:text-accent-foreground data-inset:pl-7 data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"

// MENU_CHECKABLE_ITEM_CLASS_NAME reserves trailing space for checkbox and radio indicators.
// MENU_CHECKABLE_ITEM_CLASS_NAME 为复选与单选指示器预留尾部空间。
export const MENU_CHECKABLE_ITEM_CLASS_NAME =
  "relative flex cursor-default items-center gap-1.5 rounded-md py-1 pr-8 pl-1.5 text-sm outline-hidden select-none focus:bg-accent focus:text-accent-foreground data-inset:pl-7 data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"

// MENU_LABEL_CLASS_NAME provides one hierarchy for non-interactive menu labels.
// MENU_LABEL_CLASS_NAME 为不可交互的菜单标签提供统一层级。
export const MENU_LABEL_CLASS_NAME = "px-1.5 py-1 text-xs font-medium text-muted-foreground data-inset:pl-7"

// MENU_SEPARATOR_CLASS_NAME provides one subtle divider for grouped commands.
// MENU_SEPARATOR_CLASS_NAME 为分组命令提供统一的弱分隔线。
export const MENU_SEPARATOR_CLASS_NAME = "-mx-1 my-1 h-px bg-border"

// MENU_SHORTCUT_CLASS_NAME provides one trailing shortcut treatment.
// MENU_SHORTCUT_CLASS_NAME 为尾部快捷键文本提供统一样式。
export const MENU_SHORTCUT_CLASS_NAME = "ml-auto text-xs tracking-widest text-muted-foreground"

// FLOATING_CONTENT_WIDTH_CLASS_NAME maps semantic width choices to stable layout classes.
// FLOATING_CONTENT_WIDTH_CLASS_NAME 将语义宽度选项映射为稳定的布局类。
const FLOATING_CONTENT_WIDTH_CLASS_NAME: Record<FloatingContentWidth, string> = {
  trigger: "w-(--radix-dropdown-menu-trigger-width) min-w-32",
  content: "w-max min-w-32 max-w-[min(24rem,calc(100vw-1rem))]",
  sm: "w-40",
  md: "w-56",
  lg: "w-72",
}

// floatingContentWidthClassName returns the layout class for one approved floating width.
// floatingContentWidthClassName 返回指定浮层语义宽度对应的布局类。
// Parameters: width selects a trigger-bound, content-driven, or fixed semantic width.
// 参数：width 选择触发器同宽、内容驱动或固定语义宽度。
// Returns the Tailwind class string for the requested width strategy.
// 返回所请求宽度策略对应的 Tailwind 类字符串。
export function floatingContentWidthClassName(width: FloatingContentWidth): string {
  return FLOATING_CONTENT_WIDTH_CLASS_NAME[width]
}
