import * as React from "react"
import { BrainCircuit, Save, Settings2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import { isOptionalUnsignedDraft } from "./vmm-draft-number"
import type { VmmConfigDraftSummary, VmmProjectScopeSummary } from "@/types"

// VmmRulesPageProps defines one editable rules draft and explicit application targets.
// VmmRulesPageProps 定义一份可编辑规则草稿与明确应用目标。
interface VmmRulesPageProps {
  busy: boolean
  // Scope and project identify the loaded draft before any editing.
  // 范围与项目在任何编辑之前标识已加载草稿。
  scope: "global" | "project"
  projectId: string
  draftReady: boolean
  // Selects and loads the exact requested scope; does not copy another draft.
  // 选择并加载准确请求范围，不复制其他草稿。
  onSelectScope: (scope: "global" | "project", projectId: string | null) => void
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

// VmmRulesPage edits the loaded scope and saves that exact draft to its original target.
// VmmRulesPage 编辑已加载范围，将准确草稿保存到原始目标。
function VmmRulesPage({
  busy, scope, projectId, draftReady, onSelectScope,
  draft,
  projects,
  notice,
  onChange,
  onApply,
}: VmmRulesPageProps) {
  const { t } = useI18n()
  // Numeric drafts match the backend's optional u32 contract and stay unchanged on invalid input.
  // 数字草稿对应后端可选 u32 契约，无效输入保留原文。
  const numbersValid = [draft.implicitMemoryTurns, draft.profileRefreshTurns, draft.profileNodeLimit].every((value) => isOptionalUnsignedDraft(value, 32))

  return (
    <div className="flex w-full min-w-0 flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.rules.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t("memory.rules.description")}
          </p>
        </div>
        <Button disabled={busy || !draftReady || !numbersValid} onClick={() => void onApply(scope, scope === "project" ? projectId : null, draft)} type="button">
          <Save className="size-4" aria-hidden="true" />
          {t("memory.rules.applySettings")}
        </Button>
      </div>

      <div className="space-y-2">
        <Label>{t("memory.workspace.editScope")}</Label>
        <Select disabled={busy} value={scope === "global" ? "global" : `project:${projectId}`} onValueChange={(value) => onSelectScope(value === "global" ? "global" : "project", value === "global" ? null : value.slice("project:".length))}>
          <SelectTrigger className="w-full sm:w-80" aria-label={t("memory.workspace.editScope")}><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="global">{t("memory.rules.allProjects")}</SelectItem>{projects.map((project) => <SelectItem key={project.projectId} value={`project:${project.projectId}`}>{project.name}</SelectItem>)}</SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">{t("memory.workspace.scopeHint")}</p>
      </div>
      {notice ? (
        <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
          {notice}
        </p>
      ) : null}
      {!numbersValid ? <p role="alert" className="text-sm text-destructive">{t("memory.rules.invalidNumbers")}</p> : null}

      <fieldset disabled={busy || !draftReady} className="grid min-w-0 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BrainCircuit className="size-4" aria-hidden="true" />
              {t("memory.rules.features")}
            </CardTitle>
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
              {t("memory.rules.retentionProfiles")}
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
      </fieldset>


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
        aria-invalid={!isOptionalUnsignedDraft(value, 32)}
        onChange={(event) => onChange(event.target.value)}
        value={value}
      />
    </div>
  )
}

export { VmmRulesPage }
export type { VmmRulesPageProps }
