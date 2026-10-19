import {
  History,
  PlugZap,
} from "lucide-react";
import * as React from "react";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { PanelTitle, StatusIcon } from "@/components/shared/status";
import { Button } from "@/ui-kit/primitives/button";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { Switch } from "@/ui-kit/primitives/switch";
import { Input } from "@/ui-kit/primitives/input";
import { DirectoryRow, WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import { useI18n } from "@/i18n";
import type {
  LogEntrySummary,
  StartupSnapshot,
} from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// LogsArea renders startup and diagnostic logs.
// LogsArea 渲染启动与诊断日志。
export function LogsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps;
  // Whether bounded wire-request tracing is currently enabled.
  // 当前是否启用有界线上请求追踪。
  providerRequestTraceEnabled: boolean;
  // Whether failed session requests are also retained in a separate bounded ring.
  // 是否同时将失败会话请求保留到独立有界环中。
  providerErrorTraceEnabled: boolean;
  // Startup snapshot shown in diagnostics.
  // 诊断中展示的启动快照。
  startup: StartupSnapshot;
  // Diagnostic log entries shown beside startup.
  // 启动状态旁展示的诊断日志。
  logs: LogEntrySummary[];
}) {
  const { t } = useI18n();
  // Select one local diagnostic category without changing recording or retention settings.
  // 选择本地诊断分类，不改变记录或保留设置。
  const [category, setCategory] = React.useState("startup");
  // Local view filters never change the underlying diagnostic retention or execution policy.
  // 本地视图筛选不改变诊断保留或执行策略。
  const [logFilter, setLogFilter] = React.useState("");
  // Filter the bounded service projection locally; no database request is needed while typing.
  // 在本地筛选有界服务投影，输入过程中无需查询数据库。
  const visibleLogs = React.useMemo(() => props.logs.filter((log) => `${log.level} ${log.source} ${log.message}`.toLocaleLowerCase().includes(logFilter.trim().toLocaleLowerCase())), [props.logs, logFilter]);
  // Synchronous guards prevent rapid repeat clicks before the disabled state is painted.
  // 同步守卫在禁用状态绘制前阻止快速重复点击。
  const traceActionRunningRef = React.useRef(false);
  const traceSettingRunningRef = React.useRef(false);
  const [savingTraceSetting, setSavingTraceSetting] = React.useState(false);
  const [traceSettingError, setTraceSettingError] = React.useState<string | null>(null);
  const [traceDirectoryBusy, setTraceDirectoryBusy] = React.useState(false);
  const [traceDirectoryNotice, setTraceDirectoryNotice] = React.useState<string | null>(null);
  // Destructive trace cleanup uses the same accessible confirmation surface as other settings.
  // 破坏性的追踪清理使用与其他设置一致的无障碍确认界面。
  const [clearConfirmationOpen, setClearConfirmationOpen] = React.useState(false);

  // handleTraceSettingChange persists one explicit diagnostic switch selection.
  // handleTraceSettingChange 持久化一次显式诊断开关选择。
  async function handleTraceSettingChange(enabled: boolean, errorsEnabled?: boolean): Promise<void> {
    if (traceSettingRunningRef.current) return;
    traceSettingRunningRef.current = true;
    setSavingTraceSetting(true);
    setTraceSettingError(null);
    try {
      await props.actions.onSaveProviderRequestTraceSettings(enabled, errorsEnabled);
    } catch (error) {
      setTraceSettingError(error instanceof Error ? error.message : String(error));
    } finally {
      setSavingTraceSetting(false);
      traceSettingRunningRef.current = false;
    }
  }

  // handleOpenTraceDirectory opens the selected session's exact bounded trace folder.
  // handleOpenTraceDirectory 打开当前会话的精确有界追踪目录。
  async function handleOpenTraceDirectory(): Promise<void> {
    if (traceActionRunningRef.current) return;
    traceActionRunningRef.current = true;
    setTraceDirectoryBusy(true);
    setTraceDirectoryNotice(null);
    try {
      await props.actions.onOpenSelectedProviderApiTraceDirectory();
    } catch (error) {
      setTraceDirectoryNotice(`${t("settings.diagnostics.providerRequestTrace.actionFailed")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setTraceDirectoryBusy(false);
      traceActionRunningRef.current = false;
    }
  }

  // handleClearTraceDirectory confirms and clears only the selected session's exact trace folder.
  // handleClearTraceDirectory 确认后仅清除当前会话的精确追踪目录。
  async function handleClearTraceDirectory(): Promise<void> {
    if (traceActionRunningRef.current) return;
    setTraceDirectoryBusy(true);
    traceActionRunningRef.current = true;
    setTraceDirectoryNotice(null);
    try {
      const removed = await props.actions.onClearSelectedProviderApiTraceDirectory();
      setTraceDirectoryNotice(removed
        ? t("settings.diagnostics.providerRequestTrace.clearSucceeded")
        : t("settings.diagnostics.providerRequestTrace.clearEmpty"));
    } catch (error) {
      setTraceDirectoryNotice(`${t("settings.diagnostics.providerRequestTrace.actionFailed")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setTraceDirectoryBusy(false);
      traceActionRunningRef.current = false;
    }
  }

  return (
    <WorkspaceDirectoryLayout contentClassName="settings-diagnostics-page" navigationMode="tabs" navigation={
      <WorkspaceDirectoryMenu ariaLabel={t("settings.diagnostics.diagnosticsCategories")} heading={t("workspaceToolPanel.category")}>
        {([
          ["startup", t("settings.diagnostics.startupStatus")],
          ["logs", t("settings.diagnostics.logs")],
          ["trace", t("settings.diagnostics.providerRequestTrace")],
        ] as const).map(([id, label]) => <DirectoryRow active={category === id} key={id} label={label} onSelect={() => setCategory(id)} />)}
      </WorkspaceDirectoryMenu>
    }>
      {category === "startup" ? <StartupPanel startup={props.startup} /> : null}
      {category === "logs" ? <section>
        <PanelTitle icon={<History size={17} />} title={t("settings.diagnostics.logs")} />
        <Input aria-label={t("settings.diagnostics.filterDiagnosticLogs")} placeholder={t("settings.diagnostics.searchLevelSourceOr")} value={logFilter} onChange={(event) => setLogFilter(event.target.value)} />
        <div className="diagnostic-log-rows">
          {visibleLogs.map((log) => (
            <article className="diagnostic-log-row" key={log.id}>
              <BadgeLevel level={log.level} />
              <strong>{log.source}</strong>
              <p>{log.message}</p>
            </article>
          ))}
        </div>
        {visibleLogs.length === 0 ? <p className="vk-form-note">{t("settings.diagnostics.noMatchingDiagnosticLogs")}</p> : null}
      </section> : null}
      {category === "trace" ? <section>
        <div className="provider-trace-settings">
          <article>
            {/* Keep the setting and its current state together, with recording details always visible below.
                将设置与当前状态放在一起，下方常显记录说明。 */}
            <header className="provider-trace-header">
              <div>
                <h2>{t("settings.diagnostics.providerRequestTrace.switch")}</h2>
                <p>{t("settings.diagnostics.providerRequestTrace.completeRequests")}</p>
              </div>
              <label className="provider-trace-toggle">
                <span>{props.providerRequestTraceEnabled
                  ? t("settings.diagnostics.providerRequestTrace.enabled")
                  : t("settings.diagnostics.providerRequestTrace.disabled")}</span>
              <Switch
                aria-label={t("settings.diagnostics.providerRequestTrace.switch")}
                checked={props.providerRequestTraceEnabled}
                disabled={savingTraceSetting}
                onCheckedChange={(enabled) => void handleTraceSettingChange(enabled)}
              />
              </label>
            </header>
            <header className="provider-trace-header">
              <div>
                <h2>{t("settings.diagnostics.providerRequestTrace.errorsSwitch")}</h2>
                <p>{t("settings.diagnostics.providerRequestTrace.errorsHelp")}</p>
              </div>
              <label className="provider-trace-toggle">
                <span>{props.providerErrorTraceEnabled
                  ? t("settings.diagnostics.providerRequestTrace.enabled")
                  : t("settings.diagnostics.providerRequestTrace.disabled")}</span>
                <Switch
                  aria-label={t("settings.diagnostics.providerRequestTrace.errorsSwitch")}
                  checked={props.providerErrorTraceEnabled}
                  disabled={savingTraceSetting}
                  onCheckedChange={(enabled) => void handleTraceSettingChange(props.providerRequestTraceEnabled, enabled)}
                />
              </label>
            </header>
            <dl className="provider-trace-information">
              <div>
                <dt>{t("settings.diagnostics.providerRequestTrace.recordedData")}</dt>
                <dd>{t("settings.diagnostics.providerRequestTrace.recordedDataHelp")}</dd>
              </div>
              <div>
                <dt>{t("settings.diagnostics.providerRequestTrace.retention")}</dt>
                <dd>{t("settings.diagnostics.providerRequestTrace.retentionHelp")}<code>temp/provider-api-trace</code></dd>
              </div>
              <div>
                <dt>{t("settings.diagnostics.providerRequestTrace.privacy")}</dt>
                <dd>{t("settings.diagnostics.providerRequestTrace.privacyHelp")}</dd>
              </div>
            </dl>
            <div className="provider-trace-actions">
              <Button
                disabled={traceDirectoryBusy}
                onClick={() => void handleOpenTraceDirectory()}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("settings.diagnostics.providerRequestTrace.openDirectory")}
              </Button>
              <Button
                disabled={traceDirectoryBusy}
                onClick={() => setClearConfirmationOpen(true)}
                size="sm"
                type="button"
                variant="destructive"
              >
                {t("settings.diagnostics.providerRequestTrace.clearDirectory")}
              </Button>
            </div>
            <AlertDialog open={clearConfirmationOpen} onOpenChange={setClearConfirmationOpen}>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>{t("settings.diagnostics.providerRequestTrace.clearDirectory")}</AlertDialogTitle>
                  <AlertDialogDescription>{t("settings.diagnostics.providerRequestTrace.clearConfirm")}</AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>{t("settings.diagnostics.cancel")}</AlertDialogCancel>
                  <AlertDialogAction disabled={traceDirectoryBusy} variant="destructive" onClick={() => void handleClearTraceDirectory()}>{t("settings.diagnostics.confirmCleanup")}</AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
            {traceDirectoryNotice ? (
              <p className="vk-form-note">{traceDirectoryNotice}</p>
            ) : null}
            {traceSettingError ? (
              <p className="settings-inline-error">
                {t("settings.diagnostics.providerRequestTrace.saveFailed")}: {traceSettingError}
              </p>
            ) : null}
          </article>
        </div>
      </section> : null}
    </WorkspaceDirectoryLayout>
  );
}

// StartupPanel renders service startup progress.
// StartupPanel 渲染服务启动进度。
function StartupPanel(props: {
  // Startup snapshot displayed as step rows.
  // 以步骤行展示的启动快照。
  startup: StartupSnapshot;
}) {
  const { t } = useI18n();
  return (
    <section className="settings-diagnostics-startup">
      <PanelTitle icon={<PlugZap size={17} />} title={t("settings.diagnostics.startupStatus")} />
      <ol className="vk-form-steplog">
        {props.startup.steps.map((step) => (
          <li key={step.id}>
            <StatusIcon status={step.status} />
            <div>
              <strong>{step.label}</strong>
              <span>{step.message ?? t("settings.diagnostics.waitingStatus")}</span>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}

// BadgeLevel preserves the exact log severity without implying that every information row is a success.
// BadgeLevel 保留精确日志级别，不把每条信息日志暗示为成功状态。
function BadgeLevel(props: { level: string }) {
  return <span className="diagnostic-log-level" data-level={props.level}>{props.level}</span>;
}
