import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

// WorkspaceDirectoryLayoutProps separates persistent navigation from the flexible content pane.
// WorkspaceDirectoryLayoutProps 将固定导航与弹性内容面板分离。
interface WorkspaceDirectoryLayoutProps {
  // navigation contains the existing capability-filtered menu and its exact selection callbacks.
  // navigation 包含现有能力过滤菜单及其精确选择回调。
  navigation: ReactNode;
  // children is the unrestricted page body.
  // children 是不限宽的页面正文。
  children: ReactNode;
  // contentClassName preserves page-specific container queries and presentation.
  // contentClassName 保留页面专属容器查询与呈现。
  contentClassName?: string;
  // navigationMode renders short menus as a horizontal tab strip instead of a side column; defaults to column.
  // navigationMode 将较短的菜单渲染为水平标签条而非侧栏，默认 column。
  navigationMode?: "column" | "tabs";
}

// WorkspaceDirectoryLayout renders fixed desktop navigation and a bounded top menu on narrow panes.
// WorkspaceDirectoryLayout 在桌面渲染固定导航，在窄面板渲染限高顶部菜单。
// Parameters: navigation, children and optional content styling. Returns the two-pane page shell.
// 参数：导航、正文及可选正文样式。返回双面板页面外壳。
export function WorkspaceDirectoryLayout(props: WorkspaceDirectoryLayoutProps) {
  return <div className="workspace-directory">
    <div className="workspace-directory-layout" data-navigation-mode={props.navigationMode ?? "column"}>
      <div className="workspace-directory-navigation">{props.navigation}</div>
      <div className={cn("workspace-directory-content", props.contentClassName)}>{props.children}</div>
    </div>
  </div>;
}

// WorkspaceDirectoryMenu renders a named, permanently available directory without floating controls.
// WorkspaceDirectoryMenu 渲染始终可用的具名目录，不含浮动控件。
// Parameters: accessible label, visible heading and original rows. Returns the navigation landmark.
// 参数：无障碍名称、可见标题与原始菜单行。返回导航区域。
export function WorkspaceDirectoryMenu(props: {
  // ariaLabel uniquely names the feature navigation.
  // ariaLabel 唯一命名功能导航。
  ariaLabel: string;
  // heading provides a small section label above the rows.
  // heading 在菜单行上方提供小节标签。
  heading: string;
  // children preserves the feature-owned menu rows.
  // children 保留功能持有的菜单行。
  children: ReactNode;
}) {
  return <nav className="workspace-directory-menu" aria-label={props.ariaLabel}>
    <p className="workspace-directory-heading">{props.heading}</p>
    <div className="workspace-directory-menu-list">{props.children}</div>
  </nav>;
}
