import * as React from "react"

import { StatusPill } from "@/components/shared/status"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { useI18n } from "@/i18n"
import type { ProviderSummary } from "@/types"

import { localizedLiteral } from "./provider-settings-utils"

// ProvidersArea renders model provider status.
// ProvidersArea 渲染模型 Provider 状态。
export function ProvidersArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Provider currently opened in the edit dialog.
  // 当前在编辑弹窗中打开的模型供应商。
  editingProvider: ProviderSummary | null
  // Updates the provider edit dialog target.
  // 更新模型供应商编辑弹窗目标。
  onEditingProviderChange: (provider: ProviderSummary | null) => void
  // Updates the add-provider picker open state.
  // 更新添加模型供应商选择弹窗的打开状态。
  onProviderPickerOpenChange: (open: boolean) => void
  // Whether the add-provider picker is open.
  // 添加模型供应商选择弹窗是否打开。
  pickerOpen: boolean
  // Provider summaries shown in settings.
  // 设置中展示的 Provider 摘要。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const configuredProviders = props.providers.filter((provider) => provider.configured)
  return (
    <div className="settings-direct-content">
      <div className="card-grid">
        {configuredProviders.length === 0 ? (
          <article className="settings-card">
            <strong>{text("暂无模型供应商", "No model providers")}</strong>
            <p>{text("点击添加后从目录中筛选供应商，再填写密钥等信息。", "Click Add, filter the catalog, then fill credentials and settings.")}</p>
          </article>
        ) : configuredProviders.map((provider) => (
          <article className="data-card" key={provider.id}>
            <StatusPill status={provider.configured ? "ready" : "pending"} label={provider.status} />
            <strong>{provider.name}</strong>
            <span>{provider.id}</span>
            <span>{text("已启用模型", "Enabled models")}：{provider.enabledModels}</span>
            <div className="settings-card-actions">
              <button className="skill-action-button secondary" onClick={() => props.onEditingProviderChange(provider)} type="button">{text("编辑", "Edit")}</button>
              <button className="skill-action-button secondary danger" onClick={() => void props.actions.onRemoveProvider(provider.id)} type="button">{text("删除", "Delete")}</button>
            </div>
          </article>
        ))}
      </div>
      <ProviderPickerDialog
        onOpenChange={props.onProviderPickerOpenChange}
        onSelect={(provider) => {
          props.onProviderPickerOpenChange(false)
          props.onEditingProviderChange(provider)
        }}
        open={props.pickerOpen}
        providers={props.providers.filter((provider) => !provider.configured)}
      />
      <ProviderEditDialog
        actions={props.actions}
        onOpenChange={(open) => {
          if (!open) {
            props.onEditingProviderChange(null)
          }
        }}
        open={props.editingProvider !== null}
        provider={props.editingProvider}
      />
    </div>
  )
}

// ProviderEditDialog renders the provider add/edit form.
// ProviderEditDialog 渲染 Provider 新增/编辑表单。
function ProviderEditDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Provider being edited.
  // 当前编辑的 Provider。
  provider: ProviderSummary | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [fields, setFields] = React.useState<Record<string, string>>({})
  React.useEffect(() => {
    const nextFields: Record<string, string> = {}
    props.provider?.fields?.forEach((field) => {
      nextFields[field.key] = field.value
    })
    setFields(nextFields)
  }, [props.provider])
  const save = React.useCallback(async () => {
    if (!props.provider) {
      return
    }
    await props.actions.onSaveProvider({ providerId: props.provider.id, fields })
    props.onOpenChange(false)
  }, [fields, props])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog">
        <DialogHeader>
          <DialogTitle>{props.provider?.configured ? text("编辑 Provider", "Edit provider") : text("添加 Provider", "Add provider")}</DialogTitle>
          <DialogDescription>{props.provider?.name}</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
          {(props.provider?.fields ?? []).map((field) => (
            <label className="settings-form-field" key={field.key}>
              <span>{field.label}</span>
              <input
                className="settings-input"
                onChange={(event) => setFields((current) => ({ ...current, [field.key]: event.target.value }))}
                placeholder={field.placeholder}
                type={field.secret ? "password" : "text"}
                value={fields[field.key] ?? ""}
              />
            </label>
          ))}
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ProviderPickerDialog renders the add-provider catalog picker.
// ProviderPickerDialog 渲染添加 Provider 的目录选择器。
function ProviderPickerDialog(props: {
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Provider catalog choices.
  // Provider 目录选项。
  providers: ProviderSummary[]
  // Handles one selected provider.
  // 处理单个已选择 Provider。
  onSelect: (provider: ProviderSummary) => void
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [filter, setFilter] = React.useState("")
  React.useEffect(() => {
    if (!props.open) {
      setFilter("")
    }
  }, [props.open])
  const query = filter.trim().toLowerCase()
  const visibleProviders = props.providers.filter((provider) => {
    if (!query) {
      return true
    }
    return provider.id.toLowerCase().includes(query) || provider.name.toLowerCase().includes(query)
  })
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{text("添加模型供应商", "Add model provider")}</DialogTitle>
          <DialogDescription>{text("输入关键字过滤供应商，选择后填写密钥等配置。", "Filter providers, then select one to fill credentials and settings.")}</DialogDescription>
        </DialogHeader>
        <input
          className="settings-input"
          onChange={(event) => setFilter(event.target.value)}
          placeholder={text("搜索供应商", "Search providers")}
          value={filter}
        />
        <div className="settings-modal-list">
          {visibleProviders.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有可新增的供应商，或者过滤后没有匹配结果。", "No provider can be added, or no result matches the filter.")}</p>
          ) : visibleProviders.map((provider) => (
            <button className="settings-picker-row" key={provider.id} onClick={() => props.onSelect(provider)} type="button">
              <strong>{provider.name}</strong>
              <span>{provider.id} · {(provider.catalogModels ?? []).length} models</span>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
