// @vitest-environment jsdom
import React, { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, expect, it } from "vitest"

import { PageEmptyState, PageFacts, PageItem, PageNotice, PageRow, PageSection, PageTabPanel } from "@/components/page"

let root: Root
let container: HTMLDivElement

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
  container = document.createElement("div")
  document.body.appendChild(container)
  root = createRoot(container)
})

afterEach(() => {
  act(() => root.unmount())
  container.remove()
})

// render mounts one element synchronously.
// render 同步挂载一个元素。
function render(element: React.ReactElement) {
  act(() => root.render(element))
}

// Verifies a section is a labelled region whose heading level, actions and rows layout are honoured.
// 验证区块是带标签的区域，并遵循标题级别、操作与 rows 布局。
it("renders a labelled section with header actions and flush rows", () => {
  render(
    <PageSection actions={<button type="button">Refresh</button>} description="Purpose" headingLevel={2} layout="rows" title="Recovery">
      <PageRow description="Explains the switch" label="Enabled"><input aria-label="Enabled" type="checkbox" /></PageRow>
    </PageSection>,
  )
  const region = container.querySelector("[role='region']")
  expect(region?.getAttribute("data-layout")).toBe("rows")
  const heading = container.querySelector("h2")
  expect(heading?.textContent).toBe("Recovery")
  expect(region?.getAttribute("aria-labelledby")).toBe(heading?.id)
  expect(container.querySelector(".page-section-actions button")?.textContent).toBe("Refresh")
  expect(container.querySelector(".page-row-description")?.textContent).toBe("Explains the switch")
})

// Verifies a collapsed guide stays reachable behind its disclosure label.
// 验证收起的说明仍可通过展开标签访问。
it("folds the guide behind its label when collapsed", () => {
  render(<PageSection guide={<p>guide body</p>} guideCollapsed guideLabel="How it works" title="Capacity" />)
  expect(container.textContent).toContain("How it works")
  expect(container.textContent).not.toContain("guide body")
})

// Verifies errors announce assertively and other tones politely, with raw details folded.
// 验证错误以强提示播报、其余色调以礼貌方式播报，原始详情被收起。
it("maps notice tones to live-region roles", () => {
  render(<>
    <PageNotice details="stack trace" detailsLabel="Details" title="Failed" tone="error">Retry later.</PageNotice>
    <PageNotice tone="success">Saved.</PageNotice>
  </>)
  const notices = container.querySelectorAll(".page-notice")
  expect(notices[0]?.getAttribute("role")).toBe("alert")
  expect(notices[1]?.getAttribute("role")).toBe("status")
  // Details stay mounted for search and copy, but their folded content is closed.
  // 详情保持挂载以便查找与复制，但收起的内容处于关闭状态。
  const details = [...container.querySelectorAll<HTMLElement>(".page-notice-details [data-state]")].find((node) => node.textContent?.includes("stack trace"))
  expect(details?.getAttribute("data-state")).toBe("closed")
})

// Verifies empty states, facts and items expose their content and tone.
// 验证空状态、事实与列表项暴露其内容与色调。
it("renders empty states, facts and items", () => {
  render(<>
    <PageEmptyState description="Nothing yet" title="No rules" />
    <PageFacts items={[{ key: "id", label: "Request", value: "req-1" }]} />
    <PageItem aria-label="Interrupted update" description="Resume it" title="Update interrupted" tone="attention" />
  </>)
  expect(container.querySelector(".page-empty-state")?.getAttribute("role")).toBe("status")
  expect(container.querySelector(".page-facts dd")?.textContent).toBe("req-1")
  expect(container.querySelector("[aria-label='Interrupted update']")?.getAttribute("data-tone")).toBe("attention")
})

// Verifies tab panels mount on first activation and keep visited state while hidden.
// 验证页签面板首次激活时挂载，并在隐藏时保留已访问状态。
it("mounts tab panels lazily and keeps visited panels alive", () => {
  const view = (active: "a" | "b") => <>
    <PageTabPanel active={active === "a"}><input aria-label="draft" defaultValue="" /></PageTabPanel>
    <PageTabPanel active={active === "b"}><p>second</p></PageTabPanel>
  </>
  render(view("a"))
  expect(container.textContent).not.toContain("second")
  const input = container.querySelector<HTMLInputElement>("input[aria-label='draft']")!
  input.value = "kept"
  render(view("b"))
  expect(container.textContent).toContain("second")
  expect(container.querySelector<HTMLInputElement>("input[aria-label='draft']")?.value).toBe("kept")
  expect(container.querySelector<HTMLInputElement>("input[aria-label='draft']")?.closest(".page-tab-panel")?.hasAttribute("hidden")).toBe(true)
})
