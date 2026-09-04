import * as React from "react"
import { CircleAlert, FileText, Plus, Search, Trash2 } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Textarea } from "@/components/ui/textarea"
import { useI18n } from "@/i18n"
import type {
  VmmGetTurnDetailsActionValue,
  VmmMemoryActionResult,
  VmmMemorySnapshot,
  VmmSearchMemoryEventsActionValue,
} from "@/types"

import { VmmNativeMemoryRecords } from "./vmm-native-memory-records"

// MemorySearchHitView is the exact product projection used by one search card.
// MemorySearchHitView 是单个检索卡片使用的准确产品投影。
interface MemorySearchHitView {
  memoryId: number
  sourceTurnId: number
  summary: string
  preview: string
  category: string
  createdAt: string
}

// TurnDetailView is the exact human-readable turn detail projection.
// TurnDetailView 是准确的人类可读回合详情投影。
interface TurnDetailView {
  question: string
  answer: string
  detail: string
}

// VmmContentPageProps defines content-management state and service actions.
// VmmContentPageProps 定义内容管理状态与服务操作。
interface VmmContentPageProps {
  busy: boolean
  snapshot: VmmMemorySnapshot
  result: VmmMemoryActionResult | null
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// VmmContentPage renders natural-language memory search and guarded maintenance.
// VmmContentPage 渲染自然语言记忆搜索与受保护维护操作。
function VmmContentPage({
  busy,
  snapshot,
  result,
  onRunAction,
}: VmmContentPageProps) {
  const { t } = useI18n()
  const [query, setQuery] = React.useState("")
  const [searchHits, setSearchHits] = React.useState<MemorySearchHitView[]>([])
  const [searchPerformed, setSearchPerformed] = React.useState(false)
  const [selectedHit, setSelectedHit] = React.useState<MemorySearchHitView | null>(null)
  const [turnDetail, setTurnDetail] = React.useState<TurnDetailView | null>(null)
  const [deleteHit, setDeleteHit] = React.useState<MemorySearchHitView | null>(null)
  const [deleteReason, setDeleteReason] = React.useState("")
  const [writeOpen, setWriteOpen] = React.useState(false)
  const [writeSummary, setWriteSummary] = React.useState("")
  const [writeDetails, setWriteDetails] = React.useState("")
  const canWriteMemory = Boolean(snapshot.selectedConversationId?.trim())

  React.useEffect(() => {
    if (result?.data.kind === "search_memory_events") {
      setSearchHits(projectSearchHits(result.data.value))
      setSearchPerformed(true)
    }
    if (result?.data.kind === "get_turn_details") {
      setTurnDetail(projectTurnDetail(result.data.value))
    }
  }, [result])

  // submitSearch runs one natural-language query with a stable product limit.
  // submitSearch 使用稳定的产品结果上限执行一次自然语言查询。
  const submitSearch = React.useCallback(async () => {
    const normalizedQuery = query.trim()
    if (!normalizedQuery) {
      return
    }
    await onRunAction("search_memory_events", {
      queries: [normalizedQuery],
      topK: 20,
    })
  }, [onRunAction, query])

  // openDetails resolves the hidden source turn and opens the user-facing detail sheet.
  // openDetails 解析隐藏的来源回合并打开用户可见详情抽屉。
  const openDetails = React.useCallback(async (hit: MemorySearchHitView) => {
    setSelectedHit(hit)
    setTurnDetail(null)
    await onRunAction("get_turn_details", { turnIds: [hit.sourceTurnId] })
  }, [onRunAction])

  // confirmDelete removes one selected memory using its hidden stable key.
  // confirmDelete 使用隐藏的稳定键删除一条选定记忆。
  const confirmDelete = React.useCallback(async () => {
    if (!deleteHit) {
      return
    }
    const deleted = await onRunAction("delete_memories", {
      memoryIds: [deleteHit.memoryId],
      reason: deleteReason.trim() || "manual deletion from the Vulcan Code memory page",
    })
    if (!deleted) {
      return
    }
    setSearchHits((hits) => hits.filter((hit) => hit.memoryId !== deleteHit.memoryId))
    setDeleteHit(null)
    setDeleteReason("")
  }, [deleteHit, deleteReason, onRunAction])

  // submitWrite stores one natural-language memory with product-owned safe defaults.
  // submitWrite 使用产品持有的安全默认值保存一条自然语言记忆。
  const submitWrite = React.useCallback(async () => {
    const sessionId = snapshot.selectedConversationId?.trim()
    if (!sessionId || !writeSummary.trim() || !writeDetails.trim()) {
      return
    }
    const written = await onRunAction("write_memories", {
      sessionId,
      abstractText: writeSummary.trim(),
      details: writeDetails.trim(),
      scopeLevel: "2",
      category: "1",
      priority: "2",
      memoryLevel: "2",
    })
    if (!written) {
      return
    }
    setWriteOpen(false)
    setWriteSummary("")
    setWriteDetails("")
  }, [onRunAction, snapshot.selectedConversationId, writeDetails, writeSummary])

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.content.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t(snapshot.experienceState.deployment === "native"
              ? "memory.content.nativeDescription"
              : "memory.content.description")}
          </p>
        </div>
        <Button
          disabled={busy || !canWriteMemory}
          onClick={() => setWriteOpen(true)}
          title={!canWriteMemory ? t("memory.content.sessionRequired") : undefined}
          type="button"
          variant="outline"
        >
          <Plus className="size-4" aria-hidden="true" />
          {t("memory.content.add")}
        </Button>
      </div>

      {!canWriteMemory ? (
        <Alert>
          <CircleAlert aria-hidden="true" />
          <AlertTitle>{t("memory.content.sessionRequiredTitle")}</AlertTitle>
          <AlertDescription>{t("memory.content.sessionRequired")}</AlertDescription>
        </Alert>
      ) : null}

      {snapshot.experienceState.deployment === "native" ? (
        <VmmNativeMemoryRecords
          busy={busy}
          onRunAction={onRunAction}
          snapshot={snapshot}
        />
      ) : null}

      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          void submitSearch()
        }}
      >
        <Input
          aria-label={t("memory.content.search")}
          disabled={busy}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("memory.content.searchPlaceholder")}
          value={query}
        />
        <Button disabled={busy || !query.trim()} type="submit">
          <Search className="size-4" aria-hidden="true" />
          {t("memory.content.search")}
        </Button>
      </form>

      {searchHits.length === 0 ? (
        <Empty className="min-h-72" variant="outlined">
          <EmptyHeader>
            <EmptyMedia variant="icon"><Search className="size-5" /></EmptyMedia>
            <EmptyTitle>
              {searchPerformed ? t("memory.content.noResults") : t("memory.content.title")}
            </EmptyTitle>
            <EmptyDescription>
              {searchPerformed ? t("memory.content.noResults") : t("memory.content.empty")}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {searchHits.map((hit) => (
            <Card key={hit.memoryId}>
              <CardHeader className="gap-2">
                <div className="flex items-start justify-between gap-3">
                  <CardTitle className="line-clamp-2 text-base">{hit.summary}</CardTitle>
                  {hit.category ? <Badge variant="secondary">{hit.category}</Badge> : null}
                </div>
                {hit.createdAt ? <CardDescription>{hit.createdAt}</CardDescription> : null}
              </CardHeader>
              <CardContent className="space-y-3">
                <p className="line-clamp-4 text-sm text-muted-foreground">{hit.preview}</p>
                <div className="flex justify-end gap-2">
                  <Button onClick={() => void openDetails(hit)} size="sm" type="button" variant="outline">
                    <FileText className="size-4" />
                    {t("memory.content.details")}
                  </Button>
                  <Button onClick={() => setDeleteHit(hit)} size="icon-sm" type="button" variant="ghost">
                    <Trash2 className="size-4 text-destructive" />
                    <span className="sr-only">{t("memory.identity.delete")}</span>
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Sheet
        open={selectedHit !== null}
        onOpenChange={(open) => {
          if (!open) {
            setSelectedHit(null)
            setTurnDetail(null)
          }
        }}
      >
        <SheetContent className="overflow-y-auto" size="md">
          <SheetHeader>
            <SheetTitle>{selectedHit?.summary}</SheetTitle>
            <SheetDescription>{selectedHit?.createdAt}</SheetDescription>
          </SheetHeader>
          <div className="space-y-4 px-4 pb-4">
            {busy && !turnDetail ? (
              <p className="text-sm text-muted-foreground">{t("memory.common.loading")}</p>
            ) : null}
            {turnDetail?.question ? (
              <DetailBlock label={t("memory.content.sourceConversation")} value={turnDetail.question} />
            ) : null}
            {turnDetail?.answer ? (
              <DetailBlock label={t("memory.content.details")} value={turnDetail.answer} />
            ) : null}
            <DetailBlock
              label={t("memory.content.fullDetails")}
              value={turnDetail?.detail || selectedHit?.preview || ""}
            />
          </div>
        </SheetContent>
      </Sheet>

      <AlertDialog
        open={deleteHit !== null}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteHit(null)
            setDeleteReason("")
          }
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("memory.content.deleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("memory.content.deleteDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <div className="space-y-2">
            <Label htmlFor="vmm-memory-delete-reason">{t("memory.content.deleteReason")}</Label>
            <Input
              disabled={busy}
              id="vmm-memory-delete-reason"
              onChange={(event) => setDeleteReason(event.target.value)}
              value={deleteReason}
            />
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={(event) => {
                event.preventDefault()
                void confirmDelete()
              }}
              variant="destructive"
            >
              {busy ? t("memory.common.loading") : t("memory.identity.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog
        open={writeOpen}
        onOpenChange={(open) => {
          setWriteOpen(open)
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("memory.content.addTitle")}</DialogTitle>
            <DialogDescription>
              {t(snapshot.experienceState.deployment === "native"
                ? "memory.content.nativeDescription"
                : "memory.content.description")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="vmm-write-summary">{t("memory.content.summary")}</Label>
              <Input
                disabled={busy}
                id="vmm-write-summary"
                onChange={(event) => setWriteSummary(event.target.value)}
                value={writeSummary}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="vmm-write-details">{t("memory.content.fullDetails")}</Label>
              <Textarea
                className="min-h-40"
                disabled={busy}
                id="vmm-write-details"
                onChange={(event) => setWriteDetails(event.target.value)}
                value={writeDetails}
              />
            </div>
          </div>
          <DialogFooter>
            <Button onClick={() => setWriteOpen(false)} type="button" variant="outline">
              {t("memory.common.cancel")}
            </Button>
            <Button
              disabled={busy || !canWriteMemory || !writeSummary.trim() || !writeDetails.trim()}
              onClick={() => void submitWrite()}
              type="button"
            >
              {busy ? t("memory.common.loading") : t("memory.content.write")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

// DetailBlock renders one labeled, selectable detail section.
// DetailBlock 渲染一个带标签且可选择的详情区块。
function DetailBlock({ label, value }: { label: string; value: string }) {
  return (
    <section className="space-y-1.5">
      <h3 className="text-sm font-medium">{label}</h3>
      <p className="whitespace-pre-wrap rounded-lg border bg-muted/30 p-3 text-sm">{value}</p>
    </section>
  )
}

// projectSearchHits converts the typed memory-search contract into card data.
// projectSearchHits 将强类型记忆检索契约转换为卡片数据。
function projectSearchHits(response: VmmSearchMemoryEventsActionValue): MemorySearchHitView[] {
  return response.results.flatMap((group) => {
    return group.hits.map((hit) => ({
      memoryId: hit.memory_id,
      sourceTurnId: hit.source_turn_id,
      summary: hit.abstract_text,
      preview: hit.details_preview,
      category: hit.category,
      createdAt: hit.created_datetime,
    }))
  })
}

// projectTurnDetail reads the first typed source-turn detail.
// projectTurnDetail 读取第一条强类型来源回合详情。
function projectTurnDetail(response: VmmGetTurnDetailsActionValue): TurnDetailView | null {
  const firstTurn = response.turns[0]
  if (!firstTurn) {
    return null
  }
  return {
    question: firstTurn.user_question,
    answer: firstTurn.assistant_answer,
    detail: firstTurn.detail,
  }
}

export { VmmContentPage }
export type { VmmContentPageProps }
