// FloatingContentWidth defines the approved width strategies for menus and lightweight overlays.
// FloatingContentWidth 定义菜单与轻量浮层允许使用的宽度策略。
export type FloatingContentWidth = "trigger" | "content" | "sm" | "md" | "lg"

// FLOATING_SURFACE_CLASS_NAME provides one visual surface for every lightweight overlay.
// FLOATING_SURFACE_CLASS_NAME 为所有轻量浮层提供统一的视觉表面。
export const FLOATING_SURFACE_CLASS_NAME =
  "rounded-xl bg-popover supports-[backdrop-filter]:bg-(--vcn-surface-glass) supports-[backdrop-filter]:backdrop-blur-xl supports-[backdrop-filter]:backdrop-saturate-150 text-popover-foreground shadow-(--vcn-shadow-floating) ring-1 ring-(--vcn-floating-ring) max-w-[calc(100vw-1rem)] outline-hidden"

// MENU_CONTENT_PADDING_CLASS_NAME is the one inset between a menu surface and its rows; separators bleed by the same amount.
// MENU_CONTENT_PADDING_CLASS_NAME 是菜单表面与菜单行之间的唯一内边距；分隔线以相同距离外扩。
export const MENU_CONTENT_PADDING_CLASS_NAME = "p-1.5"

// FLOATING_MOTION_CLASS_NAME provides shared directional entry and exit motion.
// FLOATING_MOTION_CLASS_NAME 提供统一的定向进入与退出动效。
export const FLOATING_MOTION_CLASS_NAME =
  "duration-(--vcn-duration-fast) ease-(--vcn-ease-out) data-closed:duration-(--vcn-duration-instant) data-closed:ease-(--vcn-ease-in) data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95"

// MENU_ITEM_CLASS_NAME provides one density and interaction language for ordinary menu rows: 13px text at least
// 32px tall, and a leading icon in muted ink (destructive rows keep their own color).
// MENU_ITEM_CLASS_NAME 为普通菜单行提供统一的密度与交互语言：13px 文字、行高至少 32px，行首图标为弱化色
// （危险操作行保留自身颜色）。
export const MENU_ITEM_CLASS_NAME =
  "relative flex min-h-8 min-w-0 cursor-default items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] [overflow-wrap:anywhere] outline-hidden select-none transition-colors duration-(--vcn-duration-instant) focus:bg-(--vcn-fill-hover) focus:text-foreground data-inset:pl-8 data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5 [&>svg]:text-muted-foreground"

// MENU_CHECKABLE_ITEM_CLASS_NAME matches ordinary rows and reserves trailing space for checkbox and radio indicators.
// MENU_CHECKABLE_ITEM_CLASS_NAME 与普通菜单行一致，并为复选与单选指示器预留尾部空间。
export const MENU_CHECKABLE_ITEM_CLASS_NAME =
  "relative flex min-h-8 min-w-0 cursor-default items-center gap-2.5 rounded-lg py-1.5 pr-8 pl-2.5 data-[state=checked]:font-medium text-[13px] [overflow-wrap:anywhere] outline-hidden select-none transition-colors duration-(--vcn-duration-instant) focus:bg-(--vcn-fill-hover) focus:text-foreground data-inset:pl-8 data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5 [&>svg]:text-muted-foreground"

// MENU_LABEL_CLASS_NAME provides one hierarchy for non-interactive menu labels, aligned with the row text inset.
// MENU_LABEL_CLASS_NAME 为不可交互的菜单标签提供统一层级，并与菜单行文字内边距对齐。
export const MENU_LABEL_CLASS_NAME = "min-w-0 px-2.5 pt-2 pb-1 text-[11px] font-medium [overflow-wrap:anywhere] text-muted-foreground data-inset:pl-8"

// MENU_SEPARATOR_CLASS_NAME provides one subtle divider for grouped commands.
// MENU_SEPARATOR_CLASS_NAME 为分组命令提供统一的弱分隔线。
export const MENU_SEPARATOR_CLASS_NAME = "-mx-1.5 my-1.5 h-px bg-border"

// MENU_SHORTCUT_CLASS_NAME provides one trailing shortcut treatment.
// MENU_SHORTCUT_CLASS_NAME 为尾部快捷键文本提供统一样式。
export const MENU_SHORTCUT_CLASS_NAME = "ml-auto shrink-0 text-xs tracking-widest text-muted-foreground"

// FLOATING_CONTENT_WIDTH_CLASS_NAME maps semantic width choices to stable layout classes.
// FLOATING_CONTENT_WIDTH_CLASS_NAME 将语义宽度选项映射为稳定的布局类。
const FLOATING_CONTENT_WIDTH_CLASS_NAME: Record<FloatingContentWidth, string> = {
  // trigger is a minimum, never a cap: a menu anchored to a small button still fits its longest row on one line.
  // trigger 是下限而非上限：锚定在小按钮上的菜单仍能让最长一行单行显示。
  trigger: "w-max min-w-[max(10rem,var(--radix-dropdown-menu-trigger-width))] max-w-[min(24rem,calc(100vw-1rem))]",
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
