import React from "react";
import { BootErrorScreen, BootLoadingScreen } from "@/app/boot-startup-screen";
import { RenderErrorBoundary } from "./render-error-boundary";
import type { StartupSnapshot } from "@/types";
import {
  hasServiceRuntime,
  serviceBootstrapErrorIsRetryable,
} from "@/service/webui-service-runtime";
import { loadStartupSnapshot } from "@/service/workbench-snapshot-service";

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

// WebuiApp renders the isolated boot gate before mounting the real workbench runtime.
// WebuiApp 在挂载真实工作台运行时前渲染隔离启动门。
export function WebuiApp() {
  // bootResult stores the ready startup snapshot that unlocks the workbench app.
  // bootResult 保存解锁工作台应用的已就绪启动快照。
  const [bootResult, setBootResult] = React.useState<BootResult | null>(null);
  if (!bootResult) {
    return <BootGate onReady={setBootResult} />;
  }
  return (
    <RenderErrorBoundary fallback={(error) => <BootErrorScreen error={error.message} uiLocale={bootResult.startup.uiLocale} onReload={() => window.location.reload()} />}>
    <React.Suspense fallback={(
      <BootLoadingScreen startup={bootResult.startup} />
    )}>
      <LazyWorkbenchApp initialStartup={bootResult.startup} />
    </React.Suspense>
    </RenderErrorBoundary>
  );
}

// BootGate owns startup probing without creating workbench state, subscriptions, or reconnect toasts.
// BootGate 持有启动探测，但不创建工作台状态、订阅或重连提示。
function BootGate(props: {
  // Called once the service reports full startup readiness.
  // 服务报告完全启动就绪后调用。
  onReady: (result: BootResult) => void;
}) {
  // startup stores the latest startup snapshot shown by the isolated boot screen.
  // startup 保存隔离启动界面展示的最近启动快照。
  const [startup, setStartup] = React.useState<StartupSnapshot | null>(null);
  // error stores a terminal boot error.
  // error 保存终态启动错误。
  const [error, setError] = React.useState<string | null>(null);
  // onReadyRef keeps the latest callback without restarting the boot probe effect.
  // onReadyRef 保存最新回调，同时避免重启动启动探测 effect。
  const onReadyRef = React.useRef(props.onReady);

  React.useEffect(() => {
    onReadyRef.current = props.onReady;
  }, [props.onReady]);

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

  if (error) {
    return <BootErrorScreen error={error} uiLocale={startup?.uiLocale} onReload={() => window.location.reload()} />;
  }

  if (startup?.phase === "failed") {
    return (
      <BootErrorScreen
        error={startupFailureMessage(startup)}
        uiLocale={startup.uiLocale}
        onReload={() => window.location.reload()}
      />
    );
  }

  return <BootLoadingScreen startup={startup} />;
}

// startupFailureMessage resolves the failed startup step before falling back to retained service logs.
// startupFailureMessage 优先解析失败启动步骤，随后回退到服务端保留日志。
function startupFailureMessage(startup: StartupSnapshot): string {
  return startup.steps.find((step) => step.status === "error")?.message
    ?? startup.recentLogs[0]
    ?? "Unknown startup failure";
}

