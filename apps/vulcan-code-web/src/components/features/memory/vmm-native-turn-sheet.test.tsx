// @vitest-environment jsdom

import { act, useState } from "react"
import { createRoot } from "react-dom/client"
import { expect, it } from "vitest"
import { I18nProvider } from "@/i18n"
import type { VmmNativeTurnRecordSummary } from "@/types"
import { NativeTurnSheet } from "./vmm-native-memory-records"

// fixture creates an exact server-shaped turn with explicit adjacent IDs and no derived content.
// fixture 创建带显式相邻 ID 且无衍生内容的准确服务端回合结构。
function fixture(turnId: number, previousTurnIds: number[], nextTurnIds: number[]): VmmNativeTurnRecordSummary {
  return {
    turnId, previousTurnIds, nextTurnIds, sessionSourceId: "session", createdAtUnixMs: 1,
    status: "pending", userPreview: `question-${turnId}`, assistantPreview: "",
    timeline: [{ role: "user", content: `body-${turnId}`, occurredAtUnixMs: 1 }],
    generatedMemoryIds: [], generatedProfileNodeIds: [],
    memoryLevels: { session: 0, phase: 0, stable: 0, persistent: 0 }, analysisAttemptCount: 0,
  }
}

// Harness mirrors the page's selected-turn state so clicks must change the actual detail body.
// Harness 镜像页面选中回合状态，使点击必须改变实际详情正文。
function Harness() {
  // turns supplies both records from one authoritative session response.
  // turns 提供来自同一权威会话响应的两条记录。
  const turns = [fixture(2, [], [10]), fixture(10, [2], [])]
  // selected is updated by the same onOpenTurn event used by the real page.
  // selected 由真实页面使用的同一 onOpenTurn 事件更新。
  const [selected, setSelected] = useState<VmmNativeTurnRecordSummary | null>(turns[0])
  return <I18nProvider initialLocale="zh-CN">
    <NativeTurnSheet turn={selected} turns={turns} memories={[]} profiles={[]}
      onClose={() => setSelected(null)} onOpenTurn={setSelected}
      onOpenMemory={() => {}} onOpenProfile={() => {}} />
  </I18nProvider>
}

// Verifies both navigation directions render the selected record, not only a callback invocation.
// 验证双向导航渲染选中记录，而不仅是调用回调。
it("opens following and preceding turn details from the native sheet", async () => {
  // container and root own only this test's mounted React tree.
  // container 与 root 仅拥有本测试挂载的 React 树。
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  try {
    await act(async () => { root.render(<Harness />) })
    expect(document.body.textContent).toContain("body-2")
    // next locates the user-visible following-record button inside the real dialog portal.
    // next 在真实对话框门户中定位用户可见的后续记录按钮。
    const next = Array.from(document.querySelectorAll("button")).find((button) => button.textContent?.includes("question-10"))
    expect(next).toBeDefined()
    await act(async () => { next!.click() })
    expect(document.body.textContent).toContain("body-10")
    expect(document.body.textContent).not.toContain("body-2")
    // previous confirms navigation remains available after the selected turn changes.
    // previous 确认选中回合变化后仍可向前导航。
    const previous = Array.from(document.querySelectorAll("button")).find((button) => button.textContent?.includes("question-2"))
    expect(previous).toBeDefined()
    await act(async () => { previous!.click() })
    expect(document.body.textContent).toContain("body-2")
    expect(document.body.textContent).not.toContain("body-10")
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})
