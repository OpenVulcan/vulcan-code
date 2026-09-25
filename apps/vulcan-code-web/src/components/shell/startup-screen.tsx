import type { StartupSnapshot } from "@/types";
import { activeStartupStep, startupProgressPercent } from "./startup-progress";
import { BootErrorScreen } from "@/app/boot-startup-screen";
import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { StartupDownloadProgress } from "@/components/shell/startup-download-progress";
import { useI18n } from "@/i18n";

// LoadingScreen renders the earliest frontend loading state.
// LoadingScreen 渲染最早期的前端加载状态。
// Parameters: props supplies the optional startup snapshot; returns the early loading view using only measured progress.
// 参数：props 提供可选启动快照；返回仅使用实测进度的早期加载界面。
export function LoadingScreen(props: {
  // Startup snapshot received before the workbench snapshot is ready.
  // 在工作台快照就绪前已经收到的启动快照。
  startup?: StartupSnapshot | null;
}) {
  return (
    <main className="loading-screen">
      <StartupProgressPanel startup={props.startup ?? null} />
    </main>
  );
}

// StartupOverlay renders the in-shell bootstrap blocker below the main header.
// StartupOverlay 渲染位于主标题栏下方的壳内启动阻塞层。
// Parameters: props supplies the startup snapshot; returns the in-shell progress blocker below the existing header.
// 参数：props 提供启动快照；返回既有标题栏下方的壳内进度阻塞层。
export function StartupOverlay(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  return (
    <div className="startup-overlay" role="status" aria-live="polite">
      <StartupProgressPanel startup={props.startup} />
    </div>
  );
}

// StartupProgressPanel renders the centered bootstrap logo and progress bar.
// StartupProgressPanel 渲染居中的启动 Logo 与进度条。
// Parameters: props supplies the latest startup state; returns prioritized step errors, measured/unknown progress and independent completed-step counts.
// 参数：props 提供最新启动状态；返回优先步骤错误、实测或未知进度及独立完成步骤计数。
function StartupProgressPanel(props: {
  // Latest service startup snapshot, or null while the service endpoint is still being discovered.
  // 最近一次服务启动快照；服务端点仍在发现时为 null。
  startup: StartupSnapshot | null;
}) {
  const { t } = useI18n();
  // activeStep selects the current startup row for the progress headline.
  // activeStep 选择当前启动步骤作为进度标题。
  const activeStep = props.startup ? activeStartupStep(props.startup.steps) : null;
  // percent preserves the active step's real progress; missing upstream measurements remain unknown.
  // percent 保留活动步骤的真实进度；缺失的上游测量保持未知。
  const percent = startupProgressPercent(props.startup);
  // completedSteps counts finished or warning-complete steps without assigning guessed durations.
  // completedSteps 统计已完成或带警告完成的步骤，不分配猜测的耗时。
  const completedSteps = props.startup?.steps.filter((step) => step.status === "ready" || step.status === "warning").length ?? 0;
  // message shows the current startup phase without expanding the full step list.
  // message 展示当前启动阶段，而不展开完整步骤列表。
  const message = activeStep?.message ?? (props.startup ? t("startup.loadingProjectIndex") : t("startup.connectingService"));
  return (
    <section className="startup-progress-panel" aria-live="polite" aria-busy={!props.startup?.ready && activeStep?.status !== "error"}>
      <StartupLogoMark />
      <div className="startup-progress-content">
        <div className="conversation-loading-heading startup-progress-heading">
          <strong>{activeStep ? t("startup.loadingStep").replace("{step}", activeStep.label) : t("startup.loadingWorkspace")}</strong>
          {percent !== null ? <span>{Math.round(percent)}%</span> : null}
        </div>
        <div className="conversation-loading-bar startup-progress-bar" aria-label={activeStep?.label ?? t("startup.loadingWorkspace")} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent !== null ? Math.round(percent) : undefined}>
          <div className={`conversation-loading-bar-value${percent === null ? " startup-download-progress-indeterminate" : ""}`} style={percent !== null ? { width: `${percent}%` } : undefined} />
        </div>
        {props.startup && props.startup.steps.length > 0 ? <small>{t("workspaceAudit.startupSteps")
          .replace("{completed}", String(completedSteps)).replace("{total}", String(props.startup.steps.length))}</small> : null}
        <p className="startup-progress-message" title={message} role={activeStep?.status === "error" ? "alert" : undefined}>{message}</p>
        <StartupDownloadProgress step={activeStep} label={t("startup.downloadProgress")} />
      </div>
    </section>
  );
}

// ErrorScreen renders a startup failure.
// ErrorScreen 渲染启动失败状态。
// Parameters: props supplies the startup error; returns the shared disclosure and explicit reload action.
// 参数：props 提供启动错误；返回共用详情展开与显式重新加载操作。
export function ErrorScreen(props: {
  // Error message rendered after startup fails.
  // 启动失败后展示的错误消息。
  error: string;
}) {
  const { locale } = useI18n();
  return <BootErrorScreen error={props.error} uiLocale={locale} onReload={() => window.location.reload()} />;
}
