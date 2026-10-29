// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterEach, describe, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { TooltipProvider } from "@/ui-kit/primitives/tooltip"
import { publicAiSettingsWithInvalidDefaultFixture } from "@/test-fixtures/public-ai-settings"
import { ModelServicesArea } from "@/components/features/settings/model-services/model-services-area"

// service replaces only catalog I/O; dialogs, route grouping and focus behavior stay real.
// service 仅替换目录读写；弹窗、路由分组及焦点行为保持真实组件实现。
const service = vi.hoisted(() => ({ load: vi.fn(), conversation: vi.fn(), internal: vi.fn(), candidate: vi.fn() }))
vi.mock("@/service/settings-actions-service", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/service/settings-actions-service")>(),
  loadPublicAiSettings: service.load,
  savePublicAiModelEnabled: service.conversation,
  savePublicAiInternalModelEnabled: service.internal,
  savePublicAiMultimodalUnderstandingCandidate: service.candidate,
}))

afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals() })

describe("model list access and usage interactions", () => {
  it("keeps route details out of rows and returns focus from the access window to its trigger", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
    // snapshot reuses existing multi-account fixtures without adding a per-file visual harness.
    // snapshot 复用现有多账号数据，不增加逐文件视觉夹具。
    const snapshot = publicAiSettingsWithInvalidDefaultFixture()
    service.load.mockResolvedValue(snapshot)
    // container and root belong only to this isolated render.
    // container 与 root 仅属于本次隔离渲染。
    const container = document.createElement("div")
    const root = createRoot(container)
    document.body.append(container)
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><ModelServicesArea /></TooltipProvider></I18nProvider>))
      expect(container.querySelectorAll(".vk-mcat-catalog-row")).toHaveLength(3)
      expect(container.querySelector("details")).toBeNull()
      expect(container.textContent).not.toContain("custom_user")
      // trigger is the first model's explicit access action, not its capability editor.
      // trigger 是首个模型明确的接入操作，不是其能力编辑器。
      const trigger = container.querySelector<HTMLButtonElement>('.vk-mcat-catalog-actions button[aria-label^="接入 "]')!
      trigger.focus()
      await act(async () => trigger.click())
      expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1)
      expect(document.querySelector(".vk-mcat-access-routes")?.textContent).toContain("custom_user")
      expect(document.querySelector(".vk-mcat-access-routes")?.textContent).toContain("模型 1 停用账号")
      expect(document.querySelector(".vk-mcat-access-routes")?.textContent).toContain("账号未准入此模型")
      expect(document.querySelectorAll(".vk-mcat-access-route")).toHaveLength(2)
      await act(async () => document.querySelector<HTMLButtonElement>('.vk-mcat-access-dialog > [data-slot="dialog-close"]')!.click())
      expect(document.querySelector('[role="dialog"]')).toBeNull()
      await vi.waitFor(() => expect(document.activeElement).toBe(trigger))
      expect(service.conversation).not.toHaveBeenCalled()
      expect(service.internal).not.toHaveBeenCalled()
      expect(service.candidate).not.toHaveBeenCalled()
    } finally { await act(async () => root.unmount()); container.remove() }
  })

  it("keeps unsupported usage columns in place and sends only the selected model's conversation change", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
    // snapshot and next preserve all independent usage dimensions while changing one exact preference.
    // snapshot 与 next 保留所有独立用途维度，仅修改一个准确偏好。
    const snapshot = publicAiSettingsWithInvalidDefaultFixture()
    const next = structuredClone(snapshot)
    snapshot.internalInferenceCandidates = [{
      providerDisplayName: "验收供应商", serviceEntryDisplayName: "隔离兼容入口", connectionDisplayName: "验收账号", modelDisplayName: "验收模型",
      route: { provider_id: "custom:audit-1", service_entry_id: "compatible-api", connection_id: "audit-account-1", model_id: "audit-chat-1",
        binding: { kind: "protocol", protocol_profile_id: "openai-chat-completions.v1" }, operation: "conversation_respond", usage_scope: "internal_background" },
      catalogGeneration: 1, enabled: true, usageScopes: ["internal_background"], capabilities: snapshot.catalogs["audit-account-1"].models[0].capabilities, overrides: {},
    }]
    next.internalInferenceCandidates = structuredClone(snapshot.internalInferenceCandidates)
    next.modelAccessRecords[0].enabled = false
    service.load.mockResolvedValue(snapshot)
    service.conversation.mockResolvedValue(next)
    // container and root never connect to the real settings service.
    // container 与 root 不会连接真实设置服务。
    const container = document.createElement("div")
    const root = createRoot(container)
    document.body.append(container)
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><ModelServicesArea /></TooltipProvider></I18nProvider>))
      // rows and cells identify visible controls without relying on protocol or account ordering.
      // rows 与 cells 标识可见控件，不依赖协议或账号排序。
      const rows = container.querySelectorAll(".vk-mcat-catalog-row")
      const cells = rows[0].querySelectorAll(".vk-mcat-purpose-cell")
      expect(cells).toHaveLength(3)
      expect(rows[1].querySelectorAll(".vk-mcat-purpose-cell")).toHaveLength(3)
      expect(rows[1].querySelectorAll(".vk-mcat-purpose-cell")[2].textContent).toContain("—")
      // Tooltip state must not replace the checked state used to color either eligible switch.
      // 提示层状态不得覆盖两种可用开关用于着色的选中状态。
      for (const cell of [cells[1], cells[2]]) {
        expect(cell.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("true")
        expect(cell.querySelector('[role="switch"]')?.getAttribute("data-state")).toBe("checked")
      }
      await act(async () => cells[0].querySelector<HTMLButtonElement>('[role="switch"]')!.click())
      expect(service.conversation).toHaveBeenCalledExactlyOnceWith({ providerId: "custom:audit-1", modelId: "audit-chat-1", enabled: false })
      expect(service.internal).not.toHaveBeenCalled()
      expect(service.candidate).not.toHaveBeenCalled()
      expect(cells[0].querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("false")
    } finally { await act(async () => root.unmount()); container.remove() }
  })

  it("lists every configured provider in the model-led view and names the owning provider on each row", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
    const snapshot = publicAiSettingsWithInvalidDefaultFixture()
    service.load.mockResolvedValue(snapshot)
    const container = document.createElement("div")
    const root = createRoot(container)
    document.body.append(container)
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><ModelServicesArea initialView="model" /></TooltipProvider></I18nProvider>))
      // rows holds every configured provider's catalog rows; same-named models are never merged.
      // rows 包含每个已配置供应商的目录行；同名模型不会合并。
      const providerIds = new Set(snapshot.connections.map((connection) => connection.provider_id))
      const rows = container.querySelectorAll(".vk-mcat-catalog-row")
      expect(providerIds.size).toBeGreaterThan(1)
      expect(rows.length).toBeGreaterThan(3)
      expect([...rows].every((row) => row.querySelector(".vk-ms-row-provider"))).toBe(true)
    } finally { await act(async () => root.unmount()); container.remove() }
  })
})
