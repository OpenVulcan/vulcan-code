// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench"
import { ProjectAgentSkillsSection } from "./project-settings-sections"

// Verifies a failed override read never displays invented inheritance or admits configuration writes.
// 验证覆盖读取失败绝不显示虚构继承，也不准入配置写入。
it("shows the authoritative read error and retries without offering an unverified override", async () => {
  const workbench = browserFallbackSnapshot()
  const onRetry = vi.fn()
  const onSetProjectAgentSkill = vi.fn()
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><ProjectAgentSkillsSection workbench={workbench} agentLoading={false} agentLoaded={false} agentError="读取失败：合成测试" agentOverrides={{}} agentBusySkill={null} onRetry={onRetry} onSetProjectAgentSkill={onSetProjectAgentSkill} /></I18nProvider>))
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("读取失败：合成测试")
    expect(container.querySelector(".project-settings-segmented")).toBeNull()
    await act(async () => container.querySelector<HTMLButtonElement>("button")!.click())
    expect(onRetry).toHaveBeenCalledTimes(1)
    expect(onSetProjectAgentSkill).not.toHaveBeenCalled()
  } finally { await act(async () => root.unmount()); container.remove() }
})
