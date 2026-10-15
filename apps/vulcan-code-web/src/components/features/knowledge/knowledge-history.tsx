import * as React from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { useI18n } from "@/i18n";
import { formatDateTime } from "@/lib/display-format";
import { getKnowledgeChange, listKnowledgeChanges } from "@/service/knowledge-base-service";
import type { KnowledgeChange, KnowledgeChangeItem, KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { KnowledgeHistoricalDocumentReader } from "./knowledge-document-reader";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { useKnowledgeRead } from "./use-knowledge-read";
import { knowledgeChangeLabel } from "@/lib/knowledge-status-labels";

// This client display length is a viewport policy; the service remains the request upper-bound authority.
// 此客户端展示长度为视口策略；请求上限权威仍归服务。
const HISTORY_PAGE_LENGTH = 20;

// Returns an item's display path from its explicitly nullable create/delete images.
// 从条目明确可空的创建及删除镜像返回展示路径。
// Parameters: item is one immutable typed row pair. Returns after-path or deletion's before-path; rejects an impossible empty pair.
// 参数：item 为一个不可变强类型行对。返回后镜像路径或删除的前镜像路径；拒绝不可能的双空行对。
export function knowledgeHistoryItemPath(item: KnowledgeChangeItem): string {
  if (item.after !== null) return item.after.path;
  if (item.before !== null) return item.before.path;
  throw new Error("ERR_KNOWLEDGE_HISTORY_EMPTY_IMAGE_PAIR");
}

// One immutable change owns its paged images and optional document comparison.
// 一个不可变变更拥有其分页镜像及可选文档比较。
interface KnowledgeHistoryDetailProps {
  // Exact physical library.
  // 精确物理库。
  libraryId: string;
  // Exact immutable selected change.
  // 精确不可变选定变更。
  change: KnowledgeChange;
  // Closes this read-only detail.
  // 关闭此只读详情。
  onClose: () => void;
}

// Reads bounded history images and loads bodies only for a consciously selected document.
// 读取有界历史镜像，且仅为主动选定文档加载正文。
// Parameters: props freezes library/change ownership. Returns exact pre/post comparisons with no editable fields.
// 参数：props 冻结库及变更归属。返回精确前后比较，不含可编辑字段。
function KnowledgeHistoryDetail({ libraryId, change, onClose }: KnowledgeHistoryDetailProps) {
  // Current-language labels do not alter immutable image values.
  // 当前语言标签不改变不可变镜像值。
  const { t } = useI18n();
  // Offsets are supplied by the bounded immutable-item endpoint.
  // 偏移由有界不可变条目端点提供。
  const [offsets, setOffsets] = React.useState([0]);
  // The selected comparison preserves one exact image pair even when another page is inspected.
  // 选定比较保留一个精确镜像对，即使检查另一页。
  const [comparison, setComparison] = React.useState<KnowledgeChangeItem | null>(null);
  // Read only the final explicit immutable offset.
  // 仅读取最后一个明确不可变偏移。
  const offset = offsets[offsets.length - 1];
  // The exact change/offset owns this request independently of current library content.
  // 精确变更及偏移拥有此请求，独立于当前库内容。
  const load = React.useCallback(() => getKnowledgeChange({ libraryId, seq: change.seq, offset }), [libraryId, change.seq, offset]);
  // Superseded details cannot replace another change or library.
  // 已替代详情不能覆盖另一变更或知识库。
  const read = useKnowledgeRead(JSON.stringify([libraryId, change.seq, offset]), load);
  return <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}><DialogContent size="xl" layout="document" scrollMode="fixed-footer">
    <DialogHeader><DialogTitle>{t("knowledge.history.change").replace("{seq}", String(change.seq))}</DialogTitle></DialogHeader>
    <div className="min-h-0 space-y-3 overflow-auto p-4">
      <p className="select-text">{change.note}</p>
      <details><summary>{t("knowledge.history.actor")}</summary><pre className="select-text whitespace-pre-wrap break-words">{JSON.stringify(change.actor, null, 2)}</pre></details>
      {read.loading ? <StreamPlaceholder label={t("knowledge.history.loading")} /> : null}
      {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
      {read.value !== null ? <>
        <ul className="space-y-1">{read.value.items.map((item) => <li key={item.entity + ":" + item.entityKey}><button type="button" className="rounded px-2 py-1 text-left underline" onClick={() => setComparison(item)}>{knowledgeHistoryItemPath(item)}</button></li>)}</ul>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" disabled={offsets.length === 1} onClick={() => setOffsets((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button>
          <Button size="sm" variant="outline" disabled={read.value.nextOffset === null} onClick={() => {
            // Capture this immutable page's exact continuation.
            // 捕获此不可变页精确续读值。
            const next = read.value === null ? null : read.value.nextOffset;
            if (next !== null) setOffsets((current) => [...current, next]);
          }}>{t("knowledge.page.next")}</Button>
        </div>
      </> : null}
      {comparison !== null ? <section className="space-y-3 border-t pt-3" key={comparison.entity + ":" + comparison.entityKey}>
        <h3 className="font-medium">{knowledgeHistoryItemPath(comparison)}</h3>
        <div className="grid gap-4 lg:grid-cols-2">{(["before", "after"] as const).map((side) => <section className="min-w-0 space-y-2" key={side}>
          <h4 className="font-medium">{t(side === "before" ? "knowledge.history.before" : "knowledge.history.after")}</h4>
          {comparison[side] === null ? <p className="text-muted-foreground">{t("knowledge.history.absent")}</p>
            : comparison.entity === "document" ? <>
              <KnowledgeHistoricalDocumentReader libraryId={libraryId} documentId={comparison.entityKey} seq={change.seq} side={side} />
              <details><summary>{t("knowledge.history.metadata")}</summary><pre className="max-h-64 select-text overflow-auto whitespace-pre-wrap break-words text-xs">{JSON.stringify(comparison[side], null, 2)}</pre></details>
            </> : <pre className="select-text whitespace-pre-wrap break-words rounded border p-3">{JSON.stringify(comparison[side], null, 2)}</pre>}
        </section>)}</div>
      </section> : null}
    </div>
    <DialogFooter><Button variant="outline" onClick={onClose}>{t("conversation.action.cancel")}</Button></DialogFooter>
  </DialogContent></Dialog>;
}

// Human-only historical rollback boundary, separate from every AI content action.
// 仅人工历史还原边界，与全部 AI 内容动作分离。
interface KnowledgeHistoryProps {
  // Active authoritative library owner.
  // 活动权威库所有者。
  library: KnowledgeLibrarySummary;
  // Prevents overlapping page mutations.
  // 阻止重叠页面变更。
  busy: boolean;
  // Commits a new restore change only after this component's confirmation.
  // 仅在此组件确认后提交新还原变更。
  onRestore: (targetSeq: number) => Promise<void>;
}

// Renders permanent immutable history with explicit confirmation before a new restore change.
// 渲染永久不可变历史，并在新还原变更前明确确认。
// Parameters: props supplies exact owner, busy state and confirmed mutation callback. Returns paged summaries and read-only diffs.
// 参数：props 提供精确所有者、忙碌状态及已确认变更回调。返回分页摘要及只读差异。
export function KnowledgeHistory({ library, busy, onRestore }: KnowledgeHistoryProps) {
  // Timestamp formatting shares the active UI locale.
  // 时间戳格式共享当前界面语言。
  const { t, locale } = useI18n();
  // Each prior boundary is an immutable sequence, independent of later appends.
  // 每个先前边界均为不可变序号，独立于稍后追加。
  const [boundaries, setBoundaries] = React.useState<(number | null)[]>([null]);
  // The detail dialog owns one exact immutable summary.
  // 详情对话框拥有一个精确不可变摘要。
  const [detail, setDetail] = React.useState<KnowledgeChange | null>(null);
  // A restore target freezes the exact reviewed sequence until confirmation.
  // 还原目标冻结精确已审阅序号，直到确认。
  const [restoreTarget, setRestoreTarget] = React.useState<number | null>(null);
  // Failed restores remain visible without replaying the mutation on read retry.
  // 失败还原保持可见，读取重试不重放变更。
  const [error, setError] = React.useState<string | null>(null);
  // The synchronous guard prevents duplicate confirmation before busy state is rendered.
  // 同步守卫在忙碌状态渲染前防止重复确认。
  const restoring = React.useRef(false);
  // The last explicit boundary selects this summary page.
  // 最后一个明确边界选择此摘要页。
  const beforeSeq = boundaries[boundaries.length - 1];
  // One extra summary detects a next page without assuming contiguous sequences.
  // 额外一个摘要检测下一页，不假设序号连续。
  const load = React.useCallback(() => listKnowledgeChanges({ libraryId: library.id, beforeSeq, limit: HISTORY_PAGE_LENGTH + 1 }), [library.id, beforeSeq]);
  // Current head changes refresh summaries without changing historical identities.
  // 当前内容头变化刷新摘要，不改变历史身份。
  const read = useKnowledgeRead(JSON.stringify([library.id, library.headChangeSeq, beforeSeq]), load);
  // The viewport retains only its requested display rows.
  // 视口仅保留其请求展示行。
  const rows = read.value === null ? [] : read.value.slice(0, HISTORY_PAGE_LENGTH);

  // Restores the frozen target exactly once and closes only on successful committed refresh.
  // 精确还原冻结目标一次，仅在成功提交及刷新后关闭。
  // No parameters; returns after the confirmed transaction or visible failure.
  // 无参数；已确认事务或可见失败后返回。
  async function confirmRestore(): Promise<void> {
    if (restoreTarget === null || busy || restoring.current) return;
    restoring.current = true;
    setError(null);
    try { await onRestore(restoreTarget); setRestoreTarget(null); setBoundaries([null]); read.refresh(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { restoring.current = false; }
  }

  return <section className="space-y-3">
    <p className="text-sm text-muted-foreground">{t("knowledge.history.description")}</p>
    <div className="flex gap-2"><Button variant="outline" size="sm" onClick={() => { setBoundaries([null]); read.refresh(); }}>{t("knowledge.refresh")}</Button><Button variant="outline" size="sm" disabled={busy} onClick={() => { setError(null); setRestoreTarget(0); }}>{t("knowledge.history.restoreEmpty")}</Button></div>
    {read.loading ? <StreamPlaceholder label={t("knowledge.history.loading")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
    {read.value !== null ? <>
      <ol className="space-y-2">{rows.map((change) => <li key={change.seq} className="flex flex-wrap items-start justify-between gap-2 rounded-md border p-3">
        <div><strong>{t("knowledge.history.change").replace("{seq}", String(change.seq))}</strong><small className="ml-2 text-muted-foreground">{formatDateTime(change.atMs, locale)}</small><p className="select-text break-words">{change.note}</p><span title={change.operation} className="text-xs">{knowledgeChangeLabel(change.operation, t)}</span></div>
        <div className="flex gap-2"><Button size="sm" variant="outline" onClick={() => setDetail(change)}>{t("knowledge.history.compare")}</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => { setError(null); setRestoreTarget(change.seq); }}>{t("knowledge.history.restore")}</Button></div>
      </li>)}</ol>
      {rows.length === 0 ? <p className="text-muted-foreground">{t("knowledge.history.empty")}</p> : null}
      <div className="flex gap-2"><Button variant="outline" size="sm" disabled={boundaries.length === 1} onClick={() => setBoundaries((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button>
        <Button variant="outline" size="sm" disabled={read.value.length <= HISTORY_PAGE_LENGTH} onClick={() => {
          // The last displayed summary, not an assumed sequence offset, anchors continuation.
          // 最后展示摘要锚定续读，而非假设的序号偏移。
          const last = rows.at(-1);
          if (last !== undefined) setBoundaries((current) => [...current, last.seq]);
        }}>{t("knowledge.page.next")}</Button></div>
    </> : null}
    {detail !== null ? <KnowledgeHistoryDetail key={detail.seq} libraryId={library.id} change={detail} onClose={() => setDetail(null)} /> : null}
    <AlertDialog open={restoreTarget !== null} onOpenChange={(open) => { if (!open && !busy && !restoring.current) setRestoreTarget(null); }}><AlertDialogContent>
      <AlertDialogHeader><AlertDialogTitle>{t("knowledge.history.restore")}</AlertDialogTitle><AlertDialogDescription>{t("knowledge.history.restoreConfirm").replace("{name}", library.name).replace("{seq}", String(restoreTarget))}</AlertDialogDescription></AlertDialogHeader>
      {error !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{error}</pre> : null}
      <AlertDialogFooter><AlertDialogCancel disabled={busy}>{t("conversation.action.cancel")}</AlertDialogCancel><AlertDialogAction disabled={busy} onClick={(event) => { event.preventDefault(); void confirmRestore(); }}>{t("knowledge.history.restore")}</AlertDialogAction></AlertDialogFooter>
    </AlertDialogContent></AlertDialog>
  </section>;
}
