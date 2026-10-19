import * as React from "react"
import { Database, SlidersHorizontal } from "lucide-react"

import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import { Input } from "@/ui-kit/primitives/input"
import { Label } from "@/ui-kit/primitives/label"
import { ScrollArea } from "@/ui-kit/primitives/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { useI18n } from "@/i18n"
import { savePublicAiInternalModelOverrides } from "@/service/settings-actions-service"
import type {
  PublicAiEmbeddingCapabilities,
  PublicAiInternalInferenceCandidate,
  PublicAiInternalModelOverrides,
  PublicAiModelCapabilities,
  PublicAiModelParameterDefinition,
  PublicAiRerankCapabilities,
  PublicAiSettingsSnapshot,
} from "@/types"

const INHERIT_VALUE = "__inherit__"

// PublicAiInternalOverridesDialog edits operation defaults from capabilities common to every matching route.
// PublicAiInternalOverridesDialog 根据全部匹配路由共有的能力编辑操作默认值。
// Parameters: props identifies the internal operation and matching route capabilities; returns validated overrides with owner-stable drafts and guarded saves.
// 参数：props 标识内部操作及匹配路由能力；返回带稳定归属草稿与受保护保存的校验覆盖表单。
export function PublicAiInternalOverridesDialog(props: {
  // Exact route candidate whose model-level defaults are being edited.
  // 正在编辑模型级默认值的精确路由候选。
  candidate: PublicAiInternalInferenceCandidate | null
  // Every route sharing the persisted provider-model-operation override key.
  // 共享持久化供应商、模型、操作覆盖键的全部路由。
  matchingCandidates: PublicAiInternalInferenceCandidate[]
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
  const [draft, setDraft] = React.useState<PublicAiInternalModelOverrides>({})
  const [parameterValues, setParameterValues] = React.useState<Record<string, string>>({})
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)
  // savingRef protects the synchronous interval before disabled controls render.
  // savingRef 保护禁用控件渲染之前的同步时间窗口。
  const savingRef = React.useRef(false)
  // formRef allows invalid local numeric text to block submission without rewriting inherited values.
  // formRef 允许无效的本地数字文本阻止提交，而不改写继承值。
  const formRef = React.useRef<HTMLFormElement>(null)
  // initializedIdentity freezes the draft per provider/model/operation while the dialog remains open.
  // initializedIdentity 在弹窗打开期间按供应商、模型与操作冻结草稿归属。
  const initializedIdentity = React.useRef<string | null>(null)
  // baselineValue tracks the exact visible initial values for explicit dirty-state feedback.
  // baselineValue 跟踪精确可见初值，用于明确的未保存状态反馈。
  const [baselineValue, setBaselineValue] = React.useState("")
  const dirty = JSON.stringify([draft, parameterValues]) !== baselineValue
  const candidate = props.candidate
  const commonCandidate = React.useMemo(
    () => commonInternalInferenceCandidate(candidate, props.matchingCandidates),
    [candidate, props.matchingCandidates],
  )
  const definitions = React.useMemo(
    () => internalParameterDefinitions(commonCandidate),
    [commonCandidate],
  )

  React.useEffect(() => {
    if (!props.open) { initializedIdentity.current = null; return }
    if (!props.open || !candidate || !commonCandidate) {
      return
    }
    // identity matches the exact model-level backend override key, excluding credential routes.
    // identity 匹配精确的模型级后端覆盖键，不混入凭据路由。
    const identity = JSON.stringify([candidate.route.provider_id, candidate.route.model_id, candidate.route.operation])
    if (initializedIdentity.current === identity) return
    initializedIdentity.current = identity
    // VMM owns reasoning and embedding wire semantics at request time, so only deployment-level
    // defaults may survive through this settings surface.
    // VMM 在请求期拥有思考与向量 Wire 语义，因此只有部署级默认值可通过此设置界面保留。
    const initialDraft = {
      ...commonCandidate.overrides,
      reasoning_effort: undefined,
      embedding_task: undefined,
      embedding_output_kind: undefined,
      embedding_encoding: undefined,
    }
    const initialParameters = Object.fromEntries(definitions.map((definition) => {
      const current = commonCandidate.overrides.model_parameters?.[definition.id] as {
        value?: unknown
      } | undefined
      return [definition.id, current?.value === undefined ? "" : String(current.value)]
    }))
    setDraft(initialDraft)
    setParameterValues(initialParameters)
    setBaselineValue(JSON.stringify([initialDraft, initialParameters]))
    setError(null)
  }, [candidate, commonCandidate, definitions, props.open])

  // save converts scalar controls into the closed backend contract and persists one replacement.
  // save 把标量控件转换为封闭后端契约并持久化一次完整替换。
  const save = React.useCallback(async () => {
    if (!candidate || !commonCandidate || !dirty || savingRef.current || !formRef.current?.reportValidity()) {
      return
    }
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      // Saved discrete dimensions must remain inside the current common route contract.
      // 已保存离散维度必须仍属于当前公共路由契约。
      const dimensions = commonCandidate.capabilities.embedding?.dimensions ?? []
      if (draft.embedding_dimensions !== undefined && dimensions.length > 0 && !dimensions.includes(draft.embedding_dimensions)) {
        throw new Error(t("settings.modelExperience.invalidValue"))
      }
      const modelParameters: Record<string, unknown> = {}
      for (const definition of definitions) {
        const raw = parameterValues[definition.id]?.trim() ?? ""
        if (!raw || raw === INHERIT_VALUE) {
          continue
        }
        if ((definition.schema.type === "boolean" && raw !== "true" && raw !== "false")
          || (definition.schema.type === "enum" && !(definition.schema.values as string[]).includes(raw))) {
          throw new Error(`${definition.display_name}: ${t("settings.modelExperience.invalidValue")}`)
        }
        modelParameters[definition.id] = modelParameterValue(definition, raw)
      }
      const next = {
        ...draft,
        model_parameters: modelParameters,
      }
      props.onSaved(await savePublicAiInternalModelOverrides({
        providerId: candidate.route.provider_id,
        modelId: candidate.route.model_id,
        operation: candidate.route.operation,
        overrides: next,
      }))
      props.onOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [candidate, commonCandidate, definitions, dirty, draft, parameterValues, props, t])

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open && savingRef.current) {
          return
        }
        props.onOpenChange(open)
      }}
    >
      <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
        <DialogHeader className="shrink-0 border-b px-5 py-4 pr-12">
          <div className="flex items-start gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-xl border bg-background shadow-xs">
              <SlidersHorizontal className="size-4 text-muted-foreground" />
            </div>
            <div className="min-w-0 space-y-2">
              <div className="space-y-1">
                <DialogTitle>{t("settings.inference.overridesTitle")}</DialogTitle>
                <DialogDescription>{t("settings.inference.overridesDescription")}</DialogDescription>
              </div>
              <div className="flex flex-wrap gap-1.5">
                <Badge variant="outline">{candidate?.providerDisplayName}</Badge>
                <Badge variant="secondary">{candidate?.modelDisplayName}</Badge>
                <Badge variant="outline">{operationLabel(candidate?.route.operation, t)}</Badge>
              </div>
            </div>
          </div>
        </DialogHeader>
        <form ref={formRef} className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => { event.preventDefault(); void save() }}>
        <ScrollArea className="min-h-0 flex-1">
          <fieldset disabled={saving} className="min-w-0 space-y-4 p-5">
            {commonCandidate?.route.operation === "conversation_respond" ? (
              <LlmDefaults candidate={commonCandidate} draft={draft} onChange={setDraft} />
            ) : null}
            {commonCandidate?.route.operation === "embedding_create" ? (
              <EmbeddingDefaults
                candidate={commonCandidate}
                draft={draft}
                matchingCandidates={props.matchingCandidates}
                onChange={setDraft}
              />
            ) : null}
            {commonCandidate?.route.operation === "rerank_documents" ? (
              <RerankDefaults candidate={commonCandidate} draft={draft} onChange={setDraft} />
            ) : null}
            {definitions.length > 0 ? (
              <section className="space-y-4 border-t pt-4">
                <div className="flex items-start gap-2.5">
                  <Database className="mt-0.5 size-4 text-muted-foreground" />
                  <div>
                    <h3 className="text-sm font-semibold">{t("settings.modelOverrides.parameters")}</h3>
                    <p className="text-xs text-muted-foreground">
                      {t("settings.modelOverrides.parametersDescription")}
                    </p>
                  </div>
                </div>
                {definitions.map((definition) => (
                  <InternalParameterField
                    definition={definition}
                    key={definition.id}
                    onChange={(value) => setParameterValues((current) => ({
                      ...current,
                      [definition.id]: value,
                    }))}
                    value={parameterValues[definition.id] ?? ""}
                  />
                ))}
              </section>
            ) : null}
            {error ? (
              <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive" role="alert">
                {error}
              </div>
            ) : null}
          </fieldset>
        </ScrollArea>
        <DialogFooter className="m-0 shrink-0 rounded-none px-5 py-3">
          <Button disabled={saving} variant="outline" onClick={() => props.onOpenChange(false)} type="button">
            {t("settings.modelOverrides.cancel")}
          </Button>
          <Button disabled={saving || !commonCandidate || !dirty} type="submit">
            {saving ? t("settings.modelOverrides.saving") : t("settings.modelOverrides.save")}
          </Button>
        </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

// LlmDefaults renders only VMM-relevant restricted LLM limits.
// LlmDefaults 仅渲染与 VMM 有关的受限 LLM 限制。
function LlmDefaults(props: InternalDefaultsProps) {
  const { t } = useI18n()
  const capabilities = props.candidate.capabilities
  return (
    <section className="space-y-4 rounded-xl border bg-card/40 p-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <InternalLimitField
          capabilityLimit={capabilities.context_limit}
          id="internal-context-limit"
          label={t("settings.modelOverrides.contextLimit")}
          onChange={(value) => props.onChange({ ...props.draft, context_limit: value })}
          value={props.draft.context_limit}
        />
        <InternalLimitField
          capabilityLimit={capabilities.output_limit}
          id="internal-output-limit"
          label={t("settings.modelOverrides.outputLimit")}
          onChange={(value) => props.onChange({ ...props.draft, output_limit: value })}
          value={props.draft.output_limit}
        />
      </div>
    </section>
  )
}

// EmbeddingDefaults renders the sole deployment-level VMM embedding choice.
// EmbeddingDefaults 渲染 VMM 唯一的部署级向量选项。
function EmbeddingDefaults(props: InternalDefaultsProps) {
  const capability = props.candidate.capabilities.embedding
  if (!capability) {
    return null
  }
  return (
    <section className="rounded-xl border bg-card/40 p-4">
      <div className="max-w-xs">
        <InternalDimensionField
          candidate={props.candidate}
          hasDiscreteConstraint={props.matchingCandidates?.some((candidate) => (
            (candidate.capabilities.embedding?.dimensions.length ?? 0) > 0
          )) ?? false}
          onChange={(value) => props.onChange({ ...props.draft, embedding_dimensions: value })}
          value={props.draft.embedding_dimensions}
        />
      </div>
    </section>
  )
}

// RerankDefaults renders exact rerank defaults without inventing score normalization.
// RerankDefaults 渲染精确重排默认值且不制造分数归一化。
function RerankDefaults(props: InternalDefaultsProps) {
  const { t } = useI18n()
  const capability = props.candidate.capabilities.rerank
  if (!capability) {
    return null
  }
  return (
    <section className="space-y-4 rounded-xl border bg-card/40 p-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <InternalLimitField
          capabilityLimit={capability.maximum_candidates}
          id="internal-rerank-top-n"
          label={t("settings.inference.rerankTopN")}
          onChange={(value) => props.onChange({ ...props.draft, rerank_top_n: value })}
          value={props.draft.rerank_top_n}
        />
        <InternalEnumField
          label={t("settings.inference.rerankTruncation")}
          onChange={(value) => props.onChange({ ...props.draft, rerank_truncation: value })}
          options={capability.truncation}
          value={props.draft.rerank_truncation}
        />
      </div>
      <div className="flex items-center justify-between gap-3 rounded-lg border px-3 py-2">
        <Label htmlFor="internal-rerank-return-content">
          {t("settings.inference.rerankReturnContent")}
        </Label>
        <Switch
          checked={props.draft.rerank_return_content ?? false}
          disabled={!capability.supports_return_content}
          id="internal-rerank-return-content"
          onCheckedChange={(value) => props.onChange({
            ...props.draft,
            rerank_return_content: value,
          })}
        />
      </div>
    </section>
  )
}

// InternalDefaultsProps shares exact candidate and immutable replacement semantics.
// InternalDefaultsProps 共享精确候选与不可变替换语义。
interface InternalDefaultsProps {
  candidate: PublicAiInternalInferenceCandidate
  draft: PublicAiInternalModelOverrides
  matchingCandidates?: PublicAiInternalInferenceCandidate[]
  onChange: (draft: PublicAiInternalModelOverrides) => void
}

// InternalLimitField renders one direct positive integer narrowing field.
// InternalLimitField 渲染一个直接输入的正整数收窄字段。
// Parameters: props supplies an optional numeric limit and capability bounds; returns an integer narrowing input preserving inherited and invalid states.
// 参数：props 提供可选数值限制与能力边界；返回保留继承和非法状态的整数收窄输入。
function InternalLimitField(props: {
  capabilityLimit?: number
  capabilityMinimum?: number
  disabled?: boolean
  id: string
  label: string
  onChange: (value: number | undefined) => void
  value?: number
}) {
  const { t } = useI18n()
  // rawValue preserves invalid user text instead of silently replacing it with inheritance.
  // rawValue 保留无效用户文本，不静默替换为继承。
  const [rawValue, setRawValue] = React.useState(props.value?.toString() ?? "")
  React.useEffect(() => { setRawValue(props.value?.toString() ?? "") }, [props.value])
  // invalid covers safe integers and the exact declared narrowing interval.
  // invalid 覆盖安全整数及精确声明的收窄范围。
  const numericValue = Number(rawValue)
  const invalid = rawValue.trim() !== "" && (!Number.isSafeInteger(numericValue) || numericValue <= 0
    || (props.capabilityMinimum !== undefined && numericValue < props.capabilityMinimum)
    || (props.capabilityLimit !== undefined && numericValue > props.capabilityLimit))
  // inputRef synchronizes native validity after an external reset as well as direct typing.
  // inputRef 在外部重置和直接输入后同步原生有效性。
  const inputRef = React.useRef<HTMLInputElement>(null)
  React.useEffect(() => { inputRef.current?.setCustomValidity(invalid ? t("settings.modelExperience.invalidValue") : "") }, [invalid, t])
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor={props.id}>{props.label}</Label>
        {props.capabilityMinimum !== undefined || props.capabilityLimit !== undefined ? (
          <Badge className="font-mono font-normal" variant="outline">
            {limitRangeLabel(props.capabilityMinimum, props.capabilityLimit)}
          </Badge>
        ) : null}
      </div>
      <Input
        ref={inputRef}
        disabled={props.disabled ?? props.capabilityLimit === undefined}
        id={props.id}
        inputMode="numeric"
        onChange={(event) => {
          const raw = event.currentTarget.value.trim()
          setRawValue(event.currentTarget.value)
          if (!raw) {
            event.currentTarget.setCustomValidity("")
            props.onChange(undefined)
            return
          }
          const value = Number(raw)
          const valid = Number.isSafeInteger(value) && value > 0
            && (props.capabilityMinimum === undefined || value >= props.capabilityMinimum)
            && (props.capabilityLimit === undefined || value <= props.capabilityLimit)
          event.currentTarget.setCustomValidity(valid ? "" : t("settings.modelExperience.invalidValue"))
          if (valid) props.onChange(value)
        }}
        pattern="[0-9]*"
        placeholder={t("settings.inference.inheritModelDefault")}
        type="text"
        value={rawValue}
        aria-invalid={invalid}
      />
      {invalid && <p role="alert" className="text-xs text-destructive">{t("settings.modelExperience.invalidValue")}</p>}
    </div>
  )
}

// InternalEnumField renders one closed capability-backed optional value.
// InternalEnumField 渲染一个由封闭能力支持的可选值。
// Parameters: props supplies a closed option set and retained value; returns a selector that visibly preserves unsupported saved values.
// 参数：props 提供封闭选项集与保留值；返回显式保留不支持已存值的选择器。
function InternalEnumField(props: {
  label: string
  onChange: (value: string | undefined) => void
  options: string[]
  value?: string
}) {
  const { t } = useI18n()
  return (
    <div className="space-y-2">
      <Label>{props.label}</Label>
      <Select
        onValueChange={(value) => props.onChange(value === INHERIT_VALUE ? undefined : value)}
        value={props.value ?? INHERIT_VALUE}
      >
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value={INHERIT_VALUE}>{t("settings.inference.inheritModelDefault")}</SelectItem>
          {props.value !== undefined && !props.options.includes(props.value) ? <SelectItem disabled value={props.value}>{t("settings.modelExperience.invalidValue")}: {props.value}</SelectItem> : null}
          {props.options.map((option) => (
            <SelectItem key={option} value={option}>{option}</SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

// InternalDimensionField renders discrete dimensions or one bounded direct input.
// InternalDimensionField 渲染离散维度或一个有边界的直接输入。
// Parameters: props supplies declared dimension options/range and draft; returns the matching discrete selector or bounded numeric input.
// 参数：props 提供声明维度选项或范围与草稿；返回对应离散选择器或有界数值输入。
function InternalDimensionField(props: {
  candidate: PublicAiInternalInferenceCandidate
  hasDiscreteConstraint: boolean
  onChange: (value: number | undefined) => void
  value?: number
}) {
  const { t } = useI18n()
  const capability = props.candidate.capabilities.embedding
  if (!capability) {
    return null
  }
  if (capability.dimensions.length > 0) {
    return (
      <div className="space-y-2">
        <Label>{t("settings.inference.embeddingDimensions")}</Label>
        <Select
          onValueChange={(value) => props.onChange(
            value === INHERIT_VALUE ? undefined : Number(value),
          )}
          value={props.value?.toString() ?? INHERIT_VALUE}
        >
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value={INHERIT_VALUE}>{t("settings.inference.inheritModelDefault")}</SelectItem>
            {props.value !== undefined && !capability.dimensions.includes(props.value) ? <SelectItem disabled value={String(props.value)}>{t("settings.modelExperience.invalidValue")}: {props.value}</SelectItem> : null}
            {capability.dimensions.map((dimension) => (
              <SelectItem key={dimension} value={dimension.toString()}>{dimension}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    )
  }
  if (props.hasDiscreteConstraint) {
    return (
      <InternalLimitField
        disabled
        id="internal-embedding-dimensions"
        label={t("settings.inference.embeddingDimensions")}
        onChange={props.onChange}
        value={undefined}
      />
    )
  }
  return (
    <InternalLimitField
      capabilityLimit={capability.maximum_dimensions}
      capabilityMinimum={capability.minimum_dimensions}
      id="internal-embedding-dimensions"
      label={t("settings.inference.embeddingDimensions")}
      onChange={props.onChange}
      value={props.value}
    />
  )
}

// InternalParameterField renders one scalar adapter-registered parameter.
// InternalParameterField 渲染一个由适配器注册的标量参数。
// Parameters: props supplies an adapter-declared scalar parameter and draft value; returns its typed field without coercing invalid saved data.
// 参数：props 提供适配器声明的标量参数及草稿值；返回不强制转换非法已存数据的类型化控件。
function InternalParameterField(props: {
  definition: PublicAiModelParameterDefinition
  onChange: (value: string) => void
  value: string
}) {
  const { t } = useI18n()
  if (props.definition.schema.type === "boolean") {
    return (
      <div className="space-y-2">
        <Label>{props.definition.display_name}</Label>
        <Select onValueChange={props.onChange} value={props.value || INHERIT_VALUE}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value={INHERIT_VALUE}>
              {t("settings.inference.inheritModelDefault")}
            </SelectItem>
            <SelectItem value="true">{t("settings.modelOverrides.enabled")}</SelectItem>
            <SelectItem value="false">{t("settings.modelOverrides.disabled")}</SelectItem>
            {props.value && props.value !== INHERIT_VALUE && props.value !== "true" && props.value !== "false" ? <SelectItem disabled value={props.value}>{t("settings.modelExperience.invalidValue")}: {props.value}</SelectItem> : null}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">{props.definition.description}</p>
      </div>
    )
  }
  const enumValues = props.definition.schema.type === "enum"
    ? (props.definition.schema.values as string[] | undefined) ?? []
    : []
  if (enumValues.length > 0) {
    return (
      <div className="space-y-2">
        <Label>{props.definition.display_name}</Label>
        <Select onValueChange={props.onChange} value={props.value || INHERIT_VALUE}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value={INHERIT_VALUE}>
              {t("settings.inference.inheritModelDefault")}
            </SelectItem>
            {enumValues.map((value) => (
              <SelectItem key={value} value={value}>{value}</SelectItem>
            ))}
            {props.value && props.value !== INHERIT_VALUE && !enumValues.includes(props.value) ? <SelectItem disabled value={props.value}>{t("settings.modelExperience.invalidValue")}: {props.value}</SelectItem> : null}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">{props.definition.description}</p>
      </div>
    )
  }
  return (
    <div className="space-y-2">
      <Label>{props.definition.display_name}</Label>
      <Input
        onChange={(event) => props.onChange(event.currentTarget.value)}
        type="text"
        value={props.value}
      />
      <p className="text-xs text-muted-foreground">{props.definition.description}</p>
    </div>
  )
}

// internalParameterDefinitions filters controls for the exact selected binding.
// internalParameterDefinitions 为精确所选绑定过滤控件。
function internalParameterDefinitions(
  candidate: PublicAiInternalInferenceCandidate | null,
): PublicAiModelParameterDefinition[] {
  if (!candidate) {
    return []
  }
  const protocolId = candidate.route.binding.kind === "protocol"
    ? candidate.route.binding.protocol_profile_id
    : null
  return candidate.capabilities.model_parameters.filter((definition) => (
    (definition.model_ids.length === 0 || definition.model_ids.includes(candidate.route.model_id))
    && (definition.protocol_ids.length === 0
      || (protocolId !== null && definition.protocol_ids.includes(protocolId)))
  ))
}

// commonInternalInferenceCandidate narrows controls to values accepted by every persisted-key route.
// commonInternalInferenceCandidate 将控件收窄为同一持久化键全部路由都接受的值。
function commonInternalInferenceCandidate(
  selected: PublicAiInternalInferenceCandidate | null,
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate | null {
  if (!selected) {
    return null
  }
  const matching = candidates.filter((candidate) => (
    candidate.route.provider_id === selected.route.provider_id
    && candidate.route.model_id === selected.route.model_id
    && candidate.route.operation === selected.route.operation
  ))
  const complete = matching.length > 0 ? matching : [selected]
  return {
    ...selected,
    capabilities: intersectInternalCapabilities(complete),
  }
}

// intersectInternalCapabilities computes the strict frontend contract matching backend all-route validation.
// intersectInternalCapabilities 计算与后端全路由校验一致的严格前端契约。
function intersectInternalCapabilities(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiModelCapabilities {
  const capabilities = candidates.map((candidate) => candidate.capabilities)
  const representative = capabilities[0]
  if (!representative) {
    throw new Error("Internal inference capability intersection requires at least one candidate.")
  }
  const reasoningControls = new Set(capabilities.map((capability) => (
    capability.reasoning_control ?? "unknown"
  )))
  const reasoningControl = reasoningControls.size === 1
    ? (representative.reasoning_control ?? "unknown")
    : "unknown"
  return {
    ...representative,
    context_limit: commonRequiredMaximum(capabilities.map((capability) => capability.context_limit)),
    output_limit: commonRequiredMaximum(capabilities.map((capability) => capability.output_limit)),
    reasoning_control: reasoningControl,
    reasoning_efforts: intersectStringLists(capabilities.map(validReasoningValues)),
    model_parameters: intersectParameterDefinitions(candidates),
    embedding: intersectEmbeddingCapabilities(capabilities),
    rerank: intersectRerankCapabilities(capabilities),
  }
}

// validReasoningValues returns explicit reasoning values accepted by one exact route.
// validReasoningValues 返回一条精确路由接受的显式思考值。
function validReasoningValues(capability: PublicAiModelCapabilities): string[] {
  const efforts = capability.reasoning_efforts ?? []
  switch (capability.reasoning_control ?? "unknown") {
    case "fixed_off":
      return ["none"]
    case "fixed_on":
      return efforts.filter((effort) => effort !== "none")
    case "toggle":
    case "effort_selection":
    case "provider_adaptive":
      return efforts
    case "unknown":
    default:
      return []
  }
}

// commonRequiredMaximum returns a usable maximum only when every route declares one.
// commonRequiredMaximum 仅在每条路由都声明上限时返回可用公共上限。
function commonRequiredMaximum(values: Array<number | undefined>): number | undefined {
  if (values.some((value) => value === undefined)) {
    return undefined
  }
  return Math.min(...values as number[])
}

// intersectParameterDefinitions keeps only identical definitions applicable to every exact route.
// intersectParameterDefinitions 仅保留适用于每条精确路由且定义完全一致的参数。
function intersectParameterDefinitions(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiModelParameterDefinition[] {
  const applicable = candidates.map((candidate) => internalParameterDefinitions(candidate))
  const first = applicable[0] ?? []
  return first.filter((definition) => applicable.every((definitions) => (
    definitions.some((candidate) => (
      candidate.id === definition.id
      && JSON.stringify(candidate) === JSON.stringify(definition)
    ))
  )))
}

// intersectEmbeddingCapabilities computes values and bounds accepted by every embedding route.
// intersectEmbeddingCapabilities 计算每条向量路由都接受的值与边界。
function intersectEmbeddingCapabilities(
  capabilities: PublicAiModelCapabilities[],
): PublicAiEmbeddingCapabilities | undefined {
  const embeddings = capabilities.map((capability) => capability.embedding)
  if (embeddings.some((embedding) => embedding === undefined)) {
    return undefined
  }
  const exact = embeddings as PublicAiEmbeddingCapabilities[]
  const finiteDimensions = exact.find((embedding) => embedding.dimensions.length > 0)?.dimensions
  const minimum = maximumDefined(exact.map((embedding) => embedding.minimum_dimensions))
  const maximum = minimumDefined(exact.map((embedding) => embedding.maximum_dimensions))
  const dimensions = finiteDimensions
    ? finiteDimensions.filter((dimension) => exact.every((embedding) => (
      (embedding.dimensions.length === 0 || embedding.dimensions.includes(dimension))
      && (embedding.minimum_dimensions === undefined || dimension >= embedding.minimum_dimensions)
      && (embedding.maximum_dimensions === undefined || dimension <= embedding.maximum_dimensions)
    )))
    : []
  return {
    ...exact[0],
    tasks: intersectStringLists(exact.map((embedding) => embedding.tasks)),
    output_kinds: intersectStringLists(exact.map((embedding) => embedding.output_kinds)),
    encodings: intersectStringLists(exact.map((embedding) => embedding.encodings)),
    dimensions,
    minimum_dimensions: minimum,
    maximum_dimensions: maximum,
    maximum_inputs: minimumDefined(exact.map((embedding) => embedding.maximum_inputs)),
    maximum_tokens_per_input: minimumDefined(
      exact.map((embedding) => embedding.maximum_tokens_per_input),
    ),
  }
}

// intersectRerankCapabilities computes values and bounds accepted by every rerank route.
// intersectRerankCapabilities 计算每条重排路由都接受的值与边界。
function intersectRerankCapabilities(
  capabilities: PublicAiModelCapabilities[],
): PublicAiRerankCapabilities | undefined {
  const reranks = capabilities.map((capability) => capability.rerank)
  if (reranks.some((rerank) => rerank === undefined)) {
    return undefined
  }
  const exact = reranks as PublicAiRerankCapabilities[]
  return {
    ...exact[0],
    maximum_candidates: minimumDefined(exact.map((rerank) => rerank.maximum_candidates)),
    maximum_tokens_per_item: minimumDefined(
      exact.map((rerank) => rerank.maximum_tokens_per_item),
    ),
    maximum_request_tokens: minimumDefined(
      exact.map((rerank) => rerank.maximum_request_tokens),
    ),
    truncation: intersectStringLists(exact.map((rerank) => rerank.truncation)),
    supports_return_content: exact.every((rerank) => rerank.supports_return_content),
  }
}

// intersectStringLists preserves first-route order while retaining values shared by every route.
// intersectStringLists 保留首条路由顺序并仅留下所有路由共有的值。
function intersectStringLists(values: string[][]): string[] {
  const first = values[0] ?? []
  return first.filter((value) => values.every((candidates) => candidates.includes(value)))
}

// minimumDefined returns the strictest declared upper bound while treating omission as unbounded.
// minimumDefined 返回最严格的已声明上限，并把省略视为无上限。
function minimumDefined(values: Array<number | undefined>): number | undefined {
  const defined = values.filter((value): value is number => value !== undefined)
  return defined.length > 0 ? Math.min(...defined) : undefined
}

// maximumDefined returns the strictest declared lower bound while treating omission as unbounded.
// maximumDefined 返回最严格的已声明下限，并把省略视为无下限。
function maximumDefined(values: Array<number | undefined>): number | undefined {
  const defined = values.filter((value): value is number => value !== undefined)
  return defined.length > 0 ? Math.max(...defined) : undefined
}

// limitRangeLabel formats an exact supported integer interval without adding hidden clamping.
// limitRangeLabel 格式化精确支持的整数区间且不增加隐藏钳制。
function limitRangeLabel(minimum?: number, maximum?: number): string {
  if (minimum !== undefined && maximum !== undefined) {
    return `${minimum}–${maximum}`
  }
  if (minimum !== undefined) {
    return `≥ ${minimum}`
  }
  return `≤ ${maximum}`
}

// modelParameterValue converts one scalar UI value into the closed tagged Rust enum.
// modelParameterValue 把一个标量 UI 值转换为封闭的带标签 Rust 枚举。
function modelParameterValue(
  definition: PublicAiModelParameterDefinition,
  raw: string,
): unknown {
  switch (definition.schema.type) {
    case "boolean":
      return { type: "boolean", value: raw === "true" }
    case "integer": {
      const value = Number(raw)
      if (!Number.isSafeInteger(value)) {
        throw new Error(`${definition.display_name}: invalid integer`)
      }
      return { type: "integer", value }
    }
    case "decimal":
      return { type: "decimal", value: raw }
    case "string":
    case "enum":
      return { type: "string", value: raw === INHERIT_VALUE ? "" : raw }
    default:
      throw new Error(`${definition.display_name}: unsupported complex parameter`)
  }
}

// operationLabel maps the closed operation union to a localized title.
// operationLabel 把封闭操作联合映射为本地化标题。
function operationLabel(
  operation: PublicAiInternalInferenceCandidate["route"]["operation"] | undefined,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (operation) {
    case "conversation_respond":
      return t("settings.inference.llm")
    case "embedding_create":
      return t("settings.inference.embedding")
    case "rerank_documents":
      return t("settings.inference.rerank")
    default:
      return ""
  }
}
