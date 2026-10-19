// @vitest-environment jsdom
import { act, StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench"
import type { VmmMemoryActionResponse, VmmMemoryActionResult } from "@/types"
import { VmmProfilePage } from "./vmm-profile-page"
import { MemoryArea } from "./memory-area"

// result preserves the exact read union while shared result feedback deliberately remains null.
// result 保留准确读取联合，同时共享结果反馈刻意始终为空。
function result(action: string, data: Exclude<VmmMemoryActionResponse, { kind: "empty" }>): VmmMemoryActionResult {
  return { action, kind: action, summary: "", request: {}, response: data.value, data }
}

// Verifies direct response ownership, failure visibility and explicit recovery without a parent result event.
// 验证直接响应所有权、失败可见性及显式恢复，整个过程没有父级结果事件。
it("loads a native profile independently of shared feedback and exposes read failures until retry", async () => {
  // snapshot declares native deployment with a real typed synthetic catalog response below.
  // snapshot 声明原生部署，下面通过真实类型提供合成目录响应。
  const snapshot = browserFallbackSnapshot().vmmMemory
  snapshot.experienceState.deployment = "native"
  let failure: "catalog" | "profile" | null = "catalog"
  const onRunAction = vi.fn(async (action: string): Promise<VmmMemoryActionResult | null> => {
    if (action === "native_project_catalog") return failure === "catalog" ? null : result(action, { kind: "native_project_catalog", value: { projects: [{ projectId: 7, name: "可用项目", tag: "host", protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }], bindings: [] } })
    if (action === "native_profile_records") return failure === "profile" ? null : result(action, { kind: "native_profile_records", value: { projectId: 7, storageVersion: 1, target: "user", nodes: [], counts: { active: 0, inactive: 0, scheduled: 0, pending: 0, expired: 0 } } })
    if (action === "load_profile_bundle") return result(action, { kind: "profile_bundle", value: { mode: "PROFILE_BUNDLE_MODE_SPLIT", combined_text: "", user_profile: "", project_profile: "", trace_id: "synthetic" } })
    throw new Error(`Unexpected action ${action}`)
  })
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  // refresh uses the actual rendered action, keeping retries explicit and user controlled.
  // refresh 使用实际渲染操作，确保重试显式且由用户控制。
  const refresh = () => [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.includes("刷新"))!.click()
  try {
    await act(async () => root.render(<StrictMode><I18nProvider initialLocale="zh-CN"><VmmProfilePage snapshot={snapshot} busy={false} result={null} onRunAction={onRunAction} /></I18nProvider></StrictMode>))
    expect(container.textContent).toContain("记忆项目目录加载失败")
    expect(container.querySelector('[data-slot="empty"]')).toBeNull()
    failure = "profile"
    await act(async () => refresh())
    expect(container.textContent).toContain("画像读取失败")
    expect([...container.querySelectorAll<HTMLButtonElement>(".vk-page-tabs nav button")].every((tab) => !tab.disabled)).toBe(true)
    expect(container.querySelector('[data-slot="empty"]')).toBeNull()
    failure = null
    await act(async () => refresh())
    expect(container.querySelector('[role="alert"]')).toBeNull()
    expect(container.querySelector('[data-slot="empty"]')).not.toBeNull()
    expect(onRunAction.mock.calls.some(([action]) => action === "load_profile_bundle")).toBe(true)
  } finally { await act(async () => root.unmount()); container.remove() }
})

// Exercise real page/controller ownership under effect replay instead of an unconstrained action stub.
// 在副作用重放下验证真实页面与控制器所有权，而非使用不受互斥约束的动作桩。
it("loads the native profile on first entry through the real exclusive controller in StrictMode", async () => {
  // snapshot and actions are isolated fixture data; no real project or memory is modified.
  // snapshot 与 actions 均为隔离夹具数据，不修改真实项目或记忆。
  const snapshot = browserFallbackSnapshot().vmmMemory
  snapshot.experienceState.deployment = "native"
  const onRunVmmAction = vi.fn(async ({ action }: { action: string }): Promise<VmmMemoryActionResult> => {
    if (action === "native_project_catalog") return result(action, { kind: "native_project_catalog", value: { projects: [{ projectId: 7, name: "可用项目", tag: "host", protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }], bindings: [] } })
    if (action === "native_profile_records") return result(action, { kind: "native_profile_records", value: { projectId: 7, storageVersion: 1, target: "user", nodes: [], counts: { active: 0, inactive: 0, scheduled: 0, pending: 0, expired: 0 } } })
    if (action === "load_profile_bundle") return result(action, { kind: "profile_bundle", value: { mode: "PROFILE_BUNDLE_MODE_SPLIT", combined_text: "", user_profile: "", project_profile: "", trace_id: "synthetic" } })
    throw new Error(`Unexpected action ${action}`)
  })
  // container and root own one complete memory page lifecycle with the production controller.
  // container 与 root 持有使用生产控制器的完整记忆页面生命周期。
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
  try {
    await act(async () => root.render(<StrictMode><I18nProvider initialLocale="zh-CN"><MemoryArea
      activeSection="profile" snapshot={snapshot} onSelectSection={() => {}} onOpenCredentialManagement={() => {}}
      actions={{ onRunVmmAction, onLoadProjectVmmConfig: async () => snapshot.projectDraft, onSaveVmmConfig: async () => {} }}
    /></I18nProvider></StrictMode>))
    expect(container.textContent).not.toContain("加载失败")
    expect(container.querySelector('[data-slot="empty"]')).not.toBeNull()
    expect(onRunVmmAction.mock.calls.map(([request]) => request.action)).toEqual([
      "native_project_catalog", "native_profile_records", "load_profile_bundle",
    ])
  } finally { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals() }
})
