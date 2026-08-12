import * as React from "react"
import {
  Activity,
  BrainCircuit,
  CheckCircle2,
  CircleAlert,
  Database,
  FolderKanban,
  Play,
  RefreshCw,
  RotateCw,
  Settings2,
  Square,
  UserRound,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { useI18n } from "@/i18n"
import type { TranslationKey } from "@/i18n-messages"
import type {
  VmmCapabilitySummary,
  VmmDeploymentMode,
  VmmHealthState,
  VmmLifecycleState,
  VmmMemorySnapshot,
  VmmPrerequisiteSummary,
  VmmSetupState,
} from "@/types"

// VmmOverviewPageProps defines the structured snapshot and user actions for the status overview.
// VmmOverviewPageProps 定义状态总览使用的结构化快照与用户操作。
interface VmmOverviewPageProps {
  snapshot: VmmMemorySnapshot
  busy: boolean
  onOpenSetup: () => void
  onOpenAdvanced: () => void
  onControl: (action: string) => Promise<void>
}

// VmmOverviewPage renders a human-oriented VMM status dashboard without internal identifiers.
// VmmOverviewPage 渲染面向用户的 VMM 状态总览，并隐藏内部标识。
function VmmOverviewPage({
  snapshot,
  busy,
  onOpenSetup,
  onOpenAdvanced,
  onControl,
}: VmmOverviewPageProps) {
  const { locale, t } = useI18n()
  const state = snapshot.experienceState
  const configured = state.deployment !== "disabled"
  const running = state.lifecycle === "running"
  const dataSpaceBlocked = [
    "unmarked_history",
    "fingerprint_changed",
    "marker_corrupt",
    "marker_version_unsupported",
    "schema_migration_required",
  ].includes(snapshot.dataSpaceState.state)
  // dataSpaceNoticeDismissed hides only the current-page warning while preserving the blocked state.
  // dataSpaceNoticeDismissed 只隐藏当前页面警告，不改变底层阻塞状态。
  const [dataSpaceNoticeDismissed, setDataSpaceNoticeDismissed] = React.useState(false)

  React.useEffect(() => {
    setDataSpaceNoticeDismissed(false)
  }, [snapshot.dataSpaceState.state])

  if (!configured) {
    return (
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
        <Card variant="outlined">
          <CardHeader className="items-start gap-3 sm:flex-row">
            <div className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
              <BrainCircuit className="size-5" aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <CardTitle>{t("vmm.experience.quickSetup.title")}</CardTitle>
              <CardDescription className="max-w-2xl">
                {t("vmm.experience.quickSetup.description")}
              </CardDescription>
            </div>
          </CardHeader>
          <CardContent>
            <Button disabled={busy} onClick={onOpenSetup} type="button">
              <Settings2 className="size-4" aria-hidden="true" />
              {t("vmm.experience.quickSetup.action")}
            </Button>
          </CardContent>
        </Card>
        <VmmPrerequisiteCard prerequisites={snapshot.prerequisites} />
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1.3fr)_minmax(18rem,0.7fr)]">
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="space-y-1">
                <CardTitle>{t("vmm.experience.title")}</CardTitle>
                <CardDescription>{t("vmm.experience.subtitle")}</CardDescription>
              </div>
              <VmmHealthBadge health={state.health} />
            </div>
          </CardHeader>
          <CardContent className="grid gap-3 sm:grid-cols-2">
            <VmmStateRow
              label={t("vmm.experience.deployment")}
              value={t(vmmDeploymentKey(state.deployment))}
            />
            <VmmStateRow
              label={t("vmm.experience.setup")}
              value={t(vmmSetupKey(state.setup))}
            />
            <VmmStateRow
              label={t("vmm.experience.lifecycle")}
              value={t(vmmLifecycleKey(state.lifecycle))}
            />
            <VmmStateRow
              label={t("vmm.experience.health")}
              value={t(vmmHealthKey(state.health))}
            />
            <VmmStateRow
              icon={<FolderKanban className="size-4" aria-hidden="true" />}
              label={t("vmm.experience.currentProject")}
              value={snapshot.displayIdentity.projectPath ?? t("vmm.experience.noProject")}
            />
            <VmmStateRow
              icon={<UserRound className="size-4" aria-hidden="true" />}
              label={t("vmm.experience.currentUser")}
              value={snapshot.displayIdentity.userName ?? t("vmm.experience.noUser")}
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="size-4" aria-hidden="true" />
              {t("vmm.experience.activity")}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <VmmStateRow
              label={t("vmm.experience.pendingWritebacks")}
              value={String(snapshot.recentActivity.pendingWritebacks)}
            />
            <VmmStateRow
              label={t("vmm.experience.completedTurns")}
              value={String(snapshot.recentActivity.completedTurns)}
            />
            <VmmStateRow
              label={t("vmm.experience.lastProfileUpdate")}
              value={formatVmmTimestamp(
                snapshot.recentActivity.lastProfileUpdateAt,
                locale,
                t("vmm.common.none"),
              )}
            />
          </CardContent>
        </Card>
      </div>

      {snapshot.prerequisites.length > 0 ? (
        <VmmPrerequisiteCard prerequisites={snapshot.prerequisites} />
      ) : null}

      {dataSpaceBlocked && !dataSpaceNoticeDismissed ? (
        <Alert variant="destructive">
          <CircleAlert aria-hidden="true" />
          <AlertTitle>
            {t(snapshot.dataSpaceState.titleKey as TranslationKey)}
          </AlertTitle>
          <AlertDescription className="space-y-3">
            <p>{t(snapshot.dataSpaceState.descriptionKey as TranslationKey)}</p>
            <div className="flex flex-wrap gap-2">
              <Button onClick={onOpenAdvanced} size="sm" type="button">
                {t("memory.dataSpace.repair")}
              </Button>
              <Button
                onClick={() => setDataSpaceNoticeDismissed(true)}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("memory.dataSpace.handleLater")}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      ) : null}

      <VmmCapabilityCard capabilities={snapshot.capabilities} />

      <div className="flex flex-wrap gap-2">
        <Button
          disabled={busy}
          onClick={() => void onControl("healthz")}
          type="button"
          variant="outline"
        >
          <RefreshCw className="size-4" aria-hidden="true" />
          {t("vmm.experience.refresh")}
        </Button>
        {state.deployment === "managed" ? (
          <>
            <Button
              disabled={busy || running}
              onClick={() => void onControl("managed_runtime_start")}
              type="button"
              variant="outline"
            >
              <Play className="size-4" aria-hidden="true" />
              {t("vmm.experience.start")}
            </Button>
            <Button
              disabled={busy || !running}
              onClick={() => void onControl("managed_runtime_restart")}
              type="button"
              variant="outline"
            >
              <RotateCw className="size-4" aria-hidden="true" />
              {t("vmm.experience.restart")}
            </Button>
            <Button
              disabled={busy || !running}
              onClick={() => void onControl("managed_runtime_stop")}
              type="button"
              variant="outline"
            >
              <Square className="size-4" aria-hidden="true" />
              {t("vmm.experience.stop")}
            </Button>
          </>
        ) : null}
        <Button disabled={busy} onClick={onOpenAdvanced} type="button" variant="ghost">
          <Settings2 className="size-4" aria-hidden="true" />
          {t("vmm.experience.openAdvanced")}
        </Button>
      </div>
    </div>
  )
}

// formatVmmTimestamp converts either an ISO timestamp or Unix milliseconds into local human time.
// formatVmmTimestamp 将 ISO 时间或 Unix 毫秒时间转换为本地的人类可读时间。
function formatVmmTimestamp(
  value: string | null | undefined,
  locale: string,
  emptyLabel: string,
): string {
  if (!value) {
    return emptyLabel
  }
  const normalized = /^\d+$/.test(value)
    ? Number(value)
    : value
  const date = new Date(normalized)
  if (Number.isNaN(date.getTime())) {
    return emptyLabel
  }
  return new Intl.DateTimeFormat(locale === "zh-CN" ? "zh-CN" : "en-US", {
    dateStyle: "short",
    timeStyle: "short",
  }).format(date)
}

// VmmHealthBadge renders the primary service health as a compact status badge.
// VmmHealthBadge 将主要服务健康状态渲染为紧凑状态徽标。
function VmmHealthBadge({ health }: { health: VmmHealthState }) {
  const { t } = useI18n()
  const healthy = health === "healthy"
  return (
    <Badge variant={healthy ? "default" : health === "unavailable" ? "destructive" : "secondary"}>
      {healthy ? (
        <CheckCircle2 className="size-3.5" aria-hidden="true" />
      ) : (
        <CircleAlert className="size-3.5" aria-hidden="true" />
      )}
      {t(vmmHealthKey(health))}
    </Badge>
  )
}

// VmmPrerequisiteCard renders actionable setup checks in user-facing language.
// VmmPrerequisiteCard 使用面向用户的文案渲染可操作配置检查。
function VmmPrerequisiteCard({ prerequisites }: { prerequisites: VmmPrerequisiteSummary[] }) {
  const { t } = useI18n()
  if (prerequisites.length === 0) {
    return null
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("vmm.experience.prerequisites")}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        {prerequisites.map((prerequisite) => (
          <Alert key={prerequisite.id} variant={prerequisite.severity === "error" ? "destructive" : "default"}>
            <CircleAlert aria-hidden="true" />
            <AlertTitle>{t(vmmPrerequisiteTitleKey(prerequisite.id))}</AlertTitle>
            <AlertDescription>
              {t(vmmPrerequisiteDescriptionKey(prerequisite.id))}
              {prerequisite.technicalDetail ? (
                <span className="mt-1 block break-words font-mono text-xs">
                  {prerequisite.technicalDetail}
                </span>
              ) : null}
            </AlertDescription>
          </Alert>
        ))}
      </CardContent>
    </Card>
  )
}

// VmmCapabilityCard renders each VMM capability independently instead of collapsing health states.
// VmmCapabilityCard 独立渲染每项 VMM 能力，避免将不同健康状态合并。
function VmmCapabilityCard({ capabilities }: { capabilities: VmmCapabilitySummary[] }) {
  const { t } = useI18n()
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Database className="size-4" aria-hidden="true" />
          {t("vmm.experience.capabilities")}
        </CardTitle>
      </CardHeader>
      <CardContent className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {capabilities.map((capability) => (
          <div
            className="flex items-center justify-between gap-3 rounded-lg border bg-muted/20 px-3 py-2"
            key={capability.id}
          >
            <span className="min-w-0 truncate text-sm">{t(vmmCapabilityKey(capability.id))}</span>
            <Badge variant={capability.available && capability.enabled ? "default" : "outline"}>
              {t(capability.available && capability.enabled
                ? "vmm.experience.state.available"
                : "vmm.experience.state.unavailable")}
            </Badge>
          </div>
        ))}
      </CardContent>
    </Card>
  )
}

// VmmStateRow renders one labeled product-facing state value.
// VmmStateRow 渲染一个带标签的产品状态值。
function VmmStateRow({
  icon,
  label,
  value,
}: {
  icon?: React.ReactNode
  label: string
  value: string
}) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-3 rounded-lg border bg-muted/20 px-3 py-2">
      <span className="flex min-w-0 items-center gap-2 text-sm text-muted-foreground">
        {icon}
        <span className="truncate">{label}</span>
      </span>
      <strong className="truncate text-sm font-medium">{value}</strong>
    </div>
  )
}

// vmmDeploymentKey maps a deployment enum to one stable translation key.
// vmmDeploymentKey 将部署枚举映射为稳定翻译键。
function vmmDeploymentKey(value: VmmDeploymentMode): TranslationKey {
  return `vmm.experience.deployment.${value}` as TranslationKey
}

// vmmSetupKey maps a setup enum to one stable translation key.
// vmmSetupKey 将设置枚举映射为稳定翻译键。
function vmmSetupKey(value: VmmSetupState): TranslationKey {
  if (value === "not_configured") {
    return "vmm.experience.setup.notConfigured"
  }
  return `vmm.experience.setup.${value}` as TranslationKey
}

// vmmLifecycleKey maps a lifecycle enum to one stable translation key.
// vmmLifecycleKey 将生命周期枚举映射为稳定翻译键。
function vmmLifecycleKey(value: VmmLifecycleState): TranslationKey {
  if (value === "waiting_retry") {
    return "vmm.experience.lifecycle.waitingRetry"
  }
  return `vmm.experience.lifecycle.${value}` as TranslationKey
}

// vmmHealthKey maps a health enum to one stable translation key.
// vmmHealthKey 将健康枚举映射为稳定翻译键。
function vmmHealthKey(value: VmmHealthState): TranslationKey {
  return `vmm.experience.health.${value}` as TranslationKey
}

// vmmPrerequisiteTitleKey maps prerequisite ids to user-facing title keys.
// vmmPrerequisiteTitleKey 将前置条件标识映射为面向用户的标题键。
function vmmPrerequisiteTitleKey(id: string): TranslationKey {
  if (id === "memory_enabled") return "vmm.experience.prerequisite.memory.title"
  if (id === "transport" || id === "external_transport") {
    return "vmm.experience.prerequisite.transport.title"
  }
  if (id === "user") return "vmm.experience.prerequisite.user.title"
  if (id === "project") return "vmm.experience.prerequisite.project.title"
  return "vmm.experience.prerequisite.runtime.title"
}

// vmmPrerequisiteDescriptionKey maps prerequisite ids to actionable description keys.
// vmmPrerequisiteDescriptionKey 将前置条件标识映射为可操作说明键。
function vmmPrerequisiteDescriptionKey(id: string): TranslationKey {
  if (id === "memory_enabled") return "vmm.experience.prerequisite.memory.description"
  if (id === "transport" || id === "external_transport") {
    return "vmm.experience.prerequisite.transport.description"
  }
  if (id === "user") return "vmm.experience.prerequisite.user.description"
  if (id === "project") return "vmm.experience.prerequisite.project.description"
  return "vmm.experience.prerequisite.runtime.description"
}

// vmmCapabilityKey maps capability ids to stable user-facing labels.
// vmmCapabilityKey 将能力标识映射为稳定的用户可见标签。
function vmmCapabilityKey(id: string): TranslationKey {
  if (id === "automatic_recall") return "vmm.experience.capability.recall"
  if (id === "automatic_writeback") return "vmm.experience.capability.writeback"
  if (id === "ai_memory_tools") return "vmm.experience.capability.tools"
  if (id === "profile_instruction") return "vmm.experience.capability.profile"
  return "vmm.experience.capability.rerank"
}

export { VmmOverviewPage }
export type { VmmOverviewPageProps }
