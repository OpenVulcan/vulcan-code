// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProfileLifetimes, VmmNativeProjectSummary } from "@/types"
import { VmmNativeProjectProfilePolicy } from "./vmm-native-project-profile-policy"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { profileDefaults } from "./vmm-native-profile-policy.test-fixture"

// Restore the React test flag after this isolated DOM suite.
// 在此隔离 DOM 测试组之后恢复 React 测试标志。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

// Same-name catalog entries prove that submitted ownership follows stable IDs.
// 同名目录项证明提交归属遵循稳定 ID。
const projects: VmmNativeProjectSummary[] = [1, 2].map((projectId) => ({ projectId, name: "同名项目", tag: `project-${projectId}`, protected: projectId === 1, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }))

// Returns the closed production response variant without personal rules.
// 返回不含个人规则的封闭生产响应变体。
function reply(projectId: number, overrides: VmmNativeProfileLifetimes | null = null): VmmMemoryActionResult {
  const value = { projectId, overrides, effective: overrides === null ? profileDefaults.project : overrides, global: profileDefaults.project }
  return { action: "native_project_profile_policy", kind: "native_project_profile_policy", summary: "", request: {}, response: value, data: { kind: "native_project_profile_policy", value } }
}

// Mounts the actual editor with DOM-only interaction and an explicit transport boundary.
// 挂载真实编辑器，仅通过 DOM 交互和显式传输边界操作。
async function mount(onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>) {
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeProjectProfilePolicy projects={projects} busy={false} onRunAction={onRunAction} /></I18nProvider>))
  return {
    container,
    // Chooses the actual catalog label while retaining its numeric ownership value.
    // 选择实际目录标签，同时保留其数字归属值。
    async choose(id: number) { await chooseMemoryOption(container.querySelector<HTMLButtonElement>("#profile-policy-project")!, `同名项目 · project-${id}`) },
    // Sends a normal input event to the stable-level base duration amount.
    // 向稳定级基础期限数量发送正常输入事件。
    async edit(value: string) {
      const input = container.querySelector<HTMLInputElement>("#project-profile-stable-baseMs")!
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value)
        input.dispatchEvent(new Event("input", { bubbles: true }))
      })
    },
    // Locates a named user action, never internal component state.
    // 定位具名用户动作，而非内部组件状态。
    button(label: string) { return Array.from(container.querySelectorAll("button")).find((button) => button.textContent === label)! },
    // Releases only this fixture's DOM and mounted effects.
    // 仅释放此夹具的 DOM 及已挂载副作用。
    async close() { await act(async () => root.unmount()); container.remove() },
  }
}

// Covers exact units, invalid drafts, project-only payload and explicit restoration of inheritance.
// 覆盖准确单位、无效草稿、项目专属载荷以及显式恢复继承。
it("saves only the selected memory project's rules and restores inheritance explicitly", async () => {
  let saved: VmmNativeProfileLifetimes | null = null
  const run = vi.fn(async (action: string, payload?: Record<string, unknown>) => {
    if (action === "native_project_profile_policy_update") saved = payload?.mode === "inherit" ? null : payload?.rules as VmmNativeProfileLifetimes
    return reply(2, saved)
  })
  const view = await mount(run)
  try {
    expect(run).not.toHaveBeenCalled()
    await view.choose(2)
    expect(run).toHaveBeenCalledExactlyOnceWith("native_project_profile_policy", { projectId: 2 })
    expect(view.container.querySelector("#profile-user-stable-baseMs")).toBeNull()
    expect(view.container.textContent).toContain("个人画像始终使用全局规则")
    // Each of the nine unit controls identifies its own level and duration field.
    // 九个单位控件分别标识自身的等级和期限字段。
    expect(new Set(Array.from(view.container.querySelectorAll('[role="combobox"][aria-label]')).map((entry) => entry.getAttribute("aria-label"))).size).toBe(9)
    expect(view.container.textContent).toContain("每个单位只影响相邻数值")
    await view.edit("0")
    expect(view.button("保存项目覆盖").disabled).toBe(true)
    await view.edit("31")
    await act(async () => view.button("保存项目覆盖").click())
    expect(run.mock.calls[1]).toEqual(["native_project_profile_policy_update", { mode: "override", projectId: 2, rules: { ...profileDefaults.project, stable: { ...profileDefaults.project.stable, baseMs: 2678400000 } } }])
    expect(view.container.querySelector('[role="status"]')?.textContent).toContain("已保存")
    await act(async () => view.button("恢复全局继承").click())
    expect(run.mock.calls[2]).toEqual(["native_project_profile_policy_update", { mode: "inherit", projectId: 2 }])
    expect(view.container.textContent).toContain("当前继承全局项目画像规则")
  } finally { await view.close() }
})

// A mismatched owner or failed save must not replace a local draft or report success.
// 归属不匹配或保存失败不得替换本地草稿或报告成功。
it("preserves failed drafts and rejects a response belonging to another project", async () => {
  const run = vi.fn().mockResolvedValueOnce(reply(1)).mockResolvedValueOnce(null).mockResolvedValueOnce(reply(2))
  const view = await mount(run)
  try {
    await view.choose(1)
    await view.edit("31")
    await act(async () => view.button("保存项目覆盖").click())
    expect(view.container.querySelector('[role="alert"]')).not.toBeNull()
    expect(view.container.querySelector<HTMLInputElement>("#project-profile-stable-baseMs")!.value).toBe("31")
    await act(async () => view.button("重新读取规则").click())
    expect(view.container.querySelector('[role="status"]')).toBeNull()
    expect(view.container.querySelector<HTMLInputElement>("#project-profile-stable-baseMs")!.value).toBe("31")
  } finally { await view.close() }
})
