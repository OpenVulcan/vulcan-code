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
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { useI18n } from "@/i18n"
import type {
  ProviderModelSummary,
  ProviderSummary,
} from "@/types"

import {
  formatProviderModelMeta,
  localizedLiteral,
  localizedModalityLabel,
  nullablePositiveInteger,
  sameStringSet,
} from "./provider-settings-utils"

// ModelCapabilityTarget identifies one provider/model pair opened for capability editing.
// ModelCapabilityTarget 标识一个正在编辑能力的供应商与模型组合。
export interface ModelCapabilityTarget {
  // Provider that owns the model.
  // 拥有该模型的供应商。
  provider: ProviderSummary
  // Model whose capability metadata is being edited.
  // 正在编辑能力元数据的模型。
  model: ProviderModelSummary
}

// ProviderModelsDialog renders the WebUI-style enabled-model picker across providers.
// ProviderModelsDialog 渲染跨供应商的 WebUI 风格启用模型选择器。
export function ProviderModelsDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Initial provider id selected when the dialog opens.
  // 弹窗打开时初始选中的供应商标识。
  initialProviderId: string | null
  // Opens the capability editor for one provider/model pair.
  // 打开单个供应商/模型组合的能力编辑器。
  onEditCapability: (target: ModelCapabilityTarget) => void
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Configured providers available in the left column.
  // 左侧可用的已配置供应商。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [draftEnabledIds, setDraftEnabledIds] = React.useState<Record<string, Set<string>>>({})
  const [filter, setFilter] = React.useState("")
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  React.useEffect(() => {
    if (!props.open) {
      return
    }
    const nextDrafts = Object.fromEntries(props.providers.map((provider) => [
      provider.id,
      new Set((provider.catalogModels ?? []).filter((model) => model.enabled).map((model) => model.id)),
    ]))
    setDraftEnabledIds(nextDrafts)
    setSelectedProviderId(props.initialProviderId && props.providers.some((provider) => provider.id === props.initialProviderId)
      ? props.initialProviderId
      : props.providers[0]?.id ?? null)
    setFilter("")
  }, [props.initialProviderId, props.open, props.providers])
  const selectedProvider = props.providers.find((provider) => provider.id === selectedProviderId) ?? props.providers[0] ?? null
  const selectedEnabledIds = selectedProvider ? draftEnabledIds[selectedProvider.id] ?? new Set<string>() : new Set<string>()
  const visibleModels = (selectedProvider?.catalogModels ?? []).filter((model) => {
    const query = filter.trim().toLowerCase()
    if (!query) {
      return true
    }
    return model.id.toLowerCase().includes(query) || model.name.toLowerCase().includes(query) || (model.family ?? "").toLowerCase().includes(query)
  })
  const toggle = React.useCallback((providerId: string, modelId: string, enabled: boolean) => {
    setDraftEnabledIds((current) => {
      const next = new Set(current[providerId] ?? [])
      if (enabled) {
        next.add(modelId)
      } else {
        next.delete(modelId)
      }
      return { ...current, [providerId]: next }
    })
  }, [])
  const save = React.useCallback(async () => {
    for (const provider of props.providers) {
      const nextIds = Array.from(draftEnabledIds[provider.id] ?? [])
      if (sameStringSet(nextIds, (provider.catalogModels ?? []).filter((model) => model.enabled).map((model) => model.id))) {
        continue
      }
      await props.actions.onSaveProviderEnabledModels({
        providerId: provider.id,
        enabledModelIds: nextIds,
      })
    }
    props.onOpenChange(false)
  }, [draftEnabledIds, props])
  const selectCurrentFiltered = React.useCallback((enabled: boolean) => {
    if (!selectedProvider) {
      return
    }
    setDraftEnabledIds((current) => {
      const next = new Set(current[selectedProvider.id] ?? [])
      visibleModels.forEach((model) => {
        if (enabled) {
          next.add(model.id)
        } else {
          next.delete(model.id)
        }
      })
      return { ...current, [selectedProvider.id]: next }
    })
  }, [selectedProvider, visibleModels])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog model-list-dialog">
        <DialogHeader>
          <DialogTitle>{text("配置启用模型", "Configure enabled models")}</DialogTitle>
          <DialogDescription>{text("左侧切换供应商，右侧编辑完整模型目录。全选、取消等操作都会写入草稿，只有点击保存才会真正落盘。", "Switch providers on the left and edit the full model catalog on the right. Changes are drafted until Save.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-split-layout modal">
          <aside className="settings-provider-nav" aria-label={text("供应商", "Providers")}>
            <div className="settings-split-column-title">{text("供应商", "Providers")}</div>
            {props.providers.map((provider) => (
              <button
                className={`settings-provider-nav-row ${selectedProvider?.id === provider.id ? "active" : ""}`}
                key={provider.id}
                onClick={() => setSelectedProviderId(provider.id)}
                type="button"
              >
                <FileText size={14} />
                <span>{provider.name}</span>
                <small>[{draftEnabledIds[provider.id]?.size ?? 0}]</small>
              </button>
            ))}
          </aside>
          <section className="settings-model-list-pane" aria-label={text("模型目录", "Model catalog")}>
            <div className="settings-model-pane-header">
              <div>
                <div className="settings-split-column-title">{text("模型目录", "Model catalog")}</div>
                <small>{selectedProvider?.name ?? text("未选择供应商", "No provider selected")}</small>
              </div>
              <input
                className="settings-input settings-model-filter"
                onChange={(event) => setFilter(event.target.value)}
                placeholder={text("输入模型名称或 ID 过滤", "Filter by model name or id")}
                value={filter}
              />
            </div>
            <div className="settings-card-actions">
              <button className="skill-action-button secondary" onClick={() => selectCurrentFiltered(true)} type="button">{text("全选当前结果", "Select filtered")}</button>
              <button className="skill-action-button secondary" onClick={() => selectCurrentFiltered(false)} type="button">{text("取消当前结果", "Clear filtered")}</button>
            </div>
            <div className="settings-model-row-list modal">
              {visibleModels.map((model) => (
                <label className="settings-model-row selectable" key={model.id}>
                  <input
                    checked={selectedEnabledIds.has(model.id)}
                    onChange={(event) => selectedProvider ? toggle(selectedProvider.id, model.id, event.target.checked) : undefined}
                    type="checkbox"
                  />
                  <span>
                    <strong>{model.name}</strong>
                    <small>{formatProviderModelMeta(model, text)}</small>
                  </span>
                  {selectedProvider ? (
                    <button
                      className="skill-action-button secondary"
                      onClick={(event) => {
                        event.preventDefault()
                        props.onEditCapability({ provider: selectedProvider, model })
                      }}
                      type="button"
                    >
                      {text("编辑能力", "Edit capabilities")}
                    </button>
                  ) : null}
                </label>
              ))}
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

// ModelCapabilityDialog renders the model capability override editor.
// ModelCapabilityDialog 渲染模型能力覆盖编辑器。
export function ModelCapabilityDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Provider and model being edited.
  // 正在编辑的供应商与模型。
  target: ModelCapabilityTarget | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [reasoning, setReasoning] = React.useState(false)
  const [attachment, setAttachment] = React.useState(false)
  const [inputModalities, setInputModalities] = React.useState<Set<string>>(() => new Set(["text"]))
  const [outputModalities, setOutputModalities] = React.useState<Set<string>>(() => new Set(["text"]))
  const [reasoningTouched, setReasoningTouched] = React.useState(false)
  const [attachmentTouched, setAttachmentTouched] = React.useState(false)
  const [inputModalitiesTouched, setInputModalitiesTouched] = React.useState(false)
  const [outputModalitiesTouched, setOutputModalitiesTouched] = React.useState(false)
  const [contextLimit, setContextLimit] = React.useState("")
  const [inputLimit, setInputLimit] = React.useState("")
  const [outputLimit, setOutputLimit] = React.useState("")
  const [reasoningProfileJson, setReasoningProfileJson] = React.useState("")
  React.useEffect(() => {
    const model = props.target?.model
    setReasoning(model?.reasoning ?? false)
    setAttachment(model?.attachment ?? false)
    setInputModalities(new Set((model?.inputModalities ?? []).map((item) => item.toLowerCase())))
    setOutputModalities(new Set((model?.outputModalities ?? []).map((item) => item.toLowerCase())))
    setReasoningTouched(false)
    setAttachmentTouched(false)
    setInputModalitiesTouched(false)
    setOutputModalitiesTouched(false)
    setContextLimit(model?.contextLimit ? String(model.contextLimit) : "")
    setInputLimit(model?.inputLimit ? String(model.inputLimit) : "")
    setOutputLimit(model?.outputLimit ? String(model.outputLimit) : "")
    setReasoningProfileJson(model?.reasoningProfileJson ?? "")
  }, [props.target])
  const toggleModality = React.useCallback((kind: "input" | "output", value: string, checked: boolean) => {
    const setter = kind === "input" ? setInputModalities : setOutputModalities
    if (kind === "input") {
      setInputModalitiesTouched(true)
    } else {
      setOutputModalitiesTouched(true)
    }
    setter((current) => {
      const next = new Set(current)
      if (checked) {
        next.add(value)
      } else {
        next.delete(value)
      }
      return next
    })
  }, [])
  const save = React.useCallback(async () => {
    if (!props.target) {
      return
    }
    const model = props.target.model
    await props.actions.onSaveProviderModelOverride({
      providerId: props.target.provider.id,
      modelId: props.target.model.id,
      reasoning: reasoningTouched || typeof model.reasoning === "boolean" ? reasoning : null,
      attachment: attachmentTouched || typeof model.attachment === "boolean" ? attachment : null,
      contextLimit: nullablePositiveInteger(contextLimit),
      inputLimit: nullablePositiveInteger(inputLimit),
      outputLimit: nullablePositiveInteger(outputLimit),
      inputModalities: inputModalitiesTouched || Boolean(model.inputModalities?.length) ? Array.from(inputModalities) : null,
      outputModalities: outputModalitiesTouched || Boolean(model.outputModalities?.length) ? Array.from(outputModalities) : null,
      reasoningProfileJson: reasoningProfileJson.trim() || null,
    })
    props.onOpenChange(false)
  }, [attachment, attachmentTouched, contextLimit, inputLimit, inputModalities, inputModalitiesTouched, outputLimit, outputModalities, outputModalitiesTouched, props, reasoning, reasoningProfileJson, reasoningTouched])
  const model = props.target?.model
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{text("编辑模型能力", "Edit model capabilities")}</DialogTitle>
          <DialogDescription>{props.target ? `${props.target.provider.name} · ${model?.name}` : ""}</DialogDescription>
        </DialogHeader>
        <div className="model-capability-form">
          <div className="settings-card-actions">
            <label className="settings-check-row inline"><input checked={reasoning} onChange={(event) => { setReasoningTouched(true); setReasoning(event.target.checked) }} type="checkbox" />{text("声明支持思考 / 推理", "Supports reasoning")}</label>
            <label className="settings-check-row inline"><input checked={attachment} onChange={(event) => { setAttachmentTouched(true); setAttachment(event.target.checked) }} type="checkbox" />{text("声明附件标记", "Supports attachments")}</label>
          </div>
          <fieldset className="settings-modality-box">
            <legend>{text("输入能力", "Input capability")}</legend>
            {["text", "image", "audio", "video", "pdf"].map((item) => (
              <label className="settings-check-row inline" key={item}>
                <input checked={inputModalities.has(item)} onChange={(event) => toggleModality("input", item, event.target.checked)} type="checkbox" />
                {localizedModalityLabel("input", item, text)}
              </label>
            ))}
          </fieldset>
          <fieldset className="settings-modality-box">
            <legend>{text("输出能力", "Output capability")}</legend>
            {["text", "image", "audio", "video"].map((item) => (
              <label className="settings-check-row inline" key={item}>
                <input checked={outputModalities.has(item)} onChange={(event) => toggleModality("output", item, event.target.checked)} type="checkbox" />
                {localizedModalityLabel("output", item, text)}
              </label>
            ))}
          </fieldset>
          <div className="settings-form-grid three">
            <label className="settings-form-field"><span>{text("上下文长度", "Context limit")}</span><input className="settings-input" onChange={(event) => setContextLimit(event.target.value)} value={contextLimit} /></label>
            <label className="settings-form-field"><span>{text("输入长度", "Input limit")}</span><input className="settings-input" onChange={(event) => setInputLimit(event.target.value)} value={inputLimit} /></label>
            <label className="settings-form-field"><span>{text("输出长度", "Output limit")}</span><input className="settings-input" onChange={(event) => setOutputLimit(event.target.value)} value={outputLimit} /></label>
          </div>
          <label className="settings-form-field">
            <span>{text("思考模式附加配置（可选 JSON）", "Reasoning profile override (optional JSON)")}</span>
            <textarea className="settings-input textarea large" onChange={(event) => setReasoningProfileJson(event.target.value)} value={reasoningProfileJson} />
          </label>
          <p className="settings-muted-line">{text("修改只会写入该模型的用户覆盖配置，保存后会刷新模型能力结果。", "Changes are saved as user overrides for this model only and refresh the resolved capability result.")}</p>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
