// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { loadProjectPermissionJudgePolicy, saveProjectPermissionJudgePolicy } from "@/service/permission-judge-service"
import type { ProjectPermissionJudgePolicySnapshot } from "@/types"
import type { PermissionMode, UnattendedConfig } from "@/service/permission-mode"
import type { JudgeBackend } from "@/types/permission-judge"
import { ComposerPermissionModeControl } from "./composer-permission-mode-control"
import { PermissionJudgeEntryContext } from "./permission-judge-entry-context"

vi.mock("@/service/permission-judge-service", () => ({
  loadProjectPermissionJudgePolicy: vi.fn(),
  saveProjectPermissionJudgePolicy: vi.fn(),
}))
const loadPolicy = vi.mocked(loadProjectPermissionJudgePolicy)
const savePolicy = vi.mocked(saveProjectPermissionJudgePolicy)

// React's concurrent test environment explicitly supports act for this isolated DOM harness.
// 为独立 DOM 测试明确启用 React 并发测试环境的 act 支持。
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })

// jsdom has no layout engine; this observer satisfies Radix without claiming visual geometry coverage.
// jsdom 没有布局引擎；此观察器满足 Radix 调用，不声称覆盖视觉几何验收。
class MenuResizeObserver {
  // Observation methods intentionally produce no layout notifications.
  // 观察方法不产生布局通知。
  observe() {}
  // Removes one test observation.
  // 移除一个测试观察。
  unobserve() {}
  // Ends the test observer without side effects.
  // 无副作用地结束测试观察器。
  disconnect() {}
}

// Renders the chip with one configured judge backend and opens its menu; returns the select spy,
// judge spy and cleanup.
// 以给定已配置判定后端渲染标签按钮并打开菜单；返回选择监听、判定监听与清理函数。
async function openPermissionMenu(judgeBackend: JudgeBackend | null, mode: PermissionMode = "workspace_write",
  unattended: UnattendedConfig | null = null, projectId: string | null = null) {
  const container = document.createElement("div")
  const root = createRoot(container)
  const select = vi.fn()
  const openJudge = vi.fn()
  const apply = vi.fn(async () => true)
  const loadDefault = vi.fn(async () => ({ denyPolicy: "never push", source: "project" as const, maxChars: 4000 }))
  vi.stubGlobal("ResizeObserver", MenuResizeObserver)
  document.body.append(container)
  await act(async () => root.render(<I18nProvider initialLocale="en-US">
    <PermissionJudgeEntryContext.Provider value={{ backend: judgeBackend, openSettings: openJudge, openGuide: () => {} }}>
      <ComposerPermissionModeControl controller={{
        mode, saving: false, available: true, select, flush: async () => true,
        unattended: { value: unattended, available: true, saving: false, apply, loadDefault },
      }} projectId={projectId} />
    </PermissionJudgeEntryContext.Provider>
  </I18nProvider>))
  const trigger = container.querySelector<HTMLButtonElement>("[data-permission-mode-trigger]")!
  expect(trigger.dataset.permissionModeTrigger).toBe(mode)
  await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })))
  const cleanup = async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals() }
  return { select, openJudge, apply, loadDefault, cleanup }
}

// Verifies an unconfigured judge greys auto-approve out and opens the judge settings instead.
// 验证判定未配置时自动审批显示为灰色，点击改为打开判定设置。
it("opens the judge settings from auto-approve until a judge is configured", async () => {
  const { select, openJudge, cleanup } = await openPermissionMenu(null)
  try {
    const options = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"][data-permission-mode]'))
    expect(options.map((option) => option.dataset.permissionMode))
      .toEqual(["read_only", "workspace_write", "auto_approve", "full_access"])
    expect(options[1].getAttribute("aria-checked")).toBe("true")
    const auto = options[2]
    expect(auto.dataset.needsSetup).toBe("")
    await act(async () => auto.click())
    expect(openJudge).toHaveBeenCalledTimes(1)
    expect(select).not.toHaveBeenCalled()
  } finally { await cleanup() }
})

// Verifies a configured judge makes auto-approve an ordinary choice.
// 验证判定已配置时自动审批是普通选项。
it("selects auto-approve once a judge is configured", async () => {
  const { select, openJudge, cleanup } = await openPermissionMenu("local")
  try {
    const auto = document.querySelector<HTMLElement>('[data-permission-mode="auto_approve"]')!
    expect(auto.dataset.needsSetup).toBeUndefined()
    await act(async () => auto.click())
    expect(select).toHaveBeenCalledExactlyOnceWith("auto_approve")
    expect(openJudge).not.toHaveBeenCalled()
  } finally { await cleanup() }
})

// Verifies unattended mode is offered only under auto-approve, the one mode with a reviewer.
// 验证无人值守只在自动审批下提供，因为只有该模式有复核器。
it.each<[PermissionMode, boolean]>([
  ["read_only", false],
  ["workspace_write", false],
  ["auto_approve", true],
  ["full_access", false],
])("offers unattended mode under %s: %s", async (mode, offered) => {
  const { cleanup } = await openPermissionMenu("local", mode)
  try {
    expect(document.querySelector("[data-unattended-entry]") !== null).toBe(offered)
  } finally { await cleanup() }
})

// policy builds one project's categories: writes outside the project allowed by default, database
// resets confirmed by default, with `chosen` overriding either.
// policy 构建一个项目的类别：项目外写入默认允许，数据库重置默认需确认，`chosen` 可覆盖任一类别。
function policy(chosen: Record<string, string> = {}): ProjectPermissionJudgePolicySnapshot {
  return {
    projectId: "p1",
    options: [
      { key: "cross_project_writes", defaultMode: "allow", modes: ["allow", "review"], chosenMode: chosen.cross_project_writes ?? null },
      { key: "database_reset", defaultMode: "confirm", modes: ["confirm", "allow"], chosenMode: chosen.database_reset ?? null },
    ],
    unsupportedOptions: {},
    issue: null,
  }
}

// Verifies the project's categories are checkboxes under auto-approve: checked means allowed, each
// change saves at once and the menu stays open.
// 验证自动审批下项目类别为勾选项：勾选表示允许，每次修改立即保存且菜单保持打开。
it("quick-sets the project's judge categories under auto-approve", async () => {
  loadPolicy.mockReset().mockResolvedValue(policy())
  savePolicy.mockReset().mockResolvedValueOnce(policy({ database_reset: "allow" }))
  const { cleanup } = await openPermissionMenu("local", "auto_approve", null, "p1")
  try {
    expect(loadPolicy).toHaveBeenCalledExactlyOnceWith("p1")
    const category = (key: string) => document.querySelector<HTMLElement>(`[data-judge-category="${key}"]`)!
    expect(category("cross_project_writes").getAttribute("aria-checked")).toBe("true")
    expect(category("database_reset").getAttribute("aria-checked")).toBe("false")
    await act(async () => category("database_reset").click())
    expect(savePolicy).toHaveBeenLastCalledWith("p1", { database_reset: "allow" })
    expect(category("database_reset").getAttribute("aria-checked")).toBe("true")
    savePolicy.mockResolvedValueOnce(policy({ database_reset: "allow", cross_project_writes: "review" }))
    await act(async () => category("cross_project_writes").click())
    expect(savePolicy).toHaveBeenLastCalledWith("p1", { database_reset: "allow", cross_project_writes: "review" })
  } finally { await cleanup() }
})

// Verifies a category set back to its default follows the default again instead of pinning it.
// 验证类别改回默认值时恢复跟随默认，而不是固定为该值。
it("follows the default again when a category returns to it", async () => {
  loadPolicy.mockReset().mockResolvedValue(policy({ database_reset: "allow" }))
  savePolicy.mockReset().mockResolvedValue(policy())
  const { cleanup } = await openPermissionMenu("local", "auto_approve", null, "p1")
  try {
    await act(async () => document.querySelector<HTMLElement>('[data-judge-category="database_reset"]')!.click())
    expect(savePolicy).toHaveBeenCalledExactlyOnceWith("p1", {})
  } finally { await cleanup() }
})

// Verifies the categories are read only under auto-approve with a project and a configured judge.
// 验证只有在自动审批、有项目且判定器已配置时才读取类别。
it.each<[JudgeBackend | null, PermissionMode, string | null]>([
  ["local", "workspace_write", "p1"],
  ["local", "auto_approve", null],
  [null, "auto_approve", "p1"],
])("hides the categories for judge %s, mode %s, project %s", async (backend, mode, projectId) => {
  loadPolicy.mockReset().mockResolvedValue(policy())
  const { cleanup } = await openPermissionMenu(backend, mode, null, projectId)
  try {
    expect(loadPolicy).not.toHaveBeenCalled()
    expect(document.querySelector("[data-judge-category]")).toBeNull()
  } finally { await cleanup() }
})

// Verifies a supervised mode applies directly while full access waits for an acknowledged confirmation.
// 验证受监督模式直接应用，而完全权限需等待勾选确认。
it("applies supervised modes directly and gates full access behind confirmation", async () => {
  const { select, cleanup } = await openPermissionMenu("local")
  try {
    const option = (mode: PermissionMode) => document.querySelector<HTMLElement>(`[data-permission-mode="${mode}"]`)!
    await act(async () => option("full_access").click())
    expect(select).not.toHaveBeenCalled()
    const confirm = document.querySelector<HTMLButtonElement>("[data-full-access-confirm]")!
    expect(confirm.disabled).toBe(true)
    await act(async () => document.querySelector<HTMLElement>("[data-full-access-acknowledge]")!.click())
    expect(confirm.disabled).toBe(false)
    await act(async () => confirm.click())
    expect(select).toHaveBeenCalledWith("full_access")
  } finally { await cleanup() }
})

// Verifies the unattended dialog prefills the layered default and applies only the confirmed text.
// 验证无人值守对话框预填分层默认值，且只应用用户确认的文本。
it("turns unattended mode on with the confirmed deny policy", async () => {
  const { apply, loadDefault, cleanup } = await openPermissionMenu("local", "auto_approve")
  try {
    const entry = document.querySelector<HTMLElement>("[data-unattended-entry]")!
    expect(entry.dataset.unattendedEntry).toBe("off")
    await act(async () => entry.click())
    expect(loadDefault).toHaveBeenCalledTimes(1)
    const policy = document.querySelector<HTMLTextAreaElement>("[data-unattended-policy]")!
    expect(policy.value).toBe("never push")
    expect(document.querySelector<HTMLElement>("[data-unattended-source]")?.dataset.unattendedSource).toBe("project")
    expect(apply).not.toHaveBeenCalled()
    await act(async () => document.querySelector<HTMLButtonElement>("[data-unattended-confirm]")!.click())
    expect(apply).toHaveBeenCalledExactlyOnceWith({ deny_policy: "never push" })
  } finally { await cleanup() }
})

// Verifies an unattended session shows its own policy and can turn the mode off.
// 验证无人值守会话显示自身策略并可关闭该模式。
it("edits or turns off an active unattended mode", async () => {
  const { apply, cleanup } = await openPermissionMenu("local", "auto_approve", { deny_policy: "keep main" })
  try {
    expect(document.querySelector("[data-unattended-chip]")).not.toBeNull()
    await act(async () => document.querySelector<HTMLElement>('[data-unattended-entry="on"]')!.click())
    expect(document.querySelector<HTMLTextAreaElement>("[data-unattended-policy]")!.value).toBe("keep main")
    expect(document.querySelector("[data-unattended-source]")).toBeNull()
    await act(async () => document.querySelector<HTMLButtonElement>("[data-unattended-disable]")!.click())
    expect(apply).toHaveBeenCalledExactlyOnceWith(null)
  } finally { await cleanup() }
})
