// @vitest-environment jsdom

import { act } from "react"
import { chooseMemoryOption } from "./vmm-native-select.test-fixture"
import { createRoot } from "react-dom/client"
import { expect, it, vi } from "vitest"
import { I18nProvider } from "@/i18n"
import { nativeDurationMilliseconds, VmmNativeDurationInput } from "./vmm-native-duration-input"

// Exact decimal conversion must reject sub-millisecond values instead of silently rounding them.
// 准确十进制转换必须拒绝不足一毫秒的值，不能静默舍入。
it("converts declared units exactly and rejects invalid or fractional milliseconds", () => {
  expect(nativeDurationMilliseconds("1.5", "day")).toBe("129600000")
  expect(nativeDurationMilliseconds("0.001", "second")).toBe("1")
  expect(nativeDurationMilliseconds("0.0001", "second")).toBe("invalid")
  expect(nativeDurationMilliseconds("1.000000000000000001", "day")).toBe("invalid")
  expect(nativeDurationMilliseconds("3650", "day")).toBe("315360000000")
  expect(nativeDurationMilliseconds("", "day")).toBe("")
  for (const value of ["-1", "NaN", "Infinity", "1e3", " 1", ".5", "9".repeat(33)]) {
    expect(nativeDurationMilliseconds(value, "day")).toBe("invalid")
  }
})

// The controlled editor preserves partially typed decimals and refreshes only on external values.
// 受控编辑器保留尚未输入完整的小数，仅在外部值改变时重新展示。
it("preserves intermediate edits, changes units explicitly, and accepts an external reset", async () => {
  // Isolated DOM and callback expose the real component behavior without persistence side effects.
  // 独立 DOM 和回调呈现真实组件行为，不产生持久化副作用。
  const container = document.createElement("div")
  const root = createRoot(container)
  const onChange = vi.fn()
  document.body.append(container)
  // Renders the authoritative millisecond draft; no unit is persisted separately.
  // 渲染权威毫秒草稿；单位不单独持久化。
  const render = async (value: string) => {
    await act(async () => { root.render(<I18nProvider initialLocale="zh-CN"><VmmNativeDurationInput id="duration" value={value} disabled={false} onChange={onChange} /></I18nProvider>) })
  }
  try {
    await render("15552000000")
    const input = container.querySelector("input")!
    const unit = container.querySelector<HTMLButtonElement>('[role="combobox"]')!
    expect(input.value).toBe("180")
    expect(unit.textContent).toBe("天")
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "1.")
      input.dispatchEvent(new Event("input", { bubbles: true }))
    })
    expect(onChange).toHaveBeenLastCalledWith("86400000")
    await render("86400000")
    expect(input.value).toBe("1.")
    await chooseMemoryOption(unit, "秒")
    expect(onChange).toHaveBeenLastCalledWith("1000")
    await render("1000")
    expect(input.value).toBe("1.")
    await render("60000")
    expect(input.value).toBe("1")
    expect(unit.textContent).toBe("分钟")
    expect(onChange).toHaveBeenCalledTimes(2)
  } finally {
    await act(async () => { root.unmount() })
    container.remove()
  }
})
