import * as React from "react"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import type { TranslationKey } from "@/i18n-messages"
import type { VmmMemoryActionResult, VmmNativeRetentionSettingsSummary, VmmNativeRuntimePolicySummary } from "@/types"

// NumericKey names only editable numeric cleanup fields from the exact host contract.
// NumericKey 仅命名准确宿主契约中的可编辑数值清洗字段。
type NumericKey = { [K in keyof VmmNativeRetentionSettingsSummary]: VmmNativeRetentionSettingsSummary[K] extends number ? K : never }[keyof VmmNativeRetentionSettingsSummary]
// DurationKey identifies persisted millisecond fields which the UI renders in explicit units.
// DurationKey 标识以毫秒持久化、由界面使用明确单位展示的字段。
type DurationKey = "sessionIdleMs" | "trashRetentionMs"
// RetentionDraft retains incomplete text and selected duration units without silently rounding.
// RetentionDraft 保留未完成文本及所选时长单位，不静默舍入。
export type RetentionDraft = Omit<VmmNativeRetentionSettingsSummary, NumericKey> & Record<NumericKey, string> & { units: Record<DurationKey, number> }

// units uses exact integer multipliers so all admitted milliseconds survive an edit round trip.
// units 使用准确整数倍率，使全部已准入毫秒值在编辑往返中保持不变。
const units = [
  [86400000, "memory.cleanup.days"], [3600000, "memory.cleanup.hours"],
  [60000, "memory.cleanup.minutes"], [1000, "memory.cleanup.seconds"], [1, "memory.cleanup.milliseconds"],
] as const
// numericFields declares exact host bounds; protected levels are rendered as a named selection below.
// numericFields 声明准确宿主边界；保护等级在下方使用名称选择。
const numericFields: { key: NumericKey; label: TranslationKey; min: number; max: number }[] = [
  { key: "scanIntervalMinutes", label: "memory.cleanup.scan", min: 1, max: 4294967295 },
  { key: "profileExpiryIntervalSeconds", label: "memory.cleanup.profileScan", min: 1, max: 4294967295 },
  { key: "turnHotWindowSize", label: "memory.cleanup.hotWindow", min: 1, max: 4294967295 },
  { key: "sessionIdleMs", label: "memory.cleanup.idle", min: 1, max: 9223372036854 },
  { key: "trashRetentionMs", label: "memory.cleanup.trash", min: 1, max: 9223372036854 },
  { key: "protectedPriorityFloor", label: "memory.cleanup.priority", min: 0, max: 100 },
  { key: "protectedMemoryLevelFloor", label: "memory.cleanup.level", min: 1, max: 4 },
]

// retentionDraft chooses the largest exact unit; unusual saved milliseconds are never rounded away.
// retentionDraft 选择最大的整除单位；特殊的已保存毫秒值绝不被舍去。
export function retentionDraft(settings: VmmNativeRetentionSettingsSummary): RetentionDraft {
  const numeric = Object.fromEntries(numericFields.map(({ key }) => [key, String(settings[key])])) as Record<NumericKey, string>
  const selectedUnits = { sessionIdleMs: 1, trashRetentionMs: 1 }
  for (const key of ["sessionIdleMs", "trashRetentionMs"] as const) {
    // The millisecond option always divides valid integer durations, making selection total.
    // 毫秒选项始终整除有效整数时长，因此选择必定存在。
    const unit = units.find(([factor]) => settings[key] % factor === 0)!
    selectedUnits[key] = unit[0]
    numeric[key] = String(settings[key] / unit[0])
  }
  return { enabled: settings.enabled, skipProtectedSharedMemories: settings.skipProtectedSharedMemories, ...numeric, units: selectedUnits }
}

// parseRetentionDraft validates complete integer inputs and exact unit conversion before submission.
// parseRetentionDraft 在提交前验证完整整数输入及准确单位换算。
export function parseRetentionDraft(draft: RetentionDraft): VmmNativeRetentionSettingsSummary | null {
  const entries: [NumericKey, number][] = []
  for (const field of numericFields) {
    const text = draft[field.key].trim()
    if (!/^\d+$/.test(text)) return null
    const value = Number(text)
    if (!Number.isSafeInteger(value)) return null
    let multiplier = 1
    if (field.key === "sessionIdleMs" || field.key === "trashRetentionMs") {
      multiplier = draft.units[field.key]
      if (!units.some(([factor]) => factor === multiplier)) return null
    }
    const converted = value * multiplier
    if (!Number.isSafeInteger(converted) || converted < field.min || converted > field.max) return null
    entries.push([field.key, converted])
  }
  return { enabled: draft.enabled, skipProtectedSharedMemories: draft.skipProtectedSharedMemories,
    ...Object.fromEntries(entries) as Record<NumericKey, number> }
}

// RetentionProps shares the already loaded document and the exclusive native maintenance channel.
// RetentionProps 共用已加载文档及独占原生维护通道。
interface RetentionProps {
  // Effective settings and authoritative reset values.
  // 有效设置及权威重置值。
  settings: VmmNativeRetentionSettingsSummary
  defaults: VmmNativeRetentionSettingsSummary
  // Shared transport occupancy and action executor.
  // 共享传输占用状态及动作执行器。
  busy: boolean
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  // Publishes the accepted full document without discarding sibling drafts.
  // 发布已接纳的完整文档，不丢弃相邻配置草稿。
  onSaved: (policy: VmmNativeRuntimePolicySummary) => void
}

// VmmNativeRetentionSettings exposes business cleanup controls and writes only after an explicit save.
// VmmNativeRetentionSettings 展示业务清洗控制项，仅在显式保存后写入。
export function VmmNativeRetentionSettings({ settings, defaults, busy, onRunAction, onSaved }: RetentionProps) {
  const { t } = useI18n()
  const [draft, setDraft] = React.useState(() => retentionDraft(settings))
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [saved, setSaved] = React.useState(false)
  // accepted distinguishes a real settings change from an unrelated group save.
  // accepted 区分真实设置变化和无关配置组保存。
  const accepted = React.useRef(settings)
  const mounted = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  React.useEffect(() => {
    if ((Object.keys(settings) as (keyof VmmNativeRetentionSettingsSummary)[]).some((key) => settings[key] !== accepted.current[key])) {
      accepted.current = settings
      setDraft(retentionDraft(settings))
      setSaved(false)
    }
  }, [settings])
  const parsed = parseRetentionDraft(draft)
  const changed = parsed && (Object.keys(settings) as (keyof VmmNativeRetentionSettingsSummary)[]).some((key) => parsed[key] !== settings[key])
  const disabled = busy || pending
  // edit retains all other fields and clears stale success feedback.
  // edit 保留其他全部字段并清除过时的成功反馈。
  const edit = (next: RetentionDraft) => { setDraft(next); setSaved(false) }
  // save sends only cleanup settings and accepts only the native policy response kind.
  // save 仅发送清洗设置，并只接纳原生策略响应类型。
  const save = async () => {
    if (!parsed) return
    setPending(true)
    setSaved(false)
    try {
      const response = await onRunAction("native_retention_settings_update", { ...parsed })
      if (!mounted.current) return
      if (response?.data.kind !== "native_runtime_policy") { setError(t("memory.policy.invalidResponse")); return }
      accepted.current = response.data.value.retention
      setDraft(retentionDraft(response.data.value.retention))
      onSaved(response.data.value)
      setError(null)
      setSaved(true)
    } catch (reason) {
      if (mounted.current) setError(String(reason))
    } finally {
      if (mounted.current) setPending(false)
    }
  }
  return <Card className="w-full min-w-0">
    <CardHeader><CardTitle>{t("memory.cleanup.title")}</CardTitle><CardDescription>{t("memory.cleanup.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {saved ? <p role="status">{t("memory.cleanup.saved")}</p> : null}
      <Label className="flex items-center justify-between rounded-lg border p-3">{t("memory.cleanup.enabled")}
        <Switch checked={draft.enabled} disabled={disabled} onCheckedChange={(enabled) => edit({ ...draft, enabled })} /></Label>
      <div className="grid gap-4 sm:grid-cols-2">
        {numericFields.filter(({ key }) => key !== "protectedMemoryLevelFloor" && key !== "protectedPriorityFloor").map((field) => {
          const durationKey = field.key === "sessionIdleMs" || field.key === "trashRetentionMs" ? field.key : null
          return <div className="space-y-2" key={field.key}>
            <Label htmlFor={`vmm-cleanup-${field.key}`}>{t(field.label)}</Label>
            <div className="flex gap-2"><Input id={`vmm-cleanup-${field.key}`} type="number" min={1} step={1}
              value={draft[field.key]} disabled={disabled} onChange={(event) => edit({ ...draft, [field.key]: event.target.value })} />
              {durationKey ? <Select disabled={disabled}
                value={String(draft.units[durationKey])} onValueChange={(value) => edit({ ...draft, units: { ...draft.units, [durationKey]: Number(value) } })}>
                <SelectTrigger aria-label={`${t(field.label)} · ${t("memory.cleanup.unit")}`}><SelectValue /></SelectTrigger>
                <SelectContent>{units.map(([factor, label]) => <SelectItem key={factor} value={String(factor)}>{t(label)}</SelectItem>)}</SelectContent>
              </Select> : null}</div>
          </div>
        })}
      </div>
      <details><summary className="cursor-pointer text-sm font-medium">{t("memory.cleanup.protection")}</summary>
        <p className="my-3 text-xs text-muted-foreground">{t("memory.cleanup.protectionHint")}</p>
        <Label className="flex items-center justify-between gap-3">{t("memory.cleanup.protectShared")}
          <Switch disabled={disabled} checked={draft.skipProtectedSharedMemories} onCheckedChange={(skipProtectedSharedMemories) => edit({ ...draft, skipProtectedSharedMemories })} /></Label>
        <div className="mt-3 grid gap-4 sm:grid-cols-2">
          <div className="space-y-2"><Label htmlFor="vmm-cleanup-protectedPriorityFloor">{t("memory.cleanup.priority")}</Label>
            <Input id="vmm-cleanup-protectedPriorityFloor" type="number" min={0} max={100} step={1} disabled={disabled}
              value={draft.protectedPriorityFloor} onChange={(event) => edit({ ...draft, protectedPriorityFloor: event.target.value })} /></div>
          <div className="space-y-2"><Label htmlFor="vmm-cleanup-protectedMemoryLevelFloor">{t("memory.cleanup.level")}</Label>
            <Select disabled={disabled}
              value={draft.protectedMemoryLevelFloor} onValueChange={(value) => edit({ ...draft, protectedMemoryLevelFloor: value })}>
              <SelectTrigger id="vmm-cleanup-protectedMemoryLevelFloor" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{([ [1, "memory.cleanup.session"], [2, "memory.cleanup.phase"], [3, "memory.cleanup.stable"], [4, "memory.cleanup.persistent"] ] as const).map(([value, label]) => <SelectItem key={value} value={String(value)}>{t(label)}</SelectItem>)}</SelectContent>
            </Select></div>
        </div>
      </details>
      {!parsed ? <p role="alert" className="text-sm text-destructive">{t("memory.cleanup.invalid")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button disabled={disabled || !parsed || !changed} onClick={() => void save()}>{t("memory.cleanup.save")}</Button>
        <Button variant="outline" disabled={disabled} onClick={() => edit(retentionDraft(defaults))}>{t("memory.policy.resetDraft")}</Button>
      </div>
    </CardContent>
  </Card>
}
