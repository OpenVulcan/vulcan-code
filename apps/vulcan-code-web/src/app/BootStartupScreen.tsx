import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { StartupDownloadProgress } from "@/components/shell/startup-download-progress";
import type { StartupSnapshot } from "@/types";
import { activeStartupStep, startupProgressPercent } from "@/components/shell/startup-progress";

// BootLocale is the minimal language set needed before the full i18n bundle is loaded.
// BootLocale 是完整 i18n 包加载前所需的最小语言集合。
type BootLocale = "zh-CN" | "en-US";

// BootText stores the minimal boot-screen labels used by the startup gate.
// BootText 保存启动门使用的最小启动屏文案。
interface BootText {
  // Workspace loading title.
  // 工作区加载标题。
  loadingWorkspace: string;
  // Service connection detail.
  // 服务连接详情。
  connectingService: string;
  // Project index loading detail.
  // 项目索引加载详情。
  loadingProjectIndex: string;
  // Startup step title prefix.
  // 启动步骤标题前缀。
  loadingStepPrefix: string;
  // Startup failure prefix.
  // 启动失败前缀。
  failedPrefix: string;
  // Label for optional raw failure details, keeping the recovery action visible.
  // 可选原始错误详情的标签，让恢复操作保持可见。
  errorDetails: string;
  // Managed-runtime download progress label.
  // 受管运行时下载进度标签。
  downloadProgress: string;
  // Real completed step count label, independent from measured percentage.
  // 真实完成步骤计数标签，独立于实测百分比。
  completedSteps: string;
}

// BOOT_TEXT maps startup labels without importing the full application translation catalog.
// BOOT_TEXT 在不导入完整应用翻译表的情况下映射启动文案。
const BOOT_TEXT: Record<BootLocale, BootText> = {
  "zh-CN": {
    loadingWorkspace: "正在加载 Workspace",
    connectingService: "正在连接 Vulcan Core Service",
    loadingProjectIndex: "正在加载项目与会话索引",
    loadingStepPrefix: "正在加载",
    failedPrefix: "启动失败：",
    errorDetails: "查看详情",
    downloadProgress: "下载进度",
    completedSteps: "已完成 {completed}/{total} 个步骤",
  },
  "en-US": {
    loadingWorkspace: "Loading Workspace",
    connectingService: "Connecting to Vulcan Core Service",
    loadingProjectIndex: "Loading project and conversation indexes",
    loadingStepPrefix: "Loading",
    failedPrefix: "Startup failed: ",
    errorDetails: "View details",
    downloadProgress: "Download progress",
    completedSteps: "Completed {completed}/{total} steps",
  },
};

// BootLoadingScreen renders the fast startup gate with real startup progress.
// BootLoadingScreen 渲染带真实启动进度的快速启动门。
export function BootLoadingScreen(props: {
  // Startup snapshot received before the workbench bundle is loaded.
  // 工作台包加载前收到的启动快照。
  startup?: StartupSnapshot | null;
}) {
  // locale selects the minimal boot language before full i18n is available.
  // locale 在完整 i18n 可用前选择最小启动语言。
  const locale = bootLocale(props.startup?.uiLocale);
  // text stores localized labels for the boot screen.
  // text 保存启动屏的本地化文案。
  const text = BOOT_TEXT[locale];
  // activeStep selects the current startup row for the progress headline.
  // activeStep 选择当前启动步骤作为进度标题。
  const activeStep = props.startup ? activeStartupStep(props.startup.steps) : null;
  // percent converts startup step states into a bounded visual progress value.
  // percent 将启动步骤状态转换为有界视觉进度值。
  const percent = startupProgressPercent(props.startup ?? null);
  // message shows the current startup phase without loading the full workbench.
  // message 展示当前启动阶段，而不加载完整工作台。
  const message = activeStep?.message ?? (props.startup ? text.loadingProjectIndex : text.connectingService);
  // heading shows either the active startup step or the workspace loading title.
  // heading 展示当前启动步骤或工作区加载标题。
  const heading = activeStep ? `${text.loadingStepPrefix} ${activeStep.label}` : text.loadingWorkspace;
  return (
    <main className="loading-screen">
      <section className="startup-progress-panel" aria-live="polite" aria-busy={!props.startup?.ready && activeStep?.status !== "error"}>
        <StartupLogoMark />
        <div className="startup-progress-content">
          <div className="conversation-loading-heading startup-progress-heading">
            <strong>{heading}</strong>
            {percent !== null ? <span>{Math.round(percent)}%</span> : null}
          </div>
          <div className="conversation-loading-bar startup-progress-bar" aria-label={activeStep?.label ?? text.loadingWorkspace} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent !== null ? Math.round(percent) : undefined}>
            <div className={`conversation-loading-bar-value${percent === null ? " startup-download-progress-indeterminate" : ""}`} style={percent !== null ? { width: `${percent}%` } : undefined} />
          </div>
          {props.startup && props.startup.steps.length > 0 ? <small>{text.completedSteps
            .replace("{completed}", String(props.startup.steps.filter((step) => step.status === "ready" || step.status === "warning").length))
            .replace("{total}", String(props.startup.steps.length))}</small> : null}
          <p className="startup-progress-message" title={message} role={activeStep?.status === "error" ? "alert" : undefined}>{message}</p>
          <StartupDownloadProgress step={activeStep} label={text.downloadProgress} />
        </div>
      </section>
    </main>
  );
}

// BootErrorScreen renders a boot failure before the full workbench bundle is loaded.
// BootErrorScreen 在完整工作台包加载前渲染启动失败状态。
export function BootErrorScreen(props: {
  // Error message rendered after startup fails.
  // 启动失败后展示的错误消息。
  error: string;
  // Service-provided UI locale.
  // 服务提供的界面语言。
  uiLocale?: string | null;
  // onReload explicitly restarts the application only after the user chooses recovery.
  // onReload 仅在用户选择恢复后显式重新加载应用。
  onReload?: () => void;
}) {
  // text stores localized labels for the boot error.
  // text 保存启动错误的本地化文案。
  const text = BOOT_TEXT[bootLocale(props.uiLocale)];
  return (
    <main className="loading-screen error">
      <section className="startup-error-panel" aria-live="assertive">
        <StartupLogoMark />
        <div className="startup-error-content">
          <strong>{text.failedPrefix}</strong>
          <details className="startup-error-details">
            <summary>{text.errorDetails}</summary>
            <p className="startup-error-message">{props.error}</p>
          </details>
          {props.onReload ? <button className="startup-reload-button" type="button" onClick={props.onReload}>
            {bootLocale(props.uiLocale) === "zh-CN" ? "重新加载应用" : "Reload application"}
          </button> : null}
        </div>
      </section>
    </main>
  );
}

// bootLocale resolves the minimal startup locale without importing the full i18n module.
// bootLocale 在不导入完整 i18n 模块的情况下解析最小启动语言。
function bootLocale(value: string | null | undefined): BootLocale {
  if (value === "zh-CN" || value === "en-US") {
    return value;
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}
