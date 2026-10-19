// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider, readLocaleMessage, type TranslationKey } from "@/i18n"
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench"
import type { VmmMemoryActionRequest } from "@/types"
import { MemoryArea } from "./memory-area"
import { maintenanceReply } from "./vmm-native-maintenance.test-fixture"

// Restore the React test environment after the real controller integration checks.
// 在真实控制器集成检查后恢复 React 测试环境。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

/** Finds a visible button by catalog key inside scope and returns its DOM element.
 * 在 scope 内按目录键寻找可见按钮，并返回其 DOM 元素。
 * key identifies the existing UI label; a missing control fails the test explicitly.
 * key 标识既有界面标签；控件缺失时明确使测试失败。
 */
function button(key: TranslationKey, scope: ParentNode = document): HTMLButtonElement {
  // label comes from the production catalog rather than a wording snapshot.
  // label 来自生产目录，不是文案快照。
  const label = readLocaleMessage("en-US", key)
  // found excludes controls retained inside inactive tabs.
  // found 排除保留在非活动页签内的控件。
  const found = [...scope.querySelectorAll<HTMLButtonElement>("button")]
    .find((entry) => entry.textContent === label && !entry.closest("[hidden]"))
  if (!found) throw new Error(`Missing control for ${key}`)
  return found
}

// Exercise the real controller boundary for all four vector operations, including explicit retry.
// 对四种向量操作验证真实控制器边界，并覆盖用户显式重试。
it.each([
  ["native_vector_status", null],
  ["native_vector_rebuild_preview", null],
  ["native_vector_adoption", "memory.maintenance.keepVectors"],
  ["native_vector_rebuild", "memory.maintenance.rebuildVectors"],
] as const)("preserves service failure for %s without replaying the request", async (action, confirmKey) => {
  // snapshot selects the native implementation without a live backend or stored user data.
  // snapshot 选择原生实现，不连接真实后端或使用持久化用户数据。
  const snapshot = browserFallbackSnapshot().vmmMemory
  snapshot.experienceState.deployment = "native"
  // diagnostic contains the actual public transport shape without binding human wording.
  // diagnostic 包含公开传输结构，不绑定面向用户的措辞。
  const diagnostic = "Native VMM maintenance embedding failed: HTTP 503 / billing_unavailable: synthetic failure"
  // rejectAction controls one explicit failure and then permits a user-initiated retry.
  // rejectAction 控制一次明确失败，随后允许用户主动重试。
  let rejectAction = true
  // run is the service boundary; the production controller must forward this rejection to the page.
  // run 是服务边界；生产控制器必须将此拒绝传递到页面。
  const run = vi.fn(async (request: VmmMemoryActionRequest) => {
    if (request.action === action && rejectAction) throw new Error(diagnostic)
    // The controller constructs these maintenance payloads from Record<string, unknown>.
    // 控制器使用 Record<string, unknown> 构造这些维护载荷。
    return maintenanceReply(request.action, request.payload as Record<string, unknown>)
  })
  // container and root own one isolated mounted workspace and its cleanup.
  // container 和 root 持有一个隔离挂载的工作区及其清理边界。
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><MemoryArea
      activeSection="maintenance"
      actions={{ onRunVmmAction: run, onLoadProjectVmmConfig: async () => snapshot.projectDraft, onSaveVmmConfig: async () => {} }}
      onOpenCredentialManagement={() => {}}
      onSelectSection={() => {}}
      snapshot={snapshot}
    /></I18nProvider>))
    await act(async () => button("memory.workspace.index", container).click())
    if (action !== "native_vector_status") {
      await act(async () => button("memory.maintenance.previewVectorRebuild", container).click())
    }
    if (confirmKey) await act(async () => button(confirmKey).click())
    // surface keeps modal failures in their originating confirmation.
    // surface 将弹窗失败保留在其发起的确认界面内。
    const surface = confirmKey ? document.querySelector('[role="alertdialog"]')! : container
    expect(surface.textContent).toContain(diagnostic)
    expect(surface.textContent).not.toContain(readLocaleMessage("en-US", "memory.maintenance.invalidResponse"))
    expect(run.mock.calls.filter(([request]) => request.action === action)).toHaveLength(1)
    if (confirmKey) expect(button(confirmKey, surface).disabled).toBe(false)
    rejectAction = false
    await act(async () => button(confirmKey ?? (action === "native_vector_status"
      ? "memory.resources.refresh" : "memory.maintenance.previewVectorRebuild"), surface).click())
    expect(run.mock.calls.filter(([request]) => request.action === action)).toHaveLength(2)
    expect(document.body.textContent).not.toContain(diagnostic)
    if (confirmKey) expect(document.querySelector('[role="alertdialog"]')).toBeNull()
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
