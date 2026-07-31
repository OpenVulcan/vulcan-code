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
  const [nodes, setNodes] = React.useState<ProfileNodeView[]>([])
  const [instruction, setInstruction] = React.useState("")
  const [bundlePreview, setBundlePreview] = React.useState(snapshot.runtime.profilePreview)

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
    const nodesResult = await onRunAction("load_profile_nodes", { target })
    if (!nodesResult) {
      return
    }
    await onRunAction("load_profile_bundle")
  }, [onRunAction, target])

  React.useEffect(() => {
    // Switching targets should immediately reveal the selected profile without another manual step.
    // 切换目标后应立即展示所选画像，不要求用户再执行一次手动刷新。
    void refreshProfile()
  }, [refreshProfile])

  // applyInstruction submits one natural-language correction and refreshes the final preview.
  // applyInstruction 提交一条自然语言修正并刷新最终预览。
  const applyInstruction = React.useCallback(async () => {
    const normalizedInstruction = instruction.trim()
    if (!normalizedInstruction) {
      return
    }
    const applied = await onRunAction("apply_profile_instruction", {
      target,
      instruction: normalizedInstruction,
    })
    if (!applied) {
      return
    }
    setInstruction("")
    await onRunAction("load_profile_bundle")
  }, [instruction, onRunAction, target])

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.profile.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t("memory.profile.description")}
          </p>
        </div>
        <Button disabled={busy} onClick={() => void refreshProfile()} type="button" variant="outline">
          <RefreshCw className="size-4" aria-hidden="true" />
          {t("memory.profile.refresh")}
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t("memory.profile.target")}</CardTitle>
          <CardDescription>
            {target === "user"
              ? snapshot.displayIdentity.userName ?? t("vmm.experience.noUser")
              : snapshot.displayIdentity.projectName ?? t("vmm.experience.noProject")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <RadioGroup
            className="grid gap-3 sm:grid-cols-2"
            onValueChange={(value) => {
              setTarget(value as "user" | "project")
              setNodes([])
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
        </CardContent>
      </Card>

      {nodes.length === 0 ? (
        <Empty className="min-h-48 border">
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
              disabled={busy || !instruction.trim()}
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
