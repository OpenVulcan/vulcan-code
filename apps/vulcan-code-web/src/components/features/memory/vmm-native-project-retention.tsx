import * as React from "react"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProjectRetentionRules, VmmNativeProjectRetentionSummary, VmmNativeProjectSummary } from "@/types"
import { VmmNativeDurationInput } from "./vmm-native-duration-input"

// Numeric project fields have the same closed bounds as the native data-retention contract.
// 项目数值字段与原生数据保留契约具有相同封闭范围。
const fields: Array<{ key: Exclude<keyof VmmNativeProjectRetentionRules, "skipProtectedSharedMemories">; label: TranslationKey; min: number; max: number }> = [
  { key: "turnHotWindowSize", label: "memory.cleanup.hotWindow", min: 1, max: 4294967295 },
  { key: "sessionIdleMs", label: "memory.cleanup.idle", min: 1, max: 9223372036854 },
  { key: "trashRetentionMs", label: "memory.cleanup.trash", min: 1, max: 9223372036854 },
  { key: "protectedPriorityFloor", label: "memory.cleanup.priority", min: 0, max: 100 },
  { key: "protectedMemoryLevelFloor", label: "memory.cleanup.level", min: 1, max: 4 },
]
// Draft preserves intermediate input text instead of inventing missing numeric values.
// 草稿保留中间输入文本，不编造缺失数值。
type Draft = Record<typeof fields[number]["key"], string> & { skipProtectedSharedMemories: boolean }

// Converts one complete authoritative rule group to a lossless editable draft.
// 将完整权威规则组转换为无损可编辑草稿。
function draftOf(rules: VmmNativeProjectRetentionRules): Draft {
  return { ...Object.fromEntries(fields.map(({ key }) => [key, String(rules[key])])) as Record<typeof fields[number]["key"], string>, skipProtectedSharedMemories: rules.skipProtectedSharedMemories }
}

// Returns the complete six-field group or null for any invalid numeric input, without clamping.
// 返回完整六字段组；任一数值输入无效则返回 null，不进行截断。
export function parseProjectRetention(draft: Draft): VmmNativeProjectRetentionRules | null {
  const values: Array<[typeof fields[number]["key"], number]> = []
  for (const field of fields) {
    const text = draft[field.key]
    if (!/^\d+$/.test(text)) return null
    const value = Number(text)
    if (!Number.isSafeInteger(value) || value < field.min || value > field.max) return null
    values.push([field.key, value])
  }
  return { ...Object.fromEntries(values) as Record<typeof fields[number]["key"], number>, skipProtectedSharedMemories: draft.skipProtectedSharedMemories }
}

// Props accept the already-loaded native catalog and the page's serialized action transport.
// 属性接收已加载原生目录及页面串行化动作传输。
interface Props {
  // An explicit parent selection removes the duplicate picker without changing saved ownership.
  // 父级显式选择用于移除重复选择器，不改变已保存归属。
  selectedProjectId?: number
  projects: VmmNativeProjectSummary[]
  busy: boolean
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
}

// Displays named project choices; the keyed editor prevents late responses from crossing project identity.
// 显示项目名称选项；按身份挂载编辑器，防止迟到响应跨越项目身份。
export function VmmNativeProjectRetention({ projects, busy, onRunAction, selectedProjectId }: Props) {
  const { t } = useI18n()
  const [selectedId, setSelectedId] = React.useState("")
  // effectiveId belongs either to the explicit parent or this component's standalone picker.
  // effectiveId 属于显式父级或本组件独立选择器。
  const effectiveId = selectedProjectId === undefined ? selectedId : String(selectedProjectId)
  const selected = projects.find((project) => String(project.projectId) === effectiveId)
  return <Card>
    <CardHeader><CardTitle>{t("memory.projectRetention.title")}</CardTitle><CardDescription>{t("memory.projectRetention.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {selectedProjectId === undefined ? <>
      <Label htmlFor="retention-project">{t("memory.projectRetention.select")}</Label>
      <Select disabled={busy} value={selected ? selectedId : ""} onValueChange={setSelectedId}>
        <SelectTrigger id="retention-project" className="w-full"><SelectValue placeholder={t("memory.projectRetention.select")} /></SelectTrigger>
        <SelectContent>{projects.map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name} · {project.tag}</SelectItem>)}</SelectContent>
      </Select></> : null}
      {selected ? <ProjectRetentionEditor key={selected.projectId} projectId={selected.projectId} busy={busy} onRunAction={onRunAction} /> : null}
    </CardContent>
  </Card>
}

// Reads and explicitly saves one exact owner; failures retain drafts and never claim persistence success.
// 读取并显式保存一个准确归属方；失败保留草稿，绝不宣称持久化成功。
function ProjectRetentionEditor({ projectId, busy, onRunAction }: Omit<Props, "projects"> & { projectId: number }) {
  const { t } = useI18n()
  const [policy, setPolicy] = React.useState<VmmNativeProjectRetentionSummary | null>(null)
  const [draft, setDraft] = React.useState<Draft | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [saved, setSaved] = React.useState(false)
  const [pending, setPending] = React.useState(false)
  const mounted = React.useRef(false)
  const issued = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  // Executes a declared read/override/inherit operation and verifies the returned project identity.
  // 执行声明的读取/覆盖/继承操作，并验证返回项目身份。
  const execute = React.useCallback(async (mode: "read" | "override" | "inherit", rules?: VmmNativeProjectRetentionRules) => {
    setPending(true); setSaved(false)
    try {
      const response = await onRunAction(mode === "read" ? "native_project_retention" : "native_project_retention_update",
        mode === "read" ? { projectId } : mode === "inherit" ? { mode, projectId } : { mode, projectId, rules })
      if (!mounted.current) return
      if (response?.data.kind !== "native_project_retention" || response.data.value.projectId !== projectId) { setError(t("memory.policy.invalidResponse")); return }
      setPolicy(response.data.value); setDraft(draftOf(response.data.value.effective)); setError(null); setSaved(mode !== "read")
    } catch (reason) { if (mounted.current) setError(String(reason)) }
    finally { if (mounted.current) setPending(false) }
  }, [onRunAction, projectId, t])
  React.useEffect(() => { if (!busy && !issued.current) { issued.current = true; void execute("read") } }, [busy, execute])
  const disabled = busy || pending
  const parsed = draft ? parseProjectRetention(draft) : null
  const changed = parsed && policy && (Object.keys(parsed) as (keyof VmmNativeProjectRetentionRules)[]).some((key) => parsed[key] !== policy.effective[key])
  // Editing changes only this draft; nothing is persisted until a named save operation succeeds.
  // 编辑仅改变当前草稿；具名保存操作成功之前不持久化任何内容。
  const edit = (next: Draft) => { setDraft(next); setSaved(false) }
  return <div className="space-y-4">
    {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
    <Button variant="outline" disabled={disabled} onClick={() => void execute("read")}>{t("memory.projectRetention.reload")}</Button>
    {policy && draft ? <>
      <p className="text-sm text-muted-foreground">{t(policy.overrides === null ? "memory.projectRetention.inherited" : "memory.projectRetention.overridden")}</p>
      <div className="grid gap-4 sm:grid-cols-2">{fields.map((field) => <div key={field.key} className="space-y-2">
        <Label htmlFor={`project-retention-${field.key}`}>{t(field.label)}</Label>
        {field.key === "sessionIdleMs" || field.key === "trashRetentionMs"
          ? <VmmNativeDurationInput id={`project-retention-${field.key}`} value={draft[field.key]} disabled={disabled} onChange={(value) => edit({ ...draft, [field.key]: value })} />
          : field.key === "protectedMemoryLevelFloor"
            ? <Select value={draft[field.key]} disabled={disabled} onValueChange={(value) => edit({ ...draft, [field.key]: value })}>
              <SelectTrigger id={`project-retention-${field.key}`} className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{([[1,"memory.cleanup.session"],[2,"memory.cleanup.phase"],[3,"memory.cleanup.stable"],[4,"memory.cleanup.persistent"]] as const).map(([value,label]) => <SelectItem key={value} value={String(value)}>{t(label)}</SelectItem>)}</SelectContent>
            </Select>
            : <Input id={`project-retention-${field.key}`} type="number" min={field.min} max={field.max} step={1} disabled={disabled} value={draft[field.key]} onChange={(event) => edit({ ...draft, [field.key]: event.target.value })} />}
      </div>)}</div>
      <Label className="flex items-center justify-between">{t("memory.cleanup.protectShared")}<Switch disabled={disabled} checked={draft.skipProtectedSharedMemories} onCheckedChange={(value) => edit({ ...draft, skipProtectedSharedMemories: value })} /></Label>
      <p className="text-xs text-muted-foreground">{t("memory.cleanup.protectionHint")}</p>
      {!parsed ? <p role="alert" className="text-sm text-destructive">{t("memory.cleanup.invalid")}</p> : null}
      {saved ? <p role="status">{t("memory.projectRetention.saved")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button disabled={disabled || !parsed || (policy.overrides !== null && !changed)} onClick={() => { if (parsed) void execute("override", parsed) }}>{t("memory.projectRetention.save")}</Button>
        <Button variant="outline" disabled={disabled || policy.overrides === null} onClick={() => void execute("inherit")}>{t("memory.projectRetention.inherit")}</Button>
        <Button variant="outline" disabled={disabled} onClick={() => edit(draftOf(policy.global))}>{t("memory.projectRetention.fillGlobal")}</Button>
      </div>
    </> : null}
  </div>
}
