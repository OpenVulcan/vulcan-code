import * as React from "react"
import { SidebarContext, useSidebar, useSidebarNavigation, type SidebarContextValue } from "./sidebar-context"
import { PanelLeftIcon } from "lucide-react"

import { Button } from "@/ui-kit/primitives/button"
import { Sheet, SheetContent, SheetTitle } from "@/ui-kit/primitives/sheet"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip"
import { useI18n } from "@/i18n"
import { cx } from "@/ui-kit/behavior/core"

// subscribeSidebarViewport observes window resizing and returns its cleanup callback.
// subscribeSidebarViewport 监听窗口尺寸变化，返回取消监听的回调。
function subscribeSidebarViewport(listener: () => void): () => void {
  window.addEventListener("resize", listener)
  return () => window.removeEventListener("resize", listener)
}

// readMobileSidebarViewport returns whether the current viewport requires drawer navigation.
// readMobileSidebarViewport 返回当前视口是否需要抽屉导航。
function readMobileSidebarViewport(): boolean {
  return window.innerWidth <= 720
}

// SidebarProvider renders the flex wrapper that fuses sidebar and inset content.
// SidebarProvider 渲染融合侧栏与 inset 内容的 flex 外层。
function SidebarProvider({
  children,
  className,
  collapsible = true,
  defaultOpen = true,
  open,
  onOpenChange,
  ...props
}: React.ComponentProps<"div"> & {
  // Whether the desktop panel may be collapsed; when false, desktop open requests are ignored and the trigger is hidden.
  // 桌面面板是否允许折叠；为 false 时忽略桌面展开状态变更请求，并隐藏触发按钮。
  collapsible?: boolean
  // Initial open state for the sidebar.
  // 侧栏初始展开状态。
  defaultOpen?: boolean
  // Controlled open state shared across remounted sidebar shells.
  // 跨重新挂载侧栏外壳共享的受控展开状态。
  open?: boolean
  // Controlled open-state change handler.
  // 受控展开状态变更处理器。
  onOpenChange?: (open: boolean) => void
}) {
  // uncontrolledOpen preserves the original standalone provider behavior.
  // uncontrolledOpen 保留原始独立 Provider 行为。
  const [uncontrolledOpen, setUncontrolledOpen] = React.useState(defaultOpen)
  // wrapperRef bounds focus restoration to this sidebar provider.
  // wrapperRef 将焦点恢复范围限定为当前侧栏 Provider。
  const wrapperRef = React.useRef<HTMLDivElement>(null)
  // focusTrigger tolerates providers that intentionally render without a trigger.
  // focusTrigger 允许调用方有意省略导航触发器。
  const focusTrigger = React.useCallback(() => wrapperRef.current?.querySelector<HTMLButtonElement>('[data-slot="sidebar-trigger"]')?.focus(), [])
  // Mobile visibility is transient; resizing must never persist a desktop preference.
  // 手机可见状态是临时状态，改变窗口尺寸不能回写桌面偏好。
  const isMobile = React.useSyncExternalStore(subscribeSidebarViewport, readMobileSidebarViewport, () => false)
  // mobileOpen starts closed so content receives the entire phone viewport.
  // mobileOpen 默认关闭，使正文获得完整手机视口。
  const [mobileOpen, setMobileOpen] = React.useState(false)
  // closeMobileSidebar also serves explicit destination-selection callbacks.
  // closeMobileSidebar 同时供明确的导航目标选择回调使用。
  const closeMobileSidebar = React.useCallback(() => setMobileOpen(false), [])
  React.useEffect(closeMobileSidebar, [isMobile, closeMobileSidebar])
  // isControlled identifies the caller-owned desktop preference.
  // isControlled 标识由调用方持有的桌面偏好。
  const isControlled = typeof open === "boolean"
  // desktopOpen retains its original owner while the phone drawer is visible.
  // desktopOpen 在手机抽屉显示期间仍保留原来的状态归属。
  const desktopOpen = isControlled ? open : uncontrolledOpen
  // currentOpen selects the visibility state belonging to the current viewport.
  // currentOpen 选择属于当前视口的可见状态。
  const currentOpen = isMobile ? mobileOpen : desktopOpen
  // setOpen routes the requested visibility to its owner and returns no value.
  // setOpen 将请求的可见状态传给对应所有者，不返回值。
  const setOpen = React.useCallback((nextOpen: boolean) => {
    if (isMobile) {
      setMobileOpen(nextOpen)
      return
    }
    if (!collapsible) {
      return
    }
    if (!isControlled) {
      setUncontrolledOpen(nextOpen)
    }
    onOpenChange?.(nextOpen)
  }, [collapsible, isControlled, isMobile, onOpenChange])
  const toggleSidebar = React.useCallback(() => setOpen(!currentOpen), [currentOpen, setOpen])
  // Ctrl+B (Cmd+B) toggles the panel from anywhere like the trigger button; a terminal keeps the key for its shell.
  // Ctrl+B（Cmd+B）可在任意位置像触发按钮一样切换面板；终端内该按键保留给其 shell。
  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return
      if (event.key.toLowerCase() !== "b" || (event.target instanceof Element && event.target.closest(".xterm"))) return
      event.preventDefault()
      toggleSidebar()
    }
    document.addEventListener("keydown", onKeyDown)
    return () => document.removeEventListener("keydown", onKeyDown)
  }, [toggleSidebar])
  const state = isMobile || currentOpen ? "expanded" : "collapsed"
  const value = React.useMemo<SidebarContextValue>(
    () => ({ state, open: currentOpen, setOpen, toggleSidebar, isMobile, closeMobileSidebar, focusTrigger }),
    [currentOpen, setOpen, state, toggleSidebar, isMobile, closeMobileSidebar, focusTrigger],
  )

  return (
    <SidebarContext.Provider value={value}>
      <div
        className={cx("vk-shell", className)}
        data-sidebar={isMobile || !currentOpen ? "closed" : "open"}
        data-state={state}
        data-mobile={isMobile}
        data-panel-collapsible={collapsible}
        data-slot="sidebar-wrapper"
        ref={wrapperRef}
        {...props}
      >
        {children}
      </div>
    </SidebarContext.Provider>
  )
}

// Sidebar renders the left navigation pane inside the fused shell.
// Sidebar 渲染融合外壳内的左侧导航面板。
function Sidebar({
  children,
  className,
  collapsible = "offcanvas",
  panel = true,
  rail,
  variant = "inset",
  ...props
}: React.ComponentProps<"aside"> & {
  // Collapse behavior used by CSS data attributes; offcanvas hides the whole panel beside the rail.
  // CSS data 属性使用的折叠行为；offcanvas 会把整个面板隐藏在导航轨道旁。
  collapsible?: "offcanvas" | "none"
  // Whether a panel exists beside the rail; false renders the rail alone on every viewport.
  // 轨道旁是否存在面板；false 时在所有视口只渲染轨道。
  panel?: boolean
  // Always-visible activity rail rendered before the collapsible panel and inside the phone drawer.
  // 始终可见的活动轨道，渲染在可折叠面板之前，手机抽屉中同样包含它。
  rail?: React.ReactNode
  // Sidebar visual variant.
  // 侧栏视觉变体。
  variant?: "sidebar" | "inset"
}) {
  const { state, isMobile, open, setOpen, focusTrigger } = useSidebar()
  const { t } = useI18n()

  // Render the same navigation children in a focus-trapped drawer on phones. The drawer itself takes
  // focus on open: focusing the first rail button would pop its tooltip over the drawer.
  // 在手机上将相同的导航内容呈现于具有焦点约束的抽屉中。打开时由抽屉本身获得焦点：
  // 聚焦第一个轨道按钮会在抽屉上弹出它的提示。
  if (isMobile && collapsible !== "none") {
    return (
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent side="left" className={cx("vk-shell-drawer", rail ? "vk-shell-drawer-rail" : undefined)} aria-describedby={undefined} initialFocus={(scope) => scope} onCloseAutoFocus={(event) => { event.preventDefault(); focusTrigger() }}>
          <SheetTitle className="vk-sr-only">{t("sidebar.console")}</SheetTitle>
          {rail}
          {panel ? (
            <aside className={cx("vk-sidebar", className)} data-slot="sidebar" data-state="expanded" data-side="left" data-variant={variant} {...props}>
              {children}
            </aside>
          ) : null}
        </SheetContent>
      </Sheet>
    )
  }

  // hidden removes an offcanvas panel from keyboard and accessibility navigation while it is collapsed.
  // hidden 在 offcanvas 面板折叠期间将其移出键盘与无障碍导航。
  const hidden = state === "collapsed" && collapsible === "offcanvas"
  if (!panel) {
    return <>{rail}</>
  }
  return (
    <>
      {rail}
      <aside
        aria-hidden={hidden || undefined}
        className={cx("vk-sidebar", className)}
        data-collapsible={state === "collapsed" ? collapsible : ""}
        data-side="left"
        data-slot="sidebar"
        data-state={state}
        data-variant={variant}
        inert={hidden || undefined}
        {...props}
      >
        {children}
      </aside>
    </>
  )
}

// SidebarInset renders the main column beside the navigation: title strip, page body and status bar.
// SidebarInset 渲染导航旁的主列：标题条、页面主体与状态栏。
function SidebarInset({ className, ...props }: React.ComponentProps<"section">) {
  return <section className={cx("vk-shell-main", className)} data-slot="sidebar-inset" {...props} />
}

// SidebarTrigger renders the button that toggles the sidebar state.
// SidebarTrigger 渲染用于切换侧栏状态的按钮。
function SidebarTrigger({ className, onClick, ...props }: React.ComponentProps<typeof Button>) {
  const { toggleSidebar, open } = useSidebar()
  const { t } = useI18n()

  return (
    <Button
      aria-label={t("sidebar.toggle")}
      aria-expanded={open}
      aria-keyshortcuts="Control+B"
      className={cx("vk-sidebar-trigger", className)}
      data-slot="sidebar-trigger"
      onClick={(event) => {
        onClick?.(event)
        toggleSidebar()
      }}
      size="icon-sm"
      type="button"
      variant="ghost"
      {...props}
    >
      <PanelLeftIcon size={16} />
    </Button>
  )
}

// SidebarHeader renders the sidebar header strip.
// SidebarHeader 渲染侧栏顶部区域。
function SidebarHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cx("vk-sidebar-head", className)} data-slot="sidebar-header" {...props} />
}

// SidebarContent renders the scrollable sidebar body.
// SidebarContent 渲染可滚动的侧栏主体。
function SidebarContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cx("vk-sidebar-body", className)} data-slot="sidebar-content" {...props} />
}

// SidebarFooter renders fixed controls below the scrollable sidebar body.
// SidebarFooter 渲染位于可滚动侧栏主体下方的固定控制区。
function SidebarFooter({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cx("vk-sidebar-foot", className)} data-slot="sidebar-footer" {...props} />
}

// SidebarGroup renders one sidebar navigation group.
// SidebarGroup 渲染一个侧栏导航分组。
function SidebarGroup({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cx("vk-side-group", className)} data-slot="sidebar-group" {...props} />
}

// SidebarGroupLabel renders one sidebar group label.
// SidebarGroupLabel 渲染单个侧栏分组标签。
function SidebarGroupLabel({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cx("vk-side-label", className)} data-slot="sidebar-group-label" {...props} />
}

// SidebarMenu renders a vertical menu list.
// SidebarMenu 渲染垂直菜单列表。
function SidebarMenu({ className, ...props }: React.ComponentProps<"ul">) {
  return <ul className={cx("vk-side-menu", className)} data-slot="sidebar-menu" {...props} />
}

// SidebarMenuItem renders one menu item container.
// SidebarMenuItem 渲染单个菜单项容器。
function SidebarMenuItem({ className, ...props }: React.ComponentProps<"li">) {
  return <li className={cx("vk-side-item", className)} data-slot="sidebar-menu-item" {...props} />
}

// SidebarMenuButton renders a selectable sidebar menu button with collapsed tooltip support.
// SidebarMenuButton 渲染支持折叠提示的可选侧栏菜单按钮。
function SidebarMenuButton({
  active = false,
  children,
  className,
  tooltip,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  // Whether the menu button is selected.
  // 菜单按钮是否处于选中状态。
  active?: boolean
  // Tooltip shown when the sidebar is collapsed.
  // 侧栏折叠时显示的提示。
  tooltip?: string
}) {
  const { state } = useSidebar()
  const button = (
    <button
      aria-label={state === "collapsed" && tooltip ? tooltip : props["aria-label"]}
      aria-current={active ? "page" : undefined}
      className={cx("vk-nav-item vk-side-button", className)}
      data-active={active ? "true" : "false"}
      data-selected={active ? "true" : "false"}
      data-slot="sidebar-menu-button"
      type="button"
      {...props}
    >
      {children}
    </button>
  )

  if (state !== "collapsed" || !tooltip) {
    return button
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right">{tooltip}</TooltipContent>
    </Tooltip>
  )
}

// SidebarMenuAction renders a trailing action inside one sidebar menu row.
// SidebarMenuAction 渲染单个侧栏菜单行内的尾部操作按钮。
function SidebarMenuAction({
  className,
  showOnHover = false,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  // Whether the action stays hidden until the row is hovered.
  // 操作按钮是否在行悬停前保持隐藏。
  showOnHover?: boolean
}) {
  return (
    <button
      className={cx("vk-side-action", className)}
      data-sidebar-action-hover={showOnHover ? "true" : "false"}
      data-slot="sidebar-menu-action"
      type="button"
      {...props}
    />
  )
}

export {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
  useSidebarNavigation,
}
