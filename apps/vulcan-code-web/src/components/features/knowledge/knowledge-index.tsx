import * as React from "react";
import { Activity, Gauge, Hammer, Pause, RotateCcw, ShieldCheck, Sparkles, Square, Wrench } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { useI18n, type TranslationKey } from "@/i18n";
import { getKnowledgeEmbeddingStatus, getKnowledgeIndexIntegrity, getKnowledgeOperation } from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { KNOWLEDGE_MODE_COPY } from "./knowledge-mode";
import { fillKnowledgeText } from "./knowledge-visuals";
import { useKnowledgeRead } from "./use-knowledge-read";
import { knowledgeOperationLabel } from "@/lib/knowledge-status-labels";

// WebUI progress cadence is a client presentation policy; only one status poll may be pending.
// WebUI 进度节奏为客户端展示策略；同时仅可等待一次状态轮询。
const KNOWLEDGE_STATUS_POLL_MS = 2_000;

// Durable job states map to badge and progress tones; unknown states stay neutral.
// 持久作业状态映射到徽章及进度色调；未知状态保持中性。
const OPERATION_TONES: Readonly<Partial<Record<string, string>>> = { queued: "neutral", running: "primary", completed: "success", cancelled: "warning", failed: "danger" };

// Vector backlog segments share tones between the stacked bar and its legend.
// 向量积压分段在堆叠条及图例间共享色调。
const VECTOR_TONES = { ready: "success", pending: "warning", failed: "danger" } as const;

// Legend labels for the three vector backlog segments.
// 三个向量积压分段的图例标签。
const VECTOR_LEGEND: Readonly<Record<keyof typeof VECTOR_TONES, TranslationKey>> = { ready: "knowledge.index.legend.ready", pending: "knowledge.index.legend.pending", failed: "knowledge.index.legend.failed" };

// The parent serializes human lifecycle mutations; this component owns only progress reads.
// 父组件串行化人工生命周期变更；此组件仅拥有进度读取。
interface KnowledgeIndexProps {
  // Active current library.
  // 活动当前知识库。
  library: KnowledgeLibrarySummary;
  // A parent-owned mutation blocks new lifecycle actions.
  // 父组件拥有的变更阻止新生命周期动作。
  busy: boolean;
  // Starts the exact library's durable derived-index rebuild.
  // 开始精确知识库的持久派生索引重建。
  onRebuild: () => Promise<void>;
  // Repairs derived state through the service's explicit repair entry.
  // 通过服务明确修复入口修复派生状态。
  onRepair: () => Promise<void>;
  // Cancels the stable operation ID currently observed for this library.
  // 取消此库当前观测到的稳定操作身份。
  onCancel: (operationId: string) => Promise<void>;
  // Empty IDs retry all failed hashes and resume paused work; explicit IDs retry those current documents.
  // 空身份重试全部失败哈希及恢复暂停工作；明确身份重试这些当前文档。
  onRetry: (documentIds: string[]) => Promise<void>;
}

// Displays independent FTS integrity, durable jobs and live vector backlog without substituting another mode.
// 独立展示全文完整性、持久任务及实时向量积压，不替换为其他模式。
// Parameters: props fixes the library and its authorized lifecycle callbacks. Returns read failures, cancellation and explicit retries.
// 参数：props 固定知识库及其授权生命周期回调。返回读取失败、取消及显式重试。
export function KnowledgeIndex({ library, busy, onRebuild, onRepair, onCancel, onRetry }: KnowledgeIndexProps) {
  // Labels use catalog keys; service errors remain English selectable diagnostics.
  // 标签使用目录键；服务错误保持英文可选择诊断。
  const { t } = useI18n();
  // Failure offsets are live service continuations, independent of content-history offsets.
  // 失败偏移为实时服务续读值，独立于内容历史偏移。
  const [offsets, setOffsets] = React.useState([0]);
  // Mutation errors never become empty successful status snapshots.
  // 变更错误绝不变为空成功状态快照。
  const [error, setError] = React.useState<string | null>(null);
  // The mounted visit prevents delayed mutation failures from opening in another page.
  // 挂载访问防止延迟变更失败在另一页面出现。
  const active = React.useRef(true);
  React.useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  // Exact failure offset for this page.
  // 此页精确失败偏移。
  const offset = offsets[offsets.length - 1];
  // Each loader has a single explicit library/selector source.
  // 每个加载器具有单个明确库及选择器来源。
  const loadOperation = React.useCallback(() => getKnowledgeOperation({ libraryId: library.id }), [library.id]);
  // Current desired-space status stays independently readable when FTS inspection fails.
  // 全文检查失败时当前期望空间状态仍保持独立可读。
  const loadEmbedding = React.useCallback(() => getKnowledgeEmbeddingStatus({ libraryId: library.id, offset }), [library.id, offset]);
  // Integrity is a read-only authoritative comparison, not an implicit repair.
  // 完整性为只读权威比较，绝非隐式修复。
  const loadIntegrity = React.useCallback(() => getKnowledgeIndexIntegrity(library.id), [library.id]);
  // Stable-ID discovery is explicit on the initial latest-operation read.
  // 首次最新操作读取明确执行稳定身份发现。
  const operation = useKnowledgeRead(JSON.stringify([library.id, "operation", library.settings, library.headChangeSeq, library.updatedAtMs]), loadOperation);
  // Active and desired vector spaces remain separate in every response.
  // 每个响应中活动及期望向量空间保持分离。
  const embedding = useKnowledgeRead(JSON.stringify([library.id, "embeddings", library.settings, library.headChangeSeq, library.updatedAtMs, offset]), loadEmbedding);
  // FTS read errors keep their own retry action.
  // 全文读取错误保留自身重试动作。
  const integrity = useKnowledgeRead(JSON.stringify([library.id, "integrity", library.settings, library.headChangeSeq, library.updatedAtMs]), loadIntegrity);
  // Only currently running durable work is cancellable; historical success is not liveness.
  // 仅当前运行持久工作可取消；历史成功不代表实时活动。
  const running = operation.value !== null && (operation.value.state === "queued" || operation.value.state === "running");
  // A failed or pending job read cannot prove it is safe to start or cancel another durable job.
  // 失败或等待任务读取不能证明可安全开始或取消另一持久任务。
  const lifecycleBusy = busy || operation.loading || operation.error !== null;
  // Paused or exhausted work waits for an explicit human retry.
  // 暂停或耗尽工作等待明确人工重试。
  const watch = !busy && !operation.loading && !embedding.loading && !integrity.loading
    && operation.error === null && embedding.error === null && integrity.error === null
    && (running || (embedding.value !== null && embedding.value.pending > 0 && !embedding.value.paused));
  React.useEffect(() => {
    if (!watch) return;
    // One poll visit is revoked on selection, pagination, terminal state or read failure.
    // 一次轮询访问在选择、分页、终态或读取失败时被撤销。
    let live = true;
    // The timer is rescheduled only after all three independent reads finish.
    // 计时器仅在三个独立读取全部完成后重新安排。
    let timer: ReturnType<typeof setTimeout>;
    // Reads live state only; this loop never replays an indexing or retry mutation.
    // 仅读取实时状态；此循环绝不重放索引或重试变更。
    const poll = async (): Promise<void> => {
      await Promise.all([operation.refresh(true), embedding.refresh(true), integrity.refresh(true)]);
      if (live) timer = setTimeout(() => void poll(), KNOWLEDGE_STATUS_POLL_MS);
    };
    timer = setTimeout(() => void poll(), KNOWLEDGE_STATUS_POLL_MS);
    return () => { live = false; clearTimeout(timer); };
  }, [watch, operation.refresh, embedding.refresh, integrity.refresh]);

  // Executes one parent-owned action, then independently refreshes live status; returns after publication.
  // 执行一个父组件拥有动作，再独立刷新实时状态；发布后返回。
  // Parameters: action is a fixed exact-owner lifecycle call. Errors remain visible on this mounted visit.
  // 参数：action 为固定精确所有者生命周期调用。错误在此挂载访问保持可见。
  async function perform(action: () => Promise<void>): Promise<void> {
    setError(null);
    try { await action(); await Promise.all([operation.refresh(true), embedding.refresh(true), integrity.refresh(true)]); }
    catch (reason) { if (active.current) setError(reason instanceof Error ? reason.message : String(reason)); }
  }

  // Job progress is a bounded ratio; an empty job renders an empty track rather than dividing by zero.
  // 作业进度为有界比例；空作业渲染空轨道，不除以零。
  const progress = operation.value === null || operation.value.totalItems === 0 ? 0 : Math.min(100, (operation.value.processedItems / operation.value.totalItems) * 100);
  // Vector totals partition the stacked bar into ready, pending and failed segments.
  // 向量总数将堆叠条划分为就绪、等待及失败段。
  const vectorTotal = embedding.value === null ? 0 : embedding.value.ready + embedding.value.pending + embedding.value.failed;
  return <>
    <div className="knowledge-toolbar">
      <p className="knowledge-hint"><Gauge size={14} aria-hidden="true" />{t(KNOWLEDGE_MODE_COPY[library.settings === null ? "browse" : library.settings.indexMode].description)}</p>
      <div className="knowledge-actions">
        <Button variant="outline" size="sm" disabled={lifecycleBusy || running} onClick={() => void perform(onRebuild)}><Hammer size={14} />{t("knowledge.rebuildIndex")}</Button>
        <Button variant="outline" size="sm" disabled={lifecycleBusy || running || integrity.value === null || integrity.value.healthy} onClick={() => void perform(onRepair)}><Wrench size={14} />{t("knowledge.repairIndex")}</Button>
        {running && operation.value !== null ? <Button variant="outline" size="sm" disabled={lifecycleBusy} onClick={() => {
          // The observed operation ID stays fixed while the cancel promise executes.
          // 取消 Promise 执行期间观测操作身份保持固定。
          const observed = operation.value;
          if (observed !== null) void perform(() => onCancel(observed.id));
        }}><Square size={12} />{t("knowledge.index.cancel")}</Button> : null}
      </div>
    </div>
    {error !== null ? <pre role="alert" className="knowledge-error">{error}</pre> : null}
    {operation.loading && operation.value === null ? <StreamPlaceholder label={t("knowledge.index.loading")} /> : null}
    <div className="knowledge-panels">
      {operation.error !== null ? <KnowledgeReadFailure error={operation.error} onRetry={operation.refresh} /> : null}
      {!operation.loading && operation.error === null && operation.value === null ? <article className="knowledge-panel"><div className="knowledge-panel-head"><h3><Activity size={14} aria-hidden="true" />{t("knowledge.index.latest")}</h3></div><p className="knowledge-panel-line">{t("knowledge.index.noJob")}</p></article> : null}
      {operation.value !== null ? <article className="knowledge-panel">
        <div className="knowledge-panel-head"><h3><Activity size={14} aria-hidden="true" />{t("knowledge.index.latest")}</h3><span className="knowledge-badge" data-tone={OPERATION_TONES[operation.value.state] ?? "neutral"} title={operation.value.state}>{knowledgeOperationLabel(operation.value.state, t)}</span></div>
        <div className="knowledge-panel-figure">{Math.round(progress)}% <small>{fillKnowledgeText(t("knowledge.index.operationProgress"), { done: operation.value.processedItems, total: operation.value.totalItems })}</small></div>
        <div className="knowledge-progress" data-tone={OPERATION_TONES[operation.value.state] ?? "neutral"} role="progressbar" aria-valuemin={0} aria-valuemax={operation.value.totalItems} aria-valuenow={operation.value.processedItems}><span style={{ width: progress + "%" }} /></div>
        {running && operation.value.currentItem !== null ? <p className="knowledge-panel-line">{fillKnowledgeText(t("knowledge.index.current"), { item: operation.value.currentItem })}</p> : null}
        {operation.value.errorMessage !== null ? <pre role="alert" className="knowledge-error">{operation.value.errorMessage}</pre> : null}
        {operation.value.warnings.map((warning, index) => <pre className="knowledge-warning" key={index}>{warning}</pre>)}
      </article> : null}
      {integrity.error !== null ? <KnowledgeReadFailure error={integrity.error} onRetry={integrity.refresh} /> : null}
      {integrity.value !== null ? <article className="knowledge-panel">
        <div className="knowledge-panel-head"><h3><ShieldCheck size={14} aria-hidden="true" />{t("knowledge.index.integrity")}</h3><span className="knowledge-badge" data-tone={integrity.value.healthy ? "success" : "warning"}>{t(integrity.value.healthy ? "knowledge.index.statusHealthy" : "knowledge.index.statusRepair")}</span></div>
        <p className="knowledge-panel-line">{t(integrity.value.healthy ? "knowledge.index.healthy" : "knowledge.index.needsRepair")}</p>
        <p className="knowledge-panel-line">{fillKnowledgeText(t("knowledge.index.fts"), { rows: integrity.value.indexedChunkCount, mismatches: integrity.value.mismatchedRows })}</p>
        <p className="knowledge-panel-line">{fillKnowledgeText(t("knowledge.index.indexedThrough"), { indexed: integrity.value.indexedChangeSeq, head: integrity.value.headChangeSeq })}</p>
        <p className="knowledge-panel-line">{fillKnowledgeText(t("knowledge.index.dictionaryRevision"), { indexed: integrity.value.indexedDictionaryRevision, current: integrity.value.dictionaryRevision })}</p>
      </article> : null}
      {embedding.error !== null ? <KnowledgeReadFailure error={embedding.error} onRetry={() => { if (offset === 0) void embedding.refresh(); else setOffsets([0]); }} /> : null}
      {embedding.value !== null ? <article className={"knowledge-panel" + (embedding.value.failures.length > 0 ? " knowledge-panel-wide" : "")}>
        <div className="knowledge-panel-head"><h3><Sparkles size={14} aria-hidden="true" />{t("knowledge.index.vectors")}</h3>{embedding.value.paused ? <span className="knowledge-badge" data-tone="warning"><Pause size={11} aria-hidden="true" />{t("knowledge.index.paused")}</span> : null}</div>
        {embedding.value.desiredSpaceId === null ? <p className="knowledge-panel-line">{t("knowledge.index.vectorsDisabled")}</p> : <>
          <div className="knowledge-stacked" role="img" aria-label={fillKnowledgeText(t("knowledge.index.vectorCounts"), { ready: embedding.value.ready, pending: embedding.value.pending, failed: embedding.value.failed })}>
            {vectorTotal > 0 ? (["ready", "pending", "failed"] as const).map((key) => <span data-tone={VECTOR_TONES[key]} key={key} style={{ width: (embedding.value === null ? 0 : embedding.value[key] / vectorTotal * 100) + "%" }} />) : null}
          </div>
          <div className="knowledge-legend">{(["ready", "pending", "failed"] as const).map((key) => <span key={key}><span className="knowledge-dot" data-tone={VECTOR_TONES[key]} />{t(VECTOR_LEGEND[key])} <b>{embedding.value === null ? 0 : embedding.value[key]}</b></span>)}</div>
          {embedding.value.activeSpaceId !== embedding.value.desiredSpaceId ? <p className="knowledge-panel-line">{t("knowledge.index.switching")}</p> : null}
          <div><Button variant="outline" size="sm" disabled={lifecycleBusy || running || (embedding.value.failed === 0 && !(embedding.value.paused && embedding.value.pending > 0))} onClick={() => void perform(() => onRetry([]))}><RotateCcw size={14} />{t("knowledge.index.retryAll")}</Button></div>
          {embedding.value.failures.length > 0 ? <h4 className="knowledge-panel-line">{t("knowledge.index.failures")}</h4> : null}
          {embedding.value.failures.map((failure) => <div className="knowledge-failure" key={failure.chunkId + ":" + failure.contentSha256}>
            <strong title={failure.path}>{failure.path}</strong>
            <Button variant="ghost" size="sm" disabled={lifecycleBusy || running} onClick={() => void perform(() => onRetry([failure.documentId]))}>{t("knowledge.index.retryDocument")}</Button>
            <pre>{failure.lastError}</pre>
            <small>{fillKnowledgeText(t("knowledge.index.attempts"), { count: failure.attempts })}</small>
          </div>)}
          {offsets.length > 1 || embedding.value.nextOffset !== null ? <div className="knowledge-pager"><Button size="sm" variant="outline" disabled={offsets.length === 1} onClick={() => setOffsets((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button><Button size="sm" variant="outline" disabled={embedding.value.nextOffset === null} onClick={() => {
            // Current status supplies the only next failure offset.
            // 当前状态提供唯一下一个失败偏移。
            const next = embedding.value === null ? null : embedding.value.nextOffset;
            if (next !== null) setOffsets((current) => [...current, next]);
          }}>{t("knowledge.page.next")}</Button></div> : null}
        </>}
      </article> : null}
    </div>
  </>;
}
