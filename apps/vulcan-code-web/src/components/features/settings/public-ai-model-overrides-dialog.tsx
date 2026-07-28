import * as React from "react"
import { Database, Gauge, SlidersHorizontal } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { useI18n } from "@/i18n"
import { savePublicAiModelOverrides } from "@/service/settings-actions-service"
import type {
  PublicAiModel,
  PublicAiModelOverrides,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicAiModelOverridesDialog edits adapter-declared controls for one provider, model, and protocol.
// PublicAiModelOverridesDialog 编辑一个供应商、模型与协议由适配器声明的控制项。
export function PublicAiModelOverridesDialog(props: {
  // User-visible provider name.
  // 用户可见供应商名称。
  providerDisplayName: string | null
  // Exact provider identifier.
  // 精确供应商标识。
  providerId: string | null
  // Exact runtime-discovered model.
  // 精确运行时发现模型。
  model: PublicAiModel | null
  // Exact protocol profile identifier.
  // 精确协议配置档标识。
  protocolId: string | null
  // Existing override value.
  // 现有覆盖值。
  overrides?: PublicAiModelOverrides
  // Dialog visibility.
  // 弹窗可见性。
  open: boolean
  // Updates dialog visibility.
  // 更新弹窗可见性。
  onOpenChange: (open: boolean) => void
  // Receives the authoritative settings snapshot after save.
  // 保存后接收权威设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { t } = useI18n()
  const [contextLimit, setContextLimit] = React.useState("")
  const [outputLimit, setOutputLimit] = React.useState("")
  const [cacheMode, setCacheMode] = React.useState("provider_default")
  const [cacheTtl, setCacheTtl] = React.useState("")
  const [parameterValues, setParameterValues] = React.useState<Record<string, string>>({})
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)
  const modelParameters = React.useMemo(
    () => applicableModelParameters(props.model, props.protocolId),
    [props.model, props.protocolId],
  )

  React.useEffect(() => {
    if (!props.open || !props.model) {
      return
    }
    const overrides = props.overrides
    setContextLimit(overrides?.context_limit?.toString() ?? "")
    setOutputLimit(overrides?.output_limit?.toString() ?? "")
    const promptCache = overrides?.prompt_cache as {
      mode?: string
      ttl?: { option_id?: string }
    } | undefined
    setCacheMode(promptCache?.mode ?? "provider_default")
    setCacheTtl(
      promptCache?.ttl?.option_id
      ?? props.model.capabilities.prompt_cache.profile_default_ttl_option_id
      ?? props.model.capabilities.prompt_cache.supported_ttl_options[0]?.id
      ?? "",
    )
    setParameterValues(
      Object.fromEntries(
        modelParameters.map((definition) => {
          const current = overrides?.model_parameters?.[definition.id] as { value?: unknown } | undefined
          return [definition.id, current?.value === undefined ? "" : String(current.value)]
        }),
      ),
    )
    setError(null)
  }, [modelParameters, props.model, props.open, props.overrides])

  // save validates browser scalar input and lets the backend enforce the authoritative contract.
  // save 校验浏览器标量输入，并由后端执行权威契约校验。
  const save = React.useCallback(async () => {
    if (!props.providerId || !props.model || !props.protocolId) {
      return
    }
    setSaving(true)
    setError(null)
    try {
      const modelParameters: Record<string, unknown> = {}
      for (const definition of applicableModelParameters(props.model, props.protocolId)) {
        const raw = parameterValues[definition.id]?.trim() ?? ""
        if (!raw) {
          continue
        }
        switch (definition.schema.type) {
          case "boolean":
            modelParameters[definition.id] = { type: "boolean", value: raw === "true" }
            break
          case "integer": {
            const value = Number(raw)
            if (!Number.isSafeInteger(value)) {
              throw new Error(`${definition.display_name}: ${t("settings.modelOverrides.validation.safeInteger")}`)
            }
            modelParameters[definition.id] = { type: "integer", value }
            break
          }
          case "decimal":
            modelParameters[definition.id] = { type: "decimal", value: raw }
            break
          case "string":
          case "enum":
            modelParameters[definition.id] = { type: "string", value: raw }
            break
          default:
            throw new Error(`${definition.display_name} 的复杂参数编辑器尚未注册。`)
        }
      }
      const parseLimit = (value: string, label: string): number | undefined => {
        const trimmed = value.trim()
        if (!trimmed) {
          return undefined
        }
        const parsed = Number(trimmed)
        if (!Number.isSafeInteger(parsed) || parsed <= 0) {
          throw new Error(`${label}: ${t("settings.modelOverrides.validation.positiveInteger")}`)
        }
        return parsed
      }
      const overrides: PublicAiModelOverrides = {
        context_limit: parseLimit(contextLimit, t("settings.modelOverrides.contextLimit")),
        output_limit: parseLimit(outputLimit, t("settings.modelOverrides.outputLimit")),
        prompt_cache: cacheMode === "managed_explicit"
          ? {
              mode: "managed_explicit",
              ttl: { type: "capability_option", option_id: cacheTtl },
            }
          : { mode: "provider_default" },
        model_parameters: modelParameters,
      }
      props.onSaved(await savePublicAiModelOverrides({
        providerId: props.providerId,
        modelId: props.model.id,
        protocolId: props.protocolId,
        overrides,
      }))
      props.onOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [
    cacheMode,
    cacheTtl,
    contextLimit,
    outputLimit,
    parameterValues,
    props,
    t,
  ])

  const promptCache = props.model?.capabilities.prompt_cache
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-h-[calc(100vh-2rem)] gap-0 overflow-hidden p-0 sm:max-w-xl">
        <DialogHeader className="border-b bg-muted/30 px-5 py-4 pr-12">
          <div className="flex items-start gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-xl border bg-background shadow-xs">
              <SlidersHorizontal className="size-4 text-muted-foreground" />
            </div>
            <div className="min-w-0 space-y-2">
              <div className="space-y-1">
                <DialogTitle>{t("settings.modelOverrides.title")}</DialogTitle>
                <DialogDescription>{t("settings.modelOverrides.description")}</DialogDescription>
              </div>
              <div className="flex flex-wrap gap-1.5">
                <Badge variant="outline">{props.providerDisplayName}</Badge>
                <Badge variant="secondary">{props.model?.display_name}</Badge>
                <Badge variant="outline">{protocolDisplayName(props.protocolId)}</Badge>
              </div>
            </div>
          </div>
        </DialogHeader>
        <ScrollArea className="max-h-[calc(100vh-12rem)]">
          <div className="space-y-4 p-5">
            <section className="space-y-4 rounded-xl border bg-card/40 p-4">
              <div className="flex items-start gap-2.5">
                <Gauge className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div className="space-y-1">
                  <h3 className="text-sm font-semibold">{t("settings.modelOverrides.limits")}</h3>
                  <p className="text-xs leading-relaxed text-muted-foreground">
                    {t("settings.modelOverrides.limitsDescription")}
                  </p>
                </div>
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <ModelLimitField
                  capabilityLimit={props.model?.capabilities.context_limit}
                  id="model-override-context-limit"
                  label={t("settings.modelOverrides.contextLimit")}
                  onChange={setContextLimit}
                  value={contextLimit}
                />
                <ModelLimitField
                  capabilityLimit={props.model?.capabilities.output_limit}
                  id="model-override-output-limit"
                  label={t("settings.modelOverrides.outputLimit")}
                  onChange={setOutputLimit}
                  value={outputLimit}
                />
              </div>
            </section>

            <section className="space-y-4 rounded-xl border bg-card/40 p-4">
              <div className="flex items-start gap-2.5">
                <Database className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div className="space-y-1">
                  <h3 className="text-sm font-semibold">{t("settings.modelOverrides.cache")}</h3>
                  <p className="text-xs leading-relaxed text-muted-foreground">
                    {t("settings.modelOverrides.cacheDescription")}
                  </p>
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor="model-override-cache-mode">{t("settings.modelOverrides.cacheMode")}</Label>
                <Select onValueChange={setCacheMode} value={cacheMode}>
                  <SelectTrigger className="w-full" id="model-override-cache-mode">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="provider_default">
                      {t("settings.modelOverrides.providerDefault")}
                    </SelectItem>
                    {promptCache?.supports_managed_explicit ? (
                      <SelectItem value="managed_explicit">
                        {t("settings.modelOverrides.managedExplicit")}
                      </SelectItem>
                    ) : null}
                  </SelectContent>
                </Select>
                {promptCache?.provider_default_behavior ? (
                  <p className="rounded-lg bg-muted/50 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
                    {promptCache.provider_default_behavior}
                  </p>
                ) : null}
              </div>
              {cacheMode === "managed_explicit" ? (
                <div className="space-y-2">
                  <Label htmlFor="model-override-cache-ttl">{t("settings.modelOverrides.cacheTtl")}</Label>
                  <Select onValueChange={setCacheTtl} value={cacheTtl}>
                    <SelectTrigger className="w-full" id="model-override-cache-ttl">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {promptCache?.supported_ttl_options.map((option) => (
                        <SelectItem key={option.id} value={option.id}>{option.display_name}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              ) : null}
            </section>

            <section className="space-y-4 rounded-xl border bg-card/40 p-4">
              <div className="space-y-1">
                <h3 className="text-sm font-semibold">{t("settings.modelOverrides.parameters")}</h3>
                <p className="text-xs leading-relaxed text-muted-foreground">
                  {t("settings.modelOverrides.parametersDescription")}
                </p>
              </div>
              {modelParameters.length > 0 ? <Separator /> : null}
              {modelParameters.length > 0 ? (
                <div className="space-y-4">
                  {modelParameters.map((definition) => (
                    <ModelParameterField
                      definition={definition}
                      key={definition.id}
                      onChange={(value) => setParameterValues((current) => ({
                        ...current,
                        [definition.id]: value,
                      }))}
                      value={parameterValues[definition.id] ?? ""}
                    />
                  ))}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">{t("settings.modelOverrides.noParameters")}</p>
              )}
            </section>

            {error ? (
              <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive" role="alert">
                {error}
              </div>
            ) : null}
          </div>
        </ScrollArea>
        <DialogFooter className="m-0 rounded-none px-5 py-3">
          <Button variant="outline" onClick={() => props.onOpenChange(false)} type="button">
            {t("settings.modelOverrides.cancel")}
          </Button>
          <Button disabled={saving} onClick={() => void save()} type="button">
            {saving ? t("settings.modelOverrides.saving") : t("settings.modelOverrides.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ModelLimitField renders one direct-entry token limit without browser stepper controls.
// ModelLimitField 渲染一个不带浏览器步进器的直接输入 Token 限制。
function ModelLimitField(props: {
  // Provider-advertised maximum value.
  // 供应商声明的最大值。
  capabilityLimit?: number
  // Stable browser field identifier.
  // 稳定浏览器字段标识。
  id: string
  // User-visible field label.
  // 用户可见字段标签。
  label: string
  // Updates the direct-entry textual value.
  // 更新直接输入的文本值。
  onChange: (value: string) => void
  // Current direct-entry textual value.
  // 当前直接输入的文本值。
  value: string
}) {
  const { t } = useI18n()
  const declared = props.capabilityLimit !== undefined
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor={props.id}>{props.label}</Label>
        {declared ? (
          <Badge className="font-mono font-normal" variant="outline">
            {t("settings.modelOverrides.providerMaximum")} {props.capabilityLimit}
          </Badge>
        ) : null}
      </div>
      <Input
        disabled={!declared}
        id={props.id}
        inputMode="numeric"
        onChange={(event) => props.onChange(event.currentTarget.value)}
        pattern="[0-9]*"
        placeholder={declared
          ? t("settings.modelOverrides.leaveBlank")
          : t("settings.modelOverrides.providerNotDeclared")}
        type="text"
        value={props.value}
      />
      {!declared ? (
        <p className="text-xs text-muted-foreground">{t("settings.modelOverrides.providerNotDeclared")}</p>
      ) : null}
    </div>
  )
}

// applicableModelParameters returns only controls registered for the exact model protocol.
// applicableModelParameters 仅返回为精确模型协议注册的控件。
function applicableModelParameters(
  model: PublicAiModel | null,
  protocolId: string | null,
): PublicAiModel["capabilities"]["model_parameters"] {
  if (!model || !protocolId) {
    return []
  }
  return model.capabilities.model_parameters.filter((definition) => {
    const matchesProtocol = definition.protocol_ids.length === 0
      || definition.protocol_ids.includes(protocolId)
    const matchesModel = definition.model_ids.length === 0
      || definition.model_ids.includes(model.id)
    return matchesProtocol && matchesModel
  })
}

// protocolDisplayName maps exact protocol identifiers to user-visible labels.
// protocolDisplayName 把精确协议标识映射为用户可见标签。
function protocolDisplayName(protocolId: string | null): string {
  switch (protocolId) {
    case "anthropic-messages.v1":
      return "Anthropic Messages"
    case "anthropic-messages-video.v1":
      return "Anthropic Messages + Video"
    case "claude-code-messages.v1":
      return "Claude Code Messages"
    case "openai-chat-completions.v1":
      return "OpenAI Chat Completions"
    case "openai-codex-responses.v1":
      return "OpenAI Codex Responses"
    case "openai-responses.v1":
      return "OpenAI Responses"
    default:
      return protocolId ?? ""
  }
}

// ModelParameterField renders one scalar control exactly from the registered schema.
// ModelParameterField 严格根据已注册 Schema 渲染一个标量控件。
function ModelParameterField(props: {
  // Registered parameter definition.
  // 已注册参数定义。
  definition: PublicAiModel["capabilities"]["model_parameters"][number]
  // Current textual form.
  // 当前文本形式。
  value: string
  // Updates the textual form.
  // 更新文本形式。
  onChange: (value: string) => void
}) {
  const { t } = useI18n()
  const fieldId = React.useId()
  const schema = props.definition.schema
  if (schema.type === "boolean") {
    const selectedValue = props.value === "true"
      ? "enabled"
      : props.value === "false"
        ? "disabled"
        : "provider-default"
    return (
      <div className="space-y-2">
        <Label htmlFor={fieldId}>{props.definition.display_name}</Label>
        <Select
          onValueChange={(value) => {
            props.onChange(value === "enabled" ? "true" : value === "disabled" ? "false" : "")
          }}
          value={selectedValue}
        >
          <SelectTrigger className="w-full" id={fieldId}>
            <SelectValue placeholder={t("settings.modelOverrides.providerDefault")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="provider-default">{t("settings.modelOverrides.providerDefault")}</SelectItem>
            <SelectItem value="enabled">{t("settings.modelOverrides.enabled")}</SelectItem>
            <SelectItem value="disabled">{t("settings.modelOverrides.disabled")}</SelectItem>
          </SelectContent>
        </Select>
        <p className="text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
      </div>
    )
  }
  if (schema.type === "enum" && Array.isArray(schema.values)) {
    const values = schema.values as string[]
    const selectedIndex = values.indexOf(props.value)
    return (
      <div className="space-y-2">
        <Label htmlFor={fieldId}>{props.definition.display_name}</Label>
        <Select
          onValueChange={(value) => {
            if (value === "provider-default") {
              props.onChange("")
              return
            }
            const optionIndex = Number(value.slice("option-".length))
            props.onChange(values[optionIndex] ?? "")
          }}
          value={selectedIndex >= 0 ? `option-${selectedIndex}` : "provider-default"}
        >
          <SelectTrigger className="w-full" id={fieldId}>
            <SelectValue placeholder={t("settings.modelOverrides.providerDefault")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="provider-default">{t("settings.modelOverrides.providerDefault")}</SelectItem>
            {values.map((value, index) => (
              <SelectItem key={`${index}/${value}`} value={`option-${index}`}>{value}</SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
      </div>
    )
  }
  const supported = ["integer", "decimal", "string"].includes(schema.type)
  return (
    <div className="space-y-2">
      <Label htmlFor={fieldId}>{props.definition.display_name}</Label>
      <Input
        disabled={!supported}
        id={fieldId}
        inputMode={schema.type === "integer" ? "numeric" : schema.type === "decimal" ? "decimal" : "text"}
        onChange={(event) => props.onChange(event.currentTarget.value)}
        placeholder={supported
          ? t("settings.modelOverrides.leaveBlank")
          : t("settings.modelOverrides.unsupportedParameter")}
        type="text"
        value={props.value}
      />
      <p className="text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
    </div>
  )
}
