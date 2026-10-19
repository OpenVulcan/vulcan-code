import * as React from "react";
import { ListTodo, XCircle } from "lucide-react";

import { PageEmptyState } from "@/components/page";
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import { Button } from "@/ui-kit/primitives/button";
import { InlineStreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { useI18n } from "@/i18n";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import { cancelProviderTask, listProviderTasks } from "@/service/settings-actions-service";
import type { ProviderTaskPhase, ProviderTaskSummary, PublicAiProviderToolCapabilityKind } from "@/types";

// PROVIDER_TASK_REFRESH_INTERVAL_MS is how often the list re-reads live status while any task can still change.
// PROVIDER_TASK_REFRESH_INTERVAL_MS 是仍有任务可能变化时列表重新读取实时状态的间隔。
const PROVIDER_TASK_REFRESH_INTERVAL_MS = 5_000;

// PROVIDER_TASK_PHASE_LABEL_KEYS maps every durable phase to its localized label.
// PROVIDER_TASK_PHASE_LABEL_KEYS 将每个持久阶段映射到其本地化名称。
const PROVIDER_TASK_PHASE_LABEL_KEYS = {
  dispatching: "settings.providerTasks.phase.dispatching",
  queued: "settings.providerTasks.phase.queued",
  running: "settings.providerTasks.phase.running",
  cancellation_requested: "settings.providerTasks.phase.cancellationRequested",
  completed: "settings.providerTasks.phase.completed",
  failed: "settings.providerTasks.phase.failed",
  cancelled: "settings.providerTasks.phase.cancelled",
  dispatch_outcome_unknown: "settings.providerTasks.phase.dispatchOutcomeUnknown",
} as const satisfies Record<ProviderTaskPhase, Parameters<ReturnType<typeof useI18n>["t"]>[0]>;

// ProviderTasksPanel lists retained provider asynchronous tasks with live status and cancellation.
// ProviderTasksPanel 列出保留的供应商异步任务，提供实时状态与取消。
// Parameters: capabilityLabel names a capability in the active language; returns the task list or its empty/error state.
// 参数：capabilityLabel 以当前语言命名能力；返回任务列表或其空状态与错误状态。
export function ProviderTasksPanel(props: {
  // Localized name of one capability, shared with the capability rows.
  // 单项能力的本地化名称，与能力行共用。
  capabilityLabel: (capability: PublicAiProviderToolCapabilityKind) => string;
}) {
  const { t, locale } = useI18n();
  const [tasks, setTasks] = React.useState<ProviderTaskSummary[] | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  // cancelling holds the execution identities whose cancellation request is in flight.
  // cancelling 保存取消请求仍在进行中的执行标识。
  const [cancelling, setCancelling] = React.useState<ReadonlySet<string>>(() => new Set());
  // reloadVersion re-reads the list after a retry or a cancellation.
  // reloadVersion 在重试或取消后重新读取列表。
  const [reloadVersion, setReloadVersion] = React.useState(0);

  React.useEffect(() => {
    let active = true;
    void listProviderTasks()
      .then((next) => {
        if (!active) return;
        setTasks(next);
        setError(null);
      })
      .catch((reason) => {
        if (active) setError(errorMessage(reason));
      });
    return () => {
      active = false;
    };
  }, [reloadVersion]);

  // hasLiveTask keeps polling only while some task can still change phase.
  // hasLiveTask 仅在仍有任务可能改变阶段时持续轮询。
  const hasLiveTask = tasks?.some((task) => !task.terminal) ?? false;
  React.useEffect(() => {
    if (!hasLiveTask) return undefined;
    const timer = window.setTimeout(() => setReloadVersion((value) => value + 1), PROVIDER_TASK_REFRESH_INTERVAL_MS);
    return () => window.clearTimeout(timer);
  }, [hasLiveTask, tasks]);

  const cancel = (task: ProviderTaskSummary) => {
    setCancelling((current) => new Set(current).add(task.executionId));
    void cancelProviderTask(task)
      .catch((reason) => setError(errorMessage(reason)))
      .finally(() => {
        setCancelling((current) => {
          const next = new Set(current);
          next.delete(task.executionId);
          return next;
        });
        setReloadVersion((value) => value + 1);
      });
  };

  if (error && !tasks) {
    return (
      <div className="ai-capabilities-message" role="alert">
        <p className="vk-field-error">{error}</p>
        <Button onClick={() => setReloadVersion((value) => value + 1)} size="sm" type="button" variant="outline">
          {t("settings.modelExperience.retryLoad")}
        </Button>
      </div>
    );
  }
  if (!tasks) {
    return <InlineStreamPlaceholder label={t("settings.providerTools.loading")} />;
  }
  if (tasks.length === 0) {
    return <PageEmptyState description={t("settings.providerTasks.empty")} icon={<ListTodo />} title={t("settings.providerTasks.emptyTitle")} />;
  }
  return (
    <>
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
      <ul className="ai-capability-list">
        {tasks.map((task) => {
          const title = task.capability ? props.capabilityLabel(task.capability) : t("settings.providerTasks.untitled");
          const started = formatDateTime(task.createdAtUnixMs, locale) ?? "-";
          const deadline = formatDateTime(task.deadlineUnixMs, locale) ?? "-";
          const cancellable = !task.terminal && task.cancellationSupported && task.phase !== "cancellation_requested";
          return (
            <li className="ai-capability-row" key={task.executionId}>
              <span className="ai-capability-icon" aria-hidden="true">
                {task.capability ? <ProviderToolCapabilityIcon capability={task.capability} size={16} /> : <ListTodo className="vk-icon" />}
              </span>
              <span className="ai-capability-copy">
                <strong>{title}</strong>
                <small title={task.executionId}>
                  {[task.targetDisplayName ?? task.targetId, t("settings.providerTasks.started").replace("{time}", started)].join(" · ")}
                </small>
              </span>
              <span className={task.error ? "ai-capability-target" : "ai-capability-target empty"} title={task.error ?? undefined}>
                {task.error ?? (task.terminal ? "" : t("settings.providerTasks.deadline").replace("{time}", deadline))}
              </span>
              <span className="ai-capability-status" data-enabled={task.phase === "completed" ? "true" : "false"}>
                {t(PROVIDER_TASK_PHASE_LABEL_KEYS[task.phase])}
              </span>
              {cancellable ? (
                <Button
                  aria-label={`${t("settings.providerTasks.cancel")} · ${title}`}
                  disabled={cancelling.has(task.executionId)}
                  onClick={() => cancel(task)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  <XCircle className="vk-icon" />
                  {t("settings.providerTasks.cancel")}
                </Button>
              ) : <span />}
            </li>
          );
        })}
      </ul>
    </>
  );
}
