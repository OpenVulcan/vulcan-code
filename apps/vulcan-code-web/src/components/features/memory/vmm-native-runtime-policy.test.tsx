// @vitest-environment jsdom

import { act } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { extractionDefaults } from "./vmm-native-extraction-policy.test-fixture"
import type { VmmMemoryActionResult, VmmNativeRecallPolicySummary, VmmNativeRuntimePolicySummary } from "@/types"
import { rankingDefaults } from "./vmm-native-retrieval-policy.test-fixture"
import { retentionDefaults } from "./vmm-native-retention-settings.test-fixture"
import { textDefaults } from "./vmm-native-text-policy.test-fixture"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"
import { parseRecallDraft, VmmNativeRuntimePolicy } from "./vmm-native-runtime-policy"

// defaults supplies the exact server contract, not a guessed nested payload.
// defaults 提供准确的服务端契约，不使用猜测的嵌套载荷。
const defaults: VmmNativeRecallPolicySummary = {
  precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 6000,
  precheckRecallTopK: 5, precheckReviewCandidateLimit: 12, precheckMinReviewScore: 0.75,
  maxSearchQueries: 16, postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 6000,
  ...extractionDefaults,
}

// result projects one exact native response envelope consumed by the real settings component.
// result 投影一份真实设置组件消费的准确原生响应信封。
function result(input: Pick<VmmNativeRuntimePolicySummary, "recall" | "defaults" | "userOverride">): VmmMemoryActionResult {
  // value adds the exact ranking group supplied by the native response contract.
  // value 补充原生响应契约提供的准确排序组。
  const value: VmmNativeRuntimePolicySummary = { ...input, retrieval: rankingDefaults, retrievalDefaults: rankingDefaults, retention: retentionDefaults, retentionDefaults, text: textDefaults, textDefaults, profiles: profileDefaults, profileDefaults }
  return { action: "native_runtime_policy", summary: "", kind: "native_runtime_policy", request: {}, response: value,
    data: { kind: "native_runtime_policy", value } }
}

// changeInput uses a browser input event so assertions exercise React's actual editing path.
// changeInput 使用浏览器输入事件，使断言覆盖 React 的实际编辑路径。
async function changeInput(container: HTMLElement, key: keyof VmmNativeRecallPolicySummary, value: string) {
  const input = container.querySelector<HTMLInputElement>(`#vmm-policy-${key}`)!
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })) })
}

// button locates an explicit user-visible operation without relying on internal component state.
// button 定位显式的用户可见操作，不依赖组件内部状态。
function button(container: HTMLElement, text: string): HTMLButtonElement {
  const found = Array.from(container.querySelectorAll("button")).find((entry) => entry.textContent?.includes(text))
  expect(found).toBeDefined()
  return found!
}

// A failed initial read must retain both its error and a usable refresh action.
// 首次读取失败必须保留错误和可用的刷新操作。
it("keeps unavailable groups visible but disabled until a failed initial load recovers", async () => {
  // onRunAction fails only the first read so the same mounted page can recover.
  // onRunAction 仅令第一次读取失败，使同一挂载页面能够恢复。
  const onRunAction = vi.fn().mockResolvedValueOnce(null).mockResolvedValue(result({ recall: defaults, defaults, userOverride: false }))
  // container and root host the actual global policy owner and all five group entries.
  // container 和 root 挂载真实全局策略容器及全部五个分组入口。
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRuntimePolicy busy={false} onRunAction={onRunAction} /></I18nProvider>) })
    expect(container.querySelectorAll(".vk-page-tabs nav button")).toHaveLength(5)
    for (const label of ["文本清洗", "画像期限", "检索排序", "定期清洗"]) expect(button(container, label).disabled).toBe(true)
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("未返回有效结果")
    expect(container.textContent).not.toContain("处理中")
    await act(async () => { button(container, "刷新").click() })
    expect(onRunAction).toHaveBeenCalledTimes(2)
    expect(container.querySelector('[role="alert"]')).toBeNull()
    expect(button(container, "文本清洗").disabled).toBe(false)
    await act(async () => { button(container, "文本清洗").click() })
    expect(button(container, "文本清洗").getAttribute("aria-current")).toBe("page")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})

it("loads without saving, rejects invalid edits, and saves the exact independently edited policy", async () => {
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>) => {
    if (action === "native_runtime_policy") return result({ recall: defaults, defaults, userOverride: false })
    expect(action).toBe("native_runtime_policy_update")
    return result({ recall: payload as unknown as VmmNativeRecallPolicySummary, defaults, userOverride: true })
  })
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRuntimePolicy busy={false} onRunAction={onRunAction} /></I18nProvider>) })
    expect(onRunAction.mock.calls.map(([action]) => action)).toEqual(["native_runtime_policy"])
    expect(container.textContent).toContain("内置默认值（尚未写入用户配置）")
    expect(button(container, "保存召回与提炼策略").disabled).toBe(true)
    await changeInput(container, "precheckRecallTopK", "33")
    expect(container.textContent).toContain("请填写范围内的有效数值")
    expect(button(container, "保存召回与提炼策略").disabled).toBe(true)
    await changeInput(container, "precheckRecallTopK", "7")
    // These independent controls must reach the same exact global save payload as the recall limit.
    // 这些独立控制项必须与召回上限一起抵达同一准确全局保存参数。
    await changeInput(container, "precheckReviewCandidateLimit", "19")
    await changeInput(container, "precheckMinReviewScore", "0.6")
    await changeInput(container, "maxSearchQueries", "2")
    await changeInput(container, "precheckHistoryTurnLimit", "2")
    await changeInput(container, "precheckMaxInputTokens", "2048")
    await changeInput(container, "postactionHistoryTurnLimit", "5")
    await changeInput(container, "postactionMaxInputTokens", "3072")
    await changeInput(container, "postactionMemoryPriority", "73")
    await changeInput(container, "postactionMemoryRefreshWeight", "9")
    await changeInput(container, "postactionMemoryExpiryMs", "1.5")
    // Exercise the declared enum controls; duration initially displays days rather than raw milliseconds.
    // 覆盖声明的枚举控件；有效期初始显示天数而非原始毫秒。
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#vmm-policy-postactionMemoryStability")!, "归档保留")
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#vmm-policy-postactionMemoryLevel")!, "持久级")
    await act(async () => { button(container, "保存召回与提炼策略").click() })
    expect(onRunAction.mock.calls[1]).toEqual(["native_runtime_policy_update", { ...defaults, precheckRecallTopK: 7, precheckHistoryTurnLimit: 2, postactionHistoryTurnLimit: 5,
      precheckReviewCandidateLimit: 19, precheckMinReviewScore: 0.6, maxSearchQueries: 2, precheckMaxInputTokens: 2048, postactionMaxInputTokens: 3072,
      postactionMemoryPriority: 73, postactionMemoryRefreshWeight: 9, postactionMemoryExpiryMs: 129600000,
      postactionMemoryStability: "archival", postactionMemoryLevel: "persistent" }])
    expect(container.textContent).toContain("用户保存的全局策略")
    expect(container.textContent).toContain("策略已保存，新命令将使用此设置")
    await act(async () => { button(container, "填入内置默认值").click() })
    expect(onRunAction).toHaveBeenCalledTimes(2)
    expect(container.querySelector<HTMLInputElement>("#vmm-policy-precheckHistoryTurnLimit")!.value).toBe("3")
    expect(button(container, "保存召回与提炼策略").disabled).toBe(false)
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})

it("retains the edited draft and reports a failed save without claiming success", async () => {
  const onRunAction = vi.fn(async (action: string) => action === "native_runtime_policy"
    ? result({ recall: defaults, defaults, userOverride: false }) : null)
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRuntimePolicy busy={false} onRunAction={onRunAction} /></I18nProvider>) })
    await changeInput(container, "precheckRecallTopK", "7")
    await act(async () => { button(container, "保存召回与提炼策略").click() })
    expect(container.textContent).toContain("当前草稿尚未保存")
    expect(container.textContent).not.toContain("策略已保存")
    expect(container.querySelector<HTMLInputElement>("#vmm-policy-precheckRecallTopK")!.value).toBe("7")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})

it("rejects empty, fractional integer, nonfinite, and out-of-range drafts without clamping", () => {
  const draft = Object.fromEntries(Object.entries(defaults).map(([key, value]) => [key, String(value)])) as Record<keyof VmmNativeRecallPolicySummary, string>
  for (const invalid of ["", " ", "1.5", "NaN", "Infinity", "0", "65"]) {
    expect(parseRecallDraft({ ...draft, precheckHistoryTurnLimit: invalid })).toBeNull()
  }
  expect(parseRecallDraft({ ...draft, precheckMinReviewScore: "1.1" })).toBeNull()
  expect(parseRecallDraft({ ...draft, precheckMinReviewScore: "0" })?.precheckMinReviewScore).toBe(0)
  expect(parseRecallDraft(draft)).toEqual(defaults)
  for (const [key, invalid] of [
    ["postactionMemoryPriority", "101"], ["postactionMemoryRefreshWeight", "0"],
    ["postactionMemoryRefreshWeight", "1000001"], ["postactionMemoryExpiryMs", "0"],
    ["postactionMemoryExpiryMs", "315360000001"], ["postactionMemoryExpiryMs", "0.5"],
    ["postactionMemoryStability", "unknown"], ["postactionMemoryLevel", "unknown"],
  ]) expect(parseRecallDraft({ ...draft, [key]: invalid })).toBeNull()
})

// The shared action channel must be free before the initial read; later busy transitions retain edits.
// 初次读取前共享动作通道必须空闲；后续忙闲转换保留编辑内容。
it("waits for the shared channel and does not reload over an edited draft", async () => {
  const onRunAction = vi.fn(async () => result({ recall: defaults, defaults, userOverride: false }))
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  // renderState changes only the real busy prop rather than replacing component ownership.
  // renderState 仅改变真实 busy 属性，不替换组件归属。
  const renderState = async (busy: boolean) => {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRuntimePolicy busy={busy} onRunAction={onRunAction} /></I18nProvider>) })
  }
  try {
    await renderState(true)
    expect(onRunAction).not.toHaveBeenCalled()
    await renderState(false)
    expect(onRunAction).toHaveBeenCalledTimes(1)
    await changeInput(container, "precheckRecallTopK", "7")
    await renderState(true)
    await renderState(false)
    expect(onRunAction).toHaveBeenCalledTimes(1)
    expect(container.querySelector<HTMLInputElement>("#vmm-policy-precheckRecallTopK")!.value).toBe("7")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})
