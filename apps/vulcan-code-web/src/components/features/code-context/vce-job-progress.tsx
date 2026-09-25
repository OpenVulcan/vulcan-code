import * as React from "react";
import { CircleAlert, CircleCheck, LoaderCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { VceJobSummary } from "@/types";
import { isVceJobActive } from "./use-vce-project-job";
import { formatDateTime } from "@/lib/display-format";
import { formatMessage, useI18n, type Translate } from "@/i18n";

// Describe one active durable job by its real operation kind and lifecycle state.
// 按真实操作类型和生命周期状态描述一项活动持久作业。
// job is the acknowledged project receipt and t is the active-locale lookup. Returns null for terminal work.
// job 为已确认项目回执，t 为当前语言查询；终态作业返回空值。
export function vceActiveJobStatusLabel(job: VceJobSummary | null, t: Translate): string | null {
  if (!isVceJobActive(job) || job === null) return null;
  if (job.state === "cancelling") {
    return job.kind === "compact" ? t("codeContext.job.cancellingPhysicalCompaction")
      : t("codeContext.job.cancellingIndexJob");
  }
  if (job.kind === "compact") {
    return job.state === "queued" ? t("codeContext.job.physicalCompactionQueued")
      : t("codeContext.job.compactingPhysicalVectorTables");
  }
  if (job.kind === "initialize") return t("codeContext.job.initializing");
  if (job.kind === "rebuild") return t("codeContext.job.rebuilding");
  return job.state === "queued" ? t("codeContext.job.indexUpdateQueued")
    : t("codeContext.job.updating");
}

// Derive one shared synchronization action from the acknowledged receipt, never from a local click count.
// 从已确认回执派生统一同步操作，绝不依赖本地点击计数。
// job is project-scoped and t is the active-locale lookup. Returns the label and disabled state for both entry points.
// job 限于当前项目，t 为当前语言查询。返回两个入口共用的标签与禁用状态。
export function vceSynchronizationControl(job: VceJobSummary | null, t: Translate): { disabled: boolean; label: string } {
  if (!isVceJobActive(job) || job === null) return { disabled: false, label: t("codeContext.job.synchronizeIndexNow") };
  if (job.state === "cancelling") return { disabled: true, label: job.kind === "compact" ? t("codeContext.job.cancellingCompaction") : t("codeContext.job.cancellingUpdate") };
  if (job.kind === "compact") return { disabled: true, label: job.state === "queued" ? t("codeContext.job.physicalCompactionQueued") : t("codeContext.job.physicalCompactionInProgress") };
  if (job.kind !== "synchronize") return { disabled: true, label: t("codeContext.job.indexJobInProgress") };
  if (job.state === "queued") return { disabled: true, label: t("codeContext.job.updateQueued") };
  if (job.followupRequested) return { disabled: true, label: t("codeContext.job.followUpUpdateScheduled") };
  return { disabled: false, label: t("codeContext.job.synchronizeAgainAfterThis") };
}

// Render truthful phase progress for a durable job without affecting conversation controls.
// 展示持久作业的真实阶段进度，不影响会话控件。
// props owns the exact receipt and cancellation callback. Returns a compact status panel.
// props 持有精确回执与取消回调。返回紧凑状态面板。
export function VceJobProgress(props: {
  // Exact durable receipt selected by project ownership.
  // 按项目归属选择的精确持久回执。
  job: VceJobSummary;
  // Optional exact-job cancellation action; omitted for passive status projections.
  // 可选精确作业取消操作；被动状态投影中省略。
  onCancel?: () => void;
  // Whether cancellation acknowledgement is pending.
  // 是否正在等待取消确认。
  cancelPending?: boolean;
  // Current authoritative query gate when projected outside project settings.
  // 在项目设置之外投影时使用的当前权威查询门禁。
  queryAvailable?: boolean;
  // Explicit retry creates a new guarded host job; unavailable for successful or active receipts.
  // 明确重试创建新的受保护宿主作业；成功或活动回执不可重试。
  onRetry?: () => void;
  // presentation keeps project settings focused while other surfaces retain expanded progress evidence.
  // presentation 使项目设置聚焦当前进度，其他界面保留展开的进度证据。
  presentation?: "settings";
}) {
  // t selects localized display labels without exposing machine state names.
  // t 选择本地化展示标签，不暴露机器状态名。
  const { t, locale } = useI18n();
  const { job } = props;
  const active = isVceJobActive(job);
  // now updates elapsed time without inventing completed work or changing the host receipt.
  // now 更新时间显示，不虚构已完成工作，也不改变宿主回执。
  const [now, setNow] = React.useState(Date.now);
  React.useEffect(() => {
    if (!active) return;
    // timer only refreshes the clock; the observer remains the sole source of job progress.
    // timer 仅刷新时钟；观察器仍是作业进度的唯一来源。
    const timer = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(timer);
  }, [active, job.jobId]);
  // Quiet time is measured from a real receipt change, not from successful polling of the same state.
  // 静默时长从实际回执变化计算，而非从相同状态的成功轮询计算。
  const quietSeconds = Math.max(0, Math.floor((now - job.updatedAtUnixMs) / 1_000));
  // Elapsed duration is informational and never contributes to either progress bar.
  // 已用时仅用于说明，不参与任何进度条计算。
  const elapsedSeconds = Math.max(0, Math.floor((now - job.createdAtUnixMs) / 1_000));
  const compact = job.kind === "compact";
  const stateLabels = {
    queued: compact ? t("codeContext.job.queuedForPhysicalCompaction") : t("codeContext.job.queuedForIndexingResources"),
    running: compact ? t("codeContext.job.compactingPhysicalVectorTables")
      : job.kind === "initialize" ? t("codeContext.job.creatingSemanticIndex")
      : job.kind === "rebuild" ? t("codeContext.job.rebuildingSemanticIndex")
      : t("codeContext.job.updatingSemanticIndex"),
    cancelling: compact ? t("codeContext.job.cancellingCompactionAfterThe") : t("codeContext.job.cancellingAndCleaningUnpublished"),
    succeeded: compact ? t("codeContext.job.physicalVectorTableCompaction") : t("codeContext.job.semanticIndexSynchronized"),
    failed: compact ? t("codeContext.job.physicalCompactionFailed") : t("codeContext.job.indexUpdateFailed"),
    cancelled: compact ? t("codeContext.job.physicalCompactionCancelled") : t("codeContext.job.indexUpdateCancelled"),
    interrupted: compact ? t("codeContext.job.physicalCompactionWasInterrupted") : t("codeContext.job.previousIndexJobInterrupted"),
  };
  const phaseLabels = {
    scanning: t("codeContext.job.scanningFiles"),
    reconciling: t("codeContext.job.reconcilingFileChanges"),
    chunking: t("codeContext.job.analyzingAndChunkingCode"),
    embedding: t("codeContext.job.generatingEmbeddings"),
    writing: compact ? t("codeContext.job.compactingVectorTable") : t("codeContext.job.writingIndex"),
    committing: t("codeContext.job.committingCheckpoint"),
    complete: compact ? t("codeContext.job.confirmingCompactionResult") : t("codeContext.job.confirmingUpdateResult"),
  };
  const unitLabels = { files: t("codeContext.job.files"), blobs: t("codeContext.job.blobs"), batches: t("codeContext.job.batches"), requests: t("codeContext.job.requests"), tables: t("codeContext.job.tables"), none: "" };
  const providerBatchStateLabels = {
    validating: t("codeContext.job.providerIsValidatingThe"),
    in_progress: t("codeContext.job.providerIsGeneratingEmbeddings"),
    finalizing: t("codeContext.job.providerIsFinalizingBatch"),
    completed: t("codeContext.job.providerBatchCompleted"),
    failed: t("codeContext.job.providerBatchFailed"),
    expired: t("codeContext.job.providerBatchExpired"),
    cancelling: t("codeContext.job.providerIsCancellingThe"),
    cancelled: t("codeContext.job.providerBatchCancelled"),
  };
  const compactionTableStateLabels = {
    pending: t("codeContext.job.pending"),
    queued: t("codeContext.job.queued"),
    running: t("codeContext.job.running"),
    succeeded: t("codeContext.job.completed"),
    failed: t("codeContext.job.failed"),
    cancelled: t("codeContext.job.cancelled"),
    state_lost: t("codeContext.job.compactionOperationStateLost"),
    interrupted: t("codeContext.job.interrupted"),
  };
  const progress = job.progress;
  // Display the explicit host activity; counters alone cannot identify a Batch submission.
  // 展示宿主明确报告的活动；仅凭计数无法识别 Batch 提交。
  const activity = progress?.activity;
  // Countdown follows the host's durable query deadline and never schedules provider work in the UI.
  // 倒计时只跟随宿主持久化的查询截止时间，界面本身不调度供应商任务。
  const pollCountdownSeconds = active && activity?.stage === "waiting"
    && activity.nextPollAtUnixMs !== null
    ? Math.max(0, Math.ceil((activity.nextPollAtUnixMs - now) / 1_000)) : null;
  // A nonterminal provider observation distinguishes a repeat wait from the first post-submit wait.
  // 供应商非终态观测用于区分再次等待与提交后的首次等待。
  const batchStillProcessing = progress?.providerBatchState === "validating"
    || progress?.providerBatchState === "in_progress"
    || progress?.providerBatchState === "finalizing"
    || progress?.providerBatchState === "cancelling";
  // Activity titles identify actual local work and each provider lifecycle boundary.
  // 活动标题标识实际本地工作及供应商生命周期各边界。
  const activityTitles = {
    preparing: t("codeContext.job.preparingEmbeddingRequests"),
    planning: t("codeContext.job.assemblingEmbeddingRequests"),
    validating: t("codeContext.job.validatingTheLocalEmbedding"),
    persisting: t("codeContext.job.savingTheLocalEmbedding"),
    planning_tasks: t("codeContext.job.planningEmbeddingBatchTasks"),
    admitting: t("codeContext.job.savingEmbeddingBatchExecution"),
    requesting: t("codeContext.job.requestingSynchronousEmbeddings"),
    uploading: t("codeContext.job.uploadingEmbeddingBatchFiles"),
    submitting: t("codeContext.job.submittingEmbeddingBatchTasks"),
    waiting: t("codeContext.job.waitingForEmbeddingBatch"),
    polling: t("codeContext.job.checkingEmbeddingBatchProcessing"),
    collecting: t("codeContext.job.retrievingAndValidatingEmbeddings"),
  };
  // Each label separates local preparation, synchronous requests and acknowledged remote work.
  // 每项文案区分本地准备、同步请求与已确认的远端工作。
  const activityLabels = {
    preparing: t("codeContext.job.preparingEmbeddingRequestsAnd"),
    planning: t("codeContext.job.fileAnalysisCompletedAssembling"),
    validating: t("codeContext.job.validatingFileSnapshotsAnd"),
    persisting: t("codeContext.job.writingFileSnapshotsAnd"),
    planning_tasks: t("codeContext.job.buildingValidatingAndSaving"),
    admitting: t("codeContext.job.savingEachEmbeddingBatch"),
    requesting: t("codeContext.job.callingTheSynchronousEmbedding"),
    uploading: t("codeContext.job.uploadingEmbeddingInputThe"),
    submitting: t("codeContext.job.submittingTheEmbeddingBatch"),
    waiting: t("codeContext.job.embeddingBatchSubmittedWaiting"),
    polling: t("codeContext.job.checkingTheSubmittedEmbedding"),
    collecting: t("codeContext.job.retrievingAndValidatingEmbedding"),
  };
  // An active receipt error means terminal persistence needs explicit recovery, so cancellation stays available.
  // 活动作业带有回执错误时表示终态持久化需要显式恢复，因此仍保持取消入口可用。
  const recoveryRequired = active && job.error !== null;
  // recoveryStatus keeps an active persistence diagnostic localized and out of the terminal error slot.
  // recoveryStatus 将活动持久化诊断本地化并移出终态错误位置。
  const recoveryStatus = recoveryRequired
    ? t("codeContext.job.theTerminalReceiptIs")
    : null;
  // technicalDiagnostics retains raw host/storage evidence behind an explicit disclosure;
  // normal active states remain localized and never render diagnostics as an inline red status.
  // technicalDiagnostics 将宿主／存储原始证据保留在显式展开项中；
  // 正常活动状态保持本地化，绝不把诊断渲染成行内红色状态。
  const technicalDiagnostics = Array.from(new Set([
    ...(job.error ? [job.error] : []),
    ...(compact && job.compaction
      ? job.compaction.tables.flatMap((table) => table.error ? [table.error] : [])
      : []),
  ]));
  // measured requires a meaningful denominator; unknown and zero totals stay indeterminate.
  // measured 要求有意义的分母；未知与零总量保持不确定态。
  const measured = progress !== null && progress.total !== null && progress.total > 0 && progress.unit !== "none";
  const overallMeasured = progress !== null
    && progress.overallTotal !== undefined
    && progress.overallTotal !== null
    && progress.overallTotal > 0;
  // settingsPresentation puts supporting counters and remote identifiers behind a user-controlled disclosure.
  // settingsPresentation 将辅助计数与远端标识放入由用户展开的详情区。
  const settingsPresentation = props.presentation === "settings";
  // progressDetails preserves the measurements and diagnostics outside the main reading path.
  // progressDetails 在主要阅读内容之外保留测量值与诊断。
  const progressDetails = <div className="space-y-2">
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>{!compact && progress && ["chunking", "embedding", "writing"].includes(progress.phase) ? t("codeContext.job.overallProgressFilesWritten") : t("codeContext.job.overallProgress")}</span>
        <span>{overallMeasured ? `${progress.overallCompleted ?? 0} / ${progress.overallTotal}` : t("codeContext.job.determiningScope")}</span>
      </div>
      <Progress className="h-1.5 w-full" aria-label={t("codeContext.job.overallProgress")} value={overallMeasured ? progress.overallCompleted ?? 0 : null} max={overallMeasured ? progress.overallTotal! : undefined} />
    </div>
    {progress?.providerBatchState ? <p className="text-xs text-muted-foreground" role="status">{providerBatchStateLabels[progress.providerBatchState]}</p> : null}
    {activity?.providerBatchId ? <p className="break-all text-xs text-muted-foreground">{t("codeContext.job.batchTask")}: {activity.providerBatchId}</p> : null}
    <p className="text-xs text-muted-foreground">{t("codeContext.job.sinceLastProgressUpdate")}: {quietSeconds} {t("codeContext.job.s")}</p>
    {compact && job.compaction ? <div className="space-y-1 rounded-md border bg-background/60 p-2">{job.compaction.tables.map((table) => (
      <p className="text-xs text-muted-foreground" key={table.role}>{table.role === "content" ? t("codeContext.job.contentVectorTable") : t("codeContext.job.pathVectorTable")} · {t("codeContext.job.fragments")} {table.plannedFragmentCount} → {table.currentFragmentCount} · {t("codeContext.job.round")} {table.round} · {compactionTableStateLabels[table.state]}</p>
    ))}</div> : null}
  </div>;
  return (
    <section className={settingsPresentation ? "vce-job-card" : "min-w-0 w-full space-y-2 overflow-hidden rounded-lg border bg-muted/20 p-3"} aria-label={compact ? t("codeContext.job.physicalIndexCompactionProgress") : t("codeContext.job.semanticIndexProgress")}>
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          {settingsPresentation ? active ? <LoaderCircle size={18} className="shrink-0 text-primary motion-safe:animate-spin" aria-hidden="true" />
            : job.state === "succeeded" ? <CircleCheck size={18} className="shrink-0 text-primary" aria-hidden="true" />
              : <CircleAlert size={18} className="shrink-0 text-destructive" aria-hidden="true" /> : null}
          <strong className="text-sm" role="status" aria-live="polite">{stateLabels[job.state]}</strong>
        </div>
        {active && props.onCancel ? <Button type="button" size="sm" variant="ghost" disabled={props.cancelPending || (!recoveryRequired && (job.state === "cancelling" || progress?.phase === "committing" || (progress?.phase === "complete" && !job.followupRequested)))} onClick={props.onCancel}>{recoveryRequired ? t("codeContext.job.recoverAndCancel") : compact ? t("codeContext.job.cancelCompaction") : job.followupRequested ? t("codeContext.job.cancelUpdateAndFollow") : t("codeContext.job.cancelUpdate")}</Button> : null}
      </div>
      {active ? (
        <>
          {job.followupRequested ? <p className="text-xs text-primary" role="status">{t("codeContext.job.oneManualFollowUp")}</p> : null}
          <p className="text-xs font-medium">{activity ? activityTitles[activity.stage] : progress ? phaseLabels[progress.phase] : compact ? t("codeContext.job.compactionHasNotStarted") : t("codeContext.job.thisBatchHasNot")}</p>
          {activity ? <div className="space-y-1 text-xs text-muted-foreground" role="status">
            <p>{activityLabels[activity.stage]}</p>
            {pollCountdownSeconds !== null ? <p>{batchStillProcessing
              ? pollCountdownSeconds > 0
                ? formatMessage(t("codeContext.job.batchStillProcessingChecking"), { pollCountdownSeconds })
                : t("codeContext.job.batchStillProcessingChecking2")
              : pollCountdownSeconds > 0
                ? formatMessage(t("codeContext.job.checkingBatchStatusIn"), { pollCountdownSeconds })
                : t("codeContext.job.checkingBatchStatusShortly")}</p> : null}
          </div> : null}
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{activity?.stage === "persisting" && progress?.unit === "files" ? t("codeContext.job.fileSnapshotsSaved") : t("codeContext.job.currentPhaseProgress")}</span>
            <span>{measured ? `${progress.completed} / ${progress.total} ${unitLabels[progress.unit]}` : progress?.unit === "files" ? formatMessage(t("codeContext.job.filesScannedCountingTotal"), { completed: progress.completed }) : t("codeContext.job.processingTotalNotYet")}</span>
          </div>
          {measured ? <Progress className="h-1.5 w-full" aria-label={t("codeContext.job.currentPhaseProgress")} value={progress.completed} max={progress.total!} />
            : <div role="progressbar" aria-label={t("codeContext.job.currentPhaseProgress")} className="h-1.5 overflow-hidden rounded-full bg-muted"><div className="h-full w-1/3 animate-pulse rounded-full bg-primary/40 motion-reduce:animate-none" /></div>}
          {/* File scope stays visible while embedding counts use batches or requests.
              当向量阶段按批次或请求计数时，仍直接展示文件范围。 */}
          {!compact && overallMeasured ? <p className="text-xs text-muted-foreground">
            {formatMessage(t("codeContext.job.filesInThisUpdate"), { overallTotal: progress.overallTotal!, overallCompleted: progress.overallCompleted ?? 0 })}
          </p> : null}
          {progress?.currentPath ? <p className="break-all text-xs text-muted-foreground" title={progress.currentPath}>
            {t("codeContext.job.currentFile")}: <span className="font-mono">{progress.currentPath}</span>
          </p> : null}
          <div className="vce-job-meta">
            <p className="text-xs text-muted-foreground">{t("codeContext.job.elapsed")}: {elapsedSeconds} {t("codeContext.job.s")}</p>
            {settingsPresentation ? <details className="vce-job-details">
              <summary>{t("codeContext.job.taskDetails")}</summary>
              {progressDetails}
            </details> : null}
          </div>
          {!settingsPresentation ? progressDetails : null}
          {quietSeconds >= 30 && activity?.stage !== "waiting" ? <p className="text-xs text-amber-700 dark:text-amber-400" role="status">{t("codeContext.job.theCurrentStepHas")}</p> : null}
          <p className="text-xs text-muted-foreground">{compact
            ? t("codeContext.job.compactionReusesExistingVectors")
            : props.queryAvailable === false
            ? t("codeContext.job.conversationsContinueSemanticRetrieval")
            : job.sourceCheckpointVersion !== null ? t("codeContext.job.conversationsAndExistingIndex") : t("codeContext.job.aiSemanticSearchBecomes")}</p>
        </>
      ) : <div className="space-y-1 text-xs text-muted-foreground"><p>{t("codeContext.job.jobFinished")}: {formatDateTime(job.updatedAtUnixMs, locale)}</p>{job.state !== "succeeded" && progress ? <p>{t("codeContext.job.stoppedDuring")}: {phaseLabels[progress.phase]}{activity ? ` · ${activityLabels[activity.stage]}` : ""}</p> : null}</div>}
      {recoveryStatus ? <p className="text-xs text-muted-foreground" role="status" aria-live="polite">{recoveryStatus}</p> : null}
      {technicalDiagnostics.length > 0 ? <details open={!active} className={active ? "text-xs text-muted-foreground" : "text-xs text-destructive"}>
        <summary className="cursor-pointer select-none">{t("codeContext.job.viewTechnicalDiagnostics")}</summary>
        <div className="mt-1 space-y-1">{technicalDiagnostics.map((diagnostic) => <p className="break-words" key={diagnostic}>{diagnostic}</p>)}</div>
      </details> : null}
      {props.onRetry && !compact && (job.state === "failed" || job.state === "cancelled" || job.state === "interrupted")
        ? <Button type="button" size="sm" variant="outline" onClick={props.onRetry}>{t("codeContext.job.retryIndexJob")}</Button> : null}
    </section>
  );
}
