import * as React from "react"
import { FileText } from "lucide-react"

import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { useI18n } from "@/i18n"
import type { ProviderSummary } from "@/types"

import {
  ModelCapabilityDialog,
  ProviderModelsDialog,
  type ModelCapabilityTarget,
} from "./provider-model-dialogs"
import {
  formatProviderModelMeta,
  localizedLiteral,
} from "./provider-settings-utils"

// ModelListSettingsArea renders enabled models with a WebUI-style provider/model split.
// ModelListSettingsArea 使用 WebUI 风格供应商/模型分栏渲染已启用模型。
export function ModelListSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the enabled-model configuration dialog is open.
  // 启用模型配置弹窗是否打开。
  dialogOpen: boolean
  // Updates the enabled-model configuration dialog open state.
  // 更新启用模型配置弹窗打开状态。
  onDialogOpenChange: (open: boolean) => void
  // Provider summaries containing enabled model rows.
  // 包含已启用模型行的 Provider 摘要。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const configuredProviders = React.useMemo(() => props.providers.filter((provider) => provider.configured), [props.providers])
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  const [capabilityTarget, setCapabilityTarget] = React.useState<ModelCapabilityTarget | null>(null)
  React.useEffect(() => {
    if (configuredProviders.length === 0) {
      setSelectedProviderId(null)
      return
    }
    if (!selectedProviderId || !configuredProviders.some((provider) => provider.id === selectedProviderId)) {
      setSelectedProviderId(configuredProviders[0]?.id ?? null)
    }
  }, [configuredProviders, selectedProviderId])
  const enabledModelCount = props.providers.reduce((count, provider) => count + (provider.models?.length ?? 0), 0)
  const selectedProvider = configuredProviders.find((provider) => provider.id === selectedProviderId) ?? configuredProviders[0] ?? null
  return (
    <div className="settings-direct-content model-list-surface">
      <div className="settings-split-layout">
        <aside className="settings-provider-nav" aria-label={text("供应商", "Providers")}>
          <div className="settings-split-column-title">{text("供应商", "Providers")}</div>
          {configuredProviders.map((provider) => (
            <button
              className={`settings-provider-nav-row ${selectedProvider?.id === provider.id ? "active" : ""}`}
              key={provider.id}
              onClick={() => setSelectedProviderId(provider.id)}
              type="button"
            >
              <FileText size={14} />
              <span>{provider.name}</span>
              <small>[{provider.models?.length ?? 0}]</small>
            </button>
          ))}
        </aside>
        <section className="settings-model-list-pane" aria-label={text("已启用模型", "Enabled models")}>
          <div className="settings-model-pane-header">
            <div>
              <div className="settings-split-column-title">{text("已启用模型", "Enabled models")}</div>
              <small>{selectedProvider ? selectedProvider.name : text("未选择供应商", "No provider selected")} · {text("总启用", "Total enabled")} {enabledModelCount}</small>
            </div>
            {selectedProvider ? <span className="settings-muted-line">{selectedProvider.name}</span> : null}
          </div>
          <div className="settings-model-row-list">
            {!selectedProvider ? (
              <p className="settings-muted-line">{text("当前没有已配置供应商。", "No configured provider is available.")}</p>
            ) : (selectedProvider.models ?? []).length === 0 ? (
              <p className="settings-muted-line">{text("该供应商当前没有启用模型。", "This provider has no enabled model.")}</p>
            ) : selectedProvider.models?.map((model) => (
              <article className="settings-model-row" key={`${selectedProvider.id}-${model.id}`}>
                <div>
                  <strong>{model.name}</strong>
                  <span>{formatProviderModelMeta(model, text)}</span>
                </div>
                <button className="skill-action-button secondary" onClick={() => setCapabilityTarget({ provider: selectedProvider, model })} type="button">{text("编辑能力", "Edit capabilities")}</button>
              </article>
            ))}
          </div>
        </section>
      </div>
      <ProviderModelsDialog
        actions={props.actions}
        initialProviderId={selectedProvider?.id ?? null}
        onEditCapability={setCapabilityTarget}
        onOpenChange={props.onDialogOpenChange}
        open={props.dialogOpen}
        providers={configuredProviders}
      />
      <ModelCapabilityDialog
        actions={props.actions}
        onOpenChange={(open) => {
          if (!open) {
            setCapabilityTarget(null)
          }
        }}
        open={capabilityTarget !== null}
        target={capabilityTarget}
      />
    </div>
  )
}
