import * as React from "react";
import { RefreshCw, X } from "lucide-react";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { useI18n, type TranslationKey } from "@/i18n";
import type { PluginSummary, WorkbenchSnapshot } from "@/types";
import type { PluginDiagnosticsSelection } from "@/types/plugin-resource-diagnostics";
import type { PluginUninstallStatus } from "@/service/plugin-uninstall-contract";
import { PageGuide, PageSection } from "@/components/page";
import { PluginPrewarmPolicyControls } from "./plugin-prewarm-policy-controls";
import { PluginRecoveryCenter, type PluginUninstallUncertainty, type RecoveryKind } from "./plugin-recovery-center";
import { PluginResourceDiagnostics } from "./plugin-resource-diagnostics";
import { PluginRuntimeControls } from "./plugin-runtime-controls";
import type { SkillActionProps } from "./skill-area-types";

// PluginMaintenanceSectionId names one maintenance concern; it doubles as the in-drawer scroll anchor.
// PluginMaintenanceSectionId 命名单个维护关注点，同时作为抽屉内滚动锚点。
export type PluginMaintenanceSectionId = "recovery" | "usage" | "capacity" | "prewarm";

// PluginMaintenanceGate is the synchronous page mutation gate shared by every recovery writer of one surface.
// PluginMaintenanceGate 是同一界面所有恢复写入者共享的同步页面变更门。
export interface PluginMaintenanceGate {
  // mutationBusy reports whether any page action currently owns admission.
  // mutationBusy 报告当前是否有页面动作持有入场。
  mutationBusy: boolean;
  // acquireMutation reserves admission synchronously and returns false when another action owns it.
  // acquireMutation 同步预留入场，另一动作持有时返回假。
  acquireMutation: () => boolean;
  // releaseMutation returns only the admission this recovery acquired.
  // releaseMutation 仅归还此恢复取得的入场。
  releaseMutation: () => void;
}

// PluginMaintenanceContext is the data and action surface shared by every maintenance section of one host.
// PluginMaintenanceContext 是同一宿主所有维护区块共享的数据与动作。
export interface PluginMaintenanceContext {
  // plugins is the full projected catalog; pluginName resolves localized display names.
  // plugins 是完整投影目录；pluginName 解析本地化显示名称。
  plugins: PluginSummary[];
  pluginName: (pluginId: string) => string;
  // conversations carries authoritative owner identities for resource navigation.
  // conversations 携带资源导航的权威所有者身份。
  conversations: WorkbenchSnapshot["conversations"];
  // actions are the existing service-backed skill actions; maintenance adds no new mutation.
  // actions 是既有服务支持的技能动作；维护不新增任何变更。
  actions: SkillActionProps;
  // onSelectPluginResourceOwner follows the existing workspace selection chain.
  // onSelectPluginResourceOwner 遵循既有工作台选择链。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
  // gate is the page mutation gate shared with the plugin lifecycle controls of the same surface.
  // gate 是与同一界面插件生命周期控件共享的页面变更门。
  gate: PluginMaintenanceGate;
  // uninstallUncertainty is the unconfirmed foreground uninstall that blocks ordinary replay.
  // uninstallUncertainty 是阻止普通重放的未确认前台卸载。
  uninstallUncertainty: PluginUninstallUncertainty | null;
  // onUninstallConfirmed clears the matching foreground uncertainty after a confirmed outcome.
  // onUninstallConfirmed 在确认结果后清除匹配的前台不确定状态。
  onUninstallConfirmed: (pluginId: string) => void;
  // onUninstallRecordsObserved receives every successful full read of retained uninstall originals.
  // onUninstallRecordsObserved 接收保留卸载原实例的每次成功全量读取。
  onUninstallRecordsObserved?: (records: PluginUninstallStatus[]) => void;
}

// guideKeys lists the when/how/effect explanation keys of every section.
// guideKeys 列出每个区块的何时／如何／效果说明键。
const guideKeys: Record<"recovery" | "usage" | "capacity" | "prewarm", readonly [TranslationKey, TranslationKey, TranslationKey]> = {
  recovery: ["pluginMaintenance.recovery.when", "pluginMaintenance.recovery.how", "pluginMaintenance.recovery.effect"],
  usage: ["pluginMaintenance.usage.when", "pluginMaintenance.usage.how", "pluginMaintenance.usage.effect"],
  capacity: ["pluginMaintenance.capacity.when", "pluginMaintenance.capacity.how", "pluginMaintenance.capacity.effect"],
  prewarm: ["pluginMaintenance.prewarm.when", "pluginMaintenance.prewarm.how", "pluginMaintenance.prewarm.effect"],
};
// useMaintenanceGuide resolves the when/how/effect answers of one section into the shared page guide.
// useMaintenanceGuide 将单个区块的何时／如何／效果回答解析为共享页面说明。
function useMaintenanceGuide(section: keyof typeof guideKeys): React.ReactNode {
  const { t } = useI18n();
  const [when, how, effect] = guideKeys[section];
  return <PageGuide items={[
    { term: t("pluginMaintenance.guide.when"), detail: t(when) },
    { term: t("pluginMaintenance.guide.how"), detail: t(how) },
    { term: t("pluginMaintenance.guide.effect"), detail: t(effect) },
  ]} />;
}

// RecoveryMaintenanceSection scans for interrupted operations and offers one "check again" control.
// RecoveryMaintenanceSection 扫描中断的操作，并提供一个"重新检查"控件。
// Parameters: kinds limits the scanned families; uninstallOnly switches to the global uninstall wording.
// 参数：kinds 限制扫描类别；uninstallOnly 切换为全局卸载文案。
export function RecoveryMaintenanceSection(props: PluginMaintenanceContext & {
  kinds: ReadonlyArray<RecoveryKind>;
  scopePluginId?: string;
  compact?: boolean;
  uninstallOnly?: boolean;
  onLocate?: (kind: "update" | "uninstall", pluginId: string, requestId: string) => void;
}) {
  const { t } = useI18n();
  const guide = useMaintenanceGuide("recovery");
  // scanToken increments on every explicit re-check; mounting already performs the first scan.
  // scanToken 在每次明确重新检查时递增；挂载即完成首次扫描。
  const [scanToken, setScanToken] = React.useState(0);
  return (
    <PageSection data-anchor="recovery" guide={guide} guideCollapsed={props.compact} guideLabel={t("pluginMaintenance.guide.toggle")}
      actions={<Button onClick={() => setScanToken((value) => value + 1)} size="sm" type="button" variant="outline">
        <RefreshCw data-icon="inline-start" />{t("pluginMaintenance.recovery.recheck")}</Button>}
      description={t(props.uninstallOnly ? "pluginMaintenance.uninstalls.summary" : "pluginMaintenance.recovery.summary")}
      title={t(props.uninstallOnly ? "pluginMaintenance.uninstalls.title" : "pluginMaintenance.recovery.title")}>
      <PluginRecoveryCenter actions={props.actions} gate={props.gate} kinds={props.kinds} onLocate={props.onLocate}
        onUninstallConfirmed={props.onUninstallConfirmed} onUninstallRecordsObserved={props.onUninstallRecordsObserved}
        pluginName={props.pluginName} scanToken={scanToken} scopePluginId={props.scopePluginId}
        uninstallUncertainty={props.uninstallUncertainty} />
    </PageSection>
  );
}

// UsageMaintenanceSection shows which conversations still hold resources of one plugin or original request.
// UsageMaintenanceSection 显示哪些会话仍持有某个插件或原请求的资源。
export function UsageMaintenanceSection(props: Pick<PluginMaintenanceContext, "conversations" | "onSelectPluginResourceOwner" | "pluginName"> & {
  selection: PluginDiagnosticsSelection;
  onSelect: (selection: PluginDiagnosticsSelection) => void;
  compact?: boolean;
  onClose?: () => void;
}) {
  const { t } = useI18n();
  const guide = useMaintenanceGuide("usage");
  return (
    <PageSection data-anchor="usage" guide={guide} guideCollapsed={props.compact} guideLabel={t("pluginMaintenance.guide.toggle")}
      actions={props.onClose ? <IconButton aria-label={t("pluginMaintenance.usage.close")} onClick={props.onClose} type="button"><X size={16} /></IconButton> : undefined}
      description={t("pluginMaintenance.usage.summary")} title={t("pluginMaintenance.usage.title")}>
      <PluginResourceDiagnostics conversations={props.conversations} onSelect={props.onSelect}
        onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} pluginName={props.pluginName} selection={props.selection} />
    </PageSection>
  );
}

// CapacityMaintenanceSection explains and edits how many Lua runtimes each plugin may keep.
// CapacityMaintenanceSection 说明并编辑每个插件可保留的 Lua 运行环境数量。
export function CapacityMaintenanceSection(props: { plugins: PluginSummary[]; onRefreshCatalog: SkillActionProps["onRefreshLuaSkills"]; pluginId?: string; compact?: boolean }) {
  const { t } = useI18n();
  const guide = useMaintenanceGuide("capacity");
  return (
    <PageSection data-anchor="capacity" guide={guide} guideCollapsed={props.compact} guideLabel={t("pluginMaintenance.guide.toggle")}
      description={t("pluginMaintenance.capacity.summary")} title={t("pluginMaintenance.capacity.title")}>
      <PluginRuntimeControls onRefreshCatalog={props.onRefreshCatalog} pluginId={props.pluginId} plugins={props.plugins} />
    </PageSection>
  );
}

// PrewarmMaintenanceSection explains and edits the automatic prewarm rules.
// PrewarmMaintenanceSection 说明并编辑自动预热规则。
export function PrewarmMaintenanceSection(props: { plugins: PluginSummary[] }) {
  const { t } = useI18n();
  const guide = useMaintenanceGuide("prewarm");
  return (
    <PageSection data-anchor="prewarm" guide={guide} description={t("pluginMaintenance.prewarm.summary")} title={t("pluginMaintenance.prewarm.title")}>
      <PluginPrewarmPolicyControls plugins={props.plugins} />
    </PageSection>
  );
}
