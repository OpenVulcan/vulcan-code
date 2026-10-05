import * as React from "react";
import { X } from "lucide-react";
import { IconButton } from "@/components/ui/button";
import { formatMessage, useI18n } from "@/i18n";
import type { PluginDiagnosticsSelection } from "@/types/plugin-resource-diagnostics";
import {
  CapacityMaintenanceSection, RecoveryMaintenanceSection, UsageMaintenanceSection,
  type PluginMaintenanceContext, type PluginMaintenanceSectionId,
} from "./plugin-maintenance-workspace";

// ALL_PLUGINS_SCOPE keys the unscoped view that follows retained uninstalls of every plugin.
// ALL_PLUGINS_SCOPE 是跟踪所有插件保留卸载的不限定视图键。
export const ALL_PLUGINS_SCOPE = "";

// MaintenanceScopeView is the navigation state of one kept-alive scope.
// MaintenanceScopeView 是单个保持存活范围的导航状态。
interface MaintenanceScopeView {
  // section is the block the drawer scrolls to when the scope is opened.
  // section 是打开范围时抽屉滚动到的区块。
  section: PluginMaintenanceSectionId;
  // selection is the read-only usage target; null shows the plugin itself in a plugin scope.
  // selection 是只读使用情况目标；插件范围中为 null 时显示插件本身。
  selection: PluginDiagnosticsSelection | null;
  // focus increments on every explicit open so the same section can be scrolled to again.
  // focus 在每次明确打开时递增，使同一区块可再次滚动定位。
  focus: number;
}

// PluginMaintenanceDrawerState lists every scope opened so far and the one currently shown.
// PluginMaintenanceDrawerState 列出迄今打开的所有范围及当前显示者。
export interface PluginMaintenanceDrawerState {
  // active is the visible scope, or null while the drawer is closed.
  // active 是可见范围，抽屉关闭时为 null。
  active: string | null;
  // views keeps every opened scope mounted so unconfirmed local state survives closing the drawer.
  // views 保持所有已打开范围挂载，使未确认本地状态在关闭抽屉后保留。
  views: ReadonlyMap<string, MaintenanceScopeView>;
}

// usePluginMaintenanceDrawer owns drawer navigation; the unscoped view is mounted from the start.
// usePluginMaintenanceDrawer 持有抽屉导航；不限定视图自始挂载。
// Returns: the state plus open/close/update operations that never discard a mounted scope.
// 返回值：状态以及打开、关闭、更新操作，均不丢弃已挂载范围。
export function usePluginMaintenanceDrawer() {
  const [state, setState] = React.useState<PluginMaintenanceDrawerState>(() => ({
    active: null, views: new Map([[ALL_PLUGINS_SCOPE, { section: "recovery", selection: null, focus: 0 }]]),
  }));
  // open shows one scope at one section; selection replaces the usage target only when explicitly given.
  // open 在指定区块显示单个范围；仅明确给出时 selection 才替换使用情况目标。
  const open = React.useCallback((scope: string, section: PluginMaintenanceSectionId, selection?: PluginDiagnosticsSelection) => {
    setState((current) => {
      const previous = current.views.get(scope);
      const views = new Map(current.views);
      views.set(scope, { section, selection: selection ?? previous?.selection ?? null, focus: (previous?.focus ?? 0) + 1 });
      return { active: scope, views };
    });
  }, []);
  // update records navigation made inside one scope without scrolling.
  // update 记录在单个范围内进行的导航，不触发滚动。
  const update = React.useCallback((scope: string, patch: Partial<Pick<MaintenanceScopeView, "selection">>) => {
    setState((current) => {
      const previous = current.views.get(scope);
      if (previous === undefined) return current;
      const views = new Map(current.views);
      views.set(scope, { ...previous, ...patch });
      return { ...current, views };
    });
  }, []);
  // close hides the drawer while every scope stays mounted.
  // close 隐藏抽屉，所有范围保持挂载。
  const close = React.useCallback(() => setState((current) => current.active === null ? current : { ...current, active: null }), []);
  return { state, open, update, close };
}

type DrawerController = ReturnType<typeof usePluginMaintenanceDrawer>;

// MaintenanceScope renders the stacked sections of one scope and scrolls to the requested one.
// MaintenanceScope 渲染单个范围的堆叠区块，并滚动到请求的区块。
function MaintenanceScope(props: PluginMaintenanceContext & { scope: string; view: MaintenanceScopeView; active: boolean; drawer: DrawerController }) {
  const { scope, view, active, drawer, ...shared } = props;
  const container = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    if (!active || view.focus === 0) return;
    // scrollIntoView is optional in non-browser renderers; absence simply leaves the scroll position unchanged.
    // scrollIntoView 在非浏览器渲染器中可能缺失；缺失时仅保持滚动位置不变。
    container.current?.querySelector<HTMLElement>(`[data-anchor="${view.section}"]`)?.scrollIntoView?.({ block: "start" });
  }, [active, view.focus, view.section]);
  const locate = (kind: "update" | "uninstall", pluginId: string, requestId: string) => drawer.open(scope, "usage", { kind, pluginId, requestId });
  if (scope === ALL_PLUGINS_SCOPE) {
    return <div className="plugin-maintenance-scope" ref={container}>
      <RecoveryMaintenanceSection {...shared} compact kinds={["uninstall"]} onLocate={locate} uninstallOnly />
      {view.selection !== null && <UsageMaintenanceSection compact conversations={shared.conversations} onClose={() => drawer.update(scope, { selection: null })}
        onSelect={(selection) => drawer.update(scope, { selection })} onSelectPluginResourceOwner={shared.onSelectPluginResourceOwner}
        pluginName={shared.pluginName} selection={view.selection} />}
    </div>;
  }
  const plugin = shared.plugins.find((candidate) => candidate.id === scope);
  return <div className="plugin-maintenance-scope" ref={container}>
    <RecoveryMaintenanceSection {...shared} compact kinds={["update", "installation", "credential", "uninstall"]} onLocate={locate} scopePluginId={scope}
      uninstallUncertainty={shared.uninstallUncertainty?.pluginId === scope ? shared.uninstallUncertainty : null} />
    <UsageMaintenanceSection compact conversations={shared.conversations} onSelect={(selection) => drawer.update(scope, { selection })}
      onSelectPluginResourceOwner={shared.onSelectPluginResourceOwner} pluginName={shared.pluginName}
      selection={view.selection ?? { kind: "plugin", pluginId: scope }} />
    {plugin?.extension?.hasLuaRuntime === true && <CapacityMaintenanceSection compact onRefreshCatalog={shared.actions.onRefreshLuaSkills}
      pluginId={scope} plugins={shared.plugins} />}
  </div>;
}

// PluginMaintenanceDrawer renders a non-modal right side panel hosting one kept-alive view per scope.
// PluginMaintenanceDrawer 渲染非模态右侧面板，为每个范围托管一个保持存活的视图。
// It is hidden rather than unmounted when closed, so pending confirmations and receipts are never lost.
// 关闭时隐藏而非卸载，因此待确认操作与回执不会丢失。
export function PluginMaintenanceDrawer(props: PluginMaintenanceContext & { drawer: DrawerController }) {
  const { t } = useI18n();
  const { drawer, ...shared } = props;
  const active = drawer.state.active;
  const titleId = React.useId();
  const panel = React.useRef<HTMLElement>(null);
  // returnFocus restores the control that opened the drawer once it closes.
  // returnFocus 在抽屉关闭后恢复打开它的控件焦点。
  const returnFocus = React.useRef<HTMLElement | null>(null);
  const isOpen = active !== null;
  React.useEffect(() => {
    if (isOpen) {
      returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      panel.current?.focus();
      return;
    }
    if (returnFocus.current?.isConnected) returnFocus.current.focus();
    returnFocus.current = null;
  }, [isOpen]);
  const title = active === null ? t("pluginMaintenance.title")
    : active === ALL_PLUGINS_SCOPE ? t("pluginMaintenance.uninstalls.title")
      : formatMessage(t("pluginMaintenance.scopedTitle"), { name: shared.pluginName(active) });
  return (
    <aside aria-labelledby={titleId} className="plugin-maintenance-drawer" hidden={!isOpen} ref={panel} role="dialog" tabIndex={-1}
      onKeyDown={(event) => {
        // Nested menus, selects and confirmations consume Escape first; only an unclaimed Escape closes the drawer.
        // 嵌套菜单、选择器与确认框优先消费 Escape；仅未被消费的 Escape 关闭抽屉。
        if (event.key === "Escape" && !event.defaultPrevented && !event.nativeEvent.defaultPrevented) {
          event.preventDefault(); drawer.close();
        }
      }}>
      <header className="plugin-maintenance-drawer-header">
        <h2 id={titleId}>{title}</h2>
        <IconButton aria-label={t("pluginMaintenance.close")} onClick={drawer.close} type="button"><X size={16} /></IconButton>
      </header>
      <div className="plugin-maintenance-drawer-body">
        {[...drawer.state.views].map(([scope, view]) => <div hidden={scope !== active} key={scope === ALL_PLUGINS_SCOPE ? "*" : `plugin:${scope}`}>
          <MaintenanceScope {...shared} active={scope === active} drawer={drawer} scope={scope} view={view} />
        </div>)}
      </div>
    </aside>
  );
}
