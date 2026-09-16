// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResponse, VmmMemoryActionResult, VmmNativeResourceCatalogSummary } from "@/types"
import { VmmNativeResourcesPage } from "./vmm-native-resources-page"

// Verify all resource groups retain their editor and file list while a delayed file read completes.
// 验证所有资源分类在延迟读取文件期间保留编辑器和文件列表。
it.each(["prompt", "pii", "noise"] as const)("keeps the %s workspace mounted when selecting a file", async (group) => {
  // catalog provides two files in the tested group without accessing user resources.
  // catalog 为待测分类提供两个文件，不访问用户资源。
  const catalog: VmmNativeResourceCatalogSummary = {
    overrideRoot: "/isolated",
    settings: { promptBundle: "default", piiLanguage: "default", noiseLanguage: "default", noiseEnabled: true, noiseSemanticEnabled: false, noiseSemanticThreshold: .5 },
    promptBundles: ["default"], promptScenes: [], piiLanguages: ["default"], noiseLanguages: ["default"],
    files: ["first.json", "second.json"].map((fileName) => ({ path: `${group}/${fileName}`, fileName, group, selector: "default", highRisk: true, hasUserOverride: false })),
  }
  // completeRead resolves the pending second response after assertions inspect the loading DOM.
  // completeRead 在断言检查加载中的 DOM 后完成第二次响应。
  let completeRead!: (result: VmmMemoryActionResult) => void
  // reply maps the requested path to a typed resource file result.
  // reply 将请求路径映射为强类型资源文件结果。
  const reply = (path: string): VmmMemoryActionResult => envelope("native_resource_read", { kind: "native_resource_file", value: { path, builtInText: path, effectiveText: path, userOverrideText: null, hasUserOverride: false, highRisk: true } })
  // onRunAction delays only the second file read and returns the catalog immediately.
  // onRunAction 仅延迟第二个文件的读取，立即返回目录。
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult> => {
    if (action === "native_resource_catalog") return envelope(action, { kind: "native_resource_catalog", value: catalog })
    if (action === "native_resource_read" && payload?.path === catalog.files[0].path) return reply(catalog.files[0].path)
    if (action === "native_resource_read" && payload?.path === catalog.files[1].path) return new Promise((resolve) => { completeRead = resolve })
    throw new Error(`Unexpected resource action ${action}`)
  })
  // container and root host the real resource page throughout both reads.
  // container 与 root 在两次读取期间持续承载真实资源页面。
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeResourcesPage busy={false} initialGroup={group} onRunAction={onRunAction} /></I18nProvider>))
    // editor and fileList capture identity so an unmount cannot masquerade as equal markup.
    // editor 与 fileList 捕获节点身份，防止相同标记掩盖卸载重建。
    const editor = container.querySelector("textarea")!
    const fileList = container.querySelector(".vmm-resource-file-groups")!
    // controls preserve both node identity and disabled styling across the pending read.
    // controls 在等待读取期间保持节点身份与禁用样式不变。
    const controls = [...container.querySelectorAll<HTMLButtonElement | HTMLInputElement>("button, input")]
    const disabledStates = controls.map((control) => control.matches(":disabled"))
    expect(editor.value).toBe(catalog.files[0].path)
    fileList.scrollTop = 80
    // secondFile selects the real file button instead of manipulating component state.
    // secondFile 选择真实文件按钮，不操纵组件内部状态。
    const secondFile = [...fileList.querySelectorAll("button")].find((button) => button.textContent?.includes("second.json"))!
    await act(async () => secondFile.click())
    expect(container.querySelector("textarea")).toBe(editor)
    expect(editor.value).toBe(catalog.files[0].path)
    expect(editor.readOnly).toBe(true)
    expect(secondFile.matches(":disabled")).toBe(false)
    expect(container.querySelector("fieldset")?.hasAttribute("inert")).toBe(true)
    expect([...container.querySelectorAll("button, input")]).toEqual(controls)
    expect(controls.map((control) => control.matches(":disabled"))).toEqual(disabledStates)
    expect(container.querySelector("fieldset")?.getAttribute("aria-busy")).toBe("true")
    expect(container.querySelector(".vmm-resource-file-groups")).toBe(fileList)
    expect(fileList.scrollTop).toBe(80)
    await act(async () => completeRead(reply(catalog.files[1].path)))
    expect(container.querySelector("textarea")).toBe(editor)
    expect(editor.value).toBe(catalog.files[1].path)
    expect(secondFile.matches(":disabled")).toBe(false)
    expect(container.querySelector("fieldset")?.hasAttribute("inert")).toBe(false)
    expect([...container.querySelectorAll("button, input")]).toEqual(controls)
    expect(controls.map((control) => control.matches(":disabled"))).toEqual(disabledStates)
    expect(fileList.scrollTop).toBe(80)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

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
