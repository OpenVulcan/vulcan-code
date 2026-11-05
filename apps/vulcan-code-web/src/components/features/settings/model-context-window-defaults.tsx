import * as React from "react"
import { toast } from "@/ui-kit/primitives/toast"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { CustomContextWindowEditor } from "@/components/shared/custom-context-window-editor"
import { useI18n } from "@/i18n"
import {
  contextWindowFitsModel,
  offeredContextWindowNodes,
  contextWindowSelectionTokens,
  formatContextWindowTokens,
  formatCustomContextWindowInput,
  validateCustomContextWindowInput,
  type CustomContextWindowInputError,
} from "@/lib/context-window-selection"
import { savePublicAiModelContextWindowDefault } from "@/service/settings-actions-service"
import type {
  ContextCompactionWindowNodeId,
  ContextWindowSelection,
  PublicAiContextCompactionWindowConfiguration,
  PublicAiModelContextWindowCandidate,
  PublicAiModelContextWindowDefault,
  PublicAiSettingsSnapshot,
} from "@/types"

// ModelDefaultChoice is the saved state one row shows: no entry, follow the model, or one selection.
// ModelDefaultChoice 是一行显示的已保存状态：无条目、跟随模型或一个选择。
type ModelDefaultChoice =
  | { kind: "inherit" }
  | { kind: "follow-model" }
  | { kind: "selection"; selection: ContextWindowSelection }

// ModelContextWindowDefaultsList lets the user pick a new-session window default for each enabled conversation model.
// ModelContextWindowDefaultsList 让用户为每个已启用会话模型选择新会话窗口默认值。
// Parameters: props carry the backend-projected candidates, saved defaults, node values and snapshot callback.
// 参数：props 提供后端投影的候选模型、已保存默认值、节点数值与快照回调。
export function ModelContextWindowDefaultsList(props: {
  candidates: PublicAiModelContextWindowCandidate[]
  defaults: PublicAiModelContextWindowDefault[]
  nodes: PublicAiContextCompactionWindowConfiguration
  customMinTokens: number
  nodeLabel: (id: ContextCompactionWindowNodeId) => string
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { t } = useI18n()
  const [filter, setFilter] = React.useState("")
  // visible keeps candidates whose name, model id or provider id contains the filter text.
  // visible 保留名称、模型标识或供应商标识包含筛选文本的候选模型。
  const visible = React.useMemo(() => {
    const needle = filter.trim().toLowerCase()
    if (!needle) return props.candidates
    return props.candidates.filter((candidate) =>
      [candidate.displayName, candidate.modelId, candidate.providerId]
        .some((value) => value.toLowerCase().includes(needle)))
  }, [filter, props.candidates])

  return (
    <div className="vk-form-prefs">
      <div className="vk-form-pref">
        <span className="vk-form-pref-text">
          <strong>{t("settings.modelContextWindowDefaults.title")}</strong>
          <small className="vk-form-note">{t("settings.modelContextWindowDefaults.description")}</small>
        </span>
        <Input
          aria-label={t("settings.modelContextWindowDefaults.filter")}
          className="vk-form-w-lg"
          onChange={(event) => setFilter(event.target.value)}
          placeholder={t("settings.modelContextWindowDefaults.filter")}
          type="search"
          value={filter}
        />
      </div>
      {props.candidates.length === 0
        ? <p className="vk-form-note">{t("settings.modelContextWindowDefaults.empty")}</p>
        : visible.length === 0
          ? <p className="vk-form-note">{t("settings.modelContextWindowDefaults.noMatch")}</p>
          : visible.map((candidate) => (
            <ModelContextWindowDefaultRow
              candidate={candidate}
              customMinTokens={props.customMinTokens}
              key={`${candidate.providerId}\u0000${candidate.modelId}`}
              nodeLabel={props.nodeLabel}
              nodes={props.nodes}
              onSaved={props.onSaved}
              saved={props.defaults.find((entry) =>
                entry.providerId === candidate.providerId && entry.modelId === candidate.modelId) ?? null}
            />
          ))}
    </div>
  )
}

// ModelContextWindowDefaultRow saves one model's default as soon as a fixed choice is picked; custom values save on Apply.
// ModelContextWindowDefaultRow 选择固定项后立即保存一个模型的默认值；自定义值在点击应用时保存。
function ModelContextWindowDefaultRow(props: {
  candidate: PublicAiModelContextWindowCandidate
  saved: PublicAiModelContextWindowDefault | null
  nodes: PublicAiContextCompactionWindowConfiguration
  customMinTokens: number
  nodeLabel: (id: ContextCompactionWindowNodeId) => string
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { t } = useI18n()
  const limit = props.candidate.contextLimit
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [customEditing, setCustomEditing] = React.useState(false)
  const [customText, setCustomText] = React.useState("")
  const [customError, setCustomError] = React.useState<CustomContextWindowInputError | null>(null)
  const choice: ModelDefaultChoice = props.saved === null
    ? { kind: "inherit" }
    : props.saved.selection === null
      ? { kind: "follow-model" }
      : { kind: "selection", selection: props.saved.selection }
  const savedSelection = choice.kind === "selection" ? choice.selection : null
  const savedTokens = contextWindowSelectionTokens(savedSelection, props.nodes.nodes)
  // savedIsInvalid flags a stored value that no longer fits the model; the backend then follows the model.
  // savedIsInvalid 标记不再适配模型的已保存值；此时后端改为跟随模型。
  const savedIsInvalid = savedTokens !== null && (limit === null || !contextWindowFitsModel(savedTokens, limit))
  const fittingNodes = offeredContextWindowNodes(
    props.nodes.nodes,
    limit,
    savedSelection && "node" in savedSelection ? savedSelection.node : null,
  )
  const customAvailable = limit !== null && contextWindowFitsModel(props.customMinTokens, limit)
  const selectValue = customEditing
    ? "custom"
    : choice.kind !== "selection"
      ? choice.kind
      : "custom" in choice.selection
        ? "custom"
        : choice.selection.node

  // save writes one default and adopts the backend-authoritative snapshot.
  // save 写入一个默认值并采用后端权威快照。
  const save = async (next: { selection: ContextWindowSelection | null } | null): Promise<boolean> => {
    if (saving) return false
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiModelContextWindowDefault(props.candidate.providerId, props.candidate.modelId, next))
      toast.success(t("settings.contextCompactionNodes.defaultSaved"))
      return true
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      return false
    } finally {
      setSaving(false)
    }
  }
  const handleValue = (value: string): void => {
    if (value === "custom") {
      setCustomText(savedSelection && "custom" in savedSelection ? formatCustomContextWindowInput(savedSelection.custom) : "")
      setCustomError(null)
      setCustomEditing(true)
      return
    }
    setCustomEditing(false)
    if (value === "inherit") void save(null)
    else if (value === "follow-model") void save({ selection: null })
    else {
      const node = props.nodes.nodes.find((candidate) => candidate.id === value)
      if (node) void save({ selection: { node: node.id } })
    }
  }
  const submitCustom = (): void => {
    if (limit === null) return
    const result = validateCustomContextWindowInput(customText, props.customMinTokens, limit)
    if ("error" in result) {
      setCustomError(result.error)
      return
    }
    void save({ selection: { custom: result.tokens } }).then((saved) => {
      if (saved) setCustomEditing(false)
    })
  }

  return (
    <div className="vk-form-pref">
      <span className="vk-form-pref-text">
        <strong>{props.candidate.displayName}</strong>
        <small className="vk-form-note">
          {props.candidate.providerId} · {props.candidate.modelId} · {limit === null
            ? t("settings.modelContextWindowDefaults.limitUnknown")
            : t("settings.modelContextWindowDefaults.limit").replace("{limit}", formatContextWindowTokens(limit))}
        </small>
      </span>
      <Select disabled={saving} onValueChange={handleValue} value={selectValue}>
        <SelectTrigger aria-label={props.candidate.displayName} className="vk-form-w-lg"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="inherit">{t("settings.modelContextWindowDefaults.inherit")}</SelectItem>
          <SelectItem value="follow-model">{t("settings.contextCompactionNodes.followModel")}</SelectItem>
          {fittingNodes.map((node) => (
            <SelectItem key={node.id} value={node.id}>
              {props.nodeLabel(node.id)} · {formatContextWindowTokens(node.tokens)}
            </SelectItem>
          ))}
          {savedSelection && "node" in savedSelection && savedIsInvalid ? (
            <SelectItem disabled value={savedSelection.node}>
              {props.nodeLabel(savedSelection.node)}{savedTokens !== null ? ` · ${formatContextWindowTokens(savedTokens)}` : ""}
            </SelectItem>
          ) : null}
          {customAvailable || (savedSelection && "custom" in savedSelection) ? (
            <SelectItem disabled={!customAvailable} value="custom">
              {savedSelection && "custom" in savedSelection && !customEditing
                ? `${t("contextWindowCustom.label")} · ${formatCustomContextWindowInput(savedSelection.custom)}`
                : t("contextWindowCustom.label")}
            </SelectItem>
          ) : null}
        </SelectContent>
      </Select>
      {customEditing && limit !== null ? (
        <CustomContextWindowEditor
          disabled={saving}
          error={customError}
          maxTokens={limit}
          minTokens={props.customMinTokens}
          onCancel={() => { setCustomEditing(false); setCustomError(null) }}
          onChange={(value) => { setCustomText(value); setCustomError(null) }}
          onSubmit={submitCustom}
          value={customText}
        />
      ) : null}
      {savedIsInvalid ? <p className="vk-form-note">{t("settings.modelContextWindowDefaults.invalid")}</p> : null}
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
    </div>
  )
}
