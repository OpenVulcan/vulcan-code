// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { TooltipProvider } from "@/ui-kit/primitives/tooltip"
import { ComposerSettingsMenu } from "./composer-session-settings"

// React's concurrent test environment explicitly supports act for this isolated DOM harness.
// 为独立 DOM 测试明确启用 React 并发测试环境的 act 支持。
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })

// jsdom has no layout engine; this observer satisfies Radix without claiming visual geometry coverage.
// jsdom 没有布局引擎；此观察器满足 Radix 调用，不声称覆盖视觉几何验收。
class MenuResizeObserver {
  // Observation methods intentionally produce no layout notifications; they return nothing.
  // 观察方法不产生布局通知，且不返回值。
  observe() {}
  // Removes one test observation; no actual layout subscription exists.
  // 移除一个测试观察；不存在实际布局订阅。
  unobserve() {}
  // Ends the test observer without side effects.
  // 无副作用地结束测试观察器。
  disconnect() {}
}
vi.mock("./composer-knowledge-control", () => ({ ComposerKnowledgeControl: () => null }))

// Exercises actual Radix menus and verifies configuration navigation never changes user policy.
// 操作真实 Radix 菜单，并验证配置导航绝不改变用户策略。
it.each([false, true])("keeps settings reachable when policy editing is %s", async (enabled) => {
  // Test-owned root and callbacks isolate menu state from actual workspace configuration.
  // 测试持有的根节点与回调将菜单状态和实际工作区配置隔离。
  const container = document.createElement("div")
  const root = createRoot(container)
  const navigate = vi.fn()
  const changePolicy = vi.fn()
  vi.stubGlobal("ResizeObserver", MenuResizeObserver)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><ComposerSettingsMenu
      answerMode={{ mode: "standard", saving: false, awaitingRequest: false, available: true, select: () => {}, flush: async () => true }}
      capabilities={[]} thinkingDisplayMode="popup_only" planModeEnabled={false}
      sessionVmmPolicy={null} vmmAvailable={enabled} vmmStatus={enabled ? "not_ready" : "disabled"}
      onOpenMemorySettings={navigate} onPick={() => {}} onThinkingDisplayModeChange={() => {}}
      onPlanModeChange={() => {}} onVmmPolicyChange={changePolicy} onOpenGoal={() => {}}
      projectId={null} conversationId={null} onOpenKnowledge={() => {}}
      outputStyleCatalog={{ defaultId: "default", selectedId: "default", entries: [] }} onOutputStyleChange={() => {}}
    /></TooltipProvider></I18nProvider>))
    // The status-bar entry opens the same session settings panel used by real users.
    // 状态栏入口打开真实用户使用的同一个会话设置面板。
    const trigger = container.querySelector<HTMLButtonElement>('[data-action="session-settings"]')!
    await act(async () => trigger.click())
    expect(document.body.textContent).toContain(enabled ? "记忆尚未就绪，仍可配置会话策略" : "当前作用域未启用记忆")
    const settings = Array.from(document.querySelectorAll<HTMLButtonElement>(".vk-ss button")).find((item) => item.textContent === "设置")!
    expect(settings).toBeDefined()
    await act(async () => settings.click())
    expect(navigate).toHaveBeenCalledTimes(1)
    expect(changePolicy).not.toHaveBeenCalled()
  } finally { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals() }
})
