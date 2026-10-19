// Step-by-step dialog for creating and editing a custom provider.
// 分步创建与编辑自定义供应商的弹窗。
import * as React from "react"
import { CircleAlert, CircleCheckBig, LoaderCircle } from "lucide-react"
import { parseCustomProviderHeaderDraft } from "../custom-provider-header-draft"
import { persistAndApplyCustomProviderSnapshot } from "@/components/features/settings/custom-provider-save-state"
import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { DisclosureSection } from "@/ui-kit/primitives/disclosure"
import { Input } from "@/ui-kit/primitives/input"
import { ScrollArea } from "@/ui-kit/primitives/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog"
import { useI18n } from "@/i18n"
import type { TranslationKey } from "@/i18n-messages"
import { CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE, codexResponsesFeaturePoliciesFromEntry, customProtocolRouteDraftValue, customProtocolRouteSelectValue, customProtocolProfileId, emptyProtocolRouteDraft, protocolCollectionChanged, protocolCollectionFromEntry, protocolRoutesFromCollection, protocolSetChange, sortProtocolsByAutoPriority } from "@/components/features/settings/custom-provider-protocol-drafts"
import type { CustomProtocolRouteDraft, CustomProviderProtocolCollection } from "@/components/features/settings/custom-provider-protocol-drafts"
import { probePublicAiCustomProvider, savePublicAiCustomProvider } from "@/service/settings-actions-service"
import type { PublicAiCustomProviderModelDiscoveryMode, PublicAiCustomProviderProbeResult, PublicAiCustomProviderProtocol, PublicAiCustomProviderSaveRequest, PublicAiResponsesRemoteCompactionPolicy, PublicAiResponsesWebsocketPolicy, PublicAiConnection, PublicAiModelCapabilityEvidence, PublicAiModelOverrideRecord, PublicAiProviderDescriptor, PublicAiServiceEntry, PublicAiSettingsSnapshot } from "@/types"
import { formatDateTime, errorMessage } from "@/lib/display-format"
import { protocolDisplayName } from "./protocol-display-name"

// CustomProtocolRouteDraftCard edits one enabled protocol's route overrides: base URL
// override, endpoint path, authentication, discovery, and fixed headers. Empty values
// inherit the shared provider projection on save (plan §R6.5).
// CustomProtocolRouteDraftCard 编辑一个启用协议的路由覆盖：基础 URL 覆盖、端点路径、
// 鉴权、发现与固定请求头。保存时空值继承共享供应商投影（方案 §R6.5）。
function CustomProtocolRouteDraftCard(props: {
  draft: CustomProtocolRouteDraft
  onChange: (draft: CustomProtocolRouteDraft) => void
  // Report incomplete raw input so a parent save cannot silently keep an older valid map.
  // 上报未完成原始输入，防止父级保存静默保留旧的有效映射。
  onHeaderValidityChange: (valid: boolean) => void
  protocol: PublicAiCustomProviderProtocol
  t: (key: TranslationKey) => string
}) {
  const { draft, onChange, protocol, t } = props
  const [headersText, setHeadersText] = React.useState(() => Object.entries(draft.requestHeaders).map(([name, value]) => `${name}: ${value}`).join("\n"))
  const headersValid = parseCustomProviderHeaderDraft(headersText) !== null
  return (
    // Start open when this protocol already has its own address or headers, so existing overrides stay visible.
    // 该协议已有独立地址或请求头时默认展开，避免已有覆盖被藏起来。
    <DisclosureSection
      defaultOpen={Boolean(draft.baseUrlOverride.trim()) || Object.keys(draft.requestHeaders).length > 0}
      description={t("settings.customProviders.routeDraftForHint")}
      keepMounted
      title={t("settings.customProviders.routeDraftFor").replace(
        "{protocol}",
        protocolDisplayName(customProtocolProfileId(protocol)),
      )}
    >
      <div className="grid gap-3 text-sm">
        <label className="settings-form-field">
          <span>{t("settings.customProviders.routeBaseUrl")}</span>
          <Input
            onChange={(event) => onChange({ ...draft, baseUrlOverride: event.target.value })}
            placeholder={t("settings.customProviders.routeBaseUrlPlaceholder")}
            value={draft.baseUrlOverride}
          />
        </label>
        <label className="settings-form-field">
          <span>{t("settings.customProviders.routeEndpointPath")}</span>
          <Input
            onChange={(event) => onChange({ ...draft, endpointPath: event.target.value })}
            placeholder="/v1/chat/completions"
            value={draft.endpointPath}
          />
        </label>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="settings-form-field">
            <span>{t("settings.customProviders.routeAuthentication")}</span>
            <Select
              onValueChange={(value) => onChange({
                ...draft,
                authentication: customProtocolRouteDraftValue(value) as CustomProtocolRouteDraft["authentication"],
              })}
              value={customProtocolRouteSelectValue(draft.authentication)}
            >
              <SelectTrigger><SelectValue placeholder={t("settings.customProviders.routeInherit")} /></SelectTrigger>
              <SelectContent>
                <SelectItem value={CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE}>
                  {t("settings.customProviders.routeInherit")}
                </SelectItem>
                <SelectItem value="bearer_authorization">Authorization: Bearer</SelectItem>
                <SelectItem value="google_api_key">Gemini API Key</SelectItem>
                <SelectItem value="x_api_key">x-api-key</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <label className="settings-form-field">
            <span>{t("settings.customProviders.routeDiscoveryMode")}</span>
            <Select
              onValueChange={(value) => onChange({
                ...draft,
                modelDiscoveryMode: customProtocolRouteDraftValue(value) as CustomProtocolRouteDraft["modelDiscoveryMode"],
              })}
              value={customProtocolRouteSelectValue(draft.modelDiscoveryMode)}
            >
              <SelectTrigger><SelectValue placeholder={t("settings.customProviders.routeInherit")} /></SelectTrigger>
              <SelectContent>
                <SelectItem value={CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE}>
                  {t("settings.customProviders.routeInherit")}
                </SelectItem>
                <SelectItem value="standard">{t("settings.customProviders.discoveryStandard")}</SelectItem>
                <SelectItem value="custom_path">{t("settings.customProviders.discoveryCustomPath")}</SelectItem>
                <SelectItem value="manual">{t("settings.customProviders.discoveryManual")}</SelectItem>
              </SelectContent>
            </Select>
          </label>
        </div>
        {draft.modelDiscoveryMode === "custom_path" ? (
          <label className="settings-form-field">
            <span>{t("settings.customProviders.routeDiscoveryPath")}</span>
            <Input
              onChange={(event) => onChange({ ...draft, modelDiscoveryPath: event.target.value })}
              placeholder="/models"
              value={draft.modelDiscoveryPath}
            />
          </label>
        ) : null}
        <label className="settings-form-field">
          <span>{t("settings.customProviders.routeHeaders")}</span>
          <textarea
            className="min-h-[72px] w-full rounded-md border bg-background px-3 py-2 text-xs"
            onChange={(event) => {
              const rawText = event.target.value
              setHeadersText(rawText)
              const headers = parseCustomProviderHeaderDraft(rawText)
              props.onHeaderValidityChange(headers !== null)
              if (headers !== null) onChange({ ...draft, requestHeaders: headers })
            }}
            placeholder="accept: application/json"
            aria-invalid={!headersValid}
            value={headersText}
          />
          {!headersValid ? <span role="alert" className="text-destructive">{t("settings.providers.completeEachRowAs")}</span> : null}
        </label>
      </div>
    </DisclosureSection>
  )
}

// CustomProviderDialog edits the exact public custom-provider contract.
// CustomProviderDialog 编辑精确的公开自定义供应商契约。
export function CustomProviderDialog(props: {
  connections: PublicAiConnection[]
  modelOverrideRecords: PublicAiModelOverrideRecord[]
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  provider: PublicAiProviderDescriptor | null
}) {
  const { t, locale } = useI18n()
  const [displayName, setDisplayName] = React.useState("")
  const [baseUrl, setBaseUrl] = React.useState("")
  const [sessionHeader, setSessionHeader] = React.useState("")
  // protocols is the authoritative enabled protocol collection (at least one element) and
  // routeDrafts keeps one independent draft per enabled protocol (plan §R6.4/§R6.5).
  // protocols 是权威启用协议集合（至少一个元素），routeDrafts 为每个启用协议保留独立草稿
  // （方案 §R6.4/§R6.5）。
  const [protocols, setProtocols] = React.useState<PublicAiCustomProviderProtocol[]>(["open_ai_responses"])
  const [routeDrafts, setRouteDrafts] = React.useState<CustomProviderProtocolCollection["drafts"]>({})
  const [remoteCompactionPolicy, setRemoteCompactionPolicy] = React.useState<PublicAiResponsesRemoteCompactionPolicy>("disabled")
  const [websocketPolicy, setWebsocketPolicy] = React.useState<PublicAiResponsesWebsocketPolicy>("disabled")
  const [authentication, setAuthentication] = React.useState<"bearer_authorization" | "google_api_key" | "x_api_key">("bearer_authorization")
  const [modelDiscoveryMode, setModelDiscoveryMode] = React.useState<PublicAiCustomProviderModelDiscoveryMode>("standard")
  const [modelDiscoveryPath, setModelDiscoveryPath] = React.useState("")
  const [credentialDisplayName, setCredentialDisplayName] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [validationConnectionId, setValidationConnectionId] = React.useState("")
  const [step, setStep] = React.useState<"connection" | "credential" | "result">("connection")
  const [probeResult, setProbeResult] = React.useState<PublicAiCustomProviderProbeResult | null>(null)
  const [probing, setProbing] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [pendingConfirmation, setPendingConfirmation] = React.useState<"manual" | "pending" | null>(null)
  // protocolOverrideConfirmation retains the original save intent while incompatible route settings are confirmed.
  // protocolOverrideConfirmation 在确认不兼容路由设置期间保留原始保存意图。
  const [protocolOverrideConfirmation, setProtocolOverrideConfirmation] = React.useState<"ready" | "manual" | "pending" | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  // One operation owns the wizard draft until probing or saving has settled.
  // 探测或保存结束前，由单次操作持有向导草稿。
  const operationRunningRef = React.useRef(false)
  const [invalidHeaderProtocols, setInvalidHeaderProtocols] = React.useState<Set<PublicAiCustomProviderProtocol>>(new Set())
  const headersValid = !protocols.some((protocol) => invalidHeaderProtocols.has(protocol))
  // providerConnections is the complete account set owned by the edited provider.
  // providerConnections 是被编辑供应商持有的完整账号集合。
  const providerConnections = React.useMemo(() => props.connections.filter(
    (connection) => connection.provider_id === props.provider?.id,
  ), [props.connections, props.provider?.id])
  // initializedDialogIdentity prevents background snapshots from erasing an in-progress provider edit.
  // initializedDialogIdentity 防止后台快照清除正在进行的供应商编辑。
  const initializedDialogIdentity = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (!props.open) {
      initializedDialogIdentity.current = null
      return
    }
    // dialogIdentity distinguishes provider creation from each concrete provider editor.
    // dialogIdentity 区分供应商新建窗口与每一个具体供应商编辑器。
    const dialogIdentity = props.provider?.id ?? "__new_custom_provider__"
    if (initializedDialogIdentity.current === dialogIdentity) {
      return
    }
    initializedDialogIdentity.current = dialogIdentity
    const entry = props.provider?.service_entries[0]
    setDisplayName(props.provider?.display_name ?? "")
    setBaseUrl(entry?.base_url ?? "")
    setSessionHeader(entry?.session_header ?? "")
    setAuthentication(entry?.authentication === "google_api_key"
      ? "google_api_key"
      : entry?.authentication === "x_api_key" ? "x_api_key" : "bearer_authorization")
    // The persisted protocol collection and its per-protocol route drafts initialize the
    // editable state so an edit never drops a route-level override (plan §R6.5).
    // 持久化协议集合及其逐协议路由草稿初始化可编辑状态，使编辑绝不丢弃路由级覆盖
    // （方案 §R6.5）。
    const collection = protocolCollectionFromEntry(entry)
    setProtocols(collection.protocols)
    setRouteDrafts(collection.drafts)
    const codexFeatures = codexResponsesFeaturePoliciesFromEntry(entry)
    setRemoteCompactionPolicy(codexFeatures.remoteCompactionPolicy)
    setWebsocketPolicy(codexFeatures.websocketPolicy)
    setModelDiscoveryMode(customProviderDiscoveryMode(entry))
    const firstDraft = collection.drafts[collection.protocols[0]]
    setModelDiscoveryPath(firstDraft?.modelDiscoveryPath ?? "")
    setCredentialDisplayName(props.provider?.display_name ?? "")
    setSecret("")
    setValidationConnectionId(providerConnections[0]?.id ?? "")
    setStep("connection")
    setProbeResult(null)
    setPendingConfirmation(null)
    setProtocolOverrideConfirmation(null)
    setInvalidHeaderProtocols(new Set())
    setError(null)
  }, [props.open, props.provider, providerConnections])

  React.useEffect(() => {
    if (!props.open || !props.provider) {
      return
    }
    setValidationConnectionId((current) => (
      providerConnections.some((connection) => connection.id === current)
        ? current
        : (providerConnections[0]?.id ?? "")
    ))
  }, [props.open, props.provider, providerConnections])

  // validateConnectionDraft validates the complete locally editable endpoint contract.
  // validateConnectionDraft 校验完整的本地可编辑入口契约。
  const validateConnectionDraft = React.useCallback((): boolean => {
    const validationError = validateCustomProviderConnectionStep({
      baseUrl,
      displayName,
      modelDiscoveryMode,
      modelDiscoveryPath,
    })
    if (validationError) {
      setError(t(validationError))
      return false
    }
    setError(null)
    return true
  }, [baseUrl, displayName, modelDiscoveryMode, modelDiscoveryPath, t])

  // continueToCredential validates the endpoint contract before asking for a secret.
  // continueToCredential 在请求秘密前校验入口契约。
  const continueToCredential = React.useCallback(() => {
    if (!headersValid || !validateConnectionDraft()) {
      return
    }
    setProbeResult(null)
    setStep("credential")
  }, [headersValid, validateConnectionDraft])

  // requestBody builds the exact typed service request from the current form.
  // requestBody 从当前表单构造精确的类型化服务请求。
  const requestBody = React.useCallback((
    acceptance?: "manual" | "pending",
    confirmInactiveProtocolOverrideDeletion = false,
  ): PublicAiCustomProviderSaveRequest => ({
    providerId: props.provider?.id,
    displayName: displayName.trim(),
    baseUrl: baseUrl.trim(),
    sessionHeader: sessionHeader.trim() || undefined,
    // The enabled protocol collection is authoritative and never empty.
    // 启用协议集合是权威事实且绝不为空。
    protocols,
    remoteCompactionPolicy,
    websocketPolicy,
    authentication,
    ...(props.provider
      ? (validationConnectionId ? { validationConnectionId } : {})
      : {
          credentialDisplayName: credentialDisplayName.trim(),
          secret: secret.trim(),
        }),
    modelDiscoveryMode,
    modelDiscoveryPath: modelDiscoveryMode === "custom_path" ? modelDiscoveryPath.trim() : undefined,
    // Per-protocol route overrides are serialized from the independent drafts so edits never
    // silently drop route-level base URLs, authentication, discovery, or headers (plan
    // §R6.5/§R6.10).
    // 逐协议路由覆盖从独立草稿序列化，使编辑绝不静默丢弃路由级基础 URL、鉴权、发现或
    // 请求头（方案 §R6.5/§R6.10）。
    protocolRoutes: protocolRoutesFromCollection({ protocols, drafts: routeDrafts }),
    allowManualFallback: acceptance === "manual",
    allowPendingValidation: acceptance === "pending",
    confirmInactiveProtocolOverrideDeletion,
  }), [authentication, baseUrl, credentialDisplayName, displayName, modelDiscoveryMode, modelDiscoveryPath, props.provider, protocols, remoteCompactionPolicy, routeDrafts, secret, sessionHeader, validationConnectionId, websocketPolicy])

  // probe verifies authentication and model discovery without persisting the provider or secret.
  // probe 在不持久化供应商或秘密的情况下验证鉴权与模型发现。
  const probe = React.useCallback(async () => {
    if (operationRunningRef.current || !headersValid) return
    if (props.provider && !validationConnectionId) {
      setError(t("settings.customProviders.validationCredentialRequired"))
      return
    }
    if (!props.provider && (!credentialDisplayName.trim() || !secret.trim())) {
      setError(t("settings.customProviders.credentialRequired"))
      return
    }
    operationRunningRef.current = true
    setProbing(true)
    setError(null)
    try {
      setProbeResult(await probePublicAiCustomProvider(requestBody()))
      setStep("result")
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      operationRunningRef.current = false
      setProbing(false)
    }
  }, [credentialDisplayName, headersValid, props.provider, requestBody, secret, t, validationConnectionId])

  // originalEntry is the persisted technical contract used to distinguish cosmetic edits.
  // originalEntry 是用于区分纯展示编辑的持久化技术契约。
  const originalEntry = props.provider?.service_entries[0]
  // originalProtocols is the exact persisted protocol set projected into the browser-editable
  // contract (plan §R6.8).
  // originalProtocols 是投影到浏览器可编辑契约中的精确持久化协议集合（方案 §R6.8）。
  const originalProtocols = protocolCollectionFromEntry(originalEntry).protocols
  // protocolSetChange distinguishes destructive protocol removals from other technical edits;
  // additions never invalidate existing settings (plan §R6.8).
  // protocolSetChange 将破坏性协议删除与其他技术编辑区分开；新增绝不让既有设置失效
  // （方案 §R6.8）。
  const protocolChange = protocolSetChange(originalProtocols, protocols)
  const removedProtocols = protocolChange.removed
  // inactiveProtocolOverrideCount counts exact settings owned by the removed protocols that
  // require explicit deletion confirmation.
  // inactiveProtocolOverrideCount 统计被移除协议拥有的、需要显式确认删除的精确设置数量。
  const inactiveProtocolOverrideCount = props.provider && removedProtocols.length > 0
    ? props.modelOverrideRecords.filter((record) => (
        record.providerId === props.provider?.id
        && removedProtocols.some((removed) => customProtocolProfileId(removed) === record.protocolId)
      )).length
    : 0

  // save persists only the explicit action represented by the result screen.
  // save 仅持久化结果页明确表示的操作。
  const save = React.useCallback(async (
    acceptance?: "manual" | "pending",
    confirmInactiveProtocolOverrideDeletion = false,
  ) => {
    if (operationRunningRef.current || !headersValid) return
    if (inactiveProtocolOverrideCount > 0 && !confirmInactiveProtocolOverrideDeletion) {
      setPendingConfirmation(null)
      setProtocolOverrideConfirmation(acceptance ?? "ready")
      return
    }
    operationRunningRef.current = true
    setSaving(true)
    setError(null)
    try {
      await persistAndApplyCustomProviderSnapshot(
        requestBody(acceptance, confirmInactiveProtocolOverrideDeletion),
        savePublicAiCustomProvider,
        props.onSaved,
      )
      props.onOpenChange(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      operationRunningRef.current = false
      setSaving(false)
    }
  }, [headersValid, inactiveProtocolOverrideCount, props, requestBody])

  // saveCredentialFreeEdit persists a valid provider contract when no account exists to probe.
  // saveCredentialFreeEdit 在不存在可探测账号时持久化有效的供应商契约。
  const saveCredentialFreeEdit = React.useCallback(() => {
    if (!validateConnectionDraft()) {
      return
    }
    void save()
  }, [save, validateConnectionDraft])

  // primaryProtocol is the first enabled protocol used for the shared endpoint preview and
  // the shared Codex capability switches; every protocol still keeps its own route draft.
  // primaryProtocol 是用于共享端点预览与共享 Codex 能力开关的第一个启用协议；每个协议
  // 仍保留自己的路由草稿。
  const primaryProtocol = protocols[0] ?? "open_ai_responses"
  const protocolEndpoint = customProviderEndpointPreview(baseUrl, primaryProtocol)
  const discoveryEndpoint = customProviderDiscoveryEndpointPreview(baseUrl, modelDiscoveryMode, modelDiscoveryPath)
  const acceptedWithoutOverride = probeResult?.status.state === "ready"
  const validationConnection = providerConnections.find(
    (connection) => connection.id === validationConnectionId,
  )
  // connectionContractChanged mirrors the backend service-entry comparison for user-facing routing.
  // connectionContractChanged 镜像后端服务入口比较，用于决定界面流程。
  const connectionContractChanged = Boolean(props.provider) && (
    !originalEntry
    || baseUrl.trim().replace(/\/+$/, "") !== originalEntry.base_url.replace(/\/+$/, "")
    || sessionHeader.trim() !== (originalEntry.session_header ?? "")
    || protocolCollectionChanged(originalEntry, { protocols, drafts: routeDrafts })
    || authentication !== customProviderAuthentication(originalEntry)
    || modelDiscoveryMode !== customProviderDiscoveryMode(originalEntry)
    || (modelDiscoveryMode === "custom_path"
      && modelDiscoveryPath.trim() !== (protocolCollectionFromEntry(originalEntry).drafts[originalProtocols[0]]?.modelDiscoveryPath ?? ""))
  )
  // responsesFeaturesChanged tracks explicit Codex transport capabilities without forcing credential revalidation.
  // responsesFeaturesChanged 跟踪显式 Codex 传输能力开关，但不会强制重新验证凭据。
  const originalResponsesFeatures = originalEntry?.protocol_routes.find(
    (route) => route.protocol_profile_id === "openai-codex-responses.v1",
  )?.responses_features
  const responsesFeaturesChanged = Boolean(props.provider)
    && protocols.includes("open_ai_codex_responses")
    && (
      remoteCompactionPolicy !== (originalResponsesFeatures?.remote_compaction ?? "disabled")
      || websocketPolicy !== (originalResponsesFeatures?.websocket ?? "disabled")
    )
  // credentialFreeEdit identifies an existing provider whose contract cannot be remotely probed yet.
  // credentialFreeEdit 标识当前尚无法远程探测契约的无账号现有供应商。
  const credentialFreeEdit = Boolean(props.provider) && providerConnections.length === 0
  // providerChanged prevents an unchanged editor from starting unnecessary validation work.
  // providerChanged 防止未发生变化的编辑器启动不必要的验证工作。
  const providerChanged = Boolean(props.provider) && (
    displayName.trim() !== props.provider?.display_name
    || connectionContractChanged
    || responsesFeaturesChanged
  )
  // directSaveOnly covers display and capability changes that do not change the credential route contract.
  // directSaveOnly 覆盖不改变凭据路由契约的展示信息与能力开关变更。
  const directSaveOnly = Boolean(props.provider) && providerChanged && !connectionContractChanged

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open && operationRunningRef.current) {
          return
        }
        props.onOpenChange(open)
      }}
      open={props.open}
    >
      <DialogContent className="flex min-h-0 flex-col" scrollMode="fixed-footer" size="lg">
        <DialogHeader>
          <DialogTitle>{t(props.provider ? "settings.customProviders.editTitle" : "settings.customProviders.addTitle")}</DialogTitle>
          <DialogDescription>
            {t("settings.customProviders.dialogDescription")}
          </DialogDescription>
        </DialogHeader>
        <fieldset className="min-h-0 flex-1 space-y-4 overflow-y-auto" disabled={probing || saving}>
        <div className="grid grid-cols-3 gap-2" aria-label={t("settings.customProviders.progressLabel")}>
          {(["connection", "credential", "result"] as const).map((candidate, index) => (
            <div className={`rounded-lg border px-3 py-2 text-xs ${candidate === step ? "border-foreground bg-muted font-medium" : "text-muted-foreground"}`} key={candidate}>
              {index + 1}. {t(customProviderStepLabelKeys[candidate])}
            </div>
          ))}
        </div>

        {step === "connection" ? (
          <div className="grid gap-4">
            <label className="settings-form-field">
              <span>{t("settings.customProviders.displayName")}</span>
              <Input onChange={(event) => setDisplayName(event.target.value)} value={displayName} />
            </label>
            <label className="settings-form-field">
              <span>{t("settings.customProviders.baseUrl")}</span>
              <Input onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://example.com/v1" value={baseUrl} />
            </label>
            <div className="grid gap-4">
              <fieldset className="settings-form-field">
                <legend className="text-sm font-medium">{t("settings.customProviders.protocols")}</legend>
                <p className="text-xs text-muted-foreground">{t("settings.customProviders.protocolsHint")}</p>
                <div className="mt-2 flex flex-wrap gap-2">
                  {customProtocolOptions.map((option) => {
                    const enabled = protocols.includes(option.id)
                    return (
                      <label
                        className={`inline-flex cursor-pointer items-center gap-2 rounded-full border px-3 py-1.5 text-sm ${enabled ? "border-foreground bg-muted font-medium" : "text-muted-foreground"}`}
                        key={option.id}
                      >
                        <Checkbox
                          checked={enabled}
                          disabled={saving}
                          onCheckedChange={() => {
                            setProtocols((current) => {
                              const next = enabled
                                ? current.filter((protocol) => protocol !== option.id)
                                : [...current, option.id]
                              // At least one protocol must stay enabled; the last protocol
                              // cannot be removed (plan §R6.4).
                              // 至少保留一个启用协议；最后一个协议不可移除（方案 §R6.4）。
                              if (next.length === 0) {
                                return current
                              }
                              if (enabled) setInvalidHeaderProtocols((invalid) => {
                                const nextInvalid = new Set(invalid)
                                nextInvalid.delete(option.id)
                                return nextInvalid
                              })
                              if (!enabled) {
                                setRouteDrafts((drafts: CustomProviderProtocolCollection["drafts"]) => ({
                                  ...drafts,
                                  [option.id]: drafts[option.id] ?? emptyProtocolRouteDraft(),
                                }))
                              }
                              return sortProtocolsByAutoPriority(next)
                            })
                          }}
                        />
                        {option.label}
                      </label>
                    )
                  })}
                </div>
              </fieldset>
              <label className="settings-form-field">
                <span>{t("settings.customProviders.authentication")}</span>
                <Select onValueChange={(value) => setAuthentication(value as "bearer_authorization" | "google_api_key" | "x_api_key")} value={authentication}>
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="bearer_authorization">Authorization: Bearer</SelectItem>
                    <SelectItem value="google_api_key">Gemini API Key</SelectItem>
                    <SelectItem value="x_api_key">x-api-key</SelectItem>
                  </SelectContent>
                </Select>
              </label>
            </div>
            {protocols.length > 1 || !headersValid ? (
              <fieldset className="settings-form-field">
                <legend className="text-sm font-medium">{t("settings.customProviders.routeDrafts")}</legend>
                <p className="text-xs text-muted-foreground">{t("settings.customProviders.routeDraftsHint")}</p>
                <div className="mt-2 grid gap-2">
                  {protocols.map((protocolId) => (
                    <CustomProtocolRouteDraftCard
                      draft={routeDrafts[protocolId] ?? emptyProtocolRouteDraft()}
                      key={protocolId}
                      onHeaderValidityChange={(valid) => setInvalidHeaderProtocols((current) => {
                        const next = new Set(current)
                        if (valid) next.delete(protocolId)
                        else next.add(protocolId)
                        return next
                      })}
                      onChange={(draft) => setRouteDrafts((drafts: CustomProviderProtocolCollection["drafts"]) => ({ ...drafts, [protocolId]: draft }))}
                      protocol={protocolId}
                      t={t}
                    />
                  ))}
                </div>
              </fieldset>
            ) : null}
            <label className="settings-form-field">
              <span>{t("settings.customProviders.discoveryMode")}</span>
              <Select onValueChange={(value) => setModelDiscoveryMode(value as PublicAiCustomProviderModelDiscoveryMode)} value={modelDiscoveryMode}>
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="standard">{t("settings.customProviders.discoveryStandard")}</SelectItem>
                  <SelectItem value="custom_path">{t("settings.customProviders.discoveryCustomPath")}</SelectItem>
                  <SelectItem value="manual">{t("settings.customProviders.discoveryManual")}</SelectItem>
                </SelectContent>
              </Select>
            </label>
            {modelDiscoveryMode === "custom_path" ? (
              <label className="settings-form-field">
                <span>{t("settings.customProviders.discoveryPath")}</span>
                <Input onChange={(event) => setModelDiscoveryPath(event.target.value)} placeholder="/models" value={modelDiscoveryPath} />
              </label>
            ) : null}
            <DisclosureSection description={t("settings.customProviders.advancedOptionsHint")} title={t("settings.customProviders.advancedOptions")}>
              <div className="grid gap-3 text-sm">
                <label className="settings-form-field">
                  <span>{t("settings.customProviders.sessionHeader")}</span>
                  <Input
                    onChange={(event) => setSessionHeader(event.target.value)}
                    placeholder="x-opencode-session"
                    value={sessionHeader}
                  />
                  <span className="text-xs text-muted-foreground">
                    {t("settings.customProviders.sessionHeaderHint")}
                  </span>
                </label>
              </div>
            </DisclosureSection>
            {protocols.includes("open_ai_codex_responses") ? (
              <div className="grid gap-3 rounded-xl border bg-muted/30 p-3 text-sm">
                <p className="font-medium text-foreground">{t("settings.customProviders.codexCapabilities")}</p>
                <label className="flex items-start gap-2">
                  <Switch
                    checked={remoteCompactionPolicy === "enabled_with_local_fallback"}
                    className="mt-1"
                    onCheckedChange={(checked) => setRemoteCompactionPolicy(checked ? "enabled_with_local_fallback" : "disabled")}
                  />
                  <span>
                    <span className="block font-medium">{t("settings.customProviders.codexRemoteCompaction")}</span>
                    <span className="block text-xs text-muted-foreground">{t("settings.customProviders.codexRemoteCompactionHint")}</span>
                  </span>
                </label>
                <label className="flex items-start gap-2">
                  <Switch
                    checked={websocketPolicy !== "disabled"}
                    className="mt-1"
                    onCheckedChange={(checked) => setWebsocketPolicy(checked ? "unverified_override" : "disabled")}
                  />
                  <span>
                    <span className="block font-medium">{t("settings.customProviders.codexWebsocket")}</span>
                    <span className="block text-xs text-muted-foreground">{t("settings.customProviders.codexWebsocketHint")}</span>
                  </span>
                </label>
              </div>
            ) : null}
            <div className="rounded-xl border bg-muted/30 p-3 text-xs text-muted-foreground">
              <p><span className="font-medium text-foreground">{t("settings.customProviders.protocolEndpoint")}</span> {protocolEndpoint || "—"}</p>
              <p className="mt-1"><span className="font-medium text-foreground">{t("settings.customProviders.modelsEndpoint")}</span> {discoveryEndpoint || t("settings.customProviders.manualOnly")}</p>
            </div>
            {credentialFreeEdit ? (
              <Alert>
                <CircleAlert className="size-4" />
                <AlertTitle>{t("settings.customProviders.credentialFreeEditTitle")}</AlertTitle>
                <AlertDescription>{t("settings.customProviders.credentialFreeEditDescription")}</AlertDescription>
              </Alert>
            ) : null}
          </div>
        ) : null}

        {step === "credential" ? (
          <div className="grid gap-4">
            <Alert>
              <CircleAlert className="size-4" />
              <AlertTitle>{t("settings.customProviders.validationTitle")}</AlertTitle>
              <AlertDescription>{t(props.provider ? "settings.customProviders.validationEditDescription" : "settings.customProviders.validationCreateDescription")}</AlertDescription>
            </Alert>
            {props.provider ? (
              <label className="settings-form-field">
                <span>{t("settings.customProviders.validationCredential")}</span>
                <Select onValueChange={setValidationConnectionId} value={validationConnectionId}>
                  <SelectTrigger><SelectValue placeholder={t("settings.customProviders.validationCredentialRequired")} /></SelectTrigger>
                  <SelectContent>
                    {providerConnections.map((connection) => (
                      <SelectItem key={connection.id} value={connection.id}>
                        {connection.display_name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
            ) : (
              <>
                <label className="settings-form-field">
                  <span>{t("settings.customProviders.firstCredentialName")}</span>
                  <Input onChange={(event) => setCredentialDisplayName(event.target.value)} value={credentialDisplayName} />
                </label>
                <label className="settings-form-field">
                  <span>{t("settings.customProviders.apiKey")}</span>
                  <Input autoComplete="off" onChange={(event) => setSecret(event.target.value)} placeholder={t("settings.customProviders.apiKeyCreatePlaceholder")} type="password" value={secret} />
                </label>
              </>
            )}
          </div>
        ) : null}

        {step === "result" && probeResult ? (
          <div className="grid gap-4">
            <Alert variant={probeResult.status.state === "authentication_failed" ? "destructive" : "default"}>
              {acceptedWithoutOverride ? <CircleCheckBig className="size-4" /> : <CircleAlert className="size-4" />}
              <AlertTitle>{t(`settings.customModels.discovery.${discoveryStateTranslationSegment(probeResult.status.state)}`)}</AlertTitle>
              <AlertDescription>{probeResult.status.message || t("settings.customProviders.validationResultDescription")}</AlertDescription>
            </Alert>
            <div className="grid gap-3 rounded-xl border p-4 sm:grid-cols-2">
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationModelCount")}</p><p className="font-medium">{probeResult.status.model_count}</p></div>
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationPageCount")}</p><p className="font-medium">{probeResult.catalog?.page_count ?? 0}</p></div>
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationCredential")}</p><p className="truncate font-medium">{validationConnection?.display_name ?? credentialDisplayName.trim()}</p></div>
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationObservedAt")}</p><p className="font-medium">{formatCustomProviderObservationTime(probeResult.status.observed_at_ms, locale)}</p></div>
            </div>
            {probeResult.catalog && probeResult.catalog.models.length > 0 ? (
              <CustomProviderProbeCatalogPreview result={probeResult} />
            ) : null}
            {probeResult.manualFallbackAvailable ? <p className="text-sm text-muted-foreground">{t("settings.customProviders.manualFallbackHint")}</p> : null}
            {probeResult.pendingSaveAvailable ? <p className="text-sm text-muted-foreground">{t("settings.customProviders.pendingSaveHint")}</p> : null}
          </div>
        ) : null}

        {error ? <p className="settings-muted-line settings-error-text" role="alert">{error}</p> : null}
        </fieldset>
        <DialogFooter>
          {step === "connection" ? <Button disabled={probing || saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.cancel")}</Button> : <Button disabled={probing || saving} onClick={() => setStep(step === "result" ? "credential" : "connection")} type="button" variant="outline">{t("settings.customProviders.back")}</Button>}
          {step === "connection" && props.provider && !providerChanged ? <Button disabled type="button">{t("memory.common.save")}</Button> : null}
          {step === "connection" && providerChanged && (directSaveOnly || credentialFreeEdit) ? <Button disabled={saving || !headersValid} onClick={credentialFreeEdit ? saveCredentialFreeEdit : () => void save()} type="button">{saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t("memory.common.save")}</Button> : null}
          {step === "connection" && (!props.provider || (providerChanged && !directSaveOnly && !credentialFreeEdit)) ? <Button disabled={!headersValid} onClick={continueToCredential} type="button">{t("settings.customProviders.next")}</Button> : null}
          {step === "credential" ? <Button disabled={probing} onClick={() => void probe()} type="button">{probing ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t(probing ? "settings.customProviders.probing" : "settings.customProviders.probe")}</Button> : null}
          {step === "result" && acceptedWithoutOverride ? <Button disabled={saving} onClick={() => void save()} type="button">{t(saving ? "settings.customProviders.saving" : "settings.customProviders.saveVerified")}</Button> : null}
          {step === "result" && probeResult?.manualFallbackAvailable ? <Button disabled={saving} onClick={() => setPendingConfirmation("manual")} type="button">{t(saving ? "settings.customProviders.saving" : "settings.customProviders.saveManual")}</Button> : null}
          {step === "result" && probeResult?.pendingSaveAvailable ? <Button disabled={saving} onClick={() => setPendingConfirmation("pending")} type="button" variant="secondary">{t(saving ? "settings.customProviders.saving" : "settings.customProviders.savePending")}</Button> : null}
        </DialogFooter>
        <AlertDialog
          onOpenChange={(open) => {
            if (!open && !saving) setPendingConfirmation(null)
          }}
          open={pendingConfirmation !== null}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("settings.customProviders.pendingConfirmTitle")}</AlertDialogTitle>
              <AlertDialogDescription>{t("settings.customProviders.pendingConfirmDescription")}</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={saving}>{t("memory.common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                disabled={saving || pendingConfirmation === null}
                onClick={() => {
                  if (pendingConfirmation) void save(pendingConfirmation)
                }}
              >
                {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
                {t("settings.customProviders.confirmPending")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
        <AlertDialog
          onOpenChange={(open) => {
            if (!open && !saving) setProtocolOverrideConfirmation(null)
          }}
          open={protocolOverrideConfirmation !== null}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("settings.customProviders.protocolOverridesConfirmTitle")}</AlertDialogTitle>
              <AlertDialogDescription>
                {t("settings.customProviders.protocolOverridesConfirmDescription")
                  .replace("{count}", String(inactiveProtocolOverrideCount))
                  .replace("{protocol}", removedProtocols
                    .map((removed) => protocolDisplayName(customProtocolProfileId(removed)))
                    .join(", ") || t("settings.customProviders.protocol"))}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={saving}>{t("memory.common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                disabled={saving || protocolOverrideConfirmation === null}
                onClick={() => {
                  const acceptance = protocolOverrideConfirmation === "ready"
                    ? undefined
                    : protocolOverrideConfirmation ?? undefined
                  void save(acceptance, true)
                }}
              >
                {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
                {t("settings.customProviders.protocolOverridesConfirmAction")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  )
}

// CustomProviderProbeCatalogPreview confirms discovered model identities before persistence.
// CustomProviderProbeCatalogPreview 在持久化前确认已发现的模型身份。
function CustomProviderProbeCatalogPreview(props: { result: PublicAiCustomProviderProbeResult }) {
  const { t } = useI18n()
  // models is the exact credential-scoped probe catalog and is never a persisted local copy.
  // models 是精确凭据作用域探测目录，绝不是持久化的本地副本。
  const models = props.result.catalog?.models ?? []
  return (
    <section className="overflow-hidden rounded-xl border">
      <div className="flex items-center justify-between gap-3 border-b bg-muted/30 px-4 py-2.5">
        <div className="min-w-0">
          <h3 className="text-sm font-medium">{t("settings.customProviders.validationModelsTitle")}</h3>
          <p className="truncate font-mono text-[11px] text-muted-foreground">
            {props.result.catalog?.discovery_schema_revision}
          </p>
        </div>
        <Badge variant="secondary">{models.length}</Badge>
      </div>
      <ScrollArea className="h-52">
        <div className="divide-y">
          {models.map((model) => (
            <div className="flex items-center justify-between gap-3 px-4 py-2.5" key={model.id}>
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">{model.display_name}</p>
                <p className="truncate font-mono text-xs text-muted-foreground">{model.id}</p>
              </div>
              <Badge variant={modelEvidenceCompleteness(model.capability_evidence) === 4 ? "secondary" : "outline"}>
                {t("settings.customProviders.validationEvidence")
                  .replace("{known}", String(modelEvidenceCompleteness(model.capability_evidence)))
                  .replace("{total}", "4")}
              </Badge>
            </div>
          ))}
        </div>
      </ScrollArea>
      <p className="border-t bg-muted/20 px-4 py-2 text-xs text-muted-foreground">
        {t("settings.customProviders.validationModelsHint")}
      </p>
    </section>
  )
}

// modelEvidenceCompleteness counts only explicitly declared capability dimensions.
// modelEvidenceCompleteness 仅统计上游明确声明的能力维度。
function modelEvidenceCompleteness(evidence: PublicAiModelCapabilityEvidence | undefined): number {
  if (!evidence) {
    return 0
  }
  return [
    evidence.context_limit_declared,
    evidence.output_limit_declared,
    evidence.input_modalities_declared,
    evidence.output_modalities_declared,
  ].filter(Boolean).length
}

// formatCustomProviderObservationTime renders a stable local timestamp for one probe.
// formatCustomProviderObservationTime 为一次探测渲染稳定的本地时间戳。
function formatCustomProviderObservationTime(observedAtMs: number, locale: string): string {
  return observedAtMs > 0 ? formatDateTime(observedAtMs, locale) ?? "—" : "—"
}

// CustomProviderConnectionDraft contains fields validated before credential collection.
// CustomProviderConnectionDraft 包含收集凭据前需要校验的字段。
interface CustomProviderConnectionDraft {
  baseUrl: string
  displayName: string
  modelDiscoveryMode: PublicAiCustomProviderModelDiscoveryMode
  modelDiscoveryPath: string
}

// validateCustomProviderConnectionStep returns the exact localized error key for invalid endpoint input.
// validateCustomProviderConnectionStep 为无效入口输入返回精确的本地化错误键。
//
// Parameters
// 参数
// `draft` is the provider identity, base URL, and model-discovery selection entered by the user.
// `draft` 是用户填写的供应商身份、基础 URL 与模型发现选择。
//
// Returns
// 返回值
// The corresponding translation key, or `null` when the first step is valid.
// 返回对应翻译键；第一步有效时返回 `null`。
function validateCustomProviderConnectionStep(draft: CustomProviderConnectionDraft): TranslationKey | null {
  if (!draft.displayName.trim()) {
    return "settings.customProviders.displayNameRequired"
  }
  try {
    const parsed = new URL(draft.baseUrl.trim())
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
      return "settings.customProviders.baseUrlInvalid"
    }
  } catch {
    return "settings.customProviders.baseUrlInvalid"
  }
  if (draft.modelDiscoveryMode === "custom_path") {
    const path = draft.modelDiscoveryPath.trim()
    if (!path.startsWith("/") || path.includes("://") || path.split("/").includes("..")) {
      return "settings.customProviders.discoveryPathInvalid"
    }
  }
  return null
}

// customProviderDiscoveryMode reconstructs the persisted discovery choice from one exact service entry.
// customProviderDiscoveryMode 从一个精确服务入口重建持久化的发现选择。
//
// Parameters
// 参数
// `entry` is the single custom-provider service entry, when editing an existing definition.
// `entry` 是编辑现有定义时的唯一自定义供应商服务入口。
//
// Returns
// 返回值
// The user-facing discovery mode represented by the runtime descriptor.
// 返回运行时描述所表示的用户发现模式。
function customProviderDiscoveryMode(entry: PublicAiServiceEntry | undefined): PublicAiCustomProviderModelDiscoveryMode {
  if (entry?.model_discovery === "manual") {
    return "manual"
  }
  if (entry?.protocol_routes[0]?.model_discovery_path) {
    return "custom_path"
  }
  return "standard"
}

// customProviderAuthentication maps one persisted entry to the editable authentication contract.
// customProviderAuthentication 将一个持久化入口映射为可编辑鉴权契约。
//
// Parameters
// 参数
// `entry` is the exact persisted custom-provider service entry.
// `entry` 是精确的持久化自定义供应商服务入口。
//
// Returns
// 返回值
// The single supported browser authentication choice.
// 返回唯一受支持的浏览器鉴权选择。
function customProviderAuthentication(
  entry: PublicAiServiceEntry | undefined,
): "bearer_authorization" | "google_api_key" | "x_api_key" {
  if (entry?.authentication === "google_api_key") {
    return "google_api_key"
  }
  return entry?.authentication === "x_api_key" ? "x_api_key" : "bearer_authorization"
}

// customProviderEndpointPreview joins the exact protocol path to the user-entered base URL.
// customProviderEndpointPreview 将精确协议路径拼接到用户输入的基础 URL。
//
// Parameters
// 参数
// `baseUrl` is the unpersisted service origin and `protocol` selects one fixed route path.
// `baseUrl` 是尚未持久化的服务源，`protocol` 选择一条固定路由路径。
//
// Returns
// 返回值
// A display-only endpoint preview, or an empty string when no base URL exists.
// 返回仅用于展示的端点预览；没有基础 URL 时返回空字符串。
function customProviderEndpointPreview(baseUrl: string, protocol: PublicAiCustomProviderProtocol): string {
  if (protocol === "gemini_generate_content") {
    return joinCustomProviderUrl(baseUrl, "/models/{model}:streamGenerateContent?alt=sse")
  }
  const path = protocol === "anthropic_messages"
    ? "/messages"
    : protocol === "open_ai_chat"
      ? "/chat/completions"
      : "/responses"
  return joinCustomProviderUrl(baseUrl, path)
}

// customProviderDiscoveryEndpointPreview returns the selected model-catalog endpoint preview.
// customProviderDiscoveryEndpointPreview 返回所选模型目录端点预览。
//
// Parameters
// 参数
// `baseUrl` is the service origin, `mode` is the discovery strategy, and `customPath` is the optional declared path.
// `baseUrl` 是服务源，`mode` 是发现策略，`customPath` 是可选的声明路径。
//
// Returns
// 返回值
// A display-only URL, or an empty string for manual discovery.
// 返回仅用于展示的 URL；手工发现时返回空字符串。
function customProviderDiscoveryEndpointPreview(
  baseUrl: string,
  mode: PublicAiCustomProviderModelDiscoveryMode,
  customPath: string,
): string {
  if (mode === "manual") {
    return ""
  }
  return joinCustomProviderUrl(baseUrl, mode === "custom_path" ? customPath : "/models")
}

// joinCustomProviderUrl normalizes one display-only base/path pair without changing persisted values.
// joinCustomProviderUrl 规范化一组仅用于展示的基础 URL 与路径且不修改持久化值。
//
// Parameters
// 参数
// `baseUrl` is the raw base and `path` is one same-origin absolute endpoint path.
// `baseUrl` 是原始基础 URL，`path` 是一个同源绝对端点路径。
//
// Returns
// 返回值
// The joined preview URL, or an empty string when either side is incomplete.
// 返回拼接后的预览 URL；任一侧不完整时返回空字符串。
function joinCustomProviderUrl(baseUrl: string, path: string): string {
  const normalizedBase = baseUrl.trim().replace(/\/+$/, "")
  const normalizedPath = path.trim()
  if (!normalizedBase || !normalizedPath) {
    return ""
  }
  return `${normalizedBase}/${normalizedPath.replace(/^\/+/, "")}`
}

// discoveryStateTranslationSegment maps the closed backend state to its translation-key suffix.
// discoveryStateTranslationSegment 将封闭后端状态映射为翻译键后缀。
//
// Parameters
// 参数
// `state` is one serialized discovery state returned by the service.
// `state` 是服务返回的一项序列化发现状态。
//
// Returns
// 返回值
// The exact suffix registered in the i18n catalog.
// 返回 i18n 目录中注册的精确后缀。
function discoveryStateTranslationSegment(
  state: PublicAiCustomProviderProbeResult["status"]["state"],
): "notChecked" | "ready" | "empty" | "unsupported" | "authenticationFailed" | "rateLimited" | "temporarilyUnavailable" | "invalidResponse" | "stale" | "failed" {
  return discoveryStateTranslationSegments[state]
}

// customProtocolOptions is the complete browser-editable compatibility protocol set.
// customProtocolOptions 是浏览器可编辑的完整兼容协议集合。
const customProtocolOptions: Array<{ id: PublicAiCustomProviderProtocol; label: string }> = [
  { id: "anthropic_messages", label: "Anthropic Messages" },
  { id: "gemini_generate_content", label: "Gemini Generate Content" },
  { id: "open_ai_chat", label: "OpenAI Chat Completions" },
  { id: "open_ai_responses", label: "OpenAI Responses" },
  { id: "open_ai_codex_responses", label: "Codex Responses (full compatibility)" },
]

// customProviderStepLabelKeys keeps step labels inside the closed translation-key contract.
// customProviderStepLabelKeys 将步骤标签保持在封闭翻译键契约内。
const customProviderStepLabelKeys = {
  connection: "settings.customProviders.step.connection",
  credential: "settings.customProviders.step.credential",
  result: "settings.customProviders.step.result",
} as const

// discoveryStateTranslationSegments maps serialized states to camel-cased i18n suffixes.
// discoveryStateTranslationSegments 将序列化状态映射为驼峰形式的 i18n 后缀。
const discoveryStateTranslationSegments = {
  not_checked: "notChecked",
  ready: "ready",
  empty: "empty",
  unsupported: "unsupported",
  authentication_failed: "authenticationFailed",
  rate_limited: "rateLimited",
  temporarily_unavailable: "temporarilyUnavailable",
  invalid_response: "invalidResponse",
  stale: "stale",
  failed: "failed",
} as const
