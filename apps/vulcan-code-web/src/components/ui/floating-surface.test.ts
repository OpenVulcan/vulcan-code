import { describe, expect, it } from "vitest"

import {
  FLOATING_MOTION_CLASS_NAME,
  FLOATING_SURFACE_CLASS_NAME,
  MENU_CHECKABLE_ITEM_CLASS_NAME,
  MENU_ITEM_CLASS_NAME,
  floatingContentWidthClassName,
} from "@/components/ui/floating-surface"

describe("floating surface visual recipes", () => {
  it("keeps every lightweight overlay on the same semantic surface", () => {
    expect(FLOATING_SURFACE_CLASS_NAME).toContain("bg-popover")
    expect(FLOATING_SURFACE_CLASS_NAME).toContain("ring-(--vcn-floating-ring)")
    expect(FLOATING_MOTION_CLASS_NAME).toContain("data-[side=top]:slide-in-from-bottom-2")
  })

  it("keeps ordinary and checkable menu rows on one density", () => {
    expect(MENU_ITEM_CLASS_NAME).toContain("gap-2")
    expect(MENU_ITEM_CLASS_NAME).toContain("py-1.5")
    expect(MENU_CHECKABLE_ITEM_CLASS_NAME).toContain("gap-2")
    expect(MENU_CHECKABLE_ITEM_CLASS_NAME).toContain("py-1.5")
  })

  it("maps semantic widths without caller-owned arbitrary values", () => {
    expect(floatingContentWidthClassName("trigger")).toContain("--radix-dropdown-menu-trigger-width")
    expect(floatingContentWidthClassName("content")).toContain("w-max")
    expect(floatingContentWidthClassName("md")).toBe("w-56")
  })
})
