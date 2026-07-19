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
  loadWorkbenchSnapshot,
} from "@/service/workbench-snapshot-service";
import { ConversationLiveStreamProvider } from "@/state/conversation-live-stream-context";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import { workbenchEnvelopeRequiresReactProjection } from "@/state/workbench-stream-projection";
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
  // liveStreamStore owns ordered streamed bodies without routing every fragment through global React state.
  // liveStreamStore 持有有序流式正文，避免每个片段经过全局 React 状态。
  const [liveStreamStore] = React.useState(() => new ConversationLiveStreamStore());
  // workbenchRef exposes the latest authoritative conversation titles to service event callbacks.
  // workbenchRef 向服务事件回调暴露最近的权威会话标题。
  const workbenchRef = React.useRef<WorkbenchSnapshot | null>(null);
  // workbenchDelta stores the latest partial workbench update signal.
  // workbenchDelta 保存最近一次局部工作台更新信号。
  const [workbenchDelta, setWorkbenchDelta] = React.useState<WorkbenchDeltaSignal | null>(null);
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
  // appLocaleRef keeps provider-outside app toasts aligned with the latest service locale.
  // appLocaleRef 让 Provider 外部的应用提示对齐最近一次服务语言。
  const appLocaleRef = React.useRef<LocaleCode>(webuiAppLocale(props.initialStartup.uiLocale));

  React.useEffect(() => {
    appLocaleRef.current = webuiAppLocale(workbench?.uiLocale ?? startup.uiLocale);
  }, [startup.uiLocale, workbench?.uiLocale]);

  React.useEffect(() => {
    workbenchRef.current = workbench;
  }, [workbench]);

  React.useEffect(() => {
    void notificationCoordinator.start().catch((startError) => {
      console.warn("Notification activation subscription failed.", startError);
    });
  }, []);

  React.useEffect(() => {
    // handleConnectionStatusChange maps transport-level connection changes to user-visible toasts.
    // handleConnectionStatusChange 将传输层连接变化映射为用户可见的提示。
    function handleConnectionStatusChange(event: Event) {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (!detail) {
        return;
      }
      if (detail.status === "connected") {
        serviceHasConnectedRef.current = true;
        if (serviceWasDisconnectedRef.current) {
          serviceWasDisconnectedRef.current = false;
          toast.success(webuiAppTranslation(appLocaleRef.current, "app.serviceConnectionRestored"), {
            id: BROWSER_SERVICE_CONNECTION_TOAST_ID,
            duration: 2200,
          });
        }
        return;
      }
      if (!serviceHasConnectedRef.current) {
        return;
      }
      serviceWasDisconnectedRef.current = true;
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
        setWorkbenchDelta((current) => ({
          seq: (current?.seq ?? 0) + 1,
          envelopes,
        }));
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

    // hasDeltaGap returns true when received deltas are waiting behind a missing generation.
    // hasDeltaGap 在已收到增量仍被缺失 generation 阻塞时返回 true。
    function hasDeltaGap(): boolean {
      if (workbenchDeltaPendingRef.current.size === 0) {
        return false;
      }
      return !workbenchDeltaPendingRef.current.has(workbenchRevisionRef.current + 1);
    }

    // drainContiguousDeltas moves generation-contiguous deltas into the React flush queue.
    // drainContiguousDeltas 将 generation 连续的增量移动到 React 刷新队列。
    function drainContiguousDeltas() {
      let drained = false;
      let nextGeneration = workbenchRevisionRef.current + 1;
      while (workbenchDeltaPendingRef.current.has(nextGeneration)) {
        const envelope = workbenchDeltaPendingRef.current.get(nextGeneration);
        workbenchDeltaPendingRef.current.delete(nextGeneration);
        if (envelope) {
          liveStreamStore.applyEnvelope(envelope);
          workbenchDeltaBufferRef.current.push(envelope);
          workbenchRevisionRef.current = nextGeneration;
          drained = true;
        }
        nextGeneration = workbenchRevisionRef.current + 1;
      }
      if (drained) {
        scheduleReadyDeltaFlush();
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
      let shouldRetryRecovery = false;
      try {
        const recoveredWorkbench = await loadWorkbenchSnapshot();
        if (cancelled) {
          return;
        }
        const revision = recoveredWorkbench.revision ?? 0;
        if (revision > workbenchRevisionRef.current || workbenchRevisionRef.current === 0) {
          liveStreamStore.rebaseWorkbenchSnapshot(recoveredWorkbench);
          liveStreamStore.flushNotifications();
          discardBufferedDeltasThroughRevision(revision);
          workbenchRevisionRef.current = Math.max(workbenchRevisionRef.current, revision);
          workbenchSnapshotSeqRef.current += 1;
          setWorkbench(recoveredWorkbench);
          drainContiguousDeltas();
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
        const nextWorkbench = await loadWorkbenchSnapshot();
        if (!cancelled) {
          retryDelayMs = 500;
          setError(null);
          workbenchSnapshotSeqRef.current += 1;
          workbenchRevisionRef.current = Math.max(workbenchRevisionRef.current, nextWorkbench.revision ?? 0);
          liveStreamStore.rebaseWorkbenchSnapshot(nextWorkbench);
          liveStreamStore.flushNotifications();
          setWorkbench(nextWorkbench);
          workbenchRef.current = nextWorkbench;
        }
        if (hasServiceRuntime()) {
          const subscriptions = await Promise.all([
            serviceClient.subscribe<StartupSnapshot>("startup", (payload) => {
              if (!cancelled) {
                setStartup(payload);
              }
            }),
            serviceClient.subscribe<WorkbenchSnapshot>("workbench", (payload) => {
              if (!cancelled) {
                const revision = payload.revision ?? 0;
                if (revision <= workbenchRevisionRef.current && workbenchRevisionRef.current !== 0) {
                  console.debug("Ignored stale workbench snapshot.", {
                    revision,
                    currentRevision: workbenchRevisionRef.current,
                  });
                  return;
                }
                discardBufferedDeltasThroughRevision(revision);
                clearDeltaGapTimer();
                workbenchRevisionRef.current = revision;
                workbenchSnapshotSeqRef.current += 1;
                liveStreamStore.rebaseWorkbenchSnapshot(payload);
                liveStreamStore.flushNotifications();
                setWorkbench(payload);
                workbenchRef.current = payload;
                drainContiguousDeltas();
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
          <WorkbenchLayout startup={startup} workbench={workbench} workbenchDelta={workbenchDelta} />
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
