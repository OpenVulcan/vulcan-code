import * as React from "react"
import { ListChecks, RefreshCw, Search } from "lucide-react"

import { ProviderModelModalitySummary } from "@/components/features/settings/provider-model-modality-summary"
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import {
  savePublicAiConnectionModelEnabled,
  savePublicAiConnectionModelsEnabled,
  syncPublicAiConnection,
} from "@/service/settings-actions-service"
import type {
  PublicAiConnection,
  PublicAiModel,
  PublicAiModelDirectoryKind,
  PublicAiProviderDescriptor,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"

// CredentialModelFilter selects the human-facing subset displayed in the dialog.
// CredentialModelFilter 选择弹窗中面向用户展示的模型子集。
type CredentialModelFilter = "all" | "enabled" | "disabled"

// CredentialModelAccessDialogProps binds one account to its provider context and authoritative snapshot.
// CredentialModelAccessDialogProps 将一个账号绑定到其供应商上下文与权威快照。
interface CredentialModelAccessDialogProps {
  // connection is the exact account whose routing access is edited.
  // connection 是要编辑路由访问状态的精确账号。
  connection: PublicAiConnection | null
  // entry is the exact service entry selected by the account.
  // entry 是账号选择的精确服务入口。
  entry?: PublicAiServiceEntry
  // onOpenChange controls dialog visibility.
  // onOpenChange 控制弹窗可见性。
  onOpenChange: (open: boolean) => void
  // onSnapshotChange publishes a successful backend mutation to the owner page.
  // onSnapshotChange 把后端成功变更发布给所属页面。
  onSnapshotChange: (snapshot: PublicAiSettingsSnapshot) => void
  // open controls dialog visibility.
  // open 控制弹窗可见性。
  open: boolean
  // provider is the registered owner of the account.
  // provider 是账号所属的已注册供应商。
  provider?: PublicAiProviderDescriptor
  // snapshot contains support and effective catalogs from one backend revision.
  // snapshot 包含同一后端修订的支持目录与有效目录。
  snapshot: PublicAiSettingsSnapshot | null
}

// CredentialModelAccessDialog renders an ergonomic account-scoped model task instead of raw policy fields.
// CredentialModelAccessDialog 渲染符合人工操作习惯的账号模型任务，而不是暴露原始策略字段。
export function CredentialModelAccessDialog(props: CredentialModelAccessDialogProps) {
  const { t } = useI18n()
  const [query, setQuery] = React.useState("")
  const [filter, setFilter] = React.useState<CredentialModelFilter>("all")
  const [busyModelId, setBusyModelId] = React.useState<string | null>(null)
  const [bulkBusy, setBulkBusy] = React.useState(false)
  const [syncBusy, setSyncBusy] = React.useState(false)
  const [disableConfirmationOpen, setDisableConfirmationOpen] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [optimisticEnabled, setOptimisticEnabled] = React.useState<Record<string, boolean>>({})

  React.useEffect(() => {
    if (!props.open) {
      setQuery("")
      setFilter("all")
      setBusyModelId(null)
      setBulkBusy(false)
      setSyncBusy(false)
      setDisableConfirmationOpen(false)
      setError(null)
      setOptimisticEnabled({})
    }
  }, [props.open])

  const connection = props.connection
  const policy = connection
    ? props.snapshot?.connectionModelAccessPolicyRecords.find(
      (record) => record.connectionId === connection.id,
    )
    : undefined
  // supportModels remains independent from conversation publication so an account can serve internal-only models.
  // supportModels 独立于会话发布状态，使账号可以服务仅供内部使用的模型。
  const supportModels = connection
    ? props.snapshot?.modelSupportCatalogs[connection.id]?.models ?? []
    : []
  const effectiveModelIds = React.useMemo(
    () => new Set(
      connection
        ? props.snapshot?.catalogs[connection.id]?.models.map((model) => model.id) ?? []
        : [],
    ),
    [connection, props.snapshot],
  )

  // modelIsEnabled resolves immediate optimistic feedback before the authoritative snapshot arrives.
  // modelIsEnabled 在权威快照返回前解析即时乐观反馈。
  const modelIsEnabled = React.useCallback((modelId: string) => (
    optimisticEnabled[modelId] ?? effectiveModelIds.has(modelId)
  ), [effectiveModelIds, optimisticEnabled])

  const normalizedQuery = query.trim().toLocaleLowerCase()
  const visibleModels = supportModels.filter((model) => {
    const enabled = modelIsEnabled(model.id)
    if (filter === "enabled" && !enabled) {
      return false
    }
    if (filter === "disabled" && enabled) {
      return false
    }
    return normalizedQuery.length === 0
      || model.id.toLocaleLowerCase().includes(normalizedQuery)
      || model.display_name.toLocaleLowerCase().includes(normalizedQuery)
  })
  const enabledCount = supportModels.filter((model) => modelIsEnabled(model.id)).length
  const directoryKind = policy?.directoryKind ?? "unavailable"
  const directoryLabel = directoryKindLabel(directoryKind, t)

  // saveModel applies one switch immediately, then reconciles it with the backend snapshot.
  // saveModel 立即应用一个开关，随后与后端快照协调。
  const saveModel = React.useCallback(async (model: PublicAiModel, enabled: boolean) => {
    if (!connection || bulkBusy || busyModelId) {
      return
    }
    setBusyModelId(model.id)
    setError(null)
    setOptimisticEnabled((current) => ({ ...current, [model.id]: enabled }))
    try {
      const next = await savePublicAiConnectionModelEnabled({
        connectionId: connection.id,
        modelId: model.id,
        enabled,
      })
      props.onSnapshotChange(next)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setOptimisticEnabled((current) => {
        const next = { ...current }
        delete next[model.id]
        return next
      })
      setBusyModelId(null)
    }
  }, [bulkBusy, busyModelId, connection, props])

  // saveAll replaces one inherited account policy in one backend operation.
  // saveAll 通过一次后端操作替换一个账号的继承策略。
  const saveAll = React.useCallback(async (enabled: boolean) => {
    if (!connection || bulkBusy || busyModelId) {
      return
    }
    setBulkBusy(true)
    setError(null)
    setOptimisticEnabled(Object.fromEntries(supportModels.map((model) => [model.id, enabled])))
    try {
      const next = await savePublicAiConnectionModelsEnabled({
        connectionId: connection.id,
        enabled,
      })
      props.onSnapshotChange(next)
      setDisableConfirmationOpen(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setOptimisticEnabled({})
      setBulkBusy(false)
    }
  }, [bulkBusy, busyModelId, connection, props, supportModels])

  // synchronize refreshes only dynamic credential directories and retains account access choices.
  // synchronize 仅刷新动态凭据目录，并保留账号访问选择。
  const synchronize = React.useCallback(async () => {
    if (!connection || syncBusy || directoryKind !== "dynamic") {
      return
    }
    setSyncBusy(true)
    setError(null)
    try {
      props.onSnapshotChange(await syncPublicAiConnection(connection.id))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSyncBusy(false)
    }
  }, [connection, directoryKind, props, syncBusy])

  return (
    <>
      <Dialog onOpenChange={props.onOpenChange} open={props.open}>
        <DialogContent className="flex max-h-[min(46rem,calc(100vh-3rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
          <DialogHeader className="border-b px-6 py-5 text-left">
            <div className="flex min-w-0 items-start gap-3 pr-8">
              <div className="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted">
                <ListChecks className="size-4" aria-hidden="true" />
              </div>
              <div className="min-w-0 flex-1">
                <DialogTitle className="truncate">
                  {t("settings.credentialModels.title")} · {connection?.display_name ?? ""}
                </DialogTitle>
                <DialogDescription className="mt-1">
                  {t("settings.credentialModels.description")}
                </DialogDescription>
                <div className="mt-2 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                  <Badge variant="outline">{props.provider?.display_name ?? connection?.provider_id}</Badge>
                  {props.entry ? <Badge variant="outline">{props.entry.display_name}</Badge> : null}
                  <Badge variant="secondary">{directoryLabel}</Badge>
                </div>
              </div>
            </div>
          </DialogHeader>

          <div className="sticky top-0 z-10 grid gap-3 border-b bg-background/95 px-6 py-4 backdrop-blur supports-[backdrop-filter]:bg-background/85">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="text-sm font-medium">
                {t("settings.credentialModels.count")
                  .replace("{enabled}", String(enabledCount))
                  .replace("{total}", String(supportModels.length))}
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {directoryKind === "dynamic" ? (
                  <Button
                    disabled={syncBusy || bulkBusy || busyModelId !== null}
                    onClick={() => void synchronize()}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <RefreshCw className={`size-3.5 ${syncBusy ? "animate-spin" : ""}`} />
                    {t(syncBusy ? "settings.credentialModels.syncing" : "settings.credentialModels.sync")}
                  </Button>
                ) : null}
                <Button
                  disabled={bulkBusy || busyModelId !== null || supportModels.length === 0}
                  onClick={() => void saveAll(true)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("settings.credentialModels.enableAll")}
                </Button>
                <Button
                  disabled={bulkBusy || busyModelId !== null || supportModels.length === 0}
                  onClick={() => setDisableConfirmationOpen(true)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("settings.credentialModels.disableAll")}
                </Button>
              </div>
            </div>
            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_10rem]">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  autoFocus
                  className="pl-9"
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("settings.credentialModels.search")}
                  value={query}
                />
              </div>
              <Select onValueChange={(value) => setFilter(value as CredentialModelFilter)} value={filter}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t("settings.credentialModels.filterAll")}</SelectItem>
                  <SelectItem value="enabled">{t("settings.credentialModels.filterEnabled")}</SelectItem>
                  <SelectItem value="disabled">{t("settings.credentialModels.filterDisabled")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <p className="text-xs text-muted-foreground">
              {t(policy?.defaultEnabled === false
                ? "settings.credentialModels.futureDisabled"
                : "settings.credentialModels.futureEnabled")}
            </p>
            {error ? <p className="text-sm text-destructive" role="alert">{error}</p> : null}
          </div>

          <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-6 py-4">
            {supportModels.length === 0 ? (
              <EmptyState message={t("settings.credentialModels.empty")} />
            ) : visibleModels.length === 0 ? (
              <EmptyState message={t("settings.credentialModels.noResults")} />
            ) : (
              <div className="grid gap-2">
                {visibleModels.map((model) => {
                  const enabled = modelIsEnabled(model.id)
                  return (
                    <div
                      className="flex min-h-16 items-center gap-3 rounded-lg border bg-card px-3 py-2.5"
                      key={model.id}
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                          <strong className="truncate text-sm">{model.display_name}</strong>
                          <span className="max-w-full truncate text-xs text-muted-foreground">({model.id})</span>
                        </div>
                        <div className="mt-1.5">
                          <ProviderModelModalitySummary model={model} />
                        </div>
                      </div>
                      <Switch
                        aria-label={`${model.display_name}: ${enabled
                          ? t("settings.credentialModels.filterEnabled")
                          : t("settings.credentialModels.filterDisabled")}`}
                        checked={enabled}
                        disabled={bulkBusy || busyModelId !== null}
                        onCheckedChange={(checked) => void saveModel(model, checked)}
                      />
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </DialogContent>
      </Dialog>

      <AlertDialog onOpenChange={setDisableConfirmationOpen} open={disableConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.credentialModels.disableConfirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.credentialModels.disableConfirmDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings.credentialModels.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void saveAll(false)}>
              {t("settings.credentialModels.confirmDisable")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

// EmptyStateProps carries one human-readable empty-list explanation.
// EmptyStateProps 承载一条人类可读的空列表说明。
interface EmptyStateProps {
  // message explains the next actionable state without exposing internal identifiers.
  // message 说明下一步可操作状态且不暴露内部标识。
  message: string
}

// EmptyState renders a calm, centered empty-list explanation.
// EmptyState 渲染平静且居中的空列表说明。
function EmptyState(props: EmptyStateProps) {
  return (
    <div className="flex min-h-44 items-center justify-center rounded-lg border border-dashed px-6 text-center text-sm text-muted-foreground">
      {props.message}
    </div>
  )
}

// directoryKindLabel maps backend-owned directory semantics to localized human language.
// directoryKindLabel 将后端持有的目录语义映射为本地化人类语言。
function directoryKindLabel(
  kind: PublicAiModelDirectoryKind,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (kind) {
    case "fixed":
      return t("settings.credentialModels.directoryFixed")
    case "dynamic":
      return t("settings.credentialModels.directoryDynamic")
    case "manual":
      return t("settings.credentialModels.directoryManual")
    case "unavailable":
      return t("settings.credentialModels.directoryUnavailable")
  }
}

// errorMessage converts one rejected frontend operation into readable inline feedback.
// errorMessage 把一个被拒绝的前端操作转换为可读的行内反馈。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
