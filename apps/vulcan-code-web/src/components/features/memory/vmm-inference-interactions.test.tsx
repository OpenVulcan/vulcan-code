// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import "./vmm-native-select.test-fixture"
import { I18nProvider } from "@/i18n"
import { TooltipProvider } from "@/components/ui/tooltip"
import { loadPublicAiSettings, savePublicAiInferenceRoute } from "@/service/settings-actions-service"
import type { PublicAiInferenceRoute, PublicAiInternalInferenceCandidate, PublicAiSettingsSnapshot } from "@/types"
import { VmmInferenceSettings } from "./vmm-inference-settings"

vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: vi.fn(), savePublicAiInferenceRoute: vi.fn(),
  testPublicAiInferenceRoute: vi.fn(), savePublicAiInternalModelOverrides: vi.fn(),
}))

// jsdom has no layout observer; tooltip placement is verified separately in the live browser.
// jsdom 没有布局观察器；提示位置由实际浏览器另行验证。
beforeAll(() => vi.stubGlobal("ResizeObserver", class {
  // Accepts observed elements without synthesizing layout measurements.
  // 接收被观察元素，不伪造布局测量值。
  observe() {}
  // Releases one observed element; returns nothing.
  // 释放一个被观察元素；无返回值。
  unobserve() {}
  // Ends observation for this test instance; returns nothing.
  // 结束此测试实例的观察；无返回值。
  disconnect() {}
}))
afterAll(() => vi.unstubAllGlobals())

// mount renders real route controls against two exact, available catalog entries.
// mount 使用两条准确可用的目录条目渲染真实路由控件。
// The optional diagnostic targets the selected route; returns DOM and an unmount callback.
// 可选诊断针对已选路由；返回 DOM 与卸载回调。
async function mount(diagnostic?: string) {
  // routes differ by model while retaining stable provider and account identities.
  // routes 使用不同模型，并保留稳定供应商与账号身份。
  const routes: PublicAiInferenceRoute[] = ["alpha", "beta"].map((model) => ({
    provider_id: "provider", service_entry_id: "entry", connection_id: "account",
    model_id: model, binding: { kind: "protocol", protocol_profile_id: "openai-chat" },
    operation: "conversation_respond", usage_scope: "internal_background",
  }))
  // candidates explicitly support the VMM JSON and disabled-reasoning requirements.
  // candidates 明确支持 VMM 的 JSON 和关闭思考要求。
  const candidates = routes.map((route) => ({
    route, providerDisplayName: "Provider", serviceEntryDisplayName: "Service",
    connectionDisplayName: "Account", modelDisplayName: route.model_id, catalogGeneration: 1,
    enabled: true, usageScopes: ["internal_background"], overrides: {},
    capabilities: { structured_output: "json_object", reasoning_control: "toggle", reasoning_efforts: ["none"] },
  })) as PublicAiInternalInferenceCandidate[]
  // snapshot contains only the service fields read by this settings surface.
  // snapshot 仅包含此设置界面读取的服务字段。
  const snapshot = {
    internalInferenceCandidates: candidates,
    inferenceProfiles: { vmm: { llm_routes: { precheck_l1: routes[0] } } },
    inferenceRouteDiagnostics: diagnostic ? [{ consumerProfileId: "vmm", purposeId: "precheck_l1", operation: "conversation_respond", route: routes[0], reason: diagnostic }] : [],
  } as unknown as PublicAiSettingsSnapshot
  vi.mocked(loadPublicAiSettings).mockResolvedValue(snapshot)
  vi.mocked(savePublicAiInferenceRoute).mockReset().mockResolvedValue(snapshot)
  // container and root own this test's real React DOM lifetime.
  // container 和 root 持有此测试的真实 React DOM 生命周期。
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><VmmInferenceSettings /></TooltipProvider></I18nProvider>))
  return { container, routes, dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

it("opens a searchable model dialog without saving, then saves the exact chosen route", async () => {
  // view owns controls and the exact expected route identities.
  // view 持有控件与准确的预期路由身份。
  const view = await mount()
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>('[aria-label="快速判断 · 模型"]')!.click())
    // dialog is the actual portaled picker, not an inline dropdown.
    // dialog 是真实传送的选择弹窗，而非行内下拉框。
    const dialog = document.querySelector('[role="dialog"]')!
    expect(dialog).not.toBeNull()
    expect(dialog.querySelector('input[aria-label="输入模型名称或 ID 过滤"]')).not.toBeNull()
    expect(savePublicAiInferenceRoute).not.toHaveBeenCalled()
    // option selects the second model using its visible catalog title.
    // option 使用可见目录标题选择第二个模型。
    const option = Array.from(dialog.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.includes("beta"))!
    await act(async () => option.click())
    expect(savePublicAiInferenceRoute).toHaveBeenCalledExactlyOnceWith({ consumerProfileId: "vmm", purposeId: "precheck_l1", operation: "conversation_respond", route: view.routes[1], reasoningEffort: "none" })
    expect(document.querySelector('[role="dialog"]')).toBeNull()
  } finally { await view.dispose() }
})

it("shows a compact success status and prioritizes a diagnostic in an accessible error tooltip", async () => {
  // healthy verifies that availability text is confined to the tooltip, not a separate row.
  // healthy 验证可用文案仅存在于提示中，不再单独占行。
  const healthy = await mount()
  try {
    expect(healthy.container.textContent).not.toContain("凭据可用")
    expect(healthy.container.querySelector('[aria-label="凭据可用"] svg.lucide-circle-check')).not.toBeNull()
  } finally { await healthy.dispose() }
  // failed uses a real diagnostic even though the credential remains in the catalog.
  // failed 在凭据仍位于目录时采用真实诊断。
  const failed = await mount("credential rejected")
  try {
    // status supports keyboard focus as well as pointer hover.
    // status 同时支持键盘聚焦与指针悬停。
    const status = failed.container.querySelector<HTMLButtonElement>('[aria-label="当前路由不可执行：credential rejected"]')!
    expect(status.classList.contains("text-destructive")).toBe(true)
    expect(status.querySelector("svg.lucide-circle-alert")).not.toBeNull()
    await act(async () => status.focus())
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain("credential rejected")
    expect(savePublicAiInferenceRoute).not.toHaveBeenCalled()
  } finally { await failed.dispose() }
})
