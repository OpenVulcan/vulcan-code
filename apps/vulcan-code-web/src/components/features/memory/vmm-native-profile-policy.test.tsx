// @vitest-environment jsdom

import { act } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { extractionDefaults } from "./vmm-native-extraction-policy.test-fixture"
import type { VmmMemoryActionResult, VmmNativeProfilePolicySummary } from "@/types"
import { parseProfileDraft, profileDraft, VmmNativeProfilePolicy } from "./vmm-native-profile-policy"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"
import { textDefaults } from "./vmm-native-text-policy.test-fixture"
import { rankingDefaults } from "./vmm-native-retrieval-policy.test-fixture"
import { retentionDefaults } from "./vmm-native-retention-settings.test-fixture"

// Isolate React's DOM test environment and restore process globals after this file.
// 隔离 React DOM 测试环境，并在本文件结束后恢复进程全局值。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

it("round-trips every duration exactly and rejects invalid formulas without rounding", () => {
  expect(profileDraft(profileDefaults).user.transient).toEqual({ baseMs: "3", stepMs: "2", maxMs: "14", unit: 86400000 })
  expect(parseProfileDraft(profileDraft(profileDefaults))).toEqual(profileDefaults)
  const unusual = structuredClone(profileDefaults)
  unusual.project.stable = { baseMs: 123, stepMs: 0, maxMs: 1234567 }
  expect(profileDraft(unusual).project.stable.unit).toBe(1)
  expect(parseProfileDraft(profileDraft(unusual))).toEqual(unusual)
  for (const value of ["", "-1", "0", "1.5", "3651", "9007199254740992"]) {
    const draft = profileDraft(profileDefaults)
    draft.project.stable.baseMs = value
    expect(parseProfileDraft(draft)).toBeNull()
  }
  const invalid = profileDraft(profileDefaults)
  invalid.user.transient.baseMs = "15"
  expect(parseProfileDraft(invalid)).toBeNull()
  invalid.user.transient.baseMs = "3"
  invalid.user.transient.unit = 7
  expect(parseProfileDraft(invalid)).toBeNull()
})

// response uses the full native policy envelope, including all independent sibling groups.
// response 使用完整原生策略信封，包括全部独立相邻配置组。
function response(profiles: VmmNativeProfilePolicySummary): VmmMemoryActionResult {
  const recall = { precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 6000, precheckRecallTopK: 5,
    precheckReviewCandidateLimit: 12, precheckMinReviewScore: 0.75, maxSearchQueries: 16,
    postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 6000, ...extractionDefaults }
  const value = { recall, defaults: recall, retrieval: rankingDefaults, retrievalDefaults: rankingDefaults,
    retention: retentionDefaults, retentionDefaults, text: textDefaults, textDefaults, profiles, profileDefaults, userOverride: true }
  return { action: "native_profile_policy_update", summary: "", kind: "native_runtime_policy", request: {}, response: value,
    data: { kind: "native_runtime_policy", value } }
}

// edit dispatches a real native input event, rather than reaching into component state.
// edit 派发真实原生输入事件，不访问组件内部状态。
async function edit(container: HTMLElement, id: string, value: string) {
  const input = container.querySelector<HTMLInputElement>(`#${id}`)!
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })) })
}

it("preserves independent drafts, submits exact units and uses acknowledged values only", async () => {
  const acknowledged = structuredClone(profileDefaults)
  acknowledged.user.transient.baseMs = 5 * 86400000
  const onRunAction = vi.fn(async () => response(acknowledged))
  const onSaved = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  // Render through the public component to exercise actual parent refresh behavior.
  // 通过公开组件渲染，验证实际父级刷新行为。
  const render = async (settings: VmmNativeProfilePolicySummary, busy = false) => {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeProfilePolicy settings={settings} defaults={profileDefaults} busy={busy} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>))
  }
  try {
    await render(profileDefaults)
    expect(onRunAction).not.toHaveBeenCalled()
    expect(container.textContent).toContain("持久级始终不过期")
    expect(container.querySelectorAll("input")).toHaveLength(18)
    await edit(container, "profile-user-transient-baseMs", "4")
    await render(structuredClone(profileDefaults))
    expect(container.querySelector<HTMLInputElement>("#profile-user-transient-baseMs")!.value).toBe("4")
    expect(container.querySelector<HTMLInputElement>("#profile-project-transient-baseMs")!.value).toBe("3")
    await render(structuredClone(profileDefaults), true)
    expect(container.querySelector<HTMLInputElement>("input")!.matches(":disabled")).toBe(true)
    expect(container.querySelector<HTMLButtonElement>("#profile-user-transient-unit")!.disabled).toBe(true)
    await render(structuredClone(profileDefaults))
    await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#profile-user-transient-unit")!, "小时")
    const save = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存画像期限")!
    await act(async () => save.click())
    const expected = structuredClone(profileDefaults)
    expected.user.transient = { baseMs: 4 * 3600000, stepMs: 2 * 3600000, maxMs: 14 * 3600000 }
    expect(onRunAction).toHaveBeenCalledExactlyOnceWith("native_profile_policy_update", expected)
    expect(onSaved).toHaveBeenCalledTimes(1)
    expect(container.querySelector<HTMLInputElement>("#profile-user-transient-baseMs")!.value).toBe("5")
    expect(container.querySelector('[role="status"]')).not.toBeNull()
    await act(async () => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "填入内置默认值（保存后生效）")!.click())
    expect(container.querySelector<HTMLInputElement>("#profile-user-transient-baseMs")!.value).toBe("3")
    expect(onRunAction).toHaveBeenCalledTimes(1)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it("blocks invalid formulas and preserves failed edits without optimistic success", async () => {
  const onRunAction = vi.fn(async () => null)
  const onSaved = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeProfilePolicy settings={profileDefaults} defaults={profileDefaults} busy={false} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>))
    const save = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存画像期限")!
    await edit(container, "profile-project-transient-baseMs", "15")
    expect(save.disabled).toBe(true)
    await edit(container, "profile-project-transient-baseMs", "1")
    const unit = container.querySelector<HTMLSelectElement>("#profile-project-transient-unit")!
    await act(async () => { unit.value = "3600000"; unit.dispatchEvent(new Event("change", { bubbles: true })) })
    await act(async () => save.click())
    expect(onRunAction).toHaveBeenCalledTimes(1)
    expect(onSaved).not.toHaveBeenCalled()
    expect(container.querySelector<HTMLInputElement>("#profile-project-transient-baseMs")!.value).toBe("1")
    expect(container.querySelector('[role="status"]')).toBeNull()
    expect(container.querySelector('[role="alert"]')).not.toBeNull()
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
