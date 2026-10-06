import * as React from "react";
import { CircleCheck, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { InlineDisclosure } from "@/components/ui/disclosure";
import { Input } from "@/components/ui/input";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { PageEmptyState, PageHint, PageItemList } from "@/components/page";
import { useI18n } from "@/i18n";
import type { PluginSummary } from "@/types";
import type { PluginUninstallStatus } from "@/service/plugin-uninstall-recovery-service";
import type { PluginMaintenanceGate } from "./plugin-maintenance-workspace";
import type { RecoveryReport } from "./plugin-maintenance-section";
import { PluginUninstallRecoveryControls, type PluginUninstallUncertainty } from "./plugin-uninstall-recovery-controls";
import { PluginUpdateControls } from "./plugin-update-controls";
import type { SkillActionProps } from "./skill-area-types";

// RecoveryKind names one family of interruptible plugin management operations.
// RecoveryKind 命名一类可被中断的插件管理操作。
export type RecoveryKind = "update" | "uninstall";

// PluginRecoveryCenterProps binds the catalog, the shared gate and the scan trigger to every reader.
// PluginRecoveryCenterProps 将目录、共享门与扫描触发绑定到所有读取器。
interface PluginRecoveryCenterProps {
  // plugins decides which owners are scanned; pluginName renders their display names.
  // plugins 决定扫描哪些所有者；pluginName 渲染其显示名称。
  plugins: PluginSummary[];
  pluginName: (pluginId: string) => string;
  // actions are the existing service-backed skill actions; the center adds no new mutation.
  // actions 是既有服务支持的技能动作；中心不新增任何变更。
  actions: SkillActionProps;
  // gate is the page mutation gate shared with the plugin lifecycle controls of the same surface.
  // gate 是与同一界面插件生命周期控件共享的页面变更门。
  gate: PluginMaintenanceGate;
  // kinds limits the scanned operation families; the global drawer only scans uninstalls.
  // kinds 限制扫描的操作类别；全局抽屉仅扫描卸载。
  kinds: ReadonlyArray<RecoveryKind>;
  // scopePluginId restricts every reader to one plugin when opened from its card.
  // scopePluginId 从插件卡片打开时将所有读取器限定到单个插件。
  scopePluginId?: string;
  // scanToken changes when the user asks to check again; every reader re-reads once.
  // scanToken 在用户要求重新检查时变化；每个读取器重新读取一次。
  scanToken: number;
  // uninstall bindings preserve the unconfirmed foreground uninstall and the panel's attention bar.
  // 卸载绑定保留未确认的前台卸载及面板提醒条。
  uninstallUncertainty: PluginUninstallUncertainty | null;
  onUninstallConfirmed: (pluginId: string) => void;
  onUninstallRecordsObserved?: (records: PluginUninstallStatus[]) => void;
  // onLocate shows which conversations still use resources held by one original request.
  // onLocate 显示哪些会话仍在使用某个原请求持有的资源。
  onLocate?: (kind: "update" | "uninstall", pluginId: string, requestId: string) => void;
}

// PluginRecoveryCenter scans every relevant plugin for interrupted operations and lists only what needs attention.
// PluginRecoveryCenter 扫描所有相关插件的中断操作，仅列出需要处理的项目。
// Returns: the findings, or a single all-clear state once every reader has settled with nothing to show.
// 返回值：发现项；所有读取器完成且无内容时显示单一的一切正常状态。
export function PluginRecoveryCenter(props: PluginRecoveryCenterProps) {
  const { t } = useI18n();
  const inputId = React.useId();
  // extraOwners are identifiers typed by the user for plugins no longer in the catalog.
  // extraOwners 是用户为已不在目录中的插件输入的标识。
  const [extraOwners, setExtraOwners] = React.useState<string[]>([]);
  const [extraDraft, setExtraDraft] = React.useState("");
  const [reports, setReports] = React.useState<ReadonlyMap<string, RecoveryReport>>(() => new Map());
  const scoped = props.scopePluginId;
  const scanUpdates = props.kinds.includes("update");
  // owners lists the user-installed packages whose updates (including first installs) are read.
  // owners 列出需要读取更新（含首次安装）的用户安装包。
  const owners = React.useMemo(() => {
    const result: string[] = [];
    const add = (pluginId: string, plugin: PluginSummary | undefined) => {
      if (scanUpdates && (plugin === undefined || plugin.extension?.origin === "userInstalled") && !result.includes(pluginId)) {
        result.push(pluginId);
      }
    };
    if (scoped !== undefined) add(scoped, props.plugins.find((plugin) => plugin.id === scoped));
    else for (const plugin of props.plugins) add(plugin.id, plugin);
    for (const pluginId of extraOwners) add(pluginId, undefined);
    return result;
  }, [extraOwners, props.plugins, scanUpdates, scoped]);
  // expected is the set of reader keys currently mounted; stale reports from removed readers are ignored.
  // expected 是当前挂载的读取器键集合；已移除读取器的陈旧报告被忽略。
  const expected = owners.map((pluginId) => `update:${pluginId}`);
  if (props.kinds.includes("uninstall")) expected.push("uninstall");
  const loading = expected.some((key) => reports.get(key)?.loading !== false);
  const visible = expected.reduce((sum, key) => sum + (reports.get(key)?.visible ?? 0), 0);
  // report records one reader's state only when it changed, avoiding render loops.
  // report 仅在读取器状态变化时记录，避免渲染循环。
  const report = React.useCallback((key: string, next: RecoveryReport) => {
    setReports((current) => {
      const previous = current.get(key);
      if (previous?.loading === next.loading && previous.visible === next.visible) return current;
      return new Map(current).set(key, next);
    });
  }, []);
  // addExtra adds one exact identifier to the scan; the readers then report like any catalog plugin.
  // addExtra 将单个精确标识加入扫描；之后读取器与目录插件一样报告。
  const addExtra = () => {
    const pluginId = extraDraft.trim();
    if (!pluginId) return;
    setExtraOwners((current) => current.includes(pluginId) ? current : [...current, pluginId]);
    setExtraDraft("");
  };
  const gate = props.gate;
  return (
    <>
      <PageItemList>
        {owners.map((pluginId) => <PluginUpdateControls actions={props.actions} key={pluginId}
          onLocateDiagnostics={props.onLocate ? (plugin, request) => props.onLocate?.("update", plugin, request) : undefined}
          onReport={(next) => report(`update:${pluginId}`, next)} pluginId={pluginId} pluginName={props.pluginName(pluginId)}
          scanToken={props.scanToken} />)}
        {props.kinds.includes("uninstall") && <PluginUninstallRecoveryControls acquireMutation={gate.acquireMutation} actions={props.actions}
          initialUncertainty={props.uninstallUncertainty} mutationBusy={gate.mutationBusy} onConfirmed={props.onUninstallConfirmed}
          onLocateDiagnostics={props.onLocate ? (plugin, request) => props.onLocate?.("uninstall", plugin, request) : undefined}
          onRecordsObserved={props.onUninstallRecordsObserved} onReport={(next) => report("uninstall", next)} pluginId={scoped}
          pluginName={props.pluginName} releaseMutation={gate.releaseMutation} scanToken={props.scanToken} />}
      </PageItemList>
      {loading && visible === 0 && <StreamPlaceholder count={2} label={t("pluginRecovery.scanning")} shape="rows" />}
      {!loading && visible === 0 && <PageEmptyState description={t(scoped === undefined ? "pluginRecovery.allClearHint" : "pluginRecovery.allClearScopedHint")}
        icon={<CircleCheck />} title={t("pluginRecovery.allClear")} />}
      {scoped === undefined && scanUpdates && <InlineDisclosure label={t("pluginRecovery.extraToggle")}>
        <PageHint>{t("pluginRecovery.extraHint")}</PageHint>
        <form className="plugin-recovery-extra-form" onSubmit={(event) => { event.preventDefault(); addExtra(); }}>
          <label className="sr-only" htmlFor={inputId}>{t("pluginRecovery.extraField")}</label>
          <Input id={inputId} onChange={(event) => setExtraDraft(event.target.value)} placeholder={t("pluginRecovery.extraField")} value={extraDraft} />
          <Button disabled={!extraDraft.trim()} size="sm" type="submit" variant="outline"><Plus data-icon="inline-start" />{t("pluginRecovery.extraAdd")}</Button>
        </form>
      </InlineDisclosure>}
    </>
  );
}
