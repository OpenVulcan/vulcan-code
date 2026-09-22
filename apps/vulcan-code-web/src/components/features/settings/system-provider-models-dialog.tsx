import * as React from "react"
import { ListTree, LoaderCircle, Pencil, Plus, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
import { savePublicAiCustomModels } from "@/service/settings-actions-service"
import type {
  PublicAiCustomModelDefinition,
  PublicAiProviderDescriptor,
  PublicAiResourceKind,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"

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

// emptyDraft starts a model with no guessed identifier, capacity, or input modality.
// emptyDraft 以不猜测身份、容量或输入模态的状态开始新增模型。
function emptyDraft(entry: PublicAiServiceEntry | undefined): SystemModelDraft {
  return {
    id: "",
    displayName: "",
    serviceEntryId: entry?.id ?? "",
    protocolProfileId: entry?.protocol_routes.find((route) => route.verified)?.protocol_profile_id ?? "",
    contextLimit: "",
    outputLimit: "",
    enabled: true,
    inputResources: [],
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
  return {
    id: draft.id.trim(),
    display_name: draft.displayName.trim(),
    enabled: draft.enabled,
    capabilities,
    request_projection: previous?.request_projection ?? {
      reasoning: {}, model_parameter_paths: {}, defaults: {}, overrides: {}, delete: [],
    },
    tool_schema_projection: previous?.tool_schema_projection ?? "lightweight",
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
  const audioEncoded = [
    "openai-chat-completions.v1",
    "gemini-generate-content.v1",
    "google-code-assist.v1",
    "gemini-interactions.v1",
  ].includes(draft?.protocolProfileId ?? "")
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
    if (draft.inputResources.includes("video") && !videoDeclared) {
      setError(t("settings.systemManualModels.videoUnavailable"))
      return
    }
    if (draft.inputResources.includes("audio") && !audioEncoded) {
      setError(t("settings.systemManualModels.audioUnavailable"))
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
      <DialogContent className="max-h-[90vh] max-w-2xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t("settings.systemManualModels.title").replace("{provider}", props.provider.display_name)}</DialogTitle>
          <DialogDescription>{t("settings.systemManualModels.description")}</DialogDescription>
        </DialogHeader>
        {error ? <p className="rounded-md border border-destructive/40 p-3 text-sm text-destructive">{error}</p> : null}
        <div className="space-y-2">
          {models.map((model) => (
            <div className="flex items-center justify-between gap-2 rounded-md border p-2" key={model.id}>
              <div className="min-w-0"><p className="truncate font-medium">{model.display_name}</p><p className="truncate font-mono text-xs text-muted-foreground">{model.id} · {model.system_binding?.service_entry_id} · {model.system_binding?.protocol_profile_id}</p></div>
              <div className="flex gap-1">
                <Button aria-label={t("settings.systemManualModels.edit")} disabled={busy} onClick={() => { setEditingId(model.id); setDraft(draftFromModel(model)); setError(null) }} size="icon-sm" type="button" variant="outline"><Pencil className="size-4" /></Button>
                <Button aria-label={t("settings.systemManualModels.remove")} disabled={busy} onClick={() => void persist(models.filter((item) => item.id !== model.id))} size="icon-sm" type="button" variant="outline"><Trash2 className="size-4" /></Button>
              </div>
            </div>
          ))}
          {models.length === 0 ? <p className="text-sm text-muted-foreground">{t("settings.systemManualModels.empty")}</p> : null}
        </div>
        {!draft ? <Button disabled={busy || entries.length === 0} onClick={() => { setEditingId(null); setDraft(emptyDraft(entries[0])); setError(null) }} type="button" variant="outline"><Plus className="mr-2 size-4" />{t("settings.customModels.addManual")}</Button> : (
          <div className="space-y-3 rounded-md border p-3">
            <div className="grid grid-cols-2 gap-3">
              <label className="space-y-1 text-sm"><span>{t("settings.customModels.modelId")}</span><Input disabled={busy || editingId !== null} value={draft.id} onChange={(event) => setDraft({ ...draft, id: event.target.value })} /></label>
              <label className="space-y-1 text-sm"><span>{t("settings.customModels.displayName")}</span><Input disabled={busy} value={draft.displayName} onChange={(event) => setDraft({ ...draft, displayName: event.target.value })} /></label>
              <label className="space-y-1 text-sm"><span>{t("settings.systemManualModels.entry")}</span><Select value={draft.serviceEntryId} onValueChange={(entryId) => { const entry = entries.find((item) => item.id === entryId); setDraft({ ...draft, serviceEntryId: entryId, protocolProfileId: entry?.protocol_routes.find((route) => route.verified)?.protocol_profile_id ?? "", inputResources: draft.inputResources.filter((resource) => resource !== "video") }) }}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent>{entries.map((entry) => <SelectItem key={entry.id} value={entry.id}>{entry.display_name}</SelectItem>)}</SelectContent></Select></label>
              <label className="space-y-1 text-sm"><span>{t("settings.customModels.protocolSelect")}</span><Select value={draft.protocolProfileId} onValueChange={(protocolId) => setDraft({ ...draft, protocolProfileId: protocolId, inputResources: draft.inputResources.filter((resource) => resource !== "video" && resource !== "audio") })}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent>{selectedEntry?.protocol_routes.filter((route) => route.verified).map((route) => <SelectItem key={route.protocol_profile_id} value={route.protocol_profile_id}>{route.protocol_profile_id}</SelectItem>)}</SelectContent></Select></label>
              <label className="space-y-1 text-sm"><span>{t("settings.customModels.contextLimit")}</span><Input disabled={busy} inputMode="numeric" value={draft.contextLimit} onChange={(event) => setDraft({ ...draft, contextLimit: event.target.value })} /></label>
              <label className="space-y-1 text-sm"><span>{t("settings.customModels.outputLimit")}</span><Input disabled={busy} inputMode="numeric" value={draft.outputLimit} onChange={(event) => setDraft({ ...draft, outputLimit: event.target.value })} /></label>
            </div>
            <fieldset className="space-y-2"><legend className="text-sm font-medium">{t("settings.customModels.input")}</legend><div className="flex flex-wrap gap-4">{(["image", "audio", "video", "file"] as const).map((resource) => <label className="flex items-center gap-2 text-sm" key={resource}><Checkbox checked={draft.inputResources.includes(resource)} disabled={busy || (resource === "audio" && !audioEncoded) || (resource === "video" && !videoDeclared)} onCheckedChange={(checked) => toggleResource(resource, checked === true)} />{t(`settings.systemManualModels.input.${resource}`)}</label>)}</div></fieldset>
            <label className="flex items-center gap-2 text-sm"><Checkbox checked={draft.enabled} disabled={busy} onCheckedChange={(checked) => setDraft({ ...draft, enabled: checked === true })} />{t("settings.customModels.enabled")}</label>
            <div className="flex justify-end gap-2"><Button disabled={busy} onClick={() => { setDraft(null); setEditingId(null); setError(null) }} type="button" variant="outline">{t("settings.systemManualModels.cancel")}</Button><Button disabled={busy} onClick={saveDraft} type="button">{busy ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : <ListTree className="mr-2 size-4" />}{t("settings.customModels.save")}</Button></div>
          </div>
        )}
        <DialogFooter><Button disabled={busy} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("ui.close")}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
