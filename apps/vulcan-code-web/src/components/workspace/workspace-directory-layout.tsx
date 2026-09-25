import { useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { cn } from "@/lib/utils";

// ACTIVE_TAB_ROW_SELECTOR matches the feature-owned rows that mark the current tab.
// ACTIVE_TAB_ROW_SELECTOR 匹配由功能持有、标记当前标签的菜单行。
const ACTIVE_TAB_ROW_SELECTOR = '.workspace-tool-panel-menu-row.active, .vmm-nav-row[aria-current="page"]';

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
  const navigationMode = props.navigationMode ?? "column";
  const navigationRef = useRef<HTMLDivElement>(null);
  const indicatorRef = useRef<HTMLSpanElement>(null);
  useSlidingTabIndicator(navigationRef, indicatorRef, navigationMode === "tabs");
  return <div className="workspace-directory">
    <div className="workspace-directory-layout" data-navigation-mode={navigationMode}>
      <div className="workspace-directory-navigation" ref={navigationRef}>
        {props.navigation}
        {navigationMode === "tabs" ? <span aria-hidden className="workspace-directory-tab-indicator" ref={indicatorRef} /> : null}
      </div>
      <div className={cn("workspace-directory-content", props.contentClassName)}>{props.children}</div>
    </div>
  </div>;
}

// useSlidingTabIndicator keeps one shared underline aligned with the active tab so selection changes slide instead of jumping.
// useSlidingTabIndicator 让共享下划线对齐当前标签，使切换时滑动过去而不是跳变。
// Parameters: the scrolling navigation strip, the indicator element and whether tabs mode is active. Returns nothing.
// 参数：可滚动导航条、指示条元素及是否处于标签模式。无返回值。
function useSlidingTabIndicator(
  navigationRef: RefObject<HTMLDivElement | null>,
  indicatorRef: RefObject<HTMLSpanElement | null>,
  enabled: boolean,
) {
  useLayoutEffect(() => {
    const navigation = navigationRef.current;
    const indicator = indicatorRef.current;
    if (!enabled || !navigation || !indicator) return;
    // observedRow tracks the row whose size changes (font loading, label updates) must re-align the indicator.
    // observedRow 记录其尺寸变化（字体加载、文案更新）需要重新对齐指示条的菜单行。
    let observedRow: Element | null = null;
    let frame = 0;
    const resizeObserver = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => align());
    const align = () => {
      const row = navigation.querySelector(ACTIVE_TAB_ROW_SELECTOR);
      if (row !== observedRow) {
        if (observedRow) resizeObserver?.unobserve(observedRow);
        if (row) resizeObserver?.observe(row);
        observedRow = row;
      }
      if (!row) {
        indicator.dataset.visible = "false";
        return;
      }
      // Offsets include scrollLeft so the indicator stays attached to its tab inside the horizontally scrolling strip.
      // 偏移包含 scrollLeft，使指示条在横向滚动的标签条内始终贴合其标签。
      const navigationRect = navigation.getBoundingClientRect();
      const rowRect = row.getBoundingClientRect();
      indicator.style.setProperty("--tab-indicator-x", `${rowRect.left - navigationRect.left + navigation.scrollLeft}px`);
      indicator.style.setProperty("--tab-indicator-width", `${rowRect.width}px`);
      indicator.dataset.visible = "true";
    };
    align();
    // The first placement must not animate from the origin; motion starts only after the initial position is painted.
    // 首次定位不能从原点滑入；初始位置绘制完成后才启用动画。
    frame = requestAnimationFrame(() => { indicator.dataset.animate = "true"; });
    const mutationObserver = new MutationObserver(() => align());
    mutationObserver.observe(navigation, { subtree: true, childList: true, attributes: true, attributeFilter: ["class", "aria-current"] });
    resizeObserver?.observe(navigation);
    return () => {
      cancelAnimationFrame(frame);
      mutationObserver.disconnect();
      resizeObserver?.disconnect();
      delete indicator.dataset.animate;
    };
  }, [enabled, indicatorRef, navigationRef]);
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
