import * as React from "react"
import { LoaderCircle, Search, SlidersHorizontal } from "lucide-react"

import { PublicAiModelCapabilityDialog } from "@/components/features/settings/public-ai-model-capability-dialog"

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
import { ScrollArea } from "@/components/ui/scroll-area"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import {
  savePublicAiModelEnabled,
  savePublicAiProviderModelsEnabled,
} from "@/service/settings-actions-service"
import type {
  PublicAiModel,
  PublicAiModelCapabilityOverrides,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

// ProviderModelAccessRow is one provider-supported model and its direct publication state.
// ProviderModelAccessRow 表示一个供应商支持模型及其直接发布状态。
export interface ProviderModelAccessRow {
  // Code-owned or user-declared default publication state.
  // 代码持有或用户声明的默认发布状态。
  defaultEnabled: boolean
  // User-visible model name.
  // 用户可见模型名称。
  displayName: string
  // Effective provider-level publication state.
  // 有效供应商级发布状态。
  enabled: boolean
  // Exact upstream model identifier.
  // 精确上游模型标识。
  id: string
  // Complete effective model snapshot used by the shared capability editor.
  // 供共享能力编辑器使用的完整有效模型快照。
  model: PublicAiModel
  // Whether this model owns an explicit capability layer.
  // 此模型是否拥有明确的能力层。
  capabilityCustomized: boolean
  // Complete persisted capability layer when one exists.
  // 存在时的完整持久化能力层。
  capabilityOverrides?: PublicAiModelCapabilityOverrides
  // Whether this model owns an explicit publication layer.
  // 此模型是否拥有明确的发布层。
  customized: boolean
}

// ProviderModelAccessDialogProps binds one provider support catalog to direct save actions.
// ProviderModelAccessDialogProps 将一个供应商支持目录绑定到直接保存操作。
interface ProviderModelAccessDialogProps {
  // Updates dialog visibility.
  // 更新弹窗可见性。
  onOpenChange: (open: boolean) => void
  // Receives each authoritative snapshot after a direct save.
  // 每次直接保存后接收权威快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  // Controls dialog visibility.
  // 控制弹窗可见性。
  open: boolean
  // Exact provider being configured.
  // 正在配置的精确供应商。
  provider: PublicAiProviderDescriptor | null
  // Authoritative provider support and access state.
  // 权威供应商支持与访问状态。
  snapshot: PublicAiSettingsSnapshot | null
}

// buildProviderModelAccessRows keeps disabled provider models visible only inside provider configuration.
// buildProviderModelAccessRows 仅在供应商配置中保留已关闭的供应商模型。
export function buildProviderModelAccessRows(
  snapshot: PublicAiSettingsSnapshot,
  providerId: string,
): ProviderModelAccessRow[] {
  // supportedNames is the credential-independent union of provider support catalogs.
  // supportedNames 是供应商支持目录中与凭据无关的并集。
  const supportedModels = new Map<string, PublicAiModel>()
  for (const connection of snapshot.connections) {
    if (connection.provider_id !== providerId) {
      continue
    }
    for (const model of snapshot.modelSupportCatalogs[connection.id]?.models ?? []) {
      supportedModels.set(model.id, model)
    }
  }
  for (const model of Object.values(snapshot.customModels?.[providerId] ?? {})) {
    supportedModels.set(model.id, {
      id: model.id,
      display_name: model.display_name,
      capabilities: model.capabilities,
    })
  }
  // accessByModel is the backend-projected provider publication contract.
  // accessByModel 是后端投影的供应商发布契约。
  const accessByModel = new Map(
    snapshot.modelAccessRecords
      .filter((record) => record.providerId === providerId)
      .map((record) => [record.modelId, record]),
  )
  const capabilityByModel = new Map(
    (snapshot.modelCapabilityOverrideRecords ?? [])
      .filter((record) => record.providerId === providerId)
      .map((record) => [record.modelId, record]),
  )
  return Array.from(supportedModels, ([id, model]) => {
    const access = accessByModel.get(id)
    const capability = capabilityByModel.get(id)
    return {
      defaultEnabled: access?.defaultEnabled ?? true,
      displayName: model.display_name,
      enabled: access?.enabled ?? true,
      id,
      model,
      capabilityCustomized: capability !== undefined,
      capabilityOverrides: capability?.overrides,
      customized: access?.customized ?? false,
    }
  }).sort((left, right) => (
    left.displayName.localeCompare(right.displayName)
    || left.id.localeCompare(right.id)
  ))
}

// ProviderModelAccessDialog is the direct provider-level publication editor for native providers.
// ProviderModelAccessDialog 是内置供应商的直接供应商级发布编辑器。
export function ProviderModelAccessDialog(props: ProviderModelAccessDialogProps) {
  const { t } = useI18n()
  // query filters only the provider-owned support union.
  // query 只筛选供应商持有的支持并集。
  const [query, setQuery] = React.useState("")
  // busyKey serializes direct writes and identifies the active row or bulk action.
  // busyKey 串行化直接写入，并标识活动行或批量操作。
  const [busyKey, setBusyKey] = React.useState<string | null>(null)
  // error contains one actionable service failure.
  // error 保存一条可操作的服务失败信息。
  const [error, setError] = React.useState<string | null>(null)
  const [editingCapabilityModel, setEditingCapabilityModel] = React.useState<ProviderModelAccessRow | null>(null)
  const [editingProviderDefaults, setEditingProviderDefaults] = React.useState(false)

  React.useEffect(() => {
    if (!props.open) {
      setQuery("")
      setBusyKey(null)
      setError(null)
      setEditingCapabilityModel(null)
      setEditingProviderDefaults(false)
    }
  }, [props.open])

  // rows is the complete management-only support list, including provider-disabled models.
  // rows 是完整的仅管理支持列表，其中包含供应商已关闭模型。
  const rows = React.useMemo(
    () => props.snapshot && props.provider
      ? buildProviderModelAccessRows(props.snapshot, props.provider.id)
      : [],
    [props.provider, props.snapshot],
  )
  // visibleRows applies a local identity search without changing publication state.
  // visibleRows 应用本地身份搜索且不改变发布状态。
  const visibleRows = React.useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase()
    if (!normalizedQuery) {
      return rows
    }
    return rows.filter((row) => (
      row.id.toLocaleLowerCase().includes(normalizedQuery)
      || row.displayName.toLocaleLowerCase().includes(normalizedQuery)
    ))
  }, [query, rows])
  // enabledCount summarizes the provider publication boundary.
  // enabledCount 汇总供应商发布边界。
  const enabledCount = rows.filter((row) => row.enabled).length
  const providerDefaultModel = rows[0]?.model ?? null
  const providerCapabilityOverrides = props.provider
    ? props.snapshot?.providerCapabilityOverrides?.[props.provider.id]
    : undefined

  // persistModel saves one provider-model publication switch immediately.
  // persistModel 立即保存一个供应商模型发布开关。
  const persistModel = React.useCallback(async (row: ProviderModelAccessRow, enabled: boolean) => {
    if (!props.provider || busyKey) {
      return
    }
    setBusyKey(row.id)
    setError(null)
    try {
      props.onSaved(await savePublicAiModelEnabled({
        providerId: props.provider.id,
        modelId: row.id,
        enabled,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyKey(null)
    }
  }, [busyKey, props])

  const restoreModel = React.useCallback(async (row: ProviderModelAccessRow) => {
    if (!props.provider || busyKey) {
      return
    }
    setBusyKey(row.id)
    setError(null)
    try {
      props.onSaved(await savePublicAiModelEnabled({
        providerId: props.provider.id,
        modelId: row.id,
        enabled: row.defaultEnabled,
        restore: true,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyKey(null)
    }
  }, [busyKey, props])

  // persistAll saves the complete provider support union in one backend operation.
  // persistAll 通过一次后端操作保存完整供应商支持并集。
  const persistAll = React.useCallback(async (enabled: boolean) => {
    if (!props.provider || busyKey || rows.length === 0) {
      return
    }
    setBusyKey(enabled ? "__all_enabled__" : "__all_disabled__")
    setError(null)
    try {
      props.onSaved(await savePublicAiProviderModelsEnabled({
        providerId: props.provider.id,
        enabled,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyKey(null)
    }
  }, [busyKey, props, rows.length])

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open && busyKey) {
          return
        }
        props.onOpenChange(open)
      }}
      open={props.open}
    >
      <DialogContent className="flex max-h-[min(46rem,calc(100vh-3rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl">
        <DialogHeader className="border-b px-6 py-5 text-left">
          <DialogTitle>{t("settings.customProviders.manageModels")} · {props.provider?.display_name}</DialogTitle>
          <DialogDescription>{t("settings.providerModels.description")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 border-b px-6 py-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="text-sm font-medium">
              {t("settings.providerModels.count")
                .replace("{enabled}", String(enabledCount))
                .replace("{total}", String(rows.length))}
            </span>
            <div className="flex gap-2">
              <Button
                disabled={busyKey !== null || providerDefaultModel === null}
                onClick={() => setEditingProviderDefaults(true)}
                size="sm"
                type="button"
                variant="outline"
              >
                <SlidersHorizontal className="mr-2 size-4" />
                供应商默认
              </Button>
              <Button
                disabled={busyKey !== null || rows.length === 0 || enabledCount === rows.length}
                onClick={() => void persistAll(true)}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("settings.models.enableAll")}
              </Button>
              <Button
                disabled={busyKey !== null || rows.length === 0 || enabledCount === 0}
                onClick={() => void persistAll(false)}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("settings.models.disableAll")}
              </Button>
            </div>
          </div>
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              autoFocus
              className="pl-9"
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("settings.models.searchPlaceholder")}
              value={query}
            />
          </div>
          {error ? <p className="text-sm text-destructive" role="alert">{error}</p> : null}
        </div>
        <ScrollArea className="min-h-0 flex-1 px-6 py-4">
          {rows.length === 0 ? (
            <p className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
              {t("settings.providerModels.empty")}
            </p>
          ) : visibleRows.length === 0 ? (
            <p className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
              {t("settings.models.emptyFilter")}
            </p>
          ) : (
            <div className="grid gap-2">
              {visibleRows.map((row) => (
                <div className="flex min-h-16 items-center gap-3 rounded-lg border px-3 py-2.5" key={row.id}>
                  <div className="min-w-0 flex-1">
                    <strong className="block truncate text-sm">{row.displayName}</strong>
                    <span className="block truncate font-mono text-xs text-muted-foreground">{row.id}</span>
                  </div>
                  {busyKey === row.id ? <LoaderCircle className="size-4 animate-spin text-muted-foreground" /> : null}
                  <Badge variant={row.customized ? "secondary" : "outline"} title="模型访问层">
                    {row.customized ? "访问自定义" : "访问继承"}
                  </Badge>
                  <Badge variant={row.capabilityCustomized ? "secondary" : "outline"} title="模型能力层">
                    {row.capabilityCustomized ? "能力自定义" : "能力继承"}
                  </Badge>
                  <Badge variant={row.enabled ? "secondary" : "outline"}>
                    {t(row.enabled ? "settings.customModels.enabled" : "settings.customModels.disabled")}
                  </Badge>
                  {row.customized ? (
                    <Button
                      disabled={busyKey !== null}
                      onClick={() => void restoreModel(row)}
                      size="sm"
                      type="button"
                      variant="ghost"
                    >
                      {t("settings.modelCapabilities.restore")}
                    </Button>
                  ) : null}
                  <Button
                    aria-label={`${t("settings.modelCapabilities.title")} ${row.displayName}`}
                    disabled={busyKey !== null}
                    onClick={() => setEditingCapabilityModel(row)}
                    size="icon"
                    type="button"
                    variant="ghost"
                  >
                    <SlidersHorizontal aria-hidden="true" className="size-4" />
                  </Button>
                  <Switch
                    aria-label={`${row.displayName} · ${t("settings.customModels.enabled")}`}
                    checked={row.enabled}
                    disabled={busyKey !== null}
                    onCheckedChange={(enabled) => void persistModel(row, enabled)}
                  />
                </div>
              ))}
            </div>
          )}
        </ScrollArea>
      </DialogContent>
      <PublicAiModelCapabilityDialog
        model={editingCapabilityModel?.model ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setEditingCapabilityModel(null)
          }
        }}
        onSaved={props.onSaved}
        open={editingCapabilityModel !== null}
        overrides={editingCapabilityModel?.capabilityOverrides}
        providerDisplayName={props.provider?.display_name ?? ""}
        providerId={props.provider?.id ?? null}
      />
      <PublicAiModelCapabilityDialog
        model={providerDefaultModel}
        onOpenChange={(open) => {
          if (!open) {
            setEditingProviderDefaults(false)
          }
        }}
        onSaved={props.onSaved}
        open={editingProviderDefaults}
        overrides={providerCapabilityOverrides}
        providerDisplayName={props.provider?.display_name ?? ""}
        providerId={props.provider?.id ?? null}
        scope="provider"
      />
    </Dialog>
  )
}

// errorMessage converts one rejected save into readable inline feedback.
// errorMessage 把一次被拒绝的保存转换为可读行内反馈。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
