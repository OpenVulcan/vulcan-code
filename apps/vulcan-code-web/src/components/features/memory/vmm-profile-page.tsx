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
import { Textarea } from "@/components/ui/textarea"
import { useI18n } from "@/i18n"
import type {
  VmmMemoryActionResult,
  VmmMemorySnapshot,
  VmmNativeProjectCatalogSummary,
  VmmProfileBundleActionValue,
  VmmProfileNodeActionValue,
} from "@/types"

// NATIVE_FIXED_USER_TARGET is a UI-only identity for the single host-owned native memory user.
// NATIVE_FIXED_USER_TARGET 是原生记忆唯一宿主用户的纯界面身份。
const NATIVE_FIXED_USER_TARGET = "__native_fixed_user__"

// ProfileNodeView is the user-facing projection of one active profile node.
// ProfileNodeView 是单个有效画像节点的用户可见投影。
interface ProfileNodeView {
  content: string
  priority: string
  level: string
  date: string
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

// vmmProfileBundlePreview selects the profile section matching the visible target.
// vmmProfileBundlePreview 选择与当前可见目标匹配的画像段落。
//
// Parameters: bundle is the exact VMM response and target is the visible profile class.
// 参数：bundle 是准确的 VMM 响应，target 是当前可见画像类别。
//
// Returns: only the user profile or project profile text requested by the page.
// 返回：仅返回页面请求的用户画像或项目画像文本。
export function vmmProfileBundlePreview(
  bundle: VmmProfileBundleActionValue,
  target: "user" | "project",
): string {
  return target === "user" ? bundle.user_profile : bundle.project_profile
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
  const isNative = vmmProfileUsesNativeProjectCatalog(snapshot.experienceState.deployment)
  const [target, setTarget] = React.useState<"user" | "project">("user")
  // selectedUserRef is the explicit VMM catalog user being inspected.
  // selectedUserRef 是当前明确查看的 VMM 目录用户。
  const [selectedUserRef, setSelectedUserRef] = React.useState(snapshot.globalDraft.userId)
  // selectedProjectRef is the explicit VMM catalog project being inspected.
  // selectedProjectRef 是当前明确查看的 VMM 目录项目。
  const [selectedProjectRef, setSelectedProjectRef] = React.useState(snapshot.globalDraft.projectId)
  const [nodes, setNodes] = React.useState<ProfileNodeView[]>([])
  const [instruction, setInstruction] = React.useState("")
  const [bundlePreview, setBundlePreview] = React.useState(snapshot.runtime.profilePreview)
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
    setBundlePreview("")
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
    }
    if (result?.data.kind === "profile_bundle") {
      setBundlePreview(vmmProfileBundlePreview(result.data.value, effectiveTarget))
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
    const nodesResult = await onRunAction("load_profile_nodes", {
      ...identityPayload,
      target: effectiveTarget,
    })
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
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.profile.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {profileDescription}
          </p>
        </div>
        <Button
          disabled={busy}
          onClick={() => void refreshProfile()}
          type="button"
          variant="outline"
        >
          <RefreshCw className="size-4" aria-hidden="true" />
          {t("memory.profile.refresh")}
        </Button>
      </div>

      <Card>
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
            className="grid gap-3 sm:grid-cols-2"
            disabled={busy || !profileCatalogReady}
            onValueChange={(value) => {
              setTarget(value as "user" | "project")
              setNodes([])
              setBundlePreview("")
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
                  setBundlePreview("")
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

      {nodes.length === 0 ? (
        <Empty className="min-h-48" variant="outlined">
          <EmptyHeader>
            <EmptyMedia variant="icon"><BrainCircuit className="size-5" /></EmptyMedia>
            <EmptyTitle>{t("memory.profile.title")}</EmptyTitle>
            <EmptyDescription>{t("memory.profile.empty")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {nodes.map((node, index) => (
            <Card key={`${node.content}-${index}`}>
              <CardContent className="space-y-3 pt-5">
                <p className="whitespace-pre-wrap text-sm">{node.content}</p>
                <div className="flex flex-wrap gap-2">
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
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle>{t("memory.profile.instruction")}</CardTitle>
          <CardDescription>{profileDescription}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
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
        </CardContent>
      </Card>

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
