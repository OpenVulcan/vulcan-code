import * as React from "react"
import { FileText } from "lucide-react"

import { PublicAiModelOverridesDialog } from "@/components/features/settings/public-ai-model-overrides-dialog"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import {
  loadPublicAiSettings,
  savePublicAiModelEnabled,
} from "@/service/settings-actions-service"
import type {
  PublicAiModel,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicModelProtocolRoute binds one discovered model to its provider protocol scope.
// PublicModelProtocolRoute 将一个发现模型绑定到其供应商协议作用域。
interface PublicModelProtocolRoute {
  // Number of credential accounts exposing this exact route.
  // 暴露此精确路由的凭据账号数量。
  accountCount: number
  // Credential-discovered model snapshot used for exact capability controls.
  // 用于精确能力控件的凭据发现模型快照。
  model: PublicAiModel
  // Exact protocol profile identifier.
  // 精确协议配置档标识。
  protocolId: string
}

// PublicProviderModels groups deduplicated models under one registered provider.
// PublicProviderModels 将去重模型归组到一个已注册供应商下。
interface PublicProviderModels {
  // Registered provider descriptor.
  // 已注册供应商描述。
  provider: PublicAiProviderDescriptor
  // Protocol-scoped model routes keyed by exact model identifier.
  // 按精确模型标识索引的协议作用域模型路由。
  models: Map<string, PublicModelProtocolRoute[]>
}

// ModelListSettingsArea renders the process-local public AI model catalog.
// ModelListSettingsArea 渲染进程内公共 AI 模型目录。
export function ModelListSettingsArea() {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  const [editingRoute, setEditingRoute] = React.useState<PublicModelProtocolRoute | null>(null)
  const [pendingModelKeys, setPendingModelKeys] = React.useState<Set<string>>(() => new Set())

  React.useEffect(() => {
    let active = true
    void loadPublicAiSettings()
      .then((next) => {
        if (active) {
          setSnapshot(next)
          setError(null)
        }
      })
      .catch((reason) => {
        if (active) {
          setError(errorMessage(reason))
        }
      })
    return () => {
      active = false
    }
  }, [])

  const groupedProviders = React.useMemo(
    () => snapshot ? buildProviderModelGroups(snapshot) : [],
    [snapshot],
  )
  React.useEffect(() => {
    if (groupedProviders.length === 0) {
      setSelectedProviderId(null)
      return
    }
    if (!selectedProviderId || !groupedProviders.some((group) => group.provider.id === selectedProviderId)) {
      setSelectedProviderId(groupedProviders[0]?.provider.id ?? null)
    }
  }, [groupedProviders, selectedProviderId])

  const selectedGroup = groupedProviders.find((group) => group.provider.id === selectedProviderId)
    ?? groupedProviders[0]
    ?? null
  const totalModelCount = groupedProviders.reduce((total, group) => total + group.models.size, 0)

  return (
    <div className="settings-direct-content model-list-surface">
      {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
      {!snapshot ? (
        <p className="settings-muted-line">正在读取实时模型目录…</p>
      ) : (
        <>
          <div className="settings-split-layout">
          <aside className="settings-provider-nav" aria-label="供应商">
            <div className="settings-split-column-title">供应商</div>
            {groupedProviders.map((group) => (
              <button
                className={`settings-provider-nav-row ${selectedGroup?.provider.id === group.provider.id ? "active" : ""}`}
                key={group.provider.id}
                onClick={() => setSelectedProviderId(group.provider.id)}
                type="button"
              >
                <FileText size={14} />
                <span>{group.provider.display_name}</span>
                <small>[{group.models.size}]</small>
              </button>
            ))}
          </aside>
          <section className="settings-model-list-pane" aria-label="实时模型">
            <div className="settings-model-pane-header">
              <div>
                <div className="settings-split-column-title">实时模型</div>
                <small>{selectedGroup?.provider.display_name ?? "未选择供应商"} · 总计 {totalModelCount}</small>
              </div>
              <span className="settings-muted-line">仅保存在内存；启动和手动同步后更新</span>
            </div>
            <div className="settings-model-row-list">
              {!selectedGroup ? (
                <p className="settings-muted-line">当前没有已同步的模型。请先在“模型供应商”中添加账号并同步。</p>
              ) : Array.from(selectedGroup.models.entries()).map(([modelId, routes]) => {
                const representative = routes[0]
                if (!representative) {
                  return null
                }
                const availableAccountCount = routes.reduce(
                  (total, route) => total + route.accountCount,
                  0,
                )
                const modelKey = `${selectedGroup.provider.id}/${modelId}`
                const access = snapshot.modelAccessRecords.find((record) => (
                  record.providerId === selectedGroup.provider.id
                  && record.modelId === modelId
                ))
                const enabled = access?.enabled ?? true
                const pending = pendingModelKeys.has(modelKey)
                return (
                  <article className="settings-model-row" key={`${selectedGroup.provider.id}/${modelId}`}>
                    <div>
                      <strong>{representative.model.display_name}</strong>
                      <span>{modelId} · {availableAccountCount} 个可用账号</span>
                      <div className="settings-card-actions">
                        {routes.map((route) => {
                          const selector = `${selectedGroup.provider.id}/${route.model.id}/${route.protocolId}`
                          return (
                            <button
                              className="skill-action-button secondary"
                              key={selector}
                              onClick={() => setEditingRoute(route)}
                              type="button"
                            >
                              {protocolDisplayName(route.protocolId)} · 附加条件
                            </button>
                          )
                        })}
                      </div>
                    </div>
                    <div className="ml-auto flex shrink-0 items-center gap-2">
                      <span className="text-xs text-muted-foreground">
                        {enabled ? t("settings.models.enabled") : t("settings.models.disabled")}
                      </span>
                      <Switch
                        aria-label={`${t("settings.models.toggle")} ${representative.model.display_name}`}
                        checked={enabled}
                        disabled={pending}
                        onCheckedChange={(checked) => {
                          setPendingModelKeys((current) => new Set(current).add(modelKey))
                          void savePublicAiModelEnabled({
                            providerId: selectedGroup.provider.id,
                            modelId,
                            enabled: checked,
                          })
                            .then((next) => {
                              setSnapshot(next)
                              setError(null)
                            })
                            .catch((reason) => {
                              setError(errorMessage(reason))
                            })
                            .finally(() => {
                              setPendingModelKeys((current) => {
                                const next = new Set(current)
                                next.delete(modelKey)
                                return next
                              })
                            })
                        }}
                      />
                    </div>
                  </article>
                )
              })}
            </div>
          </section>
          </div>
        </>
      )}
      <PublicAiModelOverridesDialog
        model={editingRoute?.model ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setEditingRoute(null)
          }
        }}
        onSaved={(next) => {
          setSnapshot(next)
          setEditingRoute(null)
        }}
        open={editingRoute !== null}
        overrides={snapshot?.modelOverrideRecords.find((record) => (
          record.providerId === selectedGroup?.provider.id
          && record.modelId === editingRoute?.model.id
          && record.protocolId === editingRoute?.protocolId
        ))?.overrides}
        protocolId={editingRoute?.protocolId ?? null}
        providerDisplayName={selectedGroup?.provider.display_name ?? null}
        providerId={selectedGroup?.provider.id ?? null}
      />
    </div>
  )
}

// buildProviderModelGroups derives provider, model, and protocol rows from current memory catalogs.
// buildProviderModelGroups 从当前内存目录派生供应商、模型与协议行。
function buildProviderModelGroups(snapshot: PublicAiSettingsSnapshot): PublicProviderModels[] {
  return snapshot.providers
    .map((provider) => {
      const models = new Map<string, PublicModelProtocolRoute[]>()
      for (const connection of snapshot.connections) {
        if (connection.provider_id !== provider.id) {
          continue
        }
        const catalog = snapshot.catalogs[connection.id]
        if (!catalog) {
          continue
        }
        for (const model of catalog.models) {
          const routes = models.get(model.id) ?? []
          const existing = routes.find((route) => route.protocolId === connection.selected_protocol)
          if (existing) {
            existing.accountCount += 1
          } else {
            routes.push({
              accountCount: 1,
              model,
              protocolId: connection.selected_protocol,
            })
          }
          models.set(model.id, routes)
        }
      }
      return { provider, models }
    })
    .filter((group) => group.models.size > 0)
}

// protocolDisplayName maps exact protocol profile identifiers to user-visible labels.
// protocolDisplayName 把精确协议配置档标识映射为用户可见标签。
function protocolDisplayName(protocolId: string): string {
  const labels: Record<string, string> = {
    "anthropic-messages.v1": "Anthropic Messages",
    "anthropic-messages-video.v1": "Anthropic Messages + Video",
    "claude-code-messages.v1": "Claude Code Messages",
    "openai-chat-completions.v1": "OpenAI Chat Completions",
    "openai-codex-responses.v1": "OpenAI Codex Responses",
    "openai-responses.v1": "OpenAI Responses",
  }
  return labels[protocolId] ?? protocolId
}

// errorMessage converts rejected service values into user-visible diagnostics.
// errorMessage 将被拒绝的服务值转换为用户可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
