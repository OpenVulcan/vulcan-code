import { StartupLogoMark } from "@/components/shell/startup-logo-mark";
import { StartupDownloadProgress } from "@/components/shell/startup-download-progress";
import type { StartupRecoverySnapshot, StartupSnapshot, StartupStepSnapshot } from "@/types";

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
  // VMM rebuild action label.
  // VMM 重建操作标签。
  rebuildVmm: string;
  // Active VMM rebuild action label.
  // VMM 重建进行中操作标签。
  rebuildingVmm: string;
  // VMM rebuild scope explanation.
  // VMM 重建范围说明。
  rebuildVmmHint: string;
  // Managed-runtime download progress label.
  // 受管运行时下载进度标签。
  downloadProgress: string;
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
    rebuildVmm: "丢弃损坏的 VMM 记忆并重建",
    rebuildingVmm: "正在重建 VMM 状态…",
    rebuildVmmHint: "仅重建此会话的 VMM 记忆状态；会话消息、工具记录与项目文件都会保留。",
    downloadProgress: "下载进度",
  },
  "en-US": {
    loadingWorkspace: "Loading Workspace",
    connectingService: "Connecting to Vulcan Core Service",
    loadingProjectIndex: "Loading project and conversation indexes",
    loadingStepPrefix: "Loading",
    failedPrefix: "Startup failed: ",
    rebuildVmm: "Discard damaged VMM memory and rebuild",
    rebuildingVmm: "Rebuilding VMM state…",
    rebuildVmmHint: "Only this conversation's VMM memory state is rebuilt. Messages, tool records, and project files are preserved.",
    downloadProgress: "Download progress",
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
      <section className="startup-progress-panel" aria-live="polite" aria-busy={!props.startup?.ready}>
        <StartupLogoMark />
        <div className="startup-progress-content">
          <div className="conversation-loading-heading startup-progress-heading">
            <strong>{heading}</strong>
            <span>{Math.round(percent)}%</span>
          </div>
          <div className="conversation-loading-bar startup-progress-bar" aria-label={text.loadingWorkspace} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
            <div className="conversation-loading-bar-value" style={{ width: `${percent}%` }} />
          </div>
          <p className="startup-progress-message">{message}</p>
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
  // Optional service-classified recovery action.
  // 可选的服务端已分类恢复操作。
  recovery?: StartupRecoverySnapshot | null;
  // Whether the recovery request is currently running.
  // 恢复请求当前是否正在执行。
  recovering?: boolean;
  // Concrete failure from the most recent recovery attempt.
  // 最近一次恢复尝试产生的具体失败。
  recoveryError?: string | null;
  // Executes the service-authorized recovery action.
  // 执行服务端授权的恢复操作。
  onRecover?: () => void;
}) {
  // text stores localized labels for the boot error.
  // text 保存启动错误的本地化文案。
  const text = BOOT_TEXT[bootLocale(props.uiLocale)];
  // canRebuildVmm prevents generic startup failures from exposing a destructive action.
  // canRebuildVmm 防止普通启动失败暴露破坏性操作。
  const canRebuildVmm = props.recovery?.kind === "rebuildSessionVmm" && Boolean(props.onRecover);
  return (
    <main className="loading-screen error">
      <section className="startup-error-panel" aria-live="assertive">
        <StartupLogoMark />
        <div className="startup-error-content">
          <strong>{text.failedPrefix}</strong>
          <p className="startup-error-message">{props.error}</p>
          {canRebuildVmm ? (
            <>
              <p className="startup-recovery-hint">{text.rebuildVmmHint}</p>
              <button
                className="startup-recovery-action"
                disabled={props.recovering}
                onClick={props.onRecover}
                type="button"
              >
                {props.recovering ? text.rebuildingVmm : text.rebuildVmm}
              </button>
              {props.recoveryError ? (
                <p className="startup-recovery-error">{props.recoveryError}</p>
              ) : null}
            </>
          ) : null}
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
      return sum + Math.max(0, Math.min(100, step.progressPercent ?? 50)) / 100;
    }
    return sum;
  }, 0);
  return Math.max(8, Math.min(96, (completed / total) * 100));
}
