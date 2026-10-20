import * as React from "react"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { Button } from "@/ui-kit/primitives/button"
import { DisclosureSection } from "@/ui-kit/primitives/disclosure"
import { Input } from "@/ui-kit/primitives/input"
import { Label } from "@/ui-kit/primitives/label"
import { useI18n } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeHostProjectPolicyOverrides, VmmNativeHostProjectPolicySummary, VmmNativeRecallPolicySummary, VmmNativeRetrievalPolicySummary, VmmNativeTextPolicySummary } from "@/types"
import { parseRecallDraft, recallDraft, recallFields, recallChoiceFields, recallKeys } from "./vmm-native-runtime-policy"
import { VmmNativeDurationInput } from "./vmm-native-duration-input"
import { VmmNativeTextPolicy } from "./vmm-native-text-policy"
import { VmmNativeRetrievalPolicy } from "./vmm-native-retrieval-policy"
import "@/ui-kit/styles/features/settings-forms.css"

// CallerPolicyProps fixes the caller identity for this mounted editor; the parent keys it by project tag.
// CallerPolicyProps 为当前挂载编辑器固定调用方身份；父级以项目标签作为组件键。
interface CallerPolicyProps {
  // Exact originating host tag, independent of the bound memory-project ID.
  // 准确发起宿主标签，独立于绑定记忆项目 ID。
  hostProjectId: string
  // Parent binding/configuration activity disables edits during conflicting actions.
  // 父级绑定/配置活动在冲突动作期间禁用编辑。
  busy: boolean
  // Existing explicit maintenance transport, never invoked with a global write action here.
  // 既有显式维护传输；此处绝不调用全局写入动作。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
}

// VmmNativeHostProjectPolicy displays group inheritance and saves exact caller overrides only.
// VmmNativeHostProjectPolicy 展示配置组继承状态，并仅保存准确调用方覆盖。
export function VmmNativeHostProjectPolicy({ hostProjectId, busy, onRunAction }: CallerPolicyProps) {
  const { t } = useI18n()
  const [policy, setPolicy] = React.useState<VmmNativeHostProjectPolicySummary | null>(null)
  const [draft, setDraft] = React.useState<ReturnType<typeof recallDraft> | null>(null)
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  // saved belongs only to the recall editor; delegated editors own their own acknowledgement labels.
  // saved 仅属于召回编辑器；委托编辑器自行持有确认提示。
  const [saved, setSaved] = React.useState(false)
  // Refresh is explicit and retries a failed initial read without remounting unrelated project settings.
  // 刷新是显式操作，可重试失败的首次读取而不重新挂载无关项目设置。
  const [refresh, setRefresh] = React.useState(0)
  // Refs preserve exact acknowledged ownership and reject concurrent or late navigation writes.
  // 引用保留准确已确认归属，并拒绝并发写入和导航后的迟到回复。
  const current = React.useRef<VmmNativeHostProjectPolicySummary | null>(null)
  const mounted = React.useRef(false)
  const occupied = React.useRef(false)
  const initialLoadIssued = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])

  // accept retains an unsaved recall draft when a sibling group alone changes.
  // accept 在仅相邻配置组变化时保留尚未保存的召回草稿。
  const accept = React.useCallback((next: VmmNativeHostProjectPolicySummary, resetRecall = false) => {
    const previous = current.current
    if (resetRecall || !previous || recallKeys.some((key) => previous.effective.recall[key] !== next.effective.recall[key])) setDraft(recallDraft(next.effective.recall))
    current.current = next
    setPolicy(next)
    setError(null)
  }, [])

  React.useEffect(() => {
    if (busy || initialLoadIssued.current) return
    initialLoadIssued.current = true
    occupied.current = true
    setPending(true)
    void onRunAction("native_host_project_policy", { hostProjectId }).then((result) => {
      if (!mounted.current) return
      if (result?.data.kind !== "native_host_project_policy" || result.data.value.hostProjectId !== hostProjectId) throw new Error(t("memory.policy.invalidResponse"))
      accept(result.data.value)
    }).catch((reason: unknown) => { if (mounted.current) setError(String(reason)) }).finally(() => {
      occupied.current = false
      if (mounted.current) setPending(false)
    })
  }, [accept, busy, hostProjectId, onRunAction, t, refresh])

  // save preserves sibling groups from the latest acknowledged response; null restores live inheritance.
  // save 从最新已确认响应保留相邻配置组；空值恢复实时继承。
  const save = async (group: keyof VmmNativeHostProjectPolicyOverrides, value: VmmNativeRecallPolicySummary | VmmNativeRetrievalPolicySummary | VmmNativeTextPolicySummary | null) => {
    if (!current.current || occupied.current || busy) return null
    occupied.current = true
    setPending(true)
    setSaved(false)
    try {
      const result = await onRunAction("native_host_project_policy_update", { hostProjectId, overrides: { ...current.current.overrides, [group]: value } })
      if (!mounted.current) return null
      if (result?.data.kind !== "native_host_project_policy" || result.data.value.hostProjectId !== hostProjectId) throw new Error(t("memory.policy.invalidResponse"))
      accept(result.data.value, group === "recall")
      setSaved(group === "recall")
      return result
    } catch (reason) {
      if (mounted.current) setError(String(reason))
      return null
    } finally {
      occupied.current = false
      if (mounted.current) setPending(false)
    }
  }

  // saveChild adapts the existing typed field editors to the caller endpoint, never to a global mutation.
  // saveChild 将既有强类型字段编辑器适配到调用方端点，绝不适配到全局变更。
  const saveChild = async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    if (!payload) throw new Error(t("memory.policy.invalidResponse"))
    let result: VmmMemoryActionResult | null
    if (action === "native_text_policy_update") result = await save("text", payload as unknown as VmmNativeTextPolicySummary)
    else if (action === "native_retrieval_policy_update") result = await save("retrieval", payload as unknown as VmmNativeRetrievalPolicySummary)
    else throw new Error("Unsupported caller policy editor action")
    if (result?.data.kind !== "native_host_project_policy") return null
    // Only the view-model envelope changes; all values come from the acknowledged caller response.
    // 仅改变视图模型信封；全部值均来自已确认调用方响应。
    const value = result.data.value.effective
    return { ...result, kind: "native_runtime_policy", response: value, data: { kind: "native_runtime_policy", value } }
  }
  const parsed = draft ? parseRecallDraft(draft) : null
  // Only explicit edits create an override; opening a group must not pin inherited defaults.
  // 仅显式编辑生成覆盖；展开配置组不能固化继承的默认值。
  const recallChanged = parsed && policy && recallKeys.some((key) => parsed[key] !== policy.effective.recall[key])
  const disabled = busy || pending
  // inheritance renders the current source and an explicit independent reset for one group.
  // inheritance 渲染当前来源及单个配置组的显式独立重置。
  const inheritance = (group: keyof VmmNativeHostProjectPolicyOverrides) => <div className="vk-form-bar vk-form-sm">
    <span>{t(policy!.overrides[group] === null ? "memory.hostPolicy.inherited" : "memory.hostPolicy.overridden")}</span>
    <Button variant="outline" disabled={disabled || policy!.overrides[group] === null} onClick={() => void save(group, null)}>{t("memory.hostPolicy.inherit")}</Button>
  </div>
  // source keeps each group's acknowledged source visible while its mounted editor is collapsed.
  // source 在已挂载的编辑器折叠时仍显示各组已确认的配置来源。
  const source = (group: keyof VmmNativeHostProjectPolicyOverrides) => t(policy!.overrides[group] === null ? "projectSettings.inheritGlobal" : "projectSettings.projectOverride")
  return <section className="project-vmm-policy vk-form-stack">
    <div className="vk-form-bar">
      <h3 className="vk-form-em">{t("memory.hostPolicy.title")}</h3>
      <Button variant="outline" size="sm" disabled={disabled} onClick={() => { initialLoadIssued.current = false; setRefresh((value) => value + 1); setSaved(false) }}>{t("memory.data.refresh")}</Button>
    </div>
    <p className="vk-form-text">{t("memory.hostPolicy.description")}</p>
    {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
    {policy && draft ? <>
      <DisclosureSection className="project-vmm-policy-group" description={t("projectSettings.policyGroup.recallHint")} keepMounted meta={source("recall")} title={t("projectSettings.policyGroup.recall")}>
        <div className="vk-form-stack">{inheritance("recall")}
        <div className="vk-form-grid">{recallFields.map((field) => <div key={field.key} className="vk-form-tight">
          <Label htmlFor={`caller-policy-${field.key}`}>{t(field.label)}</Label>
          {field.key === "postactionMemoryExpiryMs" ? <VmmNativeDurationInput id={`caller-policy-${field.key}`} value={draft[field.key]} disabled={disabled} onChange={(value) => { setDraft({ ...draft, [field.key]: value }); setSaved(false) }} /> : <Input id={`caller-policy-${field.key}`} type="number" min={field.min} max={field.max} step={field.step} disabled={disabled} value={draft[field.key]}
            onChange={(event) => { setDraft({ ...draft, [field.key]: event.target.value }); setSaved(false) }} />}
        </div>)}{recallChoiceFields.map((field) => <div key={field.key} className="vk-form-tight">
          <Label htmlFor={`caller-policy-${field.key}`}>{t(field.label)}</Label>
          <Select disabled={disabled} value={draft[field.key]} onValueChange={(value) => { setDraft({ ...draft, [field.key]: value }); setSaved(false) }}>
            <SelectTrigger id={`caller-policy-${field.key}`}><SelectValue /></SelectTrigger>
            <SelectContent>{field.options.map((option) => <SelectItem key={option.value} value={option.value}>{t(option.label)}</SelectItem>)}</SelectContent>
          </Select>
        </div>)}</div>
        <p className="vk-form-note">{t("memory.policy.extractedHint")}</p>
        {!parsed ? <p role="alert">{t("memory.policy.invalidValues")}</p> : null}
        {saved ? <p role="status">{t("memory.hostPolicy.saved")}</p> : null}
        <Button disabled={disabled || !parsed || !recallChanged} onClick={() => { if (parsed) void save("recall", parsed) }}>{t("memory.policy.save")}</Button>
        </div>
      </DisclosureSection>
      <DisclosureSection className="project-vmm-policy-group" description={t("projectSettings.policyGroup.rankingHint")} keepMounted meta={source("retrieval")} title={t("projectSettings.policyGroup.ranking")}>
        {inheritance("retrieval")}
        <VmmNativeRetrievalPolicy settings={policy.effective.retrieval} defaults={policy.effective.retrievalDefaults} busy={disabled} onRunAction={saveChild} onSaved={() => {}} />
      </DisclosureSection>
      <DisclosureSection className="project-vmm-policy-group" description={t("projectSettings.policyGroup.textHint")} keepMounted meta={source("text")} title={t("projectSettings.policyGroup.text")}>
        {inheritance("text")}
        <VmmNativeTextPolicy settings={policy.effective.text} defaults={policy.effective.textDefaults} busy={disabled} onRunAction={saveChild} onSaved={() => {}} />
      </DisclosureSection>
    </> : pending ? <StreamPlaceholder label={t("memory.common.loading")} shape="fields" count={3} /> : null}
  </section>
}
