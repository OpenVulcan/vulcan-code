// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRemovalPreview, VmmNativeRemovalTarget } from "@/types"
import { VmmNativeRemoval } from "./vmm-native-removal"

// Returns an exact server preview, including independent profiles that must remain untouched.
// 返回准确服务端预览，包含必须保留的独立画像。
function previewFor(selection: VmmNativeRemovalTarget): VmmNativeRemovalPreview {
  return {
    previewId: "opaque-server-plan", trashRetentionMs: 172_800_000,
    impact: {
      query: { vmmProjectId: 7, selection }, storageVersion: 19, hotWindowSize: 5,
      sessionSourceIds: ["session-stable"], turnIds: ["vmm-turn-2", "vmm-turn-10"],
      pendingTurnIds: ["vmm-turn-10"], hotWindowTurnIds: ["vmm-turn-10"],
      memoryIds: ["vmm-memory-8"], vectorMemoryIds: ["vmm-memory-8"],
      projectProfileNodeIds: ["project-profile"], userProfileNodeIds: ["personal-profile"],
      contextEvidence: [{ memoryId: "vmm-memory-8", contextKey: "topic", contextValue: "food" }],
    },
  }
}

// Mounts real controls with a fake transport; callers explicitly dispose the DOM after each test.
// 使用模拟传输挂载真实控件；调用者在每次测试后显式释放 DOM。
async function mount(selection: VmmNativeRemovalTarget, failed = false, busy = false) {
  // The transport captures exact payloads rather than implementing native storage behavior.
  // 传输记录准确载荷，不冒充原生存储行为。
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    if (action === "native_removal_preview") {
      const value = previewFor(selection)
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_removal_preview", value } }
    }
    if (failed) return null
    const value = { storageVersion: 20, batchId: "manual:opaque-server-plan", impact: previewFor(selection).impact }
    return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_removal", value } }
  })
  // Refresh and occupancy calls are separately observable from successful confirmation.
  // 刷新及占用调用独立于成功确认进行观察。
  const onRefresh = vi.fn(async () => {})
  const onBusyChange = vi.fn()
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  // Renders changed selections in the same component instance to verify preview invalidation.
  // 在同一组件实例中渲染变化后的选择，以验证预览失效。
  const render = async (projectId = 7, current = selection) => {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRemoval projectId={projectId} selection={current} sessionNames={new Map([["session-stable", "可修改的标题"]])} busy={busy} onBusyChange={onBusyChange} onRunAction={onRunAction} onRefresh={onRefresh} /></I18nProvider>))
  }
  await render()
  return { container, onRunAction, onRefresh, onBusyChange, render, dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

// Finds a visible consent control by its user-facing name, not by implementation order.
// 根据用户可见名称查找同意控件，而非依赖实现顺序。
function dialogButton(text: string): HTMLButtonElement {
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((item) => item.textContent === text)
  if (!button) throw new Error(`Missing dialog button: ${text}`)
  return button
}

it.each(["sessions", "turns"] as const)("previews %s and confirms only the displayed opaque identity", async (kind) => {
  // Session IDs and canonical numeric turn IDs deliberately differ from the display title.
  // 会话 ID 及规范数字回合 ID 刻意与展示标题不同。
  const selection = { kind, ids: kind === "sessions" ? ["session-stable"] : ["vmm-turn-2", "vmm-turn-10"] }
  const view = await mount(selection)
  try {
    expect(view.onRunAction).not.toHaveBeenCalled()
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(view.onRunAction).toHaveBeenCalledExactlyOnceWith("native_removal_preview", { vmmProjectId: 7, selection })
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain("保留全部用户画像和项目画像")
    expect(dialog.textContent).toContain("不删除 VulcanCode 聊天会话")
    expect(dialog.textContent).toContain("可修改的标题")
    expect(dialog.textContent).toContain("vmm-turn-10")
    expect(dialog.textContent).toContain("从确认执行时起保留 2")
    expect(dialog.textContent).toContain(kind === "sessions" ? "暂停其记忆处理" : "不改变记忆会话的启用状态")
    expect(dialog.querySelectorAll("dd")).toHaveLength(10)
    await act(async () => dialogButton("确认移入回收站").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_removal_recycle", { previewId: "opaque-server-plan", confirmed: true })
    expect(view.onRefresh).toHaveBeenCalledTimes(1)
    expect(view.onBusyChange.mock.calls).toEqual([[true], [false], [true], [false]])
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
  } finally { await view.dispose() }
})

it("cancels without moving records", async () => {
  const view = await mount({ kind: "sessions", ids: ["session-stable"] })
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    await act(async () => dialogButton("取消").click())
    expect(view.onRunAction).toHaveBeenCalledTimes(1)
    expect(view.onRefresh).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})

it("closes failed confirmation and requires a fresh preview without claiming success", async () => {
  const view = await mount({ kind: "turns", ids: ["vmm-turn-2"] }, true)
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    await act(async () => dialogButton("确认移入回收站").click())
    expect(view.onRefresh).not.toHaveBeenCalled()
    expect(view.container.querySelector('[role="alert"]')?.textContent).toContain("重新预览")
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_removal_preview", { vmmProjectId: 7, selection: { kind: "turns", ids: ["vmm-turn-2"] } })
  } finally { await view.dispose() }
})

it("invalidates an old preview when the owner changes away and back", async () => {
  const view = await mount({ kind: "sessions", ids: ["session-stable"] })
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    await view.render(8)
    await view.render(7)
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    expect(view.onRunAction).toHaveBeenCalledTimes(1)
  } finally { await view.dispose() }
})

it.each([0, 101])("does not preview an invalid selection of %i records", async (count) => {
  const view = await mount({ kind: "sessions", ids: Array.from({ length: count }, (_, index) => `session-${index}`) })
  try {
    expect(view.container.querySelector<HTMLButtonElement>("button")!.disabled).toBe(true)
    expect(view.onRunAction).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})

it("respects transport occupancy", async () => {
  const view = await mount({ kind: "sessions", ids: ["session-stable"] }, false, true)
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(view.onRunAction).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})

it("ignores a delayed preview after the selected owner changes", async () => {
  const view = await mount({ kind: "sessions", ids: ["session-stable"] })
  // A deferred transport result models a project change during the host read.
  // 延迟传输结果模拟宿主读取期间项目发生变化。
  let resolve!: (result: VmmMemoryActionResult | null) => void
  view.onRunAction.mockImplementationOnce(() => new Promise((done) => { resolve = done }))
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    await view.render(8)
    const value = previewFor({ kind: "sessions", ids: ["session-stable"] })
    await act(async () => resolve({ action: "native_removal_preview", kind: "native_removal_preview", summary: "", request: {}, response: value, data: { kind: "native_removal_preview", value } }))
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    expect(view.onRefresh).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})
