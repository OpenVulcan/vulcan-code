// @vitest-environment jsdom
import { act, StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult } from "@/types"
import { VmmNativeMaintenancePage } from "./vmm-native-maintenance-page"
import { maintenanceBatch, maintenanceReply } from "./vmm-native-maintenance.test-fixture"

// The test environment owns React's act flag and restores the original global afterwards.
// 测试环境拥有 React act 标记，结束后恢复原全局值。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

/** Mounts the full page with one explicit synthetic channel and returns disposal.
 * 使用一个明确合成通道挂载完整页面，并返回释放入口。
 * run supplies typed responses; returns the container and async cleanup function.
 * run 提供强类型响应；返回容器及异步清理函数。
 */
async function mount(run: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>) {
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  await act(async () => root.render(<StrictMode><I18nProvider initialLocale="zh-CN"><VmmNativeMaintenancePage busy={false} onRunAction={run} /></I18nProvider></StrictMode>))
  return { container, dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

/** Resolves an exact visible button in the supplied scope without relying on source order.
 * 在给定范围准确定位可见按钮，不依赖源码顺序。
 * label is its displayed text; scope limits the owner. Returns the actual button.
 * label 为显示文本；scope 限制归属。返回实际按钮。
 */
function button(label: string, scope: ParentNode = document) {
  const result = [...scope.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent === label && !entry.closest("[hidden]"))
  if (!result) throw new Error(`Missing visible button: ${label}`)
  return result
}

it("starts one StrictMode sequence and stops after an unmounted in-flight read", async () => {
  // resolveStatus controls only the first real page read; no later endpoint may be called after disposal.
  // resolveStatus 仅控制实际页面的首个读取；释放后不得调用后续端点。
  let resolveStatus!: (value: VmmMemoryActionResult) => void
  const run = vi.fn(() => new Promise<VmmMemoryActionResult>((resolve) => { resolveStatus = resolve }))
  const view = await mount(run)
  try {
    expect(run).toHaveBeenCalledExactlyOnceWith("native_retention_status")
    expect(button("刷新资源", view.container).disabled).toBe(true)
  } finally { await view.dispose() }
  await act(async () => resolveStatus(maintenanceReply("native_retention_status")))
  expect(run).toHaveBeenCalledTimes(1)
})

it("renders exact short durations and distinguishes failed reads from loading", async () => {
  const run = vi.fn(async (action: string) => action === "native_vector_status" ? null : maintenanceReply(action))
  const view = await mount(run)
  try {
    expect(run.mock.calls.map(([action]) => action)).toEqual(["native_retention_status", "native_lexical_status", "native_vector_status", "native_project_catalog", "native_recycle_bin"])
    await act(async () => button("清洗运行状态", view.container).click())
    expect(view.container.textContent).toContain("1 分钟")
    expect(view.container.textContent).toContain("86,400,001 毫秒")
    await act(async () => button("向量与检索索引").click())
    const vector = [...view.container.querySelectorAll('[data-slot="card"]')].find((entry) => entry.textContent?.includes("向量代次与重建"))!
    expect(vector.textContent).not.toContain("正在加载")
    expect(vector.textContent).toContain("返回了不匹配的结果")
  } finally { await view.dispose() }
})

it("does not leave a completed missing lexical status in an endless loading state", async () => {
  const view = await mount(async () => null)
  try {
    await act(async () => button("向量与检索索引", view.container).click())
    expect(view.container.textContent).toContain("读取失败，请刷新重试。")
    expect(view.container.textContent).not.toContain("正在加载原生资源")
    expect(button("重建全文索引", view.container).disabled).toBe(true)
  } finally { await view.dispose() }
})

it.each(["恢复", "永久清除"])("shows the exact batch before %s and returns focus on cancellation", async (label) => {
  const run = vi.fn(async (action: string, payload?: Record<string, unknown>) => maintenanceReply(action, payload))
  const view = await mount(run)
  try {
    const trigger = button(label, view.container)
    await act(async () => { trigger.focus(); trigger.click() })
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain(maintenanceBatch.batchId)
    expect(dialog.textContent).toContain("同名合成项目")
    await act(async () => button("取消", dialog).click())
    await vi.waitFor(() => expect(document.activeElement).toBe(trigger))
    expect(run.mock.calls.some(([action]) => action === "native_recycle_restore" || action === "native_recycle_purge")).toBe(false)
  } finally { await view.dispose() }
})

it.each(["恢复", "永久清除"])("locks %s during submission and keeps the original observed version", async (label) => {
  // resolveAction holds the exact confirmation to test double invocation and unavailable parent busy state.
  // resolveAction 持有准确确认，以验证重复调用及父级忙碌状态尚不可用的情况。
  let resolveAction!: (value: VmmMemoryActionResult | null) => void
  const actionName = label === "恢复" ? "native_recycle_restore" : "native_recycle_purge"
  const run = vi.fn(async (action: string, payload?: Record<string, unknown>) => action === actionName
    ? new Promise<VmmMemoryActionResult | null>((resolve) => { resolveAction = resolve }) : maintenanceReply(action, payload))
  const view = await mount(run)
  try {
    await act(async () => button(label, view.container).click())
    const dialog = document.querySelector('[role="alertdialog"]')!
    await act(async () => { button(label, dialog).click(); button(label, dialog).click() })
    expect(run.mock.calls.filter(([action]) => action === actionName)).toEqual([[actionName, { batchId: maintenanceBatch.batchId, storageVersion: 19 }]])
    expect(button("取消", dialog).disabled).toBe(true)
    await act(async () => resolveAction(null))
    expect(dialog.querySelector('[role="alert"]')).not.toBeNull()
    expect(button("取消", dialog).disabled).toBe(false)
    await act(async () => button("取消", dialog).click())
  } finally { await view.dispose() }
})

it("keeps same-dimension vectors through the exact rebuild precheck and closes the confirmation", async () => {
  const run = vi.fn(async (action: string, payload?: Record<string, unknown>) => maintenanceReply(action, payload))
  const view = await mount(run)
  try {
    await act(async () => button("向量与检索索引").click())
    await act(async () => button("预检向量重建", view.container).click())
    const dialog = document.querySelector('[role="alertdialog"]')!
    await act(async () => button("沿用现有向量", dialog).click())
    expect(run).toHaveBeenCalledWith("native_vector_adoption", { expectedStorageVersion: 19, expectedPreviewFingerprint: "audit-fingerprint", confirmed: true })
    await vi.waitFor(() => expect(document.querySelector('[role="alertdialog"]')).toBeNull())
    expect(view.container.querySelector('[role="status"]')).not.toBeNull()
  } finally { await view.dispose() }
})

it("keeps the rebuild confirmation open when the sample shows a different model", async () => {
  const run = vi.fn(async (action: string, payload?: Record<string, unknown>) => maintenanceReply(action, action === "native_vector_adoption" ? { ...payload, expectedPreviewFingerprint: "different-model" } : payload))
  const view = await mount(run)
  try {
    await act(async () => button("向量与检索索引").click())
    await act(async () => button("预检向量重建", view.container).click())
    const dialog = document.querySelector('[role="alertdialog"]')!
    await act(async () => button("沿用现有向量", dialog).click())
    expect(document.querySelector('[role="alertdialog"]')).toBe(dialog)
    expect(dialog.querySelector('[role="alert"]')).not.toBeNull()
    expect(run).not.toHaveBeenCalledWith("native_vector_rebuild", expect.anything())
  } finally { await view.dispose() }
})

it("keeps a vector rejection inside its confirmation and restores the exact trigger", async () => {
  const run = vi.fn(async (action: string, payload?: Record<string, unknown>) => action === "native_vector_rebuild" ? null : maintenanceReply(action, payload))
  const view = await mount(run)
  try {
    await act(async () => button("向量与检索索引").click())
    const trigger = button("预检向量重建", view.container)
    await act(async () => { trigger.focus(); trigger.click() })
    const dialog = document.querySelector('[role="alertdialog"]')!
    await act(async () => button("确认重建", dialog).click())
    expect(run).toHaveBeenLastCalledWith("native_vector_rebuild", { expectedStorageVersion: 19, expectedPreviewFingerprint: "audit-fingerprint", confirmed: true })
    expect(dialog.querySelector('[role="alert"]')).not.toBeNull()
    await act(async () => button("取消", dialog).click())
    await vi.waitFor(() => expect(document.activeElement).toBe(trigger))
  } finally { await view.dispose() }
})
