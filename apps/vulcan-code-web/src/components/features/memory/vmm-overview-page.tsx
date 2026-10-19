import * as React from "react"
import {
  Activity,
  BrainCircuit,
  CheckCircle2,
  CircleAlert,
  Database,
  FolderKanban,
  RefreshCw,
  Settings2,
  UserRound,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui-kit/primitives/card"
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
import { formatDateTime } from "@/lib/display-format"
import "@/ui-kit/styles/features/settings-forms.css"
import "@/ui-kit/styles/features/memory.css"

// VmmOverviewPageProps defines the structured snapshot and user actions for the status overview.
// VmmOverviewPageProps 定义状态总览使用的结构化快照与用户操作。
interface VmmOverviewPageProps {
  snapshot: VmmMemorySnapshot
  busy: boolean
  onOpenSetup: () => void
  onOpenAdvanced: () => void
  onControl: (action: string) => Promise<void>
}

// vmmNativeBindingModeKey maps the native new-project policy to one user-facing status value.
// vmmNativeBindingModeKey 将原生新增项目策略映射为面向用户的状态值。
// bindingMode is the authoritative persisted native binding policy.
// bindingMode 是权威持久化的原生绑定策略。
// The return value is the exact translation key rendered by the overview.
// 返回值是总览页渲染的准确翻译键。
export function vmmNativeBindingModeKey(
  bindingMode: VmmMemorySnapshot["globalDraft"]["nativeProjectBindingMode"],
): TranslationKey {
  return bindingMode === "fixed"
    ? "vmm.experience.nativeBindingFixed"
    : "vmm.experience.nativeBindingPerProject"
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
  // externalIdentity controls the external-only default project and user rows.
  // externalIdentity 控制仅适用于外置后端的默认项目与用户行。
  const externalIdentity = state.deployment === "external"

  if (!configured) {
    return (
      <div className="vk-form-loose">
        <Card variant="outlined">
          <CardHeader className="vk-mem-hero">
            <div className="vk-mem-hero-mark">
              <BrainCircuit className="vk-icon" aria-hidden="true" />
            </div>
            <div className="vk-form-tight">
              <CardTitle>{t("vmm.experience.quickSetup.title")}</CardTitle>
              <CardDescription className="vk-mem-measure">
                {t("vmm.experience.quickSetup.description")}
              </CardDescription>
            </div>
          </CardHeader>
          <CardContent>
            <Button disabled={busy} onClick={onOpenSetup} type="button">
              <Settings2 className="vk-icon" aria-hidden="true" />
              {t("vmm.experience.quickSetup.action")}
            </Button>
          </CardContent>
        </Card>
        <VmmPrerequisiteCard prerequisites={snapshot.prerequisites} />
      </div>
    )
  }

  return (
    <div className="vk-form-loose">
      <div className="vk-mem-overview">
        <Card>
          <CardHeader>
            <div className="vk-mem-tile-head">
              <div className="vk-form-tight">
                <CardTitle>{t("vmm.experience.title")}</CardTitle>
                <CardDescription>{t("vmm.experience.subtitle")}</CardDescription>
              </div>
              <VmmHealthBadge health={state.health} />
            </div>
          </CardHeader>
          <CardContent className="vk-form-grid">
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
            {externalIdentity ? (
              <>
                <VmmStateRow
                  icon={<FolderKanban className="vk-icon" aria-hidden="true" />}
                  label={t("vmm.experience.currentProject")}
                  value={snapshot.displayIdentity.projectPath ?? t("vmm.experience.noProject")}
                />
                <VmmStateRow
                  icon={<UserRound className="vk-icon" aria-hidden="true" />}
                  label={t("vmm.experience.currentUser")}
                  value={snapshot.displayIdentity.userName ?? t("vmm.experience.noUser")}
                />
              </>
            ) : (
              <VmmStateRow
                icon={<Database className="vk-icon" aria-hidden="true" />}
                label={t("vmm.experience.defaultMemoryMode")}
                value={t(vmmNativeBindingModeKey(snapshot.globalDraft.nativeProjectBindingMode))}
              />
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="vk-form-inline">
              <Activity className="vk-icon" aria-hidden="true" />
              {t("vmm.experience.activity")}
            </CardTitle>
          </CardHeader>
          <CardContent className="vk-form-stack">
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

      <VmmCapabilityCard capabilities={snapshot.capabilities} />

      <div className="vk-form-chips">
        <Button disabled={busy} onClick={onOpenSetup} type="button">
          <Settings2 className="vk-icon" aria-hidden="true" />
          {t("vmm.experience.reconfigure")}
        </Button>
        <Button
          disabled={busy}
          onClick={() => void onControl("healthz")}
          type="button"
          variant="outline"
        >
          <RefreshCw className="vk-icon" aria-hidden="true" />
          {t("vmm.experience.refresh")}
        </Button>
        <Button disabled={busy} onClick={onOpenAdvanced} type="button" variant="ghost">
          <Settings2 className="vk-icon" aria-hidden="true" />
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
  return formatDateTime(date, locale) ?? emptyLabel
}

// VmmHealthBadge renders the primary service health as a compact status badge.
// VmmHealthBadge 将主要服务健康状态渲染为紧凑状态徽标。
function VmmHealthBadge({ health }: { health: VmmHealthState }) {
  const { t } = useI18n()
  const healthy = health === "healthy"
  return (
    <Badge variant={healthy ? "default" : health === "unavailable" ? "destructive" : "secondary"}>
      {healthy ? (
        <CheckCircle2 className="vk-icon" aria-hidden="true" />
      ) : (
        <CircleAlert className="vk-icon" aria-hidden="true" />
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
      <CardContent className="vk-form-stack">
        {prerequisites.map((prerequisite) => (
          <Alert key={prerequisite.id} variant={prerequisite.severity === "error" ? "destructive" : "default"}>
            <CircleAlert aria-hidden="true" />
            <AlertTitle>{t(vmmPrerequisiteTitleKey(prerequisite.id))}</AlertTitle>
            <AlertDescription>
              {t(vmmPrerequisiteDescriptionKey(prerequisite.id))}
              {prerequisite.technicalDetail ? (
                <span className="vk-form-codeline vk-mem-block">
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
        <CardTitle className="vk-form-inline">
          <Database className="vk-icon" aria-hidden="true" />
          {t("vmm.experience.capabilities")}
        </CardTitle>
      </CardHeader>
      <CardContent className="vk-mem-caps">
        {capabilities.map((capability) => (
          <div
            className="vk-mem-cap"
            key={capability.id}
          >
            <span className="vk-truncate vk-form-sm">{t(vmmCapabilityKey(capability.id))}</span>
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
    <div className="vk-mem-cap">
      <span className="vk-form-inline vk-form-text">
        {icon}
        <span className="vk-truncate">{label}</span>
      </span>
      <strong className="vk-form-name">{value}</strong>
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
  if (id === "memory_enabled") return "memory.rules.enableDescription"
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
