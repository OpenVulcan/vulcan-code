import * as React from "react";
import { Circle, GitCompare, History, Minus, MoveRight, PencilLine, Plus, RefreshCw, RotateCcw, type LucideIcon } from "lucide-react";
import { PageEmptyState } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { useI18n, type TranslationKey } from "@/i18n";
import { formatDateTime } from "@/lib/display-format";
import { getKnowledgeChange, listKnowledgeChanges } from "@/service/knowledge-base-service";
import type { KnowledgeChange, KnowledgeChangeItem, KnowledgeDocumentSummary, KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { KnowledgeHistoricalDocumentReader } from "./knowledge-document-reader";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { fillKnowledgeText, KnowledgeActorMarker, knowledgeActorLabel } from "./knowledge-visuals";
import { useKnowledgeRead } from "./use-knowledge-read";
import { knowledgeChangeLabel } from "@/lib/knowledge-status-labels";

// This client display length is a viewport policy; the service remains the request upper-bound authority.
// 此客户端展示长度为视口策略；请求上限权威仍归服务。
const HISTORY_PAGE_LENGTH = 20;

// Operation tones group additive, editing, structural, destructive and restorative changes.
// 操作色调将新增、编辑、结构、删除及还原变更分组。
const OPERATION_TONES: Readonly<Record<string, string>> = {
  create: "success", ingest: "success", import: "success", update: "info", edit: "info", folder: "info",
  move: "primary", delete: "danger", delete_folder: "danger", restore: "warning",
};

// Item kinds summarize one immutable image pair for the comparison list.
// 条目种类为比较列表概括一个不可变镜像对。
type KnowledgeItemKind = "added" | "removed" | "moved" | "modified";

// Each item kind owns a glyph, tone and catalog label.
// 每种条目种类拥有图标、色调及目录标签。
const ITEM_KINDS: Readonly<Record<KnowledgeItemKind, { icon: LucideIcon; tone: string; label: TranslationKey }>> = {
  added: { icon: Plus, tone: "success", label: "knowledge.history.kind.added" },
  removed: { icon: Minus, tone: "danger", label: "knowledge.history.kind.removed" },
  moved: { icon: MoveRight, tone: "info", label: "knowledge.history.kind.moved" },
  modified: { icon: PencilLine, tone: "warning", label: "knowledge.history.kind.modified" },
};

// Returns an item's display path from its explicitly nullable create/delete images.
// 从条目明确可空的创建及删除镜像返回展示路径。
// Parameters: item is one immutable typed row pair. Returns after-path or deletion's before-path; rejects an impossible empty pair.
// 参数：item 为一个不可变强类型行对。返回后镜像路径或删除的前镜像路径；拒绝不可能的双空行对。
export function knowledgeHistoryItemPath(item: KnowledgeChangeItem): string {
  if (item.after !== null) return item.after.path;
  if (item.before !== null) return item.before.path;
  throw new Error("ERR_KNOWLEDGE_HISTORY_EMPTY_IMAGE_PAIR");
}

// Classifies one image pair; returns added, removed, moved or modified.
// 分类一个镜像对；返回新增、删除、移动或修改。
// Parameters: item is one immutable typed row pair. A path change counts as a move even when other fields changed too.
// 参数：item 为一个不可变强类型行对。路径变化即视为移动，即使其他字段同时变化。
function knowledgeItemKind(item: KnowledgeChangeItem): KnowledgeItemKind {
  if (item.before === null) return "added";
  if (item.after === null) return "removed";
  return item.before.path !== item.after.path ? "moved" : "modified";
}

// One comparable field row: catalog label plus display text for each side.
// 一行可比较字段：目录标签及每侧展示文本。
interface KnowledgeFieldRow {
  // Catalog key naming the field.
  // 命名字段的目录键。
  label: TranslationKey;
  // Display value per side; null when that image is absent.
  // 每侧展示值；该镜像不存在时为空值。
  before: string | null;
  after: string | null;
}

// Projects one image pair into comparable display rows without inventing values for absent sides.
// 将一个镜像对投影为可比较展示行，不为缺失侧虚构值。
// Parameters: item is the immutable pair; t localizes closed vocabularies. Returns field rows in a fixed order.
// 参数：item 为不可变镜像对；t 本地化封闭词汇。按固定顺序返回字段行。
function knowledgeFieldRows(item: KnowledgeChangeItem, t: (key: TranslationKey) => string): KnowledgeFieldRow[] {
  if (item.entity === "folder") {
    return [
      { label: "knowledge.field.path", before: item.before?.path ?? null, after: item.after?.path ?? null },
      { label: "knowledge.field.description", before: item.before?.description ?? null, after: item.after?.description ?? null },
    ];
  }
  // Narrowed document images; the folder branch returned above.
  // 已收窄的文档镜像；目录分支已在上方返回。
  const { before, after } = item;
  // Each projector reads one field of a present document image.
  // 每个投影器读取一个已存在文档镜像的字段。
  const fields: [TranslationKey, (document: KnowledgeDocumentSummary) => string][] = [
    ["knowledge.field.title", (document) => document.title],
    ["knowledge.field.path", (document) => document.path],
    ["knowledge.reader.summary", (document) => document.summary],
    ["knowledge.reader.keywords", (document) => document.keywords.join(", ")],
    ["knowledge.field.embedPolicy", (document) => t(document.embedPolicy === "summary" ? "knowledge.reader.embed.summary" : document.embedPolicy === "full" ? "knowledge.reader.embed.full" : "knowledge.reader.embed.none")],
    ["knowledge.field.revision", (document) => String(document.revision)],
    ["knowledge.field.body", (document) => fillKnowledgeText(t("knowledge.reader.lineCount"), { count: document.lineCount }) + " · " + document.bodySha256.slice(0, 12)],
  ];
  return fields.map(([label, read]) => ({ label, before: before === null ? null : read(before), after: after === null ? null : read(after) }));
}

// Renders a before/after field table and, for documents, the immutable bodies that actually differ.
// 渲染前后字段表，并为文档渲染实际不同的不可变正文。
// Parameters: libraryId/seq own the bodies; item is the selected pair. Returns a read-only comparison.
// 参数：libraryId 及 seq 拥有正文；item 为选定镜像对。返回只读比较。
function KnowledgeChangeComparison({ libraryId, seq, item }: { libraryId: string; seq: number; item: KnowledgeChangeItem }) {
  // Field and side labels follow the active locale.
  // 字段及侧别标签遵循当前语言。
  const { t } = useI18n();
  // Unchanged document bodies are shown once from the after image.
  // 未变文档正文仅从后镜像显示一次。
  const sameBody = item.entity === "document" && item.before !== null && item.after !== null && item.before.bodySha256 === item.after.bodySha256;
  return <section className="knowledge-tab-panel">
    <table className="knowledge-diff-table">
      <thead><tr><th scope="col">{t("knowledge.history.field")}</th><th scope="col">{t("knowledge.history.before")}</th><th scope="col">{t("knowledge.history.after")}</th></tr></thead>
      <tbody>{knowledgeFieldRows(item, t).map((row) => <tr key={row.label} data-changed={row.before !== null && row.after !== null && row.before !== row.after}>
        <th scope="row">{t(row.label)}</th>
        <td data-absent={row.before === null}>{row.before ?? t("knowledge.history.absent")}</td>
        <td data-absent={row.after === null}>{row.after ?? t("knowledge.history.absent")}</td>
      </tr>)}</tbody>
    </table>
    {item.entity === "document" ? sameBody ? <>
      <p className="knowledge-hint">{t("knowledge.history.bodyUnchanged")}</p>
      <div className="knowledge-diff-side"><h4>{t("knowledge.history.after")}</h4><KnowledgeHistoricalDocumentReader libraryId={libraryId} documentId={item.entityKey} seq={seq} side="after" /></div>
    </> : <div className="knowledge-diff-bodies">{(["before", "after"] as const).map((side) => <div className="knowledge-diff-side" key={side}>
      <h4>{t(side === "before" ? "knowledge.history.before" : "knowledge.history.after")}</h4>
      {item[side] === null ? <p className="knowledge-diff-absent">{t("knowledge.history.absent")}</p> : <KnowledgeHistoricalDocumentReader libraryId={libraryId} documentId={item.entityKey} seq={seq} side={side} />}
    </div>)}</div> : null}
  </section>;
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

// Reads bounded history images and loads bodies only for a consciously selected item.
// 读取有界历史镜像，且仅为主动选定条目加载正文。
// Parameters: props freezes library/change ownership. Returns exact pre/post comparisons with no editable fields.
// 参数：props 冻结库及变更归属。返回精确前后比较，不含可编辑字段。
function KnowledgeHistoryDetail({ libraryId, change, onClose }: KnowledgeHistoryDetailProps) {
  // Current-language labels do not alter immutable image values.
  // 当前语言标签不改变不可变镜像值。
  const { t, locale } = useI18n();
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
  // Trace identities appear only when the stored actor carries them.
  // 仅当存储执行者携带时才显示追踪身份。
  const trace: [TranslationKey, string | null][] = [["knowledge.history.session", change.actor.sessionId], ["knowledge.history.turn", change.actor.turnId], ["knowledge.history.toolCall", change.actor.toolCallId]];
  return <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}><DialogContent size="xl" layout="document" scrollMode="fixed-footer">
    <DialogHeader>
      <DialogTitle>{fillKnowledgeText(t("knowledge.history.change"), { seq: change.seq })} · {knowledgeChangeLabel(change.operation, t)}</DialogTitle>
      <DialogDescription>{t(knowledgeActorLabel(change.actor.kind))} · {formatDateTime(change.atMs, locale)}</DialogDescription>
    </DialogHeader>
    <div className="knowledge-change-dialog-body">
      {change.note !== "" ? <p className="knowledge-summary">{change.note}</p> : null}
      {trace.some(([, value]) => value !== null) ? <details className="knowledge-disclosure"><summary>{t("knowledge.history.actor")}</summary>
        <dl className="knowledge-facts">{trace.filter(([, value]) => value !== null).map(([label, value]) => <React.Fragment key={label}><dt>{t(label)}</dt><dd>{value}</dd></React.Fragment>)}</dl>
      </details> : null}
      {read.loading ? <StreamPlaceholder label={t("knowledge.history.loading")} /> : null}
      {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
      {read.value !== null ? <div className="knowledge-change-layout">
        <nav className="knowledge-change-items" aria-label={t("knowledge.history.items")}>
          {read.value.items.map((item) => {
            // Kind presentation is derived from the immutable pair alone.
            // 种类展示仅由不可变镜像对推导。
            const kind = ITEM_KINDS[knowledgeItemKind(item)];
            const Icon = kind.icon;
            return <div className="knowledge-change-item" data-tone={kind.tone} aria-current={comparison !== null && comparison.entity === item.entity && comparison.entityKey === item.entityKey} key={item.entity + ":" + item.entityKey}>
              <Icon size={13} aria-hidden="true" /><button type="button" title={knowledgeHistoryItemPath(item)} onClick={() => setComparison(item)}>{knowledgeHistoryItemPath(item)}</button><small>{t(kind.label)}</small>
            </div>;
          })}
          {offsets.length > 1 || read.value.nextOffset !== null ? <div className="knowledge-pager">
            <Button size="sm" variant="ghost" disabled={offsets.length === 1} onClick={() => setOffsets((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button>
            <Button size="sm" variant="ghost" disabled={read.value.nextOffset === null} onClick={() => {
              // Capture this immutable page's exact continuation.
              // 捕获此不可变页精确续读值。
              const next = read.value === null ? null : read.value.nextOffset;
              if (next !== null) setOffsets((current) => [...current, next]);
            }}>{t("knowledge.page.next")}</Button>
          </div> : null}
        </nav>
        {comparison !== null ? <KnowledgeChangeComparison key={comparison.entity + ":" + comparison.entityKey} libraryId={libraryId} seq={change.seq} item={comparison} />
          : <PageEmptyState icon={<GitCompare />} title={t("knowledge.history.selectItem")} />}
      </div> : null}
    </div>
    <DialogFooter><Button variant="outline" onClick={onClose}>{t("knowledge.importDialog.close")}</Button></DialogFooter>
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
// Parameters: props supplies exact owner, busy state and confirmed mutation callback. Returns a paged timeline and read-only diffs.
// 参数：props 提供精确所有者、忙碌状态及已确认变更回调。返回分页时间线及只读差异。
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
  // The empty origin is reachable only on the final page, where the timeline actually begins.
  // 空起点仅在时间线真正开始的最后一页可达。
  const lastPage = read.value !== null && read.value.length <= HISTORY_PAGE_LENGTH;

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

  return <>
    <div className="knowledge-toolbar">
      <p className="knowledge-hint"><History size={14} aria-hidden="true" />{t("knowledge.history.description")}</p>
      <Button variant="outline" size="sm" onClick={() => { setBoundaries([null]); read.refresh(); }}><RefreshCw size={14} />{t("knowledge.refresh")}</Button>
    </div>
    {read.loading ? <StreamPlaceholder label={t("knowledge.history.loading")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
    {read.value !== null ? <>
      {rows.length === 0 ? <PageEmptyState icon={<History />} title={t("knowledge.history.empty")} /> : null}
      <ol className="knowledge-timeline">
        {rows.map((change) => <li key={change.seq} className="knowledge-timeline-item">
          <KnowledgeActorMarker kind={change.actor.kind} label={t(knowledgeActorLabel(change.actor.kind))} />
          <article className="knowledge-timeline-card">
            <div className="knowledge-timeline-head">
              <b>#{change.seq}</b>
              <span className="knowledge-badge" data-tone={Object.hasOwn(OPERATION_TONES, change.operation) ? OPERATION_TONES[change.operation] : "neutral"} title={change.operation}>{knowledgeChangeLabel(change.operation, t)}</span>
              <small>{t(knowledgeActorLabel(change.actor.kind))}</small>
              <time dateTime={new Date(change.atMs).toISOString()}>{formatDateTime(change.atMs, locale)}</time>
            </div>
            <p className="knowledge-timeline-note">{change.note}</p>
            <div className="knowledge-timeline-foot">
              <span>{fillKnowledgeText(t("knowledge.history.itemCount"), { count: change.itemCount })}{change.restoreTargetSeq !== null ? " · " + (change.restoreTargetSeq === 0 ? t("knowledge.history.restoredToEmpty") : fillKnowledgeText(t("knowledge.history.restoredTo"), { seq: change.restoreTargetSeq })) : ""}</span>
              <div className="knowledge-actions">
                <Button size="sm" variant="ghost" onClick={() => setDetail(change)}><GitCompare size={14} />{t("knowledge.history.compare")}</Button>
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => { setError(null); setRestoreTarget(change.seq); }}><RotateCcw size={14} />{t("knowledge.history.restore")}</Button>
              </div>
            </div>
          </article>
        </li>)}
        {lastPage && rows.length > 0 ? <li className="knowledge-timeline-item">
          <span className="knowledge-timeline-origin"><Circle size={10} aria-hidden="true" /></span>
          <div className="knowledge-timeline-foot"><span>#0</span><Button size="sm" variant="ghost" disabled={busy} onClick={() => { setError(null); setRestoreTarget(0); }}><RotateCcw size={14} />{t("knowledge.history.restoreEmpty")}</Button></div>
        </li> : null}
      </ol>
      {boundaries.length > 1 || !lastPage ? <div className="knowledge-pager"><Button variant="outline" size="sm" disabled={boundaries.length === 1} onClick={() => setBoundaries((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button>
        <Button variant="outline" size="sm" disabled={lastPage} onClick={() => {
          // The last displayed summary, not an assumed sequence offset, anchors continuation.
          // 最后展示摘要锚定续读，而非假设的序号偏移。
          const last = rows.at(-1);
          if (last !== undefined) setBoundaries((current) => [...current, last.seq]);
        }}>{t("knowledge.page.next")}</Button></div> : null}
    </> : null}
    {detail !== null ? <KnowledgeHistoryDetail key={detail.seq} libraryId={library.id} change={detail} onClose={() => setDetail(null)} /> : null}
    <AlertDialog open={restoreTarget !== null} onOpenChange={(open) => { if (!open && !busy && !restoring.current) setRestoreTarget(null); }}><AlertDialogContent>
      <AlertDialogHeader><AlertDialogTitle>{t("knowledge.history.restore")}</AlertDialogTitle><AlertDialogDescription>{fillKnowledgeText(t("knowledge.history.restoreConfirm"), { name: library.name, seq: String(restoreTarget) })}</AlertDialogDescription></AlertDialogHeader>
      {error !== null ? <pre role="alert" className="knowledge-error">{error}</pre> : null}
      <AlertDialogFooter><AlertDialogCancel disabled={busy}>{t("conversation.action.cancel")}</AlertDialogCancel><AlertDialogAction disabled={busy} onClick={(event) => { event.preventDefault(); void confirmRestore(); }}>{t("knowledge.history.restore")}</AlertDialogAction></AlertDialogFooter>
    </AlertDialogContent></AlertDialog>
  </>;
}
