import { Button } from "@/components/ui/button";
import type { VceJobSummary } from "@/types";
import { isVceJobActive } from "./use-vce-project-job";

// Derive one shared synchronization action from the acknowledged receipt, never from a local click count.
// 从已确认回执派生统一同步操作，绝不依赖本地点击计数。
// job is project-scoped and chinese selects copy. Returns the label and disabled state for both entry points.
// job 限于当前项目，chinese 选择文案。返回两个入口共用的标签与禁用状态。
export function vceSynchronizationControl(job: VceJobSummary | null, chinese: boolean): { disabled: boolean; label: string } {
  // copy selects localized text without changing authoritative job state.
  // copy 选择本地化文本，不改变权威作业状态。
  const copy = (zh: string, en: string) => chinese ? zh : en;
  if (!isVceJobActive(job) || job === null) return { disabled: false, label: copy("手动更新索引", "Synchronize index now") };
  if (job.state === "cancelling") return { disabled: true, label: copy("正在取消更新", "Cancelling update") };
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
  const stateLabels = {
    queued: copy("已排队，等待索引资源", "Queued for indexing resources"),
    running: job.kind === "initialize" ? copy("正在创建语义索引", "Creating semantic index")
      : job.kind === "rebuild" ? copy("正在重建语义索引", "Rebuilding semantic index")
      : copy("正在更新语义索引", "Updating semantic index"),
    cancelling: copy("正在取消并清理未发布数据", "Cancelling and cleaning unpublished data"),
    succeeded: copy("语义索引已同步", "Semantic index synchronized"),
    failed: copy("本次索引更新失败", "Index update failed"),
    cancelled: copy("本次索引更新已取消", "Index update cancelled"),
    interrupted: copy("上次索引作业中断，请检查后重试", "Previous index job interrupted; inspect before retrying"),
  };
  const phaseLabels = {
    scanning: copy("扫描文件", "Scanning files"),
    reconciling: copy("核对文件变化", "Reconciling file changes"),
    chunking: copy("分析与切分代码", "Analyzing and chunking code"),
    embedding: copy("生成向量", "Generating embeddings"),
    writing: copy("写入索引", "Writing index"),
    committing: copy("提交检查点", "Committing checkpoint"),
    complete: copy("确认更新结果", "Confirming update result"),
  };
  const unitLabels = { files: copy("文件", "files"), blobs: copy("代码单元", "blobs"), batches: copy("批次", "batches"), none: "" };
  const progress = job.progress;
  // An active receipt error means terminal persistence needs explicit recovery, so cancellation stays available.
  // 活动作业带有回执错误时表示终态持久化需要显式恢复，因此仍保持取消入口可用。
  const recoveryRequired = active && job.error !== null;
  // measured requires a meaningful denominator; unknown and zero totals stay indeterminate.
  // measured 要求有意义的分母；未知与零总量保持不确定态。
  const measured = progress !== null && progress.total !== null && progress.total > 0 && progress.unit !== "none";
  const overallMeasured = progress !== null
    && progress.overallTotal !== undefined
    && progress.overallTotal !== null
    && progress.overallTotal > 0;
  return (
    <section className="space-y-2 rounded-lg border bg-muted/20 p-3" aria-label={copy("语义索引进度", "Semantic index progress")}>
      <div className="flex items-center justify-between gap-3">
        <strong className="text-sm">{stateLabels[job.state]}</strong>
        {active && props.onCancel ? <Button type="button" size="sm" variant="ghost" disabled={props.cancelPending || job.state === "cancelling" || progress?.phase === "committing" || (progress?.phase === "complete" && !job.followupRequested && !recoveryRequired)} onClick={props.onCancel}>{recoveryRequired ? copy("恢复并取消", "Recover and cancel") : job.followupRequested ? copy("取消更新及后续同步", "Cancel update and follow-up") : copy("取消更新", "Cancel update")}</Button> : null}
      </div>
      {active ? (
        <>
          {job.followupRequested ? <p className="text-xs text-primary" role="status">{copy("已合并一次后续手动同步；本次结束后核对文件，不受自动间隔限制。", "One manual follow-up is scheduled to reconcile files after this update, without the automatic cooldown.")}</p> : null}
          <p className="text-xs text-muted-foreground">{progress ? phaseLabels[progress.phase] : copy("当前批次尚未开始扫描或调用模型", "This batch has not started file scans or model calls")}{progress && progress.unit !== "none" ? ` · ${progress.completed}${progress.total !== null ? ` / ${progress.total}` : ""} ${unitLabels[progress.unit]}` : ""}</p>
          {overallMeasured ? (
            <div className="space-y-1">
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span>{copy("整体进度", "Overall progress")}</span>
                <span>{progress.overallCompleted ?? 0} / {progress.overallTotal}</span>
              </div>
              <progress className="h-1.5 w-full accent-primary" aria-label={copy("整体进度", "Overall progress")} value={progress.overallCompleted ?? 0} max={progress.overallTotal!} />
            </div>
          ) : null}
          <div className="text-xs text-muted-foreground">{copy("当前执行范围", "Current execution scope")}</div>
          <progress className="h-1.5 w-full accent-primary" aria-label={copy("当前阶段进度", "Current phase progress")} value={measured ? progress.completed : undefined} max={measured ? progress.total! : undefined} />
          {progress?.currentPath ? <p className="truncate text-xs text-muted-foreground" title={progress.currentPath}>{progress.currentPath}</p> : null}
          <p className="text-xs text-muted-foreground">{props.queryAvailable === false
            ? copy("会话可继续；语义检索须满足模型、项目与索引条件。", "Conversations continue; semantic retrieval requires valid models, project policy, and an index.")
            : job.sourceCheckpointVersion !== null ? copy("会话和已有索引查询仍可正常使用。", "Conversations and existing index queries remain available.") : copy("首次索引完成后，AI 才会获得语义检索工具。", "AI semantic search becomes available after initialization completes.")}</p>
        </>
      ) : <p className="text-xs text-muted-foreground">{copy("作业结束时间", "Job finished")}: {new Date(job.updatedAtUnixMs).toLocaleString(props.chinese ? "zh-CN" : "en-US")}</p>}
      {job.error ? <p className="break-words text-xs text-destructive">{job.error}</p> : null}
      {props.onRetry && (job.state === "failed" || job.state === "cancelled" || job.state === "interrupted")
        ? <Button type="button" size="sm" variant="outline" onClick={props.onRetry}>{copy("重试索引任务", "Retry index job")}</Button> : null}
    </section>
  );
}
