import * as React from "react"
import {
  FileText,
  SlidersHorizontal,
} from "lucide-react"

import { PanelTitle, StatusPill } from "@/components/shared/status"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { RuntimeModulesPanel } from "@/components/features/settings/settings-runtime-panels"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { useI18n } from "@/i18n"
import type {
  AiCapabilitySummary,
  AiCapabilityVendorCatalogSummary,
  AiCapabilityVendorSummary,
  ProviderSummary,
} from "@/types"

import { localizedLiteral } from "./ai-capability-settings-utils"

// AiCapabilityVendorsArea renders shared capability vendors and route targets.
// AiCapabilityVendorsArea 渲染共享能力供应商与路由目标。
export function AiCapabilityVendorsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Semantic AI capability summaries.
  // 语义 AI 能力摘要。
  capabilities: AiCapabilitySummary[]
  // Shared AI capability vendor summaries.
  // 共享 AI 能力供应商摘要。
  vendors: AiCapabilityVendorSummary[]
  // Supported AI capability vendor catalog choices.
  // 受支持 AI 能力供应商目录选项。
  catalog: AiCapabilityVendorCatalogSummary[]
  // Provider summaries used as model-provider context.
  // 作为模型供应商上下文的 Provider 摘要。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [editingVendor, setEditingVendor] = React.useState<AiCapabilityVendorSummary | null>(null)
  const [pickerOpen, setPickerOpen] = React.useState(false)
  const targets = props.capabilities.flatMap((capability) => capability.targets.map((target) => ({ capability, target })))
  const configuredVendorIds = new Set(props.vendors.map((vendor) => vendor.vendorId.toLowerCase()))
  const availableVendors = props.catalog.filter((vendor) => !configuredVendorIds.has(vendor.vendorId.toLowerCase()))
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <div className="settings-panel-title-row">
          <PanelTitle icon={<FileText size={17} />} title={text("能力供应商", "Capability vendors")} />
          <button className="skill-action-button" onClick={() => setPickerOpen(true)} type="button">{text("添加供应商", "Add vendor")}</button>
        </div>
        <div className="settings-grid compact">
          {props.vendors.length === 0 ? (
            <article className="settings-card"><strong>{text("暂无能力供应商", "No capability vendors")}</strong><p>{text("当前没有已配置的 AI 能力路由目标。", "No configured AI capability route targets.")}</p></article>
          ) : props.vendors.map((vendor) => (
            <article className="settings-card" key={vendor.vendorId}>
              <StatusPill status={vendor.enabled ? "ready" : "pending"} label={vendor.enabled ? text("启用", "Enabled") : text("停用", "Disabled")} />
              <strong>{vendor.name}</strong>
              <p>{vendor.vendorId} · {text(`${vendor.targetCount} 个能力目标`, `${vendor.targetCount} targets`)}</p>
              <div className="settings-card-actions">
                <button className="skill-action-button secondary" onClick={() => setEditingVendor(vendor)} type="button">{text("编辑", "Edit")}</button>
                <button className="skill-action-button secondary danger" onClick={() => void props.actions.onRemoveAiCapabilityVendor(vendor.vendorId)} type="button">{text("删除", "Delete")}</button>
              </div>
            </article>
          ))}
        </div>
        <AiCapabilityVendorPickerDialog
          catalog={availableVendors}
          onOpenChange={setPickerOpen}
          onSelect={(vendor) => {
            setPickerOpen(false)
            setEditingVendor({
              id: vendor.vendorId,
              vendorId: vendor.vendorId,
              name: vendor.name,
              enabled: true,
              apiKey: "",
              baseUrl: "",
              region: "",
              connectionMode: "local",
              targetCount: 0,
              targetChoices: [],
            })
          }}
          open={pickerOpen}
        />
        <AiCapabilityVendorDialog
          actions={props.actions}
          onOpenChange={(open) => {
            if (!open) {
              setEditingVendor(null)
            }
          }}
          open={editingVendor !== null}
          vendor={editingVendor}
        />
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={text("路由目标", "Route targets")} />
        <div className="settings-target-list">
          {targets.length === 0 ? (
            <p className="settings-muted-line">{text("没有可展示的能力路由目标。", "No capability route targets to display.")}</p>
          ) : targets.map(({ capability, target }) => (
            <article className="settings-target-row" key={`${capability.kind}-${target.targetSelector}`}>
              <div>
                <strong>{target.targetName || target.targetAlias}</strong>
                <span>{capability.label} · {target.vendorName} · {target.modelId}</span>
              </div>
              <button
                className={`skill-action-button ${target.enabled ? "" : "secondary"}`}
                onClick={() => void props.actions.onSetAiCapabilityTargetEnabled({
                  capabilityKind: capability.kind,
                  vendorId: target.vendorId,
                  targetAlias: target.targetAlias,
                  enabled: !target.enabled,
                })}
                type="button"
              >
                {target.enabled ? text("停用", "Disable") : text("启用", "Enable")}
              </button>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.providers.map((provider) => ({
        id: provider.id,
        label: provider.name,
        status: provider.active ? "ready" as const : "pending" as const,
        message: provider.enabledModels,
      }))} />
    </div>
  )
}

// AiCapabilityVendorDialog renders the shared AI capability vendor editor.
// AiCapabilityVendorDialog 渲染共享 AI 能力供应商编辑器。
function AiCapabilityVendorDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Vendor being edited.
  // 当前编辑的供应商。
  vendor: AiCapabilityVendorSummary | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [vendorId, setVendorId] = React.useState("")
  const [enabled, setEnabled] = React.useState(true)
  const [apiKey, setApiKey] = React.useState("")
  const [baseUrl, setBaseUrl] = React.useState("")
  const [region, setRegion] = React.useState("")
  const [connectionMode, setConnectionMode] = React.useState("local")
  React.useEffect(() => {
    setVendorId(props.vendor?.vendorId ?? "")
    setEnabled(props.vendor?.enabled ?? true)
    setApiKey(props.vendor?.apiKey ?? "")
    setBaseUrl(props.vendor?.baseUrl ?? "")
    setRegion(props.vendor?.region ?? "")
    setConnectionMode(props.vendor?.connectionMode ?? "local")
  }, [props.vendor])
  const save = React.useCallback(async () => {
    await props.actions.onSaveAiCapabilityVendor({
      vendorId,
      enabled,
      apiKey,
      baseUrl,
      region,
      connectionMode,
    })
    props.onOpenChange(false)
  }, [apiKey, baseUrl, connectionMode, enabled, props, region, vendorId])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog">
        <DialogHeader>
          <DialogTitle>{props.vendor ? text("编辑能力供应商", "Edit capability vendor") : text("添加能力供应商", "Add capability vendor")}</DialogTitle>
          <DialogDescription>{text("供应商 ID 例如 minimax、tavily、ollama。", "Vendor id examples: minimax, tavily, ollama.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
          <label className="settings-form-field">
            <span>{text("供应商 ID", "Vendor ID")}</span>
            <input className="settings-input" disabled onChange={(event) => setVendorId(event.target.value)} value={vendorId} />
          </label>
          <label className="settings-check-row inline">
            <input checked={enabled} onChange={(event) => setEnabled(event.target.checked)} type="checkbox" />
            <span>{text("启用供应商", "Enable vendor")}</span>
          </label>
          <label className="settings-form-field">
            <span>API Key</span>
            <input className="settings-input" onChange={(event) => setApiKey(event.target.value)} type="password" value={apiKey} />
          </label>
          <label className="settings-form-field">
            <span>Base URL</span>
            <input className="settings-input" onChange={(event) => setBaseUrl(event.target.value)} value={baseUrl} />
          </label>
          <label className="settings-form-field">
            <span>{text("区域", "Region")}</span>
            <input className="settings-input" onChange={(event) => setRegion(event.target.value)} value={region} />
          </label>
          <label className="settings-form-field">
            <span>{text("连接模式", "Connection mode")}</span>
            <select className="settings-input" onChange={(event) => setConnectionMode(event.target.value)} value={connectionMode}>
              <option value="local">{text("本地", "Local")}</option>
              <option value="cloud">Ollama Cloud</option>
            </select>
          </label>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AiCapabilityVendorPickerDialog renders the add-vendor catalog picker.
// AiCapabilityVendorPickerDialog 渲染添加能力供应商的目录选择器。
function AiCapabilityVendorPickerDialog(props: {
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Supported vendor catalog choices.
  // 支持的供应商目录选项。
  catalog: AiCapabilityVendorCatalogSummary[]
  // Handles one selected vendor.
  // 处理单个已选择供应商。
  onSelect: (vendor: AiCapabilityVendorCatalogSummary) => void
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
  const visibleVendors = props.catalog.filter((vendor) => {
    if (!query) {
      return true
    }
    return vendor.vendorId.toLowerCase().includes(query)
      || vendor.name.toLowerCase().includes(query)
      || vendor.capabilities.some((capability) => capability.toLowerCase().includes(query))
  })
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{text("添加能力供应商", "Add capability vendor")}</DialogTitle>
          <DialogDescription>{text("输入关键字过滤供应商或能力，选择后进入配置弹窗。", "Filter vendors or capabilities, then select one to configure.")}</DialogDescription>
        </DialogHeader>
        <input
          className="settings-input"
          onChange={(event) => setFilter(event.target.value)}
          placeholder={text("搜索供应商或能力", "Search vendors or capabilities")}
          value={filter}
        />
        <div className="settings-modal-list">
          {visibleVendors.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有可新增的供应商，或者过滤后没有匹配结果。", "No vendor can be added, or no result matches the filter.")}</p>
          ) : visibleVendors.map((vendor) => (
            <button className="settings-picker-row" key={vendor.vendorId} onClick={() => props.onSelect(vendor)} type="button">
              <strong>{vendor.name}</strong>
              <span>{vendor.vendorId} · {vendor.capabilities.length ? vendor.capabilities.join("、") : text("暂无能力", "No capabilities")}</span>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
