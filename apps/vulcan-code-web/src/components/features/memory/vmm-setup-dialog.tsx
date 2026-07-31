import * as React from "react"
import {
  BrainCircuit,
  Check,
  CircleAlert,
  ChevronLeft,
  ChevronRight,
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
import { useI18n, type TranslationKey } from "@/i18n"

// VmmQuickSetupMode identifies the service ownership selected by the user.
// VmmQuickSetupMode 标识用户选择的服务所有权。
type VmmQuickSetupMode = "managed" | "external"

// VmmQuickSetupScope identifies where the initial memory policy is enabled.
// VmmQuickSetupScope 标识首次记忆策略启用范围。
type VmmQuickSetupScope = "all_projects" | "current_project" | "not_yet"

// VmmQuickSetupInput contains the visible decisions used by the setup workflow.
// VmmQuickSetupInput 包含快速配置流程使用的可见决策。
interface VmmQuickSetupInput {
  mode: VmmQuickSetupMode
  scope: VmmQuickSetupScope
  userName: string
  projectName: string
  externalEndpoint: string
  externalApiKey: string
  externalManagementBaseUrl: string
  externalManagementToken: string
}

// VmmSetupDialogProps defines controlled visibility and the asynchronous setup action.
// VmmSetupDialogProps 定义受控可见性与异步配置操作。
interface VmmSetupDialogProps {
  open: boolean
  busy: boolean
  currentProjectAvailable: boolean
  defaultUserName: string
  defaultProjectName: string
  notice: string
  // Opens credential management while keeping VMM disabled.
  // 在保持 VMM 禁用时打开凭据管理。
  onOpenCredentialManagement: () => void
  onOpenChange: (open: boolean) => void
  onSubmit: (input: VmmQuickSetupInput) => Promise<boolean>
}

// VmmSetupDialog guides a first-time user through four task-oriented setup steps.
// VmmSetupDialog 通过四个任务导向步骤引导首次使用者完成配置。
function VmmSetupDialog({
  open,
  busy,
  currentProjectAvailable,
  defaultUserName,
  defaultProjectName,
  notice,
  onOpenCredentialManagement,
  onOpenChange,
  onSubmit,
}: VmmSetupDialogProps) {
  const { t } = useI18n()
  const [step, setStep] = React.useState(0)
  const [mode, setMode] = React.useState<VmmQuickSetupMode>("managed")
  const [scope, setScope] = React.useState<VmmQuickSetupScope>("all_projects")
  const [userName, setUserName] = React.useState(defaultUserName)
  const [projectName, setProjectName] = React.useState(defaultProjectName)
  const [externalEndpoint, setExternalEndpoint] = React.useState("")
  const [externalApiKey, setExternalApiKey] = React.useState("")
  const [externalManagementBaseUrl, setExternalManagementBaseUrl] = React.useState("")
  const [externalManagementToken, setExternalManagementToken] = React.useState("")
  const [inferenceReady, setInferenceReady] = React.useState(false)
  const [failed, setFailed] = React.useState(false)
  // submitting keeps the whole multi-request setup workflow visibly exclusive.
  // submitting 让整个多请求配置流程保持可见的独占状态。
  const [submitting, setSubmitting] = React.useState(false)

  React.useEffect(() => {
    if (!open) {
      return
    }
    setStep(0)
    setMode("managed")
    setScope("all_projects")
    setUserName(defaultUserName)
    setProjectName(defaultProjectName)
    setExternalEndpoint("")
    setExternalApiKey("")
    setExternalManagementBaseUrl("")
    setExternalManagementToken("")
    setInferenceReady(false)
    setFailed(false)
    setSubmitting(false)
  }, [defaultProjectName, defaultUserName, open])

  const effectiveBusy = busy || submitting
  const identityComplete = userName.trim().length > 0 && projectName.trim().length > 0
  const connectionComplete = mode === "managed"
    || (externalEndpoint.trim().length > 0
      && externalManagementBaseUrl.trim().length > 0
      && externalManagementToken.trim().length > 0)
  const canAdvance = step === 0
    ? connectionComplete
    : step === 1
      ? inferenceReady
      : true
  // canSubmit keeps the final enable action gated even after the inference step has been left.
  // canSubmit 在离开推理步骤后仍继续门控最终启用动作。
  const canSubmit = identityComplete
    && connectionComplete
    && inferenceReady
    && !effectiveBusy

  // handleSubmit validates visible decisions and keeps the dialog open on failure.
  // handleSubmit 校验可见决策，并在失败时保持弹窗打开。
  const handleSubmit = React.useCallback(async () => {
    if (!canSubmit) {
      return
    }
    setFailed(false)
    setSubmitting(true)
    try {
      const completed = await onSubmit({
        mode,
        scope,
        userName: userName.trim(),
        projectName: projectName.trim(),
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
    canSubmit,
    externalApiKey,
    externalEndpoint,
    externalManagementBaseUrl,
    externalManagementToken,
    mode,
    onOpenChange,
    onSubmit,
    projectName,
    scope,
    userName,
  ])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <BrainCircuit className="size-5" aria-hidden="true" />
            {t("vmm.setup.title")}
          </DialogTitle>
          <DialogDescription>
            {formatSkillText(t("vmm.setup.stepLabel"), {
              current: step + 1,
              total: 4,
            })}
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
            <SetupScopeStep
              currentProjectAvailable={currentProjectAvailable}
              onProjectNameChange={setProjectName}
              onScopeChange={setScope}
              onUserNameChange={setUserName}
              projectName={projectName}
              scope={scope}
              userName={userName}
            />
          ) : null}
          {step === 3 ? (
            <SetupConfirmationStep
              mode={mode}
              projectName={projectName}
              scope={scope}
              userName={userName}
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
              if (step === 0) {
                onOpenChange(false)
              } else {
                setStep((current) => current - 1)
              }
            }}
            type="button"
            variant="outline"
          >
            {step === 0 ? null : <ChevronLeft className="size-4" aria-hidden="true" />}
            {step === 0 ? t("vmm.setup.cancel") : t("vmm.setup.previous")}
          </Button>
          {step < 3 ? (
            <Button
              disabled={effectiveBusy || !canAdvance}
              onClick={() => setStep((current) => current + 1)}
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

// SetupModeStep renders local or external ownership without exposing runtime internals.
// SetupModeStep 渲染本地或外部所有权且不暴露运行时内部信息。
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
      <SetupStepHeading
        description={t("vmm.setup.mode.description")}
        title={t("vmm.setup.mode.title")}
      />
      <RadioGroup
        className="grid gap-3 sm:grid-cols-2"
        onValueChange={(value) => onModeChange(value as VmmQuickSetupMode)}
        value={mode}
      >
        <label className="flex cursor-pointer items-start gap-3 rounded-xl border p-4">
          <RadioGroupItem value="managed" />
          <span>
            <span className="flex items-center gap-2 font-medium">
              <BrainCircuit className="size-4" aria-hidden="true" />
              {t("vmm.setup.managed.title")}
            </span>
            <span className="mt-1 block text-sm text-muted-foreground">
              {t("vmm.setup.managed.description")}
            </span>
          </span>
        </label>
        <label className="flex cursor-pointer items-start gap-3 rounded-xl border p-4">
          <RadioGroupItem value="external" />
          <span>
            <span className="flex items-center gap-2 font-medium">
              <Server className="size-4" aria-hidden="true" />
              {t("vmm.setup.external.title")}
            </span>
            <span className="mt-1 block text-sm text-muted-foreground">
              {t("vmm.setup.external.description")}
            </span>
          </span>
        </label>
      </RadioGroup>
      {mode === "external" ? (
        <div className="grid gap-4 rounded-xl border p-4 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="vmm-setup-endpoint">{t("memory.advanced.endpoint")}</Label>
            <Input
              id="vmm-setup-endpoint"
              onChange={(event) => onEndpointChange(event.target.value)}
              placeholder="127.0.0.1:17625"
              value={endpoint}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="vmm-setup-api-key">{t("memory.advanced.apiKey")}</Label>
            <Input
              id="vmm-setup-api-key"
              onChange={(event) => onApiKeyChange(event.target.value)}
              type="password"
              value={apiKey}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="vmm-setup-management-url">{t("memory.advanced.managementEndpoint")}</Label>
            <Input
              id="vmm-setup-management-url"
              onChange={(event) => onManagementBaseUrlChange(event.target.value)}
              placeholder="http://127.0.0.1:17635"
              value={managementBaseUrl}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="vmm-setup-management-token">{t("memory.advanced.managementToken")}</Label>
            <Input
              id="vmm-setup-management-token"
              onChange={(event) => onManagementTokenChange(event.target.value)}
              type="password"
              value={managementToken}
            />
          </div>
        </div>
      ) : null}
    </div>
  )
}

// SetupScopeStep captures visible identity and initial application scope.
// SetupScopeStep 收集可见身份与首次应用范围。
function SetupScopeStep({
  currentProjectAvailable,
  onProjectNameChange,
  onScopeChange,
  onUserNameChange,
  projectName,
  scope,
  userName,
}: {
  currentProjectAvailable: boolean
  onProjectNameChange: (value: string) => void
  onScopeChange: (value: VmmQuickSetupScope) => void
  onUserNameChange: (value: string) => void
  projectName: string
  scope: VmmQuickSetupScope
  userName: string
}) {
  const { t } = useI18n()
  return (
    <div className="space-y-4">
      <SetupStepHeading
        description={t("vmm.setup.scope.description")}
        title={t("vmm.setup.scope.title")}
      />
      <RadioGroup className="grid gap-2" onValueChange={(value) => onScopeChange(value as VmmQuickSetupScope)} value={scope}>
        {(["all_projects", "current_project", "not_yet"] as VmmQuickSetupScope[]).map((value) => {
          const disabled = value === "current_project" && !currentProjectAvailable
          return (
            <label
              className="flex cursor-pointer items-center gap-3 rounded-lg border p-3 has-[[data-disabled]]:cursor-not-allowed has-[[data-disabled]]:opacity-50"
              key={value}
            >
              <RadioGroupItem disabled={disabled} value={value} />
              <span className="text-sm font-medium">
                {t(`vmm.setup.scope.${value}` as TranslationKey)}
                {disabled ? (
                  <span className="ml-2 font-normal text-muted-foreground">
                    {t("vmm.setup.scope.currentProjectUnavailable")}
                  </span>
                ) : null}
              </span>
            </label>
          )
        })}
      </RadioGroup>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor="vmm-setup-user">{t("vmm.setup.user")}</Label>
          <Input id="vmm-setup-user" onChange={(event) => onUserNameChange(event.target.value)} value={userName} />
        </div>
        <div className="space-y-2">
          <Label htmlFor="vmm-setup-project">{t("vmm.setup.project")}</Label>
          <Input id="vmm-setup-project" onChange={(event) => onProjectNameChange(event.target.value)} value={projectName} />
        </div>
      </div>
    </div>
  )
}

// SetupConfirmationStep summarizes the choices that will be written.
// SetupConfirmationStep 汇总即将写入的选择。
function SetupConfirmationStep({
  mode,
  projectName,
  scope,
  userName,
}: {
  mode: VmmQuickSetupMode
  projectName: string
  scope: VmmQuickSetupScope
  userName: string
}) {
  const { t } = useI18n()
  return (
    <div className="space-y-4">
      <SetupStepHeading
        description={t("vmm.setup.confirm.description")}
        title={t("vmm.setup.confirm.title")}
      />
      <dl className="grid gap-3 rounded-xl border p-4 sm:grid-cols-2">
        <SetupSummaryRow label={t("vmm.setup.mode.title")} value={mode === "managed" ? t("vmm.setup.managed.title") : t("vmm.setup.external.title")} />
        <SetupSummaryRow
          label={t("vmm.setup.scope.title")}
          value={t(`vmm.setup.scope.${scope}` as TranslationKey)}
        />
        <SetupSummaryRow label={t("vmm.setup.user")} value={userName} />
        <SetupSummaryRow label={t("vmm.setup.project")} value={projectName} />
      </dl>
      <Alert>
        <ShieldCheck aria-hidden="true" />
        <AlertDescription>{t("vmm.setup.safety")}</AlertDescription>
      </Alert>
    </div>
  )
}

// SetupStepHeading renders one consistent wizard step heading.
// SetupStepHeading 渲染一致的向导步骤标题。
function SetupStepHeading({ description, title }: { description: string; title: string }) {
  return (
    <div>
      <h3 className="text-base font-semibold">{title}</h3>
      <p className="mt-1 text-sm text-muted-foreground">{description}</p>
    </div>
  )
}

// SetupSummaryRow renders one human-readable confirmation fact.
// SetupSummaryRow 渲染一条人类可读确认事实。
function SetupSummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-1 truncate text-sm font-medium" title={value}>{value}</dd>
    </div>
  )
}

export { VmmSetupDialog }
export type {
  VmmQuickSetupInput,
  VmmQuickSetupMode,
  VmmQuickSetupScope,
  VmmSetupDialogProps,
}
