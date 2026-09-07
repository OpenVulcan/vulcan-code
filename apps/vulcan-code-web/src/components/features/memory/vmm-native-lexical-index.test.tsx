// @vitest-environment jsdom

import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeLexicalIndexStatus } from "@/types"
import { VmmNativeLexicalIndex } from "./vmm-native-lexical-index"
import { VmmNativeMaintenancePage } from "./vmm-native-maintenance-page"

// Equal counts deliberately demonstrate that the UI must not claim semantic index integrity.
// 故意使用相等计数，证明界面不得据此宣称索引语义完整。
const status: VmmNativeLexicalIndexStatus = { storageVersion: 7, activeMemoryCount: 3, documentCount: 3, indexedRowCount: 3 }

it("requires confirmation and freezes the displayed version instead of silently using a refreshed version", async () => {
  const onRefresh = vi.fn(async () => true)
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult> => {
    const value = { storageVersion: 7, reindexedMemoryCount: 3 }
    return { action, summary: "", kind: "native_lexical_rebuild", request: payload, response: value,
      data: { kind: "native_lexical_rebuild", value } }
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  // Re-render the actual controlled card as the parent publishes updated status.
  // 父级发布更新状态时重新渲染实际受控卡片。
  const render = async (current: VmmNativeLexicalIndexStatus) => {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeLexicalIndex status={current} busy={false} onRunAction={onRunAction} onRefresh={onRefresh} /></I18nProvider>))
  }
  try {
    await render(status)
    expect(onRunAction).not.toHaveBeenCalled()
    expect(container.textContent).toContain("数量相等不代表分词内容一定完整")
    await act(async () => container.querySelector<HTMLButtonElement>("button")!.click())
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain("3 条活跃记忆")
    expect(dialog.textContent).toContain("原向量、项目绑定与回收站内容均不改变")
    expect(onRunAction).not.toHaveBeenCalled()
    await render({ ...status, storageVersion: 8, activeMemoryCount: 4 })
    const confirm = Array.from(dialog.querySelectorAll("button")).find((button) => button.textContent === "重建全文索引")!
    await act(async () => confirm.click())
    expect(onRunAction).toHaveBeenCalledExactlyOnceWith("native_lexical_rebuild", { expectedStorageVersion: 7, confirmed: true })
    expect(onRefresh).toHaveBeenCalledTimes(1)
    expect(container.textContent).toContain("已重建 3 条全文索引，未生成向量")
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it("disables unavailable state and never refreshes or reports success for a rejected rebuild", async () => {
  const onRunAction = vi.fn(async () => null)
  const onRefresh = vi.fn(async () => true)
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeLexicalIndex status={null} busy={false} onRunAction={onRunAction} onRefresh={onRefresh} /></I18nProvider>))
    expect(container.querySelector<HTMLButtonElement>("button")!.disabled).toBe(true)
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeLexicalIndex status={status} busy={false} onRunAction={onRunAction} onRefresh={onRefresh} /></I18nProvider>))
    await act(async () => container.querySelector<HTMLButtonElement>("button")!.click())
    const confirm = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "重建全文索引")!
    await act(async () => confirm.click())
    expect(onRefresh).not.toHaveBeenCalled()
    expect(container.textContent).not.toContain("已重建")
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

it.each([true, false])("loads independent maintenance capabilities sequentially despite status failures (lexical ready: %s)", async (lexicalReady) => {
  // Retention and vector status are deliberately unavailable; neither may block lexical reads or recycle reads.
  // 故意使清洗和向量状态不可用，两者均不得阻止全文读取或回收站读取。
  const onRunAction = vi.fn(async (action: string): Promise<VmmMemoryActionResult | null> => {
    if (!lexicalReady || action !== "native_lexical_status") return null
    return { action, summary: "", kind: "native_lexical_status", request: {}, response: status,
      data: { kind: "native_lexical_status", value: status } }
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeMaintenancePage busy={false} onRunAction={onRunAction} /></I18nProvider>))
    expect(onRunAction.mock.calls.map(([action]) => action)).toEqual([
      "native_retention_status", "native_lexical_status", "native_vector_status", "native_project_catalog", "native_recycle_bin",
    ])
    const rebuildButton = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "重建全文索引")!
    expect(rebuildButton.disabled).toBe(!lexicalReady)
    expect(onRunAction.mock.calls.some(([action]) => action === "native_lexical_rebuild")).toBe(false)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
