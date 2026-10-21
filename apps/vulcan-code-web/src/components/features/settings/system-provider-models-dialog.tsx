import * as React from "react"
import { ListTree, LoaderCircle, Pencil, Plus, Trash2 } from "lucide-react"

import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { OPENAI_DIRECT_REASONING_EFFORTS } from "@/components/features/settings/direct-provider-model-settings"
import { useI18n } from "@/i18n"
import { savePublicAiCustomModels } from "@/service/settings-actions-service"
import type {
  PublicAiCustomModelDefinition,
  PublicAiProviderDescriptor,
  PublicAiResourceKind,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"
import "@/ui-kit/styles/features/settings-forms.css"

// SystemModelDraft keeps incomplete form fields separate from persisted model definitions.
// SystemModelDraft 将未完成的表单字段与持久化模型定义分开保存。
interface SystemModelDraft {
  id: string
  displayName: string
  serviceEntryId: string
  protocolProfileId: string
  contextLimit: string
  outputLimit: string
  enabled: boolean
  inputResources: PublicAiResourceKind[]
  reasoningEfforts: string[]
}

// SystemProviderModelsDialogProps binds editing to one exact system provider snapshot.
// SystemProviderModelsDialogProps 将编辑限定在一份精确系统供应商快照上。
interface SystemProviderModelsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  provider: PublicAiProviderDescriptor
  snapshot: PublicAiSettingsSnapshot
}

// emptyDraft starts a model with backend-owned capacity defaults and no guessed identity.
// emptyDraft 使用后端持有的容量默认值创建模型，不猜测模型身份。
function emptyDraft(
  entry: PublicAiServiceEntry | undefined,
  defaults: PublicAiSettingsSnapshot["manualModelDefaults"],
): SystemModelDraft {
  return {
    id: "",
    displayName: "",
    serviceEntryId: entry?.id ?? "",
    protocolProfileId: entry?.protocol_routes.find((route) => route.verified)?.protocol_profile_id ?? "",
    contextLimit: String(defaults.contextLimit),
    outputLimit: String(defaults.outputLimit),
    enabled: true,
    inputResources: [],
    reasoningEfforts: [],
  }
}

// draftFromModel isolates one persisted system model for editing.
// draftFromModel 将一个持久化系统模型复制为独立编辑草稿。
function draftFromModel(model: PublicAiCustomModelDefinition): SystemModelDraft {
  return {
    id: model.id,
    displayName: model.display_name,
    serviceEntryId: model.system_binding?.service_entry_id ?? "",
    protocolProfileId: model.system_binding?.protocol_profile_id ?? "",
    contextLimit: String(model.capabilities.context_limit ?? ""),
    outputLimit: String(model.capabilities.output_limit ?? ""),
    enabled: model.enabled,
    inputResources: [...(model.capabilities.input_resources ?? [])],
    reasoningEfforts: model.capabilities.reasoning === "native"
      ? [...(model.capabilities.reasoning_efforts ?? [])]
      : [],
  }
}

// positiveInteger accepts only exact positive safe integers for model token limits.
// positiveInteger 只接受模型 Token 限制所需的精确正安全整数。
function positiveInteger(value: string): number | null {
  if (!/^[1-9][0-9]*$/.test(value.trim())) return null
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) ? parsed : null
}

// modelFromDraft creates one user-owned text-output model without editing signed provider data.
// modelFromDraft 创建一个用户自有文本输出模型，不修改签名供应商数据。
function modelFromDraft(
  draft: SystemModelDraft,
  previous: PublicAiCustomModelDefinition | undefined,
): PublicAiCustomModelDefinition {
  const capabilities = previous ? structuredClone(previous.capabilities) : {
    operations: ["conversation_respond" as const],
    input_resources: [],
    output_resources: [],
    evidence_revision: "user.system-manual.v1",
    reasoning: "unknown",
    reasoning_control: "unknown",
    reasoning_efforts: [],
    prompt_cache: {
      provider_default_behavior: "Follow provider defaults without host-injected cache controls.",
      supports_managed_explicit: false,
      supported_ttl_options: [],
    },
    model_parameters: [],
  }
  capabilities.context_limit = positiveInteger(draft.contextLimit) ?? undefined
  capabilities.output_limit = positiveInteger(draft.outputLimit) ?? undefined
  capabilities.input_resources = [...draft.inputResources]
  // Reasoning levels are a user declaration for this exact manual identity.
  // 思考档位是用户针对这个精确手工模型身份作出的声明。
  const reasoningEnabled = draft.reasoningEfforts.some((effort) => effort !== "none")
  capabilities.reasoning = reasoningEnabled ? "native" : "unsupported"
  capabilities.reasoning_control = reasoningEnabled ? "effort_selection" : "fixed_off"
  capabilities.reasoning_efforts = reasoningEnabled ? [...draft.reasoningEfforts] : ["none"]
  return {
    id: draft.id.trim(),
    display_name: draft.displayName.trim(),
    enabled: draft.enabled,
    capabilities,
    request_projection: previous?.request_projection ?? {
      reasoning: {}, model_parameter_paths: {}, defaults: {}, overrides: {}, delete: [],
    },
    tool_schema_projection: previous?.tool_schema_projection ?? "lightweight",
    tool_mode: "direct",
    system_binding: {
      service_entry_id: draft.serviceEntryId,
      protocol_profile_id: draft.protocolProfileId,
    },
  }
}

// SystemProviderModelsDialog edits only the provider's user model overlay and its exact routes.
// SystemProviderModelsDialog 只编辑该供应商的用户模型覆盖与精确路由。
export function SystemProviderModelsDialog(props: SystemProviderModelsDialogProps) {
  const { t } = useI18n()
  // entries contains only conversation services that expose a verified protocol route.
  // entries 只包含拥有已验证协议路由的会话服务。
  const entries = React.useMemo(() => props.provider.service_entries.filter((entry) => (
    entry.kind === "conversation" && entry.protocol_routes.some((route) => route.verified)
  )), [props.provider.service_entries])
  const [models, setModels] = React.useState<PublicAiCustomModelDefinition[]>([])
  const [editingId, setEditingId] = React.useState<string | null>(null)
  const [draft, setDraft] = React.useState<SystemModelDraft | null>(null)
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  React.useEffect(() => {
    if (!props.open) return
    setModels(Object.values(props.snapshot.customModels?.[props.provider.id] ?? {}).map((model) => structuredClone(model)))
    setEditingId(null)
    setDraft(null)
    setError(null)
  }, [props.open, props.provider.id, props.snapshot])
  // selectedEntry and selectedRoute always derive from the current draft's exact identifiers.
  // selectedEntry 与 selectedRoute 始终从当前草稿的精确标识求得。
  const selectedEntry = entries.find((entry) => entry.id === draft?.serviceEntryId)
  const selectedRoute = selectedEntry?.protocol_routes.find((route) => (
    route.verified && route.protocol_profile_id === draft?.protocolProfileId
  ))
  const videoDeclared = selectedRoute?.input_resource_constraints?.some((constraint) => (
    constraint.resource_kind === "video"
  )) ?? false
  // Wire support and signed route declaration must both admit video.
  // 视频必须同时得到协议编码器和签名路由声明的支持。
  const videoEncoded = [
    "openai-chat-completions.v1",
    "anthropic-messages-video.v1",
    "google.gemini.generate_content",
    "google.code_assist.generate_content.v1",
    "gemini-interactions.v1",
  ].includes(draft?.protocolProfileId ?? "")
  const videoAvailable = videoDeclared && videoEncoded
  const audioEncoded = [
    "openai-chat-completions.v1",
    "google.gemini.generate_content",
    "google.code_assist.generate_content.v1",
    "gemini-interactions.v1",
  ].includes(draft?.protocolProfileId ?? "")
  // Gemini's current request projector accepts only four explicit thinking levels.
  // 当前 Gemini 请求投影器仅接受四个显式思考档位。
  const availableReasoningEfforts = ["google.gemini.generate_content", "google.code_assist.generate_content.v1", "gemini-interactions.v1"].includes(draft?.protocolProfileId ?? "")
    ? OPENAI_DIRECT_REASONING_EFFORTS.filter((effort) => ["minimal", "low", "medium", "high"].includes(effort))
    : OPENAI_DIRECT_REASONING_EFFORTS
  // persist sends the complete provider-owned replacement and adopts only the returned snapshot.
  // persist 发送完整供应商自有替换集合，且只采用后台返回的快照。
  const persist = async (replacement: PublicAiCustomModelDefinition[]) => {
    setBusy(true)
    setError(null)
    try {
      const snapshot = await savePublicAiCustomModels(props.provider.id, replacement)
      setModels(Object.values(snapshot.customModels?.[props.provider.id] ?? {}).map((model) => structuredClone(model)))
      props.onSaved(snapshot)
      setDraft(null)
      setEditingId(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setBusy(false)
    }
  }
  // saveDraft validates form-owned values before the backend validates route authority.
  // saveDraft 在后台校验路由权威之前校验表单自有数值。
  const saveDraft = () => {
    if (!draft) return
    if (!draft.id.trim() || !draft.displayName.trim() || !selectedRoute) {
      setError(t("settings.systemManualModels.required"))
      return
    }
    const contextLimit = positiveInteger(draft.contextLimit)
    const outputLimit = positiveInteger(draft.outputLimit)
    if (contextLimit === null || outputLimit === null || outputLimit > contextLimit) {
      setError(t("settings.systemManualModels.limitsInvalid"))
      return
    }
    if (draft.inputResources.includes("video") && !videoAvailable) {
      setError(t("settings.systemManualModels.videoUnavailable"))
      return
    }
    if (draft.inputResources.includes("audio") && !audioEncoded) {
      setError(t("settings.systemManualModels.audioUnavailable"))
      return
    }
    if (draft.reasoningEfforts.some((effort) => !availableReasoningEfforts.includes(effort as typeof availableReasoningEfforts[number]))) {
      setError(t("settings.systemManualModels.required"))
      return
    }
    const previous = models.find((model) => model.id === editingId)
    if (!editingId && models.some((model) => model.id === draft.id.trim())) {
      setError(t("settings.systemManualModels.duplicate"))
      return
    }
    const model = modelFromDraft(draft, previous)
    const replacement = editingId
      ? models.map((candidate) => candidate.id === editingId ? model : candidate)
      : [...models, model]
    void persist(replacement)
  }
  // toggleResource updates only the selected input modality in the local draft.
  // toggleResource 只更新草稿里指定的输入模态。
  const toggleResource = (resource: PublicAiResourceKind, checked: boolean) => {
    setDraft((current) => current ? {
      ...current,
      inputResources: checked
        ? [...current.inputResources.filter((value) => value !== resource), resource]
        : current.inputResources.filter((value) => value !== resource),
    } : current)
  }
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="vk-form-scrollable" size="lg">
        <DialogHeader>
          <DialogTitle>{t("settings.systemManualModels.title").replace("{provider}", props.provider.display_name)}</DialogTitle>
          <DialogDescription>{t("settings.systemManualModels.description")}</DialogDescription>
        </DialogHeader>
        {error ? <p className="vk-form-fault" role="alert">{error}</p> : null}
        <div className="vk-form-stack">
          {models.map((model) => (
            <div className="vk-form-row vk-form-framed" key={model.id}>
              <div className="vk-form-cell"><p className="vk-form-name">{model.display_name}</p><p className="vk-form-id">{model.id} · {model.system_binding?.service_entry_id} · {model.system_binding?.protocol_profile_id}</p></div>
              <div className="vk-form-inline">
                <Button aria-label={t("settings.systemManualModels.edit")} disabled={busy} onClick={() => { setEditingId(model.id); setDraft(draftFromModel(model)); setError(null) }} size="icon-sm" type="button" variant="outline"><Pencil className="vk-icon" /></Button>
                <Button aria-label={t("settings.systemManualModels.remove")} disabled={busy} onClick={() => void persist(models.filter((item) => item.id !== model.id))} size="icon-sm" type="button" variant="outline"><Trash2 className="vk-icon" /></Button>
              </div>
            </div>
          ))}
          {models.length === 0 ? <p className="vk-form-text">{t("settings.systemManualModels.empty")}</p> : null}
        </div>
        {!draft ? <Button disabled={busy || entries.length === 0} onClick={() => { setEditingId(null); setDraft(emptyDraft(entries[0], props.snapshot.manualModelDefaults)); setError(null) }} type="button" variant="outline"><Plus className="vk-icon" />{t("settings.customModels.addManual")}</Button> : (
          <div className="vk-form-box">
            <div className="vk-form-grid">
              <label className="vk-field"><span>{t("settings.customModels.modelId")}</span><Input disabled={busy || editingId !== null} value={draft.id} onChange={(event) => setDraft({ ...draft, id: event.target.value })} /></label>
              <label className="vk-field"><span>{t("settings.customModels.displayName")}</span><Input disabled={busy} value={draft.displayName} onChange={(event) => setDraft({ ...draft, displayName: event.target.value })} /></label>
              <label className="vk-field"><span>{t("settings.systemManualModels.entry")}</span><Select value={draft.serviceEntryId} onValueChange={(entryId) => { const entry = entries.find((item) => item.id === entryId); setDraft({ ...draft, serviceEntryId: entryId, protocolProfileId: entry?.protocol_routes.find((route) => route.verified)?.protocol_profile_id ?? "", inputResources: draft.inputResources.filter((resource) => resource !== "video" && resource !== "audio"), reasoningEfforts: [] }) }}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent>{entries.map((entry) => <SelectItem key={entry.id} value={entry.id}>{entry.display_name}</SelectItem>)}</SelectContent></Select></label>
              <label className="vk-field"><span>{t("settings.customModels.protocolSelect")}</span><Select value={draft.protocolProfileId} onValueChange={(protocolId) => setDraft({ ...draft, protocolProfileId: protocolId, inputResources: draft.inputResources.filter((resource) => resource !== "video" && resource !== "audio"), reasoningEfforts: [] })}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent>{selectedEntry?.protocol_routes.filter((route) => route.verified).map((route) => <SelectItem key={route.protocol_profile_id} value={route.protocol_profile_id}>{route.protocol_profile_id}</SelectItem>)}</SelectContent></Select></label>
              <label className="vk-field"><span>{t("settings.customModels.contextLimit")}</span><Input disabled={busy} inputMode="numeric" value={draft.contextLimit} onChange={(event) => setDraft({ ...draft, contextLimit: event.target.value })} /></label>
              <label className="vk-field"><span>{t("settings.customModels.outputLimit")}</span><Input disabled={busy} inputMode="numeric" value={draft.outputLimit} onChange={(event) => setDraft({ ...draft, outputLimit: event.target.value })} /></label>
            </div>
            <fieldset className="vk-form-stack"><legend className="vk-field-legend" data-variant="label">{t("settings.customModels.input")}</legend><div className="vk-form-checks">{(["image", "audio", "video", "file"] as const).map((resource) => <label className="vk-check-label" key={resource}><Checkbox checked={draft.inputResources.includes(resource)} disabled={busy || (resource === "audio" && !audioEncoded) || (resource === "video" && !videoAvailable)} onCheckedChange={(checked) => toggleResource(resource, checked === true)} />{t(`settings.systemManualModels.input.${resource}`)}</label>)}</div></fieldset>
            <fieldset className="vk-form-stack"><legend className="vk-field-legend" data-variant="label">{t("settings.customModels.reasoning")}</legend><div className="vk-form-checks">{availableReasoningEfforts.map((effort) => <label className="vk-check-label" key={effort}><Checkbox checked={draft.reasoningEfforts.includes(effort)} disabled={busy} onCheckedChange={(checked) => setDraft({ ...draft, reasoningEfforts: checked ? [...draft.reasoningEfforts, effort] : draft.reasoningEfforts.filter((value) => value !== effort) })} />{effort}</label>)}</div></fieldset>
            <label className="vk-check-label"><Checkbox checked={draft.enabled} disabled={busy} onCheckedChange={(checked) => setDraft({ ...draft, enabled: checked === true })} />{t("settings.customModels.enabled")}</label>
            <div className="vk-form-acts"><Button disabled={busy} onClick={() => { setDraft(null); setEditingId(null); setError(null) }} type="button" variant="outline">{t("settings.systemManualModels.cancel")}</Button><Button disabled={busy} onClick={saveDraft} type="button">{busy ? <LoaderCircle className="vk-spin" /> : <ListTree className="vk-icon" />}{t("settings.customModels.save")}</Button></div>
          </div>
        )}
        <DialogFooter><Button disabled={busy} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("ui.close")}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
