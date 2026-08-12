import { describe, expect, it } from "vitest"

import { normalizeProjectIconSelection } from "./project-icon-contract"

describe("normalizeProjectIconSelection", () => {
  // Verifies ordinary catalog selections remain byte-for-byte stable without loading catalogs.
  // 验证普通目录选择在不加载目录时保持逐字节稳定。
  it("preserves non-badge selections", () => {
    expect(normalizeProjectIconSelection({ pack: "skill-icons", key: "rust" })).toEqual({
      pack: "skill-icons",
      key: "rust",
    })
  })

  // Verifies legacy badge shapes normalize through the lightweight persistence contract.
  // 验证旧版徽标形状会通过轻量持久化契约完成规范化。
  it("normalizes legacy badge shapes and text", () => {
    const normalized = normalizeProjectIconSelection({
      pack: "badge",
      key: JSON.stringify({
        shape: "rounded",
        text: " vulcan ",
        background: "#112233",
        border: "#445566",
        foreground: "#ffffff",
      }),
    })

    expect(JSON.parse(normalized.key)).toEqual({
      shape: "square",
      text: "VU",
      background: "#112233",
      border: "#445566",
      foreground: "#ffffff",
    })
  })

  // Verifies malformed badge data uses the exact established default.
  // 验证损坏徽标数据使用精确的既有默认值。
  it("falls back for malformed badge data", () => {
    const normalized = normalizeProjectIconSelection({ pack: "badge", key: "not-json" })

    expect(JSON.parse(normalized.key)).toEqual({
      shape: "square",
      text: "VC",
      background: "#111827",
      border: "#111827",
      foreground: "#ffffff",
    })
  })
})
