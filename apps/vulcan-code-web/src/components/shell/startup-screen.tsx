import type { StartupSnapshot, StartupStepSnapshot } from "@/types";
import { CircleAlert } from "lucide-react";
import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { useI18n } from "@/i18n";

// LoadingScreen renders the earliest frontend loading state.
// LoadingScreen 渲染最早期的前端加载状态。
export function LoadingScreen(props: {
  // Startup snapshot received before the workbench snapshot is ready.
  // 在工作台快照就绪前已经收到的启动快照。
  startup?: StartupSnapshot | null;
}) {
  const { t } = useI18n();
  return (
    <main className="loading-screen">
      <StartupProgressPanel startup={props.startup ?? null} />
    </main>
  );
}

// StartupOverlay renders the in-shell bootstrap blocker below the main header.
// StartupOverlay 渲染位于主标题栏下方的壳内启动阻塞层。
export function StartupOverlay(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  return (
    <div className="startup-overlay" role="status" aria-live="polite" aria-busy="true">
      <StartupProgressPanel startup={props.startup} />
    </div>
  );
}

// StartupProgressPanel renders the centered bootstrap logo and progress bar.
// StartupProgressPanel 渲染居中的启动 Logo 与进度条。
function StartupProgressPanel(props: {
  // Latest service startup snapshot, or null while the service endpoint is still being discovered.
  // 最近一次服务启动快照；服务端点仍在发现时为 null。
  startup: StartupSnapshot | null;
}) {
  const { t } = useI18n();
  // activeStep selects the current startup row for the progress headline.
  // activeStep 选择当前启动步骤作为进度标题。
  const activeStep = props.startup ? activeStartupStep(props.startup.steps) : null;
  // percent converts startup step states into the same bounded progress model used by conversation loading.
  // percent 将启动步骤状态转换为会话加载同款的有界进度模型。
  const percent = startupProgressPercent(props.startup);
  // message shows the current startup phase without expanding the full step list.
  // message 展示当前启动阶段，而不展开完整步骤列表。
  const message = activeStep?.message ?? (props.startup ? t("startup.loadingProjectIndex") : t("startup.connectingService"));
  return (
    <section className="startup-progress-panel" aria-live="polite" aria-busy={!props.startup?.ready}>
      <StartupLogoMark />
      <div className="startup-progress-content">
        <div className="conversation-loading-heading startup-progress-heading">
          <strong>{activeStep ? t("startup.loadingStep").replace("{step}", activeStep.label) : t("startup.loadingWorkspace")}</strong>
          <span>{Math.round(percent)}%</span>
        </div>
        <div className="conversation-loading-bar startup-progress-bar" aria-label={t("startup.loadingWorkspace")} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
          <div className="conversation-loading-bar-value" style={{ width: `${percent}%` }} />
        </div>
        <p className="startup-progress-message">{message}</p>
      </div>
    </section>
  );
}

// activeStartupStep returns the first in-progress or pending startup step for the headline.
// activeStartupStep 返回首个进行中或待处理启动步骤，用于标题显示。
function activeStartupStep(steps: StartupStepSnapshot[]): StartupStepSnapshot | null {
  return steps.find((step) => step.status === "running")
    ?? steps.find((step) => step.status === "pending")
    ?? null;
}

// startupProgressPercent maps startup step states to a stable bounded progress percentage.
// startupProgressPercent 将启动步骤状态映射为稳定的有界进度百分比。
function startupProgressPercent(startup: StartupSnapshot | null): number {
  if (!startup) {
    return 8;
  }
  if (startup.ready) {
    return 100;
  }
  const total = startup.steps.length;
  if (total === 0) {
    return 8;
  }
  const completed = startup.steps.reduce((sum, step) => {
    if (step.status === "ready" || step.status === "warning") {
      return sum + 1;
    }
    if (step.status === "running") {
      return sum + 0.5;
    }
    return sum;
  }, 0);
  return Math.max(8, Math.min(96, (completed / total) * 100));
}

// ErrorScreen renders a startup failure.
// ErrorScreen 渲染启动失败状态。
export function ErrorScreen(props: {
  // Error message rendered after startup fails.
  // 启动失败后展示的错误消息。
  error: string;
}) {
  const { t } = useI18n();
  return (
    <main className="loading-screen error">
      <CircleAlert size={28} />
      <span>{t("startup.failed").replace("{error}", props.error)}</span>
    </main>
  );
}
