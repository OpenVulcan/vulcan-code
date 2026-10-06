import { describe, expect, it } from "vitest"
import vectors from "@catalogs/locale-matching.json"
import { catalogLoaders } from "@/i18n-catalogs"
import { detectDeviceLocale, matchLocaleTag, normalizeLocaleCode, pickBilingualText, SUPPORTED_LOCALES } from "@/i18n-locales"

// Catalog content (key coverage, placeholders, translation share) is checked once for every surface by the
// vulcan-i18n crate tests; this suite checks the WebUI's own matching code and loader table.
// 目录内容（键覆盖、占位符、翻译占比）由 vulcan-i18n crate 测试对所有分面统一检查；本套件检查 WebUI
// 自身的匹配代码与加载表。
describe("language matching", () => {
  // The shared vectors pin the same behavior for the Rust implementation used by the service and the TUI.
  // 共享向量为服务与 TUI 使用的 Rust 实现固定同一行为。
  it.each(vectors.tags)("maps tag %j", ({ tag, locale }) => {
    expect(matchLocaleTag(tag)).toBe(locale)
  })

  it.each(vectors.preferences)("detects %j", ({ tags, locale }) => {
    expect(detectDeviceLocale(tags)).toBe(locale)
  })

  it.each(vectors.savedSelections)("accepts saved selection %j", ({ value, locale }) => {
    expect(normalizeLocaleCode(value)).toBe(locale)
  })

  it("reads the Chinese side of bilingual host data only for Chinese locales", () => {
    expect(pickBilingualText("zh-TW", "中", "en")).toBe("中")
    expect(pickBilingualText("zh-HK", "中", "en")).toBe("中")
    expect(pickBilingualText("ja-JP", "中", "en")).toBe("en")
  })
})

describe("supported language registry", () => {
  // The bundler-facing loader table must list exactly the registry's languages.
  // 面向打包器的加载表必须恰好列出注册表中的语言。
  it("loads a catalog for exactly the registered languages", () => {
    expect(Object.keys(catalogLoaders).sort()).toEqual(SUPPORTED_LOCALES.map((entry) => entry.code).sort())
  })
})
