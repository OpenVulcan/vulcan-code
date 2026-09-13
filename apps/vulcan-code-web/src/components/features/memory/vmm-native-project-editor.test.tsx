// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeProjectCatalogSummary } from "@/types"
import { VmmNativeProjectPage } from "./vmm-native-project-page"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"

// Verifies a catalog refresh cannot discard the selected host's unsaved owner edit.
// 验证目录刷新不能丢弃选中宿主尚未保存的归属编辑。
it("preserves an unsaved binding across catalog refresh and projects without persisting implicitly", async () => {
  // Both host identities intentionally share their display name; paths remain distinct.
  // 两个宿主身份刻意共享显示名称；路径仍然不同。
  const projects = ["a", "b"].map((id) => ({ projectId: id, name: "同名项目", path: `D:/${id}`, hasOverride: id === "b", boundProjectId: null, memoryEnabled: true }))
  const catalog: VmmNativeProjectCatalogSummary = { projects: [1, 7].map((projectId) => ({ projectId, name: `记忆 ${projectId}`, tag: `tag-${projectId}`, protected: projectId === 1, ownerVulcanProjectIds: projectId === 7 ? ["a"] : [], linkedVulcanProjectIds: [] })), bindings: [{ vulcanProjectId: "a", ownerProjectId: 7, linkedProjectIds: [] }] }
  const onRunAction = vi.fn(async (action: string): Promise<VmmMemoryActionResult> => ({ action, kind: action, summary: "", request: {}, response: catalog, data: { kind: "native_project_catalog", value: catalog } }))
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeProjectPage busy={false} notice="" projects={projects} onRunAction={onRunAction} /></I18nProvider>))
    // The binding editor uses the existing Select while the responsive directory has a separate picker.
    // 绑定编辑器使用既有 Select，响应式目录拥有独立选择器。
    const owner = container.querySelector<HTMLButtonElement>('[data-slot="select-trigger"]')!
    await chooseMemoryOption(owner, "记忆 1")
    expect(container.textContent).toContain("有未保存变更")
    const refresh = Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.includes("刷新"))!
    await act(async () => refresh.click())
    expect(container.querySelector('[data-slot="select-trigger"]')?.textContent).toContain("记忆 1")
    expect(container.textContent).toContain("有未保存变更")
    // Directory switching keeps the host-owned draft without causing a service mutation.
    // 目录切换保留宿主持有的草稿，不引发服务修改。
    const rows = container.querySelectorAll<HTMLButtonElement>(".vmm-project-list button")
    await act(async () => rows[1].click())
    await act(async () => rows[0].click())
    expect(container.querySelector('[data-slot="select-trigger"]')?.textContent).toContain("记忆 1")
    expect(onRunAction.mock.calls.every(([action]) => action === "native_project_catalog")).toBe(true)
  } finally { await act(async () => root.unmount()); container.remove() }
})
