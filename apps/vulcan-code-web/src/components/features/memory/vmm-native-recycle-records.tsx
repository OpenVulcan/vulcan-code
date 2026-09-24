import * as React from "react"
import { Button } from "@/components/ui/button"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRecycleBatchSummary, VmmNativeRecycleRecord, VmmNativeRecycleRecordKind, VmmNativeRecycleRecordPage } from "@/types"

// Defines the exact inspected batch and existing read-only action transport.
// 定义准确查看批次及既有只读动作传输。
interface RecycleRecordsProps {
  // Stable batch identity and parent-provided occupancy.
  // 稳定批次身份及父级提供的占用状态。
  batch: VmmNativeRecycleBatchSummary
  busy: boolean
  // Executes the typed native action; no restore or purge action is issued here.
  // 执行类型化原生动作，此处不发出恢复或永久清除动作。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
}

// Maps the closed record-family contract to localized user-facing tabs.
// 将封闭记录类别契约映射为本地化用户分页标签。
const kinds: Record<VmmNativeRecycleRecordKind, TranslationKey> = {
  turn: "memory.maintenance.turns", memory: "memory.maintenance.memories",
  profile: "memory.maintenance.profiles", evidence: "memory.maintenance.evidence",
}

// Loads retained records on demand with stable-version pagination and stale-response suppression.
// 按需加载保留记录，采用稳定版本分页并抑制过时响应。
export function VmmNativeRecycleRecords({ batch, busy, onRunAction }: RecycleRecordsProps) {
  const { t } = useI18n()
  // Local visibility and current family never mutate the batch or active session.
  // 本地可见性及当前类别不修改批次或活动会话。
  const [open, setOpen] = React.useState(false)
  const [kind, setKind] = React.useState<VmmNativeRecycleRecordKind>("turn")
  const [page, setPage] = React.useState<VmmNativeRecycleRecordPage | null>(null)
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState(false)
  // Every request owns a generation; closing or unmounting invalidates late results.
  // 每个请求拥有代次，关闭或卸载使迟到结果失效。
  const generation = React.useRef(0)
  React.useEffect(() => {
    // Moving a batch preserves its ID but changes its owner; close the old view before another read.
    // 移动批次保留 ID 但改变归属，再次读取前关闭旧视图。
    generation.current += 1
    setOpen(false); setPage(null); setPending(false); setError(false)
    return () => { generation.current += 1 }
  }, [batch.projectId, batch.batchId])

  // Reads one exact page; a fresh reload explicitly discards the prior observed version.
  // 读取准确一页，主动刷新明确放弃此前观察版本。
  const load = async (recordKind: VmmNativeRecycleRecordKind, offset: number, expectedStorageVersion: number | null) => {
    const requestGeneration = ++generation.current
    setPending(true); setError(false); setPage(null); setKind(recordKind)
    try {
      const result = await onRunAction("native_recycle_records", { vmmProjectId: batch.projectId, batchId: batch.batchId, recordKind, offset, limit: 20, expectedStorageVersion })
      if (requestGeneration !== generation.current) return
      if (result?.data?.kind === "native_recycle_records") setPage(result.data.value)
      else setError(true)
    } catch { if (requestGeneration === generation.current) setError(true) }
    finally { if (requestGeneration === generation.current) setPending(false) }
  }

  return <Dialog open={open} onOpenChange={(visible) => { setOpen(visible); if (!visible) { generation.current += 1; setPage(null); setPending(false) } }}>
    <DialogTrigger asChild><Button type="button" size="sm" variant="outline" disabled={busy || pending} onClick={() => void load("turn", 0, null)}>{t("memory.recycleRecords.inspect")}</Button></DialogTrigger>
      <DialogContent className="flex min-h-0 flex-col" size="xl" scrollMode="fixed-footer">
        <DialogHeader className="shrink-0"><DialogTitle>{t("memory.recycleRecords.inspect")}</DialogTitle><DialogDescription>{t("memory.recycleRecords.description")}</DialogDescription></DialogHeader>
        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto">
        <p className="break-all font-mono text-xs text-muted-foreground">{batch.batchId}</p>
        <Tabs value={kind} onValueChange={(value) => { if (!busy && !pending) void load(value as VmmNativeRecycleRecordKind, 0, null) }}>
          {/* Keep the current tab focusable while its request runs; inactive tabs cannot start competing reads.
              当前分页请求期间保留焦点，其余分页不可发起竞争读取。 */}
          <TabsList>{(Object.keys(kinds) as VmmNativeRecycleRecordKind[]).map((value) => <TabsTrigger key={value} value={value} disabled={(busy || pending) && value !== kind} aria-disabled={busy || pending}>{t(kinds[value])}</TabsTrigger>)}</TabsList>
        </Tabs>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" variant="outline" disabled={busy || pending} onClick={() => void load(kind, 0, null)}>{t("memory.recycleRecords.reload")}</Button>
          {page ? <span className="text-sm text-muted-foreground">{t("memory.recycleRecords.total").replace("{count}", String(page.total))}</span> : null}
        </div>
        {pending ? <StreamPlaceholder label={t("memory.resources.loading")} shape="rows" count={4} /> : null}
        {error ? <p role="alert" className="text-sm text-destructive">{t("memory.recycleRecords.failed")}</p> : null}
        {page?.records.length === 0 ? <p className="rounded border border-dashed p-5 text-muted-foreground">{t("memory.recycleRecords.empty")}</p> : null}
        {page?.records.map((record) => <RecycleRecord key={recordKey(record)} record={record} />)}
        </div>
        {page ? <div className="flex shrink-0 flex-wrap gap-2">
          <Button type="button" variant="outline" disabled={busy || pending || page.query.offset === 0} onClick={() => void load(kind, Math.max(0, page.query.offset - page.query.limit), page.storageVersion)}>{t("memory.recycleRecords.previous")}</Button>
          <Button type="button" variant="outline" disabled={busy || pending || page.nextOffset === null} onClick={() => { if (page.nextOffset !== null) void load(kind, page.nextOffset, page.storageVersion) }}>{t("memory.recycleRecords.next")}</Button>
        </div> : null}
      </DialogContent>
  </Dialog>
}

// Returns the exact retained identity, using the full evidence tuple where no separate ID exists.
// 返回准确保留身份；没有独立 ID 的证据使用完整身份元组。
function recordKey(record: VmmNativeRecycleRecord): string {
  switch (record.kind) {
    case "turn": return record.turnId
    case "memory": return record.memoryId
    case "profile": return record.profileNodeId
    case "evidence": return JSON.stringify([record.memoryId, record.contextKey, record.contextValue])
  }
}

// Renders retained text as read-only business content rather than exposing raw storage JSON.
// 将保留文本渲染为只读业务内容，不暴露原始存储 JSON。
function RecycleRecord({ record }: { record: VmmNativeRecycleRecord }) {
  const { t } = useI18n()
  return <article className="min-w-0 space-y-2 rounded-lg border p-4">
    <p className="break-all font-mono text-xs text-muted-foreground">{record.kind === "evidence" ? record.memoryId : recordKey(record)}</p>
    {(record.kind === "turn" || record.kind === "memory") && record.sessionSourceId ? <p className="break-all text-xs">{t("memory.recycleRecords.session")}: {record.sessionSourceId}</p> : null}
    {record.kind === "turn" ? <>
      <p className="text-xs text-muted-foreground">{t("memory.recycleRecords.savedStatus")}: {record.status}</p>
      {record.timeline.map((item, index) => <div key={index} className="rounded bg-muted/40 p-3"><p className="mb-1 text-xs text-muted-foreground">{item.role}</p><p className="whitespace-pre-wrap break-words text-sm">{item.content}</p></div>)}
      {record.analysisDetails ? <div><p className="text-xs text-muted-foreground">{t("memory.nativeRecords.analysisDetails")}</p><p className="whitespace-pre-wrap break-words text-sm">{record.analysisDetails}</p></div> : null}
    </> : null}
    {record.kind === "memory" ? <>
      <p className="whitespace-pre-wrap break-words font-medium">{record.summary}</p><p className="whitespace-pre-wrap break-words text-sm">{record.content}</p>
      <p className="break-all text-xs text-muted-foreground">{record.level} · {record.state} · {record.sourceKind}</p>
      {record.sourceTurnId ? <p className="break-all text-xs">{t("memory.recycleRecords.sourceTurns")}: {record.sourceTurnId}</p> : null}
    </> : null}
    {record.kind === "profile" ? <><p className="text-xs">{t(record.target === "user" ? "memory.recycleRecords.userProfile" : "memory.recycleRecords.projectProfile")}</p><p className="whitespace-pre-wrap break-words text-sm">{record.content}</p><p className="break-all text-xs">{t("memory.recycleRecords.sourceTurns")}: {record.sourceTurnIds.join(", ")}</p></> : null}
    {record.kind === "evidence" ? <><p className="whitespace-pre-wrap break-words text-sm">{record.contextKey}: {record.contextValue}</p><p className="text-xs">{t("memory.recycleRecords.support")}: {record.supportCount} · {t("memory.recycleRecords.rebuttal")}: {record.rebuttalCount}</p></> : null}
  </article>
}
