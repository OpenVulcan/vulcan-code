// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { PermissionMode } from "@/service/permission-mode"
import type { JudgeState } from "@/types/permission-judge"
import { ComposerPermissionModeControl } from "./composer-permission-mode-control"
import { PermissionJudgeEntryContext } from "./permission-judge-entry-context"

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

// Renders the chip with one judge state and opens its menu; returns the select spy, judge spy and cleanup.
// 以给定判定状态渲染标签按钮并打开菜单；返回选择监听、判定监听与清理函数。
async function openPermissionMenu(judgeState: JudgeState | null, mode: PermissionMode = "workspace_write") {
  const container = document.createElement("div")
  const root = createRoot(container)
  const select = vi.fn()
  const openJudge = vi.fn()
  vi.stubGlobal("ResizeObserver", MenuResizeObserver)
  document.body.append(container)
  await act(async () => root.render(<I18nProvider initialLocale="en-US">
    <PermissionJudgeEntryContext.Provider value={{ state: judgeState, openSettings: openJudge, openGuide: () => {} }}>
      <ComposerPermissionModeControl controller={{ mode, saving: false, available: true, select, flush: async () => true }} />
    </PermissionJudgeEntryContext.Provider>
  </I18nProvider>))
  const trigger = container.querySelector<HTMLButtonElement>("[data-permission-mode-trigger]")!
  expect(trigger.dataset.permissionModeTrigger).toBe(mode)
  await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })))
  const cleanup = async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals() }
  return { select, openJudge, cleanup }
}

// Verifies the judge entry follows the saved switch and opens its settings.
// 验证判定入口跟随已保存的开关并打开其设置。
it.each<[JudgeState | null, string | null]>([
  [null, "unset"],
  ["enabled", "enabled"],
  ["off", null],
])("shows the judge entry for state %s", async (judgeState, entryState) => {
  const { select, openJudge, cleanup } = await openPermissionMenu(judgeState)
  try {
    const options = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"][data-permission-mode]'))
    expect(options.map((option) => option.dataset.permissionMode)).toEqual(["read_only", "workspace_write", "full_access"])
    expect(options[1].getAttribute("aria-checked")).toBe("true")
    const entry = document.querySelector<HTMLElement>("[data-judge-state]")
    expect(entry?.dataset.judgeState ?? null).toBe(entryState)
    if (entry) {
      await act(async () => entry.click())
      expect(openJudge).toHaveBeenCalledTimes(1)
    }
    expect(select).not.toHaveBeenCalled()
  } finally { await cleanup() }
})

// Verifies a supervised mode applies directly while full access waits for an acknowledged confirmation.
// 验证受监督模式直接应用，而完全权限需等待勾选确认。
it("applies supervised modes directly and gates full access behind confirmation", async () => {
  const { select, cleanup } = await openPermissionMenu("enabled")
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
