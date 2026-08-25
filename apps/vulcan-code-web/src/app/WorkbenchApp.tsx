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
import { I18nProvider, normalizeLocaleCode } from "@/i18n";
import { translations, type LocaleCode, type TranslationKey } from "@/i18n-messages";
import { notificationCoordinator } from "@/notifications/coordinator";
import type { NotificationIntentV1, NotificationText } from "@/notifications/types";
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
import { ConversationLiveStreamProvider } from "@/state/conversation-live-stream-context";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import {
  mergePendingWorkbenchDeltaSignal,
  workbenchEnvelopeRequiresImmediateLiveFlush,
  workbenchEnvelopeRequiresReactProjection,
} from "@/state/workbench-stream-projection";
import { coalesceWorkbenchDeltaEnvelopes } from "@/state/workbench-state";
import type {
  StartupSnapshot,
  ConversationRunSettledEvent,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";
import { toast } from "sonner";
import "../styles.css";

// STREAMING_DELTA_FLUSH_MS batches hot transcript deltas so React and GC are not driven at display-frame cadence.
// STREAMING_DELTA_FLUSH_MS 将高频转录增量批处理，避免 React 与 GC 被显示帧率牵引。
const STREAMING_DELTA_FLUSH_MS = 80;

// WORKBENCH_DELTA_GAP_RECOVERY_MS waits briefly for out-of-order delta gaps before falling back to a full snapshot.
// WORKBENCH_DELTA_GAP_RECOVERY_MS 在增量乱序缺口处短暂等待，超时后回退到完整快照。
const WORKBENCH_DELTA_GAP_RECOVERY_MS = 400;

// SHUTDOWN_PERSIST_TOAST_ID identifies the single replaceable durable-close progress notice.
// SHUTDOWN_PERSIST_TOAST_ID 标识唯一且可替换的耐久关闭进度提示。
const SHUTDOWN_PERSIST_TOAST_ID = "shutdown-persist-progress";

// WorkbenchApp owns the real workbench snapshots, event subscriptions, and reconnect notifications.
// WorkbenchApp 持有真实工作台快照、事件订阅与重连通知。
export function WorkbenchApp(props: {
  // Startup snapshot captured before the workbench runtime mounted.
  // 工作台运行时挂载前捕获的启动快照。
  initialStartup: StartupSnapshot;
}) {
  // startup stores the latest startup snapshot.
  // startup 保存最近一次启动快照。
  const [startup, setStartup] = React.useState<StartupSnapshot>(props.initialStartup);
  // workbench stores the latest workbench snapshot.
  // workbench 保存最近一次工作台快照。
  const [workbench, setWorkbench] = React.useState<WorkbenchSnapshot | null>(null);
  // serviceInstanceEpoch identifies the browser-visible local service generation.
  // serviceInstanceEpoch 标识浏览器当前可见的本地服务代次。
  const [serviceInstanceEpoch, setServiceInstanceEpoch] = React.useState(0);
  // liveStreamStore owns ordered streamed bodies without routing every fragment through global React state.
  // liveStreamStore 持有有序流式正文，避免每个片段经过全局 React 状态。
  const [liveStreamStore] = React.useState(() => new ConversationLiveStreamStore());
  // workbenchRef exposes the latest authoritative conversation titles to service event callbacks.
  // workbenchRef 向服务事件回调暴露最近的权威会话标题。
  const workbenchRef = React.useRef<WorkbenchSnapshot | null>(null);
  // workbenchDelta stores the latest partial workbench update signal.
  // workbenchDelta 保存最近一次局部工作台更新信号。
  const [workbenchDelta, setWorkbenchDelta] = React.useState<WorkbenchDeltaSignal | null>(null);
  // workbenchDeltaSignalSequenceRef assigns monotonic identities independently from acknowledged React state.
  // workbenchDeltaSignalSequenceRef 独立于已确认的 React 状态分配单调信号标识。
  const workbenchDeltaSignalSequenceRef = React.useRef(0);
  // workbenchDeltaBufferRef batches hot-path deltas until the next scheduled flush.
  // workbenchDeltaBufferRef 将已连续可应用的热路径增量批处理到下一次计划刷新。
  const workbenchDeltaBufferRef = React.useRef<WorkbenchDeltaEnvelope[]>([]);
  // workbenchDeltaPendingRef stores received deltas that cannot be applied until missing earlier generations arrive.
  // workbenchDeltaPendingRef 保存已收到但仍需等待更早 generation 到达的增量。
  const workbenchDeltaPendingRef = React.useRef<Map<number, WorkbenchDeltaEnvelope>>(new Map());
  // workbenchDeltaTimerRef stores the scheduled delta flush timer id.
  // workbenchDeltaTimerRef 保存已调度的增量刷新定时器标识。
  const workbenchDeltaTimerRef = React.useRef<number | null>(null);
  // workbenchDeltaGapTimerRef stores the snapshot-recovery timer for a detected generation gap.
  // workbenchDeltaGapTimerRef 保存检测到 generation 缺口后的快照恢复定时器。
  const workbenchDeltaGapTimerRef = React.useRef<number | null>(null);
  // workbenchDeltaRecoveringRef prevents concurrent full-snapshot recoveries for the same delta gap.
  // workbenchDeltaRecoveringRef 防止同一增量缺口触发并发完整快照恢复。
  const workbenchDeltaRecoveringRef = React.useRef(false);
  // error stores the latest loading error.
  // error 保存最近一次加载错误。
  const [error, setError] = React.useState<string | null>(null);
  // serviceWasDisconnectedRef suppresses recovery toasts before the first real disconnect.
  // serviceWasDisconnectedRef 在首次真实断开前抑制恢复提示。
  const serviceWasDisconnectedRef = React.useRef(false);
  // serviceHasConnectedRef prevents first-load probing from showing disconnect notifications.
  // serviceHasConnectedRef 防止首次加载探测显示断开通知。
  const serviceHasConnectedRef = React.useRef(false);
  // workbenchSnapshotSeqRef rejects stale full-snapshot updates across every snapshot source.
  // workbenchSnapshotSeqRef 在所有快照来源之间拒绝过期的全量快照更新。
  const workbenchSnapshotSeqRef = React.useRef(0);
  // workbenchRevisionRef records the newest backend workbench revision accepted by the client.
  // workbenchRevisionRef 记录客户端已经接受的最新后端工作台版本。
  const workbenchRevisionRef = React.useRef(0);
  // workbenchBaselineReadyRef prevents event deltas from being applied before one authoritative snapshot establishes their revision base.
  // workbenchBaselineReadyRef 防止事件增量在权威快照建立版本基线前被应用。
  const workbenchBaselineReadyRef = React.useRef(false);
  // appLocaleRef keeps provider-outside app toasts aligned with the latest service locale.
  // appLocaleRef 让 Provider 外部的应用提示对齐最近一次服务语言。
  const appLocaleRef = React.useRef<LocaleCode>(webuiAppLocale(props.initialStartup.uiLocale));

  React.useEffect(() => {
    appLocaleRef.current = webuiAppLocale(workbench?.uiLocale ?? startup.uiLocale);
  }, [startup.uiLocale, workbench?.uiLocale]);

  React.useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      return;
    }
    // disposed prevents a late native subscription from surviving component unmount.
    // disposed 防止迟到的原生订阅在组件卸载后继续存活。
    let disposed = false;
    // unlistenStarted releases the native durable-close start subscription.
    // unlistenStarted 释放原生耐久关闭开始订阅。
    let unlistenStarted: (() => void) | null = null;
    // unlistenFailed releases the native durable-close failure subscription.
    // unlistenFailed 释放原生耐久关闭失败订阅。
    let unlistenFailed: (() => void) | null = null;

    // subscribeShutdownFailure installs the user decision boundary for a failed durable writer drain.
    // subscribeShutdownFailure 为耐久写入器排空失败安装用户决策边界。
    async function subscribeShutdownFailure() {
      // eventApi provides the native event subscription and windowApi provides force-destroy only after confirmation.
      // eventApi 提供原生事件订阅，windowApi 仅在用户确认后提供强制销毁能力。
      const [eventApi, windowApi] = await Promise.all([
        import("@tauri-apps/api/event"),
        import("@tauri-apps/api/window"),
      ]);
      // nextUnlistenStarted publishes one localized progress notice for the bounded drain.
      // nextUnlistenStarted 为有界排空过程发布一条本地化进度提示。
      const nextUnlistenStarted = await eventApi.listen<void>("vulcan://shutdown-persist-started", () => {
        // locale captures the latest service-selected UI language when shutdown starts.
        // locale 在关闭开始时捕获服务最近选择的界面语言。
        const locale = appLocaleRef.current;
        toast.loading(webuiAppTranslation(locale, "app.shutdown.persisting"), {
          id: SHUTDOWN_PERSIST_TOAST_ID,
          duration: Infinity,
        });
      });
      if (disposed) {
        nextUnlistenStarted();
        return;
      }
      unlistenStarted = nextUnlistenStarted;
      // nextUnlistenFailed is retained only while this WorkbenchApp instance remains mounted.
      // nextUnlistenFailed 仅在当前 WorkbenchApp 实例保持挂载时保留。
      const nextUnlistenFailed = await eventApi.listen<string>("vulcan://shutdown-persist-failed", (event) => {
        toast.dismiss(SHUTDOWN_PERSIST_TOAST_ID);
        // locale captures the latest service-selected UI language at the decision boundary.
        // locale 在决策边界捕获服务最近选择的界面语言。
        const locale = appLocaleRef.current;
        // prompt combines localized risk guidance with the exact technical failure for diagnosis.
        // prompt 将本地化风险说明与用于诊断的精确技术失败组合起来。
        const prompt = [
          webuiAppTranslation(locale, "app.shutdown.persistFailed.title"),
          webuiAppTranslation(locale, "app.shutdown.persistFailed.description"),
          event.payload,
          webuiAppTranslation(locale, "app.shutdown.persistFailed.forceConfirm"),
        ].join("\n\n");
        if (!window.confirm(prompt)) {
          return;
        }
        // nativeWindow is destroyed only after the user explicitly accepts possible tail loss.
        // nativeWindow 仅在用户明确接受可能的尾部丢失后被销毁。
        const nativeWindow = windowApi.getCurrentWindow();
        void nativeWindow.destroy().catch((destroyError) => {
          console.error("Force-closing the desktop window failed.", destroyError);
        });
      });
      if (disposed) {
        nextUnlistenFailed();
        return;
      }
      unlistenFailed = nextUnlistenFailed;
    }

    void subscribeShutdownFailure().catch((subscribeError) => {
      unlistenStarted?.();
      unlistenStarted = null;
      unlistenFailed?.();
      unlistenFailed = null;
      console.error("Desktop shutdown failure subscription failed.", subscribeError);
    });
    return () => {
      disposed = true;
      toast.dismiss(SHUTDOWN_PERSIST_TOAST_ID);
      unlistenStarted?.();
      unlistenFailed?.();
    };
  }, []);

  React.useEffect(() => {
    workbenchRef.current = workbench;
  }, [workbench]);

  React.useEffect(() => {
    void notificationCoordinator.start().catch((startError) => {
      console.warn("Notification activation subscription failed.", startError);
    });
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    let retryTimer: number | null = null;
    let retryDelayMs = 500;
    // reconnectSnapshotTimer stores the pending authoritative snapshot reload after a service reconnect.
    // reconnectSnapshotTimer 保存服务重连后等待执行的权威快照重载定时器。
    let reconnectSnapshotTimer: number | null = null;
    // reconnectEpoch invalidates snapshot reloads that belong to an earlier connection generation.
    // reconnectEpoch 使属于较早连接代次的快照重载失效。
    let reconnectEpoch = 0;

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

    // flushReadyDeltas publishes the already-contiguous delta queue to React.
    // flushReadyDeltas 将已经连续的增量队列发布给 React。
    function flushReadyDeltas() {
      // readyEnvelopes retains every raw generation already ingested by the live stream Store.
      // readyEnvelopes 保留已经写入实时流 Store 的每条原始 generation。
      const readyEnvelopes = workbenchDeltaBufferRef.current;
      workbenchDeltaBufferRef.current = [];
      workbenchDeltaTimerRef.current = null;
      liveStreamStore.flushNotifications();
      // envelopes excludes pure body continuations so WorkbenchLayout does not rerender for streamed text growth.
      // envelopes 排除纯正文续传，使 WorkbenchLayout 不因流式文本增长而重渲染。
      const envelopes = coalesceWorkbenchDeltaEnvelopes(
        readyEnvelopes.filter(workbenchEnvelopeRequiresReactProjection),
      );
      if (envelopes.length > 0) {
        workbenchDeltaSignalSequenceRef.current += 1;
        // signalSequence remains monotonic even when a child acknowledgement clears the current signal first.
        // signalSequence 即使子组件确认先清空当前信号也保持单调。
        const signalSequence = workbenchDeltaSignalSequenceRef.current;
        setWorkbenchDelta((current) => (
          mergePendingWorkbenchDeltaSignal(current, signalSequence, envelopes)
        ));
      }
    }

    // scheduleReadyDeltaFlush batches contiguous deltas without delaying gap detection.
    // scheduleReadyDeltaFlush 批处理连续增量，但不延迟缺口检测。
    function scheduleReadyDeltaFlush() {
      if (workbenchDeltaTimerRef.current !== null) {
        return;
      }
      workbenchDeltaTimerRef.current = window.setTimeout(flushReadyDeltas, STREAMING_DELTA_FLUSH_MS);
    }

    // clearDeltaGapTimer cancels a pending gap-recovery timer once the missing delta arrives or a snapshot covers it.
    // clearDeltaGapTimer 在缺失增量到达或快照覆盖缺口后取消待执行的缺口恢复定时器。
    function clearDeltaGapTimer() {
      if (workbenchDeltaGapTimerRef.current !== null) {
        window.clearTimeout(workbenchDeltaGapTimerRef.current);
        workbenchDeltaGapTimerRef.current = null;
      }
    }

    // discardBufferedDeltasThroughRevision removes queued deltas already covered by a newer full snapshot.
    // discardBufferedDeltasThroughRevision 移除已经被更新全量快照覆盖的待应用增量。
    function discardBufferedDeltasThroughRevision(revision: number) {
      workbenchDeltaBufferRef.current = workbenchDeltaBufferRef.current.filter((envelope) => (
        envelope.generation > revision
      ));
      for (const generation of Array.from(workbenchDeltaPendingRef.current.keys())) {
        if (generation <= revision) {
          workbenchDeltaPendingRef.current.delete(generation);
        }
      }
    }

    // applyAuthoritativeWorkbenchSnapshot establishes one revision baseline and then drains events received after it.
    // applyAuthoritativeWorkbenchSnapshot 建立一个版本基线，随后排空在其后收到的事件。
    function applyAuthoritativeWorkbenchSnapshot(payload: WorkbenchSnapshot): boolean {
      const revision = payload.revision ?? 0;
      if (
        workbenchBaselineReadyRef.current
        && workbenchRevisionRef.current !== 0
        && revision <= workbenchRevisionRef.current
      ) {
        console.debug("Ignored stale workbench snapshot.", {
          revision,
          currentRevision: workbenchRevisionRef.current,
        });
        return false;
      }
      discardBufferedDeltasThroughRevision(revision);
      clearDeltaGapTimer();
      workbenchRevisionRef.current = revision;
      workbenchBaselineReadyRef.current = true;
      workbenchSnapshotSeqRef.current += 1;
      liveStreamStore.rebaseWorkbenchSnapshot(payload);
      liveStreamStore.flushNotifications();
      setWorkbench(payload);
      workbenchRef.current = payload;
      drainContiguousDeltas();
      scheduleDeltaGapRecovery();
      return true;
    }

    // hasDeltaGap returns true when received deltas are waiting behind a missing generation.
    // hasDeltaGap 在已收到增量仍被缺失 generation 阻塞时返回 true。
    function hasDeltaGap(): boolean {
      if (!workbenchBaselineReadyRef.current || workbenchDeltaPendingRef.current.size === 0) {
        return false;
      }
      return !workbenchDeltaPendingRef.current.has(workbenchRevisionRef.current + 1);
    }

    // drainContiguousDeltas moves generation-contiguous deltas into the React flush queue.
    // drainContiguousDeltas 将 generation 连续的增量移动到 React 刷新队列。
    function drainContiguousDeltas() {
      if (!workbenchBaselineReadyRef.current) {
        return;
      }
      let drained = false;
      // immediateLiveFlush becomes true when a request closes one lifecycle state in this contiguous batch.
      // immediateLiveFlush 在当前连续批次中有请求封闭一个生命周期状态时变为 true。
      let immediateLiveFlush = false;
      let nextGeneration = workbenchRevisionRef.current + 1;
      while (workbenchDeltaPendingRef.current.has(nextGeneration)) {
        const envelope = workbenchDeltaPendingRef.current.get(nextGeneration);
        workbenchDeltaPendingRef.current.delete(nextGeneration);
        if (envelope) {
          liveStreamStore.applyEnvelope(envelope);
          workbenchDeltaBufferRef.current.push(envelope);
          immediateLiveFlush = immediateLiveFlush
            || workbenchEnvelopeRequiresImmediateLiveFlush(envelope);
          workbenchRevisionRef.current = nextGeneration;
          drained = true;
        }
        nextGeneration = workbenchRevisionRef.current + 1;
      }
      if (drained) {
        if (immediateLiveFlush) {
          if (workbenchDeltaTimerRef.current !== null) {
            window.clearTimeout(workbenchDeltaTimerRef.current);
          }
          flushReadyDeltas();
        } else {
          scheduleReadyDeltaFlush();
        }
      }
      if (!hasDeltaGap()) {
        clearDeltaGapTimer();
      }
    }

    // recoverDeltaGap reloads the authoritative snapshot after a generation gap outlives the short reorder window.
    // recoverDeltaGap 在 generation 缺口超过短暂乱序等待窗口后重新加载权威快照。
    async function recoverDeltaGap() {
      if (cancelled || workbenchDeltaRecoveringRef.current) {
        return;
      }
      workbenchDeltaGapTimerRef.current = null;
      if (!hasDeltaGap()) {
        drainContiguousDeltas();
        return;
      }
      workbenchDeltaRecoveringRef.current = true;
      // requestedAtSnapshotSequence rejects a response superseded by another full snapshot or reconnect.
      // requestedAtSnapshotSequence 拒绝已被另一份全量快照或重连边界取代的响应。
      const requestedAtSnapshotSequence = workbenchSnapshotSeqRef.current;
      let shouldRetryRecovery = false;
      try {
        const recoveredWorkbench = await loadWorkbenchSnapshot();
        if (
          cancelled
          || requestedAtSnapshotSequence !== workbenchSnapshotSeqRef.current
        ) {
          return;
        }
        const revision = recoveredWorkbench.revision ?? 0;
        if (
          !workbenchBaselineReadyRef.current
          || revision > workbenchRevisionRef.current
        ) {
          applyAuthoritativeWorkbenchSnapshot(recoveredWorkbench);
          shouldRetryRecovery = hasDeltaGap();
        } else if (hasDeltaGap()) {
          shouldRetryRecovery = true;
        }
      } catch (nextError) {
        if (!cancelled) {
          console.warn("Failed to recover workbench delta gap.", nextError);
          shouldRetryRecovery = true;
        }
      } finally {
        workbenchDeltaRecoveringRef.current = false;
      }
      if (shouldRetryRecovery && !cancelled) {
        scheduleDeltaGapRecovery();
      }
    }

    // clearReconnectSnapshotTimer cancels one pending reconnect snapshot retry.
    // clearReconnectSnapshotTimer 取消一次等待中的重连快照重试。
    function clearReconnectSnapshotTimer() {
      if (reconnectSnapshotTimer !== null) {
        window.clearTimeout(reconnectSnapshotTimer);
        reconnectSnapshotTimer = null;
      }
    }

    // resetStreamOrderingForReconnect removes state owned by the previous service instance.
    // resetStreamOrderingForReconnect 移除由上一服务实例持有的流排序状态。
    function resetStreamOrderingForReconnect() {
      if (workbenchDeltaTimerRef.current !== null) {
        window.clearTimeout(workbenchDeltaTimerRef.current);
        workbenchDeltaTimerRef.current = null;
      }
      clearDeltaGapTimer();
      workbenchDeltaBufferRef.current = [];
      workbenchDeltaPendingRef.current.clear();
      workbenchDeltaRecoveringRef.current = false;
      workbenchRevisionRef.current = 0;
      workbenchBaselineReadyRef.current = false;
      workbenchSnapshotSeqRef.current += 1;
      setWorkbenchDelta(null);
    }

    // reloadSnapshotsAfterReconnect rebases the UI on the newly connected service instance.
    // reloadSnapshotsAfterReconnect 以新连接的服务实例为基准重新装载界面状态。
    async function reloadSnapshotsAfterReconnect(
      epoch: number,
      snapshotSequence: number,
      retryAttempt: number,
    ) {
      try {
        const [nextStartup, nextWorkbench] = await Promise.all([
          loadStartupSnapshot(),
          loadWorkbenchSnapshot(),
        ]);
        if (cancelled || epoch !== reconnectEpoch) {
          return;
        }
        setStartup(nextStartup);
        setServiceInstanceEpoch(epoch);
        // A workbench event may have delivered a newer full snapshot while the HTTP reload was in flight.
        // HTTP 重载进行期间，工作台事件可能已经送达更新的全量快照。
        if (snapshotSequence === workbenchSnapshotSeqRef.current) {
          applyAuthoritativeWorkbenchSnapshot(nextWorkbench);
        }
        toast.success(
          webuiAppTranslation(appLocaleRef.current, "app.serviceConnectionRestored"),
          {
            id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
            duration: 2200,
          },
        );
      } catch (reloadError) {
        if (cancelled || epoch !== reconnectEpoch) {
          return;
        }
        console.warn("Failed to reload service snapshots after reconnect.", reloadError);
        const delay = Math.min(500 * (2 ** retryAttempt), 5000);
        clearReconnectSnapshotTimer();
        reconnectSnapshotTimer = window.setTimeout(() => {
          reconnectSnapshotTimer = null;
          void reloadSnapshotsAfterReconnect(epoch, snapshotSequence, retryAttempt + 1);
        }, delay);
      }
    }

    // handleConnectionStatusChange restores authoritative snapshots after a real disconnect.
    // handleConnectionStatusChange 在真实断线后恢复权威快照。
    function handleConnectionStatusChange(event: Event) {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (!detail) {
        return;
      }
      if (detail.status === "connected") {
        serviceHasConnectedRef.current = true;
        if (!serviceWasDisconnectedRef.current) {
          return;
        }
        serviceWasDisconnectedRef.current = false;
        reconnectEpoch += 1;
        clearReconnectSnapshotTimer();
        resetStreamOrderingForReconnect();
        const epoch = reconnectEpoch;
        const snapshotSequence = workbenchSnapshotSeqRef.current;
        toast.loading(
          webuiAppTranslation(appLocaleRef.current, "app.serviceCheckingRecovery"),
          { id: BROWSER_SERVICE_CONNECTION_TOAST_ID },
        );
        void reloadSnapshotsAfterReconnect(epoch, snapshotSequence, 0);
        return;
      }
      if (!serviceHasConnectedRef.current) {
        return;
      }
      serviceWasDisconnectedRef.current = true;
      reconnectEpoch += 1;
      clearReconnectSnapshotTimer();
      if (detail.status === "reconnecting") {
        toast.loading(webuiAppTranslation(appLocaleRef.current, "app.serviceReconnecting"), {
          id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
        });
        return;
      }
      toast.loading(webuiAppTranslation(appLocaleRef.current, "app.serviceCheckingRecovery"), {
        id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
        description: detail.message,
      });
    }

    // scheduleDeltaGapRecovery waits for late deltas before using a full snapshot to bridge the gap.
    // scheduleDeltaGapRecovery 在使用完整快照弥补缺口前等待迟到增量。
    function scheduleDeltaGapRecovery() {
      if (!hasDeltaGap() || workbenchDeltaGapTimerRef.current !== null || workbenchDeltaRecoveringRef.current) {
        return;
      }
      workbenchDeltaGapTimerRef.current = window.setTimeout(() => {
        void recoverDeltaGap();
      }, WORKBENCH_DELTA_GAP_RECOVERY_MS);
    }

    // enqueueWorkbenchDelta buffers one delta until every earlier generation has been applied.
    // enqueueWorkbenchDelta 缓冲单条增量，直到所有更早 generation 都已应用。
    function enqueueWorkbenchDelta(payload: WorkbenchDeltaEnvelope) {
      if (payload.generation <= workbenchRevisionRef.current) {
        console.debug("Ignored stale workbench delta.", {
          revision: payload.generation,
          currentRevision: workbenchRevisionRef.current,
        });
        return;
      }
      if (!workbenchDeltaPendingRef.current.has(payload.generation)) {
        workbenchDeltaPendingRef.current.set(payload.generation, payload);
      }
      drainContiguousDeltas();
      scheduleDeltaGapRecovery();
    }

    // bootstrap loads the first workbench snapshot after the isolated boot gate has accepted startup readiness.
    // bootstrap 在隔离启动门确认启动就绪后加载第一份工作台快照。
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
                applyAuthoritativeWorkbenchSnapshot(payload);
              }
            }),
            serviceClient.subscribe<WorkbenchDeltaEnvelope>("workbench.delta", (payload) => {
              if (cancelled) {
                return;
              }
              enqueueWorkbenchDelta(payload);
            }),
            serviceClient.subscribe<ConversationRunSettledEvent>("conversation.run.settled", (payload) => {
              if (cancelled) {
                return;
              }
              // conversation is resolved from the latest accepted workbench projection by the event's exact id.
              // conversation 通过事件精确标识从最近接纳的工作台投影中解析。
              const conversation = workbenchRef.current?.conversations.find((item) => item.id === payload.conversationId);
              // title is optional because a lifecycle event may arrive after the sidebar row was archived.
              // title 可选，因为生命周期事件可能在侧栏行归档后到达。
              const title = conversation?.title ?? null;
              void notificationCoordinator.handleConversationRunSettled(
                payload,
                title,
                (intent) => resolveNotificationText(appLocaleRef.current, intent),
              );
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
        // The first authoritative snapshot is loaded only after every event subscription is ready.
        // 第一份权威快照仅在全部事件订阅就绪后加载。
        const nextWorkbench = await loadWorkbenchSnapshot();
        if (!cancelled) {
          retryDelayMs = 500;
          setError(null);
          applyAuthoritativeWorkbenchSnapshot(nextWorkbench);
        }
      } catch (nextError) {
        // A failed bootstrap attempt must release any subscriptions before retrying.
        // 启动尝试失败后必须先释放全部订阅，再进行重试。
        unlisten?.();
        unlisten = null;
        if (!cancelled && scheduleBootstrapRetry(nextError)) {
          setError(null);
          return;
        }
        if (!cancelled) {
          setError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      }
    }

    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatusChange);
    void bootstrap();
    return () => {
      cancelled = true;
      reconnectEpoch += 1;
      window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatusChange);
      clearReconnectSnapshotTimer();
      if (workbenchDeltaTimerRef.current !== null) {
        window.clearTimeout(workbenchDeltaTimerRef.current);
        workbenchDeltaTimerRef.current = null;
      }
      clearDeltaGapTimer();
      workbenchDeltaBufferRef.current = [];
      workbenchDeltaPendingRef.current.clear();
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      unlisten?.();
    };
  }, []);

  // handleWorkbenchDeltaConsumed clears only the exact signal already applied by WorkbenchLayout.
  // handleWorkbenchDeltaConsumed 仅清除 WorkbenchLayout 已应用的精确信号。
  //
  // Parameters: sequence identifies the signal whose envelopes have been consumed.
  // 参数：sequence 标识其增量已经被消费的信号。
  // Returns: no value; a newer unacknowledged signal remains intact.
  // 返回值：无返回值；更新且尚未确认的信号保持不变。
  const handleWorkbenchDeltaConsumed = React.useCallback((sequence: number) => {
    setWorkbenchDelta((current) => (
      current?.seq === sequence ? null : current
    ));
  }, []);

  if (error) {
    return <ErrorScreen error={error} />;
  }

  if (!workbench) {
    return (
      <I18nProvider initialLocale={startup.uiLocale}>
        <LoadingScreen startup={startup} />
        <Toaster />
      </I18nProvider>
    );
  }

  return (
    <I18nProvider initialLocale={workbench.uiLocale ?? startup.uiLocale}>
      <TooltipProvider>
        <ConversationLiveStreamProvider store={liveStreamStore}>
          <WorkbenchLayout
            serviceInstanceEpoch={serviceInstanceEpoch}
            startup={startup}
            workbench={workbench}
            workbenchDelta={workbenchDelta}
            onWorkbenchDeltaConsumed={handleWorkbenchDeltaConsumed}
          />
        </ConversationLiveStreamProvider>
        <Toaster />
      </TooltipProvider>
    </I18nProvider>
  );
}

// webuiAppLocale resolves a service-provided locale before the React i18n provider is available.
// webuiAppLocale 在 React i18n provider 可用前解析服务提供的语言。
function webuiAppLocale(value: string | null | undefined): LocaleCode {
  const browserLanguage = typeof navigator === "undefined" ? "" : navigator.language;
  return normalizeLocaleCode(value)
    ?? (browserLanguage.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US");
}

// webuiAppTranslation reads one localized app-shell label outside the i18n provider.
// webuiAppTranslation 在 i18n provider 外读取单个本地化应用外壳文案。
function webuiAppTranslation(locale: LocaleCode, key: TranslationKey): string {
  return translations[locale][key] ?? translations["zh-CN"][key];
}

// resolveNotificationText localizes one platform-neutral intent immediately before shell delivery.
// resolveNotificationText 在壳投递前即时本地化一个与平台无关的意图。
function resolveNotificationText(locale: LocaleCode, intent: NotificationIntentV1): NotificationText {
  // genericKey selects the outcome-specific message without private title data.
  // genericKey 选择不含私密标题数据的结果文案。
  const genericKey: TranslationKey = intent.localizationKey === "notifications.completed"
    ? "notifications.completed.generic"
    : "notifications.failed.generic";
  if (intent.privacyLevel === "generic") {
    return {
      title: webuiAppTranslation(locale, "notifications.appTitle"),
      body: webuiAppTranslation(locale, genericKey),
    };
  }
  // conversationTitle is required by the conversation_title intent contract.
  // conversationTitle 由 conversation_title 意图契约强制要求。
  const conversationTitle = intent.localizationArgs.conversationTitle;
  if (typeof conversationTitle !== "string" || conversationTitle.length === 0) {
    throw new Error("Conversation-title notification intent is missing conversationTitle.");
  }
  // titleKey selects the outcome-specific template that accepts the verified title argument.
  // titleKey 选择接收已验证标题参数的结果文案模板。
  const titleKey: TranslationKey = intent.localizationKey === "notifications.completed"
    ? "notifications.completed.withTitle"
    : "notifications.failed.withTitle";
  return {
    title: webuiAppTranslation(locale, "notifications.appTitle"),
    body: webuiAppTranslation(locale, titleKey).replace("{conversationTitle}", conversationTitle),
  };
}
