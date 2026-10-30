import React from "react";
import { conversationReservedWidth, navigationColumnWidth } from "@/components/shell/navigation-layout";
import { useAppearance } from "@/ui-kit/appearance";
import type { useSidebar } from "@/ui-kit/shell/sidebar";
import {
  WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH,
  WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH,
} from "@/components/workspace/workspace-resizable-body";

// WorkspacePanelLayoutOptions supplies the sidebar controller and the chat surface that may host the context panel.
// WorkspacePanelLayoutOptions 提供侧栏控制器以及可承载上下文面板的会话界面。
export interface WorkspacePanelLayoutOptions {
  // Left navigation controller that width pressure may collapse and restore.
  // 宽度压力可能折叠与恢复的左侧导航控制器。
  sidebar: ReturnType<typeof useSidebar>;
  // True only for an existing persisted conversation surface; the context panel is hidden otherwise.
  // 仅在当前界面是已存在的持久化会话时为真；否则隐藏上下文面板。
  hasSelectedChatConversation: boolean;
  // True while the chat view is shown. A draft there closes the panel; other pages only hide it, so it returns with the chat.
  // 显示会话视图时为真。其中的草稿会关闭面板；其他页面只隐藏面板，返回会话时恢复。
  chatViewActive: boolean;
  // Whether the context panel is open; the caller holds it so it outlives this shell.
  // 上下文面板是否展开；由调用方保存，使其比本外壳存活更久。
  rightPanelOpen: boolean;
  // Updates the context panel open state.
  // 更新上下文面板展开状态。
  setRightPanelOpen: (open: boolean) => void;
  // Lets the active chat preserve scroll before shell geometry changes.
  // 让当前会话在外壳几何变化前保留滚动位置。
  notifyConversationLayoutWillChange: () => void;
}

// WorkspacePanelLayout exposes the measured shell, the context panel mode and the manual panel toggles.
// WorkspacePanelLayout 暴露被测量的外壳、上下文面板模式与手动面板切换。
export interface WorkspacePanelLayout {
  // Shell element measured for automatic panel collapse.
  // 用于自动折叠面板而测量的外壳元素。
  shellRef: React.RefObject<HTMLDivElement | null>;
  // Whether the right panel overlays instead of resizing the chat area.
  // 右侧面板是否以浮动方式覆盖而不是压缩会话区。
  rightPanelFloating: boolean;
  // Whether the right context panel is visible on the current surface.
  // 右侧上下文面板在当前界面是否可见。
  effectiveRightPanelOpen: boolean;
  // Opens the context panel, floating it when the docked panel would not fit.
  // 打开上下文面板，停靠放不下时改为浮动。
  openRightContextPanel: () => void;
  // Closes the context panel and clears its floating mode.
  // 关闭上下文面板并清除其浮动模式。
  closeRightContextPanel: () => void;
  // Records a manual left sidebar toggle.
  // 记录一次手动切换左侧栏。
  handleLeftPanelToggle: () => void;
  // Toggles the right context panel manually.
  // 手动切换右侧上下文面板。
  handleRightPanelToggle: () => void;
}

// useWorkspacePanelLayout owns the left and right panel visibility driven by user toggles and shell width.
// useWorkspacePanelLayout 持有由用户切换与外壳宽度共同驱动的左右面板可见性。
// Parameters: options supplies the sidebar controller, chat surface flag and layout-change notifier.
// 参数：options 提供侧栏控制器、会话界面标记与布局变化通知器。
// Returns: the shell ref to attach, the context panel mode and the panel toggles.
// 返回值：需挂载的外壳 ref、上下文面板模式与面板切换。
export function useWorkspacePanelLayout(options: WorkspacePanelLayoutOptions): WorkspacePanelLayout {
  const {
    sidebar,
    hasSelectedChatConversation,
    chatViewActive,
    rightPanelOpen,
    setRightPanelOpen,
    notifyConversationLayoutWillChange,
  } = options;
  // shellRef measures the available workspace width for automatic panel collapse.
  // shellRef 测量工作台可用宽度，用于自动折叠面板。
  const shellRef = React.useRef<HTMLDivElement | null>(null);
  // appearance re-evaluates the panel modes when the reading width or text tier moves the reserve.
  // appearance 在阅读宽度或文字档位改变预留宽度时重新判断面板模式。
  const appearance = useAppearance();
  // previousShellWidthRef tracks resize direction so auto-restore only runs while width grows.
  // previousShellWidthRef 跟踪尺寸变化方向，确保自动恢复只在宽度增长时执行。
  const previousShellWidthRef = React.useRef<number | null>(null);
  // rightPanelFloating stores whether the right panel overlays instead of resizing the chat area.
  // rightPanelFloating 保存右侧面板是否以浮动方式覆盖而不是压缩会话区。
  const [rightPanelFloating, setRightPanelFloating] = React.useState(false);
  // leftAutoCollapsed records automatic left-sidebar collapse caused by width pressure.
  // leftAutoCollapsed 记录宽度压力导致的左侧栏自动折叠。
  const [leftAutoCollapsed, setLeftAutoCollapsed] = React.useState(false);
  // effectiveRightPanelOpen suppresses stale right-panel state while a draft conversation is selected.
  // effectiveRightPanelOpen 在选中新建草稿会话时屏蔽残留的右侧面板状态。
  const effectiveRightPanelOpen = hasSelectedChatConversation && rightPanelOpen;

  // openRightContextPanel opens the context rail and chooses floating mode from the current shell width.
  // openRightContextPanel 打开上下文栏，并根据当前外壳宽度选择浮动模式。
  const openRightContextPanel = React.useCallback(() => {
    notifyConversationLayoutWillChange();
    const width = shellRef.current?.getBoundingClientRect().width ?? 0;
    const leftWidth = navigationColumnWidth(sidebar.isMobile, sidebar.open);
    setRightPanelFloating(width > 0 && width - leftWidth - WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH
      - WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH < conversationReservedWidth());
    setRightPanelOpen(true);
  }, [notifyConversationLayoutWillChange, setRightPanelOpen, sidebar.isMobile, sidebar.open]);

  // closeRightContextPanel is the single close path shared by the manual toggle and the empty-tabs rule.
  // closeRightContextPanel 是手动切换与“页签清空”规则共用的唯一关闭路径。
  const closeRightContextPanel = React.useCallback(() => {
    notifyConversationLayoutWillChange();
    setRightPanelFloating(false);
    setRightPanelOpen(false);
  }, [notifyConversationLayoutWillChange, setRightPanelOpen]);

  // handleLeftPanelToggle clears automatic restore intent when the user toggles the left sidebar manually.
  // handleLeftPanelToggle 在用户手动切换左侧栏时清除自动恢复意图。
  const handleLeftPanelToggle = React.useCallback(() => {
    notifyConversationLayoutWillChange();
    setLeftAutoCollapsed(false);
  }, [notifyConversationLayoutWillChange]);
  // handleRightPanelToggle clears automatic restore intent when the user toggles the right panel manually.
  // handleRightPanelToggle 在用户手动切换右侧面板时清除自动恢复意图。
  const handleRightPanelToggle = React.useCallback(() => {
    if (rightPanelOpen) {
      closeRightContextPanel();
      return;
    }
    openRightContextPanel();
  }, [closeRightContextPanel, openRightContextPanel, rightPanelOpen]);

  React.useEffect(() => {
    if (chatViewActive && !hasSelectedChatConversation && rightPanelOpen) {
      notifyConversationLayoutWillChange();
      setRightPanelOpen(false);
      setRightPanelFloating(false);
    }
  }, [chatViewActive, hasSelectedChatConversation, notifyConversationLayoutWillChange, rightPanelOpen, setRightPanelOpen]);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const previousWidth = previousShellWidthRef.current;
      const isGrowing = previousWidth !== null && width > previousWidth + 1;
      // Only initial layout or a real shrink may override desktop visibility; rerenders must respect a manual reopen.
      // 只有初次布局或实际缩窄可以自动折叠；普通重绘不能覆盖用户手动重新展开的选择。
      const canAutoCollapse = previousWidth === null || width < previousWidth - 1;
      previousShellWidthRef.current = width;
      const leftWidth = navigationColumnWidth(sidebar.isMobile, sidebar.open);
      const reservedWidth = conversationReservedWidth();
      const dockedPanelFits = width - leftWidth - WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH
        - WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH >= reservedWidth;
      const shouldFloatRightPanel = effectiveRightPanelOpen && !dockedPanelFits;
      const dockedRightWidth = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH : 0;
      const shellGaps = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH : 0;
      const mainWidth = width - leftWidth - dockedRightWidth - shellGaps;
      if (effectiveRightPanelOpen) {
        if (rightPanelFloating !== shouldFloatRightPanel) {
          notifyConversationLayoutWillChange();
        }
        setRightPanelFloating(shouldFloatRightPanel);
      }
      // Phone navigation overlays the workspace; desktop auto-collapse must not dismiss it.
      // 手机导航覆盖于工作区上方，桌面自动折叠不能将其关闭。
      if (sidebar.isMobile) {
        return;
      }
      if (canAutoCollapse && mainWidth < reservedWidth && sidebar.open) {
        notifyConversationLayoutWillChange();
        sidebar.setOpen(false);
        setLeftAutoCollapsed(true);
        return;
      }
      if (!isGrowing) {
        return;
      }
      if (leftAutoCollapsed && !sidebar.open) {
        const restoredRightWidth = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH : 0;
        const restoredShellGaps = effectiveRightPanelOpen && !shouldFloatRightPanel ? WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH : 0;
        const restoredMainWidth = width - navigationColumnWidth(false, true) - restoredRightWidth - restoredShellGaps;
        if (restoredMainWidth >= reservedWidth) {
          notifyConversationLayoutWillChange();
          sidebar.setOpen(true);
          setLeftAutoCollapsed(false);
          return;
        }
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [appearance.text, appearance.width, effectiveRightPanelOpen, leftAutoCollapsed, notifyConversationLayoutWillChange, rightPanelFloating, sidebar]);

  return {
    shellRef,
    rightPanelFloating,
    effectiveRightPanelOpen,
    openRightContextPanel,
    closeRightContextPanel,
    handleLeftPanelToggle,
    handleRightPanelToggle,
  };
}
