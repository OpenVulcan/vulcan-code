import { readFileSync } from "node:fs"
import { beforeAll, describe, expect, it } from "vitest"
import { ensureLocaleCatalog, loadedLocaleCatalog } from "@/i18n-catalogs"
import { detectDeviceLocale, matchLocaleTag, normalizeLocaleCode, pickBilingualText, SUPPORTED_LOCALES, type LocaleCode } from "@/i18n-locales"
import { enUSTranslations } from "@/i18n-messages.en-US"
import type { TranslationKey } from "@/i18n-messages"

// placeholders lists the `{name}` placeholders of one message in a stable order.
// placeholders 以稳定顺序列出一条文案中的 `{name}` 占位符。
function placeholders(message: string): string {
  return [...message.matchAll(/\{(\w+)\}/gu)].map((match) => match[1]).sort().join(",")
}

describe("device language detection", () => {
  // Device tags resolve by language; Chinese resolves by script, then region; anything else falls back to English.
  // 设备标签按语言解析；中文先按书写系统再按地区解析；其他语言回退到英文。
  it.each<[string[], LocaleCode]>([
    [["zh-Hant-HK"], "zh-HK"],
    [["zh-Hant"], "zh-TW"],
    [["zh-Hant-TW"], "zh-TW"],
    [["zh-TW"], "zh-TW"],
    [["zh-HK"], "zh-HK"],
    [["zh_HK"], "zh-HK"],
    [["zh_MO"], "zh-TW"],
    [["zh-Hans-HK"], "zh-CN"],
    [["zh-Hans-SG"], "zh-CN"],
    [["zh"], "zh-CN"],
    [["ja"], "ja-JP"],
    [["ko-KR"], "ko-KR"],
    [["es-MX"], "es-ES"],
    [["fr-CA"], "fr-FR"],
    [["de-AT"], "de-DE"],
    [["pt-PT"], "pt-BR"],
    [["ru"], "ru-RU"],
    [["en-GB"], "en-US"],
    [["ar-SA", "hi-IN", "de-CH"], "de-DE"],
    [["ar-SA"], "en-US"],
    [[], "en-US"],
  ])("maps %j to %s", (tags, expected) => {
    expect(detectDeviceLocale(tags)).toBe(expected)
  })

  // Only exact registry codes are accepted as a saved selection.
  // 仅接受注册表中的精确代码作为已保存选择。
  it("accepts only exact supported codes as a saved selection", () => {
    expect(normalizeLocaleCode("zh-TW")).toBe("zh-TW")
    expect(normalizeLocaleCode("zh-HK")).toBe("zh-HK")
    expect(normalizeLocaleCode("zh")).toBeNull()
    expect(normalizeLocaleCode("ar-SA")).toBeNull()
    expect(matchLocaleTag("")).toBeNull()
  })

  // Host-provided bilingual text uses Chinese for all Chinese locales and English elsewhere.
  // 宿主提供的双语文本对所有中文语言使用中文，其他语言使用英文。
  it("reads the Chinese side of bilingual host data only for Chinese locales", () => {
    expect(pickBilingualText("zh-TW", "中", "en")).toBe("中")
    expect(pickBilingualText("zh-HK", "中", "en")).toBe("中")
    expect(pickBilingualText("ja-JP", "中", "en")).toBe("en")
  })
})

describe("supported language registry", () => {
  // Full catalog validation owns its loading cost instead of imposing it on unrelated behavior tests.
  // 完整目录验证承担自身加载成本，不将其强加给无关行为测试。
  beforeAll(async () => { await Promise.all(SUPPORTED_LOCALES.map(({ code }) => ensureLocaleCatalog(code))) })
  // The service validates saved selections against its own copy of the list.
  // 服务使用自己的列表副本校验已保存选择。
  it("matches the locales the service accepts when saving a selection", () => {
    const source = readFileSync(new URL("../../../crates/vulcan-service-kernel/src/workbench_model_routes.rs", import.meta.url), "utf8")
    const block = /const SUPPORTED_WEBUI_LOCALES: &\[&str\] = &\[([^\]]*)\]/u.exec(source)
    expect(block, "SUPPORTED_WEBUI_LOCALES must stay a literal list in workbench_model_routes.rs").not.toBeNull()
    const serviceLocales = [...block![1].matchAll(/"([^"]+)"/gu)].map((match) => match[1]).sort()
    expect(serviceLocales).toEqual(SUPPORTED_LOCALES.map((entry) => entry.code).sort())
  })

  // Every catalog defines every key with exactly the English placeholders, so no language renders a broken template.
  // 每个目录都定义全部键，且占位符与英文完全一致，使任何语言都不会渲染损坏的模板。
  it.each(SUPPORTED_LOCALES.map((entry) => entry.code))("%s covers every key with English placeholders", (locale) => {
    const catalog = loadedLocaleCatalog(locale)
    expect(catalog, `${locale} catalog should be loaded by this suite`).toBeDefined()
    const mismatches = (Object.keys(enUSTranslations) as TranslationKey[]).filter((key) => (
      typeof catalog![key] !== "string"
      // Split-sentence fragments around an inline element may be empty when word order needs only one side.
      || (enUSTranslations[key].trim() !== "" && catalog![key].trim() === "" && !/(Prefix|Suffix)$/u.test(key))
      || placeholders(catalog![key]) !== placeholders(enUSTranslations[key])
    ))
    expect(mismatches).toEqual([])
  })

  // A catalog that is mostly an English copy would pass the key check while showing English, so cap the shared share.
  // 大部分照抄英文的目录能通过键检查却仍显示英文，因此限制与英文相同文案的占比。
  it.each(SUPPORTED_LOCALES.map((entry) => entry.code).filter((locale) => locale !== "en-US"))("%s is translated rather than copied from English", (locale) => {
    const catalog = loadedLocaleCatalog(locale)!
    const keys = Object.keys(enUSTranslations) as TranslationKey[]
    const identical = keys.filter((key) => catalog[key] === enUSTranslations[key]).length
    expect(identical / keys.length, `${locale} shares ${identical} of ${keys.length} messages with English`).toBeLessThan(0.1)
  })
})
