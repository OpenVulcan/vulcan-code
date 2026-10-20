import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { IconButton } from "@/ui-kit/primitives/button"

describe("IconButton", () => {
  // Maps every semantic icon size to one distinct shared Button recipe.
  // 将每个语义图标尺寸映射到不同的共享 Button 配方。
  it("maps the four icon sizes to distinct square recipes", () => {
    expect(renderToStaticMarkup(<IconButton aria-label="xs" size="xs" />)).toContain('data-size="icon-xs"')
    expect(renderToStaticMarkup(<IconButton aria-label="sm" size="sm" />)).toContain('data-size="icon-sm"')
    expect(renderToStaticMarkup(<IconButton aria-label="md" size="md" />)).toContain('data-size="icon"')
    expect(renderToStaticMarkup(<IconButton aria-label="lg" size="lg" />)).toContain('data-size="icon-lg"')
  })

  // Uses the quiet ghost hierarchy unless a stronger semantic variant is requested.
  // 未指定更强语义时使用低干扰的 ghost 层级。
  it("uses the ghost hierarchy by default", () => {
    expect(renderToStaticMarkup(<IconButton aria-label="action" />)).toContain('data-variant="ghost"')
  })
})
