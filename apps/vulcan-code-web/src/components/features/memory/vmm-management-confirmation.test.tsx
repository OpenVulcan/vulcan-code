// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { ensureLocaleCatalog } from "@/i18n-catalogs"
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

// typePurgePhrase enters one phrase into the controlled purge confirmation input.
// typePurgePhrase 向受控的永久删除确认输入框填入一个短语。
async function typePurgePhrase(value: string) {
  const input = document.querySelector<HTMLInputElement>("#vmm-purge-confirmation")!
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value)
    input.dispatchEvent(new Event("input", { bubbles: true }))
  })
}

// The purge gesture is typed in the UI language, while the service still receives its own confirmation token.
// 永久删除手势以界面语言输入，而服务仍收到它自己签发的确认令牌。
it.each<[string, string | undefined]>([
  ["issues a confirmation token", "fixture-service-token"],
  ["omits the confirmation token", undefined],
])("localizes the purge phrase when the service %s", async (_case, requiredConfirmation) => {
  const locale = "de-DE"
  const catalog = await ensureLocaleCatalog(locale)
  const snapshot = browserFallbackSnapshot().vmmMemory
  snapshot.runtime.processReady = true
  snapshot.runtime.postactionReady = true
  // onRunAction serves one recycled batch and records the purge request.
  // onRunAction 提供一个已回收批次并记录永久删除请求。
  const onRunAction = vi.fn(async (action: string, _payload?: unknown): Promise<VmmMemoryActionResult | null> => {
    if (action === "management_capabilities") return management(action, { protocol_version: 1, service_version: "fixture", storage_mode: "fixture", max_page_size: 30, content_chunk_bytes: 65536, restore_supported: true, full_text_index_status: "ready" })
    if (action === "management_list_users" || action === "management_list_projects" || action === "management_list_sessions") return management(action, { items: [], has_more: false })
    if (action === "management_list_recycle_batches") return management(action, { items: [{ batch_id: "fixture-batch", source: "human", restorable: true, state: "recycled", reason: "", session_count: 1, turn_count: 1, memory_count: 1, profile_count: 0, recycled_at: "2026-09-12T00:00:00Z" }], has_more: false })
    if (action === "management_create_removal_preview") return management(action, { preview_token: "private-preview", action: "purge", session_count: 1, turn_count: 1, memory_count: 1, vector_count: 1, profile_node_count: 0, pending_turn_count: 0, hot_window_turn_count: 0, protected_count: 0, required_confirmation: requiredConfirmation })
    if (action === "management_purge") return management(action, {})
    throw new Error(`Unexpected management action: ${action}`)
  })
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale={locale}><VmmDataPage snapshot={snapshot} busy={false} result={null} onRunAction={onRunAction} /></I18nProvider>))
    const recycleTab = [...container.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((tab) => tab.textContent === catalog["memory.data.recycleBin"])!
    await act(async () => recycleTab.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 })))
    await vi.waitFor(() => expect([...container.querySelectorAll("button")].some((button) => button.textContent === catalog["memory.data.purge"])).toBe(true))
    await act(async () => [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === catalog["memory.data.purge"])!.click())
    const phrase = catalog["memory.data.purgeConfirmationPhrase"]
    expect(document.querySelector('[role="dialog"]')!.textContent).toContain(phrase)
    const confirm = () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === catalog["memory.data.confirmAction"])!
    // Typing the service token instead of the localized phrase does not unlock the purge.
    // 输入服务令牌而非本地化短语不会解锁永久删除。
    await typePurgePhrase(requiredConfirmation ?? "")
    expect(confirm().disabled).toBe(true)
    await typePurgePhrase(phrase)
    if (requiredConfirmation === undefined) {
      // Without a service-issued token there is nothing valid to send, so the purge stays locked.
      // 服务未签发令牌时没有可发送的有效值，永久删除保持锁定。
      expect(confirm().disabled).toBe(true)
      return
    }
    expect(confirm().disabled).toBe(false)
    await act(async () => confirm().click())
    expect(onRunAction).toHaveBeenCalledWith("management_purge", { previewToken: "private-preview", confirmText: requiredConfirmation })
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
