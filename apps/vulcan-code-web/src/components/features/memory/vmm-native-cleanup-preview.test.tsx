// @vitest-environment jsdom

import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRetentionPreview } from "@/types"
import { VmmNativeCleanupPreview } from "./vmm-native-cleanup-preview"
import { VmmNativeMaintenancePage } from "./vmm-native-maintenance-page"

// The fixture contains reversible records, personal-profile expiry and irreversible batch deletion.
// 夹具包含可恢复记录、个人画像过期与不可逆批次删除。
const preview: VmmNativeRetentionPreview = {
  previewId: "server-cleanup-1", storageVersion: 7, evaluatedAtUnixMs: 1000, expiresAtUnixMs: 301000,
  projects: [{ projectId: 1, projectName: "我的记忆空间", recycledMemoryIds: ["memory-1", "memory-2"],
    recycledTurnIds: ["turn-1"], expiredProfileNodeIds: ["user-profile-1"], purgedBatchIds: ["expired-batch-1"] }],
}

it("shows exact impact without executing on render and requires deliberate confirmation", async () => {
  const onConfirm = vi.fn()
  const onClose = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeCleanupPreview preview={preview}
      busy={false} onConfirm={onConfirm} onClose={onClose} /></I18nProvider>))
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain("我的记忆空间")
    expect(dialog.textContent).toContain("回收记忆 2 条、回合 1 个；过期画像 1 条；永久清除批次 1 个")
    expect(dialog.textContent).toContain("不受回收站筛选影响")
    expect(dialog.textContent).toContain("永久清除")
    expect(dialog.textContent).toContain("memory-1, memory-2")
    expect(dialog.textContent).toContain("user-profile-1")
    expect(onConfirm).not.toHaveBeenCalled()
    const confirm = Array.from(dialog.querySelectorAll("button")).find((button) => button.textContent === "立即清洗")!
    await act(async () => confirm.click())
    expect(onConfirm).toHaveBeenCalledTimes(1)
    expect(onClose).not.toHaveBeenCalled()
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeCleanupPreview preview={preview}
      busy={true} onConfirm={onConfirm} onClose={onClose} /></I18nProvider>))
    expect(confirm.disabled).toBe(true)
    await act(async () => confirm.click())
    expect(onConfirm).toHaveBeenCalledTimes(1)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it("cancels without granting execution", async () => {
  const onConfirm = vi.fn()
  const onClose = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeCleanupPreview preview={preview}
      busy={false} onConfirm={onConfirm} onClose={onClose} /></I18nProvider>))
    const cancel = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "取消")!
    await act(async () => cancel.click())
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(onConfirm).not.toHaveBeenCalled()
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it("routes the page through preview before confirmation and discards a rejected plan", async () => {
  // Unavailable ancillary status is deliberate: the assertion concerns only the manual action chain.
  // 附属状态故意设为不可用，本断言只验证人工动作链。
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    if (action !== "native_retention_preview") return null
    return { action, summary: "", kind: "native_retention_preview", request: payload, response: preview,
      data: { kind: "native_retention_preview", value: preview } }
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeMaintenancePage busy={false} onRunAction={onRunAction} /></I18nProvider>))
    const previewButton = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "预览清洗")!
    expect(onRunAction.mock.calls.some(([action]) => action === "native_retention_run")).toBe(false)
    await act(async () => previewButton.click())
    expect(onRunAction.mock.calls.some(([action]) => action === "native_retention_run")).toBe(false)
    const confirm = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "立即清洗")!
    await act(async () => confirm.click())
    expect(onRunAction.mock.calls.filter(([action]) => action === "native_retention_run")).toEqual([
      ["native_retention_run", { previewId: "server-cleanup-1", confirmed: true }],
    ])
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    expect(previewButton.disabled).toBe(false)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
