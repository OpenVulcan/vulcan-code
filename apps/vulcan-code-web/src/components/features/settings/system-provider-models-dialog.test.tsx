// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { I18nProvider, readLocaleMessage, type TranslationKey } from "@/i18n"
import { publicAiSettingsWithInvalidDefaultFixture } from "@/test-fixtures/public-ai-settings"
import type { PublicAiCustomModelDefinition, PublicAiModelCandidatesRecord, PublicAiProviderDescriptor } from "@/types"
import { SystemProviderModelsDialog } from "./system-provider-models-dialog"

// Discovery and saving use detached service fixtures with no provider requests.
// 拉取与保存使用独立服务夹具，不向供应商发请求。
const service = vi.hoisted(() => ({ fetch: vi.fn(), save: vi.fn() }))
vi.mock("@/service/settings-actions-service", async (original) => ({
  ...await original<object>(), fetchPublicAiModelCandidates: service.fetch, savePublicAiCustomModels: service.save,
}))

// Root owns the real dialog and its portals for each test.
// Root 在每项测试中持有真实弹窗及其 Portal。
let root: Root
let host: HTMLDivElement
beforeEach(() => {
  vi.resetAllMocks()
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })
  host = document.createElement("div")
  document.body.append(host)
  root = createRoot(host)
})
afterEach(async () => { await act(async () => root.unmount()); host.remove() })

// mountDialog creates one static and one remote entry, each with its own account, and renders the dialog.
// mountDialog 创建一个静态入口和一个远程入口，各自拥有账号，并渲染弹窗。
async function mountDialog() {
  // Reuse the complete public settings shape and give the two sources distinct identities.
  // 复用完整公共设置结构，为两个来源赋予不同身份。
  const snapshot = publicAiSettingsWithInvalidDefaultFixture()
  const original = snapshot.providers.find((provider) => provider.id === "custom:audit-1")!
  const entry = original.service_entries.find((item) => item.kind === "conversation")!
  const protocolId = entry.protocol_routes.find((route) => route.verified)!.protocol_profile_id
  const account = snapshot.connections.find((item) => item.id === "audit-account-1")!
  const provider: PublicAiProviderDescriptor = {
    ...original, id: "fixture-system", kind: "system",
    service_entries: [
      { ...entry, id: "fixture-static", display_name: "Fixture Token Plan", model_discovery: "manifest_static" },
      { ...entry, id: "fixture-remote", display_name: "Fixture Remote", model_discovery: "open_ai" },
    ],
  }
  snapshot.connections = [
    { ...account, id: "static-account", provider_id: provider.id, service_entry_id: "fixture-static" },
    { ...account, id: "remote-account", provider_id: provider.id, service_entry_id: "fixture-remote" },
  ]
  snapshot.connectionModelAccessPolicyRecords = [
    { connectionId: "static-account", directoryKind: "fixed", defaultEnabled: true, inheritModelDefaults: false, modelEnabledOverrides: {} },
    { connectionId: "remote-account", directoryKind: "dynamic", defaultEnabled: true, inheritModelDefaults: false, modelEnabledOverrides: {} },
  ]
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><SystemProviderModelsDialog
    onOpenChange={vi.fn()} onSaved={vi.fn()} open provider={provider} snapshot={snapshot}
  /></I18nProvider>))
  return { snapshot, provider, protocolId }
}

// button locates the visible action for one catalog key and fails if the action is absent.
// button 定位一个目录键对应的可见动作，缺少动作时失败。
function button(key: TranslationKey): HTMLButtonElement {
  const action = [...document.querySelectorAll("button")].find((node) => node.textContent === readLocaleMessage("en-US", key))
  if (!action) throw new Error(`Missing action for ${key}`)
  return action
}

// selectEntry chooses a service option through the actual dialog select widget.
// selectEntry 通过实际弹窗选择器选中服务入口。
async function selectEntry(name: string) {
  const label = readLocaleMessage("en-US", "settings.systemManualModels.entry")
  const trigger = [...document.querySelectorAll<HTMLButtonElement>('[role="combobox"]')].find((node) => node.getAttribute("aria-label") === label)
  if (!trigger) throw new Error("Missing service entry selector")
  await act(async () => trigger.click())
  const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((node) => node.textContent?.trim() === name)
  if (!option) throw new Error(`Missing service option ${name}`)
  await act(async () => option.click())
}

it("fetches only the selected remote account and saves one confirmed conversation candidate", async () => {
  // Build the response with one text candidate and two ineligible output types.
  // 构建包含一个文本候选和两种不合格输出类型的响应。
  const { provider, snapshot, protocolId } = await mountDialog()
  const catalog: PublicAiModelCandidatesRecord = {
    providerId: provider.id, serviceEntryId: "fixture-remote", candidates: [
      { modelId: "fresh-chat", displayName: "Fresh Chat", kind: "conversation", addable: true,
        prefill: { protocolProfileId: protocolId, contextLimit: 65536, outputLimit: 8192, inputResources: ["image"], reasoningEfforts: [] } },
      { modelId: "fresh-image", displayName: "Fresh Image", kind: "image", addable: false },
      { modelId: "fresh-video", displayName: "Fresh Video", kind: "video", addable: false },
    ],
  }
  service.fetch.mockResolvedValue(catalog)
  service.save.mockImplementation(async (_providerId: string, models: PublicAiCustomModelDefinition[]) => ({
    ...snapshot, customModels: { [provider.id]: Object.fromEntries(models.map((model) => [model.id, model])) },
  }))

  expect(button("settings.systemManualModels.fetch").disabled).toBe(true)
  expect(service.fetch).not.toHaveBeenCalled()
  await selectEntry("Fixture Remote")
  await act(async () => button("settings.systemManualModels.fetch").click())
  expect(service.fetch).toHaveBeenCalledExactlyOnceWith("remote-account", expect.any(AbortSignal))
  expect(document.body.textContent).toContain("fresh-chat")
  expect(document.body.textContent).not.toContain("fresh-image")
  expect(document.body.textContent).not.toContain("fresh-video")
  await act(async () => button("settings.systemManualModels.addCandidate").click())
  expect(service.save).not.toHaveBeenCalled()
  expect([...document.querySelectorAll("input")].find((input) => input.value === "fresh-chat")?.disabled).toBe(true)
  await act(async () => button("settings.customModels.save").click())
  expect(service.save).toHaveBeenCalledExactlyOnceWith(provider.id, [expect.objectContaining({
    id: "fresh-chat",
    system_binding: { service_entry_id: "fixture-remote", protocol_profile_id: protocolId },
    capabilities: expect.objectContaining({ operations: ["conversation_respond"], output_resources: [], context_limit: 65536, output_limit: 8192 }),
  })])
  expect(document.body.textContent).toContain(readLocaleMessage("en-US", "settings.systemManualModels.noCandidates"))
})

it("discards a remote catalog that arrives after switching service entries", async () => {
  const { provider, protocolId } = await mountDialog()
  // Keep the first request unresolved until the real selector changes its entry.
  // 保持首次请求未完成，直到真实选择器切换入口。
  let complete!: (catalog: PublicAiModelCandidatesRecord) => void
  service.fetch.mockImplementation(() => new Promise((resolve) => { complete = resolve }))
  await selectEntry("Fixture Remote")
  await act(async () => button("settings.systemManualModels.fetch").click())
  // The captured request signal proves the entry change cancels the selected account's client request.
  // 捕获的请求信号证明切换入口会取消所选账号的客户端请求。
  const signal = service.fetch.mock.calls[0][1] as AbortSignal
  await selectEntry("Fixture Token Plan")
  expect(signal.aborted).toBe(true)
  await act(async () => complete({ providerId: provider.id, serviceEntryId: "fixture-remote", candidates: [{
    modelId: "late-chat", displayName: "Late Chat", kind: "conversation", addable: true,
    prefill: { protocolProfileId: protocolId, contextLimit: 8192, outputLimit: 1024, inputResources: [], reasoningEfforts: [] },
  }] }))
  await selectEntry("Fixture Remote")
  expect(document.body.textContent).not.toContain("late-chat")
  expect(service.save).not.toHaveBeenCalled()
})
