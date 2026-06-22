import * as React from "react"
import { FileText } from "lucide-react"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type {
  AiCapabilityTargetSaveInput,
} from "@/components/features/settings/settings-action-types"
import { useI18n } from "@/i18n"
import type {
  AiCapabilitySummary,
  AiCapabilityVendorSummary,
  AiCapabilityVendorTargetChoiceSummary,
} from "@/types"

import { localizedLiteral } from "./ai-capability-settings-utils"

// AiCapabilityTargetsDialog renders the WebUI-style target configuration modal.
// AiCapabilityTargetsDialog 渲染 WebUI 风格能力目标配置弹窗。
export function AiCapabilityTargetsDialog(props: {
  // Capability being configured.
  // 正在配置的能力。
  capability: AiCapabilitySummary
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Saves the ordered target list.
  // 保存有序目标列表。
  onSave: (targets: AiCapabilityTargetSaveInput[]) => Promise<void>
  // Configured vendors with catalog target choices.
  // 带目录目标选项的已配置供应商。
  vendors: AiCapabilityVendorSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [selectedVendorId, setSelectedVendorId] = React.useState<string | null>(null)
  const [drafts, setDrafts] = React.useState<AiCapabilityTargetDraft[]>([])
  React.useEffect(() => {
    if (!props.open) {
      return
    }
    setDrafts(props.capability.targets.map((target) => ({
      vendorId: target.vendorId,
      vendorName: target.vendorName,
      targetAlias: target.targetAlias,
      targetName: target.targetName || target.targetAlias,
      modelId: target.modelId,
      enabled: target.enabled,
      supportsModelOverride: true,
      modelOptions: [],
    })))
    setSelectedVendorId(props.vendors[0]?.vendorId ?? null)
  }, [props.capability.targets, props.open, props.vendors])
  const selectedVendor = props.vendors.find((vendor) => vendor.vendorId === selectedVendorId) ?? props.vendors[0] ?? null
  const choices = (selectedVendor?.targetChoices ?? []).filter((choice) => choice.capabilityKind === props.capability.kind)
  const selectedCount = drafts.filter((draft) => draft.enabled).length
  const toggleChoice = React.useCallback((vendor: AiCapabilityVendorSummary, choice: AiCapabilityVendorTargetChoiceSummary, checked: boolean) => {
    setDrafts((current) => {
      const existingIndex = current.findIndex((draft) => draft.vendorId === vendor.vendorId && draft.targetAlias === choice.targetAlias)
      if (!checked) {
        return existingIndex >= 0 ? current.filter((_, index) => index !== existingIndex) : current
      }
      const nextDraft: AiCapabilityTargetDraft = {
        vendorId: vendor.vendorId,
        vendorName: vendor.name,
        targetAlias: choice.targetAlias,
        targetName: choice.label,
        modelId: choice.defaultModelId || choice.targetAlias,
        enabled: true,
        supportsModelOverride: choice.supportsModelOverride,
        modelOptions: choice.modelOptions,
      }
      if (existingIndex < 0) {
        return [...current, nextDraft]
      }
      return current.map((draft, index) => index === existingIndex ? { ...draft, enabled: true } : draft)
    })
  }, [])
  const updateDraftModel = React.useCallback((vendorId: string, targetAlias: string, modelId: string) => {
    setDrafts((current) => current.map((draft) => (
      draft.vendorId === vendorId && draft.targetAlias === targetAlias ? { ...draft, modelId } : draft
    )))
  }, [])
  const save = React.useCallback(async () => {
    await props.onSave(drafts.map((draft) => ({
      vendorId: draft.vendorId,
      targetAlias: draft.targetAlias,
      modelId: draft.modelId,
      enabled: draft.enabled,
    })))
    props.onOpenChange(false)
  }, [drafts, props])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog model-list-dialog">
        <DialogHeader>
          <DialogTitle>{text(`配置接口 · ${props.capability.label}`, `Configure targets · ${props.capability.label}`)}</DialogTitle>
          <DialogDescription>{text("左侧选择供应商，右侧勾选要启用的接口。保存后会写回当前能力页的接口、顺序与配置。", "Select a vendor on the left and choose targets on the right. Save persists targets, order, and model config.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-split-layout modal">
          <aside className="settings-provider-nav" aria-label={text("供应商", "Providers")}>
            <div className="settings-split-column-title">{text("供应商", "Providers")}</div>
            {props.vendors.map((vendor) => {
              const vendorChoices = (vendor.targetChoices ?? []).filter((choice) => choice.capabilityKind === props.capability.kind)
              return (
                <button
                  className={`settings-provider-nav-row ${selectedVendor?.vendorId === vendor.vendorId ? "active" : ""}`}
                  key={vendor.vendorId}
                  onClick={() => setSelectedVendorId(vendor.vendorId)}
                  type="button"
                >
                  <FileText size={14} />
                  <span>{vendor.name}</span>
                  <small>[{vendorChoices.length}]</small>
                </button>
              )
            })}
          </aside>
          <section className="settings-model-list-pane">
            <div className="settings-model-pane-header">
              <div>
                <div className="settings-split-column-title">{text("接口列表", "Target list")}</div>
                <small>{selectedVendor?.name ?? text("未选择供应商", "No provider selected")}</small>
              </div>
              <span className="settings-muted-line">{text("已勾选", "Selected")} {selectedCount}</span>
            </div>
            <div className="settings-model-row-list modal">
              {!selectedVendor ? (
                <p className="settings-muted-line">{text("当前没有能力供应商。", "No capability vendor is configured.")}</p>
              ) : choices.length === 0 ? (
                <p className="settings-muted-line">{text("该供应商没有当前能力的接口。", "This vendor has no target for the current capability.")}</p>
              ) : choices.map((choice) => {
                const draft = drafts.find((item) => item.vendorId === selectedVendor.vendorId && item.targetAlias === choice.targetAlias)
                return (
                  <label className="settings-model-row selectable capability-target-choice" key={`${selectedVendor.vendorId}-${choice.targetAlias}`}>
                    <input
                      checked={Boolean(draft)}
                      onChange={(event) => toggleChoice(selectedVendor, choice, event.target.checked)}
                      type="checkbox"
                    />
                    <span>
                      <strong>{choice.label}</strong>
                      <small>{selectedVendor.vendorId}/{choice.targetAlias}</small>
                    </span>
                    {draft ? (
                      choice.modelOptions.length > 0 ? (
                        <select className="settings-input compact" onChange={(event) => updateDraftModel(selectedVendor.vendorId, choice.targetAlias, event.target.value)} value={draft.modelId}>
                          {choice.modelOptions.map((option) => <option key={option} value={option}>{option}</option>)}
                        </select>
                      ) : choice.supportsModelOverride ? (
                        <input className="settings-input compact" onChange={(event) => updateDraftModel(selectedVendor.vendorId, choice.targetAlias, event.target.value)} value={draft.modelId} />
                      ) : (
                        <span className="settings-muted-line">{draft.modelId}</span>
                      )
                    ) : null}
                  </label>
                )
              })}
            </div>
          </section>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AiCapabilityTargetDraft stores one dialog-local target binding draft.
// AiCapabilityTargetDraft 保存弹窗内单个目标绑定草稿。
interface AiCapabilityTargetDraft {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // User-facing vendor name.
  // 面向用户显示的供应商名称。
  vendorName: string
  // Stable target alias.
  // 稳定目标别名。
  targetAlias: string
  // User-facing target name.
  // 面向用户显示的目标名称。
  targetName: string
  // Provider request model id.
  // Provider 请求模型标识。
  modelId: string
  // Whether the target is enabled.
  // 目标是否启用。
  enabled: boolean
  // Whether the request model can be edited.
  // 请求模型是否可编辑。
  supportsModelOverride: boolean
  // Optional finite model choices.
  // 可选有限模型选项。
  modelOptions: string[]
}
