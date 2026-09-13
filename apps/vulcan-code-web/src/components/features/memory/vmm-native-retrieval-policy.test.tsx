// @vitest-environment jsdom

import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { extractionDefaults } from "./vmm-native-extraction-policy.test-fixture"
import type { VmmMemoryActionResult, VmmNativeRetrievalPolicySummary } from "@/types"
import { parseRankingDraft, VmmNativeRetrievalPolicy } from "./vmm-native-retrieval-policy"
import { rankingDefaults } from "./vmm-native-retrieval-policy.test-fixture"
import { retentionDefaults } from "./vmm-native-retention-settings.test-fixture"
import { textDefaults } from "./vmm-native-text-policy.test-fixture"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"

// stringDraft converts fixture numbers into the same editable text shape used by the form.
// stringDraft 将夹具数值转换为表单使用的同一可编辑文本结构。
function stringDraft(settings: VmmNativeRetrievalPolicySummary) {
  return Object.fromEntries(Object.entries(settings).map(([key, value]) => [key, typeof value === "number" ? String(value) : value])) as Parameters<typeof parseRankingDraft>[0]
}

it("preserves the full signed-64 RRF range and exact decimal weights", () => {
  const draft = stringDraft(rankingDefaults)
  expect(parseRankingDraft(draft)).toEqual(rankingDefaults)
  expect(parseRankingDraft({ ...draft, rrfK: "9223372036854775807", mmrLambda: "0.123456789" })).toEqual({ ...rankingDefaults, rrfK: "9223372036854775807", mmrLambda: 0.123456789 })
  for (const rrfK of ["", "0", "01", " 1", "+1", "1.5", "1e3", "9223372036854775808"]) {
    expect(parseRankingDraft({ ...draft, rrfK })).toBeNull()
  }
  for (const invalid of ["", "NaN", "Infinity", "1.5", "33"]) {
    expect(parseRankingDraft({ ...draft, rerankTopN: invalid })).toBeNull()
  }
  expect(parseRankingDraft({ ...draft, rerankTopN: "0", hardDedupeCosineThreshold: "0" })).toMatchObject({ rerankTopN: 0, hardDedupeCosineThreshold: 0 })
  expect(parseRankingDraft({ ...draft, weibullShape: "0" })).toBeNull()
  expect(parseRankingDraft({ ...draft, weibullScaleHours: "0.0001" })?.weibullScaleHours).toBe(0.0001)
})

// input sends the browser's actual edit event without reaching into component state.
// input 发送浏览器实际编辑事件，不读取组件内部状态。
async function input(container: HTMLElement, key: string, value: string) {
  const element = container.querySelector<HTMLInputElement>(`#vmm-ranking-${key}`)!
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
  await act(async () => { setter.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })) })
}

it("saves only the ranking group and never rounds the submitted RRF constant", async () => {
  const onSaved = vi.fn()
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>) => {
    expect(action).toBe("native_retrieval_policy_update")
    // recall is an independent server-owned group; ranking requests must not supply it.
    // recall 是独立的服务端归属组；排序请求不得提供它。
    const recall = { precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 6000, precheckRecallTopK: 5,
      precheckReviewCandidateLimit: 12, precheckMinReviewScore: 0.75, maxSearchQueries: 16,
    postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 6000, ...extractionDefaults }
    const value = { recall, defaults: recall, retrieval: payload as unknown as VmmNativeRetrievalPolicySummary, retrievalDefaults: rankingDefaults, retention: retentionDefaults, retentionDefaults, text: textDefaults, textDefaults, profiles: profileDefaults, profileDefaults, userOverride: true }
    return { action, summary: "", kind: "native_runtime_policy", request: payload, response: value,
      data: { kind: "native_runtime_policy", value } } satisfies VmmMemoryActionResult
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRetrievalPolicy settings={rankingDefaults} defaults={rankingDefaults} busy={false} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>) })
    expect(onRunAction).not.toHaveBeenCalled()
    await input(container, "rrfK", "9223372036854775807")
    await input(container, "rerankTopN", "0")
    // Replacement floors and hard-dedupe pools are distinct controls, including zero-as-disabled.
    // 替代门槛与硬去重池是不同控制项，包含以零表示关闭。
    await input(container, "replaceMinSimilarity", "0.86")
    await input(container, "hardDedupeCosineThreshold", "0")
    await input(container, "hardDedupePoolTopK", "3")
    await input(container, "replacementTopK", "2")
    // Real switch/input events must carry both channel enablement and the shared physical window.
    // 真实开关／输入事件必须携带通道启用与共享物理窗口。
    await input(container, "lexicalCandidateFloor", "3")
    // Diversity enablement and its fractional relevance weight must retain their separate declared types.
    // 多样性启用与小数相关性权重必须保留各自声明类型。
    await input(container, "mmrLambda", "0.25")
    // All six editable decay controls must be submitted with numeric precision and explicit enablement.
    // 全部六个可编辑衰减控制项必须以准确数值及显式启用状态提交。
    await input(container, "weibullShape", "2")
    await input(container, "weibullScaleHours", "0.5")
    await input(container, "weibullMinMultiplier", "0.25")
    await input(container, "weibullReinforceWeight", "0.5")
    await input(container, "weibullCrossSessionBoost", "0.75")
    await act(async () => Array.from(container.querySelectorAll("label")).find((label) => label.textContent === "读取时衰减")!.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    await act(async () => Array.from(container.querySelectorAll("label")).find((label) => label.textContent === "多样性选择（MMR）")!.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    await act(async () => Array.from(container.querySelectorAll("label")).find((label) => label.textContent === "向量与全文融合")!.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    const save = Array.from(container.querySelectorAll("button")).find((entry) => entry.textContent === "保存检索排序策略")!
    expect(save.disabled).toBe(false)
    await act(async () => { save.click() })
    expect(onRunAction.mock.calls[0][1]).toEqual({ ...rankingDefaults, rrfK: "9223372036854775807", rerankTopN: 0, hybridEnabled: false, lexicalCandidateFloor: 3, mmrEnabled: false, mmrLambda: 0.25,
      weibullEnabled: false, weibullShape: 2, weibullScaleHours: 0.5, weibullMinMultiplier: 0.25, weibullReinforceWeight: 0.5, weibullCrossSessionBoost: 0.75,
      replaceMinSimilarity: 0.86, hardDedupeCosineThreshold: 0, hardDedupePoolTopK: 3, replacementTopK: 2 })
    expect(onSaved).toHaveBeenCalledTimes(1)
    expect(container.textContent).toContain("排序策略已保存")
    // A later server refresh invalidates the old acknowledgement without issuing another write.
    // 后续服务端刷新使旧确认失效，不触发再次写入。
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRetrievalPolicy settings={{ ...rankingDefaults, rrfK: "31" }} defaults={rankingDefaults} busy={false} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>) })
    expect(container.textContent).not.toContain("排序策略已保存")
    expect(container.querySelector<HTMLInputElement>("#vmm-ranking-rrfK")!.value).toBe("31")
    expect(onRunAction).toHaveBeenCalledTimes(1)
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})

it("retains unsaved ranking edits when a sibling save returns an unchanged ranking group", async () => {
  const container = document.createElement("div")
  const root = createRoot(container)
  const onRunAction = vi.fn(async () => null)
  document.body.append(container)
  // renderSettings changes the server object identity without inventing a different persisted value.
  // renderSettings 改变服务端对象身份，不虚构不同的持久化值。
  const renderSettings = async (settings: VmmNativeRetrievalPolicySummary) => {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRetrievalPolicy settings={settings} defaults={rankingDefaults} busy={false} onRunAction={onRunAction} onSaved={() => {}} /></I18nProvider>) })
  }
  try {
    await renderSettings(rankingDefaults)
    await input(container, "mmrLambda", "0.5")
    await renderSettings({ ...rankingDefaults })
    expect(container.querySelector<HTMLInputElement>("#vmm-ranking-mmrLambda")!.value).toBe("0.5")
    const save = Array.from(container.querySelectorAll("button")).find((entry) => entry.textContent === "保存检索排序策略")!
    await act(async () => { save.click() })
    expect(container.textContent).toContain("当前草稿尚未保存")
    expect(container.textContent).not.toContain("排序策略已保存")
    expect(container.querySelector<HTMLInputElement>("#vmm-ranking-mmrLambda")!.value).toBe("0.5")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})
