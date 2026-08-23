import {
  Bug,
  History,
  PlugZap,
} from "lucide-react";
import * as React from "react";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { PanelTitle, StatusIcon, StatusPill } from "@/components/shared/status";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import type {
  LogEntrySummary,
  StartupSnapshot,
} from "@/types";

// LogsArea renders startup and diagnostic logs.
// LogsArea 渲染启动与诊断日志。
export function LogsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps;
  // Whether bounded wire-request tracing is currently enabled.
  // 当前是否启用有界线上请求追踪。
  providerRequestTraceEnabled: boolean;
  // Startup snapshot shown in diagnostics.
  // 诊断中展示的启动快照。
  startup: StartupSnapshot;
  // Diagnostic log entries shown beside startup.
  // 启动状态旁展示的诊断日志。
  logs: LogEntrySummary[];
}) {
  const { t } = useI18n();
  const [savingTraceSetting, setSavingTraceSetting] = React.useState(false);
  const [traceSettingError, setTraceSettingError] = React.useState<string | null>(null);

  // handleTraceSettingChange persists one explicit diagnostic switch selection.
  // handleTraceSettingChange 持久化一次显式诊断开关选择。
  async function handleTraceSettingChange(enabled: boolean): Promise<void> {
    setSavingTraceSetting(true);
    setTraceSettingError(null);
    try {
      await props.actions.onSaveProviderRequestTraceSettings(enabled);
    } catch (error) {
      setTraceSettingError(error instanceof Error ? error.message : String(error));
    } finally {
      setSavingTraceSetting(false);
    }
  }

  return (
    <div className="area-grid two-columns">
      <StartupPanel startup={props.startup} />
      <section className="panel">
        <PanelTitle icon={<History size={17} />} title={t("settings.diagnostics.logs")} />
        <div className="log-list">
          {props.logs.map((log) => (
            <article className="log-row" key={log.id}>
              <StatusPill status={log.level === "info" ? "ready" : "warning"} label={log.level} />
              <strong>{log.source}</strong>
              <span>{log.message}</span>
            </article>
          ))}
        </div>
      </section>
      <section className="panel">
        <PanelTitle icon={<Bug size={17} />} title={t("settings.diagnostics.providerRequestTrace")} />
        <div className="settings-form-grid">
          <article className="settings-card">
            <StatusPill
              status={props.providerRequestTraceEnabled ? "warning" : "ready"}
              label={props.providerRequestTraceEnabled
                ? t("settings.diagnostics.providerRequestTrace.enabled")
                : t("settings.diagnostics.providerRequestTrace.disabled")}
            />
            <strong>{t("settings.diagnostics.providerRequestTrace.completeRequests")}</strong>
            <p>{t("settings.diagnostics.providerRequestTrace.description")}</p>
            <label className="settings-form-field">
              <span>{t("settings.diagnostics.providerRequestTrace.switch")}</span>
              <Switch
                checked={props.providerRequestTraceEnabled}
                disabled={savingTraceSetting}
                onCheckedChange={(enabled) => void handleTraceSettingChange(enabled)}
              />
            </label>
            {traceSettingError ? (
              <p className="settings-inline-error">
                {t("settings.diagnostics.providerRequestTrace.saveFailed")}: {traceSettingError}
              </p>
            ) : null}
          </article>
        </div>
      </section>
    </div>
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
    <section className="panel startup-panel">
      <PanelTitle icon={<PlugZap size={17} />} title={t("settings.diagnostics.startupStatus")} />
      <div className="step-list">
        {props.startup.steps.map((step) => (
          <div className="step-item" key={step.id}>
            <StatusIcon status={step.status} />
            <div>
              <strong>{step.label}</strong>
              <span>{step.message ?? t("settings.diagnostics.waitingStatus")}</span>
            </div>
          </div>
        ))}
      </div>
      <div className="log-preview">
        {props.startup.recentLogs.map((line) => (
          <code key={line}>{line}</code>
        ))}
      </div>
    </section>
  );
}
