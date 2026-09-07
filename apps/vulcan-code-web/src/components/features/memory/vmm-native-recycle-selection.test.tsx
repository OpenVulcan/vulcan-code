// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRecycleBatchSummary, VmmNativeRemovalTarget } from "@/types"
import { VmmNativeMaintenancePage } from "./vmm-native-maintenance-page"

// Declare React's act environment for this isolated test file, then restore the prior globals.
// 为此隔离测试文件声明 React act 环境，然后恢复原全局值。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

it.each([
  ["manual_session_recycle", { kind: "sessions", ids: ["empty-original-session"] }, "人工回收会话"],
  ["manual_turn_recycle", { kind: "turns", ids: ["vmm-turn-10"] }, "人工回收回合"],
] satisfies [string, VmmNativeRemovalTarget, string][])("shows original %s identities even when body counts are zero", async (reason, manualSelection, label) => {
  // Empty counts deliberately cannot reconstruct the selected source identity.
  // 空计数刻意无法重建所选来源身份。
  const batch: VmmNativeRecycleBatchSummary = { batchId: "manual:plan", projectId: 7, reason, manualSelection, recycledAtUnixMs: 1, expiresAtUnixMs: 100, turnCount: 0, memoryCount: 0, profileNodeCount: 0, contextEvidenceCount: 0 }
  // Other independent maintenance status failures must not hide a successfully loaded recycle batch.
  // 其他独立维护状态失败不能隐藏已成功加载的回收批次。
  const onRunAction = vi.fn(async (action: string): Promise<VmmMemoryActionResult | null> => {
    if (action !== "native_recycle_bin") return null
    const value = { storageVersion: 4, batches: [batch] }
    return { action, kind: action, request: {}, summary: "", response: value, data: { kind: "native_recycle_bin", value } }
  })
  // The real page owns loading and rendering; the test never exports a private card just for coverage.
  // 真实页面拥有加载及渲染；测试不为覆盖而导出私有卡片。
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeMaintenancePage busy={false} onRunAction={onRunAction} /></I18nProvider>))
    expect(container.textContent).toContain(label)
    expect(container.textContent).not.toContain(reason)
    expect(container.querySelector("summary")?.textContent).toBe("查看原始人工选择 (1)")
    expect(container.querySelector("details")?.textContent).toContain(manualSelection.ids[0])
    expect(Array.from(container.querySelectorAll("button")).some((button) => button.textContent === "恢复")).toBe(true)
    expect(container.textContent).toContain("永久清除")
    expect(onRunAction.mock.calls.some(([action]) => action === "native_recycle_restore" || action === "native_recycle_purge")).toBe(false)
  } finally { await act(async () => root.unmount()); container.remove() }
})
