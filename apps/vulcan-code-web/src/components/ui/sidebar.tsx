import * as React from "react"
import { PanelLeftIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"

// SidebarContextValue stores the collapsible sidebar state for one inset shell.
// SidebarContextValue 保存单个 inset 外壳内的可折叠侧栏状态。
interface SidebarContextValue {
  // Current visual state used by sidebar data attributes.
  // 当前用于侧栏 data 属性的视觉状态。
  state: "expanded" | "collapsed"
  // Whether the sidebar is currently open.
  // 当前侧栏是否处于展开状态。
  open: boolean
  // Updates the open state.
  // 更新展开状态。
  setOpen: (open: boolean) => void
  // Toggles the sidebar open state.
  // 切换侧栏展开状态。
  toggleSidebar: () => void
}

// SidebarContext shares state between Sidebar, SidebarInset, and SidebarTrigger.
// SidebarContext 在 Sidebar、SidebarInset 与 SidebarTrigger 之间共享状态。
const SidebarContext = React.createContext<SidebarContextValue | null>(null)

// useSidebar returns the nearest sidebar context.
// useSidebar 返回最近的侧栏上下文。
function useSidebar(): SidebarContextValue {
  const context = React.useContext(SidebarContext)
  if (!context) {
    throw new Error("useSidebar must be used within a SidebarProvider.")
  }
  return context
}

// SidebarProvider renders the flex wrapper that fuses sidebar and inset content.
// SidebarProvider 渲染融合侧栏与 inset 内容的 flex 外层。
function SidebarProvider({
  children,
  className,
  defaultOpen = true,
  ...props
}: React.ComponentProps<"div"> & {
  // Initial open state for the sidebar.
  // 侧栏初始展开状态。
  defaultOpen?: boolean
}) {
  const [open, setOpen] = React.useState(defaultOpen)
  const toggleSidebar = React.useCallback(() => setOpen((current) => !current), [])
  const state = open ? "expanded" : "collapsed"
  const value = React.useMemo<SidebarContextValue>(
    () => ({ state, open, setOpen, toggleSidebar }),
    [open, state, toggleSidebar],
  )

  return (
    <SidebarContext.Provider value={value}>
      <div
        className={cn("vcn-sidebar-wrapper group/sidebar-wrapper", className)}
        data-state={state}
        data-slot="sidebar-wrapper"
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
  collapsible = "icon",
  variant = "inset",
  ...props
}: React.ComponentProps<"aside"> & {
  // Collapse behavior used by CSS data attributes.
  // CSS data 属性使用的折叠行为。
  collapsible?: "icon" | "offcanvas" | "none"
  // Sidebar visual variant.
  // 侧栏视觉变体。
  variant?: "sidebar" | "inset"
}) {
  const { state } = useSidebar()

  return (
    <aside
      className={cn("vcn-sidebar", className)}
      data-collapsible={state === "collapsed" ? collapsible : ""}
      data-side="left"
      data-slot="sidebar"
      data-state={state}
      data-variant={variant}
      {...props}
    >
      {children}
    </aside>
  )
}

// SidebarInset renders the right content pane with shadcn dashboard-like framing.
// SidebarInset 渲染具有 shadcn dashboard 风格边框的右侧内容面板。
function SidebarInset({ className, ...props }: React.ComponentProps<"section">) {
  return <section className={cn("vcn-sidebar-inset", className)} data-slot="sidebar-inset" {...props} />
}

// SidebarTrigger renders the button that toggles the sidebar state.
// SidebarTrigger 渲染用于切换侧栏状态的按钮。
function SidebarTrigger({ className, onClick, ...props }: React.ComponentProps<typeof Button>) {
  const { toggleSidebar } = useSidebar()

  return (
    <Button
      aria-label="切换侧栏"
      className={cn("vcn-sidebar-trigger", className)}
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
  return <div className={cn("vcn-sidebar-header", className)} data-slot="sidebar-header" {...props} />
}

// SidebarContent renders the scrollable sidebar body.
// SidebarContent 渲染可滚动的侧栏主体。
function SidebarContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("vcn-sidebar-content", className)} data-slot="sidebar-content" {...props} />
}

// SidebarGroup renders one sidebar navigation group.
// SidebarGroup 渲染一个侧栏导航分组。
function SidebarGroup({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("vcn-sidebar-group", className)} data-slot="sidebar-group" {...props} />
}

// SidebarGroupLabel renders one sidebar group label.
// SidebarGroupLabel 渲染单个侧栏分组标签。
function SidebarGroupLabel({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("vcn-sidebar-group-label", className)} data-slot="sidebar-group-label" {...props} />
}

// SidebarMenu renders a vertical menu list.
// SidebarMenu 渲染垂直菜单列表。
function SidebarMenu({ className, ...props }: React.ComponentProps<"ul">) {
  return <ul className={cn("vcn-sidebar-menu", className)} data-slot="sidebar-menu" {...props} />
}

// SidebarMenuItem renders one menu item container.
// SidebarMenuItem 渲染单个菜单项容器。
function SidebarMenuItem({ className, ...props }: React.ComponentProps<"li">) {
  return <li className={cn("vcn-sidebar-menu-item", className)} data-slot="sidebar-menu-item" {...props} />
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
      className={cn("vcn-sidebar-menu-button", active && "active", className)}
      data-active={active ? "true" : "false"}
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

export {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
}
