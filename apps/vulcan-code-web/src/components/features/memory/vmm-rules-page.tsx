import * as React from "react"
import { BrainCircuit, Save, Settings2 } from "lucide-react"

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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import type { VmmConfigDraftSummary, VmmProjectScopeSummary } from "@/types"

// VmmRulesPageProps defines one editable rules draft and explicit application targets.
// VmmRulesPageProps 定义一份可编辑规则草稿与明确应用目标。
interface VmmRulesPageProps {
  busy: boolean
  draft: VmmConfigDraftSummary
  projects: VmmProjectScopeSummary[]
  notice: string
  onChange: (patch: Partial<VmmConfigDraftSummary>) => void
  onApply: (
    scope: "global" | "project",
    projectId: string | null,
    draft: VmmConfigDraftSummary,
  ) => Promise<boolean>
}

// VmmRulesPage renders task-oriented memory policies and a separate scope dialog.
// VmmRulesPage 渲染任务导向的记忆策略与独立应用范围弹窗。
function VmmRulesPage({
  busy,
  draft,
  projects,
  notice,
  onChange,
  onApply,
}: VmmRulesPageProps) {
  const { t } = useI18n()
  const [applyOpen, setApplyOpen] = React.useState(false)
  const [targetScope, setTargetScope] = React.useState<"global" | "project">("global")
  const [targetProjectId, setTargetProjectId] = React.useState("")

  // submitApplication persists the exact visible draft to the selected target.
  // submitApplication 将准确的可见草稿持久化到所选目标。
  const submitApplication = React.useCallback(async () => {
    const applied = await onApply(
      targetScope,
      targetScope === "project" ? targetProjectId : null,
      draft,
    )
    if (applied) {
      setApplyOpen(false)
    }
  }, [draft, onApply, targetProjectId, targetScope])

  const projectOptions = projects.map((project) => ({
    value: project.projectId,
    label: project.name,
    description: project.path,
  }))

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.rules.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t("memory.rules.description")}
          </p>
        </div>
        <Button disabled={busy} onClick={() => setApplyOpen(true)} type="button">
          <Save className="size-4" aria-hidden="true" />
          {t("memory.rules.applySettings")}
        </Button>
      </div>

      {notice ? (
        <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
          {notice}
        </p>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BrainCircuit className="size-4" aria-hidden="true" />
              {t("memory.rules.title")}
            </CardTitle>
            <CardDescription>{t("memory.rules.description")}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-1">
            <RuleSwitch
              checked={draft.enabled}
              description={t("memory.rules.enableDescription")}
              label={t("memory.rules.enable")}
              onCheckedChange={(enabled) => onChange({ enabled })}
            />
            <RuleSwitch
              checked={draft.precheckMode === "enabled" || draft.precheckEnabled}
              description={t("memory.rules.recallDescription")}
              label={t("memory.rules.recall")}
              onCheckedChange={(precheckEnabled) => onChange({
                precheckEnabled,
                precheckMode: precheckEnabled ? "enabled" : "disabled",
              })}
            />
            <RuleSwitch
              checked={draft.memoryWritebackMode === "enabled" || draft.memoryWritebackEnabled}
              description={t("memory.rules.organizeDescription")}
              label={t("memory.rules.organize")}
              onCheckedChange={(memoryWritebackEnabled) => onChange({
                memoryWritebackEnabled,
                memoryWritebackMode: memoryWritebackEnabled ? "enabled" : "disabled",
              })}
            />
            <RuleSwitch
              checked={draft.aiToolsEnabled}
              description={t("memory.rules.aiToolsDescription")}
              label={t("memory.rules.aiTools")}
              onCheckedChange={(aiToolsEnabled) => onChange({ aiToolsEnabled })}
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Settings2 className="size-4" aria-hidden="true" />
              {t("memory.rules.profileRefresh")}
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <NumberSetting
              label={t("memory.rules.recallTurns")}
              value={draft.implicitMemoryTurns}
              onChange={(implicitMemoryTurns) => onChange({ implicitMemoryTurns })}
            />
            <NumberSetting
              label={t("memory.rules.profileRefresh")}
              value={draft.profileRefreshTurns}
              onChange={(profileRefreshTurns) => onChange({ profileRefreshTurns })}
            />
            <NumberSetting
              label={t("memory.rules.profileLimit")}
              value={draft.profileNodeLimit}
              onChange={(profileNodeLimit) => onChange({ profileNodeLimit })}
            />
          </CardContent>
        </Card>
      </div>

      <Dialog
        open={applyOpen}
        onOpenChange={(open) => {
          setApplyOpen(open)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("memory.rules.applyTitle")}</DialogTitle>
            <DialogDescription>{t("memory.rules.applyDescription")}</DialogDescription>
          </DialogHeader>
          <RadioGroup
            onValueChange={(value) => setTargetScope(value as "global" | "project")}
            value={targetScope}
          >
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3">
              <RadioGroupItem value="global" />
              <span className="text-sm font-medium">{t("memory.rules.allProjects")}</span>
            </label>
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3">
              <RadioGroupItem value="project" />
              <span className="text-sm font-medium">{t("memory.rules.oneProject")}</span>
            </label>
          </RadioGroup>
          {targetScope === "project" ? (
            <div className="space-y-2">
              <Label>{t("memory.rules.selectProject")}</Label>
              <Combobox
                ariaLabel={t("memory.rules.selectProject")}
                emptyText={t("memory.rules.noProject")}
                onValueChange={setTargetProjectId}
                options={projectOptions}
                placeholder={t("memory.rules.selectProject")}
                searchPlaceholder={t("memory.rules.searchProject")}
                value={targetProjectId}
              />
            </div>
          ) : null}
          <DialogFooter>
            <Button onClick={() => setApplyOpen(false)} type="button" variant="outline">
              {t("memory.common.cancel")}
            </Button>
            <Button
              disabled={busy || (targetScope === "project" && !targetProjectId)}
              onClick={() => void submitApplication()}
              type="button"
            >
              {busy ? t("memory.common.loading") : t("memory.common.apply")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

// RuleSwitch renders one primary policy with its explanatory text.
// RuleSwitch 渲染单个主要策略及其说明文字。
function RuleSwitch({
  checked,
  description,
  label,
  onCheckedChange,
}: {
  checked: boolean
  description: string
  label: string
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-lg px-2 py-3 hover:bg-muted/40">
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
      <span className="min-w-0">
        <span className="block text-sm font-medium">{label}</span>
        <span className="block text-xs text-muted-foreground">{description}</span>
      </span>
    </label>
  )
}

// NumberSetting renders one direct numeric text input without browser steppers.
// NumberSetting 渲染一个不带浏览器步进器的直接数值文本输入框。
function NumberSetting({
  label,
  onChange,
  value,
}: {
  label: string
  onChange: (value: string) => void
  value: string
}) {
  const inputId = React.useId()
  return (
    <div className="space-y-2">
      <Label htmlFor={inputId}>{label}</Label>
      <Input
        id={inputId}
        inputMode="numeric"
        onChange={(event) => onChange(event.target.value.replace(/\D+/g, ""))}
        value={value}
      />
    </div>
  )
}

export { VmmRulesPage }
export type { VmmRulesPageProps }
