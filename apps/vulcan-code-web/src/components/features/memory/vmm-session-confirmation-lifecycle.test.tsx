// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeMemorySessionSummary } from "@/types"
import { VmmNativeSessionArchive } from "./vmm-native-session-archive"
import { VmmNativeSessionRestart } from "./vmm-native-session-restart"

// Verifies late prepared confirmations cannot reopen after the exact project selection has changed.
// 验证准确项目选择改变后，迟到的预备确认不能再次打开。
it.each(["archive", "restart"] as const)("invalidates late %s preview and excludes duplicate clicks", async (kind) => {
  // sessions provide exact lifecycle eligibility without any persisted user records.
  // sessions 提供准确生命周期准入条件，不使用任何持久用户记录。
  const sessions: VmmNativeMemorySessionSummary[] = [{ sessionSourceId: "synthetic-session", displayTitle: "合成会话", status: kind === "archive" ? "active" : "forgotten", turnCount: 0, turnStatuses: { pending: 0, completed: 0, processedUnknown: 0, analysisBypassed: 0, corrupted: 0 }, memoryCount: 0, directMemoryCount: 0, profileNodeCount: 0, memoryLevels: { session: 0, phase: 0, stable: 0, persistent: 0 } }]
  let complete!: (response: VmmMemoryActionResult) => void
  const onRunAction = vi.fn(() => new Promise<VmmMemoryActionResult>((resolve) => { complete = resolve }))
  const onRefresh = vi.fn(async () => {})
  const onBusyChange = vi.fn()
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  const Component = kind === "archive" ? VmmNativeSessionArchive : VmmNativeSessionRestart
  // render preserves the same component while switching only its authoritative owner.
  // render 保留同一组件，仅切换其权威归属。
  const render = (projectId: number) => root.render(<I18nProvider initialLocale="zh-CN"><Component projectId={projectId} sessions={sessions} busy={false} onBusyChange={onBusyChange} onRunAction={onRunAction} onRefresh={onRefresh} /></I18nProvider>)
  try {
    await act(async () => render(7))
    const button = container.querySelector<HTMLButtonElement>("button")!
    await act(async () => { button.click(); button.click() })
    expect(onRunAction).toHaveBeenCalledTimes(1)
    await act(async () => render(8))
    const selection = { vmmProjectId: 7, sessionSourceIds: ["synthetic-session"] }
    const data = kind === "archive"
      ? { kind: "native_session_archive_preview" as const, value: { previewId: "stale", storageVersion: 1, selection: { ...selection, archived: true } } }
      : { kind: "native_session_restart_preview" as const, value: { previewId: "stale", storageVersion: 1, selection } }
    await act(async () => complete({ action: `native_session_${kind}_preview`, kind, summary: "", request: {}, response: data.value, data }))
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    expect(onRefresh).not.toHaveBeenCalled()
    expect(onBusyChange.mock.lastCall).toEqual([false])
  } finally { await act(async () => root.unmount()); container.remove() }
})
