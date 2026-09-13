// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench"
import type { VmmMemoryActionResult } from "@/types"
import { VmmDataPage } from "./vmm-data-page"

// management wraps exact management responses without a live server or persisted configuration.
// management 封装准确管理响应，不使用实际服务器或持久配置。
function management(action: string, value: unknown): VmmMemoryActionResult {
  return { action, kind: action, summary: "", request: {}, response: value, data: { kind: "management", value } }
}

// A declined exclusive call must preserve the preview and must not start success refreshes.
// 被拒绝的独占调用必须保留预览，不得开始成功刷新。
it("retains the external preview when execution has no acknowledgement", async () => {
  // snapshot only enables the isolated management workspace loading path.
  // snapshot 仅启用隔离管理工作区的加载路径。
  const snapshot = browserFallbackSnapshot().vmmMemory
  snapshot.runtime.processReady = true
  snapshot.runtime.postactionReady = true
  // onRunAction provides the real management envelopes and refuses the mutation explicitly.
  // onRunAction 提供真实管理信封，并显式拒绝变更。
  const onRunAction = vi.fn(async (action: string): Promise<VmmMemoryActionResult | null> => {
    if (action === "management_capabilities") return management(action, { protocol_version: 1, service_version: "fixture", storage_mode: "fixture", max_page_size: 30, content_chunk_bytes: 65536, restore_supported: true, full_text_index_status: "ready" })
    if (action === "management_list_users" || action === "management_list_projects") return management(action, { items: [] })
    if (action === "management_list_sessions") return management(action, { items: [{ session_id: "fixture-session", display_title: "隔离会话", user: { id: "1", name: "用户" }, project: { id: "2", name: "项目", path: "/fixture" }, status: "active", created_at: "2026-09-12T00:00:00Z", updated_at: "2026-09-12T00:00:00Z", historical_turn_count: 1, visible_turn_count: 1, pending_turn_count: 0, active_memory_count: 1, profile_reference_count: 0 }], has_more: false })
    if (action === "management_create_removal_preview") return management(action, { preview_token: "private-preview", action: "archive", session_count: 1, turn_count: 1, memory_count: 1, vector_count: 1, profile_node_count: 0, pending_turn_count: 0, hot_window_turn_count: 0, protected_count: 0 })
    if (action === "management_execute_removal") return null
    throw new Error(`Unexpected management action: ${action}`)
  })
  // container hosts the workspace while Radix owns its normal document portal.
  // container 承载工作区，Radix 使用正常文档门户。
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmDataPage snapshot={snapshot} busy={false} result={null} onRunAction={onRunAction} /></I18nProvider>))
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmDataPage snapshot={snapshot} busy result={null} onRunAction={onRunAction} /></I18nProvider>))
    expect([...container.querySelectorAll<HTMLButtonElement>('[role="tab"]')].every((tab) => tab.disabled)).toBe(true)
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmDataPage snapshot={snapshot} busy={false} result={null} onRunAction={onRunAction} /></I18nProvider>))
    await act(async () => container.querySelector<HTMLButtonElement>('[role="checkbox"]')!.click())
    await act(async () => [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "归档")!.click())
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    // readsBefore distinguishes a preserved preview from an unacknowledged success refresh.
    // readsBefore 区分保留预览与未经确认的成功刷新。
    const readsBefore = onRunAction.mock.calls.filter(([action]) => action === "management_list_sessions").length
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "确认执行")!.click())
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    expect(onRunAction.mock.calls.filter(([action]) => action === "management_list_sessions")).toHaveLength(readsBefore)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
