import React from "react";
import { BootErrorScreen, BootLoadingScreen } from "@/app/boot-startup-screen";
import { RenderErrorBoundary } from "./render-error-boundary";
import type { StartupSnapshot } from "@/types";
import {
  hasServiceRuntime,
  serviceBootstrapErrorIsRetryable,
} from "@/service/webui-service-runtime";
import { loadStartupSnapshot } from "@/service/workbench-snapshot-service";
import { ensureLocaleCatalog } from "@/i18n-catalogs";
import { resolveAppLocale } from "@/i18n-locales";

// LazyWorkbenchApp loads the full workbench only after the startup gate reports readiness.
// LazyWorkbenchApp 仅在启动门报告就绪后加载完整工作台。
const LazyWorkbenchApp = React.lazy(() => import("./workbench-app").then((module) => ({ default: module.WorkbenchApp })));

// BootResult stores the startup snapshot that allowed the real workbench to mount.
// BootResult 保存允许真实工作台挂载的启动快照。
interface BootResult {
  // Ready startup snapshot captured by the isolated boot gate.
  // 隔离启动门捕获的已就绪启动快照。
  startup: StartupSnapshot;
}

// WebuiApp renders the isolated boot gate before mounting the real workbench runtime. One persistent startup surface
// covers every loading phase (boot probing, locale download, workbench chunk download, first workbench snapshot) from
// a fixed tree position, so the loading screen is never remounted and its logo motion plays exactly once.
// WebuiApp 在挂载真实工作台运行时前渲染隔离启动门。一个常驻启动层从固定的树位置覆盖全部加载阶段
// （启动探测、语言下载、工作台分块下载、首份工作台快照），因此加载界面不会被重新挂载，标志动效只播放一次。
export function WebuiApp() {
  // bootResult stores the ready startup snapshot that unlocks the workbench app.
  // bootResult 保存解锁工作台应用的已就绪启动快照。
  const [bootResult, setBootResult] = React.useState<BootResult | null>(null);
  // workbenchStartup is undefined until the workbench reports, the snapshot to show while it is still loading, and
  // null once the workbench (or its failure screen) owns the screen.
  // workbenchStartup 在工作台报告前为 undefined，工作台仍在加载时为要展示的快照，工作台（或其失败界面）接管屏幕后为 null。
  const [workbenchStartup, setWorkbenchStartup] = React.useState<StartupSnapshot | null | undefined>(undefined);
  // surfaceRetired turns true after the hand-off fade, unmounting the startup surface for good.
  // surfaceRetired 在交接淡出后变为 true，永久卸载启动层。
  const [surfaceRetired, setSurfaceRetired] = React.useState(false);
  // handleReady downloads the display language while the boot screen is still visible, so the workbench
  // never flashes the English fallback; a failed download still mounts the workbench in English.
  // handleReady 在启动界面仍可见时下载显示语言，使工作台不会闪现英文回退；下载失败时仍以英文挂载工作台。
  const handleReady = React.useCallback((result: BootResult) => {
    void ensureLocaleCatalog(resolveAppLocale(result.startup.uiLocale))
      .catch(() => undefined)
      .finally(() => setBootResult(result));
  }, []);
  const boot = useBootProbe(handleReady);
  // releaseSurface hands the screen to a workbench failure screen that never reported through WorkbenchApp.
  // releaseSurface 将屏幕交给未经 WorkbenchApp 报告的工作台失败界面。
  const releaseSurface = React.useCallback(() => setWorkbenchStartup(null), []);
  // retireSurface unmounts the startup surface once its fade-out finished.
  // retireSurface 在淡出完成后卸载启动层。
  const retireSurface = React.useCallback(() => setSurfaceRetired(true), []);

  // bootFailure is the terminal boot error that replaces the startup surface before the workbench exists.
  // bootFailure 是工作台存在之前取代启动层的终态启动错误。
  const bootFailure = bootResult ? null
    : boot.error ?? (boot.startup?.phase === "failed" ? startupFailureMessage(boot.startup) : null);
  // screen is the phase-specific content beneath the startup surface.
  // screen 是启动层下方随阶段变化的内容。
  let screen: React.ReactNode = null;
  if (bootFailure !== null) {
    screen = <BootErrorScreen error={bootFailure} uiLocale={boot.startup?.uiLocale} onReload={() => window.location.reload()} />;
  } else if (bootResult) {
    screen = (
      <RenderErrorBoundary fallback={(error) => (
        <ReleaseStartupSurface onRelease={releaseSurface}>
          <BootErrorScreen error={error.message} uiLocale={bootResult.startup.uiLocale} onReload={() => window.location.reload()} />
        </ReleaseStartupSurface>
      )}>
        <React.Suspense fallback={null}>
          <LazyWorkbenchApp initialStartup={bootResult.startup} onStartupSurfaceChange={setWorkbenchStartup} />
        </React.Suspense>
      </RenderErrorBoundary>
    );
  }
  // The surface keeps the second fragment slot in every phase, so React reuses the same DOM across phase changes.
  // 启动层在每个阶段都占据片段的第二个位置，使 React 在阶段切换时复用同一 DOM。
  return (
    <>
      {screen}
      {bootFailure !== null || surfaceRetired ? null : (
        <BootLoadingScreen
          startup={workbenchStartup ?? bootResult?.startup ?? boot.startup}
          leaving={workbenchStartup === null}
          onLeft={retireSurface}
        />
      )}
    </>
  );
}

// ReleaseStartupSurface releases the startup surface as soon as a workbench failure screen is committed.
// ReleaseStartupSurface 在工作台失败界面提交后立即释放启动层。
function ReleaseStartupSurface(props: {
  // Called once the failure screen is in the document.
  // 失败界面进入文档后调用。
  onRelease: () => void;
  // Failure screen revealed beneath the fading surface.
  // 在淡出的启动层下方显露的失败界面。
  children: React.ReactNode;
}) {
  const { onRelease } = props;
  React.useLayoutEffect(() => {
    onRelease();
  }, [onRelease]);
  return props.children;
}

// BootProbeState is the latest startup observation of the isolated boot probe.
// BootProbeState 是隔离启动探测的最新启动观测。
interface BootProbeState {
  // Latest startup snapshot, or null before the service answered.
  // 最近一次启动快照；服务应答前为 null。
  startup: StartupSnapshot | null;
  // Terminal boot error, or null while probing can continue.
  // 终态启动错误；探测可继续时为 null。
  error: string | null;
}

// useBootProbe owns startup probing without creating workbench state, subscriptions, or reconnect toasts; it stops
// polling once the service reports full readiness.
// useBootProbe 持有启动探测，但不创建工作台状态、订阅或重连提示；服务报告完全就绪后停止轮询。
function useBootProbe(
  // Called once the service reports full startup readiness.
  // 服务报告完全启动就绪后调用。
  onReady: (result: BootResult) => void,
): BootProbeState {
  // startup stores the latest startup snapshot shown by the isolated boot screen.
  // startup 保存隔离启动界面展示的最近启动快照。
  const [startup, setStartup] = React.useState<StartupSnapshot | null>(null);
  // error stores a terminal boot error.
  // error 保存终态启动错误。
  const [error, setError] = React.useState<string | null>(null);
  // onReadyRef keeps the latest callback without restarting the boot probe effect.
  // onReadyRef 保存最新回调，同时避免重启动启动探测 effect。
  const onReadyRef = React.useRef(onReady);

  React.useEffect(() => {
    onReadyRef.current = onReady;
  }, [onReady]);

  React.useEffect(() => {
    let cancelled = false;
    let retryTimer: number | null = null;
    let retryDelayMs = 250;

    // scheduleBootProbe queues the next startup-only probe while the service is not ready.
    // scheduleBootProbe 在服务未就绪时排队下一次仅启动探测。
    function scheduleBootProbe() {
      if (cancelled || retryTimer !== null) {
        return;
      }
      const delay = retryDelayMs;
      retryDelayMs = Math.min(retryDelayMs * 2, 1500);
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        void probeStartup();
      }, delay);
    }

    // probeStartup reads only startup state so boot-time fallback workbench snapshots cannot leak into the app.
    // probeStartup 只读取启动状态，避免启动期回退工作台快照泄漏进正式应用。
    async function probeStartup() {
      try {
        const nextStartup = await loadStartupSnapshot();
        if (cancelled) {
          return;
        }
        setStartup(nextStartup);
        setError(null);
        if (nextStartup.ready) {
          onReadyRef.current({ startup: nextStartup });
          return;
        }
        retryDelayMs = 500;
        scheduleBootProbe();
      } catch (nextError) {
        if (cancelled) {
          return;
        }
        if (hasServiceRuntime() && serviceBootstrapErrorIsRetryable(nextError)) {
          setError(null);
          scheduleBootProbe();
          return;
        }
        setError(nextError instanceof Error ? nextError.message : String(nextError));
      }
    }

    void probeStartup();
    return () => {
      cancelled = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, []);

  return { startup, error };
}

// startupFailureMessage resolves the failed startup step before falling back to retained service logs.
// startupFailureMessage 优先解析失败启动步骤，随后回退到服务端保留日志。
function startupFailureMessage(startup: StartupSnapshot): string {
  return startup.steps.find((step) => step.status === "error")?.message
    ?? startup.recentLogs[0]
    ?? "Unknown startup failure";
}

