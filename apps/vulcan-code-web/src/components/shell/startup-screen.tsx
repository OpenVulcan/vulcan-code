import { useEffect, useRef } from "react";
import type { StartupSnapshot } from "@/types";
import { BootErrorScreen, useBootText } from "@/app/boot-startup-screen";
import { StartupProgressPanel } from "@/components/shell/startup-progress-panel";
import { useI18n } from "@/i18n";
import type { LocaleCode } from "@/i18n-messages";
import { toast } from "@/ui-kit/primitives/toast";

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
  const { locale } = useI18n();
  // The shared startup surface displays the current locale after its small catalog arrives.
  // 小型目录到达后，共用启动层显示当前语言。
  const text = useBootText(locale);
  return (
    <div className="vk-startup-overlay" role="status" aria-live="polite">
      {text ? <StartupProgressPanel startup={props.startup} labels={text} /> : null}
    </div>
  );
}

// Identifies the dismissible warning for the current service startup.
// 标识当前服务启动对应的可关闭警告。
const STARTUP_DEPENDENCY_WARNING_ID = "startup-luaskills-dependency-failure";

/** Shows the dependency warning for a ready startup snapshot in the workbench locale.
 * 对已就绪的启动快照，使用工作台语言显示依赖警告。 */
export function useStartupDependencyWarning(startup: StartupSnapshot, locale: LocaleCode): void {
  // The last shown set suppresses repeated startup RPC and event snapshots.
  // 已显示的失败集合用于去重启动 RPC 和事件快照。
  const shown = useRef<string | null>(null);
  // The warning waits for the selected language before publishing the notification.
  // 警告在选定语言就绪后再发布通知。
  const text = useBootText(locale);
  useEffect(() => {
    const failures = startup.luaskillsDependencyFailures;
    if (!startup.ready || !failures?.length) {
      if (shown.current !== null) toast.dismiss(STARTUP_DEPENDENCY_WARNING_ID);
      shown.current = null;
      return;
    }
    if (!text) return;
    // The server returns a stable ordered list of skill IDs.
    // 服务返回顺序稳定的技能标识列表。
    const skills = failures.join(", ");
    if (shown.current === skills) return;
    shown.current = skills;
    toast.warning(text.dependencyFailure.replace("{skills}", skills), {
      id: STARTUP_DEPENDENCY_WARNING_ID,
      description: text.githubMirrorHint,
      duration: Infinity,
    });
  }, [startup.ready, startup.luaskillsDependencyFailures, text]);
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
