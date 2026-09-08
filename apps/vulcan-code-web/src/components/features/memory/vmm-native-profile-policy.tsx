import * as React from "react"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useI18n } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProfilePolicySummary, VmmNativeRuntimePolicySummary } from "@/types"

// Exact contract keys determine both the editor and validation traversal.
// 准确契约键同时决定编辑器与校验遍历。
const roles = ["user", "project"] as const
const levels = ["transient", "situational", "stable"] as const
const fields = ["baseMs", "stepMs", "maxMs"] as const
// Integer units preserve all admitted millisecond values without rounding.
// 整数单位保留全部已准入毫秒值，不进行舍入。
const units = [[86400000, "memory.cleanup.days"], [3600000, "memory.cleanup.hours"],
  [60000, "memory.cleanup.minutes"], [1000, "memory.cleanup.seconds"], [1, "memory.cleanup.milliseconds"]] as const
// A row shares one explicit unit while retaining incomplete numeric input.
// 一行共用一个明确单位，并保留尚未完成的数值输入。
type LifetimeDraft = Record<typeof fields[number], string> & { unit: number }
// Separate role branches prevent project defaults from replacing user lifetimes.
// 独立角色分支防止项目默认值替换用户生命周期。
export type ProfileDraft = Record<typeof roles[number], Record<typeof levels[number], LifetimeDraft>>

// profileDraft returns editable rows in the largest unit dividing all three values exactly.
// profileDraft 返回可编辑行，使用能准确整除三个数值的最大单位。
export function profileDraft(settings: VmmNativeProfilePolicySummary): ProfileDraft {
  return Object.fromEntries(roles.map((role) => [role, Object.fromEntries(levels.map((level) => {
    const rule = settings[role][level]
    // Milliseconds always divide valid integer input, so the exact unit exists.
    // 毫秒始终整除有效整数输入，因此准确单位必定存在。
    const unit = units.find(([factor]) => fields.every((field) => rule[field] % factor === 0))![0]
    return [level, { unit, ...Object.fromEntries(fields.map((field) => [field, String(rule[field] / unit)])) }]
  }))])) as ProfileDraft
}

// parseProfileDraft returns exact bounded milliseconds, or null without clamping invalid edits.
// parseProfileDraft 返回准确有界毫秒值，或对非法编辑返回 null，不截断输入。
export function parseProfileDraft(draft: ProfileDraft): VmmNativeProfilePolicySummary | null {
  const result = { user: {}, project: {} } as VmmNativeProfilePolicySummary
  for (const role of roles) for (const level of levels) {
    const row = draft[role][level]
    if (!units.some(([factor]) => factor === row.unit)) return null
    const rule = { baseMs: 0, stepMs: 0, maxMs: 0 }
    for (const field of fields) {
      if (!/^\d+$/.test(row[field].trim())) return null
      const value = Number(row[field]) * row.unit
      if (!Number.isSafeInteger(value) || value < (field === "stepMs" ? 0 : 1) || value > 3650 * 86400000) return null
      rule[field] = value
    }
    if (rule.baseMs > rule.maxMs) return null
    result[role][level] = rule
  }
  return result
}

// sameProfiles compares typed values, independent of JSON property ordering.
// sameProfiles 比较强类型值，不依赖 JSON 属性顺序。
function sameProfiles(a: VmmNativeProfilePolicySummary, b: VmmNativeProfilePolicySummary): boolean {
  return roles.every((role) => levels.every((level) => fields.every((field) => a[role][level][field] === b[role][level][field])))
}

// ProfilePolicyProps shares the parent's loaded policy and explicit native maintenance channel.
// ProfilePolicyProps 共用父级已加载策略及显式原生维护通道。
interface ProfilePolicyProps {
  // Effective values and Core-owned defaults; neither is written during rendering.
  // 有效值及 Core 默认值；渲染过程中两者都不会被写入。
  settings: VmmNativeProfilePolicySummary
  defaults: VmmNativeProfilePolicySummary
  // The parent's shared maintenance occupancy.
  // 父级共享维护占用状态。
  busy: boolean
  // Execute an explicit save and publish only acknowledged values.
  // 执行显式保存，仅发布经过确认的值。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  onSaved: (policy: VmmNativeRuntimePolicySummary) => void
}

// VmmNativeProfilePolicy edits both role-specific groups without touching existing profile nodes.
// VmmNativeProfilePolicy 编辑两种角色专属配置组，不修改已有画像节点。
export function VmmNativeProfilePolicy({ settings, defaults, busy, onRunAction, onSaved }: ProfilePolicyProps) {
  const { t } = useI18n()
  const [draft, setDraft] = React.useState(() => profileDraft(settings))
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [saved, setSaved] = React.useState(false)
  // Preserve local drafts across sibling saves and ignore replies after navigation.
  // 跨相邻配置组保存保留本地草稿，忽略导航后的回复。
  const accepted = React.useRef(settings)
  const mounted = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  React.useEffect(() => {
    if (!sameProfiles(settings, accepted.current)) {
      accepted.current = settings
      setDraft(profileDraft(settings))
      setSaved(false)
    }
  }, [settings])
  const parsed = parseProfileDraft(draft)
  const disabled = busy || pending
  // edit changes only the selected row and clears the previous success indicator.
  // edit 仅修改选中行，并清除先前成功指示。
  const edit = (role: typeof roles[number], level: typeof levels[number], next: LifetimeDraft) => {
    setDraft((current) => ({ ...current, [role]: { ...current[role], [level]: next } }))
    setSaved(false)
  }
  // save sends exact role keys and retains a failed draft for correction.
  // save 发送准确角色键，并保留失败草稿供修正。
  const save = async () => {
    if (!parsed || disabled) return
    setPending(true)
    setSaved(false)
    try {
      const result = await onRunAction("native_profile_policy_update", { ...parsed })
      if (!mounted.current) return
      if (result?.data.kind !== "native_runtime_policy") { setError(t("memory.policy.invalidResponse")); return }
      accepted.current = result.data.value.profiles
      setDraft(profileDraft(accepted.current))
      onSaved(result.data.value)
      setError(null)
      setSaved(true)
    } catch (reason) {
      if (mounted.current) setError(String(reason))
    } finally {
      if (mounted.current) setPending(false)
    }
  }
  return <Card className="w-full min-w-0">
    <CardHeader><CardTitle>{t("memory.profilePolicy.title")}</CardTitle><CardDescription>{t("memory.profilePolicy.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {saved ? <p role="status">{t("memory.profilePolicy.saved")}</p> : null}
      {roles.map((role) => <fieldset key={role} disabled={disabled} className="space-y-3 rounded-lg border p-3">
        <legend className="px-1 font-medium">{t(`memory.profilePolicy.${role}`)}</legend>
        {levels.map((level) => <fieldset key={level} className="grid gap-2 rounded-md border p-3 sm:grid-cols-3">
          <legend className="px-1 text-sm">{t(`memory.profilePolicy.${level}`)}</legend>
          {fields.map((field) => <div key={field} className="space-y-1">
            <Label htmlFor={`profile-${role}-${level}-${field}`}>{t(`memory.profilePolicy.${field}`)}</Label>
            <Input id={`profile-${role}-${level}-${field}`} type="number" min={field === "stepMs" ? 0 : 1} step={1}
              value={draft[role][level][field]} onChange={(event) => edit(role, level, { ...draft[role][level], [field]: event.target.value })} />
          </div>)}
          <div className="space-y-1 sm:col-span-3"><Label htmlFor={`profile-${role}-${level}-unit`}>{t("memory.profilePolicy.unit")}</Label>
            <Select disabled={disabled} value={String(draft[role][level].unit)}
              onValueChange={(value) => edit(role, level, { ...draft[role][level], unit: Number(value) })}>
              <SelectTrigger id={`profile-${role}-${level}-unit`} className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{units.map(([factor, key]) => <SelectItem key={factor} value={String(factor)}>{t(key)}</SelectItem>)}</SelectContent>
            </Select></div>
        </fieldset>)}
      </fieldset>)}
      <p className="text-xs text-muted-foreground">{t("memory.profilePolicy.hint")}</p>
      {!parsed ? <p role="alert" className="text-sm text-destructive">{t("memory.policy.invalidValues")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button disabled={disabled || !parsed || sameProfiles(parsed, settings)} onClick={() => void save()}>{t("memory.profilePolicy.save")}</Button>
        <Button variant="outline" disabled={disabled} onClick={() => { setDraft(profileDraft(defaults)); setSaved(false) }}>{t("memory.policy.resetDraft")}</Button>
      </div>
    </CardContent>
  </Card>
}
