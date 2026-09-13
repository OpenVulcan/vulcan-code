import * as React from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import type { TranslationKey } from "@/i18n-messages"
import type { VmmMemoryActionResult, VmmNativeRetrievalPolicySummary, VmmNativeRuntimePolicySummary } from "@/types"

// NumericKey selects only the declared numeric ranking fields; RRF stays lossless decimal text.
// NumericKey 仅选择声明为数值的排序字段；RRF 保持无损十进制文本。
type NumericKey = { [K in keyof VmmNativeRetrievalPolicySummary]: VmmNativeRetrievalPolicySummary[K] extends number ? K : never }[keyof VmmNativeRetrievalPolicySummary]
// RankingDraft preserves intermediate text while retaining exact boolean and decimal-string semantics.
// RankingDraft 保留编辑中间文本，同时保持准确的布尔值和十进制字符串语义。
type RankingDraft = Omit<VmmNativeRetrievalPolicySummary, NumericKey> & Record<NumericKey, string>

// RankingField declares a business range, not a selectable score-contract identity.
// RankingField 声明业务取值范围，不提供评分契约身份选择。
interface RankingField {
  // Exact numeric field and localized label.
  // 准确数值字段及本地化标签。
  key: NumericKey
  label: TranslationKey
  // Bounds match Core; an omitted maximum means any finite value is allowed.
  // 边界与 Core 一致；未设置上限表示允许任意有限值。
  min: number
  max?: number
  exclusiveMin?: boolean
  // Windows require integers; mathematical weights retain fractional precision.
  // 窗口要求整数；数学权重保留小数精度。
  integer: boolean
}

// fields lists only values verified to reach the retrieval execution chain.
// fields 仅列出已核实进入检索执行链的值。
const fields: RankingField[] = [
  { key: "lexicalCandidateFloor", label: "memory.ranking.lexical", min: 1, max: 32, integer: true },
  { key: "replaceMinSimilarity", label: "memory.ranking.replace", min: 0, max: 1, integer: false },
  { key: "hardDedupeCosineThreshold", label: "memory.ranking.dedupe", min: 0, max: 1, integer: false },
  { key: "hardDedupePoolTopK", label: "memory.ranking.dedupePool", min: 1, max: 32, integer: true },
  { key: "replacementTopK", label: "memory.ranking.replacePool", min: 1, max: 32, integer: true },
  { key: "rerankTopN", label: "memory.ranking.rerank", min: 0, max: 32, integer: true },
  { key: "mmrLambda", label: "memory.ranking.lambda", min: 0, max: 1, integer: false },
  { key: "weibullShape", label: "memory.ranking.shape", min: 0, exclusiveMin: true, integer: false },
  { key: "weibullScaleHours", label: "memory.ranking.scale", min: 0, exclusiveMin: true, integer: false },
  { key: "weibullMinMultiplier", label: "memory.ranking.floor", min: 0, max: 1, integer: false },
  { key: "weibullReinforceWeight", label: "memory.ranking.reinforce", min: 0, integer: false },
  { key: "weibullCrossSessionBoost", label: "memory.ranking.crossSession", min: 0, integer: false },
]

// draftFromSettings converts validated server numbers to text without changing their values.
// draftFromSettings 将经过校验的服务端数值转为文本，不改变其值。
function draftFromSettings(settings: VmmNativeRetrievalPolicySummary): RankingDraft {
  // textFields covers exactly the numeric keys listed in the closed editor descriptor.
  // textFields 准确覆盖封闭编辑器描述中列出的数值键。
  const textFields = Object.fromEntries(fields.map(({ key }) => [key, String(settings[key])])) as Record<NumericKey, string>
  return { ...settings, ...textFields }
}

// parseRankingDraft accepts a complete valid group; RRF is checked with BigInt and returned unchanged.
// parseRankingDraft 接纳完整有效组；RRF 使用 BigInt 校验后保持原文返回。
export function parseRankingDraft(draft: RankingDraft): VmmNativeRetrievalPolicySummary | null {
  if (!/^[1-9][0-9]{0,18}$/.test(draft.rrfK) || BigInt(draft.rrfK) > 9223372036854775807n) return null
  // values holds only numbers admitted by their actual policy bounds.
  // values 仅保存通过实际策略边界检查的数值。
  const values: [NumericKey, number][] = []
  for (const field of fields) {
    const text = draft[field.key].trim()
    const value = Number(text)
    if (!text || !Number.isFinite(value) || value < field.min
      || (field.exclusiveMin && value === field.min)
      || (field.max !== undefined && value > field.max)
      || (field.integer && !Number.isInteger(value))) return null
    values.push([field.key, value])
  }
  // numericFields replaces every text-backed numeric field only after all validations pass.
  // numericFields 仅在全部校验通过后替换每个以文本编辑的数值字段。
  const numericFields = Object.fromEntries(values) as Record<NumericKey, number>
  return { ...draft, ...numericFields }
}

// RankingProps shares the already-loaded policy to avoid competing catalog reads on the exclusive channel.
// RankingProps 共享已加载策略，避免在独占通道上竞争目录读取。
interface RankingProps {
  // Current effective settings and authoritative reset values.
  // 当前有效设置及权威重置值。
  settings: VmmNativeRetrievalPolicySummary
  defaults: VmmNativeRetrievalPolicySummary
  // Shared channel occupancy.
  // 共享通道占用状态。
  busy: boolean
  // Native action executor and accepted-result publisher.
  // 原生动作执行器及接纳结果发布者。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  onSaved: (policy: VmmNativeRuntimePolicySummary) => void
}

// VmmNativeRetrievalPolicy edits ranking separately from recall and never writes on mount or reset.
// VmmNativeRetrievalPolicy 独立于召回编辑排序，挂载或重置时绝不写入。
export function VmmNativeRetrievalPolicy({ settings, defaults, busy, onRunAction, onSaved }: RankingProps) {
  const { t } = useI18n()
  const [draft, setDraft] = React.useState(() => draftFromSettings(settings))
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [saved, setSaved] = React.useState(false)
  // accepted prevents a sibling recall save from discarding unchanged ranking drafts.
  // accepted 防止相邻召回保存丢弃服务端排序值未变的排序草稿。
  const accepted = React.useRef(settings)
  const mounted = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  React.useEffect(() => {
    if ((Object.keys(settings) as (keyof VmmNativeRetrievalPolicySummary)[]).some((key) => settings[key] !== accepted.current[key])) {
      setDraft(draftFromSettings(settings))
      accepted.current = settings
      // A refreshed policy supersedes the last save acknowledgement.
      // 刷新后的策略取代上一次保存确认，避免显示过时的成功状态。
      setSaved(false)
    }
  }, [settings])

  // save submits only the ranking group, preserving the current recall group in the host configuration gate.
  // save 仅提交排序组，由宿主配置锁内保留当前召回组。
  const save = async (ranking: VmmNativeRetrievalPolicySummary) => {
    setPending(true)
    setSaved(false)
    try {
      const response = await onRunAction("native_retrieval_policy_update", { ...ranking })
      if (!mounted.current) return
      if (response?.data.kind !== "native_runtime_policy") { setError(t("memory.policy.invalidResponse")); return }
      setDraft(draftFromSettings(response.data.value.retrieval))
      accepted.current = response.data.value.retrieval
      onSaved(response.data.value)
      setSaved(true)
      setError(null)
    } catch (reason) {
      if (mounted.current) setError(String(reason))
    } finally {
      if (mounted.current) setPending(false)
    }
  }

  const parsed = parseRankingDraft(draft)
  const changed = parsed && (Object.keys(settings) as (keyof VmmNativeRetrievalPolicySummary)[]).some((key) => parsed[key] !== settings[key])
  const disabled = busy || pending
  return <Card className="w-full min-w-0">
    <CardHeader><CardTitle>{t("memory.ranking.title")}</CardTitle><CardDescription>{t("memory.ranking.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {saved ? <p role="status">{t("memory.ranking.saved")}</p> : null}
      <div className="grid gap-3 sm:grid-cols-3">
        {([ ["hybridEnabled", "memory.ranking.hybrid"], ["mmrEnabled", "memory.ranking.mmr"], ["weibullEnabled", "memory.ranking.decay"] ] as const).map(([key, label]) =>
          <Label key={key} className="flex items-center justify-between gap-3 rounded-lg border p-3">
            {t(label)}<Switch checked={draft[key]} disabled={disabled} onCheckedChange={(checked) => { setDraft({ ...draft, [key]: checked }); setSaved(false) }} />
          </Label>)}
      </div>
      <details>
        <summary className="cursor-pointer text-sm font-medium">{t("memory.ranking.advanced")}</summary>
        <p className="my-3 text-xs text-muted-foreground">{t("memory.ranking.scoreHint")}</p>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2"><Label htmlFor="vmm-ranking-rrfK">{t("memory.ranking.rrf")}</Label>
            <Input id="vmm-ranking-rrfK" inputMode="numeric" value={draft.rrfK} disabled={disabled}
              onChange={(event) => { setDraft({ ...draft, rrfK: event.target.value }); setSaved(false) }} />
            <p className="text-xs text-muted-foreground">1–9223372036854775807</p>
          </div>
          {fields.map((field) => <div className="space-y-2" key={field.key}>
            <Label htmlFor={`vmm-ranking-${field.key}`}>{t(field.label)}</Label>
            <Input id={`vmm-ranking-${field.key}`} type="number" min={field.min} max={field.max} step={field.integer ? 1 : "any"}
              value={draft[field.key]} disabled={disabled} onChange={(event) => { setDraft({ ...draft, [field.key]: event.target.value }); setSaved(false) }} />
            <p className="text-xs text-muted-foreground">{field.exclusiveMin ? "> " : "≥ "}{field.min}{field.max !== undefined ? ` · ≤ ${field.max}` : ""}</p>
          </div>)}
        </div>
      </details>
      {!parsed ? <p role="alert" className="text-sm text-destructive">{t("memory.ranking.invalidValues")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button type="button" disabled={disabled || !changed || !parsed} onClick={() => { if (parsed) void save(parsed) }}>{t("memory.ranking.save")}</Button>
        <Button type="button" variant="outline" disabled={disabled} onClick={() => { setDraft(draftFromSettings(defaults)); setSaved(false) }}>{t("memory.policy.resetDraft")}</Button>
      </div>
    </CardContent>
  </Card>
}
