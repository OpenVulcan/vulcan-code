// @vitest-environment jsdom

import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { extractionDefaults } from "./vmm-native-extraction-policy.test-fixture"
import type { VmmMemoryActionResult, VmmNativeRuntimePolicySummary, VmmNativeTextPolicySummary } from "@/types"
import { VmmNativeTextPolicy } from "./vmm-native-text-policy"
import { textDefaults } from "./vmm-native-text-policy.test-fixture"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"
import { rankingDefaults } from "./vmm-native-retrieval-policy.test-fixture"
import { retentionDefaults } from "./vmm-native-retention-settings.test-fixture"

// Declare React's real DOM act environment only for this isolated test file and restore it afterwards.
// 仅为当前隔离测试文件声明 React 真实 DOM act 环境，结束后恢复。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

// response retains every sibling group in the actual host response shape.
// response 在实际宿主响应结构中保留全部相邻配置组。
function response(text: VmmNativeTextPolicySummary): VmmMemoryActionResult {
  const recall = { precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 6000, precheckRecallTopK: 5,
    precheckReviewCandidateLimit: 12, precheckMinReviewScore: 0.75, maxSearchQueries: 16,
    postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 6000, ...extractionDefaults }
  const value: VmmNativeRuntimePolicySummary = { recall, defaults: recall, retrieval: rankingDefaults,
    retrievalDefaults: rankingDefaults, retention: retentionDefaults, retentionDefaults, text, textDefaults, profiles: profileDefaults, profileDefaults, userOverride: true }
  return { action: "native_text_policy_update", summary: "", kind: "native_runtime_policy", request: {}, response: value,
    data: { kind: "native_runtime_policy", value } }
}

// editBudget dispatches the native input event and preserves invalid intermediate strings.
// editBudget 派发原生输入事件，保留非法的中间输入字符串。
async function editBudget(container: HTMLElement, value: string) {
  const input = container.querySelector<HTMLInputElement>("#vmm-text-budget")!
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })) })
}

it("saves both controls only explicitly, preserves sibling-refresh drafts and publishes acknowledged values", async () => {
  const onRunAction = vi.fn(async () => response({ foldMachineText: false, maxFragmentTokens: 800 }))
  const onSaved = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  // Re-render through the real public component, never by altering its internal state.
  // 通过真实公开组件重渲染，绝不修改内部状态。
  const render = async (settings: VmmNativeTextPolicySummary, busy = false) => {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeTextPolicy settings={settings} defaults={textDefaults} busy={busy} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>))
  }
  try {
    await render(textDefaults)
    expect(onRunAction).not.toHaveBeenCalled()
    expect(container.textContent).toContain("PII 和噪音仍按各自设置执行")
    await editBudget(container, "600")
    await act(async () => container.querySelector<HTMLButtonElement>('[role="switch"]')!.click())
    await render({ ...textDefaults })
    expect(container.querySelector<HTMLInputElement>("input")!.value).toBe("600")
    await render({ ...textDefaults }, true)
    expect(container.querySelector<HTMLInputElement>("input")!.disabled).toBe(true)
    await render({ ...textDefaults })
    const save = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存文本清洗设置")!
    await act(async () => save.click())
    expect(onRunAction).toHaveBeenCalledExactlyOnceWith("native_text_policy_update", { foldMachineText: false, maxFragmentTokens: 600 })
    expect(onSaved).toHaveBeenCalledTimes(1)
    expect(container.querySelector<HTMLInputElement>("input")!.value).toBe("800")
    expect(container.querySelector('[role="status"]')!.textContent).toContain("已保存")
    await act(async () => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "填入内置默认值（保存后生效）")!.click())
    expect(container.querySelector<HTMLInputElement>("input")!.value).toBe("1200")
    expect(onRunAction).toHaveBeenCalledTimes(1)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it("rejects invalid token input and retains a failed draft without reporting successful save", async () => {
  const onRunAction = vi.fn(async () => null)
  const onSaved = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeTextPolicy settings={textDefaults} defaults={textDefaults} busy={false} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>))
    const save = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存文本清洗设置")!
    for (const invalid of ["", "0", "255", "256.5", "131073"]) {
      await editBudget(container, invalid)
      expect(save.disabled).toBe(true)
    }
    await editBudget(container, "256")
    await act(async () => save.click())
    expect(onRunAction).toHaveBeenCalledTimes(1)
    expect(onSaved).not.toHaveBeenCalled()
    expect(container.querySelector<HTMLInputElement>("input")!.value).toBe("256")
    expect(container.querySelector('[role="status"]')).toBeNull()
    expect(container.querySelector('[role="alert"]')).not.toBeNull()
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
