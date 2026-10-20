// @vitest-environment jsdom
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { ActivityRailFlyout } from "./activity-rail-flyout"

// container and root isolate one flyout per test.
// container 与 root 为每个测试隔离一个浮层。
let container: HTMLDivElement
let root: Root
beforeEach(() => {
  vi.useFakeTimers()
  container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
})
afterEach(async () => {
  await act(async () => root.unmount())
  container.remove()
  vi.useRealTimers()
})

// renderFlyout mounts a flyout whose content owns one menu trigger in the given state.
// renderFlyout 挂载一个浮层，其内容包含一个处于给定状态的菜单触发器。
async function renderFlyout(props: { menuOpen?: boolean; dismissKey?: string; onFooter?: () => void }) {
  await act(async () => root.render(
    <ActivityRailFlyout
      content={<button aria-expanded={props.menuOpen ?? false} aria-haspopup="menu" type="button">项目菜单</button>}
      dismissKey={props.dismissKey}
      footer={{ label: "返回工作台", icon: null, onSelect: props.onFooter ?? vi.fn() }}
      title="会话"
    >
      <button type="button">入口</button>
    </ActivityRailFlyout>,
  ))
}

const panel = () => container.querySelector<HTMLElement>(".vk-rail-flyout")
const trigger = () => container.querySelector<HTMLButtonElement>(".vk-rail-flyout-anchor button")!

// Verifies keyboard entry shows the hosted content and Escape returns focus to the rail button.
// 验证键盘进入会显示承载内容，且 Escape 将焦点交还轨道按钮。
it("opens by keyboard focus and returns focus on Escape", async () => {
  await renderFlyout({})
  await act(async () => trigger().focus())
  expect(panel()?.getAttribute("aria-label")).toBe("会话")
  expect(panel()?.textContent).toContain("项目菜单")
  const inner = panel()!.querySelector<HTMLButtonElement>("button")!
  await act(async () => { inner.focus(); inner.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })) })
  expect(panel()).toBeNull()
  expect(document.activeElement).toBe(trigger())
})

// Verifies a menu opened from the hosted content keeps the panel alive after the pointer leaves for the menu portal.
// 验证从承载内容中打开的菜单在指针移向菜单传送层后仍保留面板。
it("stays open while a menu opened inside it is open", async () => {
  await renderFlyout({ menuOpen: true })
  await act(async () => trigger().dispatchEvent(new MouseEvent("mouseover", { bubbles: true })))
  expect(panel()).not.toBeNull()
  await act(async () => { panel()!.dispatchEvent(new MouseEvent("mouseout", { bubbles: true })); vi.runAllTimers() })
  expect(panel()).not.toBeNull()

  await renderFlyout({ menuOpen: false })
  await act(async () => { panel()!.dispatchEvent(new MouseEvent("mouseout", { bubbles: true })); vi.runAllTimers() })
  expect(panel()).toBeNull()
})

// Verifies choosing a destination (a new dismiss key) or the footer action closes the panel.
// 验证选定目标（新的关闭键）或底部动作都会关闭面板。
it("closes when the destination changes or the footer action runs", async () => {
  const footer = vi.fn()
  await renderFlyout({ dismissKey: "a", onFooter: footer })
  await act(async () => trigger().focus())
  await renderFlyout({ dismissKey: "b", onFooter: footer })
  expect(panel()).toBeNull()

  await act(async () => { trigger().blur(); trigger().focus() })
  const footerButton = [...panel()!.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "返回工作台")!
  await act(async () => footerButton.click())
  expect(footer).toHaveBeenCalledTimes(1)
  expect(panel()).toBeNull()
})
