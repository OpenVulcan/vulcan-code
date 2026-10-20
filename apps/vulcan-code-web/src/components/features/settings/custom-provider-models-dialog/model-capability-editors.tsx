// Capability, account availability and advanced request editors for one custom model.
// 单个自定义模型的能力、账号可用性与高级请求编辑器。
import * as React from "react"
import { CircleAlert, LoaderCircle, RotateCcw, Trash2 } from "lucide-react"
import { buildCapabilityOverridePatch, ReasoningProjectionEditor } from "@/components/features/settings/public-ai-model-capability-dialog"
import { CustomProviderMutationContext } from "../custom-provider-mutation-context"
import { directCustomModelSettingsDraft, directProviderModelSettingsEqual, OPENAI_DIRECT_REASONING_EFFORTS } from "@/components/features/settings/direct-provider-model-settings"
import { accountActuallyDiscoveredModel, accountExposesModel, accountIncludesModel, type CustomProviderModelAccountState, type CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/ui-kit/primitives/accordion"
import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { AlertDialog, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { Textarea } from "@/ui-kit/primitives/textarea"
import { useI18n } from "@/i18n"
import { formatTokenCount } from "@/lib/usage-format"
import { loadPublicAiSettings, savePublicAiCustomModelConnectionAvailability, savePublicAiModelCapabilityOverrides, validatePublicAiCustomModel } from "@/service/settings-actions-service"
import type { PublicAiCustomModelDefinition, PublicAiModelCapabilityOverrides, PublicAiResourceKind, PublicAiSettingsSnapshot, PublicAiToolSchemaProjectionMode } from "@/types"
import { errorMessage } from "@/lib/display-format"
import { EMPTY_PROJECTION, DISCOVERY_STATE_LABEL_KEYS } from "./model-dialog-constants"
import "@/ui-kit/styles/features/provider-models.css"
import "@/ui-kit/styles/features/settings-forms.css"

// RESOURCE_KINDS excludes video until custom routes can declare provider-specific video input.
// RESOURCE_KINDS 在自定义路由能够声明供应商专属视频输入前排除视频。
const RESOURCE_KINDS: PublicAiResourceKind[] = ["image", "audio", "file"]

// RESOURCE_LABEL_KEYS maps resource kinds to localized labels without dynamic translation keys.
// RESOURCE_LABEL_KEYS 将资源类型映射到本地化标签且不使用动态翻译键。
const RESOURCE_LABEL_KEYS = {
  image: "settings.models.modality.image",
  audio: "settings.models.modality.audio",
  video: "settings.models.modality.video",
  file: "settings.models.modality.file",
} as const

// REASONING_EFFORT_LABEL_KEYS maps canonical effort values to localized labels.
// REASONING_EFFORT_LABEL_KEYS 将规范思考档位映射为本地化标签。
const REASONING_EFFORT_LABEL_KEYS = {
  none: "settings.customModels.effortNone",
  minimal: "settings.customModels.effortMinimal",
  low: "settings.customModels.effortLow",
    medium: "settings.customModels.effortMedium",
    high: "settings.customModels.effortHigh",
    xhigh: "settings.customModels.effortExtraHigh",
    max: "settings.customModels.effortMax",
  } as const

// ResolvedCapabilitySummary explains final basic capabilities and their strongest visible source.
// ResolvedCapabilitySummary 说明最终基础能力及其最强可见来源。
export function CapabilityAccountSelector(props: {
  connectionId: string
  onConnectionChange: (connectionId: string) => void
  row: CustomProviderModelRow
}) {
  const { t } = useI18n()
  if (props.row.accounts.length === 0) {
    return <Alert><CircleAlert aria-hidden="true" /><AlertTitle>{t("settings.customModels.noAccounts")}</AlertTitle><AlertDescription>{t("settings.customModels.noAccountsDescription")}</AlertDescription></Alert>
  }
  return (
    <label className="vk-field">
      {t("settings.customModels.capabilityAccount")}
      <Select onValueChange={props.onConnectionChange} value={props.connectionId}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          {props.row.accounts.map((account) => (
            <SelectItem key={account.connection.id} value={account.connection.id}>
              {account.connection.display_name} · {account.connection.pending_validation ? t("settings.customModels.pendingValidation") : t(DISCOVERY_STATE_LABEL_KEYS[account.status?.state ?? "not_checked"])}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  )
}

// ResolvedCapabilitySummary explains final basic capabilities for one exact account.
// ResolvedCapabilitySummary 说明一个精确账号的最终基础能力。
export function ResolvedCapabilitySummary(props: {
  account?: CustomProviderModelAccountState
  defaults: { context_limit: number; output_limit: number } | null
  row: CustomProviderModelRow
}) {
  const { t } = useI18n()
  const capabilities = props.account?.effectiveModel?.capabilities
    ?? props.account?.supportModel?.capabilities
    ?? props.row.capabilityModel?.capabilities
    ?? props.row.effectiveModel?.capabilities
  const context = capabilities?.context_limit ?? props.defaults?.context_limit
  const output = capabilities?.output_limit ?? props.defaults?.output_limit
  const inputHasText = resolvedInputTextSupport(props.row, props.account)
  return (
    <div className="vk-form-facts">
      <CapabilityFact label={t("settings.customModels.contextLimit")} source={capabilitySource(props.row, props.account, "context_limit", t)} value={context != null ? formatTokenCount(context) : "—"} />
      <CapabilityFact label={t("settings.customModels.outputLimit")} source={capabilitySource(props.row, props.account, "output_limit", t)} value={output != null ? formatTokenCount(output) : "—"} />
      <CapabilityFact label={t("settings.customModels.input")} source={capabilitySource(props.row, props.account, "input_resources", t)} value={resourceSummary(capabilities?.input_resources ?? [], inputHasText, t)} />
      <CapabilityFact label={t("settings.customModels.output")} source={t("settings.customModels.defaultsSystemSource")} value={t("settings.models.modality.text")} />
    </div>
  )
}

// CapabilityFact renders one resolved value together with a provenance badge.
// CapabilityFact 渲染一个解析值及其来源徽标。
function CapabilityFact(props: { label: string; source: string; value: string }) {
  return <div className="vk-form-fact"><div><span>{props.label}</span><Badge variant="outline">{props.source}</Badge></div><strong>{props.value}</strong></div>
}

// AccountAvailabilityEditor renders exact source/effective states and explicit manual overrides.
// AccountAvailabilityEditor 渲染精确源/有效状态与显式手工覆盖。
// Parameters: props identifies the exact account/model capability state and mutation actions; returns access, validation and reset controls.
// 参数：props 标识精确账号模型能力状态与变更操作；返回访问、验证及恢复控件。
export function AccountAvailabilityEditor(props: {
  onAccountSelected: (connectionId: string) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  providerId: string
  row: CustomProviderModelRow
  selectedConnectionId: string | null
}) {
  const { t } = useI18n()
  // Account validation and membership writes share the provider-level lock.
  // 账号验证与成员关系写入共享供应商级锁。
  const mutationBoundary = React.useContext(CustomProviderMutationContext)
  const [savingConnectionId, setSavingConnectionId] = React.useState<string | null>(null)
  const [validationTarget, setValidationTarget] = React.useState<CustomProviderModelAccountState | null>(null)
  const [validatingConnectionId, setValidatingConnectionId] = React.useState<string | null>(null)
  const [validationResult, setValidationResult] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  // mutationBusy serializes membership changes and quota-consuming account validation.
  // mutationBusy 串行化成员关系变更和消耗配额的账号验证。
  const mutationBusy = React.useRef(false)

  // saveAvailability immediately persists one direct account-model membership choice or restoration.
  // saveAvailability 立即持久化一个直接账号模型成员关系选择或还原操作。
  const saveAvailability = React.useCallback(async (
    connectionId: string,
    enabled: boolean | undefined,
  ) => {
    if (mutationBusy.current) return
    if (mutationBoundary && !mutationBoundary.acquire()) return
    mutationBusy.current = true
    setSavingConnectionId(connectionId)
    setError(null)
    try {
      props.onSaved(await savePublicAiCustomModelConnectionAvailability({
        connectionId,
        modelId: props.row.id,
        enabled,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      mutationBusy.current = false
      mutationBoundary?.release()
      setSavingConnectionId(null)
    }
  }, [mutationBoundary, props])

  // validateAccount sends one user-confirmed minimal paid request before releasing a pending account.
  // validateAccount 在解除待验证账号隔离前发送一次用户确认的最小付费请求。
  const validateAccount = React.useCallback(async () => {
    if (!validationTarget || mutationBusy.current || !validationTarget.connection.enabled) {
      return
    }
    if (mutationBoundary && !mutationBoundary.acquire()) return
    mutationBusy.current = true
    setValidatingConnectionId(validationTarget.connection.id)
    setError(null)
    setValidationResult(null)
    try {
      const result = await validatePublicAiCustomModel({
        providerId: props.providerId,
        connectionId: validationTarget.connection.id,
        modelId: props.row.id,
      })
      props.onSaved(await loadPublicAiSettings())
      setValidationResult(
        t("settings.customModels.validationSucceeded")
          .replace("{account}", validationTarget.connection.display_name)
          .replace("{response}", result.responseText),
      )
      setValidationTarget(null)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      mutationBusy.current = false
      mutationBoundary?.release()
      setValidatingConnectionId(null)
    }
  }, [mutationBoundary, props, t, validationTarget])

  return (
    <div className="vk-pm-review">
      {props.row.accounts.map((account) => (
        <div className="vk-pm-account-row" data-selected={props.selectedConnectionId === account.connection.id ? "" : undefined} key={account.connection.id}>
          <div>
            <strong>{account.connection.display_name}</strong>
            <div className="vk-form-chips">
              <Badge variant="outline">{t(DISCOVERY_STATE_LABEL_KEYS[account.status?.state ?? "not_checked"])}</Badge>
              {account.connection.pending_validation ? <Badge variant="destructive">{t("settings.customModels.pendingValidation")}</Badge> : null}
              <Badge variant={accountActuallyDiscoveredModel(account) ? "secondary" : "outline"}>{accountActuallyDiscoveredModel(account) ? t("settings.customModels.accountReturned") : t("settings.customModels.accountNotReturned")}</Badge>
              <Badge variant={accountExposesModel(account) ? "secondary" : "outline"}>{accountExposesModel(account) ? t("settings.customModels.accountAvailable") : t("settings.customModels.accountUnavailable")}</Badge>
            </div>
          </div>
          <div className="vk-form-acts">
            <Button
              onClick={() => props.onAccountSelected(account.connection.id)}
              size="sm"
              type="button"
              variant={props.selectedConnectionId === account.connection.id ? "secondary" : "outline"}
            >
              {t("settings.customModels.viewAccountCapabilities")}
            </Button>
            <Button
              disabled={!account.connection.enabled || !accountExposesModel(account) || validatingConnectionId !== null || savingConnectionId !== null}
              onClick={() => {
                setError(null)
                setValidationResult(null)
                setValidationTarget(account)
              }}
              size="sm"
              type="button"
              variant={account.connection.pending_validation ? "default" : "outline"}
            >
              {validatingConnectionId === account.connection.id ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : null}
              {t("settings.customModels.validateModel")}
            </Button>
            {props.row.manualDefinition ? (
              <div className="vk-form-inline">
                {account.availabilityOverride !== undefined ? (
                  <Button
                    aria-label={t("settings.modelCapabilities.restore")}
                    disabled={savingConnectionId !== null || validatingConnectionId !== null}
                    onClick={() => void saveAvailability(account.connection.id, undefined)}
                    size="icon-sm"
                    title={t("settings.modelCapabilities.restore")}
                    type="button"
                    variant="ghost"
                  >
                    <RotateCcw aria-hidden="true" />
                  </Button>
                ) : null}
                {savingConnectionId === account.connection.id ? (
                  <LoaderCircle aria-hidden="true" className="vk-spin vk-faint" />
                ) : null}
                <Switch
                  aria-label={`${account.connection.display_name} · ${t("settings.customModels.accountAvailable")}`}
                  checked={accountIncludesModel(account)}
                  disabled={savingConnectionId !== null || validatingConnectionId !== null}
                  onCheckedChange={(enabled) => void saveAvailability(account.connection.id, enabled)}
                />
              </div>
            ) : null}
          </div>
        </div>
      ))}
      {validationResult ? <Alert><AlertTitle>{t("settings.customModels.validationComplete")}</AlertTitle><AlertDescription>{validationResult}</AlertDescription></Alert> : null}
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
      <AlertDialog
        onOpenChange={(open) => {
          if (!open && !mutationBusy.current) {
            setValidationTarget(null)
          }
        }}
        open={validationTarget !== null}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.validateModelTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.customModels.validateModelDescription")
                .replace("{model}", props.row.displayName)
                .replace("{account}", validationTarget?.connection.display_name ?? "")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {error ? <p className="vk-form-fault" role="alert">{error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={validatingConnectionId !== null}>{t("memory.common.cancel")}</AlertDialogCancel>
            <Button disabled={validatingConnectionId !== null} onClick={() => void validateAccount()} type="button">
              {validatingConnectionId !== null ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : null}
              {t("settings.customModels.confirmValidation")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

// CapabilityOverridesEditor displays effective values while persisting only touched model fields.
// CapabilityOverridesEditor 显示有效值，但只持久化用户触碰过的模型字段。
// Parameters: props supplies one model and effective capability values; returns touched-field override drafts with shared mutation guards.
// 参数：props 提供一个模型及生效能力值；返回带共享变更守卫的已编辑字段覆盖草稿。
export function CapabilityOverridesEditor(props: { onSaved: (snapshot: PublicAiSettingsSnapshot) => void; providerId: string; row: CustomProviderModelRow }) {
  const { t } = useI18n()
  // Capability saves keep sibling defaults, navigation and account editors frozen until settlement.
  // 能力保存在结束前冻结同级默认值、导航及账号编辑器。
  const mutationBoundary = React.useContext(CustomProviderMutationContext)
  // draft is the complete visible model-level setting set, never a second enablement gate.
  // draft 是完整可见的模型级设置集合，不再是二次启用开关。
  const [draft, setDraft] = React.useState<PublicAiModelCapabilityOverrides>(() => directCustomModelSettingsDraft(props.row))
  // baselineDraft is the authoritative value against which local semantic changes are measured.
  // baselineDraft 是衡量本地语义变更所依据的权威值。
  const [baselineDraft, setBaselineDraft] = React.useState<PublicAiModelCapabilityOverrides>(() => directCustomModelSettingsDraft(props.row))
  const [saving, setSaving] = React.useState(false)
  // savingRef blocks duplicate writes while preserving invalid raw numeric input for correction.
  // savingRef 阻止重复写入，同时保留无效原始数值输入供修正。
  const savingRef = React.useRef(false)
  const [error, setError] = React.useState<string | null>(null)
  const [edited, setEdited] = React.useState(false)
  // touchedFields preserves provider inheritance when one visible field is edited.
  // touchedFields 在只编辑一个可见字段时保留供应商继承关系。
  const [touchedFields, setTouchedFields] = React.useState<Set<string>>(new Set())
  // dirty protects a real in-progress edit from unrelated runtime snapshot refreshes.
  // dirty 保护真实进行中的编辑，避免被无关运行时快照刷新覆盖。
  const dirty = !directProviderModelSettingsEqual(draft, baselineDraft)
  // draftModelKey uniquely identifies the provider-model editor identity.
  // draftModelKey 唯一标识供应商模型编辑器身份。
  const draftModelKey = `${props.providerId}\u0000${props.row.id}`
  // initializedModelKey tracks which identity currently owns the local draft.
  // initializedModelKey 跟踪当前本地草稿所属的身份。
  const initializedModelKey = React.useRef(draftModelKey)
  React.useEffect(() => {
    // modelChanged requires a full reset even when the previous model had unfinished edits.
    // modelChanged 即使前一模型存在未完成编辑，也要求完整重置。
    const modelChanged = initializedModelKey.current !== draftModelKey
    if (!modelChanged && (dirty || edited)) {
      return
    }
    // nextDraft becomes both the visible and comparison state for this authoritative row.
    // nextDraft 同时成为该权威模型行的可见状态与比较状态。
    const nextDraft = directCustomModelSettingsDraft(props.row)
    initializedModelKey.current = draftModelKey
    setDraft(nextDraft)
    setBaselineDraft(nextDraft)
    setError(null)
    setEdited(false)
    setTouchedFields(new Set())
  }, [dirty, draftModelKey, edited, props.row])

  // updateDraft changes visible values; semantic comparison derives whether a real edit exists.
  // updateDraft 修改可见值；是否存在真实编辑由语义比较推导。
  const updateDraft = React.useCallback((
    fields: string | readonly string[],
    update: (current: PublicAiModelCapabilityOverrides) => PublicAiModelCapabilityOverrides,
  ) => {
    setDraft(update)
    setEdited(true)
    setTouchedFields((current) => {
      const next = new Set(current)
      for (const field of typeof fields === "string" ? [fields] : fields) {
        next.add(field)
      }
      return next
    })
    setError(null)
  }, [])

  // settingsToPersist converts the complete visible draft into the sparse model patch accepted by the service.
  // settingsToPersist 将完整可见草稿转换为服务端接受的稀疏模型补丁。
  const settingsToPersist = React.useMemo(
    () => buildCapabilityOverridePatch(draft, props.row.capabilityOverrides, touchedFields),
    [draft, props.row.capabilityOverrides, touchedFields],
  )

  // persist writes the visible model layer or removes it to restore provider inheritance.
  // persist 写入可见模型层，或移除该层以恢复供应商继承。
  const persist = React.useCallback(async (restore: boolean) => {
    if (savingRef.current) return
    if (!restore && (!Number.isSafeInteger(draft.context_limit) || (draft.context_limit ?? 0) <= 0)) {
      setError(t("settings.modelCapabilities.contextInvalid"))
      return
    }
    if (!restore && (!Number.isSafeInteger(draft.output_limit) || (draft.output_limit ?? 0) <= 0 || (draft.output_limit ?? 0) > (draft.context_limit ?? 0))) {
      setError(t("settings.customModels.defaultsOutputInvalid"))
      return
    }
    if (!restore && draft.reasoning !== "unsupported" && !(draft.reasoning_efforts ?? []).some((effort) => effort !== "none")) {
      setError(t("settings.customModels.defaultsReasoningInvalid"))
      return
    }
    if (mutationBoundary && !mutationBoundary.acquire()) return
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiModelCapabilityOverrides({
        providerId: props.providerId,
        modelId: props.row.id,
        overrides: restore ? undefined : settingsToPersist,
      }))
      setBaselineDraft(draft)
      setEdited(false)
      setTouchedFields(new Set())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      savingRef.current = false
      mutationBoundary?.release()
      setSaving(false)
    }
  }, [draft, mutationBoundary, props, settingsToPersist, t])

  // restore discards an unsaved draft locally or removes an existing persisted model layer.
  // restore 在本地丢弃未保存草稿，或移除已持久化模型层。
  const restore = React.useCallback(() => {
    if (props.row.capabilityOverrides) {
      void persist(true)
      return
    }
    // nextDraft discards only the unsaved local delta when no persisted model layer exists.
    // nextDraft 在不存在已持久化模型层时，仅丢弃未保存的本地差异。
    const nextDraft = directCustomModelSettingsDraft(props.row)
    setDraft(nextDraft)
    setBaselineDraft(nextDraft)
    setEdited(false)
    setTouchedFields(new Set())
    setError(null)
  }, [persist, props.row])

  // reasoningEnabled projects the paired support and control contract into one clear switch.
  // reasoningEnabled 将配对的支持与控制契约投影为一个清晰开关。
  const reasoningEnabled = draft.reasoning !== "unsupported" && draft.reasoning_control !== "fixed_off"
  return (
    <fieldset disabled={saving} className="vk-fieldset">
      <div className="vk-form-head">
        <div><p>{t("settings.modelCapabilities.overlayHint")}</p></div>
        <Badge variant={props.row.capabilityOverrides ? "default" : "outline"}>{t(props.row.capabilityOverrides ? "settings.modelCapabilities.modelSource" : "settings.modelCapabilities.inheritedSource")}</Badge>
      </div>
      <div className="vk-form-grid">
        <label className="vk-field">{t("settings.customModels.contextLimit")}<Input type="number" min={1} step={1} inputMode="numeric" onChange={(event) => { const value = event.target.value; updateDraft("context_limit", (current) => ({ ...current, context_limit: value === "" ? undefined : Number(value) })) }} value={draft.context_limit ?? ""} /></label>
        <label className="vk-field">{t("settings.customModels.outputLimit")}<Input type="number" min={1} step={1} inputMode="numeric" onChange={(event) => { const value = event.target.value; updateDraft("output_limit", (current) => ({ ...current, output_limit: value === "" ? undefined : Number(value) })) }} value={draft.output_limit ?? ""} /></label>
      </div>
      <fieldset className="vk-form-group">
        <legend>{t("settings.customModels.input")}</legend>
        <div className="vk-form-checks">
          {RESOURCE_KINDS.map((resource) => <label className="vk-check-label" key={resource}><Checkbox checked={(draft.input_resources ?? []).includes(resource)} onCheckedChange={(checked) => updateDraft("input_resources", (current) => ({ ...current, input_resources: checked ? [...(current.input_resources ?? []), resource] : (current.input_resources ?? []).filter((item) => item !== resource) }))} />{t(RESOURCE_LABEL_KEYS[resource])}</label>)}
        </div>
      </fieldset>
      <div className="vk-form-box" data-tone="muted">
        <div className="vk-form-head">
          <div>
            <strong>{t("settings.customModels.output")}: {t("settings.models.modality.text")}</strong>
            <small>{t("settings.customModels.fixedTextOutputDescription")}</small>
          </div>
        </div>
      </div>
      <div className="vk-form-group">
        <label className="vk-form-switch"><span>{t("settings.customModels.reasoning")}</span><Switch checked={reasoningEnabled} onCheckedChange={(checked) => updateDraft(["reasoning", "reasoning_control", "reasoning_efforts"], (current) => ({ ...current, reasoning: checked ? "native" : "unsupported", reasoning_control: checked ? "effort_selection" : "fixed_off", reasoning_efforts: checked ? [...OPENAI_DIRECT_REASONING_EFFORTS] : ["none"] }))} /></label>
        {reasoningEnabled ? <div className="vk-form-efforts">{OPENAI_DIRECT_REASONING_EFFORTS.map((effort) => <label className="vk-check-label" key={effort}><Checkbox checked={(draft.reasoning_efforts ?? []).includes(effort)} disabled={effort !== "none" && (draft.reasoning_efforts ?? []).includes(effort) && (draft.reasoning_efforts ?? []).filter((item) => item !== "none").length === 1} onCheckedChange={(checked) => updateDraft("reasoning_efforts", (current) => ({ ...current, reasoning_efforts: checked ? [...(current.reasoning_efforts ?? []), effort] : (current.reasoning_efforts ?? []).filter((item) => item !== effort) }))} />{t(REASONING_EFFORT_LABEL_KEYS[effort])}</label>)}</div> : null}
      </div>
      <ReasoningProjectionEditor
        efforts={draft.reasoning_efforts?.length ? draft.reasoning_efforts : ["none"]}
        onChange={(request_projection) => updateDraft("request_projection", (current) => ({ ...current, request_projection }))}
        projection={draft.request_projection ?? EMPTY_PROJECTION}
      />
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
      <div className="vk-form-acts"><Button disabled={saving || (!props.row.capabilityOverrides && !dirty && !edited)} onClick={restore} type="button" variant="outline">{t("settings.modelCapabilities.restore")}</Button><Button disabled={saving || (!dirty && !edited)} onClick={() => void persist(false)} type="button">{saving ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : null}{t("settings.customModels.saveCapabilityOverrides")}</Button></div>
    </fieldset>
  )
}

// AdvancedModelEditor edits manual identity metadata and protocol-specific exact declarations.
// AdvancedModelEditor 编辑手工身份元数据与协议专属精确声明。
// Parameters: props supplies the manual model and protocol declarations; returns raw JSON/identity drafts and guarded atomic-save controls.
// 参数：props 提供手工模型与协议声明；返回原始 JSON、身份草稿及受保护的原子保存控件。
export function AdvancedModelEditor(props: {
  customized: boolean
  dirty: boolean
  manualDefinition: PublicAiCustomModelDefinition
  onRestore: () => void
  onSave: () => void
  onUpdate: (update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => void
  parameterText: string
  projectionText: string
  setParameterText: (value: string) => void
  setProjectionText: (value: string) => void
  saving: boolean
}) {
  const { t } = useI18n()
  // model is always the visible inherited or explicit definition and is editable immediately.
  // model 始终是可见的继承或显式定义，并且可以立即编辑。
  const model = props.manualDefinition
  return (
    <fieldset disabled={props.saving} className="vk-fieldset">
      <label className="vk-field">{t("settings.customModels.displayName")}<Input onChange={(event) => props.onUpdate((current) => ({ ...current, display_name: event.target.value }))} value={model.display_name} /></label>
      <Accordion collapsible type="single">
        <AccordionItem value="json">
          <AccordionTrigger>{t("settings.customModels.advanced")}</AccordionTrigger>
          <AccordionContent className="vk-fieldset">
            <label className="vk-field">
              {t("settings.customModels.toolSchemaProjection")}
              <Select
                onValueChange={(value) => props.onUpdate((current) => ({
                  ...current,
                  tool_schema_projection: value as PublicAiToolSchemaProjectionMode,
                }))}
                value={model.tool_schema_projection}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="lightweight">{t("settings.customModels.toolSchemaProjectionLightweight")}</SelectItem>
                  <SelectItem value="passthrough">{t("settings.customModels.toolSchemaProjectionPassthrough")}</SelectItem>
                  <SelectItem value="strict">{t("settings.customModels.toolSchemaProjectionStrict")}</SelectItem>
                </SelectContent>
              </Select>
              <span className="vk-field-hint">{t("settings.customModels.toolSchemaProjectionHint")}</span>
            </label>
            <label className="vk-field">{t("settings.customModels.parameters")}<Textarea className="vk-form-code" onChange={(event) => props.setParameterText(event.target.value)} spellCheck={false} value={props.parameterText} /></label>
            <label className="vk-field">{t("settings.customModels.projection")}<Textarea className="vk-form-code" data-size="lg" onChange={(event) => props.setProjectionText(event.target.value)} spellCheck={false} value={props.projectionText} /></label>
          </AccordionContent>
        </AccordionItem>
      </Accordion>
      <div className="vk-form-acts">{props.customized ? <Button disabled={props.saving} onClick={props.onRestore} type="button" variant="outline"><Trash2 aria-hidden="true" />{t("settings.modelCapabilities.restore")}</Button> : null}<Button disabled={props.saving || !props.dirty} onClick={props.onSave} type="button">{t("settings.customModels.saveAdvanced")}</Button></div>
    </fieldset>
  )
}

// capabilitySource resolves the strongest visible source for one account and capability field.
// capabilitySource 解析一个账号与能力字段的最强可见来源。
function capabilitySource(
  row: CustomProviderModelRow,
  account: CustomProviderModelAccountState | undefined,
  field: keyof PublicAiModelCapabilityOverrides,
  t: ReturnType<typeof useI18n>["t"],
): string {
  if (row.capabilityOverrides && field in row.capabilityOverrides) return t("settings.customModels.sourceUserOverride")
  const evidence = account?.discoveredModel?.capability_evidence
    ?? account?.supportModel?.capability_evidence
    ?? account?.effectiveModel?.capability_evidence
  if (evidence) {
    if (field === "context_limit" && evidence.context_limit_declared) return t("settings.customModels.sourceAccount")
    if (field === "output_limit" && evidence.output_limit_declared) return t("settings.customModels.sourceAccount")
    if (field === "input_resources" && evidence.input_modalities_declared) return t("settings.customModels.sourceAccount")
  }
  if (row.templateDecision?.decision === "applied" && field !== "output_limit") return t("settings.customModels.sourceTemplate")
  return t("settings.customModels.sourceProviderDefault")
}

// resolvedInputTextSupport resolves input text support without treating explicit false as missing.
// resolvedInputTextSupport 解析输入文本支持且不会把显式 false 当作缺失值。
function resolvedInputTextSupport(
  row: CustomProviderModelRow,
  account: CustomProviderModelAccountState | undefined,
): boolean {
  const evidence = account?.discoveredModel?.capability_evidence
    ?? account?.supportModel?.capability_evidence
    ?? account?.effectiveModel?.capability_evidence
  if (evidence?.input_modalities_declared) {
    return evidence.supports_text_input === true
  }
  if (row.templateDecision?.decision === "applied") {
    return row.templateDecision.selection.supports_text_input
  }
  return true
}

// resourceSummary formats explicit text and non-text resource modalities.
// resourceSummary 格式化明确的文本与非文本资源模态。
function resourceSummary(
  resources: PublicAiResourceKind[],
  includesText: boolean,
  t: ReturnType<typeof useI18n>["t"],
): string {
  const values = [
    ...(includesText ? [t("settings.models.modality.text")] : []),
    ...resources.map((resource) => t(RESOURCE_LABEL_KEYS[resource])),
  ]
  return values.length > 0 ? values.join(" · ") : "—"
}
