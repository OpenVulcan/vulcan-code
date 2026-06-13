import {
  History,
  PlugZap,
} from "lucide-react";
import { PanelTitle, StatusIcon, StatusPill } from "@/components/shared/status";
import type {
  LogEntrySummary,
  StartupSnapshot,
} from "@/types";

// LogsArea renders startup and diagnostic logs.
// LogsArea 渲染启动与诊断日志。
export function LogsArea(props: {
  // Startup snapshot shown in diagnostics.
  // 诊断中展示的启动快照。
  startup: StartupSnapshot;
  // Diagnostic log entries shown beside startup.
  // 启动状态旁展示的诊断日志。
  logs: LogEntrySummary[];
}) {
  return (
    <div className="area-grid two-columns">
      <StartupPanel startup={props.startup} />
      <section className="panel">
        <PanelTitle icon={<History size={17} />} title="诊断日志" />
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
  return (
    <section className="panel startup-panel">
      <PanelTitle icon={<PlugZap size={17} />} title="启动状态" />
      <div className="step-list">
        {props.startup.steps.map((step) => (
          <div className="step-item" key={step.id}>
            <StatusIcon status={step.status} />
            <div>
              <strong>{step.label}</strong>
              <span>{step.message ?? "等待状态更新"}</span>
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
