// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { describe, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { PublicAiContextWindowPolicyOverrides } from "@/types"
import { ContextBasicsSettingsArea } from "./context-basics-settings"

// service owns only synthetic policy and fixed-node snapshots used by these UI regressions.
// service 仅持有这些界面回归使用的合成策略与固定节点快照。
const service = vi.hoisted(() => {
  // snapshot supplies exactly the fields read by this page; media and retry editors are isolated below.
  // snapshot 提供此页读取的精确字段；媒体与重试编辑器在下方隔离。
  const snapshot = {
    contextWindowPolicyOverrides: {},
    contextCompactionWindowNodes: { nodes: [
      { id: "compression-window-1", tokens: 10000 },
      { id: "compression-window-2", tokens: 20000 },
      { id: "compression-window-3", tokens: 30000 },
      { id: "compression-window-4", tokens: 40000 },
      { id: "compression-window-5", tokens: 50000 },
    ] },
    newSessionContextCompactionWindowNodeId: null,
    usageRefreshIntervalSeconds: 300,
  }
  return { snapshot, load: vi.fn(async () => snapshot), savePolicy: vi.fn(async (policy: unknown) => ({ ...snapshot, contextWindowPolicyOverrides: policy })) }
})

vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: service.load,
  savePublicAiContextWindowPolicyOverrides: service.savePolicy,
  savePublicAiContextCompactionWindowNodes: vi.fn(),
  savePublicAiNewSessionContextWindowDefault: vi.fn(),
  savePublicAiUsageRefreshSettings: vi.fn(),
}))
vi.mock("./provider-retry-settings", () => ({ ProviderRetrySettingsArea: () => <span>retry editor</span> }))
vi.mock("./media-delivery-policy-card", () => ({ MediaDeliveryPolicyCard: () => <span>media editor</span> }))
vi.mock("./public-ai-model-capability-dialog", () => ({
  // Editor makes one explicit policy edit while the page retains responsibility for draft ownership.
  // Editor 执行一次显式策略编辑，页面继续负责草稿归属。
  ContextWindowPolicyEditor: (props: { onChange: (policy: PublicAiContextWindowPolicyOverrides) => void; value?: PublicAiContextWindowPolicyOverrides }) => (
    <button type="button" aria-label="edit policy" onClick={() => props.onChange({ effective_context_window_percent: 80 })}>{props.value?.effective_context_window_percent ?? "inherit"}</button>
  ),
}))

describe("context settings draft navigation", () => {
  it("retains the context draft across task sections and saves the exact edited policy", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
    // container and root are temporary test-owned UI resources.
    // container 和 root 是临时的测试自有界面资源。
    const container = document.createElement("div")
    const root = createRoot(container)
    document.body.append(container)
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><ContextBasicsSettingsArea actions={{
        onSavePromptOutputStyle: vi.fn(async () => undefined),
        onSavePromptAnswerMode: vi.fn(async () => undefined),
        onSaveGitCommitSettings: vi.fn(async () => undefined),
        onSaveProviderRetrySettings: vi.fn(async () => undefined),
        onSaveProviderRequestTraceSettings: vi.fn(async () => undefined),
        onOpenSelectedProviderApiTraceDirectory: vi.fn(async () => undefined),
        onClearSelectedProviderApiTraceDirectory: vi.fn(async () => false),
        onRestoreArchivedConversations: vi.fn(async () => undefined),
        onDeleteArchivedConversations: vi.fn(async () => undefined),
      }} /></I18nProvider>))
      await act(async () => container.querySelector<HTMLButtonElement>('[aria-label="edit policy"]')?.click())
      // Directory actions change the visible category while keeping each editor mounted.
      // 目录操作切换可见分类，同时保持各编辑器挂载。
      const tabs = Array.from(container.querySelectorAll<HTMLButtonElement>('.vk-directory-nav button'))
      const retryTab = tabs.find((tab) => tab.textContent === "出错重试")
      const contextTab = tabs.find((tab) => tab.textContent === "对话长度与压缩")
      expect(retryTab).toBeDefined()
      expect(contextTab).toBeDefined()
      await act(async () => retryTab!.click())
      expect(retryTab!.getAttribute("aria-current")).toBe("page")
      expect(container.querySelector('[aria-label="edit policy"]')?.closest('section[hidden]')).not.toBeNull()
      await act(async () => contextTab!.click())
      expect(contextTab!.getAttribute("aria-current")).toBe("page")
      expect(container.querySelector('[aria-label="edit policy"]')?.textContent).toBe("80")
      expect(container.textContent).toContain("有未保存的修改")
      // saveButton is located within the active panel, not another independently mounted section.
      // saveButton 在活动面板内定位，不命中其他独立挂载分区。
      const saveButton = Array.from(container.querySelectorAll<HTMLButtonElement>('.context-basics-tabs > section:not([hidden]) button')).find((button) => button.textContent?.includes("保存"))
      expect(saveButton).toBeDefined()
      await act(async () => saveButton?.click())
      expect(service.savePolicy).toHaveBeenCalledExactlyOnceWith({ effective_context_window_percent: 80 })
      expect(container.textContent).toContain("已保存")
    } finally {
      await act(async () => root.unmount())
      container.remove()
    }
  })
})
