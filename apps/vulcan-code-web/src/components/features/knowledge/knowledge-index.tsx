import * as React from "react";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { useI18n } from "@/i18n";
import { getKnowledgeEmbeddingStatus, getKnowledgeIndexIntegrity, getKnowledgeOperation } from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { useKnowledgeRead } from "./use-knowledge-read";
import { knowledgeOperationLabel } from "@/lib/knowledge-status-labels";

// WebUI progress cadence is a client presentation policy; only one status poll may be pending.
// WebUI 进度节奏为客户端展示策略；同时仅可等待一次状态轮询。
const KNOWLEDGE_STATUS_POLL_MS = 2_000;

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

  return <section className="space-y-4">
    <div className="flex flex-wrap gap-2">
      <Button variant="outline" disabled={lifecycleBusy || running} onClick={() => void perform(onRebuild)}>{t("knowledge.rebuildIndex")}</Button>
      <Button variant="outline" disabled={lifecycleBusy || running || integrity.value === null || integrity.value.healthy} onClick={() => void perform(onRepair)}>{t("knowledge.repairIndex")}</Button>
      {running && operation.value !== null ? <Button variant="outline" disabled={lifecycleBusy} onClick={() => {
        // The observed operation ID stays fixed while the cancel promise executes.
        // 取消 Promise 执行期间观测操作身份保持固定。
        const observed = operation.value;
        if (observed !== null) void perform(() => onCancel(observed.id));
      }}>{t("knowledge.index.cancel")}</Button> : null}
    </div>
    {error !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{error}</pre> : null}
    {operation.error !== null ? <KnowledgeReadFailure error={operation.error} onRetry={operation.refresh} /> : null}
    {operation.loading && operation.value === null ? <StreamPlaceholder label={t("knowledge.index.loading")} /> : null}
    {operation.value !== null ? <article className="space-y-2 rounded-md border p-3">
      <h3 className="font-medium">{t("knowledge.index.latest")}</h3><p>{t("knowledge.index.operationProgress").replace("{done}", String(operation.value.processedItems)).replace("{total}", String(operation.value.totalItems))} · <span title={operation.value.state}>{knowledgeOperationLabel(operation.value.state, t)}</span></p>
      {operation.value.errorMessage !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{operation.value.errorMessage}</pre> : null}
      {operation.value.warnings.map((warning, index) => <pre className="select-text whitespace-pre-wrap break-words text-sm" key={index}>{warning}</pre>)}
    </article> : null}
    {integrity.error !== null ? <KnowledgeReadFailure error={integrity.error} onRetry={integrity.refresh} /> : null}
    {integrity.value !== null ? <article className="space-y-2 rounded-md border p-3"><h3 className="font-medium">{t("knowledge.index.integrity")}</h3><p>{t(integrity.value.healthy ? "knowledge.index.healthy" : "knowledge.index.needsRepair")}</p><p>{t("knowledge.index.fts").replace("{rows}", String(integrity.value.indexedChunkCount)).replace("{mismatches}", String(integrity.value.mismatchedRows))}</p></article> : null}
    {embedding.error !== null ? <KnowledgeReadFailure error={embedding.error} onRetry={() => { if (offset === 0) void embedding.refresh(); else setOffsets([0]); }} /> : null}
    {embedding.value !== null ? <article className="space-y-3 rounded-md border p-3">
      <h3 className="font-medium">{t("knowledge.index.vectors")}</h3>
      {embedding.value.desiredSpaceId === null ? <p>{t("knowledge.index.vectorsDisabled")}</p> : <>
        <p>{t("knowledge.index.vectorCounts").replace("{ready}", String(embedding.value.ready)).replace("{pending}", String(embedding.value.pending)).replace("{failed}", String(embedding.value.failed))}</p>
        {embedding.value.paused ? <p>{t("knowledge.index.paused")}</p> : null}
        {embedding.value.activeSpaceId !== embedding.value.desiredSpaceId ? <p>{t("knowledge.index.switching")}</p> : null}
        <Button variant="outline" disabled={lifecycleBusy || running || (embedding.value.failed === 0 && !(embedding.value.paused && embedding.value.pending > 0))} onClick={() => void perform(() => onRetry([]))}>{t("knowledge.index.retryAll")}</Button>
        <ul className="space-y-2">{embedding.value.failures.map((failure) => <li className="rounded border p-2" key={failure.chunkId + ":" + failure.contentSha256}><strong className="break-words">{failure.path}</strong><pre className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{failure.lastError}</pre><Button variant="outline" size="sm" disabled={lifecycleBusy || running} onClick={() => void perform(() => onRetry([failure.documentId]))}>{t("knowledge.index.retryDocument")}</Button></li>)}</ul>
        <div className="flex gap-2"><Button size="sm" variant="outline" disabled={offsets.length === 1} onClick={() => setOffsets((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button><Button size="sm" variant="outline" disabled={embedding.value.nextOffset === null} onClick={() => {
          // Current status supplies the only next failure offset.
          // 当前状态提供唯一下一个失败偏移。
          const next = embedding.value === null ? null : embedding.value.nextOffset;
          if (next !== null) setOffsets((current) => [...current, next]);
        }}>{t("knowledge.page.next")}</Button></div>
      </>}
    </article> : null}
  </section>;
}
