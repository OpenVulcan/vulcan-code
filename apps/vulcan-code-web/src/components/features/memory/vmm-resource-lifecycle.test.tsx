// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResponse, VmmMemoryActionResult, VmmNativeResourceCatalogSummary } from "@/types"
import { VmmNativeResourcesPage } from "./vmm-native-resources-page"

// envelope preserves typed native resource replies without installing a runtime or touching resource files.
// envelope 保留强类型原生资源回复，不安装运行时或触碰资源文件。
function envelope(action: string, data: Exclude<VmmMemoryActionResponse, { kind: "empty" }>): VmmMemoryActionResult {
  return { action, kind: action, summary: "", request: {}, response: data.value, data }
}

// Explicit refresh retries a failed current resource read; a failed reset keeps confirmation and its error visible.
// 显式刷新重试失败的当前资源读取；重置失败保持确认及错误可见。
it("retries the current resource and retains a rejected reset confirmation", async () => {
  // catalog contains one clearly synthetic overridden prompt, matching the actual file contract.
  // catalog 包含一份明确合成的覆盖提示词，并匹配实际文件契约。
  const catalog: VmmNativeResourceCatalogSummary = { overrideRoot: "/isolated", settings: { promptBundle: "default", piiLanguage: "zh", noiseLanguage: "zh", noiseEnabled: true, noiseSemanticEnabled: false, noiseSemanticThreshold: .5 }, promptBundles: ["default"], promptScenes: [], piiLanguages: ["zh"], noiseLanguages: ["zh"], files: [{ path: "prompt/default/precheck_l1_main.md", fileName: "precheck_l1_main.md", group: "prompt", selector: "default", highRisk: true, hasUserOverride: true }] }
  // readCount admits only the second read, making same-file refresh behavior observable.
  // readCount 仅准入第二次读取，使同文件刷新行为可观察。
  let readCount = 0
  const onRunAction = vi.fn(async (action: string): Promise<VmmMemoryActionResult | null> => {
    if (action === "native_resource_catalog") return envelope(action, { kind: "native_resource_catalog", value: catalog })
    if (action === "native_resource_read") return ++readCount === 1 ? null : envelope(action, { kind: "native_resource_file", value: { path: catalog.files[0].path, builtInText: "built-in fixture", effectiveText: "synthetic override", userOverrideText: "synthetic override", hasUserOverride: true, highRisk: true } })
    if (action === "native_resource_reset") return null
    throw new Error(`Unexpected resource action ${action}`)
  })
  // container hosts the production resource editor and normal Radix confirmation portal.
  // container 承载生产资源编辑器及正常 Radix 确认门户。
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  // click resolves the actual localized action instead of accessing private React state.
  // click 解析实际本地化操作，不访问 React 私有状态。
  const click = (scope: ParentNode, label: string) => {
    const button = [...scope.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent?.includes(label))
    if (!button) throw new Error(`Missing action ${label}`)
    button.click()
  }
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeResourcesPage busy={false} onRunAction={onRunAction} /></I18nProvider>))
    expect(readCount).toBe(1)
    await act(async () => click(container, "刷新"))
    expect(readCount).toBe(2)
    expect(container.querySelector("textarea")?.value).toBe("synthetic override")
    await act(async () => click(container, "恢复内置"))
    // affirmative is the actual destructive confirmation button, not the cancel control.
    // affirmative 是实际破坏性确认按钮，而非取消控件。
    const affirmative = document.querySelector<HTMLButtonElement>('[role="alertdialog"] [data-slot="alert-dialog-action"]')
    expect(affirmative).not.toBeNull()
    await act(async () => affirmative!.click())
    expect(document.querySelector('[role="alertdialog"]')).not.toBeNull()
    expect(document.querySelector('[role="alertdialog"] [role="alert"]')).not.toBeNull()
    expect(container.querySelector("textarea")?.value).toBe("synthetic override")
  } finally { await act(async () => root.unmount()); container.remove() }
})
