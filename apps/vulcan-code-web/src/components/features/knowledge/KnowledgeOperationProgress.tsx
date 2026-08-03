import * as React from "react";
import { Activity, Braces, Database, Gauge, LoaderCircle, Server, Sparkles } from "lucide-react";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
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
  const job = props.job;
  const observability = job?.observability ?? EMPTY_OBSERVABILITY;
  const currentRequest = observability.currentRequest;
  const isImport = job?.operationKind === "import";
  const total = isImport ? job?.totalFiles ?? 0 : job?.totalIndexItems ?? 0;
  const processed = isImport ? job?.processedFiles ?? 0 : job?.processedIndexItems ?? 0;
  const percent = total > 0 ? Math.min(100, Math.max(0, Math.round((processed / total) * 100))) : null;
  const status = props.cancelRequested
    ? "正在请求取消"
    : props.outcome === "failed"
      ? "操作失败"
      : props.outcome === "cancelled"
        ? "操作已取消"
        : props.outcome === "completed"
          ? "操作完成"
          : props.running
            ? "正在执行"
            : "等待操作";

  return (
    <div className="knowledge-operation-progress">
      <div className="knowledge-operation-progress-head">
        <div>
          <strong>{status}</strong>
          <span>{job?.phase ?? "queued"}</span>
        </div>
        <span className="knowledge-operation-progress-percent">
          {percent === null ? "处理中" : `${percent}%`}
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
        <span style={{ width: `${percent ?? 38}%` }} />
      </div>

      <div className="knowledge-operation-progress-meta">
        <span>{processed} / {total || "?"} 项</span>
        <span>{job?.processedBytes ?? 0} bytes</span>
      </div>

      <div className="knowledge-operation-stat-grid">
        <OperationStat icon={<Sparkles size={14} />} label="LLM 请求" value={observability.llmRequestCount} detail={`重试 ${observability.llmRetryCount}`} />
        <OperationStat icon={<Database size={14} />} label="向量请求" value={observability.embeddingRequestCount} detail={`生成 ${observability.generatedVectorCount}`} />
        <OperationStat icon={<Gauge size={14} />} label="输入 Token" value={formatCount(observability.inputTokens)} detail="累计" />
        <OperationStat icon={<Activity size={14} />} label="输出 Token" value={formatCount(observability.outputTokens)} detail="累计" />
        <OperationStat icon={<Server size={14} />} label="缓存读 / 写" value={`${formatCount(observability.cacheReadTokens)} / ${formatCount(observability.cacheWriteTokens)}`} detail="累计" />
      </div>

      <section className="knowledge-operation-live">
        <div className="knowledge-operation-section-title">
          <span><Braces size={14} /> 当前请求</span>
          {currentRequest ? <span>{currentRequest.provider} / {currentRequest.model}</span> : <span>尚未发起请求</span>}
        </div>
        {currentRequest ? (
          <>
            <div className="knowledge-operation-request-meta">
              <span>{currentRequest.phase} · {currentRequest.protocol || "provider"}</span>
              <span>{currentRequest.status} · 输入 {formatCount(currentRequest.inputTokens)} / 输出 {formatCount(currentRequest.outputTokens)}</span>
            </div>
            <div className="knowledge-operation-stream">
              <MarkdownMessage content={currentRequest.outputMarkdown || "```json\n等待 JSON 流输出…\n```"} eager streaming={props.running} />
            </div>
          </>
        ) : (
          <div className="knowledge-operation-empty">文件扫描或索引准备阶段，暂时没有 LLM 流输出。</div>
        )}
      </section>

      <section className="knowledge-operation-history">
        <div className="knowledge-operation-section-title">
          <span><LoaderCircle size={14} /> 最近请求（最多 3 条）</span>
          <span>{currentRequest?.label ?? ""}</span>
        </div>
        {observability.recentRequests.length === 0 ? (
          <div className="knowledge-operation-empty">暂无已完成请求记录。</div>
        ) : (
          <div className="knowledge-operation-history-list">
            {observability.recentRequests.map((request) => <RecentRequestRow key={`${request.requestId}-${request.durationMs ?? 0}`} request={request} />)}
          </div>
        )}
      </section>

      {job?.currentFile ? <div className="knowledge-operation-current-file">当前文件：<code>{job.currentFile}</code></div> : null}
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
  const request = props.request;
  return (
    <div className="knowledge-operation-history-row">
      <span className="knowledge-operation-history-status">{request.status}</span>
      <span className="knowledge-operation-history-label">{request.label}</span>
      <span>{request.model}</span>
      <span>{request.durationMs == null ? "—" : `${request.durationMs} ms`}</span>
    </div>
  );
}

// formatCount keeps unknown provider usage explicit instead of pretending it is zero.
// formatCount 明确展示供应商未提供的用量，而不是伪装成零。
function formatCount(value: number | null | undefined): string {
  return value == null ? "未提供" : value.toLocaleString();
}
