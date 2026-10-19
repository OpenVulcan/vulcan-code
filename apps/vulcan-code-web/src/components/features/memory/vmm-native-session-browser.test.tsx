// @vitest-environment jsdom
import { act, StrictMode } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResponse, VmmMemoryActionResult, VmmNativeMemorySessionSummary, VmmNativeProjectCatalogSummary, VmmNativeSessionListQuery, VmmNativeRemovalQuery, VmmNativeRemovalImpact, VmmNativeTurnRecordSummary } from "@/types"
import { nativeSessionDateBound, nativeSessionInitialQuery, VmmNativeSessionBrowser } from "./vmm-native-session-browser"

// The bound project is not the protected default; names deliberately differ from stable identities.
// 绑定项目不是受保护默认项目；名称刻意与稳定身份不同。
const catalog: VmmNativeProjectCatalogSummary = {
  projects: [1, 7].map((projectId) => ({ projectId, name: `项目 ${projectId}`, tag: `tag-${projectId}`, protected: projectId === 1, ownerVulcanProjectIds: projectId === 7 ? ["host"] : [], linkedVulcanProjectIds: [] })),
  bindings: [{ vulcanProjectId: "host", ownerProjectId: 7, linkedProjectIds: [] }],
}

// Builds summary-only rows, including a completed empty extraction rather than invented memories.
// 构建仅摘要的行，包括已完成的空提取，不伪造记忆。
function session(index: number): VmmNativeMemorySessionSummary {
  return { sessionSourceId: `source-${index}`, displayTitle: `显示标题 ${index}`, status: "active", turnCount: 1, turnStatuses: { pending: 0, completed: 1, processedUnknown: 0, analysisBypassed: 0, corrupted: 0 }, memoryCount: 0, directMemoryCount: 0, profileNodeCount: 0, memoryLevels: { session: 0, phase: 0, stable: 0, persistent: 0 } }
}

// Wraps the closed result union without loosening its shape for tests.
// 封装封闭的结果联合，不为测试放宽结构。
function result(action: string, data: Exclude<VmmMemoryActionResponse, { kind: "empty" }>): VmmMemoryActionResult {
  return { action, kind: action, summary: "", request: {}, response: data.value, data }
}

// Builds ordinary source turns with exact numeric IDs, without claiming any extraction output.
// 构造具有准确数字 ID 的普通来源回合，不声称存在提取产物。
function turnRecord(turnId: number): VmmNativeTurnRecordSummary {
  return { turnId, previousTurnIds: [], nextTurnIds: [], sessionSourceId: "source-0", createdAtUnixMs: 1, status: "pending", userPreview: `原始回合 ${turnId}`, assistantPreview: "", timeline: [], generatedMemoryIds: [], generatedProfileNodeIds: [], memoryLevels: { session: 0, phase: 0, stable: 0, persistent: 0 }, analysisAttemptCount: 0 }
}

// Mounts actual browser, archive dialog and provenance components with controlled service responses.
// 通过受控服务响应挂载实际浏览器、归档对话框及来源组件。
async function mount(options: { stale?: boolean; failCatalog?: boolean; failDetail?: boolean; mixed?: boolean; busy?: boolean; turns?: boolean; turnCount?: number; updates?: boolean } = {}) {
  // Captures the previewed server impact so confirmation cannot replace the selected source IDs.
  // 捕获已预览的服务端影响，使确认不能替换所选来源 ID。
  let removalImpact: VmmNativeRemovalImpact | null = null
  const onRunAction = vi.fn(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    if (action === "native_project_catalog") return options.failCatalog ? null : result(action, { kind: "native_project_catalog", value: catalog })
    if (action === "native_session_list") {
      const query = payload as unknown as VmmNativeSessionListQuery
      if (options.stale && query.expectedStorageVersion !== null) return null
      const rows = Array.from({ length: query.offset === 0 ? 20 : 1 }, (_, index) => session(index + query.offset))
      if (options.mixed) rows[1].status = "archived"
      return result(action, { kind: "native_session_list", value: { projectId: query.vmmProjectId, storageVersion: 42, total: 21, offset: query.offset, limit: query.limit, sessions: rows } })
    }
    if (action === "native_memory_records") return options.failDetail ? null : result(action, { kind: "native_memory_records", value: { projectId: Number(payload!.vmmProjectId), storageVersion: 42, selectedSessionSourceId: payload!.sessionSourceId as string | undefined, sessions: [session(0)], memories: [], turns: options.turnCount ? Array.from({ length: options.turnCount }, (_, index) => turnRecord(index + 1)) : options.turns ? [turnRecord(2), turnRecord(10)] : [], profileNodes: [] } })
    if (action === "native_removal_preview") {
      const query = payload as unknown as VmmNativeRemovalQuery
      removalImpact = { query, storageVersion: 42, hotWindowSize: 3, sessionSourceIds: query.selection.kind === "sessions" ? query.selection.ids : ["source-0"], turnIds: query.selection.kind === "turns" ? query.selection.ids : [], pendingTurnIds: [], hotWindowTurnIds: [], memoryIds: [], vectorMemoryIds: [], projectProfileNodeIds: [], userProfileNodeIds: [], contextEvidence: [] }
      return result(action, { kind: "native_removal_preview", value: { previewId: "manual-preview", trashRetentionMs: 86_400_000, impact: removalImpact } })
    }
    if (action === "native_removal_recycle") {
      if (!removalImpact) throw new Error("Missing removal preview")
      return result(action, { kind: "native_removal", value: { storageVersion: 43, batchId: "manual:manual-preview", impact: removalImpact } })
    }
    if (action === "native_session_archive_preview") return result(action, { kind: "native_session_archive_preview", value: { previewId: "preview-batch", storageVersion: 42, selection: { vmmProjectId: Number(payload!.vmmProjectId), sessionSourceIds: payload!.sessionSourceIds as string[], archived: Boolean(payload!.archived) } } })
    if (action === "native_session_archive") return result(action, { kind: "native_session_archive", value: { storageVersion: 43, selection: { vmmProjectId: 7, sessionSourceIds: ["source-0", "source-1"], archived: true } } })
    throw new Error(`Unexpected action ${action}`)
  })
  const container = document.createElement("div")
  // Read-only background checks use a separate injected channel and never touch foreground action history.
  // 只读后台检查使用独立注入通道，绝不修改前台动作历史。
  const onCheckUpdates = vi.fn(async (query: VmmNativeSessionListQuery) => result("native_session_list", { kind: "native_session_list", value: { projectId: query.vmmProjectId, storageVersion: 43, total: options.updates ? 22 : 21, offset: query.offset, limit: query.limit, sessions: Array.from({ length: 20 }, (_, index) => session(index)) } }))
  const root = createRoot(container)
  document.body.append(container)
  await act(async () => root.render(<StrictMode><I18nProvider initialLocale="zh-CN"><VmmNativeSessionBrowser busy={options.busy ?? false} snapshot={{ selectedProjectId: "host", selectedConversationId: "different-host-session" }} onRunAction={onRunAction} onCheckUpdates={onCheckUpdates} /></I18nProvider></StrictMode>))
  return { container, onRunAction, onCheckUpdates, dispose: async () => { await act(async () => root.unmount()); container.remove() } }
}

// Finds the visible button by its exact user-facing label and returns the real DOM control.
// 根据准确用户文案查找可见按钮并返回真实 DOM 控件。
function button(container: ParentNode, label: string): HTMLButtonElement {
  const found = Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((entry) => entry.textContent === label)
  if (!found) throw new Error(`Button not found: ${label}`)
  return found
}

// Verifies polling never replaces reading position or selection and uses the independent read channel.
// 验证轮询绝不替换阅读位置或选择，并使用独立读取通道。
it("announces background changes without replacing the retained list or foreground state", async () => {
  vi.useFakeTimers()
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible")
  const view = await mount({ updates: true })
  try {
    await act(async () => view.container.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click())
    await act(async () => { await vi.advanceTimersByTimeAsync(15000) })
    expect(view.onCheckUpdates).toHaveBeenCalledTimes(1)
    expect(view.onRunAction).toHaveBeenCalledTimes(2)
    expect(view.container.textContent).toContain("会话有更新，刷新后查看")
    expect(view.container.textContent).toContain("共 21 个会话")
    expect(view.container.textContent).toContain("已选 20 个会话")
    await act(async () => { await vi.advanceTimersByTimeAsync(45000) })
    expect(view.onCheckUpdates).toHaveBeenCalledTimes(1)
  } finally { await view.dispose(); vi.useRealTimers(); vi.restoreAllMocks() }
})

// Pauses list reads while its detail replaces the visible surface, then resumes after returning.
// 详情替换可见列表时暂停列表读取，返回后恢复检查。
it("pauses hidden-list polling while a session detail is open", async () => {
  vi.useFakeTimers()
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible")
  // The real navigation keeps the list mounted, reproducing the hidden timer lifetime.
  // 真实导航保留列表挂载，复现隐藏计时器的生命周期。
  const view = await mount()
  try {
    await act(async () => button(view.container, "显示标题 0").click())
    await act(async () => { await vi.advanceTimersByTimeAsync(45000) })
    expect(view.onCheckUpdates).not.toHaveBeenCalled()
    await act(async () => view.container.querySelector<HTMLButtonElement>(".vmm-session-detail > button")!.click())
    await act(async () => { await vi.advanceTimersByTimeAsync(15000) })
    expect(view.onCheckUpdates).toHaveBeenCalledTimes(1)
    expect(view.container.textContent).toContain("共 21 个会话")
  } finally { await view.dispose(); vi.useRealTimers(); vi.restoreAllMocks() }
})

// Measures a large synthetic response while checking bounded DOM and exact second-page record ownership.
// 测量大规模合成响应，同时检查有界 DOM 及第二页准确记录归属。
it("bounds large session-detail rendering and pages without another full record download", async () => {
  const view = await mount({ turnCount: 5000 })
  try {
    const started = performance.now()
    await act(async () => button(view.container, "显示标题 0").click())
    const detail = view.container.querySelector(".vmm-session-detail")!
    expect(detail.querySelectorAll('.vk-mem-pick > button.vk-mem-tile')).toHaveLength(30)
    expect(detail.textContent).toContain("1–30 / 5000")
    await act(async () => button(detail, "下一页").click())
    expect(detail.textContent).toContain("31–60 / 5000")
    expect(detail.textContent).toContain("原始回合 31")
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_memory_records")).toHaveLength(1)
    console.info(`Native memory synthetic detail: 5000 turns, 30 rendered rows, open + next page ${(performance.now() - started).toFixed(1)}ms`)
  } finally { await view.dispose() }
})

it("loads only a bounded summary page and opens exact source identity without project-body prefetch", async () => {
  const view = await mount()
  try {
    expect(view.onRunAction.mock.calls).toEqual([["native_project_catalog", undefined], ["native_session_list", nativeSessionInitialQuery(7)]])
    expect(view.container.textContent).toContain("共 21 个会话")
    await act(async () => button(view.container, "显示标题 0").click())
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_memory_records")).toEqual([["native_memory_records", { vmmProjectId: 7, sessionSourceId: "source-0" }]])
    expect(view.container.textContent).toContain("返回会话列表")
  } finally { await view.dispose() }
})

it("keeps project-wide provenance explicit and preserves the retained list on return", async () => {
  const view = await mount()
  try {
    await act(async () => button(view.container, "查看项目全部来源").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_memory_records", { vmmProjectId: 7 })
    await act(async () => button(view.container, "返回会话列表").click())
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_session_list")).toEqual([["native_session_list", nativeSessionInitialQuery(7)]])
    expect(view.container.textContent).toContain("共 21 个会话")
    expect(view.container.querySelector(".vmm-session-detail")).toBeNull()
  } finally { await view.dispose() }
})

it("pages using the accepted version and clears selections that are no longer visible", async () => {
  const view = await mount()
  try {
    await act(async () => view.container.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click())
    expect(view.container.textContent).toContain("已选 20 个会话")
    await act(async () => button(view.container, "下一页").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", { ...nativeSessionInitialQuery(7), offset: 20, expectedStorageVersion: 42 })
    expect(view.container.textContent).toContain("已选 0 个会话")
    expect(view.container.textContent).toContain("显示标题 20")
    expect(button(view.container, "下一页").disabled).toBe(true)
    await act(async () => button(view.container, "显示标题 20").click())
    await act(async () => button(view.container, "返回会话列表").click())
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_session_list")).toHaveLength(2)
    expect(view.container.textContent).toContain("显示标题 20")
    expect(button(view.container, "下一页").disabled).toBe(true)
    await act(async () => button(view.container, "上一页").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", { ...nativeSessionInitialQuery(7), expectedStorageVersion: 42 })
  } finally { await view.dispose() }
})

it("shows failed stale pages without old actionable rows and refreshes with no stale version", async () => {
  const view = await mount({ stale: true })
  try {
    await act(async () => button(view.container, "下一页").click())
    expect(view.container.querySelector('[role="alert"]')).not.toBeNull()
    expect(view.container.textContent).not.toContain("显示标题 0")
    expect(view.container.querySelector('input[type="checkbox"]')).toBeNull()
    await act(async () => button(view.container, "刷新记录").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", nativeSessionInitialQuery(7))
    expect(view.container.querySelector('[role="alert"]')).toBeNull()
  } finally { await view.dispose() }
})

it("applies lifecycle, extraction, text, local date and ordering together as a fresh query", async () => {
  const view = await mount()
  try {
    const form = view.container.querySelector("form")!
    const fields = { search: "renamed title", status: "unregistered", extraction: "completed", sort: "title_asc", from: "2026-09-05", before: "2026-09-06" }
    await chooseMemoryOption(form.querySelector<HTMLButtonElement>('[aria-label="会话状态"]')!, "尚未建立提取会话")
    await chooseMemoryOption(form.querySelector<HTMLButtonElement>('[aria-label="提取状态"]')!, "已提取")
    await chooseMemoryOption(form.querySelector<HTMLButtonElement>('[aria-label="排序方式"]')!, "标题顺序")
    await act(async () => {
      for (const [name, value] of Object.entries({ search: fields.search, from: fields.from, before: fields.before })) {
        const field = form.elements.namedItem(name) as HTMLInputElement
        field.value = value
        field.dispatchEvent(new Event("change", { bubbles: true }))
      }
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }))
    })
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", { ...nativeSessionInitialQuery(7), search: fields.search, status: fields.status, extractionStatus: fields.extraction, sort: fields.sort, lastActivityFromUnixMs: new Date(2026, 8, 5).getTime(), lastActivityBeforeUnixMs: new Date(2026, 8, 7).getTime() })
  } finally { await view.dispose() }
})

it("switches projects by ID and resets applied query and visible filter fields", async () => {
  const view = await mount()
  try {
    await chooseMemoryOption(view.container.querySelector<HTMLButtonElement>('[role="combobox"][aria-label="记忆项目"]')!, "项目 1 · tag-1")
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", nativeSessionInitialQuery(1))
    expect(view.container.querySelector<HTMLInputElement>('input[name="search"]')!.value).toBe("")
  } finally { await view.dispose() }
})

it("previews all selected stable IDs and confirms only the server preview identity", async () => {
  const view = await mount()
  try {
    const boxes = view.container.querySelectorAll<HTMLInputElement>('input[type="checkbox"][aria-label]')
    await act(async () => boxes[0].click())
    await act(async () => boxes[1].click())
    await act(async () => button(view.container, "归档会话").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_archive_preview", { vmmProjectId: 7, sessionSourceIds: ["source-0", "source-1"], archived: true })
    const dialog = document.querySelector('[role="alertdialog"]')!
    expect(dialog.textContent).toContain("显示标题 0")
    expect(dialog.textContent).toContain("显示标题 1")
    await act(async () => button(dialog, "确认变更").click())
    expect(view.onRunAction.mock.calls).toContainEqual(["native_session_archive", { previewId: "preview-batch", confirmed: true }])
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", nativeSessionInitialQuery(7))
    expect(view.container.textContent).toContain("已选 0 个会话")
  } finally { await view.dispose() }
})

it("refreshes the open detail after archiving from the retained list", async () => {
  // view retains both the list and its selected source detail during the batch action.
  // view 在批量操作期间同时保留列表及其已选来源详情。
  const view = await mount()
  try {
    await act(async () => button(view.container, "显示标题 0").click())
    // list isolates the batch action from the individual action in the reading pane.
    // list 区分批量操作与阅读区中的单条操作。
    const list = view.container.querySelector(".vmm-session-browser")!
    await act(async () => list.querySelector<HTMLInputElement>('input[type="checkbox"][aria-label]')!.click())
    await act(async () => button(list, "归档会话").click())
    await act(async () => button(document.querySelector('[role="alertdialog"]')!, "确认变更").click())
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_memory_records")).toEqual([
      ["native_memory_records", { vmmProjectId: 7, sessionSourceId: "source-0" }],
      ["native_memory_records", { vmmProjectId: 7, sessionSourceId: "source-0" }],
    ])
  } finally { await view.dispose() }
})

it("does not silently drop mixed-state rows from a bulk archive", async () => {
  const view = await mount({ mixed: true })
  try {
    await act(async () => view.container.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click())
    expect(view.container.textContent).toContain("批量归档要求")
    expect(view.onRunAction.mock.calls.some(([action]) => action === "native_session_archive_preview")).toBe(false)
    expect(Array.from(view.container.querySelectorAll("button")).some((entry) => entry.textContent === "归档会话")).toBe(false)
  } finally { await view.dispose() }
})

it("recycles a mixed active/archived selection through the actual list controls", async () => {
  const view = await mount({ mixed: true })
  try {
    const boxes = view.container.querySelectorAll<HTMLInputElement>('input[type="checkbox"][aria-label]')
    await act(async () => boxes[0].click())
    await act(async () => boxes[1].click())
    await act(async () => button(view.container, "预览回收会话 (2)").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_removal_preview", { vmmProjectId: 7, selection: { kind: "sessions", ids: ["source-0", "source-1"] } })
    await act(async () => button(document, "确认移入回收站").click())
    expect(view.onRunAction.mock.calls).toContainEqual(["native_removal_recycle", { previewId: "manual-preview", confirmed: true }])
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_session_list", nativeSessionInitialQuery(7))
    expect(view.container.textContent).toContain("已选 0 个会话")
  } finally { await view.dispose() }
})

it("maps selected visible turns to canonical IDs and refreshes the exact source after confirmation", async () => {
  const view = await mount({ turns: true })
  try {
    await act(async () => button(view.container, "显示标题 0").click())
    const turn2 = view.container.querySelector<HTMLButtonElement>('[role="checkbox"][aria-label="选择回收回合 #2"]')!
    const turn10 = view.container.querySelector<HTMLButtonElement>('[role="checkbox"][aria-label="选择回收回合 #10"]')!
    await act(async () => turn2.click())
    await act(async () => turn10.click())
    await act(async () => button(view.container, "预览回收所选回合 (2)").click())
    expect(view.onRunAction).toHaveBeenLastCalledWith("native_removal_preview", { vmmProjectId: 7, selection: { kind: "turns", ids: ["vmm-turn-2", "vmm-turn-10"] } })
    await act(async () => button(document, "确认移入回收站").click())
    expect(view.onRunAction.mock.calls.slice(-2)).toEqual([
      ["native_memory_records", { vmmProjectId: 7, sessionSourceId: "source-0" }],
      ["native_session_list", nativeSessionInitialQuery(7)],
    ])
    expect(button(view.container, "预览回收所选回合 (0)").disabled).toBe(true)
  } finally { await view.dispose() }
})

it("clears a bulk selection as soon as a filter changes, before submitting the new query", async () => {
  const view = await mount()
  try {
    await act(async () => view.container.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click())
    expect(view.container.textContent).toContain("已选 20 个会话")
    await chooseMemoryOption(view.container.querySelector<HTMLButtonElement>('[aria-label="会话状态"]')!, "已归档")
    expect(view.container.textContent).toContain("已选 0 个会话")
    expect(view.onRunAction).toHaveBeenCalledTimes(2)
  } finally { await view.dispose() }
})

it("allows explicit retry after catalog failure without a render-triggered retry loop", async () => {
  const view = await mount({ failCatalog: true })
  try {
    expect(view.onRunAction).toHaveBeenCalledTimes(1)
    expect(view.container.querySelector('[role="alert"]')).not.toBeNull()
    await act(async () => button(view.container, "刷新记录").click())
    expect(view.onRunAction).toHaveBeenCalledTimes(2)
  } finally { await view.dispose() }
})

it("does not dispatch during an unrelated controller operation", async () => {
  const view = await mount({ busy: true })
  try { expect(view.onRunAction).not.toHaveBeenCalled() } finally { await view.dispose() }
})

it("shows detail failure and retries the same exact session without a full-project fallback", async () => {
  const view = await mount({ failDetail: true })
  try {
    await act(async () => button(view.container, "显示标题 0").click())
    expect(view.container.querySelector('[role="alert"]')?.textContent).toContain("未能加载来源详情")
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_memory_records")).toHaveLength(1)
    await act(async () => button(view.container.querySelector(".vmm-session-detail")!, "刷新记录").click())
    expect(view.onRunAction.mock.calls.filter(([action]) => action === "native_memory_records")).toEqual([
      ["native_memory_records", { vmmProjectId: 7, sessionSourceId: "source-0" }],
      ["native_memory_records", { vmmProjectId: 7, sessionSourceId: "source-0" }],
    ])
  } finally { await view.dispose() }
})

it("rejects reversed dates without dispatching a new query", async () => {
  const view = await mount()
  try {
    const form = view.container.querySelector("form")!
    await act(async () => {
      (form.elements.namedItem("from") as HTMLInputElement).value = "2026-09-07"
      ;(form.elements.namedItem("before") as HTMLInputElement).value = "2026-09-06"
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }))
    })
    expect(view.onRunAction).toHaveBeenCalledTimes(2)
    expect(view.container.querySelector('[role="alert"]')?.textContent).toContain("截止日期不得早于起始日期")
  } finally { await view.dispose() }
})

it("converts inclusive local calendar days and rejects silently rolled-over or malformed dates", () => {
  expect(nativeSessionDateBound("", false)).toBeNull()
  expect(nativeSessionDateBound("2026-09-06", false)).toBe(new Date(2026, 8, 6).getTime())
  expect(nativeSessionDateBound("2026-09-06", true)).toBe(new Date(2026, 8, 7).getTime())
  for (const date of ["2026-02-30", "2026-13-01", "2026-9-06", "invalid"]) expect(() => nativeSessionDateBound(date, false)).toThrow()
})
