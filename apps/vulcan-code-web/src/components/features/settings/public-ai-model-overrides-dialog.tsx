import * as React from "react"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
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
              throw new Error(`${definition.display_name} 必须是安全整数。`)
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
          throw new Error(`${label}必须是正整数。`)
        }
        return parsed
      }
      const overrides: PublicAiModelOverrides = {
        context_limit: parseLimit(contextLimit, "上下文限制"),
        output_limit: parseLimit(outputLimit, "输出限制"),
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
  ])

  const promptCache = props.model?.capabilities.prompt_cache
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>模型附加条件</DialogTitle>
          <DialogDescription>
            {props.providerDisplayName} · {props.model?.display_name} · {protocolDisplayName(props.protocolId)}
          </DialogDescription>
        </DialogHeader>
        <label className="settings-field">
          <span>上下文限制</span>
          <input
            max={props.model?.capabilities.context_limit}
            min={1}
            onChange={(event) => setContextLimit(event.target.value)}
            placeholder={props.model?.capabilities.context_limit?.toString() ?? "供应商未声明"}
            type="number"
            value={contextLimit}
          />
        </label>
        <label className="settings-field">
          <span>输出 Token 限制</span>
          <input
            max={props.model?.capabilities.output_limit}
            min={1}
            onChange={(event) => setOutputLimit(event.target.value)}
            placeholder={props.model?.capabilities.output_limit?.toString() ?? "供应商未声明"}
            type="number"
            value={outputLimit}
          />
        </label>
        <label className="settings-field">
          <span>提示词缓存</span>
          <select onChange={(event) => setCacheMode(event.target.value)} value={cacheMode}>
            <option value="provider_default">供应商默认，不额外注入</option>
            {promptCache?.supports_managed_explicit ? (
              <option value="managed_explicit">Vulcan 显式管理</option>
            ) : null}
          </select>
          <small>{promptCache?.provider_default_behavior}</small>
        </label>
        {cacheMode === "managed_explicit" ? (
          <label className="settings-field">
            <span>缓存 TTL</span>
            <select onChange={(event) => setCacheTtl(event.target.value)} value={cacheTtl}>
              {promptCache?.supported_ttl_options.map((option) => (
                <option key={option.id} value={option.id}>{option.display_name}</option>
              ))}
            </select>
          </label>
        ) : null}
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
        {error ? <p className="settings-error-text">{error}</p> : null}
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">
            取消
          </button>
          <button className="skill-action-button" disabled={saving} onClick={() => void save()} type="button">
            {saving ? "保存中…" : "保存"}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
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
  const schema = props.definition.schema
  if (schema.type === "boolean") {
    return (
      <label className="settings-field">
        <span>{props.definition.display_name}</span>
        <select onChange={(event) => props.onChange(event.target.value)} value={props.value}>
          <option value="">供应商默认</option>
          <option value="true">启用</option>
          <option value="false">停用</option>
        </select>
        <small>{props.definition.description}</small>
      </label>
    )
  }
  if (schema.type === "enum" && Array.isArray(schema.values)) {
    return (
      <label className="settings-field">
        <span>{props.definition.display_name}</span>
        <select onChange={(event) => props.onChange(event.target.value)} value={props.value}>
          <option value="">供应商默认</option>
          {(schema.values as string[]).map((value) => <option key={value} value={value}>{value}</option>)}
        </select>
        <small>{props.definition.description}</small>
      </label>
    )
  }
  const supported = ["integer", "decimal", "string"].includes(schema.type)
  return (
    <label className="settings-field">
      <span>{props.definition.display_name}</span>
      <input
        disabled={!supported}
        onChange={(event) => props.onChange(event.target.value)}
        placeholder={supported ? "留空使用供应商默认值" : "该复杂参数尚未注册编辑器"}
        type={schema.type === "integer" || schema.type === "decimal" ? "number" : "text"}
        value={props.value}
      />
      <small>{props.definition.description}</small>
    </label>
  )
}
