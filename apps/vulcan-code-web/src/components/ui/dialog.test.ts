import { describe, expect, it } from "vitest"

import { cn } from "@/lib/utils"
import { dialogContentSizeClassNames } from "./dialog"

describe("DialogContent width recipes", () => {
  // Verifies an explicit wide page contract replaces the compact default instead of being overridden by it.
  // 验证页面明确声明的宽版契约会替换紧凑默认值，而不是被默认值覆盖。
  it("allows explicit page widths to override the compact default", () => {
    const className = cn(dialogContentSizeClassNames.compact, "sm:max-w-3xl")

    expect(className).toContain("sm:max-w-3xl")
    expect(className).not.toContain("sm:max-w-sm")
  })

  // Verifies legacy unprefixed dialog widths are not coupled to a higher-priority data selector.
  // 验证既有的无前缀弹窗宽度不会再受更高优先级 data 选择器限制。
  it("keeps size recipes free of data-selector width precedence", () => {
    expect(Object.values(dialogContentSizeClassNames).every((className) => !className.includes("data-[size="))).toBe(true)
  })

  // Verifies semantic wide sizes remain available without page-specific width classes.
  // 验证没有页面专用宽度类时，语义化宽版尺寸仍然有效。
  it("keeps semantic wide sizes", () => {
    expect(dialogContentSizeClassNames.lg).toBe("sm:max-w-2xl")
    expect(dialogContentSizeClassNames.xl).toBe("sm:max-w-4xl")
  })
})
