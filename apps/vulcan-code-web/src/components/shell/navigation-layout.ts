import type * as React from "react";

// ACTIVITY_RAIL_WIDTH_PX is the single width source for the rail and the workspace layout calculations.
// ACTIVITY_RAIL_WIDTH_PX 是轨道宽度及工作区布局计算的唯一来源。
export const ACTIVITY_RAIL_WIDTH_PX = 52;

// NAVIGATION_PANEL_MAX_WIDTH_PX is the widest any navigation panel (sessions, feature menus, settings) grows beside the rail.
// NAVIGATION_PANEL_MAX_WIDTH_PX 是任一导航面板（会话、功能菜单、设置）在轨道旁的最大宽度。
export const NAVIGATION_PANEL_MAX_WIDTH_PX = 248;

// navigationShellStyle publishes the panel width limit to the stylesheet as a CSS variable.
// navigationShellStyle 将面板宽度上限以 CSS 变量形式提供给样式表。
// Takes no arguments; returns the inline style for a sidebar provider root.
// 无参数；返回侧栏 Provider 根节点的内联样式。
export function navigationShellStyle(): React.CSSProperties {
  return { "--console-sidebar-max-width": `${NAVIGATION_PANEL_MAX_WIDTH_PX}px` } as React.CSSProperties;
}

// activityRailStyle publishes the rail width on the rail itself, so it also holds inside the portaled phone drawer.
// activityRailStyle 在轨道自身上提供宽度变量，使其在传送到 body 的手机抽屉中同样生效。
// Takes no arguments; returns the inline style for the rail root.
// 无参数；返回轨道根节点的内联样式。
export function activityRailStyle(): React.CSSProperties {
  return { "--activity-rail-width": `${ACTIVITY_RAIL_WIDTH_PX}px` } as React.CSSProperties;
}

// navigationColumnWidth estimates the left navigation footprint used by width-based layout decisions.
// navigationColumnWidth 估算左侧导航占用宽度，供基于宽度的布局判断使用。
// isMobile hides the column behind a drawer; panelOpen adds the panel beside the rail. Returns pixels.
// isMobile 表示导航收入抽屉；panelOpen 表示轨道旁面板展开。返回像素值。
export function navigationColumnWidth(isMobile: boolean, panelOpen: boolean): number {
  if (isMobile) {
    return 0;
  }
  return ACTIVITY_RAIL_WIDTH_PX + (panelOpen ? NAVIGATION_PANEL_MAX_WIDTH_PX : 0);
}
