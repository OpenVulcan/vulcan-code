// @vitest-environment jsdom

import { act } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProjectRetentionRules, VmmNativeProjectRetentionSummary, VmmNativeProjectSummary } from "@/types"
import { VmmNativeProjectRetention } from "./vmm-native-project-retention"

// Restore React's testing flag after the isolated DOM suite.
// 在隔离 DOM 测试组结束后恢复 React 测试标志。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

// Fixtures use two same-name projects to prove selection follows stable identity rather than display text.
// 夹具使用两个同名项目，证明选择遵循稳定身份而非显示文本。
const projects: VmmNativeProjectSummary[] = [1, 2].map((projectId) => ({ projectId, name: "同名项目", tag: `project-${projectId}`, protected: projectId === 1, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }))
// Only six allowed project data fields are present; no worker or personal-profile settings are fabricated.
// 仅包含六个允许的项目数据字段，不编造工作器或个人画像设置。
const defaults: VmmNativeProjectRetentionRules = { turnHotWindowSize: 8, sessionIdleMs: 259200000, trashRetentionMs: 604800000, protectedPriorityFloor: 80, protectedMemoryLevelFloor: 3, skipProtectedSharedMemories: true }

// Builds the exact typed response union consumed by the production editor.
// 构建生产编辑器消费的准确强类型响应联合。
function result(projectId: number, overrides: VmmNativeProjectRetentionRules | null = null): VmmMemoryActionResult {
  const value: VmmNativeProjectRetentionSummary = { projectId, overrides, effective: overrides ?? defaults, global: defaults }
  return { action: "native_project_retention", kind: "native_project_retention", summary: "", request: {}, response: value, data: { kind: "native_project_retention", value } }
}

// Mounts the real component and exposes only DOM events and deterministic response transport.
// 挂载真实组件，仅暴露 DOM 事件及确定性响应传输。
async function mount(onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>) {
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeProjectRetention projects={projects} busy={false} onRunAction={onRunAction} /></I18nProvider>))
  return {
    container,
    // Selects one catalog-backed memory-project option through its real browser change event.
    // 通过真实浏览器变更事件选择目录支持的记忆项目选项。
    async choose(id: string) {
      await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#retention-project")!, `同名项目 · project-${id}`)
    },
    // Dispatches actual text input without reaching into component state.
    // 派发实际文本输入，不访问组件内部状态。
    async edit(key: keyof VmmNativeProjectRetentionRules, value: string) {
      const input = container.querySelector<HTMLInputElement>(`#project-retention-${key}`)!
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value)
        input.dispatchEvent(new Event("input", { bubbles: true }))
      })
    },
    // Finds a user-visible operation by its exact localized label.
    // 按准确本地化标签定位用户可见操作。
    button(label: string) { return Array.from(container.querySelectorAll("button")).find((button) => button.textContent === label)! },
    // Unmounts and removes only this test's owned DOM.
    // 仅卸载并移除当前测试持有的 DOM。
    async close() { await act(async () => root.unmount()); container.remove() },
  }
}

// Real editing must submit only the allowed owner group and require an explicit inheritance operation.
// 真实编辑必须仅提交获准归属组，并要求显式继承操作。
it("selects names, saves exact owner fields and explicitly restores current global inheritance", async () => {
  let saved: VmmNativeProjectRetentionRules | null = null
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>) => {
    expect(payload?.projectId).toBe(2)
    if (action === "native_project_retention_update") saved = payload?.mode === "inherit" ? null : payload?.rules as VmmNativeProjectRetentionRules
    return result(2, saved)
  })
  const view = await mount(onRunAction)
  try {
    expect(onRunAction).not.toHaveBeenCalled()
    expect(view.container.querySelector("#retention-project")?.getAttribute("data-placeholder")).not.toBeNull()
    await view.choose("2")
    expect(onRunAction).toHaveBeenCalledExactlyOnceWith("native_project_retention", { projectId: 2 })
    expect(view.container.textContent).toContain("当前继承全局")
    expect(view.container.querySelector("#vmm-cleanup-scanIntervalMinutes")).toBeNull()
    await view.edit("turnHotWindowSize", "0")
    expect(view.button("保存项目覆盖").disabled).toBe(true)
    await view.edit("turnHotWindowSize", "2")
    await view.edit("trashRetentionMs", "2.5")
    await act(async () => view.button("保存项目覆盖").click())
    expect(onRunAction.mock.calls[1]).toEqual(["native_project_retention_update", { mode: "override", projectId: 2, rules: { ...defaults, turnHotWindowSize: 2, trashRetentionMs: 216000000 } }])
    expect(view.container.querySelector('[role="status"]')?.textContent).toContain("已保存")
    await act(async () => view.button("填入当前全局值（尚未保存）").click())
    expect(onRunAction).toHaveBeenCalledTimes(2)
    expect(view.container.querySelector<HTMLInputElement>("#project-retention-turnHotWindowSize")!.value).toBe("8")
    await act(async () => view.button("恢复全局继承").click())
    expect(onRunAction.mock.calls[2]).toEqual(["native_project_retention_update", { mode: "inherit", projectId: 2 }])
    expect(view.container.textContent).toContain("当前继承全局")
    expect(view.button("恢复全局继承").disabled).toBe(true)
  } finally { await view.close() }
})

// A failed save must keep the exact draft and reject a response belonging to another project.
// 保存失败必须保留准确草稿，并拒绝属于另一项目的响应。
it("keeps failed drafts and rejects mismatched project responses", async () => {
  const onRunAction = vi.fn().mockResolvedValueOnce(result(1)).mockResolvedValueOnce(null).mockResolvedValueOnce(result(2))
  const view = await mount(onRunAction)
  try {
    await view.choose("1")
    await view.edit("turnHotWindowSize", "2")
    await act(async () => view.button("保存项目覆盖").click())
    expect(view.container.querySelector('[role="alert"]')).not.toBeNull()
    expect(view.container.querySelector('[role="status"]')).toBeNull()
    expect(view.container.querySelector<HTMLInputElement>("#project-retention-turnHotWindowSize")!.value).toBe("2")
    await act(async () => view.button("保存项目覆盖").click())
    expect(view.container.querySelector<HTMLInputElement>("#project-retention-turnHotWindowSize")!.value).toBe("2")
    expect(view.container.querySelector('[role="status"]')).toBeNull()
  } finally { await view.close() }
})

// A delayed result from the previous keyed editor must never replace the newly selected owner's values.
// 上一个按身份挂载编辑器的迟到结果绝不能替换新选归属方的值。
it("discards late results after selecting another project", async () => {
  let finish!: (value: VmmMemoryActionResult) => void
  const delayed = new Promise<VmmMemoryActionResult>((resolve) => { finish = resolve })
  const onRunAction = vi.fn(async (_action: string, payload?: Record<string, unknown>) => payload?.projectId === 1 ? delayed : result(2, { ...defaults, turnHotWindowSize: 3 }))
  const view = await mount(onRunAction)
  try {
    await view.choose("1")
    await view.choose("2")
    expect(view.container.querySelector<HTMLInputElement>("#project-retention-turnHotWindowSize")!.value).toBe("3")
    await act(async () => finish(result(1, { ...defaults, turnHotWindowSize: 19 })))
    expect(view.container.querySelector<HTMLInputElement>("#project-retention-turnHotWindowSize")!.value).toBe("3")
    expect(view.container.querySelector("#retention-project")!.textContent).toBe("同名项目 · project-2")
  } finally { await view.close() }
})
