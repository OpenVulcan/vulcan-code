import { expect, it } from "vitest"
import { formatNativeDuration } from "./native-duration-display"

// Integer millisecond policies must never display repeating fractional days or a rounded zero.
// 整数毫秒策略不得显示循环小数天数或舍入后的零值。
it("preserves exact short and mixed native retention durations", () => {
  expect(formatNativeDuration(60000, "en-US", (key) => key)).toBe("1 memory.policy.minutes")
  expect(formatNativeDuration(172800000, "en-US", (key) => key)).toBe("2 memory.policy.days")
  expect(formatNativeDuration(1, "en-US", (key) => key)).toBe("1 memory.policy.milliseconds")
  expect(formatNativeDuration(86400001, "en-US", (key) => key)).toBe("86,400,001 memory.policy.milliseconds")
})
