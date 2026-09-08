import * as React from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRuntimePolicySummary, VmmNativeTextPolicySummary } from "@/types"

// TextPolicyProps shares the parent-owned load and native action channel; rendering never performs an action.
// TextPolicyProps 共用父级加载与原生动作通道；渲染绝不执行动作。
interface TextPolicyProps {
  // Effective and reset values from the host's complete policy document.
  // 宿主完整策略文档提供的有效值与重置值。
  settings: VmmNativeTextPolicySummary
  defaults: VmmNativeTextPolicySummary
  // Occupancy of the shared action channel.
  // 共享动作通道的占用状态。
  busy: boolean
  // Executes an explicit edit and publishes only the acknowledged policy.
  // 执行显式编辑，仅发布已确认的策略。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  onSaved: (policy: VmmNativeRuntimePolicySummary) => void
}

// VmmNativeTextPolicy exposes reduction controls, never switches for mandatory privacy or reasoning removal.
// VmmNativeTextPolicy 暴露缩减控制项，不提供关闭强制隐私或思考内容移除的开关。
export function VmmNativeTextPolicy({ settings, defaults, busy, onRunAction, onSaved }: TextPolicyProps) {
  const { t } = useI18n()
  const [fold, setFold] = React.useState(settings.foldMachineText)
  const [budget, setBudget] = React.useState(String(settings.maxFragmentTokens))
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [saved, setSaved] = React.useState(false)
  // accepted prevents sibling saves from erasing unsaved text edits; mounted rejects late navigation replies.
  // accepted 防止相邻组保存抹掉未保存文本编辑；mounted 拒绝导航后的迟到回复。
  const accepted = React.useRef(settings)
  const mounted = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  React.useEffect(() => {
    if (settings.foldMachineText !== accepted.current.foldMachineText || settings.maxFragmentTokens !== accepted.current.maxFragmentTokens) {
      accepted.current = settings
      setFold(settings.foldMachineText)
      setBudget(String(settings.maxFragmentTokens))
      setSaved(false)
    }
  }, [settings])
  const tokens = Number(budget)
  const valid = budget.trim() !== "" && Number.isInteger(tokens) && tokens >= 256 && tokens <= 131072
  const changed = fold !== settings.foldMachineText || tokens !== settings.maxFragmentTokens
  const disabled = busy || pending
  // save sends only the two typed settings and retains failed drafts for correction.
  // save 仅发送两个强类型设置，保留失败草稿供修正。
  const save = async () => {
    if (!valid) return
    setPending(true)
    setSaved(false)
    try {
      const result = await onRunAction("native_text_policy_update", { foldMachineText: fold, maxFragmentTokens: tokens })
      if (!mounted.current) return
      if (result?.data.kind !== "native_runtime_policy") { setError(t("memory.policy.invalidResponse")); return }
      accepted.current = result.data.value.text
      setFold(accepted.current.foldMachineText)
      setBudget(String(accepted.current.maxFragmentTokens))
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
    <CardHeader><CardTitle>{t("memory.textPolicy.title")}</CardTitle><CardDescription>{t("memory.textPolicy.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {saved ? <p role="status">{t("memory.textPolicy.saved")}</p> : null}
      <Label className="flex items-center justify-between gap-3 rounded-lg border p-3">{t("memory.textPolicy.fold")}
        <Switch checked={fold} disabled={disabled} onCheckedChange={(next) => { setFold(next); setSaved(false) }} /></Label>
      <div className="space-y-2"><Label htmlFor="vmm-text-budget">{t("memory.textPolicy.budget")}</Label>
        <Input id="vmm-text-budget" type="number" min={256} max={131072} step={1} value={budget} disabled={disabled}
          onChange={(event) => { setBudget(event.target.value); setSaved(false) }} />
        <p className="text-xs text-muted-foreground">{t("memory.textPolicy.budgetHint")}</p>
      </div>
      {!valid ? <p role="alert" className="text-sm text-destructive">{t("memory.policy.invalidValues")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button disabled={disabled || !valid || !changed} onClick={() => void save()}>{t("memory.textPolicy.save")}</Button>
        <Button variant="outline" disabled={disabled} onClick={() => { setFold(defaults.foldMachineText); setBudget(String(defaults.maxFragmentTokens)); setSaved(false) }}>{t("memory.policy.resetDraft")}</Button>
      </div>
    </CardContent>
  </Card>
}
