import * as React from "react";
import { useI18n, type TranslationKey } from "@/i18n";
import { formatDurationSeconds } from "@/lib/usage-format";
import { Activity, Braces, Database, Gauge, LoaderCircle, Server, Sparkles } from "lucide-react";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { KnowledgeSkippedDocuments } from "./knowledge-skipped-documents";
import type {
  KnowledgeImportJobSummary,
  KnowledgeOperationObservability,
  KnowledgeOperationRequestSummary,
} from "@/types";

// EMPTY_OBSERVABILITY keeps the progress surface compatible with rows created before observability migration.
// EMPTY_OBSERVABILITY 让进度界面兼容可观测性字段迁移前创建的历史任务行。
const EMPTY_OBSERVABILITY: KnowledgeOperationObservability = {
  llmRequestCount: 0,
  llmRetryCount: 0,
  embeddingRequestCount: 0,
  generatedVectorCount: 0,
  inputTokens: 0,
  outputTokens: 0,
  cacheReadTokens: 0,
  cacheWriteTokens: 0,
  currentRequest: null,
  recentRequests: [],
};

// KnowledgeOperationProgress renders a compact, refresh-safe streaming operation panel.
// KnowledgeOperationProgress 渲染紧凑且可刷新恢复的流式操作面板。
// Parameters: props supplies the authoritative operation snapshot; returns measured progress, known/unknown usage and live request/output details.
// 参数：props 提供权威操作快照；返回实测进度、已知或未知用量及实时请求和输出详情。
export function KnowledgeOperationProgress(props: {
  // Durable operation snapshot returned by the service.
  // 服务返回的持久化操作快照。
  job: KnowledgeImportJobSummary | null;
  // Whether the operation is still expected to accept cancellation.
  // 操作是否仍然应该接受取消。
  running: boolean;
  // Whether cancellation has been requested but not yet observed by the worker.
  // 是否已经请求取消但工作器尚未观察到取消。
  cancelRequested: boolean;
  // Terminal outcome recorded by the command path.
  // 命令路径记录的终态结果。
  outcome: "completed" | "cancelled" | "failed" | null;
}) {
  const { t } = useI18n();
  const job = props.job;
  const observability = job?.observability ?? EMPTY_OBSERVABILITY;
  // Skip counts are part of processed progress but never inflate generated document counts.
  // 忽略数量计入处理进度，但绝不虚增已生成文档数量。
  const skippedCount = observability.skippedDocuments?.length ?? 0;
  const currentRequest = observability.currentRequest;
  const isImport = job?.operationKind === "import";
  const total = isImport ? job?.totalFiles ?? 0 : job?.totalIndexItems ?? 0;
  const processed = isImport ? job?.processedFiles ?? 0 : job?.processedIndexItems ?? 0;
  const percent = total > 0 ? Math.min(processed >= total ? 100 : 99, Math.max(0, Math.round((processed / total) * 100))) : null;
  // outcome falls back to the durable job when reopening a completed operation.
  // outcome 在重新打开已结束操作时使用持久化任务终态。
  const outcome = props.outcome ?? job?.state;
  const status = props.cancelRequested
    ? t("knowledge.experience.cancelling")
    : outcome === "failed"
      ? t("knowledge.experience.failed")
      : outcome === "cancelled"
        ? t("knowledge.experience.cancelled")
        : outcome === "completed"
          ? t("knowledge.experience.completed")
          : props.running
            ? t("knowledge.experience.running")
            : t("knowledge.experience.waiting");

  return (
    <div className="knowledge-operation-progress" data-outcome={outcome}>
      <div className="knowledge-operation-progress-head">
        <div>
          <strong>{status}</strong>
          <span>{t(knowledgeOperationPhaseKey(job?.phase))}</span>
        </div>
        <span className="knowledge-operation-progress-percent">
          {percent === null ? t("knowledge.experience.processing") : `${percent}%`}
        </span>
      </div>

      <div
        aria-label={status}
        aria-valuemax={percent === null ? undefined : 100}
        aria-valuemin={percent === null ? undefined : 0}
        aria-valuenow={percent ?? undefined}
        className={`knowledge-operation-progress-bar${percent === null && props.running ? " indeterminate" : ""}`}
        role="progressbar"
      >
        <span style={{ width: `${percent ?? (props.running ? 38 : 0)}%` }} />
      </div>

      <div className="knowledge-operation-progress-meta">
        <span>{t("knowledge.experience.items").replace("{done}", String(processed)).replace("{total}", total ? String(total) : "?")}</span>
        {isImport ? <span>{job?.processedBytes ?? 0} bytes</span> : null}
        {job?.operationKind === "display_build" && skippedCount > 0 ? <span>{t("knowledge.skipped.counts").replace("{generated}", String(Math.max(0, processed - skippedCount))).replace("{skipped}", String(skippedCount))}</span> : null}
      </div>

      <KnowledgeSkippedDocuments documents={observability.skippedDocuments} />

      <div className="knowledge-operation-stat-grid">
        <OperationStat icon={<Sparkles size={14} />} label={t("knowledge.experience.llmRequests")} value={observability.llmRequestCount} detail={t("knowledge.experience.retries").replace("{count}", String(observability.llmRetryCount))} />
        <OperationStat icon={<Database size={14} />} label={t("knowledge.experience.embeddingRequests")} value={observability.embeddingRequestCount} detail={t("knowledge.experience.generated").replace("{count}", String(observability.generatedVectorCount))} />
        <OperationStat icon={<Gauge size={14} />} label={t("knowledge.experience.inputTokens")} value={formatCount(observability.inputTokens, t("knowledge.experience.unknown"))} detail={t("knowledge.experience.cumulative")} />
        <OperationStat icon={<Activity size={14} />} label={t("knowledge.experience.outputTokens")} value={formatCount(observability.outputTokens, t("knowledge.experience.unknown"))} detail={t("knowledge.experience.cumulative")} />
        <OperationStat icon={<Server size={14} />} label={t("knowledge.experience.cacheTokens")} value={`${formatCount(observability.cacheReadTokens, t("knowledge.experience.unknown"))} / ${formatCount(observability.cacheWriteTokens, t("knowledge.experience.unknown"))}`} detail={t("knowledge.experience.cumulative")} />
      </div>

      <section className="knowledge-operation-live">
        <div className="knowledge-operation-section-title">
          <span><Braces size={14} /> {t("knowledge.experience.currentRequest")}</span>
          {currentRequest ? <span>{currentRequest.provider} / {currentRequest.model}</span> : <span>{t("knowledge.experience.noRequest")}</span>}
        </div>
        {currentRequest ? (
          <div className="knowledge-operation-request-meta">
            <span>{t(knowledgeOperationPhaseKey(currentRequest.phase))} · {currentRequest.protocol || "provider"}</span>
            <span>{t(knowledgeRequestStatusKey(currentRequest.status))} · {t("knowledge.experience.requestTokens").replace("{input}", formatCount(currentRequest.inputTokens, t("knowledge.experience.unknown"))).replace("{output}", formatCount(currentRequest.outputTokens, t("knowledge.experience.unknown")))}</span>
          </div>
        ) : (
          <div className="knowledge-operation-empty">{t("knowledge.experience.preparing")}</div>
        )}
      </section>

      {currentRequest ? (
        <details className="knowledge-operation-output">
          <summary>{t("knowledge.feedback.output")}</summary>
          <div className="knowledge-operation-stream">
            <MarkdownMessage key={currentRequest.requestId} content={currentRequest.outputMarkdown || t("knowledge.experience.waitingStream")} defaultCodeWrap eager streaming={props.running && currentRequest.status === "running"} />
          </div>
        </details>
      ) : null}

      <section className="knowledge-operation-history">
        <div className="knowledge-operation-section-title">
          <span><LoaderCircle size={14} /> {t("knowledge.experience.recentRequests")}</span>
          <span title={currentRequest?.label}>{currentRequest?.label ?? ""}</span>
        </div>
        {observability.recentRequests.length === 0 ? (
          <div className="knowledge-operation-empty">{t("knowledge.experience.noHistory")}</div>
        ) : (
          <div className="knowledge-operation-history-list">
            {observability.recentRequests.map((request) => <RecentRequestRow key={`${request.requestId}-${request.durationMs ?? 0}`} request={request} />)}
          </div>
        )}
      </section>

      {job?.currentFile ? <div className="knowledge-operation-current-file">{t("knowledge.experience.currentFile")}<code>{job.currentFile}</code></div> : null}
    </div>
  );
}

// OperationStat renders one responsive metric tile without expanding the dialog vertically.
// OperationStat 渲染一个响应式指标卡片，避免垂直撑大对话框。
function OperationStat(props: { icon: React.ReactNode; label: string; value: React.ReactNode; detail: string }) {
  return (
    <div className="knowledge-operation-stat">
      <span>{props.icon} {props.label}</span>
      <strong>{props.value}</strong>
      <small>{props.detail}</small>
    </div>
  );
}

// RecentRequestRow renders one compact request history item.
// RecentRequestRow 渲染一条紧凑的请求历史记录。
function RecentRequestRow(props: { request: KnowledgeOperationRequestSummary }) {
  // t translates durable request statuses for the user.
  // t 为用户翻译持久化请求状态。
  const { t } = useI18n();
  const request = props.request;
  return (
    <div className="knowledge-operation-history-row">
      <span className="knowledge-operation-history-status" data-status={request.status}>{t(knowledgeRequestStatusKey(request.status))}</span>
      <span className="knowledge-operation-history-label" title={request.label}>{request.label}</span>
      <span>{request.model}</span>
      <span>{request.durationMs == null ? "—" : formatDurationSeconds(request.durationMs)}</span>
    </div>
  );
}

// knowledgeRequestStatusKey maps a persisted status to its localized display label.
// knowledgeRequestStatusKey 将持久化状态映射为本地化显示标签。
// Parameters: status is the backend status. Returns: the corresponding translation key.
// 参数：status 是后端状态。返回值：对应翻译键。
function knowledgeRequestStatusKey(status: string): TranslationKey {
  switch (status) {
    case "completed": return "knowledge.experience.completed";
    case "failed": return "knowledge.experience.failed";
    case "cancelled": return "knowledge.experience.cancelled";
    case "running": return "knowledge.experience.running";
    default: return "knowledge.experience.waiting";
  }
}

// knowledgeOperationPhaseKey localizes known job phases without exposing internal phase identifiers.
// knowledgeOperationPhaseKey 翻译已知任务阶段，避免显示内部阶段标识。
// Parameters: phase is the optional backend phase. Returns: its user-facing translation key.
// 参数：phase 是可选后端阶段。返回值：面向用户的翻译键。
function knowledgeOperationPhaseKey(phase: string | undefined): TranslationKey {
  switch (phase) {
    case "curating": return "knowledge.experience.displayOperation";
    case "embedding": return "knowledge.experience.vectorOperation";
    case "scanning": return "knowledge.importPhase.scanning";
    case "parsing": return "knowledge.importPhase.parsing";
    case "committing": return "knowledge.importPhase.committing";
    case "indexing": return "knowledge.importPhase.indexing";
    case "repairing": return "knowledge.importPhase.repairing";
    case "rebuilding": return "knowledge.importPhase.rebuilding";
    case "verifying": return "knowledge.importPhase.verifying";
    case "finalizing": return "knowledge.importPhase.finalizing";
    default: return "knowledge.experience.preparing";
  }
}

// formatCount keeps unknown provider usage explicit instead of pretending it is zero.
// formatCount 明确展示供应商未提供的用量，而不是伪装成零。
// Parameters: value is optional measured usage and unknownLabel is explicit missing-data copy; returns formatted known usage or that unknown label.
// 参数：value 是可选实测用量，unknownLabel 是明确的缺失文案；返回已知用量格式化结果或该未知标签。
function formatCount(value: number | null | undefined, unknownLabel: string): string {
  return value == null ? unknownLabel : value.toLocaleString();
}
