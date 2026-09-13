import * as React from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProfileLifetimes, VmmNativeProjectProfilePolicySummary, VmmNativeProjectSummary } from "@/types"
import { VmmNativeDurationInput } from "./vmm-native-duration-input"

// Finite levels and complete formula keys match the closed native policy contract.
// 有限等级及完整公式键与封闭原生策略契约一致。
const levels = ["transient", "situational", "stable"] as const
const fields = ["baseMs", "stepMs", "maxMs"] as const
// Draft values remain exact millisecond strings, including invalid intermediate edits.
// 草稿值保持准确的毫秒字符串，包含无效中间编辑。
type Draft = Record<typeof levels[number], Record<typeof fields[number], string>>

// Converts one complete project group into editable values without personal fields.
// 将一个完整项目组转换为可编辑值，不包含个人字段。
function draftOf(rules: VmmNativeProfileLifetimes): Draft {
  return Object.fromEntries(levels.map((level) => [level, Object.fromEntries(fields.map((field) => [field, String(rules[level][field])]))])) as Draft
}

// Validates every exact duration and the authoritative base/cap relation; returns no partial group.
// 校验每个准确时长及权威基础值/上限关系，不返回部分配置组。
export function parseProjectProfileDraft(draft: Draft): VmmNativeProfileLifetimes | null {
  const rules = {} as VmmNativeProfileLifetimes
  for (const level of levels) {
    const row = { baseMs: 0, stepMs: 0, maxMs: 0 }
    for (const field of fields) {
      const text = draft[level][field]
      if (!/^\d+$/.test(text)) return null
      const value = Number(text)
      if (!Number.isSafeInteger(value) || value > 315360000000 || (field !== "stepMs" && value === 0)) return null
      row[field] = value
    }
    if (row.baseMs > row.maxMs) return null
    rules[level] = row
  }
  return rules
}

// Props carry the authoritative memory catalog and explicit read/save transport.
// 属性携带权威记忆目录及显式读取/保存传输。
interface Props {
  // An explicit parent selection removes the duplicate picker without changing saved ownership.
  // 父级显式选择用于移除重复选择器，不改变已保存归属。
  selectedProjectId?: number
  // Only stable catalog identities are submitted; names remain presentation.
  // 仅提交稳定目录身份；名称仅用于展示。
  projects: VmmNativeProjectSummary[]
  // Parent operations prevent conflicting edits.
  // 父级操作期间阻止冲突编辑。
  busy: boolean
  // Returns the typed service reply, or null when the operation failed.
  // 返回强类型服务响应，操作失败时返回 null。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
}

// Selects an existing memory project by name while preserving stable ID ownership across renames.
// 按名称选择已有记忆项目，同时在改名后保持稳定 ID 归属。
export function VmmNativeProjectProfilePolicy({ projects, busy, onRunAction, selectedProjectId }: Props) {
  const { t } = useI18n()
  const [selectedId, setSelectedId] = React.useState("")
  // effectiveId belongs either to the explicit parent or this component's standalone picker.
  // effectiveId 属于显式父级或本组件独立选择器。
  const effectiveId = selectedProjectId === undefined ? selectedId : String(selectedProjectId)
  const selected = projects.find((project) => String(project.projectId) === effectiveId)
  return <Card>
    <CardHeader><CardTitle>{t("memory.projectProfile.title")}</CardTitle><CardDescription>{t("memory.projectProfile.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {selectedProjectId === undefined ? <>
      <Label htmlFor="profile-policy-project">{t("memory.projectRetention.select")}</Label>
      <Select disabled={busy} value={selected ? selectedId : ""} onValueChange={setSelectedId}>
        <SelectTrigger id="profile-policy-project" className="w-full"><SelectValue placeholder={t("memory.projectRetention.select")} /></SelectTrigger>
        <SelectContent>{projects.map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name} · {project.tag}</SelectItem>)}</SelectContent>
      </Select></> : null}
      {selected ? <ProjectProfileEditor key={selected.projectId} projectId={selected.projectId} busy={busy} onRunAction={onRunAction} /> : null}
    </CardContent>
  </Card>
}

// Owns one selected project's draft; changing the identity unmounts and discards late replies.
// 持有一个已选项目的草稿；身份变化会卸载编辑器并丢弃迟到响应。
function ProjectProfileEditor({ projectId, busy, onRunAction }: Omit<Props, "projects"> & { projectId: number }) {
  const { t } = useI18n()
  const [policy, setPolicy] = React.useState<VmmNativeProjectProfilePolicySummary | null>(null)
  const [draft, setDraft] = React.useState<Draft | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [pending, setPending] = React.useState(false)
  const [saved, setSaved] = React.useState(false)
  const mounted = React.useRef(false)
  const issued = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  // Only explicit override/inherit operations persist; every response must match the selected ID.
  // 仅显式覆盖/继承操作会持久化；每个响应都必须匹配已选 ID。
  const execute = React.useCallback(async (mode: "read" | "override" | "inherit", rules?: VmmNativeProfileLifetimes) => {
    setPending(true); setSaved(false)
    try {
      const response = await onRunAction(mode === "read" ? "native_project_profile_policy" : "native_project_profile_policy_update",
        mode === "read" ? { projectId } : mode === "inherit" ? { mode, projectId } : { mode, projectId, rules })
      if (!mounted.current) return
      if (response?.data.kind !== "native_project_profile_policy" || response.data.value.projectId !== projectId) { setError(t("memory.policy.invalidResponse")); return }
      setPolicy(response.data.value); setDraft(draftOf(response.data.value.effective)); setError(null); setSaved(mode !== "read")
    } catch (reason) { if (mounted.current) setError(String(reason)) }
    finally { if (mounted.current) setPending(false) }
  }, [onRunAction, projectId, t])
  React.useEffect(() => { if (!busy && !issued.current) { issued.current = true; void execute("read") } }, [busy, execute])
  const disabled = busy || pending
  const parsed = draft ? parseProjectProfileDraft(draft) : null
  const changed = parsed && policy && levels.some((level) => fields.some((field) => parsed[level][field] !== policy.effective[level][field]))
  return <div className="space-y-4">
    {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
    <Button variant="outline" disabled={disabled} onClick={() => void execute("read")}>{t("memory.projectRetention.reload")}</Button>
    {policy && draft ? <>
      <p className="text-sm text-muted-foreground">{t(policy.overrides === null ? "memory.projectProfile.inherited" : "memory.projectRetention.overridden")}</p>
      {levels.map((level) => <fieldset key={level} disabled={disabled} className="grid gap-3 rounded-md border p-3 sm:grid-cols-3">
        <legend className="px-1 text-sm">{t(`memory.profilePolicy.${level}`)}</legend>
        {fields.map((field) => <div key={field} className="space-y-1">
          <Label htmlFor={`project-profile-${level}-${field}`}>{t(`memory.profilePolicy.${field}`)}</Label>
          <VmmNativeDurationInput id={`project-profile-${level}-${field}`} value={draft[level][field]} disabled={disabled}
            unitLabel={`${t(`memory.profilePolicy.${level}`)} · ${t(`memory.profilePolicy.${field}`)} · ${t("memory.policy.durationUnit")}`}
            onChange={(value) => { setDraft({ ...draft, [level]: { ...draft[level], [field]: value } }); setSaved(false) }} />
        </div>)}
      </fieldset>)}
      <p className="text-xs text-muted-foreground">{t("memory.projectProfile.durationHint")}</p>
      {!parsed ? <p role="alert" className="text-sm text-destructive">{t("memory.policy.invalidValues")}</p> : null}
      {saved ? <p role="status">{t("memory.projectProfile.saved")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button disabled={disabled || !parsed || (policy.overrides !== null && !changed)} onClick={() => { if (parsed) void execute("override", parsed) }}>{t("memory.projectRetention.save")}</Button>
        <Button variant="outline" disabled={disabled || policy.overrides === null} onClick={() => void execute("inherit")}>{t("memory.projectRetention.inherit")}</Button>
        <Button variant="outline" disabled={disabled} onClick={() => { setDraft(draftOf(policy.global)); setSaved(false) }}>{t("memory.projectRetention.fillGlobal")}</Button>
      </div>
    </> : null}
  </div>
}
