import { useLayoutEffect, useRef, type CSSProperties, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { cx } from "@/ui-kit/behavior/core";
import { moveInList } from "@/ui-kit/behavior/roving";

// ACTIVE_TAB_ROW_SELECTOR matches the directory row that marks the current tab.
// ACTIVE_TAB_ROW_SELECTOR 匹配标记当前标签的目录行。
const ACTIVE_TAB_ROW_SELECTOR = '.vk-dir-row[aria-current="page"]';

// TAB_ROW_SELECTOR matches the tab strip entries that Left and Right step through.
// TAB_ROW_SELECTOR 匹配左右键逐项经过的标签条条目。
const TAB_ROW_SELECTOR = ".vk-dir-row:not(:disabled)";

// TAB_STRIP_KEYS maps the strip's horizontal keys onto list movement.
// TAB_STRIP_KEYS 将标签条的横向按键映射为列表移动。
const TAB_STRIP_KEYS: Record<string, string> = { ArrowRight: "ArrowDown", ArrowLeft: "ArrowUp", Home: "Home", End: "End" };

// moveInTabStrip lets Left, Right, Home and End move focus between tabs; Enter or Space then opens the focused tab.
// moveInTabStrip 让左右键、Home 与 End 在标签之间移动焦点；随后按回车或空格打开该标签。
function moveInTabStrip(event: KeyboardEvent<HTMLDivElement>): void {
  const key = TAB_STRIP_KEYS[event.key];
  if (!key || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  if (!(event.target instanceof HTMLElement) || !event.target.matches(TAB_ROW_SELECTOR)) return;
  moveInList({ key, preventDefault: () => event.preventDefault() }, event.currentTarget, TAB_ROW_SELECTOR);
}

// WorkspaceDirectoryLayoutProps separates persistent navigation from the flexible content pane.
// WorkspaceDirectoryLayoutProps 将固定导航与弹性内容面板分离。
interface WorkspaceDirectoryLayoutProps {
  // navigation contains the existing capability-filtered menu and its exact selection callbacks;
  // omit it when the menu lives in the navigation panel beside the rail.
  // navigation 包含现有能力过滤菜单及其精确选择回调；菜单位于轨道旁导航面板时省略。
  navigation?: ReactNode;
  // children is the unrestricted page body.
  // children 是不限宽的页面正文。
  children: ReactNode;
  // contentClassName preserves page-specific container queries and presentation.
  // contentClassName 保留页面专属容器查询与呈现。
  contentClassName?: string;
  // navigationMode renders short menus as a horizontal tab strip instead of a side column; defaults to column.
  // navigationMode 将较短的菜单渲染为水平标签条而非侧栏，默认 column。
  navigationMode?: "column" | "tabs";
  // pageKey names the page shown in the body; a new key starts the body scrolled to the top.
  // pageKey 标识正文显示的页面；键变化时正文从顶部开始滚动。
  pageKey?: string;
}

// WorkspaceDirectoryLayout renders fixed desktop navigation and a bounded top menu on narrow panes.
// WorkspaceDirectoryLayout 在桌面渲染固定导航，在窄面板渲染限高顶部菜单。
// Parameters: navigation, children and optional content styling. Returns the two-pane page shell.
// 参数：导航、正文及可选正文样式。返回双面板页面外壳。
export function WorkspaceDirectoryLayout(props: WorkspaceDirectoryLayoutProps) {
  const navigationMode = props.navigation === undefined ? "none" : props.navigationMode ?? "column";
  const navigationRef = useRef<HTMLDivElement>(null);
  const indicatorRef = useRef<HTMLSpanElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => { if (bodyRef.current) bodyRef.current.scrollTop = 0; }, [props.pageKey]);
  useSlidingTabIndicator(navigationRef, indicatorRef, navigationMode === "tabs");
  useTabStripOverflow(navigationRef, navigationMode === "tabs");
  return <div className="vk-directory">
    <div className="vk-directory-grid" data-navigation-mode={navigationMode}>
      {navigationMode === "none" ? null : (
        <div className={cx("vk-directory-nav", navigationMode === "tabs" && "vk-tabstrip")} onKeyDown={navigationMode === "tabs" ? moveInTabStrip : undefined} ref={navigationRef}>
          {props.navigation}
          {navigationMode === "tabs" ? <span aria-hidden className="vk-tabstrip-ink" ref={indicatorRef} /> : null}
        </div>
      )}
      <div className={cx("vk-directory-body", props.contentClassName)} ref={bodyRef}>{props.children}</div>
    </div>
  </div>;
}

// useTabStripOverflow makes a strip wider than its pane discoverable: edge flags drive a fade where tabs are hidden,
// a vertical wheel scrolls the strip sideways, and the active tab is scrolled into view when it changes.
// useTabStripOverflow 让宽于面板的标签条可被发现：边缘标记驱动被遮住一侧的渐隐，竖向滚轮横向滚动标签条，
// 当前标签变化时滚动到可见范围。
// Parameters: the scrolling strip and whether tabs mode is active. Returns nothing.
// 参数：可滚动标签条及是否处于标签模式。无返回值。
function useTabStripOverflow(stripRef: RefObject<HTMLDivElement | null>, enabled: boolean) {
  useLayoutEffect(() => {
    const strip = stripRef.current;
    if (!enabled || !strip) return;
    let activeRow: Element | null = null;
    const update = () => {
      const row = strip.querySelector<HTMLElement>(ACTIVE_TAB_ROW_SELECTOR);
      if (row && row !== activeRow) {
        const stripRect = strip.getBoundingClientRect();
        const rowRect = row.getBoundingClientRect();
        if (rowRect.left < stripRect.left) strip.scrollLeft += rowRect.left - stripRect.left;
        else if (rowRect.right > stripRect.right) strip.scrollLeft += rowRect.right - stripRect.right;
      }
      activeRow = row;
      const overflowStart = strip.scrollLeft > 1;
      const overflowEnd = strip.scrollLeft + strip.clientWidth < strip.scrollWidth - 1;
      strip.toggleAttribute("data-overflow-start", overflowStart);
      strip.toggleAttribute("data-overflow-end", overflowEnd);
    };
    const wheel = (event: WheelEvent) => {
      if (strip.scrollWidth <= strip.clientWidth || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      event.preventDefault();
      strip.scrollLeft += event.deltaY;
    };
    update();
    strip.addEventListener("scroll", update, { passive: true });
    strip.addEventListener("wheel", wheel, { passive: false });
    const resizeObserver = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    resizeObserver?.observe(strip);
    const mutationObserver = new MutationObserver(update);
    mutationObserver.observe(strip, { subtree: true, childList: true, attributes: true, attributeFilter: ["aria-current"] });
    return () => {
      strip.removeEventListener("scroll", update);
      strip.removeEventListener("wheel", wheel);
      resizeObserver?.disconnect();
      mutationObserver.disconnect();
    };
  }, [enabled, stripRef]);
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

// PageTabOption describes one tab of a page tab strip.
// PageTabOption 描述页面标签条中的一个标签。
export interface PageTabOption<Value extends string> {
  // value is the stable identity reported on selection.
  // value 是选择时回传的稳定标识。
  value: Value;
  // label is the visible tab text.
  // label 是可见的标签文字。
  label: ReactNode;
  // count is an optional trailing quantity shown after the label.
  // count 是标签文字后可选的数量。
  count?: ReactNode;
  // disabled keeps the tab visible but not selectable.
  // disabled 保留标签可见但不可选择。
  disabled?: boolean;
}

// PageTabs renders the Vulcan page tab strip: underlined tabs, one sliding flame indicator and optional trailing actions.
// It shares every rule with the directory layout's tabs mode, so in-page section switches look like provider management.
// PageTabs 渲染 Vulcan 页面标签条：下划线标签、一条滑动的火焰指示条及可选的尾部操作。
// 它与目录布局的标签模式共用全部规则，使页内分区切换与供应商管理页外观一致。
// Parameters: accessible name, options, the selected value, the selection callback and optional actions. Returns the strip.
// 参数：无障碍名称、选项、当前值、选择回调及可选操作。返回标签条。
export function PageTabs<Value extends string>(props: {
  // ariaLabel names the tab navigation.
  // ariaLabel 命名标签导航。
  ariaLabel: string;
  // options lists the tabs in display order.
  // options 按显示顺序列出标签。
  options: readonly PageTabOption<Value>[];
  // value is the selected tab.
  // value 是当前选中的标签。
  value: Value;
  // onValueChange receives the newly selected tab.
  // onValueChange 接收新选中的标签。
  onValueChange: (value: Value) => void;
  // actions renders page commands at the trailing end of the strip.
  // actions 在标签条尾部渲染页面命令。
  actions?: ReactNode;
  // className styles the outer strip container.
  // className 设置外层标签条容器样式。
  className?: string;
}) {
  const stripRef = useRef<HTMLDivElement>(null);
  const indicatorRef = useRef<HTMLSpanElement>(null);
  useSlidingTabIndicator(stripRef, indicatorRef, true);
  useTabStripOverflow(stripRef, true);
  return <div className={cx("vk-page-tabs", props.className)}>
    <div className="vk-tabstrip" onKeyDown={moveInTabStrip} ref={stripRef}>
      <nav aria-label={props.ariaLabel} className="vk-dir-list">
        {props.options.map((option) => <DirectoryRow
          active={option.value === props.value}
          count={option.count}
          disabled={option.disabled}
          key={option.value}
          label={option.label}
          onSelect={() => props.onValueChange(option.value)}
        />)}
      </nav>
      <span aria-hidden className="vk-tabstrip-ink" ref={indicatorRef} />
    </div>
    {props.actions ? <div className="vk-page-actions">{props.actions}</div> : null}
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
  return <nav className="vk-dir" aria-label={props.ariaLabel}>
    <p className="vk-dir-head vk-label">{props.heading}</p>
    <div className="vk-dir-list">{props.children}</div>
  </nav>;
}

// DirectoryRowProps describes one selectable entry of a directory menu or tab strip.
// DirectoryRowProps 描述目录菜单或标签条中的一个可选条目。
export interface DirectoryRowProps {
  // active marks the entry whose page is shown; it drives the ember bar and the sliding tab indicator.
  // active 标记当前显示其页面的条目；它驱动余烬色竖条与滑动标签指示条。
  active: boolean;
  // label is the visible entry name.
  // label 是可见的条目名称。
  label: ReactNode;
  // icon is an optional leading glyph.
  // icon 是可选的前置图标。
  icon?: ReactNode;
  // swatch is an optional emoji badge tinted with a category colour, replacing the icon.
  // swatch 是可选的表情徽标，以分类颜色着色，替代图标。
  swatch?: { glyph: ReactNode; color?: string };
  // count is an optional trailing quantity.
  // count 是可选的尾部数量。
  count?: ReactNode;
  // title is the optional hover description.
  // title 是可选的悬停说明。
  title?: string;
  // disabled keeps the entry visible but not selectable.
  // disabled 保留条目可见但不可选择。
  disabled?: boolean;
  // onSelect receives the click on this entry.
  // onSelect 接收此条目的点击。
  onSelect: () => void;
}

// DirectoryRow renders one directory entry; every feature menu and page tab strip shares this row.
// DirectoryRow 渲染一个目录条目；所有功能菜单与页面标签条共用此行。
export function DirectoryRow(props: DirectoryRowProps) {
  return <button
    aria-current={props.active ? "page" : undefined}
    className="vk-dir-row"
    disabled={props.disabled}
    onClick={props.onSelect}
    title={props.title}
    type="button"
  >
    {props.swatch
      ? <i aria-hidden className="vk-dir-swatch" style={{ "--vk-dir-swatch": props.swatch.color } as CSSProperties}>{props.swatch.glyph}</i>
      : props.icon}
    <span>{props.label}</span>
    {props.count === undefined || props.count === null ? null : <small>{props.count}</small>}
  </button>;
}
