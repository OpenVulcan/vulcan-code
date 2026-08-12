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
  VmmProfileNodeActionValue,
} from "@/types"

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

// VmmProfilePage renders current profile content without internal node identifiers.
// VmmProfilePage 渲染不含内部节点标识的当前画像内容。
function VmmProfilePage({
  busy,
  snapshot,
  result,
  onRunAction,
}: VmmProfilePageProps) {
  const { t } = useI18n()
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
  // automaticLoadKey prevents one completed profile request from retriggering on busy-state changes.
  // automaticLoadKey 防止一次已完成画像请求因忙碌状态变化而重复触发。
  const automaticLoadKey = React.useRef("")

  // userOptions exposes human-readable VMM users without leaking their numeric transport ids.
  // userOptions 暴露人类可读的 VMM 用户，同时不显示数字传输标识。
  const userOptions = React.useMemo(() => snapshot.managementCatalog.users.map((user) => ({
    value: user.stableKey,
    label: user.displayName,
  })), [snapshot.managementCatalog.users])
  // projectOptions exposes canonical VMM paths so projects with the same leaf name remain distinct.
  // projectOptions 暴露规范 VMM 路径，确保同名末级项目仍可区分。
  const projectOptions = React.useMemo(() => snapshot.managementCatalog.projects.map((project) => ({
    value: project.stableKey,
    label: project.displayName,
    description: project.displayPath,
    keywords: [project.displayPath],
  })), [snapshot.managementCatalog.projects])
  // profileCatalogReady prevents profile calls from racing the first user/project directory load.
  // profileCatalogReady 防止画像调用与首次用户/项目目录加载发生竞争。
  const profileCatalogReady = userOptions.length > 0 && projectOptions.length > 0

  React.useEffect(() => {
    setSelectedUserRef((current) => {
      if (snapshot.managementCatalog.users.some((user) => user.stableKey === current)) {
        return current
      }
      return snapshot.managementCatalog.users.find(
        (user) => user.stableKey === snapshot.globalDraft.userId,
      )?.stableKey ?? snapshot.managementCatalog.users[0]?.stableKey ?? ""
    })
  }, [snapshot.globalDraft.userId, snapshot.managementCatalog.users])

  React.useEffect(() => {
    setSelectedProjectRef((current) => {
      if (snapshot.managementCatalog.projects.some((project) => project.stableKey === current)) {
        return current
      }
      return snapshot.managementCatalog.projects.find(
        (project) => project.stableKey === snapshot.globalDraft.projectId,
      )?.stableKey ?? snapshot.managementCatalog.projects[0]?.stableKey ?? ""
    })
  }, [snapshot.globalDraft.projectId, snapshot.managementCatalog.projects])

  const selectedTargetRef = target === "user" ? selectedUserRef : selectedProjectRef
  const selectedTargetDescription = target === "user"
    ? snapshot.managementCatalog.users.find((user) => user.stableKey === selectedUserRef)?.displayName
    : snapshot.managementCatalog.projects.find(
      (project) => project.stableKey === selectedProjectRef,
    )?.displayPath

  React.useEffect(() => {
    if (result?.data.kind === "profile_nodes") {
      setNodes(projectProfileNodes(result.data.value.nodes))
    }
    if (result?.data.kind === "profile_bundle") {
      setBundlePreview(result.data.value.combined_text)
    }
    if (result?.data.kind === "profile_instruction") {
      setNodes(projectProfileNodes(result.data.value.accepted_nodes))
    }
  }, [result])

  // refreshProfile loads both atomic entries and the final profile preview.
  // refreshProfile 同时加载原子条目与最终画像预览。
  const refreshProfile = React.useCallback(async () => {
    if (!profileCatalogReady) {
      await onRunAction("load_users_projects")
      return
    }
    if (!selectedTargetRef) {
      return
    }
    const identityPayload = {
      userRef: selectedUserRef,
      projectRef: selectedProjectRef,
    }
    const nodesResult = await onRunAction("load_profile_nodes", {
      ...identityPayload,
      target,
    })
    if (!nodesResult) {
      return
    }
    await onRunAction("load_profile_bundle", identityPayload)
  }, [
    onRunAction,
    profileCatalogReady,
    selectedProjectRef,
    selectedTargetRef,
    selectedUserRef,
    target,
  ])

  React.useEffect(() => {
    if (busy || !profileCatalogReady || !selectedTargetRef) {
      return
    }
    const nextAutomaticLoadKey = [
      target,
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
    profileCatalogReady,
    refreshProfile,
    selectedProjectRef,
    selectedTargetRef,
    selectedUserRef,
    target,
  ])

  // applyInstruction submits one natural-language correction and refreshes the final preview.
  // applyInstruction 提交一条自然语言修正并刷新最终预览。
  const applyInstruction = React.useCallback(async () => {
    const normalizedInstruction = instruction.trim()
    if (!normalizedInstruction || !profileCatalogReady || !selectedTargetRef) {
      return
    }
    const identityPayload = {
      userRef: selectedUserRef,
      projectRef: selectedProjectRef,
    }
    const applied = await onRunAction("apply_profile_instruction", {
      ...identityPayload,
      target,
      instruction: normalizedInstruction,
    })
    if (!applied) {
      return
    }
    setInstruction("")
    await onRunAction("load_profile_bundle", identityPayload)
  }, [
    instruction,
    onRunAction,
    profileCatalogReady,
    selectedProjectRef,
    selectedTargetRef,
    selectedUserRef,
    target,
  ])

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.profile.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t("memory.profile.description")}
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
              ?? (target === "user"
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
          <div className="space-y-2">
            <Label>
              {target === "user"
                ? t("memory.profile.selectUser")
                : t("memory.profile.selectProject")}
            </Label>
            <Combobox
              ariaLabel={target === "user"
                ? t("memory.profile.selectUser")
                : t("memory.profile.selectProject")}
              disabled={busy || !profileCatalogReady}
              emptyText={target === "user"
                ? t("memory.profile.noUser")
                : t("memory.profile.noProject")}
              onValueChange={(value) => {
                setNodes([])
                setBundlePreview("")
                if (target === "user") {
                  setSelectedUserRef(value)
                } else {
                  setSelectedProjectRef(value)
                }
              }}
              options={target === "user" ? userOptions : projectOptions}
              placeholder={target === "user"
                ? t("memory.profile.selectUser")
                : t("memory.profile.selectProject")}
              searchPlaceholder={target === "user"
                ? t("memory.profile.searchUser")
                : t("memory.profile.searchProject")}
              value={selectedTargetRef}
            />
          </div>
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
          <CardDescription>{t("memory.profile.description")}</CardDescription>
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
                {t("memory.profile.previewDescription")}
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
