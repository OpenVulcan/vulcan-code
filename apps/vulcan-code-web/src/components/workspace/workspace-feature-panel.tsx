import type * as React from "react";

import { Sidebar, SidebarContent, SidebarHeader } from "@/components/ui/sidebar";
import { SidebarBrandMenu } from "@/components/sidebar/sidebar-profile-menus";

// WorkspaceFeaturePanel replaces the session panel while a feature page owns the workbench.
// It shows only that feature's own menu, so no project or conversation rows appear beside management pages.
// WorkspaceFeaturePanel 在功能页持有工作台时替换会话面板。
// 它只显示该功能自身的菜单，使管理页面旁不出现项目或会话行。
export function WorkspaceFeaturePanel(props: {
  // Always-visible activity rail rendered beside this panel.
  // 渲染在此面板旁、始终可见的活动轨道。
  rail: React.ReactNode;
  // Feature name shown in the panel header.
  // 面板标题中显示的功能名称。
  title: string;
  // Feature-owned menu; null means the feature has no panel and only the rail is shown.
  // 功能自有菜单；null 表示该功能没有面板，只显示轨道。
  navigation: React.ReactNode | null;
  // Whether the header should be a native window drag region.
  // 标题是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
}) {
  return (
    <Sidebar collapsible="offcanvas" panel={props.navigation !== null} rail={props.rail} variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          currentLabel={props.title}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        />
      </SidebarHeader>
      <SidebarContent>
        <div className="workspace-feature-panel-nav">{props.navigation}</div>
      </SidebarContent>
    </Sidebar>
  );
}
