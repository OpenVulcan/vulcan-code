import * as React from "react"

import { cn } from "@/lib/utils"

export { PageTabs, type PageTabOption } from "@/components/workspace/workspace-directory-layout"

// PageBodyProps describes the single content root of one workspace page.
// PageBodyProps 描述单个工作区页面的唯一内容根。
export interface PageBodyProps extends React.ComponentProps<"div"> {
  // width selects full-width dashboards or a readable column for forms and prose.
  // width 选择全宽仪表板或适合表单与正文的可读列宽。
  width?: "full" | "readable"
}

// PageBody stacks page-level blocks with the shared vertical rhythm; the page header already names the page.
// PageBody 以共享纵向节奏堆叠页面级区块；页面标题栏已经承担页面命名。
export function PageBody({ className, width = "full", ...props }: PageBodyProps) {
  return <div className={cn("page-body", className)} data-slot="page-body" data-width={width} {...props} />
}

// PageToolbarProps describes one row of page filters on the start side and page actions on the end side.
// PageToolbarProps 描述一行页面工具栏：起始侧放筛选，结束侧放页面操作。
export interface PageToolbarProps extends React.ComponentProps<"div"> {
  // actions are trailing page-level commands such as refresh or create.
  // actions 是尾部页面级命令，例如刷新或新建。
  actions?: React.ReactNode
}

// PageToolbar wraps filters and actions, keeping actions visible on narrow containers.
// PageToolbar 包裹筛选与操作，在窄容器中保持操作可见。
export function PageToolbar({ actions, children, className, ...props }: PageToolbarProps) {
  return (
    <div className={cn("page-toolbar", className)} data-slot="page-toolbar" {...props}>
      {children ? <div className="page-toolbar-start">{children}</div> : null}
      {actions ? <div className="page-toolbar-actions">{actions}</div> : null}
    </div>
  )
}

// PageTabPanelProps binds one panel to the active PageTabs value.
// PageTabPanelProps 将单个面板绑定到当前 PageTabs 值。
export interface PageTabPanelProps extends Omit<React.ComponentProps<"div">, "hidden"> {
  // active reports whether this panel's tab is selected.
  // active 表示此面板对应的页签是否被选中。
  active: boolean
  // keepMounted keeps an already visited panel alive while hidden so unsaved state survives switching.
  // keepMounted 让已访问面板在隐藏时保持挂载，使未保存状态在切换后保留。
  keepMounted?: boolean
}

// PageTabPanel mounts a panel on first activation and, when keepMounted, hides it instead of unmounting.
// PageTabPanel 在首次激活时挂载面板；keepMounted 时以隐藏代替卸载。
export function PageTabPanel({ active, keepMounted = true, className, children, ...props }: PageTabPanelProps) {
  // visited records the first activation; it is adjusted during render so the first frame already shows content.
  // visited 记录首次激活；在渲染期间调整，使首帧即可显示内容。
  const [visited, setVisited] = React.useState(active)
  if (active && !visited) setVisited(true)
  if (!active && (!keepMounted || !visited)) return null
  return <div className={cn("page-tab-panel", className)} data-slot="page-tab-panel" hidden={!active} {...props}>{children}</div>
}

// PageActionBarProps describes the commit bar of an editable page.
// PageActionBarProps 描述可编辑页面的提交栏。
export interface PageActionBarProps extends React.ComponentProps<"div"> {
  // actions are the commit commands, primary last.
  // actions 是提交命令，主操作放在最后。
  actions: React.ReactNode
  // pending marks unsaved changes, raising the bar above the content while it sticks.
  // pending 标记存在未保存更改，吸附时将提交栏提升到内容之上。
  pending?: boolean
}

// PageActionBar sticks to the bottom of the scrolling page so save and discard stay reachable.
// PageActionBar 吸附在滚动页面底部，使保存与放弃始终可及。
export function PageActionBar({ actions, pending, className, children, ...props }: PageActionBarProps) {
  return (
    <div className={cn("page-action-bar", className)} data-pending={pending ? "true" : undefined} data-slot="page-action-bar" {...props}>
      <div className="page-action-bar-status">{children}</div>
      <div className="page-action-bar-actions">{actions}</div>
    </div>
  )
}
