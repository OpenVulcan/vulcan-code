// @vitest-environment jsdom
import * as React from "react"
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { loadCloudMemorySetup, type CloudMemoryChoice, type CloudMemoryOffer, type CloudMemorySetup } from "@/service/cloud-memory-service"
import { VmmCloudMemoryPicker } from "./vmm-cloud-memory"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"

vi.mock("@/service/cloud-memory-service", () => ({ loadCloudMemorySetup: vi.fn() }))

// Declares a synthetic split inference route and its embedding and optional reranking providers.
// 声明合成的分阶段推理路由及其向量和可选重排供应商。
const offer: CloudMemoryOffer = {
  id: "next",
  name: "Next Model",
  consent_revision: "next-revision",
  providers: [
    { capability: "inference", purposes: ["pcl1", "pcl2"], model: "fixture-precheck", provider: "Precheck Provider",
      region: "Region A", data_policy: "Provider A policy", privacy_url: "https://example.com/a" },
    { capability: "inference", purposes: ["ptl1", "ptl2", "profile_instruction"], model: "fixture-postaction",
      provider: "Postaction Provider", region: "Region B", data_policy: "Provider B policy", privacy_url: "https://example.com/b" },
    { capability: "embedding", purposes: [], model: "fixture-embedding", provider: "Embedding Provider",
      region: "Region C", data_policy: "Provider C policy", privacy_url: "https://example.com/c" },
    { capability: "rerank", purposes: [], model: "fixture-rerank", provider: "Rerank Provider",
      region: "Region D", data_policy: "Provider D policy", privacy_url: "https://example.com/d" },
  ],
}

// Mounts the production picker with the supplied catalog and captures its submitted choice.
// 使用所给目录挂载正式选择器，并捕获它提交的选择。
async function mount(setup: CloudMemorySetup, locale = "en-US") {
  vi.mocked(loadCloudMemorySetup).mockResolvedValue(setup)
  const container = document.body.appendChild(document.createElement("div"))
  const root = createRoot(container)
  const report = vi.fn<(choice: CloudMemoryChoice | null) => void>()
  // Host models the controlled state used by the setup wizard.
  // Host 模拟设置向导使用的受控状态。
  function Host() {
    const [choice, setChoice] = React.useState<CloudMemoryChoice | null>(null)
    return <VmmCloudMemoryPicker choice={choice} onChoice={(value) => { report(value); setChoice(value) }} />
  }
  await act(async () => root.render(<I18nProvider initialLocale={locale}><Host /></I18nProvider>))
  return {
    container,
    report,
    dispose: async () => { await act(async () => root.unmount()); container.remove() },
  }
}

// Verifies the new site default never replaces a withdrawn selection and consent is tied to the visible disclosure.
// 验证新站点默认项不会替换已下架选择，且用户同意绑定到可见披露。
it("requires a fresh model, local PII region, and consent after withdrawal", async () => {
  const page = await mount({
    status: "selection_changed",
    selectedName: "Withdrawn model",
    selected: { modelId: "old", consentRevision: "old-revision", accountContext: "site", piiRegion: "zh-CN", rerank: false },
    accountContext: "site",
    catalog: { default_id: offer.id, models: [offer] },
    piiRegions: ["en-US", "zh-CN"],
  })
  try {
    const model = page.container.querySelector<HTMLButtonElement>('[aria-labelledby="memory-cloud-model-label"]')!
    expect(model.hasAttribute("data-placeholder")).toBe(true)
    expect(page.container.querySelectorAll('[role="checkbox"]')).toHaveLength(0)
    await chooseMemoryOption(model, "Next Model · Site default")
    expect(page.container.querySelectorAll('a[href^="https://example.com/"]')).toHaveLength(4)
    const [rerank, consent] = Array.from(page.container.querySelectorAll<HTMLButtonElement>('[role="checkbox"]'))
    expect(rerank.getAttribute("aria-checked")).toBe("true")
    expect(consent.disabled).toBe(false)
    expect(consent.getAttribute("aria-checked")).toBe("false")
    const region = page.container.querySelector<HTMLButtonElement>('[aria-labelledby="memory-cloud-region-label"]')!
    expect(region.textContent).toContain(new Intl.DisplayNames(["en-US"], { type: "language" }).of("zh-CN"))
    await act(async () => consent.click())
    expect(page.report).toHaveBeenLastCalledWith({
      modelId: "next", consentRevision: "next-revision", accountContext: "site", piiRegion: "zh-CN", rerank: true,
    })
    await act(async () => rerank.click())
    expect(page.report).toHaveBeenLastCalledWith(null)
  } finally {
    await page.dispose()
  }
})

// Uses the supported interface locale as the initial PII region while leaving consent unchecked.
// 初次配置时使用本机支持的界面语言作为 PII 地区，并保持同意框未勾选。
it("defaults the local PII region to the interface locale", async () => {
  const page = await mount({
    status: "available", selectedName: null, selected: null, accountContext: "site",
    catalog: { default_id: offer.id, models: [offer] }, piiRegions: ["en-US", "zh-CN"],
  }, "zh-CN")
  try {
    expect(page.report).toHaveBeenLastCalledWith(null)
    const consent = page.container.querySelectorAll<HTMLButtonElement>('[role="checkbox"]')[1]
    expect(consent.disabled).toBe(false)
    expect(consent.getAttribute("aria-checked")).toBe("false")
    await act(async () => consent.click())
    expect(page.report).toHaveBeenLastCalledWith(expect.objectContaining({ piiRegion: "zh-CN" }))
  } finally {
    await page.dispose()
  }
})
