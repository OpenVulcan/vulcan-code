import * as React from "react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { VceJobSummary } from "@/types";
import { isVceJobActive } from "./use-vce-project-job";

// Describe one active durable job by its real operation kind and lifecycle state.
// 按真实操作类型和生命周期状态描述一项活动持久作业。
// job is the acknowledged project receipt and chinese selects locale. Returns null for terminal work.
// job 为已确认项目回执，chinese 选择语言；终态作业返回空值。
export function vceActiveJobStatusLabel(job: VceJobSummary | null, chinese: boolean): string | null {
  if (!isVceJobActive(job) || job === null) return null;
  // copy selects display language without weakening the durable state distinction.
  // copy 选择显示语言，不弱化持久状态区分。
  const copy = (zh: string, en: string) => chinese ? zh : en;
  if (job.state === "cancelling") {
    return job.kind === "compact" ? copy("正在取消物理整理", "Cancelling physical compaction")
      : copy("正在取消索引任务", "Cancelling index job");
  }
  if (job.kind === "compact") {
    return job.state === "queued" ? copy("物理整理已排队", "Physical compaction queued")
      : copy("正在整理物理向量表", "Compacting physical vector tables");
  }
  if (job.kind === "initialize") return copy("正在初始化", "Initializing");
  if (job.kind === "rebuild") return copy("正在重建", "Rebuilding");
  return job.state === "queued" ? copy("索引更新已排队", "Index update queued")
    : copy("正在更新", "Updating");
}

// Derive one shared synchronization action from the acknowledged receipt, never from a local click count.
// 从已确认回执派生统一同步操作，绝不依赖本地点击计数。
// job is project-scoped and chinese selects copy. Returns the label and disabled state for both entry points.
// job 限于当前项目，chinese 选择文案。返回两个入口共用的标签与禁用状态。
export function vceSynchronizationControl(job: VceJobSummary | null, chinese: boolean): { disabled: boolean; label: string } {
  // copy selects localized text without changing authoritative job state.
  // copy 选择本地化文本，不改变权威作业状态。
  const copy = (zh: string, en: string) => chinese ? zh : en;
  if (!isVceJobActive(job) || job === null) return { disabled: false, label: copy("手动更新索引", "Synchronize index now") };
  if (job.state === "cancelling") return { disabled: true, label: job.kind === "compact" ? copy("正在取消整理", "Cancelling compaction") : copy("正在取消更新", "Cancelling update") };
  if (job.kind === "compact") return { disabled: true, label: job.state === "queued" ? copy("物理整理已排队", "Physical compaction queued") : copy("物理整理进行中", "Physical compaction in progress") };
  if (job.kind !== "synchronize") return { disabled: true, label: copy("索引任务进行中", "Index job in progress") };
  if (job.state === "queued") return { disabled: true, label: copy("已排队等待更新", "Update queued") };
  if (job.followupRequested) return { disabled: true, label: copy("已安排后续更新", "Follow-up update scheduled") };
  return { disabled: false, label: copy("本次完成后再同步", "Synchronize again after this update") };
}

// Render truthful phase progress for a durable job without affecting conversation controls.
// 展示持久作业的真实阶段进度，不影响会话控件。
// props owns the exact receipt, locale, and cancellation callback. Returns a compact status panel.
// props 持有精确回执、语言与取消回调。返回紧凑状态面板。
export function VceJobProgress(props: {
  // Exact durable receipt selected by project ownership.
  // 按项目归属选择的精确持久回执。
  job: VceJobSummary;
  // Whether Chinese UI copy is selected.
  // 是否选择中文界面文案。
  chinese: boolean;
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
}) {
  // copy selects bilingual display labels without exposing machine state names.
  // copy 选择双语展示标签，不暴露机器状态名。
  const copy = (zh: string, en: string) => props.chinese ? zh : en;
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
    queued: compact ? copy("已排队，等待物理整理资源", "Queued for physical compaction resources") : copy("已排队，等待索引资源", "Queued for indexing resources"),
    running: compact ? copy("正在整理物理向量表", "Compacting physical vector tables")
      : job.kind === "initialize" ? copy("正在创建语义索引", "Creating semantic index")
      : job.kind === "rebuild" ? copy("正在重建语义索引", "Rebuilding semantic index")
      : copy("正在更新语义索引", "Updating semantic index"),
    cancelling: compact ? copy("正在取消整理，等待当前安全步骤结束", "Cancelling compaction after the current safe step") : copy("正在取消并清理未发布数据", "Cancelling and cleaning unpublished data"),
    succeeded: compact ? copy("物理向量表整理完成", "Physical vector table compaction completed") : copy("语义索引已同步", "Semantic index synchronized"),
    failed: compact ? copy("本次物理整理失败", "Physical compaction failed") : copy("本次索引更新失败", "Index update failed"),
    cancelled: compact ? copy("本次物理整理已取消", "Physical compaction cancelled") : copy("本次索引更新已取消", "Index update cancelled"),
    interrupted: compact ? copy("物理整理已中断，请重新生成计划", "Physical compaction was interrupted; generate a new plan") : copy("上次索引作业中断，请检查后重试", "Previous index job interrupted; inspect before retrying"),
  };
  const phaseLabels = {
    scanning: copy("扫描文件", "Scanning files"),
    reconciling: copy("核对文件变化", "Reconciling file changes"),
    chunking: copy("分析与切分代码", "Analyzing and chunking code"),
    embedding: copy("生成向量", "Generating embeddings"),
    writing: compact ? copy("整理向量表", "Compacting vector table") : copy("写入索引", "Writing index"),
    committing: copy("提交检查点", "Committing checkpoint"),
    complete: compact ? copy("确认整理结果", "Confirming compaction result") : copy("确认更新结果", "Confirming update result"),
  };
  const unitLabels = { files: copy("文件", "files"), blobs: copy("代码单元", "blobs"), batches: copy("批次", "batches"), requests: copy("请求项", "requests"), tables: copy("张表", "tables"), none: "" };
  const providerBatchStateLabels = {
    validating: copy("供应商正在校验批次", "Provider is validating the batch"),
    in_progress: copy("供应商正在生成向量", "Provider is generating embeddings"),
    finalizing: copy("供应商正在整理批次结果", "Provider is finalizing batch results"),
    completed: copy("供应商批次已完成", "Provider batch completed"),
    failed: copy("供应商批次失败", "Provider batch failed"),
    expired: copy("供应商批次已过期", "Provider batch expired"),
    cancelling: copy("供应商正在取消批次", "Provider is cancelling the batch"),
    cancelled: copy("供应商批次已取消", "Provider batch cancelled"),
  };
  const compactionTableStateLabels = {
    pending: copy("待提交", "Pending"),
    queued: copy("已排队", "Queued"),
    running: copy("整理中", "Running"),
    succeeded: copy("已完成", "Completed"),
    failed: copy("失败", "Failed"),
    cancelled: copy("已取消", "Cancelled"),
    state_lost: copy("整理操作状态已丢失", "Compaction operation state lost"),
    interrupted: copy("已中断", "Interrupted"),
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
    preparing: copy("准备向量请求", "Preparing embedding requests"),
    planning: copy("汇总向量请求", "Assembling embedding requests"),
    validating: copy("校验本地向量计划", "Validating the local embedding plan"),
    persisting: copy("保存本地向量计划", "Saving the local embedding plan"),
    planning_tasks: copy("规划向量批量任务", "Planning embedding Batch tasks"),
    admitting: copy("保存向量批次执行状态", "Saving embedding batch execution states"),
    requesting: copy("同步请求向量", "Requesting synchronous embeddings"),
    uploading: copy("上传向量批量文件", "Uploading embedding Batch files"),
    submitting: copy("提交向量批量任务", "Submitting embedding Batch tasks"),
    waiting: copy("等待向量批量处理", "Waiting for embedding Batch processing"),
    polling: copy("查询向量批量处理状态", "Checking embedding Batch processing"),
    collecting: copy("获取并校验向量结果", "Retrieving and validating embeddings"),
  };
  // Each label separates local preparation, synchronous requests and acknowledged remote work.
  // 每项文案区分本地准备、同步请求与已确认的远端工作。
  const activityLabels = {
    preparing: copy("正在准备向量请求与持久化计划，尚未提交向量。", "Preparing embedding requests and the durable plan; embeddings have not been submitted."),
    planning: copy("文件分析已完成，正在汇总内容与路径向量请求。", "File analysis completed; assembling content and path embedding requests."),
    validating: copy("正在校验文件快照与完整向量计划，尚未调用向量接口。", "Validating file snapshots and the complete embedding plan; no embedding request has been made."),
    persisting: copy("正在将文件快照与向量计划写入本地存储，尚未调用向量接口。", "Writing file snapshots and embedding plans to local storage; no embedding request has been made."),
    planning_tasks: copy("正在本地生成、校验并保存当前向量计划的批量任务记录。", "Building, validating and saving Batch task records for the current embedding plan locally."),
    admitting: copy("正在本地逐条保存向量批次执行状态；完成后上传或提交当前计划。", "Saving each embedding batch execution state locally, before uploading or submitting the current plan."),
    requesting: copy("正在请求同步向量接口，等待当前请求返回。", "Calling the synchronous embedding endpoint; waiting for the current response."),
    uploading: copy("正在上传批量向量输入文件，尚未提交 Batch 任务。", "Uploading embedding input; the Batch task has not been submitted yet."),
    submitting: copy("正在提交批量向量任务，等待供应商确认。", "Submitting the embedding Batch; waiting for provider acknowledgement."),
    waiting: copy("批量向量任务已提交，正在等待供应商处理；系统会自动查询状态。", "Embedding Batch submitted; waiting for provider processing. Status will be checked automatically."),
    polling: copy("正在查询已提交的批量向量任务状态。", "Checking the submitted embedding Batch status."),
    collecting: copy("正在获取并校验批量向量结果。", "Retrieving and validating embedding Batch results."),
  };
  // An active receipt error means terminal persistence needs explicit recovery, so cancellation stays available.
  // 活动作业带有回执错误时表示终态持久化需要显式恢复，因此仍保持取消入口可用。
  const recoveryRequired = active && job.error !== null;
  // recoveryStatus keeps an active persistence diagnostic localized and out of the terminal error slot.
  // recoveryStatus 将活动持久化诊断本地化并移出终态错误位置。
  const recoveryStatus = recoveryRequired
    ? copy("终态回执尚未完成写入，作业仍保留以便恢复。请点击“恢复并取消”释放作业。", "The terminal receipt is not fully persisted; the job remains recoverable. Choose “Recover and cancel” to release it.")
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
  return (
    <section className="min-w-0 w-full space-y-2 overflow-hidden rounded-lg border bg-muted/20 p-3" aria-label={compact ? copy("物理索引整理进度", "Physical index compaction progress") : copy("语义索引进度", "Semantic index progress")}>
      <div className="flex items-center justify-between gap-3">
        <strong className="text-sm">{stateLabels[job.state]}</strong>
        {active && props.onCancel ? <Button type="button" size="sm" variant="ghost" disabled={props.cancelPending || (!recoveryRequired && (job.state === "cancelling" || progress?.phase === "committing" || (progress?.phase === "complete" && !job.followupRequested)))} onClick={props.onCancel}>{recoveryRequired ? copy("恢复并取消", "Recover and cancel") : compact ? copy("取消整理", "Cancel compaction") : job.followupRequested ? copy("取消更新及后续同步", "Cancel update and follow-up") : copy("取消更新", "Cancel update")}</Button> : null}
      </div>
      {active ? (
        <>
          {job.followupRequested ? <p className="text-xs text-primary" role="status">{copy("已合并一次后续手动同步；本次结束后核对文件，不受自动间隔限制。", "One manual follow-up is scheduled to reconcile files after this update, without the automatic cooldown.")}</p> : null}
          <p className="text-xs font-medium">{activity ? activityTitles[activity.stage] : progress ? phaseLabels[progress.phase] : compact ? copy("整理作业尚未开始，不会调用模型", "Compaction has not started and will not call a model") : copy("当前批次尚未开始扫描或调用模型", "This batch has not started file scans or model calls")}</p>
          {progress?.providerBatchState ? <p className="text-xs text-muted-foreground" role="status">{providerBatchStateLabels[progress.providerBatchState]}</p> : null}
          {activity ? <div className="space-y-1 text-xs text-muted-foreground" role="status">
            <p>{activityLabels[activity.stage]}</p>
            {activity.providerBatchId ? <p className="truncate" title={activity.providerBatchId}>{copy("批量任务", "Batch task")}: {activity.providerBatchId}</p> : null}
            {pollCountdownSeconds !== null ? <p>{batchStillProcessing
              ? pollCountdownSeconds > 0
                ? copy(`批量任务尚未完成，${pollCountdownSeconds} 秒后继续查询`, `Batch still processing; checking again in ${pollCountdownSeconds} s`)
                : copy("批量任务尚未完成，即将再次查询", "Batch still processing; checking again shortly")
              : pollCountdownSeconds > 0
                ? copy(`${pollCountdownSeconds} 秒后查询批量任务状态`, `Checking Batch status in ${pollCountdownSeconds} s`)
                : copy("即将查询批量任务状态", "Checking Batch status shortly")}</p> : null}
          </div> : null}
          {(
            <div className="space-y-1">
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span>{!compact && progress && ["chunking", "embedding", "writing"].includes(progress.phase) ? copy("整体进度（已写入文件）", "Overall progress (files written)") : copy("整体进度", "Overall progress")}</span>
                <span>{overallMeasured ? `${progress.overallCompleted ?? 0} / ${progress.overallTotal}` : copy("正在确定范围", "Determining scope")}</span>
              </div>
              <Progress className="h-1.5 w-full" aria-label={copy("整体进度", "Overall progress")} value={overallMeasured ? progress.overallCompleted ?? 0 : null} max={overallMeasured ? progress.overallTotal! : undefined} />
            </div>
          )}
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{copy("当前阶段进度", "Current phase progress")}</span>
            <span>{measured ? `${progress.completed} / ${progress.total} ${unitLabels[progress.unit]}` : copy("处理中，总量待确定", "Processing; total not yet known")}</span>
          </div>
          {measured ? <Progress className="h-1.5 w-full" aria-label={copy("当前阶段进度", "Current phase progress")} value={progress.completed} max={progress.total!} />
            : <div role="progressbar" aria-label={copy("当前阶段进度", "Current phase progress")} className="h-1.5 overflow-hidden rounded-full bg-muted"><div className="h-full w-1/3 animate-pulse rounded-full bg-primary/40 motion-reduce:animate-none" /></div>}
          <p className="text-xs text-muted-foreground">{copy("已用时", "Elapsed")}: {elapsedSeconds} {copy("秒", "s")} · {copy("距最近进度更新", "Since last progress update")}: {quietSeconds} {copy("秒", "s")}</p>
          {quietSeconds >= 30 && activity?.stage !== "waiting" ? <p className="text-xs text-amber-700 dark:text-amber-400" role="status">{copy("当前步骤尚未返回新的完成回执，最后确认的处理阶段如上。", "The current step has not returned another completion receipt; its last confirmed stage is shown above.")}</p> : null}
          {compact && job.compaction ? <div className="space-y-1 rounded-md border bg-background/60 p-2">{job.compaction.tables.map((table) => (
            <p className="text-xs text-muted-foreground" key={table.role}>{table.role === "content" ? copy("内容向量表", "Content vector table") : copy("路径向量表", "Path vector table")} · {copy("片段", "fragments")} {table.plannedFragmentCount} → {table.currentFragmentCount} · {copy("轮次", "round")} {table.round} · {compactionTableStateLabels[table.state]}</p>
          ))}</div> : null}
          <p className="text-xs text-muted-foreground">{compact
            ? copy("整理复用现有向量；会话和已发布索引查询仍可继续使用。", "Compaction reuses existing vectors; conversations and published-index queries remain available.")
            : props.queryAvailable === false
            ? copy("会话可继续；语义检索须满足模型、项目与索引条件。", "Conversations continue; semantic retrieval requires valid models, project policy, and an index.")
            : job.sourceCheckpointVersion !== null ? copy("会话和已有索引查询仍可正常使用。", "Conversations and existing index queries remain available.") : copy("首次索引完成后，AI 才会获得语义检索工具。", "AI semantic search becomes available after initialization completes.")}</p>
        </>
      ) : <div className="space-y-1 text-xs text-muted-foreground"><p>{copy("作业结束时间", "Job finished")}: {new Date(job.updatedAtUnixMs).toLocaleString(props.chinese ? "zh-CN" : "en-US")}</p>{job.state !== "succeeded" && progress ? <p>{copy("停止阶段", "Stopped during")}: {phaseLabels[progress.phase]}{activity ? ` · ${activityLabels[activity.stage]}` : ""}</p> : null}</div>}
      {recoveryStatus ? <p className="text-xs text-muted-foreground" role="status" aria-live="polite">{recoveryStatus}</p> : null}
      {technicalDiagnostics.length > 0 ? <details open={!active} className={active ? "text-xs text-muted-foreground" : "text-xs text-destructive"}>
        <summary className="cursor-pointer select-none">{copy("查看技术诊断", "View technical diagnostics")}</summary>
        <div className="mt-1 space-y-1">{technicalDiagnostics.map((diagnostic) => <p className="break-words" key={diagnostic}>{diagnostic}</p>)}</div>
      </details> : null}
      {props.onRetry && !compact && (job.state === "failed" || job.state === "cancelled" || job.state === "interrupted")
        ? <Button type="button" size="sm" variant="outline" onClick={props.onRetry}>{copy("重试索引任务", "Retry index job")}</Button> : null}
    </section>
  );
}
