import type React from "react";
import type { AdaptiveToolPanelSide } from "@/components/workspace/adaptive-tool-panel";

// WorkspacePagePanelDescriptor describes one registered adaptive side panel for a workspace page.
// WorkspacePagePanelDescriptor 描述工作区页面注册的单个自适应侧边面板。
export interface WorkspacePagePanelDescriptor {
  // Preferred side where the panel should float or reserve inline space.
  // 面板浮动或内联占位时优先使用的侧边。
  side: AdaptiveToolPanelSide;
  // Whether the panel content is expanded.
  // 面板内容当前是否展开。
  expanded: boolean;
  // Fixed expanded panel width used by shared reserve-space math.
  // 共享占位计算使用的展开面板固定宽度。
  panelWidth?: number;
  // Minimum main content width required before reserving panel space.
  // 面板占位前主内容需要保留的最小宽度。
  minContentWidth?: number;
  // Extra width guard that prevents tight edge overlap.
  // 防止贴边重叠的额外宽度保护值。
  reserveGap?: number;
  // Gap between the reserved panel and centered content.
  // 内联面板与居中内容之间的间距。
  inlineGap?: number;
  // Additional root class name for page-specific layout hooks.
  // 页面专用布局钩子使用的附加根类名。
  className?: string;
  // Additional content shell class name for scroll and padding rules.
  // 滚动与留白规则使用的附加内容外壳类名。
  contentShellClassName?: string;
  // Additional content class name for page-specific inner sizing.
  // 页面专用内部尺寸规则使用的附加内容类名。
  contentClassName?: string;
  // Rendered panel body, including its own expanded and collapsed controls.
  // 渲染的面板主体，包含自身展开与折叠控件。
  panel: React.ReactNode;
}

// WorkspacePageDescriptor describes the public chrome requested by one workspace page.
// WorkspacePageDescriptor 描述单个工作区页面请求的公共外壳能力。
export interface WorkspacePageDescriptor {
  // Optional title that replaces the active conversation title.
  // 替代当前会话标题的可选标题。
  title?: string;
  // Optional secondary title text rendered inline after the title.
  // 标题后方横向渲染的可选二级信息。
  detail?: string;
  // Optional page-level header actions rendered before shell actions.
  // 在外壳动作前渲染的可选页面级标题按钮。
  headerActions?: React.ReactNode;
  // Optional right-edge header actions registered by the current page.
  // 当前页面注册的可选标题栏右侧动作。
  headerRightActions?: React.ReactNode;
  // Optional registered adaptive side panels for the page.
  // 页面注册的可选自适应侧边面板列表。
  toolPanels?: WorkspacePagePanelDescriptor[];
  // Main content rendered inside the shared workspace body.
  // 渲染在共享工作区主体内的主内容。
  content: React.ReactNode;
}
