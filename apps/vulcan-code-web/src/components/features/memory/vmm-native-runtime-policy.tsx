import * as React from "react"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { RefreshCw, RotateCcw, Save } from "lucide-react"
import { Button } from "@/components/ui/button"
import { PageTabs } from "@/components/workspace/workspace-directory-layout"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useI18n } from "@/i18n"
import { VmmNativeRetrievalPolicy } from "./vmm-native-retrieval-policy"
import { VmmNativeRetentionSettings } from "./vmm-native-retention-settings"
import { VmmNativeTextPolicy } from "./vmm-native-text-policy"
import { VmmNativeProfilePolicy } from "./vmm-native-profile-policy"
import { VmmNativeDurationInput } from "./vmm-native-duration-input"
import type { TranslationKey } from "@/i18n-messages"
import type { VmmMemoryActionResult, VmmNativeRecallPolicySummary, VmmNativeRuntimePolicySummary } from "@/types"

// RecallField binds a visible input to one exact server field and its closed numeric range.
// RecallField 将可见输入绑定到一个准确的服务端字段及其封闭数值范围。
interface RecallField {
  // Exact typed field, never a user-editable configuration ID.
  // 准确的强类型字段，不是可供用户编辑的配置 ID。
  key: Exclude<keyof VmmNativeRecallPolicySummary, "postactionMemoryStability" | "postactionMemoryLevel">
  // Localized user label.
  // 本地化用户标签。
  label: TranslationKey
  // Inclusive minimum and maximum accepted by Core.
  // Core 接受的包含端点的最小值和最大值。
  min: number
  max: number
  // Input increment; only the reviewer score permits fractional values.
  // 输入增量；仅审核评分允许小数。
  step: number
}

// recallFields lists business inputs without exposing host-owned score contracts or provider settings.
// recallFields 列出业务输入，不暴露宿主持有的评分契约或供应商设置。
export const recallFields: RecallField[] = [
  { key: "precheckHistoryTurnLimit", label: "memory.policy.precheckHistory", min: 1, max: 64, step: 1 },
  { key: "precheckMaxInputTokens", label: "memory.policy.precheckBudget", min: 1, max: 131072, step: 1 },
  { key: "precheckRecallTopK", label: "memory.policy.recallTopK", min: 1, max: 32, step: 1 },
  { key: "precheckReviewCandidateLimit", label: "memory.policy.reviewLimit", min: 1, max: 256, step: 1 },
  { key: "precheckMinReviewScore", label: "memory.policy.reviewScore", min: 0, max: 1, step: 0.01 },
  { key: "maxSearchQueries", label: "memory.policy.queryLimit", min: 1, max: 64, step: 1 },
  { key: "postactionHistoryTurnLimit", label: "memory.policy.postactionHistory", min: 1, max: 64, step: 1 },
  { key: "postactionMaxInputTokens", label: "memory.policy.postactionBudget", min: 1, max: 131072, step: 1 },
  { key: "postactionMemoryPriority", label: "memory.policy.extractedPriority", min: 0, max: 100, step: 1 },
  { key: "postactionMemoryRefreshWeight", label: "memory.policy.extractedWeight", min: 1, max: 1000000, step: 1 },
  { key: "postactionMemoryExpiryMs", label: "memory.policy.extractedExpiry", min: 1, max: 315360000000, step: 1 },
]

// Closed human-readable options map exactly to the API enums, not numeric internal discriminants.
// 封闭的可读选项准确映射到接口枚举，不使用内部数字判别值。
export const recallChoiceFields: Array<{ key: "postactionMemoryStability" | "postactionMemoryLevel"; label: TranslationKey; options: Array<{ value: string; label: TranslationKey }> }> = [
  { key: "postactionMemoryStability", label: "memory.policy.extractedStability", options: [
    { value: "working", label: "memory.policy.working" }, { value: "stable", label: "memory.policy.stable" }, { value: "archival", label: "memory.policy.archival" },
  ] },
  { key: "postactionMemoryLevel", label: "memory.policy.extractedLevel", options: [
    { value: "session", label: "memory.nativeRecords.levelSession" }, { value: "phase", label: "memory.nativeRecords.levelPhase" },
    { value: "stable", label: "memory.nativeRecords.levelStable" }, { value: "persistent", label: "memory.nativeRecords.levelPersistent" },
  ] },
]

// Every field participates in dirty detection and caller-group refresh, including closed choices.
// 每个字段都参与变更检测及调用方配置组刷新，包括封闭选项。
export const recallKeys = [...recallFields.map(({ key }) => key), ...recallChoiceFields.map(({ key }) => key)]

// RecallDraft retains empty and intermediate input text until explicit validation succeeds.
// RecallDraft 保留空值及编辑中的输入文本，直到显式校验通过。
type RecallDraft = Record<keyof VmmNativeRecallPolicySummary, string>

// recallDraft renders one complete authoritative policy as editable strings without normalization.
// recallDraft 将完整权威策略转换为可编辑字符串，不进行归一化。
export function recallDraft(recall: VmmNativeRecallPolicySummary): RecallDraft {
  return Object.fromEntries(recallKeys.map((key) => [key, String(recall[key])])) as RecallDraft
}

// parseRecallDraft returns validated numbers and closed choices or null; blank inputs never imply zero.
// parseRecallDraft 返回经过校验的数值与封闭选项或 null；空输入不会隐式变成零。
export function parseRecallDraft(draft: RecallDraft): VmmNativeRecallPolicySummary | null {
  // entries are accepted only as a complete set, matching the closed server document.
  // entries 仅以完整集合接纳，与封闭的服务端文档一致。
  const entries: [keyof VmmNativeRecallPolicySummary, number | string][] = []
  for (const field of recallFields) {
    const text = draft[field.key].trim()
    const value = Number(text)
    if (!text || !Number.isFinite(value) || value < field.min || value > field.max
      || (field.step === 1 && !Number.isInteger(value))) return null
    entries.push([field.key, value])
  }
  for (const field of recallChoiceFields) {
    if (!field.options.some((option) => option.value === draft[field.key])) return null
    entries.push([field.key, draft[field.key]])
  }
  return Object.fromEntries(entries) as unknown as VmmNativeRecallPolicySummary
}

// VmmNativeRuntimePolicyProps uses the existing native-only maintenance action transport.
// VmmNativeRuntimePolicyProps 使用现有仅原生维护动作传输。
interface VmmNativeRuntimePolicyProps {
  // Whether the shared memory action channel is occupied.
  // 共享记忆动作通道是否被占用。
  busy: boolean
  // Executes one action and returns the typed host response or a reported failure.
  // 执行一个动作，返回强类型宿主响应或已报告的失败。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  // leading is one sibling page placed first in the same tab strip, so the rules page needs no nested tabs.
  // leading 是放在同一标签条首位的同级页面，使规则页无需嵌套标签。
  leading?: { label: React.ReactNode; content: React.ReactNode }
}

// VmmNativeRuntimePolicy renders effective global defaults and saves only explicit user edits.
// VmmNativeRuntimePolicy 渲染有效全局默认策略，并仅保存显式用户编辑。
export function VmmNativeRuntimePolicy({ busy, onRunAction, leading }: VmmNativeRuntimePolicyProps) {
  const { t } = useI18n()
  // policyTab controls visibility only; all loaded drafts retain their ownership.
  // policyTab 仅控制可见性；已加载草稿均保留其归属。
  const [policyTab, setPolicyTab] = React.useState(leading ? "leading" : "recall")
  const [policy, setPolicy] = React.useState<VmmNativeRuntimePolicySummary | null>(null)
  const [draft, setDraft] = React.useState<RecallDraft | null>(null)
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [saved, setSaved] = React.useState(false)
  // mounted rejects late responses after navigation; initialLoadIssued avoids resetting edited drafts.
  // mounted 拒绝导航后的迟到响应；initialLoadIssued 避免重置已编辑草稿。
  const mounted = React.useRef(false)
  const initialLoadIssued = React.useRef(false)

  React.useEffect(() => {
    mounted.current = true
    return () => { mounted.current = false }
  }, [])

  // accept publishes exactly what the host accepted rather than the optimistic local draft.
  // accept 发布宿主准确接纳的值，不使用乐观的本地草稿。
  const accept = React.useCallback((next: VmmNativeRuntimePolicySummary) => {
    setPolicy(next)
    setDraft(recallDraft(next.recall))
    setError(null)
  }, [])

  React.useEffect(() => {
    if (busy || initialLoadIssued.current) return
    initialLoadIssued.current = true
    setPending(true)
    void onRunAction("native_runtime_policy").then((result) => {
      if (!mounted.current) return
      if (result?.data.kind === "native_runtime_policy") accept(result.data.value)
      else setError(t("memory.policy.invalidResponse"))
    }).catch((reason: unknown) => {
      if (mounted.current) setError(String(reason))
    }).finally(() => {
      if (mounted.current) setPending(false)
    })
  }, [accept, busy, onRunAction, t])

  // execute validates response ownership for a refresh or explicit save and keeps failed drafts visible.
  // execute 校验刷新或显式保存响应的归属，并在失败时保持草稿可见。
  const execute = async (recall?: VmmNativeRecallPolicySummary) => {
    setPending(true)
    setSaved(false)
    try {
      const result = await onRunAction(recall ? "native_runtime_policy_update" : "native_runtime_policy", recall ? { ...recall } : undefined)
      if (!mounted.current) return
      if (result?.data.kind !== "native_runtime_policy") {
        setError(t("memory.policy.invalidResponse"))
        return
      }
      accept(result.data.value)
      setSaved(recall !== undefined)
    } catch (reason) {
      if (mounted.current) setError(String(reason))
    } finally {
      if (mounted.current) setPending(false)
    }
  }

  const parsed = draft ? parseRecallDraft(draft) : null
  const changed = parsed && policy && recallKeys.some((key) => parsed[key] !== policy.recall[key])
  const disabled = busy || pending

  return <>
    {/* Keep every group discoverable without leaving the only recovery surface before data arrives.
        保留全部分组入口，在数据到达前避免离开唯一可重试的区域。 */}
    <PageTabs
      ariaLabel={leading ? t("memory.rules.title") : t("memory.workspace.globalPolicy")}
      className="vmm-page-tabs"
      onValueChange={setPolicyTab}
      options={[
        ...(leading ? [{ label: leading.label, value: "leading" }] : []),
        ...([["recall", "memory.workspace.recallPolicy"], ["text", "memory.workspace.textPolicy"], ["profile", "memory.workspace.profilePolicy"], ["retrieval", "memory.workspace.rankingPolicy"], ["retention", "memory.workspace.retentionPolicy"]] as const).map(([value, label]) => ({ disabled: !policy && value !== "recall", label: t(label), value })),
      ]}
      value={policyTab}
    />
    {leading ? <div hidden={policyTab !== "leading"}>{leading.content}</div> : null}
    <div hidden={policyTab !== "recall"}><Card className="w-full min-w-0">
    <CardHeader>
      <CardTitle>{t("memory.policy.title")}</CardTitle>
      <CardDescription>{t("memory.policy.description")}</CardDescription>
    </CardHeader>
    <CardContent className="space-y-4">
      {policy ? <p className="text-sm text-muted-foreground">{t(policy.userOverride ? "memory.policy.userSource" : "memory.policy.defaultSource")}</p> : disabled ? <StreamPlaceholder label={t("memory.common.loading")} shape="fields" count={2} /> : null}
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {saved ? <p role="status" className="text-sm text-muted-foreground">{t("memory.policy.saved")}</p> : null}
      {draft ? <div className="grid gap-4 sm:grid-cols-2">
        {recallFields.map((field) => <div className="space-y-2" key={field.key}>
          <Label htmlFor={`vmm-policy-${field.key}`}>{t(field.label)}</Label>
          {field.key === "postactionMemoryExpiryMs" ? <VmmNativeDurationInput id={`vmm-policy-${field.key}`} value={draft[field.key]} disabled={disabled} onChange={(value) => { setDraft({ ...draft, [field.key]: value }); setSaved(false) }} /> : <Input id={`vmm-policy-${field.key}`} type="number" min={field.min} max={field.max} step={field.step}
            value={draft[field.key]} disabled={disabled} onChange={(event) => {
              setDraft({ ...draft, [field.key]: event.target.value })
              setSaved(false)
            }} />}
          <p className="text-xs text-muted-foreground">{field.key === "postactionMemoryExpiryMs" ? t("memory.policy.expiryRange") : `${field.min}–${field.max}`}</p>
        </div>)}
        {recallChoiceFields.map((field) => <div key={field.key} className="space-y-2">
          <Label htmlFor={`vmm-policy-${field.key}`}>{t(field.label)}</Label>
          <Select disabled={disabled} value={draft[field.key]} onValueChange={(value) => { setDraft({ ...draft, [field.key]: value }); setSaved(false) }}>
            <SelectTrigger id={`vmm-policy-${field.key}`} className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent>{field.options.map((option) => <SelectItem key={option.value} value={option.value}>{t(option.label)}</SelectItem>)}</SelectContent>
          </Select>
        </div>)}
      </div> : null}
      <p className="text-xs text-muted-foreground">{t("memory.policy.extractedHint")}</p>
      <p className="text-xs text-muted-foreground">{t("memory.policy.scoreHint")}</p>
      {draft && !parsed ? <p role="alert" className="text-sm text-destructive">{t("memory.policy.invalidValues")}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Button type="button" disabled={disabled || !changed || !parsed} onClick={() => { if (parsed) void execute(parsed) }}>
          <Save className="size-4" aria-hidden="true" />{t("memory.policy.save")}
        </Button>
        <Button type="button" variant="outline" disabled={disabled || !policy} onClick={() => {
          if (policy) setDraft(recallDraft(policy.defaults))
          setSaved(false)
        }}><RotateCcw className="size-4" aria-hidden="true" />{t("memory.policy.resetDraft")}</Button>
        <Button type="button" variant="outline" disabled={disabled} onClick={() => void execute()}>
          <RefreshCw className="size-4" aria-hidden="true" />{t("memory.data.refresh")}
        </Button>
      </div>
    </CardContent>
  </Card></div>
    {policy ? <div hidden={policyTab !== "text"}><VmmNativeTextPolicy settings={policy.text} defaults={policy.textDefaults}
      busy={disabled} onRunAction={onRunAction} onSaved={setPolicy} /></div> : null}
    {policy ? <div hidden={policyTab !== "profile"}><VmmNativeProfilePolicy settings={policy.profiles} defaults={policy.profileDefaults}
      busy={disabled} onRunAction={onRunAction} onSaved={setPolicy} /></div> : null}
    {policy ? <div hidden={policyTab !== "retrieval"}><VmmNativeRetrievalPolicy settings={policy.retrieval} defaults={policy.retrievalDefaults}
      busy={disabled} onRunAction={onRunAction} onSaved={setPolicy} /></div> : null}
    {policy ? <div hidden={policyTab !== "retention"}><VmmNativeRetentionSettings settings={policy.retention} defaults={policy.retentionDefaults}
      busy={disabled} onRunAction={onRunAction} onSaved={setPolicy} /></div> : null}
  </>
}
