import type { StartupSnapshot } from "@/types";
import { BootErrorScreen } from "@/app/boot-startup-screen";
import { StartupProgressPanel, type StartupProgressLabels } from "@/components/shell/startup-progress-panel";
import { useI18n } from "@/i18n";

// StartupOverlay renders the in-shell bootstrap blocker below the main header. The first load is covered by the
// persistent boot surface (see WebuiApp); this overlay only appears when the service restarts under a mounted shell.
// StartupOverlay 渲染位于主标题栏下方的壳内启动阻塞层。首次加载由常驻启动层覆盖（见 WebuiApp）；
// 本遮罩仅在已挂载外壳下服务重启时出现。
// Parameters: props supplies the startup snapshot; returns the in-shell progress blocker below the existing header.
// 参数：props 提供启动快照；返回既有标题栏下方的壳内进度阻塞层。
export function StartupOverlay(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  const { t } = useI18n();
  // labels maps the full i18n catalog onto the shared startup panel copy.
  // labels 将完整 i18n 文案映射为共用启动面板文案。
  const labels: StartupProgressLabels = {
    loadingWorkspace: t("startup.loadingWorkspace"),
    connectingService: t("startup.connectingService"),
    loadingProjectIndex: t("startup.loadingProjectIndex"),
    loadingStep: t("startup.loadingStep"),
    completedSteps: t("workspaceAudit.startupSteps"),
    downloadProgress: t("startup.downloadProgress"),
  };
  return (
    <div className="vk-startup-overlay" role="status" aria-live="polite">
      <StartupProgressPanel startup={props.startup} labels={labels} />
    </div>
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
