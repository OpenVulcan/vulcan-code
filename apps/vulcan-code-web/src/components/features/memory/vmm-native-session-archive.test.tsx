// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeMemorySessionSummary } from "@/types"
import { VmmNativeSessionArchive } from "./vmm-native-session-archive"

// A title deliberately differs from identity to catch accidental name-based binding.
// 标题刻意与身份不同，以发现意外的按名称绑定。
const session: VmmNativeMemorySessionSummary = {
  sessionSourceId: "exact-session", displayTitle: "可改名标题", status: "active",
  turnCount: 1, turnStatuses: { pending: 0, completed: 1, processedUnknown: 0, analysisBypassed: 0, corrupted: 0 },
  memoryCount: 1, profileNodeCount: 1, directMemoryCount: 0,
  memoryLevels: { session: 1, phase: 0, stable: 0, persistent: 0 },
}

// Mounts the actual dialog with controlled host responses and always exposes explicit disposal.
// 使用受控宿主响应挂载实际对话框，并始终提供显式释放入口。
async function mount(status: VmmNativeMemorySessionSummary["status"], failed = false, busy = false) {
  const onRefresh = vi.fn(async () => {})
  const onBusyChange = vi.fn()
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    // Preview is successful even when execution is rejected, modeling a stale storage version.
    // 即使执行被拒绝预览仍成功，以模拟存储版本过期。
    const selection = { vmmProjectId: 7, sessionSourceIds: [session.sessionSourceId], archived: status === "active" }
    if (action === "native_session_restart_preview") {
      const value = { previewId: "restart-preview", storageVersion: 23, selection: { vmmProjectId: 7, sessionSourceIds: [session.sessionSourceId] } }
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_session_restart_preview", value } }
    }
    if (action === "native_session_restart") {
      if (failed) return null
      const value = { storageVersion: 24, selection: { vmmProjectId: 7, sessionSourceIds: [session.sessionSourceId] } }
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_session_restart", value } }
    }
    if (action === "native_session_archive_preview") {
      const value = { previewId: "server-preview", storageVersion: 23, selection }
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_session_archive_preview", value } }
    }
    if (failed) return null
    const value = { storageVersion: 24, selection }
    return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_session_archive", value } }
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeSessionArchive projectId={7} sessions={[{ ...session, status }]} busy={busy} onBusyChange={onBusyChange} onRunAction={onRunAction} onRefresh={onRefresh} /></I18nProvider>))
  return { container, onRunAction, onRefresh, onBusyChange, dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

it.each(["active", "archived"] as const)("previews and confirms the exact %s session without submitting a replacement selection", async (status) => {
  const view = await mount(status)
  try {
    expect(view.onRunAction).not.toHaveBeenCalled()
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(view.onRunAction).toHaveBeenCalledExactlyOnceWith("native_session_archive_preview", { vmmProjectId: 7, sessionSourceIds: ["exact-session"], archived: status === "active" })
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain("不删除记忆、画像或向量")
    expect(dialog.textContent).toContain("可改名标题")
    expect(dialog.textContent).toContain("exact-session")
    const confirm = Array.from(dialog.querySelectorAll("button")).find((button) => button.textContent === "确认变更")!
    await act(async () => confirm.click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_archive", { previewId: "server-preview", confirmed: true })
    expect(view.onRefresh).toHaveBeenCalledTimes(1)
    expect(view.onBusyChange.mock.calls).toEqual([[true], [false], [true], [false]])
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
  } finally { await view.dispose() }
})

it("requires a new preview after rejected execution without refreshing or claiming success", async () => {
  const view = await mount("active", true)
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    const confirm = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "确认变更")!
    await act(async () => confirm.click())
    expect(view.onRefresh).not.toHaveBeenCalled()
    expect(view.container.querySelector('[role="alert"]')?.textContent).toContain("请重新预览")
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
  } finally { await view.dispose() }
})

it.each([null, "recycled", "forgotten"] as const)("never offers an archive action for %s", async (status) => {
  const view = await mount(status)
  try {
    if (status === "forgotten") expect(view.container.querySelector("button")?.textContent).toBe("重新启用记忆")
    else expect(view.container.querySelector("button")).toBeNull()
    expect(view.onRunAction).not.toHaveBeenCalled()
    if (status === null) expect(view.container.textContent).toContain("尚未建立提取会话")
  } finally { await view.dispose() }
})

it("does not preview while the parent transport is occupied", async () => {
  const view = await mount("active", false, true)
  try {
    const button = view.container.querySelector<HTMLButtonElement>("button")!
    expect(button.disabled).toBe(true)
    await act(async () => button.click())
    expect(view.onRunAction).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})

it("cancels a preview without executing or refreshing records", async () => {
  const view = await mount("active")
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    const cancel = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "取消")!
    await act(async () => cancel.click())
    expect(view.onRunAction).toHaveBeenCalledTimes(1)
    expect(view.onRefresh).not.toHaveBeenCalled()
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
  } finally { await view.dispose() }
})

it.each([false, true])("confirms restart by preview ID without replaying content; failure=%s", async (failed) => {
  const view = await mount("forgotten", failed)
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(view.onRunAction).toHaveBeenCalledExactlyOnceWith("native_session_restart_preview", { vmmProjectId: 7, sessionSourceIds: ["exact-session"] })
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain("不会恢复已删除")
    expect(dialog.textContent).toContain("可改名标题")
    const confirm = Array.from(dialog.querySelectorAll("button")).find((button) => button.textContent === "确认重新启用")!
    await act(async () => confirm.click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_restart", { previewId: "restart-preview", confirmed: true })
    expect(view.onRefresh).toHaveBeenCalledTimes(failed ? 0 : 1)
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    if (failed) expect(view.container.querySelector('[role="alert"]')?.textContent).toContain("请重新预览")
  } finally { await view.dispose() }
})

it("does not preview restart while transport is occupied", async () => {
  const view = await mount("forgotten", false, true)
  try {
    const button = view.container.querySelector<HTMLButtonElement>("button")!
    expect(button.disabled).toBe(true)
    await act(async () => button.click())
    expect(view.onRunAction).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})
