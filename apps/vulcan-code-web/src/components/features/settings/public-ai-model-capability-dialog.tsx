import * as React from "react"
import { LoaderCircle, RotateCcw } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { useI18n } from "@/i18n"
import { savePublicAiModelCapabilityOverrides } from "@/service/settings-actions-service"
import type {
  PublicAiModel,
  PublicAiModelCapabilityOverrides,
  PublicAiResourceKind,
  PublicAiSettingsSnapshot,
} from "@/types"

// ModelCapabilityDialogProps identifies one immutable model and its optional user difference.
// ModelCapabilityDialogProps 标识一个不可变模型及其可选用户差异。
interface ModelCapabilityDialogProps {
  model: PublicAiModel | null
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  overrides?: PublicAiModelCapabilityOverrides
  providerDisplayName: string | null
  providerId: string | null
}

// RESOURCE_KINDS is the complete non-text resource set owned by the common contract.
// RESOURCE_KINDS 是公共契约拥有的完整非文本资源集合。
const RESOURCE_KINDS: PublicAiResourceKind[] = ["image", "audio", "video", "file"]

// RESOURCE_LABEL_KEYS maps closed resource values to localized model-modality labels.
// RESOURCE_LABEL_KEYS 将封闭资源值映射到本地化模型模态标签。
const RESOURCE_LABEL_KEYS = {
  image: "settings.models.modality.image",
  audio: "settings.models.modality.audio",
  video: "settings.models.modality.video",
  file: "settings.models.modality.file",
} as const

// PublicAiModelCapabilityDialog edits only user differences from immutable capability sources.
// PublicAiModelCapabilityDialog 仅编辑用户相对于不可变能力来源的差异。
export function PublicAiModelCapabilityDialog(props: ModelCapabilityDialogProps) {
  const { t } = useI18n()
  const initializedModelKey = React.useRef<string | null>(null)
  const [contextLimit, setContextLimit] = React.useState(200000)
  const [overrideContext, setOverrideContext] = React.useState(false)
  const [inputResources, setInputResources] = React.useState<PublicAiResourceKind[]>([])
  const [outputResources, setOutputResources] = React.useState<PublicAiResourceKind[]>([])
  const [overrideInput, setOverrideInput] = React.useState(false)
  const [overrideOutput, setOverrideOutput] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!props.open) {
      initializedModelKey.current = null
      return
    }
    if (!props.providerId || !props.model) {
      return
    }
    const modelKey = `${props.providerId}\u0000${props.model.id}`
    if (initializedModelKey.current === modelKey) {
      return
    }
    setContextLimit(props.overrides?.context_limit ?? props.model.capabilities.context_limit ?? 200000)
    setOverrideContext(props.overrides?.context_limit !== undefined)
    setInputResources(props.overrides?.input_resources ?? props.model.capabilities.input_resources ?? [])
    setOutputResources(props.overrides?.output_resources ?? props.model.capabilities.output_resources ?? [])
    setOverrideInput(props.overrides?.input_resources !== undefined)
    setOverrideOutput(props.overrides?.output_resources !== undefined)
    setError(null)
    initializedModelKey.current = modelKey
  }, [props.model, props.open, props.overrides, props.providerId])

  // persist sends one exact difference object or null to restore source defaults.
  // persist 发送一个精确差异对象，或发送 null 以恢复来源默认值。
  const persist = React.useCallback(async (restore: boolean) => {
    if (!props.providerId || !props.model) {
      return
    }
    if (!restore && overrideContext && (!Number.isSafeInteger(contextLimit) || contextLimit <= 0)) {
      setError(t("settings.modelCapabilities.contextInvalid"))
      return
    }
    setSaving(true)
    setError(null)
    try {
      const nextSnapshot = await savePublicAiModelCapabilityOverrides({
        providerId: props.providerId,
        modelId: props.model.id,
        overrides: restore ? undefined : {
          context_limit: overrideContext ? contextLimit : undefined,
          input_resources: overrideInput ? inputResources : undefined,
          output_resources: overrideOutput ? outputResources : undefined,
        },
      })
      props.onSaved(nextSnapshot)
      props.onOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [contextLimit, inputResources, outputResources, overrideContext, overrideInput, overrideOutput, props, t])

  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="sm:max-w-[620px]">
        <DialogHeader>
          <DialogTitle>{t("settings.modelCapabilities.title")}</DialogTitle>
          <DialogDescription>
            {props.providerDisplayName} · {props.model?.display_name} · {t("settings.modelCapabilities.overlayHint")}
          </DialogDescription>
        </DialogHeader>
        <fieldset className="rounded-lg border p-4">
          <legend className="px-1 text-sm font-medium">{t("settings.modelCapabilities.contextLimit")}</legend>
          <label className="mb-4 flex items-center gap-2 text-sm">
            <Checkbox checked={overrideContext} onCheckedChange={(checked) => setOverrideContext(checked === true)} />
            {t("settings.modelCapabilities.overrideContext")}
          </label>
          <Input
            disabled={!overrideContext}
            inputMode="numeric"
            onChange={(event) => setContextLimit(Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))}
            value={contextLimit || ""}
          />
          <span className="mt-2 block text-xs font-normal text-muted-foreground">{t("settings.modelCapabilities.contextHint")}</span>
        </fieldset>
        <CapabilityResourceOverride
          enabled={overrideInput}
          label={t("settings.modelCapabilities.inputResources")}
          onEnabledChange={setOverrideInput}
          onValueChange={setInputResources}
          value={inputResources}
        />
        <CapabilityResourceOverride
          enabled={overrideOutput}
          label={t("settings.modelCapabilities.outputResources")}
          onEnabledChange={setOverrideOutput}
          onValueChange={setOutputResources}
          value={outputResources}
        />
        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        <DialogFooter className="sm:justify-between">
          <Button disabled={saving || !props.overrides} onClick={() => void persist(true)} type="button" variant="ghost">
            <RotateCcw className="mr-2 size-4" />
            {t("settings.modelCapabilities.restore")}
          </Button>
          <div className="flex justify-end gap-2">
            <Button onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.cancel")}</Button>
            <Button disabled={saving} onClick={() => void persist(false)} type="button">
              {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
              {t("memory.common.save")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// CapabilityResourceOverride distinguishes inheritance from an explicit empty modality set.
// CapabilityResourceOverride 区分继承来源与显式空模态集合。
function CapabilityResourceOverride(props: {
  enabled: boolean
  label: string
  onEnabledChange: (enabled: boolean) => void
  onValueChange: (value: PublicAiResourceKind[]) => void
  value: PublicAiResourceKind[]
}) {
  const { t } = useI18n()
  return (
    <fieldset className="rounded-lg border p-4">
      <legend className="px-1 text-sm font-medium">{props.label}</legend>
      <label className="mb-4 flex items-center gap-2 text-sm">
        <Checkbox checked={props.enabled} onCheckedChange={(checked) => props.onEnabledChange(checked === true)} />
        {t("settings.modelCapabilities.overrideResources")}
      </label>
      <div className="grid grid-cols-4 gap-2 max-sm:grid-cols-2">
        {RESOURCE_KINDS.map((resource) => (
          <label className={`flex items-center gap-2 rounded-md border px-3 py-2 text-sm ${props.enabled ? "" : "opacity-50"}`} key={resource}>
            <Checkbox
              checked={props.value.includes(resource)}
              disabled={!props.enabled}
              onCheckedChange={(checked) => props.onValueChange(checked ? [...props.value, resource] : props.value.filter((item) => item !== resource))}
            />
            {t(RESOURCE_LABEL_KEYS[resource])}
          </label>
        ))}
      </div>
    </fieldset>
  )
}
