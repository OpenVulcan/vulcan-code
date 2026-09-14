// @vitest-environment jsdom

import * as React from "react"
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"

import { I18nProvider } from "@/i18n"
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench"
import type { VmmMemoryActionRequest } from "@/types"

import { MemoryArea } from "./MemoryArea"
import { maintenanceReply } from "./vmm-native-maintenance.test-fixture"
import { VmmNativeVectorHealthAlert } from "./vmm-native-vector-health-alert"

beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

// The real status envelope must expose a blocking warning and a navigation-only repair action.
// 真实状态封套必须显示阻断警示，以及只导航、不重建的修复入口。
it("shows a global vector blocker and opens the guarded repair page", async () => {
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  const onRunAction = vi.fn(async (_request: VmmMemoryActionRequest) => maintenanceReply("native_vector_status"))
  const onOpenRepair = vi.fn()
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
      <VmmNativeVectorHealthAlert
        onRunAction={onRunAction}
        onOpenRepair={onOpenRepair}
        onOpenEmbeddingSettings={vi.fn()}
      />
    </I18nProvider>))
    expect(onRunAction).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
      action: "native_vector_status", scope: "global", projectId: null,
    }))
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("记忆读写已被向量代次阻断")
    expect(container.textContent).toContain("1 条活动记忆")
    const repairButton = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("前往向量重建"))!
    await act(async () => repairButton.click())
    expect(onOpenRepair).toHaveBeenCalledOnce()
    expect(onRunAction).toHaveBeenCalledTimes(1)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})

// A clicked warning must land on the vector tab through the real memory workspace navigation.
// 点击警示后必须经真实记忆工作区导航，直达向量页。
it("navigates from the workspace warning to the vector rebuild preview", async () => {
  const snapshot = browserFallbackSnapshot().vmmMemory
  snapshot.experienceState.deployment = "native"
  const onRunVmmAction = vi.fn(async (request: VmmMemoryActionRequest) => maintenanceReply(request.action))
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  // Harness retains the same MemoryArea instance while the active section changes.
  // Harness 在活动分区切换时保留同一个 MemoryArea 实例。
  function Harness() {
    const [section, setSection] = React.useState("overview")
    return <I18nProvider initialLocale="zh-CN"><MemoryArea
      activeSection={section}
      actions={{ onRunVmmAction, onLoadProjectVmmConfig: async () => snapshot.projectDraft, onSaveVmmConfig: async () => {} }}
      onOpenCredentialManagement={() => {}}
      onSelectSection={setSection}
      snapshot={snapshot}
    /></I18nProvider>
  }
  try {
    await act(async () => root.render(<Harness />))
    const repairButton = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("前往向量重建"))!
    await act(async () => repairButton.click())
    const indexTab = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "向量与检索索引")!
    expect(indexTab.getAttribute("aria-pressed")).toBe("true")
    expect(container.textContent).toContain("预检向量重建")
    expect(onRunVmmAction.mock.calls.some(([request]) => request.action === "native_vector_rebuild_preview")).toBe(false)
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
