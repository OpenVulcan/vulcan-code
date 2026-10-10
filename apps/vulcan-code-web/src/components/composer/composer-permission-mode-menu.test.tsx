// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { TooltipProvider } from "@/components/ui/tooltip"
import type { JudgeState } from "@/types/permission-judge"
import { ComposerSettingsMenu } from "./composer-attachment-controls"
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

// Unrelated menus are excluded so this test performs no project or provider requests.
// 排除无关菜单，确保本测试不发起项目或供应商请求。
vi.mock("@/components/features/code-context/vce-project-controls", () => ({ VceComposerMenu: () => null }))
vi.mock("./composer-knowledge-control", () => ({ ComposerKnowledgeControl: () => null }))

// Opens the actual permission submenu and verifies the judge entry follows the saved switch.
// 打开真实的权限子菜单，并验证判定入口跟随已保存的开关。
it.each<[JudgeState | null, string | null]>([
  [null, "unset"],
  ["enabled", "enabled"],
  ["off", null],
])("shows the judge entry for state %s", async (judgeState, entryState) => {
  const container = document.createElement("div")
  const root = createRoot(container)
  const select = vi.fn()
  const openJudge = vi.fn()
  vi.stubGlobal("ResizeObserver", MenuResizeObserver)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><TooltipProvider>
      <PermissionJudgeEntryContext.Provider value={{ state: judgeState, openSettings: openJudge }}><ComposerSettingsMenu
      answerMode={{ mode: "standard", saving: false, awaitingRequest: false, available: true, select: () => {}, flush: async () => true }}
      permissionMode={{ mode: "workspace_write", saving: false, available: true, select, flush: async () => true }}
      capabilities={[]} thinkingDisplayMode="popup_only" planModeEnabled={false}
      sessionVmmPolicy={null} vmmAvailable={false} onPick={() => {}} onThinkingDisplayModeChange={() => {}}
      onPlanModeChange={() => {}} onVmmPolicyChange={() => {}} onOpenGoal={() => {}}
      projectId={null} conversationId={null} onOpenKnowledge={() => {}}
      outputStyleCatalog={{ defaultId: "default", selectedId: "default", entries: [] }} onOutputStyleChange={() => {}}
    /></PermissionJudgeEntryContext.Provider></TooltipProvider></I18nProvider>))
    const trigger = container.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]')!
    await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })))
    const submenu = document.querySelector<HTMLElement>('[data-menu="permission-mode"]')!
    expect(submenu).not.toBeNull()
    await act(async () => submenu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })))
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
  } finally { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals() }
})
