import * as React from "react"
import { Maximize2, Minimize2 } from "lucide-react"

import { cn } from "@/lib/utils"

// WorkspaceToolPanelMenuProps defines the shared expandable menu used by adaptive workspace panels.
// WorkspaceToolPanelMenuProps 定义自适应工作区面板使用的共享可展开菜单。
export interface WorkspaceToolPanelMenuProps {
  // activeLabel is shown inside the collapsed menu pill.
  // activeLabel 显示在收起菜单胶囊中。
  activeLabel: string
  // ariaLabel names the expanded navigation region.
  // ariaLabel 命名展开后的导航区域。
  ariaLabel: string
  // children contains the page-owned menu rows.
  // children 包含页面拥有的菜单行。
  children: React.ReactNode
  // className adds page-specific menu sizing or scrolling behavior.
  // className 增加页面专属菜单尺寸或滚动行为。
  className?: string
  // collapseLabel names the collapse action.
  // collapseLabel 命名收起操作。
  collapseLabel: string
  // expanded controls whether the full menu or collapsed pill is rendered.
  // expanded 控制渲染完整菜单还是收起胶囊。
  expanded: boolean
  // expandLabel describes the collapsed pill action.
  // expandLabel 描述收起胶囊的展开操作。
  expandLabel: string
  // heading labels the expanded menu header.
  // heading 标记展开菜单的标题。
  heading: string
  // onExpandedChange updates the shared expansion state.
  // onExpandedChange 更新共享展开状态。
  onExpandedChange: (expanded: boolean) => void
}

// WorkspaceToolPanelMenu renders the canonical expandable workspace menu shell.
// WorkspaceToolPanelMenu 渲染规范的可展开工作区菜单外壳。
export function WorkspaceToolPanelMenu(props: WorkspaceToolPanelMenuProps) {
  if (!props.expanded) {
    return (
      <button
        className="workspace-tool-panel-menu-collapsed"
        onClick={() => props.onExpandedChange(true)}
        title={props.expandLabel}
        type="button"
      >
        <span>{props.activeLabel}</span>
        <Maximize2 size={13} />
      </button>
    )
  }

  return (
    <aside
      aria-label={props.ariaLabel}
      className={cn("workspace-tool-panel-menu", props.className)}
    >
      <div className="workspace-tool-panel-menu-header">
        <span>{props.heading}</span>
        <button
          aria-label={props.collapseLabel}
          className="workspace-tool-panel-menu-icon-button"
          onClick={() => props.onExpandedChange(false)}
          type="button"
        >
          <Minimize2 size={13} />
        </button>
      </div>
      <div className="workspace-tool-panel-menu-list">{props.children}</div>
    </aside>
  )
}
