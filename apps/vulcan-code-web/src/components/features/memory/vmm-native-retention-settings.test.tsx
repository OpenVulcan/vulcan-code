// @vitest-environment jsdom

import { act } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { extractionDefaults } from "./vmm-native-extraction-policy.test-fixture"
import type { VmmMemoryActionResult, VmmNativeRetentionSettingsSummary } from "@/types"
import { parseRetentionDraft, retentionDraft, VmmNativeRetentionSettings } from "./vmm-native-retention-settings"
import { retentionDefaults } from "./vmm-native-retention-settings.test-fixture"
import { rankingDefaults } from "./vmm-native-retrieval-policy.test-fixture"
import { textDefaults } from "./vmm-native-text-policy.test-fixture"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"

it("round trips exact durations, validates units and rejects invalid whole numbers", () => {
  const draft = retentionDraft(retentionDefaults)
  expect(draft.sessionIdleMs).toBe("15")
  expect(draft.trashRetentionMs).toBe("30")
  expect(draft.units.sessionIdleMs).toBe(86400000)
  expect(parseRetentionDraft(draft)).toEqual(retentionDefaults)
  for (const milliseconds of [1, 17, 1000, 3600000, 9223372036854]) {
    const settings = { ...retentionDefaults, sessionIdleMs: milliseconds, trashRetentionMs: milliseconds }
    expect(parseRetentionDraft(retentionDraft(settings))).toEqual(settings)
  }
  for (const input of ["", "0", "1.5", "NaN", "Infinity", "4294967296"]) {
    expect(parseRetentionDraft({ ...draft, scanIntervalMinutes: input })).toBeNull()
  }
  expect(parseRetentionDraft({ ...draft, units: { ...draft.units, sessionIdleMs: 2 } })).toBeNull()
  expect(parseRetentionDraft({ ...draft, protectedMemoryLevelFloor: "5" })).toBeNull()
  expect(parseRetentionDraft({ ...draft, protectedPriorityFloor: "101" })).toBeNull()
})

// changeInput dispatches the browser's real input event without modifying React state directly.
// changeInput 派发浏览器实际输入事件，不直接修改 React 状态。
async function changeInput(container: HTMLElement, key: string, value: string) {
  const element = container.querySelector<HTMLInputElement>(`#vmm-cleanup-${key}`)!
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!
  await act(async () => { setter.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })) })
}

it("saves only cleanup fields with exact duration units and named protection levels", async () => {
  const onSaved = vi.fn()
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>) => {
    expect(action).toBe("native_retention_settings_update")
    const recall = { precheckHistoryTurnLimit: 3, precheckMaxInputTokens: 6000, precheckRecallTopK: 5,
      precheckReviewCandidateLimit: 12, precheckMinReviewScore: 0.75, maxSearchQueries: 16,
    postactionHistoryTurnLimit: 3, postactionMaxInputTokens: 6000, ...extractionDefaults }
    const value = { recall, defaults: recall, retrieval: rankingDefaults, retrievalDefaults: rankingDefaults,
      retention: payload as unknown as VmmNativeRetentionSettingsSummary, retentionDefaults, text: textDefaults, textDefaults, profiles: profileDefaults, profileDefaults, userOverride: true }
    return { action, summary: "", kind: "native_runtime_policy", request: payload, response: value,
      data: { kind: "native_runtime_policy", value } } satisfies VmmMemoryActionResult
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRetentionSettings settings={retentionDefaults}
      defaults={retentionDefaults} busy={false} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>) })
    expect(onRunAction).not.toHaveBeenCalled()
    const save = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存清洗设置")!
    expect(save.disabled).toBe(true)
    expect(container.querySelector("#vmm-cleanup-protectedMemoryLevelFloor")!.textContent).toBe("稳定级")
    await changeInput(container, "sessionIdleMs", "2")
    await chooseMemoryOption(container.querySelector<HTMLInputElement>("#vmm-cleanup-sessionIdleMs")!.parentElement!.querySelector<HTMLButtonElement>('[role="combobox"]')!, "小时")
    await changeInput(container, "scanIntervalMinutes", "7")
    await act(async () => { container.querySelector<HTMLButtonElement>('[role="switch"]')!.click() })
    await act(async () => { save.click() })
    expect(onRunAction.mock.calls[0][1]).toEqual({ ...retentionDefaults, enabled: false, scanIntervalMinutes: 7, sessionIdleMs: 7200000 })
    expect(onSaved).toHaveBeenCalledTimes(1)
    expect(container.textContent).toContain("清洗设置已保存")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})

it("keeps drafts on failed saves and unrelated sibling responses", async () => {
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  const onRunAction = vi.fn(async () => null)
  const onSaved = vi.fn()
  // renderSettings simulates sibling updates without inventing a changed persisted cleanup group.
  // renderSettings 模拟相邻配置更新，不虚构已保存清洗组的变化。
  const renderSettings = async (settings: VmmNativeRetentionSettingsSummary) => {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRetentionSettings settings={settings}
      defaults={retentionDefaults} busy={false} onRunAction={onRunAction} onSaved={onSaved} /></I18nProvider>) })
  }
  try {
    await renderSettings(retentionDefaults)
    await changeInput(container, "turnHotWindowSize", "19")
    await renderSettings({ ...retentionDefaults })
    expect(container.querySelector<HTMLInputElement>("#vmm-cleanup-turnHotWindowSize")!.value).toBe("19")
    await act(async () => { Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "保存清洗设置")!.click() })
    expect(onSaved).not.toHaveBeenCalled()
    expect(container.textContent).not.toContain("清洗设置已保存")
    expect(container.querySelector<HTMLInputElement>("#vmm-cleanup-turnHotWindowSize")!.value).toBe("19")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})
