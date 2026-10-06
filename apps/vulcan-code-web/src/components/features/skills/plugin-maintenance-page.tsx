import * as React from "react";
import { PageBody, PageTabPanel, PageTabs } from "@/components/page";
import { useI18n } from "@/i18n";
import type { PluginSummary, WorkbenchSnapshot } from "@/types";
import type { PluginDiagnosticsSelection } from "@/types/plugin-resource-diagnostics";
import { pluginDisplayName } from "./plugin-localized-text";
import {
  CapacityMaintenanceSection, PrewarmMaintenanceSection, RecoveryMaintenanceSection, UsageMaintenanceSection,
  type PluginMaintenanceGate,
} from "./plugin-maintenance-workspace";
import type { SkillActionProps } from "./skill-area-types";

// PluginMaintenancePageProps supplies the catalog, sessions and existing actions to the global maintenance page.
// PluginMaintenancePageProps 为全局维护页面提供目录、会话及既有动作。
interface PluginMaintenancePageProps {
  // plugins is the full projected plugin catalog.
  // plugins 是完整投影插件目录。
  plugins: PluginSummary[];
  // conversations binds resource owners for the usage view.
  // conversations 为使用情况视图绑定资源所有者。
  conversations: WorkbenchSnapshot["conversations"];
  // actions are the existing service-backed skill actions.
  // actions 是既有服务支持的技能动作。
  actions: SkillActionProps;
  // onSelectPluginResourceOwner follows the existing workspace selection chain.
  // onSelectPluginResourceOwner 遵循既有工作台选择链。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
}

// PageTab names one page tab; usage is reached from a located operation rather than as its own tab.
// PageTab 命名单个页面分页；使用情况从已定位的操作进入，而非独立分页。
type PageTab = "recovery" | "capacity" | "prewarm";
const pageTabs: readonly PageTab[] = ["recovery", "capacity", "prewarm"];

// PluginMaintenancePage is the dedicated home of every low-frequency plugin maintenance tool.
// PluginMaintenancePage 是所有低频插件维护工具的专属页面。
// It owns its own synchronous gate because the plugin list page is not mounted at the same time.
// 由于插件列表页面不会同时挂载，它持有自己的同步门。
export function PluginMaintenancePage(props: PluginMaintenancePageProps) {
  const { locale, t } = useI18n();
  const [tab, setTab] = React.useState<PageTab>("recovery");
  const [selection, setSelection] = React.useState<PluginDiagnosticsSelection | null>(null);
  // admission closes the repeat-click window before React commits the busy state.
  // admission 在 React 提交忙碌状态前关闭重复点击窗口。
  const admission = React.useRef(false);
  const [mutationBusy, setMutationBusy] = React.useState(false);
  const acquireMutation = React.useCallback((): boolean => {
    if (admission.current) return false;
    admission.current = true; setMutationBusy(true); return true;
  }, []);
  const releaseMutation = React.useCallback((): void => {
    admission.current = false; setMutationBusy(false);
  }, []);
  const gate = React.useMemo<PluginMaintenanceGate>(() => ({ mutationBusy, acquireMutation, releaseMutation }),
    [mutationBusy, acquireMutation, releaseMutation]);
  const pluginName = React.useCallback((pluginId: string) => pluginDisplayName(props.plugins, locale, pluginId), [locale, props.plugins]);
  return (
    <PageBody className="plugin-maintenance-page">
      <PageTabs ariaLabel={t("pluginMaintenance.title")} onValueChange={setTab} value={tab}
        options={pageTabs.map((value) => ({ value, label: t(`pluginMaintenance.tab.${value}`) }))} />
      {/* Visited panels stay mounted while hidden so unconfirmed state survives switching. */}
      {/* 已访问面板隐藏时保持挂载，使未确认状态在切换后保留。 */}
      <PageTabPanel active={tab === "recovery"}>
        <RecoveryMaintenanceSection actions={props.actions} conversations={props.conversations} gate={gate}
          kinds={["update", "uninstall"]}
          onLocate={(kind, pluginId, requestId) => setSelection({ kind, pluginId, requestId })}
          onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} onUninstallConfirmed={() => undefined}
          pluginName={pluginName} plugins={props.plugins} uninstallUncertainty={null} />
        {selection !== null && <UsageMaintenanceSection conversations={props.conversations} onClose={() => setSelection(null)}
          onSelect={setSelection} onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} pluginName={pluginName} selection={selection} />}
      </PageTabPanel>
      <PageTabPanel active={tab === "capacity"}>
        <CapacityMaintenanceSection onRefreshCatalog={props.actions.onRefreshLuaSkills} plugins={props.plugins} />
      </PageTabPanel>
      <PageTabPanel active={tab === "prewarm"}>
        <PrewarmMaintenanceSection plugins={props.plugins} />
      </PageTabPanel>
    </PageBody>
  );
}
