import * as React from "react"
import { BrainCircuit, RefreshCw, Send } from "lucide-react"

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Combobox } from "@/components/ui/combobox"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog"
import { Textarea } from "@/components/ui/textarea"
import { useI18n } from "@/i18n"
import type {
  VmmMemoryActionResult,
  VmmMemorySnapshot,
  VmmNativeProfileRecordSummary,
  VmmNativeProfileStateCounts,
  VmmNativeProjectCatalogSummary,
  VmmProfileBundleActionValue,
  VmmProfileNodeActionValue,
} from "@/types"

// NATIVE_FIXED_USER_TARGET is a UI-only identity for the single host-owned native memory user.
// NATIVE_FIXED_USER_TARGET 是原生记忆唯一宿主用户的纯界面身份。
const NATIVE_FIXED_USER_TARGET = "__native_fixed_user__"

// ProfileNodeView is the user-facing projection of one active or audit-only profile node.
// ProfileNodeView 是单个活动或仅供审计画像节点的用户可见投影。
interface ProfileNodeView {
  content: string
  priority: string
  level: string
  date: string
  state?: VmmNativeProfileRecordSummary["effectiveState"]
  storedState?: VmmNativeProfileRecordSummary["storedState"]
  activeForInjection?: boolean
  sourceKind?: VmmNativeProfileRecordSummary["sourceKind"]
  sourceId?: string | null
  sourceTurnIds?: number[]
  statusReason?: string | null
  updatedAtUnixMs?: number
}

// VmmProfilePageProps defines the active profile snapshot and service actions.
// VmmProfilePageProps 定义当前画像快照与服务操作。
interface VmmProfilePageProps {
  busy: boolean
  snapshot: VmmMemorySnapshot
  result: VmmMemoryActionResult | null
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// vmmProfileUsesNativeProjectCatalog identifies the one-level host-owned profile directory.
// vmmProfileUsesNativeProjectCatalog 标识宿主持有的单层原生画像目录。
export function vmmProfileUsesNativeProjectCatalog(
  deployment: VmmMemorySnapshot["experienceState"]["deployment"],
): boolean {
  return deployment === "native"
}

// vmmProfileCatalogAction returns the only catalog action valid for the selected backend.
// vmmProfileCatalogAction 返回当前后端唯一有效的画像目录动作。
export function vmmProfileCatalogAction(
  deployment: VmmMemorySnapshot["experienceState"]["deployment"],
): "native_project_catalog" | "load_users_projects" {
  return vmmProfileUsesNativeProjectCatalog(deployment)
    ? "native_project_catalog"
    : "load_users_projects"
}

// vmmProfileNodeAction selects native lifecycle audit records or the external active-only contract.
// vmmProfileNodeAction 选择原生生命周期审计记录或外置仅活动节点契约。
export function vmmProfileNodeAction(
  deployment: VmmMemorySnapshot["experienceState"]["deployment"],
): "native_profile_records" | "load_profile_nodes" {
  return vmmProfileUsesNativeProjectCatalog(deployment)
    ? "native_profile_records"
    : "load_profile_nodes"
}

// vmmProfileEffectiveTarget preserves the selected profile class for both native and external modes.
// vmmProfileEffectiveTarget 在原生与外置模式下都保留所选画像类别。
//
// Parameters: deployment identifies the active backend and requestedTarget is the user-selected class.
// 参数：deployment 标识当前后端，requestedTarget 是用户选择的画像类别。
//
// Returns: the exact requested user or project target; native user identity remains host-owned.
// 返回：准确的用户或项目目标；原生用户身份仍由宿主持有。
export function vmmProfileEffectiveTarget(
  _deployment: VmmMemorySnapshot["experienceState"]["deployment"],
  requestedTarget: "user" | "project",
): "user" | "project" {
  return requestedTarget
}

// vmmProfileUsesFixedNativeUser identifies the non-selectable host-owned user profile target.
// vmmProfileUsesFixedNativeUser 标识不可选择身份的宿主持有固定用户画像目标。
//
// Parameters: deployment identifies the backend and target identifies the selected profile class.
// 参数：deployment 标识后端，target 标识所选画像类别。
//
// Returns: true only for the native user profile.
// 返回：仅原生用户画像返回真。
export function vmmProfileUsesFixedNativeUser(
  deployment: VmmMemorySnapshot["experienceState"]["deployment"],
  target: "user" | "project",
): boolean {
  return deployment === "native" && target === "user"
}

// vmmProfileBundlePreview consumes only the representation declared by the server's bundle mode.
// vmmProfileBundlePreview 仅消费服务端画像包模式声明的表示。
//
// Parameters: bundle is the exact VMM response and target is the visible profile class.
// 参数：bundle 是准确的 VMM 响应，target 是当前可见画像类别。
//
// Returns: Full text or the selected Split section; null reports an unspecified/unsupported mode.
// 返回：Full 正文或选定的 Split 分区；null 表示未指定或不支持的模式。
export function vmmProfileBundlePreview(
  bundle: VmmProfileBundleActionValue,
  target: "user" | "project",
): string | null {
  switch (bundle.mode) {
    case "PROFILE_BUNDLE_MODE_FULL": return bundle.combined_text
    case "PROFILE_BUNDLE_MODE_SPLIT": return target === "user" ? bundle.user_profile : bundle.project_profile
    default: return null
  }
}

// VmmProfilePage renders current profile content without internal node identifiers.
// VmmProfilePage 渲染不含内部节点标识的当前画像内容。
function VmmProfilePage({
  busy,
  snapshot,
  result,
  onRunAction,
}: VmmProfilePageProps) {
  const { t } = useI18n()
  // correctionOpen keeps correction separate from read-only profile browsing.
  // correctionOpen 将修正操作与只读画像浏览分开。
  const [correctionOpen, setCorrectionOpen] = React.useState(false)
  const isNative = vmmProfileUsesNativeProjectCatalog(snapshot.experienceState.deployment)
  const [target, setTarget] = React.useState<"user" | "project">("user")
  // selectedUserRef is the explicit VMM catalog user being inspected.
  // selectedUserRef 是当前明确查看的 VMM 目录用户。
  const [selectedUserRef, setSelectedUserRef] = React.useState(snapshot.globalDraft.userId)
  // selectedProjectRef is the explicit VMM catalog project being inspected.
  // selectedProjectRef 是当前明确查看的 VMM 目录项目。
  const [selectedProjectRef, setSelectedProjectRef] = React.useState(snapshot.globalDraft.projectId)
  const [nodes, setNodes] = React.useState<ProfileNodeView[]>([])
  // nativeStateCounts keeps all profile lifecycle states visible without changing active injection.
  // nativeStateCounts 让全部画像生命周期状态可见，同时不改变活动注入逻辑。
  const [nativeStateCounts, setNativeStateCounts] = React.useState<VmmNativeProfileStateCounts | null>(null)
  const [instruction, setInstruction] = React.useState("")
  const [bundlePreview, setBundlePreview] = React.useState(snapshot.runtime.profilePreview)
  // Invalid server modes are reported rather than guessed from whichever field is nonempty.
  // 非法服务端模式明确报告，不从哪个字段非空来猜测。
  const [bundleModeError, setBundleModeError] = React.useState(false)
  // nativeCatalog contains the host-owned one-level project identities available to native profiles.
  // nativeCatalog 包含原生画像可选的宿主持有单层项目身份。
  const [nativeCatalog, setNativeCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  // automaticLoadKey prevents one completed profile request from retriggering on busy-state changes.
  // automaticLoadKey 防止一次已完成画像请求因忙碌状态变化而重复触发。
  const automaticLoadKey = React.useRef("")
  // nativeCatalogLoadIssued prevents duplicate native project catalog requests during one mount.
  // nativeCatalogLoadIssued 防止一次挂载期间重复请求原生项目目录。
  const nativeCatalogLoadIssued = React.useRef(false)

  // userOptions exposes human-readable VMM users without leaking their numeric transport ids.
  // userOptions 暴露人类可读的 VMM 用户，同时不显示数字传输标识。
  const userOptions = React.useMemo(() => snapshot.managementCatalog.users.map((user) => ({
    value: user.stableKey,
    label: user.displayName,
  })), [snapshot.managementCatalog.users])
  // projectOptions exposes canonical VMM paths so projects with the same leaf name remain distinct.
  // projectOptions 暴露规范 VMM 路径，确保同名末级项目仍可区分。
  const projectOptions = React.useMemo(() => isNative
    ? (nativeCatalog?.projects ?? []).map((project) => ({
      value: String(project.projectId),
      label: project.name,
      description: project.tag,
      keywords: [project.tag],
    }))
    : snapshot.managementCatalog.projects.map((project) => ({
      value: project.stableKey,
      label: project.displayName,
      description: project.displayPath,
      keywords: [project.displayPath],
    })), [isNative, nativeCatalog?.projects, snapshot.managementCatalog.projects])
  // profileCatalogReady prevents profile calls from racing the first user/project directory load.
  // profileCatalogReady 防止画像调用与首次用户/项目目录加载发生竞争。
  const profileCatalogReady = isNative
    ? projectOptions.length > 0
    : userOptions.length > 0 && projectOptions.length > 0

  React.useEffect(() => {
    if (isNative) {
      setTarget("user")
    } else {
      setNativeCatalog(null)
    }
    setNodes([])
    setNativeStateCounts(null)
    setBundlePreview("")
    setBundleModeError(false)
    automaticLoadKey.current = ""
    nativeCatalogLoadIssued.current = false
  }, [isNative])

  React.useEffect(() => {
    if (!isNative || busy || nativeCatalog || nativeCatalogLoadIssued.current) {
      return
    }
    nativeCatalogLoadIssued.current = true
    void onRunAction("native_project_catalog").then((catalogResult) => {
      if (!catalogResult) {
        nativeCatalogLoadIssued.current = false
      }
    })
  }, [busy, isNative, nativeCatalog, onRunAction])

  React.useEffect(() => {
    if (isNative) {
      return
    }
    setSelectedUserRef((current) => {
      if (snapshot.managementCatalog.users.some((user) => user.stableKey === current)) {
        return current
      }
      return snapshot.managementCatalog.users.find(
        (user) => user.stableKey === snapshot.globalDraft.userId,
      )?.stableKey ?? snapshot.managementCatalog.users[0]?.stableKey ?? ""
    })
  }, [isNative, snapshot.globalDraft.userId, snapshot.managementCatalog.users])

  React.useEffect(() => {
    setSelectedProjectRef((current) => {
      if (projectOptions.some((project) => project.value === current)) {
        return current
      }
      const configuredProjectRef = isNative
        ? String(snapshot.globalDraft.nativeFixedProjectId)
        : snapshot.globalDraft.projectId
      return projectOptions.find((project) => project.value === configuredProjectRef)?.value
        ?? projectOptions[0]?.value
        ?? ""
    })
  }, [
    isNative,
    projectOptions,
    snapshot.globalDraft.nativeFixedProjectId,
    snapshot.globalDraft.projectId,
  ])

  const effectiveTarget = vmmProfileEffectiveTarget(snapshot.experienceState.deployment, target)
  const profileDescription = t(isNative
    ? "memory.profile.nativeDescription"
    : "memory.profile.description")
  // usesFixedNativeUser distinguishes the host-owned user profile from selectable external users.
  // usesFixedNativeUser 区分宿主持有的固定用户画像与可选择的外置用户。
  const usesFixedNativeUser = vmmProfileUsesFixedNativeUser(
    snapshot.experienceState.deployment,
    effectiveTarget,
  )
  const selectedTargetRef = usesFixedNativeUser
    ? (selectedProjectRef ? NATIVE_FIXED_USER_TARGET : "")
    : effectiveTarget === "user" ? selectedUserRef : selectedProjectRef
  const selectedTargetDescription = usesFixedNativeUser
    ? t("memory.profile.nativeUserName")
    : effectiveTarget === "user"
      ? snapshot.managementCatalog.users.find((user) => user.stableKey === selectedUserRef)?.displayName
    : projectOptions.find((project) => project.value === selectedProjectRef)?.description

  React.useEffect(() => {
    if (result?.data.kind === "profile_nodes") {
      setNodes(projectProfileNodes(result.data.value.nodes))
      setNativeStateCounts(null)
    }
    if (result?.data.kind === "native_profile_records") {
      setNodes(nativeProfileNodes(result.data.value.nodes))
      setNativeStateCounts(result.data.value.counts)
    }
    if (result?.data.kind === "profile_bundle") {
      const preview = vmmProfileBundlePreview(result.data.value, effectiveTarget)
      setBundleModeError(preview === null)
      setBundlePreview(preview ?? "")
    }
    if (result?.data.kind === "profile_instruction") {
      setNodes(projectProfileNodes(result.data.value.accepted_nodes))
    }
    if (result?.data.kind === "native_project_catalog") {
      setNativeCatalog(result.data.value)
    }
  }, [effectiveTarget, result])

  // refreshProfile loads both atomic entries and the final profile preview.
  // refreshProfile 同时加载原子条目与最终画像预览。
  const refreshProfile = React.useCallback(async () => {
    if (!profileCatalogReady) {
      await onRunAction(vmmProfileCatalogAction(snapshot.experienceState.deployment))
      return
    }
    if (!selectedTargetRef) {
      return
    }
    const identityPayload = isNative
      ? { projectRef: selectedProjectRef }
      : { userRef: selectedUserRef, projectRef: selectedProjectRef }
    const nodesResult = await onRunAction(
      vmmProfileNodeAction(snapshot.experienceState.deployment),
      isNative
        ? { vmmProjectId: Number(selectedProjectRef), target: effectiveTarget }
        : { ...identityPayload, target: effectiveTarget },
    )
    if (!nodesResult) {
      return
    }
    await onRunAction("load_profile_bundle", identityPayload)
  }, [
    effectiveTarget,
    isNative,
    onRunAction,
    profileCatalogReady,
    selectedProjectRef,
    selectedTargetRef,
    selectedUserRef,
    snapshot.experienceState.deployment,
  ])

  React.useEffect(() => {
    if (busy || !profileCatalogReady || !selectedTargetRef) {
      return
    }
    const nextAutomaticLoadKey = [
      effectiveTarget,
      selectedUserRef,
      selectedProjectRef,
    ].join("\u0000")
    if (automaticLoadKey.current === nextAutomaticLoadKey) {
      return
    }
    automaticLoadKey.current = nextAutomaticLoadKey
    // Switching explicit catalog targets immediately reveals that profile without another manual step.
    // 切换明确目录目标后立即展示对应画像，不要求用户再执行一次手动刷新。
    void refreshProfile()
  }, [
    busy,
    effectiveTarget,
    profileCatalogReady,
    refreshProfile,
    selectedProjectRef,
    selectedTargetRef,
    selectedUserRef,
  ])

  // applyInstruction submits one natural-language correction and refreshes the final preview.
  // applyInstruction 提交一条自然语言修正并刷新最终预览。
  const applyInstruction = React.useCallback(async () => {
    const normalizedInstruction = instruction.trim()
    if (!normalizedInstruction || !profileCatalogReady || !selectedTargetRef) {
      return
    }
    const identityPayload = isNative
      ? { projectRef: selectedProjectRef }
      : { userRef: selectedUserRef, projectRef: selectedProjectRef }
    const applied = await onRunAction("apply_profile_instruction", {
      ...identityPayload,
      target: effectiveTarget,
      instruction: normalizedInstruction,
    })
    if (!applied) {
      return
    }
    setInstruction("")
    setCorrectionOpen(false)
    // A profile instruction returns only changed nodes; reload the complete active node list and preview.
    // 画像指令只返回发生变化的节点；随后重新加载完整活跃节点列表与预览。
    await refreshProfile()
  }, [
    effectiveTarget,
    isNative,
    instruction,
    onRunAction,
    profileCatalogReady,
    refreshProfile,
    selectedProjectRef,
    selectedTargetRef,
    selectedUserRef,
  ])

  return (
    <div className="flex w-full min-w-0 flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.profile.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {profileDescription}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
        <Button disabled={busy || !profileCatalogReady || !selectedTargetRef} onClick={() => setCorrectionOpen(true)}>{t("memory.profile.instruction")}</Button>
        <Button
          disabled={busy}
          onClick={() => void refreshProfile()}
          type="button"
          variant="outline"
        >
          <RefreshCw className="size-4" aria-hidden="true" />
          {t("memory.profile.refresh")}
        </Button></div>
      </div>

      <Card className="vmm-profile-target">
        <CardHeader>
          <CardTitle>{t("memory.profile.target")}</CardTitle>
          <CardDescription>
            {selectedTargetDescription
              ?? (effectiveTarget === "user"
                ? t("memory.profile.noUser")
                : t("memory.profile.noProject"))}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <RadioGroup
            className="flex flex-wrap gap-3"
            disabled={busy || !profileCatalogReady}
            onValueChange={(value) => {
              setTarget(value as "user" | "project")
              setNodes([])
              setNativeStateCounts(null)
              setBundlePreview("")
              setBundleModeError(false)
            }}
            value={target}
          >
            <label className="flex cursor-pointer items-center gap-3 rounded-lg border p-3">
              <RadioGroupItem value="user" />
              <span className="text-sm font-medium">{t("memory.profile.user")}</span>
            </label>
            <label className="flex cursor-pointer items-center gap-3 rounded-lg border p-3">
              <RadioGroupItem value="project" />
              <span className="text-sm font-medium">{t("memory.profile.project")}</span>
            </label>
          </RadioGroup>
          {usesFixedNativeUser ? (
            <div className="space-y-1 rounded-lg border bg-muted/30 px-3 py-2.5">
              <p className="text-sm font-medium">{t("memory.profile.nativeUserName")}</p>
              <p className="text-xs text-muted-foreground">
                {t("memory.profile.nativeUserDescription")}
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              <Label>
                {effectiveTarget === "user"
                  ? t("memory.profile.selectUser")
                  : t("memory.profile.selectProject")}
              </Label>
              <Combobox
                ariaLabel={effectiveTarget === "user"
                  ? t("memory.profile.selectUser")
                  : t("memory.profile.selectProject")}
                disabled={busy || !profileCatalogReady}
                emptyText={effectiveTarget === "user"
                  ? t("memory.profile.noUser")
                  : t("memory.profile.noProject")}
                onValueChange={(value) => {
                  setNodes([])
                  setNativeStateCounts(null)
                  setBundlePreview("")
                  setBundleModeError(false)
                  if (effectiveTarget === "user") {
                    setSelectedUserRef(value)
                  } else {
                    setSelectedProjectRef(value)
                  }
                }}
                options={effectiveTarget === "user" ? userOptions : projectOptions}
                placeholder={effectiveTarget === "user"
                  ? t("memory.profile.selectUser")
                  : t("memory.profile.selectProject")}
                searchPlaceholder={effectiveTarget === "user"
                  ? t("memory.profile.searchUser")
                  : t("memory.profile.searchProject")}
                value={selectedTargetRef}
              />
            </div>
          )}
        </CardContent>
      </Card>

      {isNative && nativeStateCounts ? (
        <Card className="vmm-profile-counts">
          <CardHeader>
            <CardTitle>{t("memory.profile.stateSummary")}</CardTitle>
            <CardDescription>{t("memory.profile.stateSummaryDescription")}</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
            <ProfileStateMetric label={t("memory.profile.stateActive")} value={nativeStateCounts.active} />
            <ProfileStateMetric label={t("memory.profile.statePending")} value={nativeStateCounts.pending} />
            <ProfileStateMetric label={t("memory.profile.stateScheduled")} value={nativeStateCounts.scheduled} />
            <ProfileStateMetric label={t("memory.profile.stateInactive")} value={nativeStateCounts.inactive} />
            <ProfileStateMetric label={t("memory.profile.stateExpired")} value={nativeStateCounts.expired} />
          </CardContent>
        </Card>
      ) : null}

      {nodes.length === 0 ? (
        <Empty className="min-h-48" variant="outlined">
          <EmptyHeader>
            <EmptyMedia variant="icon"><BrainCircuit className="size-5" /></EmptyMedia>
            <EmptyTitle>{t("memory.profile.title")}</EmptyTitle>
            <EmptyDescription>{t("memory.profile.empty")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="vmm-profile-list">
          {nodes.map((node, index) => (
            <Card key={`${node.content}-${index}`}>
              <CardContent className="space-y-3 pt-5">
                <p className="whitespace-pre-wrap text-sm">{node.content}</p>
                <div className="flex flex-wrap gap-2">
                  {node.state ? (
                    <Badge variant={node.state === "active" ? "secondary" : "outline"}>
                      {t(nativeProfileStateKey(node.state))}
                    </Badge>
                  ) : null}
                  {node.activeForInjection === false ? (
                    <Badge variant="outline">{t("memory.profile.notInjected")}</Badge>
                  ) : null}
                  {node.priority ? (
                    <Badge variant="secondary">
                      {t("memory.profile.priority")}{t("global.labelSeparator")}{node.priority}
                    </Badge>
                  ) : null}
                  {node.level ? (
                    <Badge variant="outline">
                      {t("memory.profile.level")}{t("global.labelSeparator")}{node.level}
                    </Badge>
                  ) : null}
                  {node.date ? <span className="text-xs text-muted-foreground">{node.date}</span> : null}
                </div>
                {node.sourceKind ? (
                  <div className="space-y-1 text-xs text-muted-foreground">
                    <p>
                      {t("memory.profile.source")}{t("global.labelSeparator")}
                      {t(nativeProfileSourceKey(node.sourceKind))}
                      {node.sourceId ? ` · ${node.sourceId}` : ""}
                    </p>
                    {node.sourceTurnIds?.length ? (
                      <p>
                        {t("memory.profile.sourceTurns")}{t("global.labelSeparator")}
                        {node.sourceTurnIds.map((turnId) => `#${turnId}`).join(", ")}
                      </p>
                    ) : null}
                    {node.statusReason ? (
                      <p>
                        {t("memory.profile.statusReason")}{t("global.labelSeparator")}
                        {node.statusReason}
                      </p>
                    ) : null}
                    {node.updatedAtUnixMs != null ? (
                      <p>
                        {t("memory.profile.updatedAt")}{t("global.labelSeparator")}
                        {new Date(node.updatedAtUnixMs).toLocaleString()}
                      </p>
                    ) : null}
                    {node.storedState && node.state && node.storedState !== node.state ? (
                      <p>
                        {t("memory.profile.storedState")}{t("global.labelSeparator")}
                        {t(nativeProfileStateKey(node.storedState))}
                      </p>
                    ) : null}
                  </div>
                ) : null}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={correctionOpen} onOpenChange={(open) => { if (!busy) setCorrectionOpen(open) }}>
        <DialogContent className="sm:max-w-xl" onInteractOutside={(event) => { if (busy) event.preventDefault() }} onEscapeKeyDown={(event) => { if (busy) event.preventDefault() }}>
        <DialogHeader>
          <DialogTitle>{t("memory.profile.instruction")}</DialogTitle>
          <DialogDescription>{t(effectiveTarget === "user" ? "memory.profile.user" : "memory.profile.project")} · {selectedTargetDescription}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <Label htmlFor="vmm-profile-instruction">{t("memory.profile.instruction")}</Label>
          <Textarea
            className="min-h-28"
            disabled={busy}
            id="vmm-profile-instruction"
            onChange={(event) => setInstruction(event.target.value)}
            placeholder={t("memory.profile.instructionPlaceholder")}
            value={instruction}
          />
          <div className="flex justify-end">
            <Button
              disabled={
                busy
                || !profileCatalogReady
                || !selectedTargetRef
                || !instruction.trim()
              }
              onClick={() => void applyInstruction()}
              type="button"
            >
              <Send className="size-4" aria-hidden="true" />
              {t("memory.profile.applyInstruction")}
            </Button>
          </div>
        </div>
        </DialogContent>
      </Dialog>

      {bundleModeError ? <p role="alert" className="text-sm text-destructive">{t("memory.profile.invalidBundleMode")}</p> : null}
      {bundlePreview ? (
        <Accordion className="rounded-xl border px-4" collapsible type="single">
          <AccordionItem value="bundle">
            <AccordionTrigger>{t("memory.profile.preview")}</AccordionTrigger>
            <AccordionContent>
              <p className="mb-3 text-sm text-muted-foreground">
                {t(isNative
                  ? "memory.profile.nativePreviewDescription"
                  : "memory.profile.previewDescription")}
              </p>
            <p className="whitespace-pre-wrap rounded-lg border bg-muted/30 p-3 text-sm">
              {bundlePreview}
            </p>
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      ) : null}
    </div>
  )
}

// ProfileStateMetricProps defines one compact native profile-state counter.
// ProfileStateMetricProps 定义一个紧凑的原生画像状态计数器。
interface ProfileStateMetricProps {
  label: string
  value: number
}

// ProfileStateMetric renders one lifecycle count without exposing transport details.
// ProfileStateMetric 渲染一个生命周期计数且不暴露传输细节。
function ProfileStateMetric({ label, value }: ProfileStateMetricProps) {
  return (
    <div className="rounded-lg border bg-muted/20 px-3 py-2.5">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-lg font-semibold tabular-nums">{value}</p>
    </div>
  )
}

// nativeProfileStateKey maps one closed native state token to localized user text.
// nativeProfileStateKey 将一个封闭的原生状态标记映射为本地化用户文本。
function nativeProfileStateKey(
  state: VmmNativeProfileRecordSummary["effectiveState"]
    | VmmNativeProfileRecordSummary["storedState"],
) {
  switch (state) {
    case "active": return "memory.profile.stateActive" as const
    case "inactive": return "memory.profile.stateInactive" as const
    case "pending": return "memory.profile.statePending" as const
    case "expired": return "memory.profile.stateExpired" as const
    case "scheduled": return "memory.profile.stateScheduled" as const
  }
}

// nativeProfileSourceKey maps one closed native provenance token to localized user text.
// nativeProfileSourceKey 将一个封闭的原生来源标记映射为本地化用户文本。
function nativeProfileSourceKey(sourceKind: VmmNativeProfileRecordSummary["sourceKind"]) {
  switch (sourceKind) {
    case "turn_extract": return "memory.profile.sourceTurnExtract" as const
    case "manual_instruction": return "memory.profile.sourceManualInstruction" as const
    case "system_seed": return "memory.profile.sourceSystemSeed" as const
    case "retained_after_subject_delete": return "memory.profile.sourceRetained" as const
  }
}

// nativeProfileNodes converts native all-state records into detailed profile cards.
// nativeProfileNodes 将原生全状态记录转换为详细画像卡片。
function nativeProfileNodes(nodes: VmmNativeProfileRecordSummary[]): ProfileNodeView[] {
  return nodes
    .filter((node) => node.content.trim().length > 0)
    .map((node) => ({
      content: node.content,
      priority: String(node.priority),
      level: node.level,
      date: node.effectiveDateAtUnixMs == null
        ? ""
        : new Date(node.effectiveDateAtUnixMs).toLocaleString(),
      state: node.effectiveState,
      storedState: node.storedState,
      activeForInjection: node.activeForInjection,
      sourceKind: node.sourceKind,
      sourceId: node.sourceId,
      sourceTurnIds: node.sourceTurnIds,
      statusReason: node.statusReason,
      updatedAtUnixMs: node.updatedAtUnixMs,
    }))
}

// projectProfileNodes converts typed profile entries into identifier-free card data.
// projectProfileNodes 将强类型画像条目转换为不含标识符的卡片数据。
function projectProfileNodes(nodes: VmmProfileNodeActionValue[]): ProfileNodeView[] {
  return nodes
    .filter((node) => node.content.trim().length > 0)
    .map((node) => ({
      content: node.content,
      priority: node.priority,
      level: node.level,
      date: node.profile_date,
    }))
}

export { VmmProfilePage }
export type { VmmProfilePageProps }
