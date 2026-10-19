import * as React from "react";
import { composeHandlers, cx, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { useRovingGroup } from "../behavior/roving-group";

// TabsContextValue shares the active value and id base with the list, triggers and panels.
// TabsContextValue 与标签列表、触发器和面板共享当前值与 id 前缀。
interface TabsContextValue {
  value: string;
  baseId: string;
  orientation: "horizontal" | "vertical";
  select: (value: string) => void;
}

const TabsContext = React.createContext<TabsContextValue | null>(null);

// useTabs reads the enclosing tabs root.
// useTabs 读取外层标签根组件。
function useTabs(): TabsContextValue {
  const context = React.useContext(TabsContext);
  if (!context) throw new Error("Tabs parts must be rendered inside Tabs.");
  return context;
}

// TabsProps follow the Radix tabs root.
// TabsProps 沿用 Radix 标签根组件。
export interface TabsProps extends Omit<React.ComponentProps<"div">, "defaultValue" | "dir"> {
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  orientation?: "horizontal" | "vertical";
  dir?: "ltr" | "rtl";
  activationMode?: "automatic" | "manual";
}

// Tabs renders the root that switches between panels.
// Tabs 渲染在面板之间切换的根组件。
export function Tabs({ value: controlled, defaultValue = "", onValueChange, orientation = "horizontal", dir, activationMode, className, ...props }: TabsProps) {
  const [value, setValue] = useControllableState(controlled, defaultValue, onValueChange);
  const baseId = useStableId("vk-tabs");
  const context = React.useMemo(() => ({ value, baseId, orientation, select: setValue }), [value, baseId, orientation, setValue]);
  return (
    <TabsContext.Provider value={context}>
      <div data-slot="tabs" data-orientation={orientation} dir={dir} className={cx("vk-tabs-root", className)} {...props} />
    </TabsContext.Provider>
  );
}

// TabsList renders the tab strip; "line" underlines the active tab, "default" uses a segmented track.
// TabsList 渲染标签条；"line" 为当前标签加下划线，"default" 使用分段轨道。
export function TabsList({ className, variant = "default", ref, onFocus, onKeyDown, ...props }: React.ComponentProps<"div"> & { variant?: "default" | "line" | null }) {
  const tabs = useTabs();
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref);
  const roving = useRovingGroup(element, '[role="tab"]', tabs.orientation, {
    // Automatic activation: moving focus with arrows selects the tab.
    // 自动激活：用方向键移动焦点即选中标签。
    onFocusItem: (item) => {
      const next = item.dataset.value;
      if (next !== undefined) tabs.select(next);
    },
  });
  return (
    <div
      ref={composed}
      role="tablist"
      aria-orientation={tabs.orientation}
      data-slot="tabs-list"
      data-variant={variant ?? "default"}
      data-vk-roving=""
      tabIndex={roving.tabIndex}
      onFocus={composeHandlers(onFocus, roving.onFocus)}
      onKeyDown={composeHandlers(onKeyDown, roving.onKeyDown)}
      className={cx("vk-tabs", className)}
      {...props}
    />
  );
}

// TabsTrigger is one tab; it activates on press, click and Enter or Space.
// TabsTrigger 是一个标签；在按下、点击以及 Enter 或 Space 时激活。
export function TabsTrigger({ value, disabled, className, onMouseDown, onClick, onKeyDown, ...props }: Omit<React.ComponentProps<"button">, "value"> & { value: string }) {
  const tabs = useTabs();
  const selected = tabs.value === value;
  return (
    <button
      type="button"
      role="tab"
      id={`${tabs.baseId}-trigger-${value}`}
      aria-controls={`${tabs.baseId}-content-${value}`}
      aria-selected={selected}
      data-state={selected ? "active" : "inactive"}
      data-disabled={disabled ? "" : undefined}
      data-value={value}
      data-slot="tabs-trigger"
      disabled={disabled}
      className={cx("vk-tab", className)}
      onMouseDown={composeHandlers(onMouseDown, (event) => {
        if (event.button === 0 && !event.ctrlKey) tabs.select(value);
      })}
      onClick={composeHandlers(onClick, () => tabs.select(value))}
      onKeyDown={composeHandlers(onKeyDown, (event) => {
        if (event.key === "Enter" || event.key === " ") tabs.select(value);
      })}
      {...props}
    />
  );
}

// TabsContent is the panel of one tab. An inactive panel stays in the document as an empty hidden element so
// the tab's aria-controls always resolves; forceMount keeps its children mounted too.
// TabsContent 是一个标签的面板。非当前面板以空的隐藏元素留在文档中，使标签的 aria-controls 始终可解析；
// forceMount 同时保留其子内容。
export function TabsContent({ value, forceMount, className, children, ...props }: React.ComponentProps<"div"> & { value: string; forceMount?: boolean }) {
  const tabs = useTabs();
  const selected = tabs.value === value;
  return (
    <div
      role="tabpanel"
      id={`${tabs.baseId}-content-${value}`}
      aria-labelledby={`${tabs.baseId}-trigger-${value}`}
      data-state={selected ? "active" : "inactive"}
      data-slot="tabs-content"
      hidden={!selected}
      tabIndex={0}
      className={cx("vk-tab-panel", className)}
      {...props}
    >
      {selected || forceMount ? children : null}
    </div>
  );
}

// tabsListVariants returns the kit classes for a tab strip (kept for callers styling custom lists).
// tabsListVariants 返回标签条的 kit 类名（为自定义列表的调用方保留）。
export function tabsListVariants(_options: { variant?: "default" | "line" | null } = {}): string {
  return "vk-tabs";
}
