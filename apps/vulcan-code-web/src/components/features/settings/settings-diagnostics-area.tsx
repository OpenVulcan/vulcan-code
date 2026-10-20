import {
  FolderOpen,
  History,
  Trash2,
} from "lucide-react";
import * as React from "react";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { PageEmptyState, PageFacts, PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { Switch } from "@/ui-kit/primitives/switch";
import { Input } from "@/ui-kit/primitives/input";
import { formatMessage, useI18n } from "@/i18n";
import type { LogEntrySummary } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// LogLevelFilter groups exact log levels into the filters the console offers.
// LogLevelFilter 将精确日志级别归入控制台提供的筛选项。
type LogLevelFilter = "all" | "error" | "warn" | "info";

// LOG_LEVEL_FILTERS lists the console filters in display order.
// LOG_LEVEL_FILTERS 按显示顺序列出控制台筛选项。
const LOG_LEVEL_FILTERS: readonly LogLevelFilter[] = ["all", "error", "warn", "info"];

// logLevelGroup maps one exact level onto its filter group; unknown levels read as information.
// logLevelGroup 将精确级别映射到筛选分组；未知级别按信息处理。
function logLevelGroup(level: string): Exclude<LogLevelFilter, "all"> {
  const normalized = level.toLowerCase();
  if (normalized === "error") return "error";
  if (normalized === "warn" || normalized === "warning") return "warn";
  return "info";
}

// LogsArea is the single runtime diagnostics page: the log console fills the main column and the model request
// recording controls sit in a side rail. Startup progress is not repeated here; the startup screen owns it.
// LogsArea 是唯一的运行诊断页：日志控制台占据主列，模型请求记录控制位于侧栏。启动进度不在此重复，
// 由启动画面负责。
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
  // Diagnostic log entries.
  // 诊断日志条目。
  logs: LogEntrySummary[];
}) {
  const { locale, t } = useI18n();
  // Local view filters never change the underlying diagnostic retention or execution policy.
  // 本地视图筛选不改变诊断保留或执行策略。
  const [logFilter, setLogFilter] = React.useState("");
  const [levelFilter, setLevelFilter] = React.useState<LogLevelFilter>("all");
  // levelCounts sizes each filter from the bounded service projection.
  // levelCounts 依据有界服务投影统计各筛选项数量。
  const levelCounts = React.useMemo(() => {
    const counts: Record<LogLevelFilter, number> = { all: props.logs.length, error: 0, warn: 0, info: 0 };
    for (const log of props.logs) counts[logLevelGroup(log.level)] += 1;
    return counts;
  }, [props.logs]);
  // Filter the bounded service projection locally; no database request is needed while typing.
  // 在本地筛选有界服务投影，输入过程中无需查询数据库。
  const visibleLogs = React.useMemo(() => {
    const needle = logFilter.trim().toLocaleLowerCase();
    return props.logs.filter((log) => (levelFilter === "all" || logLevelGroup(log.level) === levelFilter)
      && `${log.level} ${log.source} ${log.message}`.toLocaleLowerCase().includes(needle));
  }, [props.logs, logFilter, levelFilter]);
  const levelLabel = (level: LogLevelFilter) => t(
    level === "all" ? "settings.diagnostics.levelAll"
      : level === "error" ? "settings.diagnostics.levelError"
        : level === "warn" ? "settings.diagnostics.levelWarn" : "settings.diagnostics.levelInfo",
  );

  return (
    <div className="diagnostics-page">
      <PageSection
        className="diagnostics-console"
        layout="rows"
        title={t("settings.diagnostics.logs")}
        status={<span className="diagnostics-count">{formatMessage(t("settings.diagnostics.logCount"), { count: visibleLogs.length.toLocaleString(locale) })}</span>}
      >
        <div className="diagnostics-console-tools">
          <Input aria-label={t("settings.diagnostics.filterDiagnosticLogs")} placeholder={t("settings.diagnostics.searchLevelSourceOr")} value={logFilter} onChange={(event) => setLogFilter(event.target.value)} />
          <div className="diagnostics-levels" role="group" aria-label={t("settings.diagnostics.levelFilter")}>
            {LOG_LEVEL_FILTERS.map((level) => (
              <button aria-pressed={levelFilter === level} data-level={level} key={level} onClick={() => setLevelFilter(level)} type="button">
                {levelLabel(level)}<small>{levelCounts[level].toLocaleString(locale)}</small>
              </button>
            ))}
          </div>
        </div>
        {visibleLogs.length > 0 ? (
          <ol aria-label={t("settings.diagnostics.logs")} className="diagnostic-log-list">
            {visibleLogs.map((log) => (
              <li className="diagnostic-log-row" data-level={logLevelGroup(log.level)} key={log.id}>
                <span className="diagnostic-log-level">{log.level}</span>
                <strong>{log.source}</strong>
                <p>{log.message}</p>
              </li>
            ))}
          </ol>
        ) : <PageEmptyState icon={<History />} title={t("settings.diagnostics.noMatchingDiagnosticLogs")} />}
      </PageSection>
      <ProviderRequestTracePanel actions={props.actions} errorsEnabled={props.providerErrorTraceEnabled} enabled={props.providerRequestTraceEnabled} />
    </div>
  );
}

// ProviderRequestTracePanel holds the model request recording switches, the trace folder actions and what is kept.
// ProviderRequestTracePanel 包含模型请求记录开关、记录文件夹操作及保存内容说明。
function ProviderRequestTracePanel(props: {
  actions: SettingsActionProps;
  enabled: boolean;
  errorsEnabled: boolean;
}) {
  const { t } = useI18n();
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

  // runTraceDirectoryAction runs one folder action under the shared guard and reports its outcome.
  // runTraceDirectoryAction 在共享守卫下执行一次文件夹操作并报告结果。
  async function runTraceDirectoryAction(action: () => Promise<string | null>): Promise<void> {
    if (traceActionRunningRef.current) return;
    traceActionRunningRef.current = true;
    setTraceDirectoryBusy(true);
    setTraceDirectoryNotice(null);
    try {
      setTraceDirectoryNotice(await action());
    } catch (error) {
      setTraceDirectoryNotice(`${t("settings.diagnostics.providerRequestTrace.actionFailed")}: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setTraceDirectoryBusy(false);
      traceActionRunningRef.current = false;
    }
  }

  return (
    <PageSection
      className="diagnostics-trace provider-trace-settings"
      layout="rows"
      title={t("settings.diagnostics.providerRequestTrace")}
      status={<span className="diagnostics-trace-state" data-on={props.enabled || props.errorsEnabled || undefined}>
        {t(props.enabled || props.errorsEnabled ? "settings.diagnostics.providerRequestTrace.enabled" : "settings.diagnostics.providerRequestTrace.disabled")}
      </span>}
      guide={<PageFacts items={[
        { label: t("settings.diagnostics.providerRequestTrace.recordedData"), value: t("settings.diagnostics.providerRequestTrace.recordedDataHelp") },
        { label: t("settings.diagnostics.providerRequestTrace.retention"), value: <>{t("settings.diagnostics.providerRequestTrace.retentionHelp")} <code>temp/provider-api-trace</code></> },
        { label: t("settings.diagnostics.providerRequestTrace.privacy"), value: t("settings.diagnostics.providerRequestTrace.privacyHelp"), tone: "attention" },
      ]} layout="list" />}
      guideCollapsed
      guideLabel={t("settings.diagnostics.providerRequestTrace.aboutTitle")}
      footer={<>
        <Button disabled={traceDirectoryBusy} onClick={() => void runTraceDirectoryAction(async () => {
          await props.actions.onOpenSelectedProviderApiTraceDirectory();
          return null;
        })} size="sm" type="button" variant="outline">
          <FolderOpen />
          {t("settings.diagnostics.providerRequestTrace.openDirectory")}
        </Button>
        <Button disabled={traceDirectoryBusy} onClick={() => setClearConfirmationOpen(true)} size="sm" type="button" variant="ghost">
          <Trash2 />
          {t("settings.diagnostics.providerRequestTrace.clearDirectory")}
        </Button>
      </>}
    >
      <PageRow description={t("settings.diagnostics.providerRequestTrace.completeRequests")} label={t("settings.diagnostics.providerRequestTrace.switch")}>
        <Switch
          aria-label={t("settings.diagnostics.providerRequestTrace.switch")}
          checked={props.enabled}
          disabled={savingTraceSetting}
          onCheckedChange={(enabled) => void handleTraceSettingChange(enabled)}
        />
      </PageRow>
      <PageRow description={t("settings.diagnostics.providerRequestTrace.errorsHelp")} label={t("settings.diagnostics.providerRequestTrace.errorsSwitch")}>
        <Switch
          aria-label={t("settings.diagnostics.providerRequestTrace.errorsSwitch")}
          checked={props.errorsEnabled}
          disabled={savingTraceSetting}
          onCheckedChange={(enabled) => void handleTraceSettingChange(props.enabled, enabled)}
        />
      </PageRow>
      {traceSettingError ? (
        <PageNotice details={traceSettingError} tone="error">{t("settings.diagnostics.providerRequestTrace.saveFailed")}</PageNotice>
      ) : null}
      {traceDirectoryNotice ? <PageNotice tone="info">{traceDirectoryNotice}</PageNotice> : null}
      <AlertDialog open={clearConfirmationOpen} onOpenChange={setClearConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.diagnostics.providerRequestTrace.clearDirectory")}</AlertDialogTitle>
            <AlertDialogDescription>{t("settings.diagnostics.providerRequestTrace.clearConfirm")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings.diagnostics.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={traceDirectoryBusy} variant="destructive" onClick={() => void runTraceDirectoryAction(async () => {
              const removed = await props.actions.onClearSelectedProviderApiTraceDirectory();
              return t(removed ? "settings.diagnostics.providerRequestTrace.clearSucceeded" : "settings.diagnostics.providerRequestTrace.clearEmpty");
            })}>{t("settings.diagnostics.confirmCleanup")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageSection>
  );
}
