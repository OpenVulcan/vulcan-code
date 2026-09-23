// @vitest-environment jsdom

import { act } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { extractionDefaults } from "./vmm-native-extraction-policy.test-fixture"
import type { VmmMemoryActionResult, VmmNativeHostProjectPolicyOverrides, VmmNativeHostProjectPolicySummary } from "@/types"
import { VmmNativeHostProjectPolicy } from "./vmm-native-host-project-policy"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"
import { textDefaults } from "./vmm-native-text-policy.test-fixture"
import { rankingDefaults } from "./vmm-native-retrieval-policy.test-fixture"
import { retentionDefaults } from "./vmm-native-retention-settings.test-fixture"

// Isolate React DOM assertions and restore process globals after this file.
// 隔离 React DOM 断言，在本文件结束后恢复进程全局值。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

// fixture builds the exact server inheritance envelope, with no user/project identity substitution.
// fixture 构建准确服务端继承信封，不替换用户或项目身份。
function fixture(overrides: VmmNativeHostProjectPolicyOverrides = { recall: null, retrieval: null, text: null }, hostProjectId = "host-a"): VmmNativeHostProjectPolicySummary {
  const recall = { precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 6000, precheckRecallTopK: 5,
    precheckReviewCandidateLimit: 12, precheckMinReviewScore: 0.75, maxSearchQueries: 16,
    postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 6000, ...extractionDefaults }
  return { hostProjectId, overrides, effective: {
    recall: overrides.recall ?? recall, defaults: recall,
    retrieval: overrides.retrieval ?? rankingDefaults, retrievalDefaults: rankingDefaults,
    text: overrides.text ?? textDefaults, textDefaults, profiles: profileDefaults, profileDefaults,
    retention: retentionDefaults, retentionDefaults, userOverride: false,
  } }
}

// envelope is the real caller-policy response shape, not a global policy response stub.
// envelope 是真实调用方策略响应形态，不是全局策略响应桩。
function envelope(value: VmmNativeHostProjectPolicySummary): VmmMemoryActionResult {
  return { action: "native_host_project_policy", summary: "", kind: "native_host_project_policy", request: {}, response: value,
    data: { kind: "native_host_project_policy", value } }
}

// edit drives the public native input event and returns only after React has processed it.
// edit 驱动公开原生输入事件，并仅在 React 处理后返回。
async function edit(container: HTMLElement, id: string, value: string) {
  const input = container.querySelector<HTMLInputElement>(`#${id}`)!
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })) })
}

it("writes only caller groups, preserves sibling drafts and restores live inheritance explicitly", async () => {
  let saved = fixture()
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>) => {
    expect(payload?.hostProjectId).toBe("host-a")
    if (action === "native_host_project_policy_update") saved = fixture(payload!.overrides as VmmNativeHostProjectPolicyOverrides)
    else expect(action).toBe("native_host_project_policy")
    return envelope(saved)
  })
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeHostProjectPolicy hostProjectId="host-a" busy={false} onRunAction={onRunAction} /></I18nProvider>))
    expect(onRunAction).toHaveBeenCalledExactlyOnceWith("native_host_project_policy", { hostProjectId: "host-a" })
    expect(container.textContent).toContain("跟随全局设置")
    expect(container.querySelectorAll("#profile-user-transient-baseMs")).toHaveLength(0)
    // Group disclosure changes no policy and never unmounts unsaved child editors.
    // 配置组展开不改变策略，也不卸载带有未保存草稿的子编辑器。
    const groups = Array.from(container.querySelectorAll<HTMLElement>(".project-vmm-policy-group"))
    expect(groups).toHaveLength(3)
    expect(groups.every((group) => group.dataset.state === "closed")).toBe(true)
    await act(async () => groups.forEach((group) => group.querySelector<HTMLButtonElement>("[data-slot=\"disclosure-trigger\"]")!.click()))
    expect(onRunAction).toHaveBeenCalledTimes(1)
    expect(Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "保存召回与提炼策略")!.disabled).toBe(true)
    await edit(container, "caller-policy-precheckHistoryTurnLimit", "2")
    await edit(container, "caller-policy-precheckMaxInputTokens", "512")
    await edit(container, "caller-policy-postactionMaxInputTokens", "1024")
    // Caller-specific precheck controls must remain in this host's recall group across sibling saves.
    // 调用方专属预检控制项在保存其他组时必须保留在此宿主召回组中。
    await edit(container, "caller-policy-precheckRecallTopK", "1")
    await edit(container, "caller-policy-precheckReviewCandidateLimit", "2")
    await edit(container, "caller-policy-precheckMinReviewScore", "0.25")
    await edit(container, "caller-policy-maxSearchQueries", "1")
    await edit(container, "caller-policy-postactionMemoryPriority", "81")
    await edit(container, "caller-policy-postactionMemoryRefreshWeight", "7")
    await edit(container, "caller-policy-postactionMemoryExpiryMs", "2.5")
    // Project enum edits belong only to the caller recall group, not to global profile ownership.
    // 项目枚举编辑仅属于调用方召回组，不属于全局画像归属。
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#caller-policy-postactionMemoryStability")!, "短期工作")
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#caller-policy-postactionMemoryLevel")!, "阶段级")
    await edit(container, "vmm-text-budget", "512")
    await act(async () => groups[2].querySelector<HTMLButtonElement>("[data-slot=\"disclosure-trigger\"]")!.click())
    await act(async () => groups[2].querySelector<HTMLButtonElement>("[data-slot=\"disclosure-trigger\"]")!.click())
    expect(container.querySelector<HTMLInputElement>("#vmm-text-budget")!.value).toBe("512")
    // The embedded ranking editor must persist only this host's explicit rerank prefix.
    // 内嵌排序编辑器必须仅持久化此宿主的显式重排前缀。
    await edit(container, "vmm-ranking-rerankTopN", "2")
    // Caller overrides preserve an exact floor endpoint and a hard pool larger than the review window.
    // 调用方覆盖保留准确门槛端点及大于评审窗口的硬去重池。
    await edit(container, "vmm-ranking-replaceMinSimilarity", "1")
    await edit(container, "vmm-ranking-hardDedupeCosineThreshold", "0.55")
    await edit(container, "vmm-ranking-hardDedupePoolTopK", "3")
    await edit(container, "vmm-ranking-replacementTopK", "1")
    // Hybrid, lexical window and exact decimal RRF edits belong to the same caller-owned group.
    // 混合、词法窗口及准确十进制倒数排名编辑属于同一调用方配置组。
    await edit(container, "vmm-ranking-lexicalCandidateFloor", "1")
    await edit(container, "vmm-ranking-rrfK", "1")
    // Zero is a valid caller relevance weight, and disabling MMR must not erase that explicit value.
    // 零是有效调用方相关性权重，关闭多样性不能抹去此显式值。
    await edit(container, "vmm-ranking-mmrLambda", "0")
    // Caller decay fields preserve sub-hour scales and explicit zero reinforcement/cross-session weights.
    // 调用方衰减字段保留小数小时尺度及显式零强化／跨会话权重。
    await edit(container, "vmm-ranking-weibullShape", "0.75")
    await edit(container, "vmm-ranking-weibullScaleHours", "0.125")
    await edit(container, "vmm-ranking-weibullMinMultiplier", "0.6")
    await edit(container, "vmm-ranking-weibullReinforceWeight", "0")
    await edit(container, "vmm-ranking-weibullCrossSessionBoost", "0")
    await act(async () => Array.from(container.querySelectorAll("label")).find((label) => label.textContent === "读取时衰减")!.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    await act(async () => Array.from(container.querySelectorAll("label")).find((label) => label.textContent === "多样性选择（MMR）")!.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    await act(async () => Array.from(container.querySelectorAll("label")).find((label) => label.textContent === "向量与全文融合")!.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    const buttons = () => Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
    await act(async () => buttons().find((button) => button.textContent === "保存文本清洗设置")!.click())
    expect(saved.overrides.text?.maxFragmentTokens).toBe(512)
    expect(saved.overrides.recall).toBeNull()
    expect(container.querySelector<HTMLInputElement>("#vmm-ranking-rerankTopN")!.value).toBe("2")
    await act(async () => buttons().find((button) => button.textContent === "保存检索排序策略")!.click())
    expect(saved.overrides.retrieval).toEqual({ ...rankingDefaults, rerankTopN: 2, hybridEnabled: false, lexicalCandidateFloor: 1, rrfK: "1", mmrEnabled: false, mmrLambda: 0,
      weibullEnabled: false, weibullShape: 0.75, weibullScaleHours: 0.125, weibullMinMultiplier: 0.6, weibullReinforceWeight: 0, weibullCrossSessionBoost: 0,
      replaceMinSimilarity: 1, hardDedupeCosineThreshold: 0.55, hardDedupePoolTopK: 3, replacementTopK: 1 })
    expect(saved.overrides.text?.maxFragmentTokens).toBe(512)
    expect(saved.overrides.recall).toBeNull()
    expect(container.querySelector<HTMLInputElement>("#caller-policy-precheckHistoryTurnLimit")!.value).toBe("2")
    await act(async () => buttons().find((button) => button.textContent === "保存召回与提炼策略")!.click())
    expect(saved.overrides.recall?.precheckHistoryTurnLimit).toBe(2)
    expect(saved.overrides.recall).toMatchObject({ postactionMemoryPriority: 81,
      precheckRecallTopK: 1, precheckReviewCandidateLimit: 2, precheckMinReviewScore: 0.25, maxSearchQueries: 1, precheckMaxInputTokens: 512, postactionMaxInputTokens: 1024,
      postactionMemoryRefreshWeight: 7, postactionMemoryExpiryMs: 216000000,
      postactionMemoryStability: "working", postactionMemoryLevel: "phase" })
    expect(saved.overrides.text?.maxFragmentTokens).toBe(512)
    await act(async () => buttons().filter((button) => button.textContent === "改回跟随全局")[0].click())
    expect(saved.overrides.recall).toBeNull()
    expect(saved.overrides.text?.maxFragmentTokens).toBe(512)
    // An acknowledged recall action must not claim that another group's pending draft was saved.
    // 已确认的召回动作不能宣称另一组待保存草稿已经保存。
    expect(groups[0].querySelector('[role="status"]')?.textContent).toContain("已保存，从下一条消息开始生效")
    expect(container.querySelector('.project-vmm-policy > [role="status"]')).toBeNull()
    expect(container.querySelector<HTMLInputElement>("#caller-policy-precheckHistoryTurnLimit")!.value).toBe("3")
    expect(container.querySelector<HTMLInputElement>("#caller-policy-postactionMemoryExpiryMs")!.value).toBe("180")
    expect(container.querySelector("#caller-policy-postactionMemoryLevel")!.textContent).toBe("稳定级")
    // Restoring retrieval inheritance must preserve the independently saved text group.
    // 恢复检索继承必须保留独立保存的文本组。
    await act(async () => buttons().filter((button) => button.textContent === "改回跟随全局")[1].click())
    expect(saved.overrides.retrieval).toBeNull()
    expect(saved.overrides.text?.maxFragmentTokens).toBe(512)
    expect(groups[0].querySelector('[role="status"]')).toBeNull()
    expect(container.querySelector<HTMLInputElement>("#vmm-ranking-rerankTopN")!.value).toBe(String(rankingDefaults.rerankTopN))
    expect(onRunAction.mock.calls.every(([action]) => action.startsWith("native_host_project_policy"))).toBe(true)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it("retries failed loads and rejects a response for another caller without applying it", async () => {
  const onRunAction = vi.fn().mockResolvedValueOnce(null).mockResolvedValueOnce(envelope(fixture()))
    .mockResolvedValueOnce(envelope(fixture(undefined, "host-b")))
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeHostProjectPolicy hostProjectId="host-a" busy={false} onRunAction={onRunAction} /></I18nProvider>))
    expect(container.querySelector('[role="alert"]')).not.toBeNull()
    await act(async () => container.querySelector<HTMLButtonElement>("button")!.click())
    expect(container.querySelector('[role="alert"]')).toBeNull()
    await edit(container, "caller-policy-precheckHistoryTurnLimit", "2")
    await act(async () => Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "保存召回与提炼策略")!.click())
    expect(container.querySelector('[role="alert"]')).not.toBeNull()
    expect(container.querySelector('[role="status"]')).toBeNull()
    expect(container.querySelector<HTMLInputElement>("#caller-policy-precheckHistoryTurnLimit")!.value).toBe("2")
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
