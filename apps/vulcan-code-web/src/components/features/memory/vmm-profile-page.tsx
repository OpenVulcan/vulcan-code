import * as React from "react"
import { BrainCircuit, RefreshCw, Send } from "lucide-react"

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Button } from "@/components/ui/button"
import { PageTabs } from "@/components/workspace/workspace-directory-layout"
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
import { formatDateTime } from "@/lib/display-format"

// NATIVE_FIXED_USER_TARGET is a UI-only identity for the single host-owned native memory user.
// NATIVE_FIXED_USER_TARGET 是原生记忆唯一宿主用户的纯界面身份。
const NATIVE_FIXED_USER_TARGET = "__native_fixed_user__"

// ProfileNodeView is the user-facing projection of one active or audit-only profile node.
// ProfileNodeView 是单个活动或仅供审计画像节点的用户可见投影。
interface ProfileNodeView {
  content: string
  priority: string
  level: string
  // Typed project profile date, shown as provided by the memory service.
  // 强类型项目画像日期，按记忆服务提供的原样显示。
  date: string
  // Native effective time, formatted at render time so it follows the current locale.
  // 原生生效时间，在渲染时格式化以跟随当前语言。
  effectiveAtUnixMs?: number | null
  expiresAtUnixMs?: number | null
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
  // notice preserves the controller failure detail for this visible profile page.
  // notice 保留当前画像页面控制器的具体失败信息。
  notice?: string
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
  notice = "",
  onRunAction,
}: VmmProfilePageProps) {
  const { t, locale } = useI18n()
  // readState distinguishes pending, failed and confirmed-empty profile content.
  // readState 区分加载中、失败与已确认空画像内容。
  const [readState, setReadState] = React.useState<"idle" | "loading" | "failed" | "ready">("idle")
  // readError retains local contract failures when shared page feedback is cleared.
  // readError 在共享页面反馈清除后仍保留局部契约错误。
  const [readError, setReadError] = React.useState("")
  // readGeneration rejects responses belonging to a previous backend or unmounted page.
  // readGeneration 拒绝先前后端或已经卸载页面的响应。
  const readGeneration = React.useRef(0)
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
  // Share the exact in-flight directory read across effect replay; only the latest reader publishes it.
  // 副作用重放共用同一在途目录读取，仅由最新读取方发布结果。
  const catalogRequest = React.useRef<{
    // Deployment fixes the authority of this directory request.
    // 部署模式固定本次目录请求的权威来源。
    deployment: VmmMemorySnapshot["experienceState"]["deployment"]
    // Promise carries the original response through the exclusive management channel.
    // Promise 携带独占管理通道返回的原始响应。
    promise: Promise<VmmMemoryActionResult | null>
  } | null>(null)

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
      description: project.tag === project.name ? undefined : project.tag,
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
    setReadState("idle")
    setReadError("")
    readGeneration.current += 1
    return () => { readGeneration.current += 1 }
  }, [isNative])

  // loadCatalog consumes responses directly so page feedback resets cannot discard a valid directory.
  // loadCatalog 直接消费响应，防止页面反馈重置丢弃有效目录。
  const loadCatalog = React.useCallback(async () => {
    // generation binds this asynchronous read to the active backend lifetime.
    // generation 将此次异步读取绑定到当前后端生命周期。
    const generation = readGeneration.current
    setReadState("loading")
    setReadError("")
    // pending reuses one request only for the identical backend, never a cached response from another mode.
    // pending 仅复用完全相同后端的一次请求，不复用其他模式的缓存响应。
    let pending = catalogRequest.current
    if (!pending || pending.deployment !== snapshot.experienceState.deployment) {
      pending = {
        deployment: snapshot.experienceState.deployment,
        promise: onRunAction(vmmProfileCatalogAction(snapshot.experienceState.deployment)),
      }
      catalogRequest.current = pending
    }
    try {
      const response = await pending.promise
      if (generation !== readGeneration.current) return
      if (isNative) {
        if (response?.data.kind !== "native_project_catalog") throw new Error(t("memory.profile.memoryProjectDirectoryCould"))
        setNativeCatalog(response.data.value)
      } else if (response?.data.kind !== "users_projects") {
        throw new Error(t("memory.profile.userAndProjectDirectory"))
      }
      setReadState("idle")
    } catch (error) {
      if (generation !== readGeneration.current) return
      setReadError(error instanceof Error ? error.message : String(error))
      setReadState("failed")
    } finally {
      if (catalogRequest.current === pending) catalogRequest.current = null
    }
  }, [isNative, onRunAction, snapshot.experienceState.deployment, t])

  React.useEffect(() => {
    if (!isNative || busy || nativeCatalog || nativeCatalogLoadIssued.current) {
      return
    }
    nativeCatalogLoadIssued.current = true
    void loadCatalog()
  }, [busy, isNative, nativeCatalog, loadCatalog])

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
    : isNative
      ? nativeCatalog?.projects.find((project) => String(project.projectId) === selectedProjectRef)?.name
      : projectOptions.find((project) => project.value === selectedProjectRef)?.description

  // refreshProfile publishes direct target responses and confirms empty content only after successful reads.
  // refreshProfile 发布目标直接响应，且仅在读取成功后确认空内容。
  const refreshProfile = React.useCallback(async () => {
    if (!profileCatalogReady) { await loadCatalog(); return }
    if (!selectedTargetRef) return
    // generation owns both sequential reads and blocks stale publication after navigation.
    // generation 持有两次顺序读取，并阻止导航后的旧响应发布。
    const generation = ++readGeneration.current
    const identityPayload = isNative ? { projectRef: selectedProjectRef } : { userRef: selectedUserRef, projectRef: selectedProjectRef }
    setReadState("loading")
    setReadError("")
    setBundlePreview("")
    try {
      const nodesResult = await onRunAction(vmmProfileNodeAction(snapshot.experienceState.deployment), isNative
        ? { vmmProjectId: Number(selectedProjectRef), target: effectiveTarget }
        : { ...identityPayload, target: effectiveTarget })
      if (generation !== readGeneration.current) return
      if (isNative && nodesResult?.data.kind === "native_profile_records") {
        setNodes(nativeProfileNodes(nodesResult.data.value.nodes))
        setNativeStateCounts(nodesResult.data.value.counts)
      } else if (!isNative && nodesResult?.data.kind === "profile_nodes") {
        setNodes(projectProfileNodes(nodesResult.data.value.nodes))
        setNativeStateCounts(null)
      } else {
        throw new Error(t("memory.profile.profileCouldNotBe"))
      }
      const bundle = await onRunAction("load_profile_bundle", identityPayload)
      if (generation !== readGeneration.current) return
      if (bundle?.data.kind !== "profile_bundle") throw new Error(t("memory.profile.profilePreviewCouldNot"))
      const preview = vmmProfileBundlePreview(bundle.data.value, effectiveTarget)
      setBundleModeError(preview === null)
      setBundlePreview(preview ?? "")
      setReadState("ready")
    } catch (error) {
      if (generation !== readGeneration.current) return
      setReadError(error instanceof Error ? error.message : String(error))
      setReadState("failed")
    }
  }, [effectiveTarget, isNative, loadCatalog, onRunAction, profileCatalogReady, selectedProjectRef, selectedTargetRef, selectedUserRef, snapshot.experienceState.deployment, t])

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
      {/* The profile target is the page's primary view switch, so it uses the shared page tab strip.
          画像目标是本页的主视图切换，因此使用共享页面标签条。 */}
      <PageTabs
        actions={<>
          <Button disabled={busy || readState === "loading" || !profileCatalogReady || !selectedTargetRef} onClick={() => setCorrectionOpen(true)} size="sm">{t("memory.profile.instruction")}</Button>
          <Button
            disabled={busy || readState === "loading"}
            onClick={() => void refreshProfile()}
            size="sm"
            type="button"
            variant="outline"
          >
            <RefreshCw className="size-4" aria-hidden="true" />
            {t("memory.profile.refresh")}
          </Button>
        </>}
        ariaLabel={t("memory.profile.target")}
        className="vmm-page-tabs"
        onValueChange={(value) => {
          setTarget(value)
          setNodes([])
          setNativeStateCounts(null)
          setBundlePreview("")
          setBundleModeError(false)
        }}
        options={(["user", "project"] as const).map((value) => ({
          disabled: busy || readState === "loading" || !profileCatalogReady,
          label: value === "user" ? t("memory.profile.user") : t("memory.profile.project"),
          value,
        }))}
        value={target}
      />

      {notice ? <p role="status" className="text-sm text-muted-foreground">{notice}</p> : null}
      {readError ? <p role="alert" className="text-sm text-destructive">{readError}</p> : null}
      {!profileCatalogReady && readState === "idle" ? <p role="status" className="text-sm text-muted-foreground">{t("memory.profile.createAnAvailableMemory")}</p> : null}

      <section className="vmm-profile-target" aria-label={t("memory.profile.target")}>
          {usesFixedNativeUser ? (
            <div className="vmm-profile-identity space-y-1">
              <p className="text-sm font-medium">{t("memory.profile.nativeUserName")}</p>
              <p className="text-xs text-muted-foreground">
                {t("memory.profile.nativeUserDescription")}
              </p>
            </div>
          ) : (
            <div className="vmm-profile-picker space-y-2">
              <Label>
                {effectiveTarget === "user"
                  ? t("memory.profile.selectUser")
                  : t("memory.profile.selectProject")}
              </Label>
              <Combobox
                ariaLabel={effectiveTarget === "user"
                  ? t("memory.profile.selectUser")
                  : t("memory.profile.selectProject")}
                disabled={busy || readState === "loading" || !profileCatalogReady}
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
        {selectedTargetDescription && !isNative ? <p className="vmm-profile-target-description text-xs text-muted-foreground">{selectedTargetDescription}</p> : null}
      </section>

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

      {readState === "loading" ? <StreamPlaceholder label={t("memory.resources.loading")} shape="rows" count={4} /> : nodes.length === 0 && readState !== "ready" ? null : nodes.length === 0 ? (
        <Empty className="min-h-48" variant="outlined">
          <EmptyHeader>
            <EmptyMedia variant="icon"><BrainCircuit className="size-5" /></EmptyMedia>
            <EmptyTitle>{t("memory.profile.title")}</EmptyTitle>
            <EmptyDescription>{t("memory.profile.empty")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <ol className="vmm-profile-list" aria-label={t("memory.profile.title")}>
          {nodes.map((node, index) => (
            <ProfileNodeItem key={`${node.content}-${index}`} locale={locale} node={node} />
          ))}
        </ol>
      )}

      <Dialog open={correctionOpen} onOpenChange={(open) => { if (!busy) setCorrectionOpen(open) }}>
        <DialogContent className="sm:max-w-xl" onInteractOutside={(event) => { if (busy) event.preventDefault() }} onEscapeKeyDown={(event) => { if (busy) event.preventDefault() }}>
        <DialogHeader>
          <DialogTitle>{t("memory.profile.instruction")}</DialogTitle>
          <DialogDescription>{t(effectiveTarget === "user" ? "memory.profile.user" : "memory.profile.project")} · {selectedTargetDescription}</DialogDescription>
        </DialogHeader>
        {notice ? <p className="text-sm text-muted-foreground" role="status">{notice}</p> : null}
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

// ProfileNodeFact is one labelled provenance or timing value of a profile node.
// ProfileNodeFact 是画像节点的一项带标签的来源或时间信息。
interface ProfileNodeFact {
  label: string
  value: React.ReactNode
}

// ProfileNodeItem renders one profile node as a structured record: state and weight, the statement, then labelled facts.
// ProfileNodeItem 将单个画像节点渲染为结构化记录：状态与权重、陈述内容，以及带标签的信息。
function ProfileNodeItem({ locale, node }: { locale: string; node: ProfileNodeView }) {
  const { t } = useI18n()
  const separator = t("global.labelSeparator")
  const levelKey = profileLevelKey(node.level)
  const facts: ProfileNodeFact[] = []
  if (node.sourceKind) {
    facts.push({
      label: t("memory.profile.source"),
      value: (
        <>
          {t(nativeProfileSourceKey(node.sourceKind))}
          {node.sourceId ? <code className="vmm-profile-node-ref">{node.sourceId}</code> : null}
        </>
      ),
    })
  }
  if (node.sourceTurnIds?.length) {
    facts.push({ label: t("memory.profile.sourceTurns"), value: node.sourceTurnIds.map((turnId) => `#${turnId}`).join(", ") })
  }
  const effectiveAt = node.effectiveAtUnixMs == null ? null : formatDateTime(node.effectiveAtUnixMs, locale)
  if (effectiveAt) {
    facts.push({ label: t("memory.profile.effectiveAt"), value: effectiveAt })
  } else if (node.date) {
    facts.push({ label: t("memory.profile.date"), value: node.date })
  }
  const expiresAt = node.expiresAtUnixMs == null ? null : formatDateTime(node.expiresAtUnixMs, locale)
  if (expiresAt) {
    facts.push({ label: t("memory.profile.expiresAt"), value: expiresAt })
  }
  const updatedAt = node.updatedAtUnixMs == null ? null : formatDateTime(node.updatedAtUnixMs, locale)
  if (updatedAt) {
    facts.push({ label: t("memory.profile.updatedAt"), value: updatedAt })
  }
  if (node.storedState && node.state && node.storedState !== node.state) {
    facts.push({ label: t("memory.profile.storedState"), value: t(nativeProfileStateKey(node.storedState)) })
  }
  if (node.statusReason) {
    facts.push({ label: t("memory.profile.statusReason"), value: node.statusReason })
  }
  return (
    <li className="vmm-profile-node" data-state={node.state}>
      <div className="vmm-profile-node-head">
        {node.state ? (
          <span className="vmm-profile-node-state">
            <span className="vmm-profile-node-dot" aria-hidden="true" />
            {t(nativeProfileStateKey(node.state))}
          </span>
        ) : null}
        {node.activeForInjection === false ? (
          <span className="vmm-profile-node-tag">{t("memory.profile.notInjected")}</span>
        ) : null}
        {node.priority || node.level ? (
          <span className="vmm-profile-node-weights">
            {node.priority ? (
              <span className="vmm-profile-node-tag">
                {t("memory.profile.priority")}{separator}<strong>{node.priority}</strong>
              </span>
            ) : null}
            {node.level ? (
              <span className="vmm-profile-node-tag">
                {t("memory.profile.level")}{separator}<strong>{levelKey ? t(levelKey) : node.level}</strong>
              </span>
            ) : null}
          </span>
        ) : null}
      </div>
      <p className="vmm-profile-node-content">{node.content}</p>
      {facts.length > 0 ? (
        <dl className="vmm-profile-node-facts">
          {facts.map((fact) => (
            <div key={fact.label}>
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
    </li>
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

// profileLevelKey maps a known profile level token to localized user text; unknown service tokens stay verbatim.
// profileLevelKey 将已知画像层级标记映射为本地化用户文本；未知服务标记保持原样。
function profileLevelKey(level: string) {
  switch (level) {
    case "transient": return "memory.profile.levelTransient" as const
    case "situational": return "memory.profile.levelSituational" as const
    case "stable": return "memory.profile.levelStable" as const
    case "persistent": return "memory.profile.levelPersistent" as const
    default: return null
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
      date: "",
      effectiveAtUnixMs: node.effectiveDateAtUnixMs,
      expiresAtUnixMs: node.expiresAtUnixMs,
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
