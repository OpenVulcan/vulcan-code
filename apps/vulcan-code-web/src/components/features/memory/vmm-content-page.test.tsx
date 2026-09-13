// @vitest-environment jsdom

import { act } from "react"
import { createRoot } from "react-dom/client"
import { afterAll, beforeAll, expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmMemoryActionResult, VmmMemorySearchRetrieval } from "@/types"
import { VmmContentPage } from "./vmm-content-page"

// Restore React test globals after exercising the real page and its result projection.
// 验证真实页面及其结果投影后恢复 React 测试全局值。
beforeAll(() => vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true))
afterAll(() => vi.unstubAllGlobals())

// Distinct totals and matched evidence make field swaps visible in the actual search card.
// 不同的总数及匹配证据使字段互换在实际检索卡片中可见。
const retrieval: VmmMemorySearchRetrieval = {
  source_kind: "direct", origin: "lexical_rerank", origin_label: "Lexical + Rerank",
  origin_explanation: "Both stages executed.", final_score: 0.875,
  context_observed: true, support_count: 9, rebuttal_count: 5,
  matched_context_values: ["fruit=西瓜"], matched_context_support_count: 2,
  matched_context_rebuttal_count: 1, matched_context_score_delta: -0.01,
}

// Render received results through the actual page; missing external facts must produce no explanation card.
// 通过实际页面渲染接收结果；外置事实缺失时不能产生解释卡片。
it("preserves native provenance and distinguishes unobserved context from absent external facts", async () => {
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  const onRunAction = vi.fn(async () => null)
  // The page consumes the exact closed action envelope, not a component-only result replacement.
  // 页面消费准确封闭动作信封，而非仅替换子组件结果。
  const render = async (facts?: VmmMemorySearchRetrieval) => {
    const value = { trace_id: "trace", results: [{ query_index: 0, query: "西瓜", hits: [{
      memory_id: 7, source_turn_id: 0, abstract_text: "喜欢西瓜", details_preview: "明确饮食偏好",
      category: "general", created_datetime: "2026-09-06", ...(facts ? { retrieval: facts } : {}),
    }] }] }
    const result: VmmMemoryActionResult = {
      action: "search_memory_events", kind: "search_memory_events", summary: "", request: {}, response: value,
      data: { kind: "search_memory_events", value },
    }
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><VmmContentPage
      busy={true} snapshot={{ selectedProjectId: "host", selectedConversationId: "session", experienceState: { deployment: "native" } }}
      result={result} onRunAction={onRunAction} /></I18nProvider>))
  }
  try {
    await render(retrieval)
    const details = container.querySelector("details")!
    expect(details.querySelector("summary")!.textContent).toContain("Lexical + Rerank")
    expect(details.textContent).toContain("主动写入")
    expect(Array.from(details.querySelectorAll("dd")).map((entry) => entry.textContent)).toEqual(["0.8750", "主动写入", "9 / 5", "2 / 1", "-0.01"])
    expect(details.textContent).toContain("fruit=西瓜")
    await render({ ...retrieval, matched_context_score_delta: -0.010000000000005 })
    expect(details.querySelector("dd:last-child")!.textContent).toBe("-0.01")
    expect(details.querySelector("dd:last-child")!.getAttribute("title")).toBe("-0.010000000000005")
    await render({ ...retrieval, matched_context_score_delta: -0.0000001 })
    expect(details.querySelector("dd:last-child")!.textContent).toBe("0")
    await render({ ...retrieval, matched_context_score_delta: null })
    expect(details.querySelector("dd:last-child")!.textContent).toBe("—")
    await render({ ...retrieval, context_observed: false, matched_context_score_delta: null })
    expect(container.querySelector("details")!.textContent).toContain("本次未评估情境证据")
    expect(container.querySelector("details")!.textContent).not.toContain("9 / 5")
    expect(container.querySelector("details")!.textContent).not.toContain("fruit=西瓜")
    await render()
    expect(container.querySelector("details")).toBeNull()
    expect(container.textContent).toContain("喜欢西瓜")
    expect(onRunAction).not.toHaveBeenCalled()
  } finally {
    await act(async () => root.unmount())
    container.remove()
  }
})
