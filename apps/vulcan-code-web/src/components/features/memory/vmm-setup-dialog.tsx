import * as React from "react"
import {
  BrainCircuit,
  Check,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  LoaderCircle,
  Server,
  ShieldCheck,
} from "lucide-react"

import { VmmInferenceSettings } from "@/components/features/memory/vmm-inference-settings"
import { formatSkillText } from "@/components/features/featureText"
import { Alert, AlertDescription } from "@/components/ui/alert"
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
import { Progress } from "@/components/ui/progress"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useI18n } from "@/i18n"
import type {
  VmmNativeProjectCatalogSummary,
  VmmProjectBindingPlanEntry,
  VmmProjectScopeSummary,
  VmmProtocolProjectEntry,
  VmmProtocolUserEntry,
} from "@/types"

// VmmQuickSetupMode identifies the mutually exclusive VMM backend.
// VmmQuickSetupMode 标识互斥的 VMM 后端。
type VmmQuickSetupMode = "native" | "external"

// VmmProjectBindingMode identifies the default binding policy for new VulcanCode projects.
// VmmProjectBindingMode 标识新建 VulcanCode 项目的默认绑定策略。
type VmmProjectBindingMode = "fixed" | "per_project"

// VmmQuickSetupConnectionInput contains only the connection values required to load a catalog.
// VmmQuickSetupConnectionInput 只包含加载目录所需的连接值。
interface VmmQuickSetupConnectionInput {
  mode: VmmQuickSetupMode
  externalEndpoint: string
  externalApiKey: string
  externalManagementBaseUrl: string
  externalManagementToken: string
}

// VmmQuickSetupCatalog contains backend-specific projects available to the binding step.
// VmmQuickSetupCatalog 包含绑定步骤可用的后端专属项目。
interface VmmQuickSetupCatalog {
  native?: VmmNativeProjectCatalogSummary | null
  externalUsers?: VmmProtocolUserEntry[]
  externalProjects?: VmmProtocolProjectEntry[]
}

// VmmQuickSetupInput contains every visible decision committed by the setup workflow.
// VmmQuickSetupInput 包含快速配置流程提交的全部可见决定。
interface VmmQuickSetupInput extends VmmQuickSetupConnectionInput {
  bindingMode: VmmProjectBindingMode
  userName: string
  nativeFixedProjectId: number
  externalProjectPrefix: string
  externalFixedAction: "create_and_bind" | "bind_existing"
  externalFixedProjectName: string
  externalFixedTargetProjectId?: number | null
  bindingPlan: VmmProjectBindingPlanEntry[]
}

// VmmSetupDialogProps defines controlled visibility and backend preparation actions.
// VmmSetupDialogProps 定义受控可见性与后端准备操作。
interface VmmSetupDialogProps {
  open: boolean
  busy: boolean
  projects: VmmProjectScopeSummary[]
  notice: string
  // Opens credential management while keeping VMM disabled.
  // 在保持 VMM 禁用时打开凭据管理。
  onOpenCredentialManagement: () => void
  onOpenChange: (open: boolean) => void
  onPrepare: (input: VmmQuickSetupConnectionInput) => Promise<VmmQuickSetupCatalog | null>
  onSubmit: (input: VmmQuickSetupInput) => Promise<boolean>
}

// EditableBindingDecision owns one unbound project's explicit setup decision.
// EditableBindingDecision 持有一个未绑定项目的显式配置决定。
interface EditableBindingDecision {
  action: "create_and_bind" | "bind_existing"
  projectName: string
  targetProjectId: string
}

// FIXED_TARGET_SENTINEL represents a row that still follows the current global fixed-project choice.
// FIXED_TARGET_SENTINEL 表示仍跟随当前全局固定项目选择的配置行。
const FIXED_TARGET_SENTINEL = "__global_fixed_vmm_project__"

// setupBindingTargetProjectId resolves one visible bind-existing decision for service submission.
// setupBindingTargetProjectId 为服务提交解析一条可见的绑定已有项目决定。
//
// Parameters: mode selects native or external semantics, bindingMode identifies whether the row
// can inherit a fixed default, decision is the exact row state, and nativeFixedProjectId is the
// currently selected native default rather than the value present when rows were initialized.
// 参数：mode 选择原生或外置语义，bindingMode 标识该行能否继承固定默认值，decision 是准确的行状态，
// nativeFixedProjectId 是当前选择的原生默认值，而不是初始化配置行时的旧值。
//
// Returns: the stable selected ID, the current native fixed ID, or null for an external fixed
// project whose ID will be materialized only after remote creation.
// 返回：稳定的已选 ID、当前原生固定 ID，或尚待远端创建后物化 ID 的外置固定项目所对应的 null。
export function setupBindingTargetProjectId(input: {
  // Selected backend semantics.
  // 已选择的后端语义。
  mode: VmmQuickSetupMode
  // Global binding mode shown in the setup dialog.
  // 配置向导中显示的全局绑定模式。
  bindingMode: VmmProjectBindingMode
  // Exact row decision, absent only before initialization completes.
  // 准确的逐行决定，仅在初始化完成前可能缺失。
  decision?: EditableBindingDecision
  // Current native fixed project identity.
  // 当前原生固定项目身份。
  nativeFixedProjectId: string
}): number | null {
  if (input.decision?.action !== "bind_existing") return null
  if (
    input.bindingMode === "fixed"
    && input.decision.targetProjectId === FIXED_TARGET_SENTINEL
  ) {
    return input.mode === "native" ? Number(input.nativeFixedProjectId) : null
  }
  return Number(input.decision.targetProjectId)
}

// VmmSetupDialog guides the user through backend, inference, binding, and confirmation.
// VmmSetupDialog 引导用户完成后端、推理、绑定与确认配置。
function VmmSetupDialog({
  open,
  busy,
  projects,
  notice,
  onOpenCredentialManagement,
  onOpenChange,
  onPrepare,
  onSubmit,
}: VmmSetupDialogProps) {
  const { t } = useI18n()
  const [step, setStep] = React.useState(0)
  const [mode, setMode] = React.useState<VmmQuickSetupMode>("native")
  const [bindingMode, setBindingMode] = React.useState<VmmProjectBindingMode>("fixed")
  const [userName, setUserName] = React.useState("Vulcan Code")
  const [nativeFixedProjectId, setNativeFixedProjectId] = React.useState("1")
  const [externalProjectPrefix, setExternalProjectPrefix] = React.useState("vulcan/space")
  const [externalEndpoint, setExternalEndpoint] = React.useState("")
  const [externalApiKey, setExternalApiKey] = React.useState("")
  const [externalManagementBaseUrl, setExternalManagementBaseUrl] = React.useState("")
  const [externalManagementToken, setExternalManagementToken] = React.useState("")
  const [externalFixedAction, setExternalFixedAction] = React.useState<"create_and_bind" | "bind_existing">("create_and_bind")
  const [externalFixedProjectName, setExternalFixedProjectName] = React.useState("default")
  const [externalFixedTargetProjectId, setExternalFixedTargetProjectId] = React.useState("")
  const [catalog, setCatalog] = React.useState<VmmQuickSetupCatalog | null>(null)
  const [decisions, setDecisions] = React.useState<Record<string, EditableBindingDecision>>({})
  const [inferenceReady, setInferenceReady] = React.useState(false)
  const [failed, setFailed] = React.useState(false)
  const [preparing, setPreparing] = React.useState(false)
  const [submitting, setSubmitting] = React.useState(false)

  React.useEffect(() => {
    if (!open) return
    setStep(0)
    setMode("native")
    setBindingMode("fixed")
    setUserName("Vulcan Code")
    setNativeFixedProjectId("1")
    setExternalProjectPrefix("vulcan/space")
    setExternalEndpoint("")
    setExternalApiKey("")
    setExternalManagementBaseUrl("")
    setExternalManagementToken("")
    setExternalFixedAction("create_and_bind")
    setExternalFixedProjectName("default")
    setExternalFixedTargetProjectId("")
    setCatalog(null)
    setDecisions({})
    setInferenceReady(false)
    setFailed(false)
    setPreparing(false)
    setSubmitting(false)
  }, [open])

  const nativeBindings = catalog?.native?.bindings ?? []
  const unboundProjects = mode === "native"
    ? projects.filter((project) => !nativeBindings.some((binding) => binding.vulcanProjectId === project.projectId))
    : projects.filter((project) => !project.boundProjectId)
  const nativeProjects = catalog?.native?.projects ?? []
  const externalProjects = catalog?.externalProjects ?? []
  const effectiveBusy = busy || preparing || submitting
  const connectionComplete = mode === "native"
    || (externalEndpoint.trim().length > 0
      && externalManagementBaseUrl.trim().length > 0
      && externalManagementToken.trim().length > 0)

  // initializeDecisions creates one explicit row for every currently unbound project.
  // initializeDecisions 为每个当前未绑定项目创建一条显式决定。
  const initializeDecisions = React.useCallback((
    nextMode: VmmQuickSetupMode,
    nextBindingMode: VmmProjectBindingMode,
    nextCatalog: VmmQuickSetupCatalog,
  ) => {
    const boundIds = new Set((nextCatalog.native?.bindings ?? []).map((binding) => binding.vulcanProjectId))
    const nextUnbound = nextMode === "native"
      ? projects.filter((project) => !boundIds.has(project.projectId))
      : projects.filter((project) => !project.boundProjectId)
    const nextDecisions: Record<string, EditableBindingDecision> = {}
    for (const project of nextUnbound) {
      const bindFixed = nextBindingMode === "fixed"
      nextDecisions[project.projectId] = {
        action: bindFixed ? "bind_existing" : "create_and_bind",
        projectName: project.projectId,
        targetProjectId: bindFixed ? FIXED_TARGET_SENTINEL : "",
      }
    }
    setDecisions(nextDecisions)
  }, [nativeFixedProjectId, projects])

  // handlePrepare verifies the selected backend and loads the authoritative binding catalog.
  // handlePrepare 校验所选后端并加载权威绑定目录。
  const handlePrepare = React.useCallback(async () => {
    if (!connectionComplete || effectiveBusy) return
    setFailed(false)
    setPreparing(true)
    try {
      const nextCatalog = await onPrepare({
        mode,
        externalEndpoint: externalEndpoint.trim(),
        externalApiKey,
        externalManagementBaseUrl: externalManagementBaseUrl.trim(),
        externalManagementToken,
      })
      if (!nextCatalog) {
        setFailed(true)
        return
      }
      setCatalog(nextCatalog)
      initializeDecisions(mode, bindingMode, nextCatalog)
      setStep(mode === "native" ? 1 : 2)
    } finally {
      setPreparing(false)
    }
  }, [
    bindingMode,
    connectionComplete,
    effectiveBusy,
    externalApiKey,
    externalEndpoint,
    externalManagementBaseUrl,
    externalManagementToken,
    initializeDecisions,
    mode,
    onPrepare,
  ])

  // updateBindingMode resets row defaults so the visible global policy and row decisions agree.
  // updateBindingMode 重置行默认值，使可见全局策略与逐行决定一致。
  const updateBindingMode = React.useCallback((nextMode: VmmProjectBindingMode) => {
    setBindingMode(nextMode)
    if (catalog) initializeDecisions(mode, nextMode, catalog)
  }, [catalog, initializeDecisions, mode])

  // updateDecision applies one exact row edit without changing any neighboring project.
  // updateDecision 应用一条准确行编辑且不改变其他项目。
  const updateDecision = React.useCallback((
    vulcanProjectId: string,
    patch: Partial<EditableBindingDecision>,
  ) => {
    setDecisions((current) => ({
      ...current,
      [vulcanProjectId]: { ...current[vulcanProjectId], ...patch },
    }))
  }, [])

  const bindingPlan = unboundProjects.map((project): VmmProjectBindingPlanEntry => {
    const decision = decisions[project.projectId]
    return {
      vulcanProjectId: project.projectId,
      action: decision?.action ?? "create_and_bind",
      projectName: decision?.action === "create_and_bind" ? decision.projectName.trim() : null,
      targetProjectId: decision?.action === "bind_existing"
        ? setupBindingTargetProjectId({ mode, bindingMode, decision, nativeFixedProjectId })
        : null,
    }
  })
  const rowsComplete = bindingPlan.every((entry) => (
    entry.action === "create_and_bind"
      ? Boolean(entry.projectName)
      : Boolean(entry.targetProjectId && entry.targetProjectId > 0)
        || (mode === "external" && bindingMode === "fixed" && entry.targetProjectId == null)
  ))
  const prefixComplete = externalProjectPrefix.split("/").filter(Boolean).length === 2
  const externalFixedComplete = externalFixedAction === "create_and_bind"
    ? Boolean(externalFixedProjectName.trim())
    : Number(externalFixedTargetProjectId) > 0
  const bindingComplete = mode === "native"
    ? Number(nativeFixedProjectId) > 0 && rowsComplete
    : Boolean(userName.trim())
      && prefixComplete
      && rowsComplete
      && (bindingMode !== "fixed" || externalFixedComplete)
  const canAdvance = step === 1 ? inferenceReady : step === 2 ? bindingComplete : true
  const canSubmit = Boolean(catalog)
    && (mode === "external" || inferenceReady)
    && bindingComplete
    && !effectiveBusy

  // handleSubmit commits exactly the decisions shown in the confirmation step.
  // handleSubmit 精确提交确认步骤展示的决定。
  const handleSubmit = React.useCallback(async () => {
    if (!canSubmit) return
    setFailed(false)
    setSubmitting(true)
    try {
      const completed = await onSubmit({
        mode,
        bindingMode,
        userName: userName.trim(),
        nativeFixedProjectId: Number(nativeFixedProjectId),
        externalProjectPrefix: externalProjectPrefix.trim(),
        externalFixedAction,
        externalFixedProjectName: externalFixedProjectName.trim(),
        externalFixedTargetProjectId: externalFixedAction === "bind_existing"
          ? Number(externalFixedTargetProjectId)
          : null,
        bindingPlan,
        externalEndpoint: externalEndpoint.trim(),
        externalApiKey,
        externalManagementBaseUrl: externalManagementBaseUrl.trim(),
        externalManagementToken,
      })
      if (completed) {
        onOpenChange(false)
        return
      }
      setFailed(true)
    } finally {
      setSubmitting(false)
    }
  }, [
    bindingMode,
    bindingPlan,
    canSubmit,
    externalApiKey,
    externalEndpoint,
    externalFixedAction,
    externalFixedProjectName,
    externalFixedTargetProjectId,
    externalManagementBaseUrl,
    externalManagementToken,
    externalProjectPrefix,
    mode,
    nativeFixedProjectId,
    onOpenChange,
    onSubmit,
    userName,
  ])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <BrainCircuit className="size-5" aria-hidden="true" />
            {t("vmm.setup.title")}
          </DialogTitle>
          <DialogDescription>
            {formatSkillText(t("vmm.setup.stepLabel"), { current: step + 1, total: 4 })}
          </DialogDescription>
        </DialogHeader>

        <Progress value={(step + 1) * 25} />
        <div className="min-h-80 py-1">
          {step === 0 ? (
            <SetupModeStep
              apiKey={externalApiKey}
              endpoint={externalEndpoint}
              managementBaseUrl={externalManagementBaseUrl}
              managementToken={externalManagementToken}
              mode={mode}
              onApiKeyChange={setExternalApiKey}
              onEndpointChange={setExternalEndpoint}
              onManagementBaseUrlChange={setExternalManagementBaseUrl}
              onManagementTokenChange={setExternalManagementToken}
              onModeChange={setMode}
            />
          ) : null}
          {step === 1 ? (
            <div className="space-y-3">
              <SetupStepHeading
                description={t("vmm.setup.inference.description")}
                title={t("vmm.setup.inference.title")}
              />
              <VmmInferenceSettings onRequiredRouteReady={setInferenceReady} />
              {!inferenceReady ? (
                <Alert>
                  <CircleAlert aria-hidden="true" />
                  <AlertDescription className="flex flex-col items-start gap-2 sm:flex-row sm:items-center sm:justify-between">
                    <span>{t("vmm.setup.inference.required")}</span>
                    <Button
                      onClick={() => {
                        onOpenChange(false)
                        onOpenCredentialManagement()
                      }}
                      size="sm"
                      type="button"
                      variant="outline"
                    >
                      {t("vmm.setup.inference.openCredentials")}
                    </Button>
                  </AlertDescription>
                </Alert>
              ) : null}
            </div>
          ) : null}
          {step === 2 ? (
            <SetupBindingsStep
              bindingMode={bindingMode}
              decisions={decisions}
              externalFixedAction={externalFixedAction}
              externalFixedProjectName={externalFixedProjectName}
              externalFixedTargetProjectId={externalFixedTargetProjectId}
              externalProjectPrefix={externalProjectPrefix}
              externalProjects={externalProjects}
              mode={mode}
              nativeFixedProjectId={nativeFixedProjectId}
              nativeProjects={nativeProjects}
              onBindingModeChange={updateBindingMode}
              onDecisionChange={updateDecision}
              onExternalFixedActionChange={setExternalFixedAction}
              onExternalFixedProjectNameChange={setExternalFixedProjectName}
              onExternalFixedTargetProjectIdChange={setExternalFixedTargetProjectId}
              onExternalProjectPrefixChange={setExternalProjectPrefix}
              onNativeFixedProjectIdChange={setNativeFixedProjectId}
              onUserNameChange={setUserName}
              projects={unboundProjects}
              userName={userName}
            />
          ) : null}
          {step === 3 ? (
            <SetupConfirmationStep
              bindingMode={bindingMode}
              mode={mode}
              projectCount={unboundProjects.length}
            />
          ) : null}

          {effectiveBusy ? (
            <p className="mt-4 flex items-center gap-2 text-sm text-muted-foreground">
              <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
              {t("vmm.setup.running")}
            </p>
          ) : null}
          {failed ? (
            <Alert className="mt-4" variant="destructive">
              <AlertDescription>
                {t("vmm.setup.failed")}
                {notice ? <span className="mt-1 block text-xs">{notice}</span> : null}
              </AlertDescription>
            </Alert>
          ) : null}
        </div>

        <DialogFooter className="sm:justify-between">
          <Button
            disabled={effectiveBusy}
            onClick={() => {
              if (step === 0) onOpenChange(false)
              else if (step === 2 && mode === "external") setStep(0)
              else setStep((current) => current - 1)
            }}
            type="button"
            variant="outline"
          >
            {step === 0 ? null : <ChevronLeft className="size-4" aria-hidden="true" />}
            {step === 0 ? t("vmm.setup.cancel") : t("vmm.setup.previous")}
          </Button>
          {step < 3 ? (
            <Button
              disabled={effectiveBusy || (step === 0 ? !connectionComplete : !canAdvance)}
              onClick={() => {
                if (step === 0) void handlePrepare()
                else setStep((current) => current + 1)
              }}
              type="button"
            >
              {t("vmm.setup.next")}
              <ChevronRight className="size-4" aria-hidden="true" />
            </Button>
          ) : (
            <Button disabled={!canSubmit} onClick={() => void handleSubmit()} type="button">
              {effectiveBusy ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Check className="size-4" aria-hidden="true" />}
              {t("vmm.setup.submit")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// SetupModeStep renders native or external ownership and only external connection fields.
// SetupModeStep 渲染原生或外置模式，并且只在外置模式显示连接字段。
function SetupModeStep({
  apiKey,
  endpoint,
  managementBaseUrl,
  managementToken,
  mode,
  onApiKeyChange,
  onEndpointChange,
  onManagementBaseUrlChange,
  onManagementTokenChange,
  onModeChange,
}: {
  apiKey: string
  endpoint: string
  managementBaseUrl: string
  managementToken: string
  mode: VmmQuickSetupMode
  onApiKeyChange: (value: string) => void
  onEndpointChange: (value: string) => void
  onManagementBaseUrlChange: (value: string) => void
  onManagementTokenChange: (value: string) => void
  onModeChange: (value: VmmQuickSetupMode) => void
}) {
  const { t } = useI18n()
  return (
    <div className="space-y-4">
      <SetupStepHeading description={t("vmm.setup.mode.description")} title={t("vmm.setup.mode.title")} />
      <RadioGroup className="grid gap-3 sm:grid-cols-2" onValueChange={(value) => onModeChange(value as VmmQuickSetupMode)} value={mode}>
        <label className="flex cursor-pointer items-start gap-3 rounded-xl border p-4">
          <RadioGroupItem value="native" />
          <span>
            <span className="flex items-center gap-2 font-medium">
              <BrainCircuit className="size-4" aria-hidden="true" />
              {t("vmm.setup.native.title")}
            </span>
            <span className="mt-1 block text-sm text-muted-foreground">{t("vmm.setup.native.description")}</span>
          </span>
        </label>
        <label className="flex cursor-pointer items-start gap-3 rounded-xl border p-4">
          <RadioGroupItem value="external" />
          <span>
            <span className="flex items-center gap-2 font-medium">
              <Server className="size-4" aria-hidden="true" />
              {t("vmm.setup.external.title")}
            </span>
            <span className="mt-1 block text-sm text-muted-foreground">{t("vmm.setup.external.description")}</span>
          </span>
        </label>
      </RadioGroup>
      {mode === "external" ? (
        <div className="grid gap-4 rounded-xl border p-4 sm:grid-cols-2">
          <SetupInput id="vmm-setup-endpoint" label={t("memory.advanced.endpoint")} onChange={onEndpointChange} placeholder="127.0.0.1:17625" value={endpoint} />
          <SetupInput id="vmm-setup-api-key" label={t("memory.advanced.apiKey")} onChange={onApiKeyChange} password value={apiKey} />
          <SetupInput id="vmm-setup-management-url" label={t("memory.advanced.managementEndpoint")} onChange={onManagementBaseUrlChange} placeholder="http://127.0.0.1:17635" value={managementBaseUrl} />
          <SetupInput id="vmm-setup-management-token" label={t("memory.advanced.managementToken")} onChange={onManagementTokenChange} password value={managementToken} />
        </div>
      ) : null}
    </div>
  )
}

// SetupBindingsStep renders backend-specific global policy and explicit unbound-project rows.
// SetupBindingsStep 渲染后端专属全局策略与显式未绑定项目行。
function SetupBindingsStep(props: {
  mode: VmmQuickSetupMode
  bindingMode: VmmProjectBindingMode
  projects: VmmProjectScopeSummary[]
  decisions: Record<string, EditableBindingDecision>
  nativeProjects: VmmNativeProjectCatalogSummary["projects"]
  externalProjects: VmmProtocolProjectEntry[]
  nativeFixedProjectId: string
  externalProjectPrefix: string
  userName: string
  externalFixedAction: "create_and_bind" | "bind_existing"
  externalFixedProjectName: string
  externalFixedTargetProjectId: string
  onBindingModeChange: (value: VmmProjectBindingMode) => void
  onDecisionChange: (projectId: string, patch: Partial<EditableBindingDecision>) => void
  onNativeFixedProjectIdChange: (value: string) => void
  onExternalProjectPrefixChange: (value: string) => void
  onUserNameChange: (value: string) => void
  onExternalFixedActionChange: (value: "create_and_bind" | "bind_existing") => void
  onExternalFixedProjectNameChange: (value: string) => void
  onExternalFixedTargetProjectIdChange: (value: string) => void
}) {
  const { t } = useI18n()
  const existingProjects = props.mode === "native"
    ? props.nativeProjects.map((project) => ({ id: String(project.projectId), label: project.name }))
    : props.externalProjects.map((project) => ({ id: String(project.project_id), label: project.display_path }))
  return (
    <div className="space-y-4">
      <SetupStepHeading description={t("vmm.setup.binding.description")} title={t("vmm.setup.binding.title")} />
      <RadioGroup className="grid gap-3 sm:grid-cols-2" onValueChange={(value) => props.onBindingModeChange(value as VmmProjectBindingMode)} value={props.bindingMode}>
        <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3">
          <RadioGroupItem value="fixed" />
          <span><span className="block text-sm font-medium">{t("vmm.setup.binding.fixed")}</span><span className="text-xs text-muted-foreground">{t("vmm.setup.binding.fixedDescription")}</span></span>
        </label>
        <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3">
          <RadioGroupItem value="per_project" />
          <span><span className="block text-sm font-medium">{t("vmm.setup.binding.separate")}</span><span className="text-xs text-muted-foreground">{t("vmm.setup.binding.separateDescription")}</span></span>
        </label>
      </RadioGroup>

      {props.mode === "native" ? (
        <div className="space-y-2">
          <Label>{t("vmm.setup.binding.defaultProject")}</Label>
          <ProjectSelect onChange={props.onNativeFixedProjectIdChange} options={existingProjects} value={props.nativeFixedProjectId} />
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          <SetupInput id="vmm-setup-user" label={t("vmm.setup.user")} onChange={props.onUserNameChange} value={props.userName} />
          <SetupInput id="vmm-setup-prefix" label={t("vmm.setup.binding.prefix")} onChange={props.onExternalProjectPrefixChange} placeholder="vulcan/space" value={props.externalProjectPrefix} />
        </div>
      )}

      {props.mode === "external" && props.bindingMode === "fixed" ? (
        <div className="space-y-3">
          <div className="grid gap-3 rounded-lg border p-3 sm:grid-cols-[12rem_minmax(0,1fr)]">
            <ActionSelect onChange={props.onExternalFixedActionChange} value={props.externalFixedAction} />
            {props.externalFixedAction === "create_and_bind" ? (
              <Input onChange={(event) => props.onExternalFixedProjectNameChange(event.target.value)} value={props.externalFixedProjectName} />
            ) : (
              <ProjectSelect onChange={props.onExternalFixedTargetProjectIdChange} options={existingProjects} value={props.externalFixedTargetProjectId} />
            )}
          </div>
        </div>
      ) : null}

      {(
        <div className="space-y-2">
          <p className="text-sm font-medium">{t("vmm.setup.binding.unboundProjects")}</p>
          {props.projects.length === 0 ? (
            <p className="rounded-lg border border-dashed p-4 text-sm text-muted-foreground">{t("vmm.setup.binding.none")}</p>
          ) : props.projects.map((project) => {
            const decision = props.decisions[project.projectId]
            const fixedExternalLabel = props.externalFixedAction === "create_and_bind"
              ? props.externalFixedProjectName
              : existingProjects.find((projectOption) => projectOption.id === props.externalFixedTargetProjectId)?.label
                ?? t("vmm.setup.binding.selectProject")
            const fixedTargetLabel = props.mode === "native"
              ? existingProjects.find((projectOption) => projectOption.id === props.nativeFixedProjectId)?.label
                ?? t("vmm.setup.binding.selectProject")
              : fixedExternalLabel
            const rowProjectOptions = props.bindingMode === "fixed"
              ? [{ id: FIXED_TARGET_SENTINEL, label: `${t("vmm.setup.binding.fixed")}: ${fixedTargetLabel}` }, ...existingProjects]
              : existingProjects
            return (
              <div className="grid gap-3 rounded-lg border p-3 md:grid-cols-[minmax(10rem,1fr)_12rem_minmax(12rem,1fr)]" key={project.projectId}>
                <div className="min-w-0"><p className="truncate text-sm font-medium">{project.name}</p><p className="truncate text-xs text-muted-foreground">{project.projectId}</p></div>
                <ActionSelect
                  onChange={(action) => props.onDecisionChange(project.projectId, {
                    action,
                    targetProjectId: action === "bind_existing"
                      ? (props.bindingMode === "fixed"
                          ? FIXED_TARGET_SENTINEL
                          : existingProjects[0]?.id ?? "")
                      : "",
                  })}
                  value={decision?.action ?? "create_and_bind"}
                />
                {decision?.action === "bind_existing" ? (
                  <ProjectSelect onChange={(targetProjectId) => props.onDecisionChange(project.projectId, { targetProjectId })} options={rowProjectOptions} value={decision.targetProjectId} />
                ) : (
                  <Input onChange={(event) => props.onDecisionChange(project.projectId, { projectName: event.target.value })} value={decision?.projectName ?? project.projectId} />
                )}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

// SetupConfirmationStep summarizes the immutable backend policy and affected project count.
// SetupConfirmationStep 汇总不可变的后端策略与受影响项目数量。
function SetupConfirmationStep({ mode, bindingMode, projectCount }: {
  mode: VmmQuickSetupMode
  bindingMode: VmmProjectBindingMode
  projectCount: number
}) {
  const { t } = useI18n()
  return (
    <div className="space-y-4">
      <SetupStepHeading description={t("vmm.setup.confirm.description")} title={t("vmm.setup.confirm.title")} />
      <dl className="grid gap-3 rounded-xl border p-4 sm:grid-cols-2">
        <SetupSummaryRow label={t("vmm.setup.mode.title")} value={mode === "native" ? t("vmm.setup.native.title") : t("vmm.setup.external.title")} />
        <SetupSummaryRow label={t("vmm.setup.binding.title")} value={bindingMode === "fixed" ? t("vmm.setup.binding.fixed") : t("vmm.setup.binding.separate")} />
        <SetupSummaryRow label={t("vmm.setup.binding.unboundProjects")} value={String(projectCount)} />
      </dl>
      <Alert>
        <ShieldCheck aria-hidden="true" />
        <AlertDescription>{t("vmm.setup.safety")}</AlertDescription>
      </Alert>
    </div>
  )
}

// SetupInput renders one labeled setup input with optional secret masking.
// SetupInput 渲染一个带标签并可选密码遮罩的配置输入框。
function SetupInput({ id, label, onChange, password = false, placeholder, value }: {
  id: string
  label: string
  onChange: (value: string) => void
  password?: boolean
  placeholder?: string
  value: string
}) {
  return <div className="space-y-2"><Label htmlFor={id}>{label}</Label><Input id={id} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} type={password ? "password" : "text"} value={value} /></div>
}

// ActionSelect switches one row between creating a project and binding an existing project.
// ActionSelect 在创建项目与绑定已有项目之间切换一行决定。
function ActionSelect({ onChange, value }: {
  onChange: (value: "create_and_bind" | "bind_existing") => void
  value: "create_and_bind" | "bind_existing"
}) {
  const { t } = useI18n()
  return <Select onValueChange={(next) => onChange(next as "create_and_bind" | "bind_existing")} value={value}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectItem value="create_and_bind">{t("vmm.setup.binding.create")}</SelectItem><SelectItem value="bind_existing">{t("vmm.setup.binding.existing")}</SelectItem></SelectContent></Select>
}

// ProjectSelect renders opaque project ids through stable human-readable names.
// ProjectSelect 通过稳定的人类可读名称渲染不透明项目标识。
function ProjectSelect({ onChange, options, value }: {
  onChange: (value: string) => void
  options: Array<{ id: string; label: string }>
  value: string
}) {
  const { t } = useI18n()
  return <Select onValueChange={onChange} value={value}><SelectTrigger><SelectValue placeholder={t("vmm.setup.binding.selectProject")} /></SelectTrigger><SelectContent>{options.map((option) => <SelectItem key={option.id} value={option.id}>{option.label}</SelectItem>)}</SelectContent></Select>
}

// SetupStepHeading renders one consistent wizard step heading.
// SetupStepHeading 渲染一致的向导步骤标题。
function SetupStepHeading({ description, title }: { description: string; title: string }) {
  return <div><h3 className="text-base font-semibold">{title}</h3><p className="mt-1 text-sm text-muted-foreground">{description}</p></div>
}

// SetupSummaryRow renders one human-readable confirmation fact.
// SetupSummaryRow 渲染一条人类可读确认事实。
function SetupSummaryRow({ label, value }: { label: string; value: string }) {
  return <div><dt className="text-xs text-muted-foreground">{label}</dt><dd className="mt-1 truncate text-sm font-medium" title={value}>{value}</dd></div>
}

export { VmmSetupDialog }
export type {
  VmmProjectBindingMode,
  VmmQuickSetupCatalog,
  VmmQuickSetupConnectionInput,
  VmmQuickSetupInput,
  VmmQuickSetupMode,
  VmmSetupDialogProps,
}
