import * as React from "react"
import { Search } from "lucide-react"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger } from "@/ui-kit/primitives/alert-dialog"
import { Button } from "@/ui-kit/primitives/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { useI18n } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeLexicalIndexStatus } from "@/types"

// The parent sequences status loading through the exclusive memory-action channel.
// 父级通过独占记忆动作通道顺序加载状态。
interface LexicalIndexProps {
  // Actual source/index counts; null means unavailable or still loading.
  // 实际源及索引计数，空值表示不可用或仍在加载。
  status: VmmNativeLexicalIndexStatus | null
  // loading distinguishes an initial read from a completed unavailable result when the owner supplies it.
  // loading 在归属方提供时区分初始读取与已结束的不可用结果。
  loading?: boolean
  // Disables actions while another operation owns the channel.
  // 其他操作拥有通道时禁用动作。
  busy: boolean
  // Sends one native-only management action and returns its typed outcome.
  // 发送一个仅原生的管理动作并返回强类型结果。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  // Reloads status after success; false means the parent reported a read failure.
  // 成功后重读状态，假值表示父级已报告读取失败。
  onRefresh: () => Promise<boolean>
}

// Displays actual counts and a version-bound confirmation; rendering never requests inference or repair.
// 显示实际计数及版本绑定确认，渲染绝不请求推理或修复。
// Returns a controlled card whose callbacks run only after explicit user actions.
// 返回受控卡片，仅在用户明确操作后运行回调。
export function VmmNativeLexicalIndex({ status, loading = true, busy, onRunAction, onRefresh }: LexicalIndexProps) {
  const { t } = useI18n()
  // Retain the displayed version even if the parent receives a newer status later.
  // 即使父级后续收到新状态，也保留显示时的版本。
  const [confirmation, setConfirmation] = React.useState<VmmNativeLexicalIndexStatus | null>(null)
  const [pending, setPending] = React.useState(false)
  const [notice, setNotice] = React.useState<string | null>(null)

  // Rebuild only the confirmed version; rejected requests need a fresh confirmation.
  // 仅重建确认的版本，被拒绝的请求需要重新确认。
  const rebuild = async () => {
    if (!confirmation || pending) return
    setPending(true)
    try {
      const result = await onRunAction("native_lexical_rebuild", { expectedStorageVersion: confirmation.storageVersion, confirmed: true })
      setConfirmation(null)
      if (result?.data.kind !== "native_lexical_rebuild") {
        setNotice(t("memory.maintenance.invalidResponse"))
        return
      }
      if (!await onRefresh()) return
      setNotice(t("memory.lexical.completed").replace("{count}", String(result.data.value.reindexedMemoryCount)))
    } finally { setPending(false) }
  }

  return <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !pending) setConfirmation(null) }}><Card>
    <CardHeader><CardTitle>{t("memory.lexical.title")}</CardTitle><CardDescription>{t("memory.lexical.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-3">
      {status ? <dl className="grid gap-3 text-sm sm:grid-cols-3">
        <div><dt className="text-muted-foreground">{t("memory.maintenance.activeMemories")}</dt><dd>{status.activeMemoryCount}</dd></div>
        <div><dt className="text-muted-foreground">{t("memory.lexical.documents")}</dt><dd>{status.documentCount}</dd></div>
        <div><dt className="text-muted-foreground">{t("memory.lexical.rows")}</dt><dd>{status.indexedRowCount}</dd></div>
      </dl> : <p className="text-sm text-muted-foreground">{t(loading ? "memory.resources.loading" : "memory.maintenance.readFailed")}</p>}
      <p className="text-xs text-muted-foreground">{t("memory.lexical.countsNote")}</p>
      <AlertDialogTrigger asChild><Button disabled={busy || pending || status === null} onClick={() => { setNotice(null); setConfirmation(status) }} variant="outline" type="button"><Search aria-hidden="true" className="size-4" />{t("memory.lexical.rebuild")}</Button></AlertDialogTrigger>
      {notice ? <p role="status" className="text-sm">{notice}</p> : null}
        <AlertDialogContent>
          <AlertDialogHeader><AlertDialogTitle>{t("memory.lexical.confirmTitle")}</AlertDialogTitle><AlertDialogDescription>{t("memory.lexical.confirmDescription").replace("{count}", String(confirmation?.activeMemoryCount ?? 0))}</AlertDialogDescription></AlertDialogHeader>
          <AlertDialogFooter><AlertDialogCancel disabled={busy || pending}>{t("memory.common.cancel")}</AlertDialogCancel><AlertDialogAction disabled={busy || pending} onClick={(event) => { event.preventDefault(); void rebuild() }}>{t("memory.lexical.rebuild")}</AlertDialogAction></AlertDialogFooter>
        </AlertDialogContent>
    </CardContent>
  </Card></AlertDialog>
}
