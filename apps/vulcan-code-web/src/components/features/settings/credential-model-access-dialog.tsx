import * as React from "react"
import { ListChecks, RefreshCw, Search, X } from "lucide-react"

import { ProviderModelModalitySummary } from "@/components/features/settings/provider-model-modality-summary"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button, IconButton } from "@/ui-kit/primitives/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui-kit/primitives/input-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { PageEmptyState } from "@/components/page"
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
import { errorMessage } from "@/lib/display-format"
import "@/ui-kit/styles/features/settings-forms.css"

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
// Parameters: props identifies the account, provider snapshot and mutation callbacks; returns model filtering, access controls and guarded bulk confirmation.
// 参数：props 标识账号、供应商快照及变更回调；返回模型过滤、访问控制及受保护批量确认。
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
  // mutationBusy serializes sync and policy edits against the same account catalog.
  // mutationBusy 对同一账号目录的同步和策略编辑进行串行化。
  const mutationBusy = React.useRef(false)

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
  // blockedModels uses backend route diagnostics to keep unsafe models visible but not enableable.
  // blockedModels 使用后端路由诊断，让不安全模型保持可见但不可启用。
  const blockedModels = React.useMemo(() => new Map(
    (props.snapshot?.modelRouteRecords ?? [])
      .filter((record) => record.connectionId === connection?.id && !record.executable)
      .map((record) => [record.modelId, record.unavailableReason] as const),
  ), [connection?.id, props.snapshot?.modelRouteRecords])
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
  // providerName follows registered built-in localization while preserving unknown labels.
  // providerName 遵循已注册内置供应商本地化，同时保留未知标签。
  const providerName = localizedProviderName(
    connection?.provider_id ?? props.provider?.id ?? "",
    props.provider?.display_name ?? connection?.provider_id ?? "",
    t,
  )

  // saveModel applies one switch immediately, then reconciles it with the backend snapshot.
  // saveModel 立即应用一个开关，随后与后端快照协调。
  const saveModel = React.useCallback(async (model: PublicAiModel, enabled: boolean) => {
    if (!connection || mutationBusy.current) {
      return
    }
    mutationBusy.current = true
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
      mutationBusy.current = false
    }
  }, [bulkBusy, busyModelId, connection, props])

  // saveAll replaces one inherited account policy in one backend operation.
  // saveAll 通过一次后端操作替换一个账号的继承策略。
  const saveAll = React.useCallback(async (enabled: boolean) => {
    if (!connection || mutationBusy.current) {
      return
    }
    mutationBusy.current = true
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
      mutationBusy.current = false
    }
  }, [bulkBusy, busyModelId, connection, props, supportModels])

  // synchronize refreshes only dynamic credential directories and retains account access choices.
  // synchronize 仅刷新动态凭据目录，并保留账号访问选择。
  const synchronize = React.useCallback(async () => {
    if (!connection || !connection.enabled || mutationBusy.current || directoryKind !== "dynamic") {
      return
    }
    mutationBusy.current = true
    setSyncBusy(true)
    setError(null)
    try {
      props.onSnapshotChange(await syncPublicAiConnection(connection.id))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSyncBusy(false)
      mutationBusy.current = false
    }
  }, [connection, directoryKind, props, syncBusy])

  return (
    <>
      <Dialog onOpenChange={(open) => { if (!mutationBusy.current) props.onOpenChange(open) }} open={props.open}>
        <DialogContent className="vk-form-dialog" layout="document" showCloseButton={false} size="xl">
          <header className="vk-viewer-head">
            <span className="vk-viewer-mark"><ListChecks aria-hidden="true" /></span>
            <div className="vk-viewer-titles">
              <DialogTitle>
                {t("settings.credentialModels.title")} · {connection?.display_name ?? ""}
              </DialogTitle>
              <DialogDescription>
                {t("settings.credentialModels.description")}
              </DialogDescription>
            </div>
            <div className="vk-form-chips">
              <Badge variant="outline">{providerName}</Badge>
              {props.entry ? <Badge variant="outline">{props.entry.display_name}</Badge> : null}
              <Badge variant="secondary">{directoryLabel}</Badge>
            </div>
            <div className="vk-actions">
              <IconButton aria-label={t("ui.close")} onClick={() => { if (!mutationBusy.current) props.onOpenChange(false) }} title={t("ui.close")} type="button">
                <X aria-hidden="true" />
              </IconButton>
            </div>
          </header>

          <div className="vk-form-toolbar">
            <div className="vk-form-row">
              <div className="vk-form-strong">
                {t("settings.credentialModels.count")
                  .replace("{enabled}", String(enabledCount))
                  .replace("{total}", String(supportModels.length))}
              </div>
              <div className="vk-form-inline">
                {directoryKind === "dynamic" ? (
                  <Button
                    disabled={!connection?.enabled || syncBusy || bulkBusy || busyModelId !== null}
                    onClick={() => void synchronize()}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <RefreshCw aria-hidden="true" className={syncBusy ? "vk-spin" : undefined} />
                    {t(syncBusy ? "settings.credentialModels.syncing" : "settings.credentialModels.sync")}
                  </Button>
                ) : null}
                <Button
                  disabled={syncBusy || bulkBusy || busyModelId !== null || supportModels.length === 0}
                  onClick={() => void saveAll(true)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("settings.credentialModels.enableAll")}
                </Button>
                <Button
                  disabled={syncBusy || bulkBusy || busyModelId !== null || supportModels.length === 0}
                  onClick={() => setDisableConfirmationOpen(true)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {t("settings.credentialModels.disableAll")}
                </Button>
              </div>
            </div>
            <div className="vk-form-search">
              <InputGroup>
                <InputGroupAddon align="inline-start"><Search aria-hidden="true" /></InputGroupAddon>
                <InputGroupInput
                  autoFocus
                  aria-label={t("settings.credentialModels.search")}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("settings.credentialModels.search")}
                  value={query}
                />
              </InputGroup>
              <Select onValueChange={(value) => setFilter(value as CredentialModelFilter)} value={filter}>
                <SelectTrigger aria-label={t("settings.modelExperience.usageFilter")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t("settings.credentialModels.filterAll")}</SelectItem>
                  <SelectItem value="enabled">{t("settings.credentialModels.filterEnabled")}</SelectItem>
                  <SelectItem value="disabled">{t("settings.credentialModels.filterDisabled")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {policy ? <p className="vk-form-note">
              {t(policy.inheritModelDefaults
                ? "settings.credentialModels.futureSignedDefaults"
                : policy.defaultEnabled
                  ? "settings.credentialModels.futureEnabled"
                  : "settings.credentialModels.futureDisabled")}
            </p> : null}
            {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
          </div>

          <div className="vk-form-list">
            {supportModels.length === 0 ? (
              <EmptyState message={t("settings.credentialModels.empty")} />
            ) : visibleModels.length === 0 ? (
              <EmptyState message={t("settings.credentialModels.noResults")} />
            ) : (
              <div className="vk-form-stack">
                {visibleModels.map((model) => {
                  const enabled = modelIsEnabled(model.id)
                  const blockedReason = blockedModels.get(model.id)
                  return (
                    <div
                      className="vk-form-item"
                      data-on={enabled ? "" : undefined}
                      key={model.id}
                    >
                      <div className="vk-form-fill vk-form-tight">
                        <div className="vk-form-item-title">
                          <strong>{model.display_name}</strong>
                          <code>{model.id}</code>
                        </div>
                        <ProviderModelModalitySummary model={model} />
                        {blockedReason ? <p className="vk-field-error" role="status">
                          {t("settings.models.routeUnavailableReason")}: {blockedReason}
                        </p> : null}
                      </div>
                      <Switch
                        aria-label={`${model.display_name}: ${enabled
                          ? t("settings.credentialModels.filterEnabled")
                          : t("settings.credentialModels.filterDisabled")}`}
                        checked={enabled}
                        disabled={syncBusy || bulkBusy || busyModelId !== null || (blockedModels.has(model.id) && !enabled)}
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

      <AlertDialog onOpenChange={(open) => { if (!mutationBusy.current) setDisableConfirmationOpen(open) }} open={disableConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.credentialModels.disableConfirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.credentialModels.disableConfirmDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={bulkBusy}>{t("settings.credentialModels.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={bulkBusy} onClick={(event) => { event.preventDefault(); void saveAll(false) }}>
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
  return <PageEmptyState size="panel" title={props.message} />
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
