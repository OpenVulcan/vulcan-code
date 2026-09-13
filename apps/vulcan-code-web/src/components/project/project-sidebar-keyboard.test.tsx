// @vitest-environment jsdom
import { act } from "react"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { CollapsedFeatureMenu } from "./project-sidebar-menus"

// Verifies focus entry, menu traversal and Escape focus return use the actual navigation callbacks.
// 验证焦点进入、菜单遍历及 Escape 焦点返回使用实际导航回调。
it("opens collapsed navigation by keyboard and returns focus on Escape", async () => {
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  const select = vi.fn()
  try {
    await act(async () => root.render(<CollapsedFeatureMenu label="功能" items={[{ id: "one", icon: null, label: "第一项", active: false, onSelect: select }, { id: "two", icon: null, label: "第二项", active: false, onSelect: select }]}><button type="button">入口</button></CollapsedFeatureMenu>))
    const trigger = container.querySelector<HTMLButtonElement>("button")!
    await act(async () => trigger.focus())
    const items = container.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')
    expect(items).toHaveLength(2)
    await act(async () => { items[0].focus(); items[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })) })
    expect(document.activeElement).toBe(items[1])
    await act(async () => items[1].dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })))
    expect(container.querySelector('[role="menu"]')).toBeNull()
    expect(document.activeElement).toBe(trigger)
    expect(select).not.toHaveBeenCalled()
  } finally { await act(async () => root.unmount()); container.remove() }
})
