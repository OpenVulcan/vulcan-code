import React from "react";
import { SidebarTrigger } from "@/ui-kit/shell/sidebar";

// StageHead renders the title strip above the main stage: the panel toggle, the page or conversation title
// with its menu, page actions, and the app chrome (app menu and desktop window controls) at the right edge.
// Empty title space is the window drag region; double-clicking it maximizes or restores the window.
// StageHead 渲染主舞台上方的标题条：面板开关、页面或会话标题及其菜单、页面操作，以及右端的应用 chrome
// （应用菜单与桌面窗口控制）。标题空白处是窗口拖动区域，双击可最大化或还原窗口。
export function StageHead(props: {
  // Visible title of the page or conversation.
  // 页面或会话的可见标题。
  title: string;
  // Optional crumb placed before the title, such as the owning project.
  // 标题前的可选层级入口，例如所属项目。
  titlePrefix?: React.ReactNode;
  // Optional one-line detail shown beside the title.
  // 显示在标题旁的可选单行详情。
  detail?: string;
  // Optional menu placed right after the title.
  // 紧跟在标题后的可选菜单。
  titleMenu?: React.ReactNode;
  // Page or conversation actions before the app chrome.
  // 位于应用 chrome 之前的页面或会话操作。
  actions?: React.ReactNode;
  // App menu and window controls rendered at the far right.
  // 渲染在最右端的应用菜单与窗口控制。
  chrome?: React.ReactNode;
  // Whether empty title space should be a native drag region.
  // 标题空白处是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title double-click maximize or restore where native dragging is off.
  // 在原生拖动关闭时处理标题双击最大化或还原。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Records a manual panel toggle.
  // 记录一次手动面板切换。
  onToggleSidebar?: () => void;
}) {
  // dragProps marks one element as window drag space.
  // dragProps 将一个元素标记为窗口拖动空间。
  const dragProps = {
    "data-tauri-drag-region": props.titlebarDragRegion ? "" : undefined,
    onDoubleClick: props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick,
  };
  return (
    <header className="vk-stage-head">
      <SidebarTrigger onClick={props.onToggleSidebar} />
      <div className="vk-stage-title">
        {props.titlePrefix}
        <div className="vk-stage-name" {...dragProps}>
          <h1>{props.title}</h1>
          {props.detail ? <small>{props.detail}</small> : null}
        </div>
        {props.titleMenu}
        <span aria-hidden="true" className="vk-stage-drag" {...dragProps} />
      </div>
      {props.actions ? <div className="vk-stage-actions">{props.actions}</div> : null}
      {props.chrome}
    </header>
  );
}
