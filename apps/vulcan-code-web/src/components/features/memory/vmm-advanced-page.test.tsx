// @vitest-environment jsdom

import { act, StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult } from "@/types"
import { NativeFixedProjectSelect } from "./vmm-advanced-page"

// catalogReply returns a typed native directory for the supplied project name and stable ID 7.
// catalogReply 按传入项目名称返回强类型原生目录，稳定标识为 7。
function catalogReply(name: string): VmmMemoryActionResult {
  return { action: "native_project_catalog", summary: "", kind: "native_project_catalog", request: {}, response: {},
    data: { kind: "native_project_catalog", value: { projects: [{ projectId: 7, name, tag: "test-tag",
      protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }], bindings: [] } } }
}

// mountPicker renders the real picker in StrictMode and returns scoped rerender/cleanup helpers.
// mountPicker 在 StrictMode 中渲染真实选择器，并返回作用域内的重渲染与清理方法。
async function mountPicker(onRunAction: (action: string) => Promise<VmmMemoryActionResult | null>, disabled = false) {
  // DOM and root belong only to this test; selection changes remain observable callbacks.
  // DOM 与根节点仅属于当前测试；选择修改通过回调进行观察。
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  const onChange = vi.fn()
  // render changes only shared occupancy and preserves the picker lifetime.
  // render 仅改变共享占用状态，保留选择器生命周期。
  const render = async (busy: boolean) => {
    await act(async () => { root.render(<StrictMode><I18nProvider initialLocale="zh-CN">
      <NativeFixedProjectSelect disabled={busy} label="固定记忆项目" onChange={onChange} onRunAction={onRunAction} value="7" />
    </I18nProvider></StrictMode>) })
  }
  await render(disabled)
  return { container, root, onChange, render,
    // cleanup unmounts the real component before removing its DOM.
    // cleanup 先卸载真实组件，再移除其 DOM。
    cleanup: async () => { await act(async () => root.unmount()); container.remove() } }
}

// Busy entry must not consume the only automatic read or clear the saved selection.
// 忙碌时进入不能消耗唯一自动读取机会，也不能清除已保存选择。
it("waits for the shared channel and retains the selected native identity", async () => {
  const run = vi.fn(async () => catalogReply("已配置项目"))
  const view = await mountPicker(run, true)
  try {
    expect(run).not.toHaveBeenCalled()
    await view.render(false)
    expect(run.mock.calls).toHaveLength(1)
    expect(view.container.querySelector('[role="combobox"]')?.textContent).toContain("已配置项目")
    expect(view.container.querySelector('[role="combobox"]')?.textContent).toContain("test-tag")
    await view.render(true)
    await view.render(false)
    expect(run.mock.calls).toHaveLength(1)
    expect(view.onChange).not.toHaveBeenCalled()
  } finally { await view.cleanup() }
})

// StrictMode and busy updates must not cancel the one pending directory response.
// StrictMode 与忙闲更新不能取消唯一进行中的目录响应。
it("publishes the pending directory after StrictMode replay and busy transitions", async () => {
  let resolve!: (reply: VmmMemoryActionResult) => void
  const pending = new Promise<VmmMemoryActionResult>((done) => { resolve = done })
  const run = vi.fn(() => pending)
  const view = await mountPicker(run)
  try {
    expect(run).toHaveBeenCalledTimes(1)
    await view.render(true)
    await act(async () => resolve(catalogReply("等待后出现的项目")))
    await view.render(false)
    expect(view.container.querySelector('[role="combobox"]')?.textContent).toContain("等待后出现的项目")
    expect(run).toHaveBeenCalledTimes(1)
    expect(view.onChange).not.toHaveBeenCalled()
  } finally { await view.cleanup() }
})

// Rejected/null responses remain visible failures and retry only on an explicit user action.
// 拒绝或空响应保持可见失败，仅在用户显式操作时重试。
it.each(["null", "rejection"])("offers a bounded retry after %s without looping", async (failure) => {
  const run = vi.fn<(action: string) => Promise<VmmMemoryActionResult | null>>()
  if (failure === "null") run.mockResolvedValueOnce(null)
  else run.mockRejectedValueOnce(new Error("directory unavailable"))
  run.mockResolvedValue(catalogReply("重试恢复项目"))
  const view = await mountPicker(run)
  try {
    expect(view.container.querySelector('[role="alert"]')?.textContent).toContain("目录加载失败")
    await view.render(true)
    await view.render(false)
    expect(run).toHaveBeenCalledTimes(1)
    const retry = Array.from(view.container.querySelectorAll("button")).find((button) => button.textContent === "重新加载目录")!
    await act(async () => { retry.click(); retry.click() })
    expect(run).toHaveBeenCalledTimes(2)
    expect(view.container.querySelector('[role="alert"]')).toBeNull()
    expect(view.container.querySelector('[role="combobox"]')?.textContent).toContain("重试恢复项目")
    expect(view.onChange).not.toHaveBeenCalled()
  } finally { await view.cleanup() }
})

// Leaving the native branch discards its reply even after another picker is mounted.
// 离开原生分支后丢弃其响应，即使另一选择器已经挂载。
it("does not publish a late directory into a replacement picker", async () => {
  let resolve!: (reply: VmmMemoryActionResult) => void
  const pending = new Promise<VmmMemoryActionResult>((done) => { resolve = done })
  const oldView = await mountPicker(() => pending)
  await oldView.cleanup()
  const nextView = await mountPicker(async () => catalogReply("当前项目"))
  try {
    await act(async () => resolve(catalogReply("旧项目")))
    expect(nextView.container.querySelector('[role="combobox"]')?.textContent).toContain("当前项目")
    expect(nextView.container.textContent).not.toContain("旧项目")
    expect(nextView.onChange).not.toHaveBeenCalled()
  } finally { await nextView.cleanup() }
})
