// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeRecycleBatchSummary, VmmNativeRecycleRecord, VmmNativeRecycleRecordQuery } from "@/types"
import { VmmNativeRecycleRecords } from "./vmm-native-recycle-records"

// Restore the React act flag so the real Radix interactions remain isolated from other test files.
// 恢复 React act 标志，使真实 Radix 交互与其他测试文件隔离。
const actDescriptor = Object.getOwnPropertyDescriptor(globalThis, "IS_REACT_ACT_ENVIRONMENT")
beforeAll(() => Object.defineProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT", { configurable: true, writable: true, value: true }))
afterAll(() => {
  if (actDescriptor) Object.defineProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT", actDescriptor)
  else Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
})

// A fixed batch exposes every retained family without any active-record API.
// 固定批次公开全部保留类别，不使用任何活动记录 API。
const batch: VmmNativeRecycleBatchSummary = { manualSelection: null, batchId: "batch-a", projectId: 7, reason: "idle_session_recycle", recycledAtUnixMs: 1, expiresAtUnixMs: 100, turnCount: 21, memoryCount: 1, profileNodeCount: 1, contextEvidenceCount: 1 }

// Mounts the actual standard dialog with deterministic typed page responses and explicit teardown.
// 使用确定性类型化分页响应挂载实际标准对话框，并提供明确清理。
async function mount(failNext = false, busy = false) {
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    expect(action).toBe("native_recycle_records")
    if (failNext && payload!.offset === 20) return null
    const query: VmmNativeRecycleRecordQuery = { vmmProjectId: 7, batchId: "batch-a", recordKind: "turn", offset: Number(payload!.offset), limit: 20, expectedStorageVersion: payload!.expectedStorageVersion === null ? null : Number(payload!.expectedStorageVersion) }
    let records: VmmNativeRecycleRecord[] = []
    switch (payload!.recordKind) {
      case "turn": records = Array.from({ length: query.offset === 0 ? 20 : 1 }, (_, index) => ({ kind: "turn", turnId: `turn-${query.offset + index}`, sessionSourceId: "source-session", timeline: [{ role: "user", content: `保留原文 ${query.offset + index}`, occurredAtUnixMs: 1 }], status: "completed", analysisDetails: "已提取事实" })); break
      case "memory": query.recordKind = "memory"; records = [{ kind: "memory", memoryId: "memory-a", sessionSourceId: "source-session", sourceTurnId: "turn-0", summary: "数据库决定", content: "使用 PostgreSQL", level: "stable", state: "active", sourceKind: "turn" }]; break
      case "profile": query.recordKind = "profile"; records = [{ kind: "profile", profileNodeId: "profile-a", target: "user", sourceTurnIds: ["turn-0"], content: "用户喜欢西瓜" }]; break
      case "evidence": query.recordKind = "evidence"; records = [{ kind: "evidence", memoryId: "memory-a", contextKey: "fruit", contextValue: "西瓜", supportCount: 4, rebuttalCount: 2 }]; break
      default: throw new Error("unexpected record family")
    }
    const value = { query, storageVersion: 41, total: query.recordKind === "turn" ? 21 : 1, nextOffset: query.recordKind === "turn" && query.offset === 0 ? 20 : null, records }
    return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_recycle_records", value } }
  })
  const container = document.createElement("div")
  const root = createRoot(container)
  document.body.append(container)
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRecycleRecords batch={batch} busy={busy} onRunAction={onRunAction} /></I18nProvider>))
  return { container, onRunAction,
    // A batch can keep its ID across an explicit project migration.
    // 显式项目迁移后批次可以保留原 ID。
    replaceBatch: async (nextBatch: VmmNativeRecycleBatchSummary) => { await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeRecycleRecords batch={nextBatch} busy={busy} onRunAction={onRunAction} /></I18nProvider>)) },
    dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

// Finds only a button belonging to the visible standard dialog, never a hidden sibling action.
// 只查找可见标准对话框中的按钮，不选择隐藏的相邻动作。
function dialogButton(label: string): HTMLButtonElement {
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find((item) => item.textContent === label)
  expect(button).toBeDefined()
  return button!
}

it("loads lazily and preserves the observed version when paging", async () => {
  const view = await mount()
  try {
    expect(view.onRunAction).not.toHaveBeenCalled()
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(view.onRunAction).toHaveBeenCalledExactlyOnceWith("native_recycle_records", { vmmProjectId: 7, batchId: "batch-a", recordKind: "turn", offset: 0, limit: 20, expectedStorageVersion: null })
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("保留原文 0")
    await act(async () => dialogButton("下一页").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_recycle_records", { vmmProjectId: 7, batchId: "batch-a", recordKind: "turn", offset: 20, limit: 20, expectedStorageVersion: 41 })
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("保留原文 20")
    expect(dialogButton("下一页").disabled).toBe(true)
    await act(async () => dialogButton("上一页").click())
    expect(view.onRunAction.mock.calls.at(-1)?.[1]?.expectedStorageVersion).toBe(41)
    await act(async () => document.querySelector('[role="dialog"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })))
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).toBeNull())
    await vi.waitFor(() => expect(document.activeElement).toBe(view.container.querySelector("button")))
  } finally { await view.dispose() }
})

it("shows each retained family as business text without restore or purge requests", async () => {
  const view = await mount()
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    for (const [label, expected] of [["记忆", "使用 PostgreSQL"], ["画像节点", "用户喜欢西瓜"], ["情境证据", "反驳: 2"]]) {
      await act(async () => dialogButton(label).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })))
      expect(document.querySelector('[role="dialog"]')?.textContent).toContain(expected)
    }
    expect(view.onRunAction.mock.calls.every(([action]) => action === "native_recycle_records")).toBe(true)
  } finally { await view.dispose() }
})

it("clears stale content after a rejected page and requires an explicit fresh reload", async () => {
  const view = await mount(true)
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    await act(async () => dialogButton("下一页").click())
    expect(document.querySelector('[role="alert"]')?.textContent).toContain("请刷新重试")
    expect(document.querySelector('[role="dialog"]')?.textContent).not.toContain("保留原文")
    await act(async () => dialogButton("刷新明细").click())
    expect(view.onRunAction.mock.calls.at(-1)?.[1]?.expectedStorageVersion).toBeNull()
  } finally { await view.dispose() }
})

it("does not fetch while the parent is busy", async () => {
  const view = await mount(false, true)
  try {
    const button = view.container.querySelector<HTMLButtonElement>("button")!
    expect(button.disabled).toBe(true)
    await act(async () => button.click())
    expect(view.onRunAction).not.toHaveBeenCalled()
  } finally { await view.dispose() }
})

it("closes retained details when migration changes the owner but preserves the batch ID", async () => {
  const view = await mount()
  try {
    await act(async () => view.container.querySelector<HTMLButtonElement>("button")!.click())
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    await view.replaceBatch({ ...batch, projectId: 9 })
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(view.onRunAction).toHaveBeenCalledTimes(1)
  } finally { await view.dispose() }
})
