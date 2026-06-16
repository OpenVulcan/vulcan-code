import React from "react";
import {
  ErrorScreen,
  LoadingScreen,
} from "@/components/shell/startup-screen";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  WorkbenchLayout,
  type WorkbenchDeltaSignal,
} from "@/components/workspace/workbench-layout";
import { I18nProvider } from "@/i18n";
import type {
  StartupSnapshot,
  SessionAddonChangedEvent,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  type BrowserServiceConnectionDetail,
} from "@/service";
import {
  BROWSER_SERVICE_CONNECTION_TOAST_ID,
  hasServiceRuntime,
  serviceBootstrapErrorIsRetryable,
  serviceClient,
} from "@/service/webui-service-runtime";
import {
  loadStartupSnapshot,
  loadWorkbenchSnapshot,
} from "@/service/workbench-snapshot-service";
import { coalesceWorkbenchDeltaEnvelopes } from "@/state/workbench-state";
import { toast } from "sonner";

// STREAMING_DELTA_FLUSH_MS batches hot transcript deltas so React and GC are not driven at display-frame cadence.
// STREAMING_DELTA_FLUSH_MS 将高频转录增量批处理，避免 React 与 GC 被显示帧率牵引。
const STREAMING_DELTA_FLUSH_MS = 80;

// WebuiApp renders the Vulcan Code WebUI workbench and owns startup subscriptions.
// WebuiApp 渲染 Vulcan Code WebUI 工作台并持有启动订阅。
export function WebuiApp() {
  // startup stores the latest startup snapshot.
  // startup 保存最近一次启动快照。
  const [startup, setStartup] = React.useState<StartupSnapshot | null>(null);
  // workbench stores the latest workbench snapshot.
  // workbench 保存最近一次工作台快照。
  const [workbench, setWorkbench] = React.useState<WorkbenchSnapshot | null>(null);
  // workbenchDelta stores the latest partial workbench update signal.
  // workbenchDelta 保存最近一次局部工作台更新信号。
  const [workbenchDelta, setWorkbenchDelta] = React.useState<WorkbenchDeltaSignal | null>(null);
  // workbenchDeltaBufferRef batches hot-path deltas until the next scheduled flush.
  // workbenchDeltaBufferRef 将热路径增量批处理到下一次计划刷新。
  const workbenchDeltaBufferRef = React.useRef<WorkbenchDeltaEnvelope[]>([]);
  // workbenchDeltaTimerRef stores the scheduled delta flush timer id.
  // workbenchDeltaTimerRef 保存已调度的增量刷新定时器标识。
  const workbenchDeltaTimerRef = React.useRef<number | null>(null);
  // error stores the latest loading error.
  // error 保存最近一次加载错误。
  const [error, setError] = React.useState<string | null>(null);
  // serviceWasDisconnectedRef suppresses recovery toasts before the first real disconnect.
  // serviceWasDisconnectedRef 在首次真实断开前抑制恢复提示。
  const serviceWasDisconnectedRef = React.useRef(false);
  // workbenchSnapshotSeqRef rejects stale full-snapshot updates across every snapshot source.
  // workbenchSnapshotSeqRef 在所有快照来源之间拒绝过期的全量快照更新。
  const workbenchSnapshotSeqRef = React.useRef(0);

  React.useEffect(() => {
    // handleConnectionStatusChange maps transport-level connection changes to user-visible toasts.
    // handleConnectionStatusChange 将传输层连接变化映射为用户可见的提示。
    function handleConnectionStatusChange(event: Event) {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (!detail) {
        return;
      }
      if (detail.status === "connected") {
        if (serviceWasDisconnectedRef.current) {
          serviceWasDisconnectedRef.current = false;
          toast.success("本地服务连接已恢复。", {
            id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
            duration: 2200,
          });
        }
        return;
      }
      serviceWasDisconnectedRef.current = true;
      if (detail.status === "reconnecting") {
        toast.loading("本地服务连接已断开，正在重连...", {
          id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
        });
        return;
      }
      toast.loading("本地服务连接已断开，正在检测恢复...", {
        id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
        description: detail.message,
      });
    }

    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatusChange);
    return () => {
      window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatusChange);
    };
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    let retryTimer: number | null = null;
    let retryDelayMs = 500;

    // scheduleBootstrapRetry retries service discovery after transient startup races.
    // scheduleBootstrapRetry 在瞬时启动竞态后重试服务发现。
    function scheduleBootstrapRetry(error: unknown): boolean {
      if (!hasServiceRuntime() || !serviceBootstrapErrorIsRetryable(error)) {
        return false;
      }
      if (retryTimer !== null) {
        return true;
      }
      const delay = retryDelayMs;
      retryDelayMs = Math.min(retryDelayMs * 2, 5000);
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        void bootstrap();
      }, delay);
      return true;
    }

    // bootstrap loads the first startup and workbench snapshots.
    // bootstrap 加载第一份启动和工作台快照。
    async function bootstrap() {
      try {
        if (hasServiceRuntime()) {
          const subscriptions = await Promise.all([
            serviceClient.subscribe<StartupSnapshot>("startup", (payload) => {
              if (!cancelled) {
                setStartup(payload);
              }
            }),
            serviceClient.subscribe<WorkbenchSnapshot>("workbench", (payload) => {
              if (!cancelled) {
                workbenchSnapshotSeqRef.current += 1;
                setWorkbench(payload);
              }
            }),
            serviceClient.subscribe<WorkbenchDeltaEnvelope>("workbench.delta", (payload) => {
              if (cancelled) {
                return;
              }
              workbenchDeltaBufferRef.current.push(payload);
              if (workbenchDeltaTimerRef.current === null) {
                workbenchDeltaTimerRef.current = window.setTimeout(() => {
                  const envelopes = coalesceWorkbenchDeltaEnvelopes(workbenchDeltaBufferRef.current);
                  workbenchDeltaBufferRef.current = [];
                  workbenchDeltaTimerRef.current = null;
                  if (envelopes.length > 0) {
                    setWorkbenchDelta((current) => ({
                      seq: (current?.seq ?? 0) + 1,
                      envelopes,
                    }));
                  }
                }, STREAMING_DELTA_FLUSH_MS);
              }
            }),
            serviceClient.subscribe<SessionAddonChangedEvent>("session.addon.changed", () => {
              if (cancelled) {
                return;
              }
              const refreshSeq = workbenchSnapshotSeqRef.current + 1;
              workbenchSnapshotSeqRef.current = refreshSeq;
              void loadWorkbenchSnapshot()
                .then((snapshot) => {
                  if (!cancelled && workbenchSnapshotSeqRef.current === refreshSeq) {
                    setWorkbench(snapshot);
                  }
                })
                .catch((nextError) => {
                  if (!cancelled) {
                    console.warn("Failed to refresh workbench after session addon change.", nextError);
                  }
                });
            }),
          ]);
          if (cancelled) {
            subscriptions.forEach((unsubscribe) => unsubscribe());
          } else {
            unlisten = () => {
              subscriptions.forEach((unsubscribe) => unsubscribe());
            };
          }
        }
        const [nextStartup, nextWorkbench] = await Promise.all([
          loadStartupSnapshot(),
          loadWorkbenchSnapshot(),
        ]);
        if (!cancelled) {
          retryDelayMs = 500;
          setError(null);
          setStartup(nextStartup);
          workbenchSnapshotSeqRef.current += 1;
          setWorkbench(nextWorkbench);
        }
      } catch (nextError) {
        if (!cancelled && scheduleBootstrapRetry(nextError)) {
          setError(null);
          return;
        }
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      }
    }

    void bootstrap();
    return () => {
      cancelled = true;
      if (workbenchDeltaTimerRef.current !== null) {
        window.clearTimeout(workbenchDeltaTimerRef.current);
        workbenchDeltaTimerRef.current = null;
      }
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      unlisten?.();
    };
  }, []);

  if (error) {
    return <ErrorScreen error={error} />;
  }

  if (!startup || !workbench) {
    return (
      <I18nProvider>
        <LoadingScreen startup={startup} />
        <Toaster />
      </I18nProvider>
    );
  }

  return (
    <I18nProvider>
      <TooltipProvider>
        <WorkbenchLayout startup={startup} workbench={workbench} workbenchDelta={workbenchDelta} />
        <Toaster />
      </TooltipProvider>
    </I18nProvider>
  );
}
