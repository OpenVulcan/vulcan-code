import * as React from "react"
import { FileText } from "lucide-react"

import { PublicAiModelOverridesDialog } from "@/components/features/settings/public-ai-model-overrides-dialog"
import {
  loadPublicAiSettings,
  selectPublicAiModel,
} from "@/service/settings-actions-service"
import type {
  PublicAiConnection,
  PublicAiModel,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicModelRoute binds one discovered model to the credential that exposes it.
// PublicModelRoute 将一个发现模型绑定到暴露该模型的凭据。
interface PublicModelRoute {
  // Exact credential connection.
  // 精确凭据连接。
  connection: PublicAiConnection
  // Credential-scoped model snapshot.
  // 凭据作用域模型快照。
  model: PublicAiModel
}

// PublicProviderModels groups deduplicated models under one registered provider.
// PublicProviderModels 将去重模型归组到一个已注册供应商下。
interface PublicProviderModels {
  // Registered provider descriptor.
  // 已注册供应商描述。
  provider: PublicAiProviderDescriptor
  // Model routes keyed by exact model identifier.
  // 按精确模型标识索引的模型路由。
  models: Map<string, PublicModelRoute[]>
}

// ModelListSettingsArea renders the process-local public AI model catalog.
// ModelListSettingsArea 渲染进程内公共 AI 模型目录。
export function ModelListSettingsArea() {
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  const [busySelector, setBusySelector] = React.useState<string | null>(null)
  const [editingRoute, setEditingRoute] = React.useState<PublicModelRoute | null>(null)

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

  // activateRoute persists one exact provider, model, and account selection.
  // activateRoute 持久化一个精确供应商、模型与账号选择。
  const activateRoute = React.useCallback(async (route: PublicModelRoute) => {
    const selector = `${route.connection.id}/${route.model.id}`
    setBusySelector(selector)
    setError(null)
    try {
      setSnapshot(await selectPublicAiModel({
        providerId: route.connection.provider_id,
        modelId: route.model.id,
        connectionId: route.connection.id,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusySelector(null)
    }
  }, [])

  return (
    <div className="settings-direct-content model-list-surface">
      {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
      {!snapshot ? (
        <p className="settings-muted-line">正在读取实时模型目录…</p>
      ) : (
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
                return (
                  <article className="settings-model-row" key={`${selectedGroup.provider.id}/${modelId}`}>
                    <div>
                      <strong>{representative.model.display_name}</strong>
                      <span>{modelId} · {routes.length} 个可用账号</span>
                      <div className="settings-card-actions">
                        {routes.map((route) => {
                          const selector = `${route.connection.id}/${route.model.id}`
                          const active = snapshot.active?.connection_id === route.connection.id
                            && snapshot.active.model_id === route.model.id
                          return (
                            <React.Fragment key={selector}>
                              <button
                                className={`skill-action-button secondary ${active ? "active" : ""}`}
                                disabled={busySelector === selector}
                                onClick={() => void activateRoute(route)}
                                type="button"
                              >
                                {active ? "当前 · " : ""}{route.connection.display_name}
                              </button>
                              <button
                                className="skill-action-button secondary"
                                onClick={() => setEditingRoute(route)}
                                type="button"
                              >
                                附加条件
                              </button>
                            </React.Fragment>
                          )
                        })}
                      </div>
                    </div>
                  </article>
                )
              })}
            </div>
          </section>
        </div>
      )}
      <PublicAiModelOverridesDialog
        connection={editingRoute?.connection ?? null}
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
          record.connectionId === editingRoute?.connection.id
          && record.modelId === editingRoute?.model.id
        ))?.overrides}
      />
    </div>
  )
}

// buildProviderModelGroups derives provider/model/account rows from current memory catalogs.
// buildProviderModelGroups 从当前内存目录派生供应商、模型与账号行。
function buildProviderModelGroups(snapshot: PublicAiSettingsSnapshot): PublicProviderModels[] {
  return snapshot.providers
    .map((provider) => {
      const models = new Map<string, PublicModelRoute[]>()
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
          routes.push({ connection, model })
          models.set(model.id, routes)
        }
      }
      return { provider, models }
    })
    .filter((group) => group.models.size > 0)
}

// errorMessage converts rejected service values into user-visible diagnostics.
// errorMessage 将被拒绝的服务值转换为用户可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
