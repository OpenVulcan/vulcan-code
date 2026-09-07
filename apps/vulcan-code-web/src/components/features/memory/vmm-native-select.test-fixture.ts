import { act } from "react"
import { afterAll, beforeAll, expect, vi } from "vitest"

// Preserve DOM descriptors because jsdom lacks scrolling requested by the real Radix menu.
// 保留 DOM 属性描述，因为 jsdom 缺少真实 Radix 菜单请求的滚动能力。
const scrollDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView")
const actDescriptor = Object.getOwnPropertyDescriptor(globalThis, "IS_REACT_ACT_ENVIRONMENT")
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() })
  Object.defineProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT", { configurable: true, writable: true, value: true })
})
afterAll(() => {
  if (scrollDescriptor) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", scrollDescriptor)
  else Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView")
  if (actDescriptor) Object.defineProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT", actDescriptor)
  else Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
})

// Selects an exact visible option through keyboard DOM events on the production shared Select.
// 通过生产共享 Select 的键盘 DOM 事件选择准确的可见选项。
// `trigger` is the actual combobox and `label` its localized option; absent or disabled controls fail.
// `trigger` 是真实组合框，`label` 为本地化选项；缺失或禁用控件使测试失败。
export async function chooseMemoryOption(trigger: HTMLButtonElement, label: string): Promise<void> {
  expect(trigger).not.toBeNull()
  expect(trigger.disabled).toBe(false)
  await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })) })
  const option = Array.from(document.querySelectorAll<HTMLElement>('[role="option"]')).find((entry) => entry.textContent === label)
  expect(option, `visible option: ${label}`).toBeDefined()
  await act(async () => { option!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })) })
}
