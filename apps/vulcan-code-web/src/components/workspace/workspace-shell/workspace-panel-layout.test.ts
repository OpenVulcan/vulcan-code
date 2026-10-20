// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { conversationReservedWidth, navigationColumnWidth } from "@/components/shell/navigation-layout";
import type { useSidebar } from "@/ui-kit/shell/sidebar";
import {
  WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH,
  WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH,
} from "@/components/workspace/workspace-resizable-body";
import { useWorkspacePanelLayout, type WorkspacePanelLayout } from "./use-workspace-panel-layout";

// observerCallbacks stores the live ResizeObserver callbacks so tests can publish shell widths.
// observerCallbacks 保存存活的 ResizeObserver 回调，供测试发布外壳宽度。
const observerCallbacks = new Set<ResizeObserverCallback>();

// FakeResizeObserver records callbacks instead of measuring layout, which jsdom does not perform.
// FakeResizeObserver 记录回调而不是测量布局，因为 jsdom 不执行布局。
class FakeResizeObserver {
  private readonly callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
  }

  observe(): void {
    observerCallbacks.add(this.callback);
  }

  disconnect(): void {
    observerCallbacks.delete(this.callback);
  }
}

// root hosts the probe component that runs the hook under test.
// root 承载运行被测 hook 的探针组件。
let root: Root;
// container is the detached DOM node owned by root.
// container 是 root 持有的游离 DOM 节点。
let container: HTMLDivElement;

// SHELL_TOKENS stands in for the kit stylesheet, which jsdom does not load: the layout reads these tokens.
// SHELL_TOKENS 代替 jsdom 不加载的 kit 样式表：布局读取这些令牌。
const SHELL_TOKENS = { "--vk-rail-w": "48px", "--vk-sidebar-w": "264px", "--vk-reserve-w": "760px" };

beforeEach(() => {
  for (const [name, value] of Object.entries(SHELL_TOKENS)) document.documentElement.style.setProperty(name, value);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  container = document.createElement("div");
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  observerCallbacks.clear();
  vi.unstubAllGlobals();
  for (const name of Object.keys(SHELL_TOKENS)) document.documentElement.style.removeProperty(name);
});

// publishShellWidth delivers one measured width to every live observer.
// publishShellWidth 将一次测量宽度发送给所有存活观察者。
function publishShellWidth(width: number): void {
  act(() => {
    for (const callback of [...observerCallbacks]) {
      callback([{ contentRect: { width } } as ResizeObserverEntry], {} as ResizeObserver);
    }
  });
}

// PanelHarness exposes the latest hook result and the sidebar state it drives.
// PanelHarness 暴露最新 hook 结果及其驱动的侧栏状态。
interface PanelHarness {
  // Latest hook result.
  // 最新 hook 结果。
  layout: () => WorkspacePanelLayout;
  // Latest desktop sidebar visibility.
  // 最新桌面侧栏可见性。
  sidebarOpen: () => boolean;
  // Re-renders the hook on another surface, as switching pages or selecting a draft does.
  // 在另一界面上重新渲染 hook，如同切换页面或选择草稿。
  showSurface: (surface: PanelSurface) => void;
}

// PanelSurface is the page the shell shows: a persisted chat, a chat draft, or another page such as settings.
// PanelSurface 是外壳显示的页面：持久会话、会话草稿或设置等其他页面。
type PanelSurface = "chat" | "draft" | "page";

// renderPanelLayout mounts the hook with a stateful desktop sidebar and a measurable shell element.
// renderPanelLayout 使用有状态的桌面侧栏与可测量的外壳元素挂载 hook。
// Parameters: hasSelectedChatConversation selects a persisted chat surface (otherwise another page); shellWidth is the
// bounding width.
// 参数：hasSelectedChatConversation 选择持久会话界面（否则为其他页面）；shellWidth 是外壳边界宽度。
function renderPanelLayout(hasSelectedChatConversation: boolean, shellWidth: number): PanelHarness {
  // latest stores the most recent hook result and sidebar visibility.
  // latest 保存最近一次 hook 结果与侧栏可见性。
  let latest: { layout: WorkspacePanelLayout; sidebarOpen: boolean } | undefined;
  // Probe owns sidebar visibility like SidebarProvider and the context panel flag like the workbench, and attaches
  // the shell ref.
  // Probe 像 SidebarProvider 一样持有侧栏可见性，像工作台一样持有上下文面板标记，并挂载外壳 ref。
  function Probe(props: { surface: PanelSurface }): React.ReactElement {
    const [open, setOpen] = React.useState(true);
    const [rightPanelOpen, setRightPanelOpen] = React.useState(false);
    const sidebar = React.useMemo(
      () => ({ isMobile: false, open, setOpen }) as unknown as ReturnType<typeof useSidebar>,
      [open],
    );
    const layout = useWorkspacePanelLayout({
      sidebar,
      hasSelectedChatConversation: props.surface === "chat",
      chatViewActive: props.surface !== "page",
      rightPanelOpen,
      setRightPanelOpen,
      notifyConversationLayoutWillChange: () => undefined,
    });
    latest = { layout, sidebarOpen: open };
    return React.createElement("div", {
      ref: (element: HTMLDivElement | null) => {
        layout.shellRef.current = element;
        if (element) {
          element.getBoundingClientRect = () => ({ width: shellWidth }) as DOMRect;
        }
      },
    });
  }
  const showSurface = (surface: PanelSurface) => act(() => root.render(React.createElement(Probe, { surface })));
  showSurface(hasSelectedChatConversation ? "chat" : "page");
  return {
    layout: () => latest!.layout,
    sidebarOpen: () => latest!.sidebarOpen,
    showSurface,
  };
}

// openNavigationWidth is the desktop rail plus the expanded navigation panel.
// openNavigationWidth 是桌面轨道加展开导航面板的宽度。
const openNavigationWidth = (): number => navigationColumnWidth(false, true);
// dockedPanelFitWidth is the narrowest shell that docks the context panel beside the open navigation.
// dockedPanelFitWidth 是在导航展开时仍能停靠上下文面板的最窄外壳宽度。
const dockedPanelFitWidth = (): number => openNavigationWidth() + WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH
  + WORKSPACE_CONTEXT_RESIZE_HANDLE_WIDTH + conversationReservedWidth();

// Width pressure collapses the navigation and growth restores only an automatic collapse.
// 宽度压力折叠导航，宽度增长只恢复自动折叠。
it("auto-collapses the navigation on a narrow shell and restores it when the shell grows", () => {
  const harness = renderPanelLayout(true, 0);
  publishShellWidth(openNavigationWidth() + 700);
  expect(harness.sidebarOpen()).toBe(false);
  publishShellWidth(openNavigationWidth() + 900);
  expect(harness.sidebarOpen()).toBe(true);
});

// A manual toggle after an automatic collapse is the user's choice; later growth must not override it.
// 自动折叠后的手动切换是用户选择；之后的宽度增长不得覆盖。
it("keeps a manually toggled navigation closed when the shell grows", () => {
  const harness = renderPanelLayout(true, 0);
  publishShellWidth(openNavigationWidth() + 700);
  expect(harness.sidebarOpen()).toBe(false);
  act(() => harness.layout().handleLeftPanelToggle());
  publishShellWidth(openNavigationWidth() + 900);
  expect(harness.sidebarOpen()).toBe(false);
});

// The context panel docks only when the conversation keeps its reserved width.
// 只有会话保留其预留宽度时上下文面板才停靠。
it("floats the context panel when docking would squeeze the conversation", () => {
  const narrow = renderPanelLayout(true, dockedPanelFitWidth() - 1);
  act(() => narrow.layout().openRightContextPanel());
  expect(narrow.layout().effectiveRightPanelOpen).toBe(true);
  expect(narrow.layout().rightPanelFloating).toBe(true);
  act(() => narrow.layout().handleRightPanelToggle());
  expect(narrow.layout().effectiveRightPanelOpen).toBe(false);
  expect(narrow.layout().rightPanelFloating).toBe(false);

  const wide = renderPanelLayout(true, dockedPanelFitWidth());
  act(() => wide.layout().openRightContextPanel());
  expect(wide.layout().rightPanelFloating).toBe(false);
});

// Draft and feature pages never show a context panel opened by another surface.
// 草稿与功能页面绝不显示由其他界面打开的上下文面板。
it("keeps the context panel closed without a persisted conversation", () => {
  const harness = renderPanelLayout(false, dockedPanelFitWidth());
  act(() => harness.layout().openRightContextPanel());
  expect(harness.layout().effectiveRightPanelOpen).toBe(false);
});

// Another page only hides the panel, so it is back on return; a chat draft closes it.
// 其他页面只隐藏面板，返回时恢复；会话草稿则关闭面板。
it("restores the context panel after another page but not after a draft", () => {
  const harness = renderPanelLayout(true, dockedPanelFitWidth());
  act(() => harness.layout().openRightContextPanel());

  harness.showSurface("page");
  expect(harness.layout().effectiveRightPanelOpen).toBe(false);
  harness.showSurface("chat");
  expect(harness.layout().effectiveRightPanelOpen).toBe(true);

  harness.showSurface("draft");
  harness.showSurface("chat");
  expect(harness.layout().effectiveRightPanelOpen).toBe(false);
});
