import type { LocaleCode } from "@/i18n-catalogs"
import registry from "@catalogs/locales.json"

export type { LocaleCode } from "@/i18n-catalogs"

// LocaleEntry is one language of the shared registry.
// LocaleEntry 是共享注册表中的一种语言。
export interface LocaleEntry {
  readonly code: LocaleCode
  readonly nativeName: string
  readonly mark: string
}

// SUPPORTED_LOCALES lists the languages, native names and picker marks of `catalogs/locales.json` in the
// vulcan-i18n crate, the single registry shared with the service, the TUI and the desktop shell. A test
// keeps its codes aligned with the catalog loaders.
// SUPPORTED_LOCALES 列出 vulcan-i18n crate 中 `catalogs/locales.json` 的语言、本地名称与选择器标记；
// 该注册表由服务、TUI 与桌面壳共用。一个测试保证其代码与目录加载器一致。
export const SUPPORTED_LOCALES = registry.locales as readonly LocaleEntry[]

// DEFAULT_LOCALE is used when no device language matches and completes every catalog's missing keys.
// DEFAULT_LOCALE 在没有匹配的设备语言时使用，并补全各目录缺失的文案键。
export const DEFAULT_LOCALE = registry.default as LocaleCode

// SUPPORTED_LOCALE_CODES indexes the registry for exact code validation.
// SUPPORTED_LOCALE_CODES 为精确代码校验索引注册表。
const SUPPORTED_LOCALE_CODES: ReadonlySet<string> = new Set(SUPPORTED_LOCALES.map((entry) => entry.code))

// TRADITIONAL_CHINESE_REGIONS lists regions whose Chinese defaults to Traditional script when no script subtag is given.
// TRADITIONAL_CHINESE_REGIONS 列出未给出书写系统子标签时默认使用繁体中文的地区。
const TRADITIONAL_CHINESE_REGIONS: ReadonlySet<string> = new Set(["tw", "hk", "mo"])

// normalizeLocaleCode validates one persisted or service-provided locale code.
// normalizeLocaleCode 校验单个持久化或服务提供的语言代码。
//
// # Parameters
// 参数
// `value` is an exact wire value such as `zh-TW`.
// `value` 是精确传输值，例如 `zh-TW`。
//
// # Returns
// 返回值
// The supported code, or `null` for anything outside the registry.
// 受支持的代码；不在注册表内时返回 `null`。
export function normalizeLocaleCode(value: string | null | undefined): LocaleCode | null {
  return value && SUPPORTED_LOCALE_CODES.has(value) ? value as LocaleCode : null
}

// matchLocaleTag maps one BCP 47 language tag onto the closest supported locale.
// matchLocaleTag 将一个 BCP 47 语言标签映射到最接近的受支持语言。
//
// # Parameters
// 参数
// `tag` is one device or browser language tag, for example `zh-Hant-HK`, `pt-PT` or `en`.
// `tag` 是一个设备或浏览器语言标签，例如 `zh-Hant-HK`、`pt-PT` 或 `en`。
//
// # Returns
// 返回值
// The supported locale for the tag's language, or `null` when that language is not supported.
// Chinese resolves by script first (`Hant`/`Hans`); Traditional HK uses the Hong Kong catalog, while TW and MO use Taiwan.
// 该标签语言对应的受支持语言；语言不受支持时返回 `null`。中文先按书写系统（`Hant`/`Hans`）解析；香港繁体使用香港目录，台湾与澳门繁体使用台湾目录。
export function matchLocaleTag(tag: string): LocaleCode | null {
  const subtags = tag.trim().replace(/_/gu, "-").toLowerCase().split("-").filter(Boolean)
  const language = subtags[0]
  if (!language) {
    return null
  }
  if (language === "zh") {
    // traditionalLocale preserves regional wording without overriding an explicit script choice.
    // traditionalLocale 保留地区用语差异，同时不覆盖显式选择的书写系统。
    const traditionalLocale = subtags.includes("hk") ? "zh-HK" : "zh-TW"
    if (subtags.includes("hant")) return traditionalLocale
    if (subtags.includes("hans")) return "zh-CN"
    return subtags.some((subtag) => TRADITIONAL_CHINESE_REGIONS.has(subtag)) ? traditionalLocale : "zh-CN"
  }
  const exact = SUPPORTED_LOCALES.find((entry) => entry.code.toLowerCase() === subtags.join("-"))
  if (exact) {
    return exact.code
  }
  return SUPPORTED_LOCALES.find((entry) => entry.code.split("-")[0].toLowerCase() === language)?.code ?? null
}

// detectDeviceLocale picks the first supported language from the device preference list.
// detectDeviceLocale 从设备偏好列表中选出第一种受支持语言。
//
// # Parameters
// 参数
// `preferredTags` is the device preference list in priority order, normally `navigator.languages`.
// `preferredTags` 是按优先级排列的设备偏好列表，通常为 `navigator.languages`。
//
// # Returns
// 返回值
// The first matching supported locale, or English when no preferred language is supported.
// 第一个匹配的受支持语言；没有受支持的偏好语言时返回英文。
export function detectDeviceLocale(preferredTags: readonly string[]): LocaleCode {
  for (const tag of preferredTags) {
    const match = matchLocaleTag(tag)
    if (match) {
      return match
    }
  }
  return DEFAULT_LOCALE
}

// browserPreferredLocaleTags returns the device language preference list available to the page.
// browserPreferredLocaleTags 返回页面可获得的设备语言偏好列表。
export function browserPreferredLocaleTags(): readonly string[] {
  if (typeof navigator === "undefined") {
    return []
  }
  return navigator.languages?.length ? navigator.languages : [navigator.language].filter(Boolean)
}

// resolveAppLocale returns the user's saved language, or the best device language when none was saved.
// resolveAppLocale 返回用户保存的语言；未保存时返回最匹配的设备语言。
//
// # Parameters
// 参数
// `savedLocale` is the service-provided selection, absent until the user picks a language.
// `savedLocale` 是服务提供的选择，用户选择语言前为空。
//
// # Returns
// 返回值
// A supported locale; device detection falls back to English. Detection never writes the user's configuration.
// 受支持的语言；设备检测回退到英文。检测结果不会写入用户配置。
export function resolveAppLocale(savedLocale: string | null | undefined): LocaleCode {
  return normalizeLocaleCode(savedLocale) ?? detectDeviceLocale(browserPreferredLocaleTags())
}

// isChineseLocale returns whether a locale reads Chinese, used to pick the Chinese side of host-provided bilingual data.
// isChineseLocale 返回语言是否为中文，用于选择宿主提供的双语数据中的中文部分。
//
// # Parameters
// 参数
// `locale` is the active supported UI language.
// `locale` 是当前受支持的界面语言。
//
// # Returns
// 返回值
// `true` for the Simplified, Taiwan Traditional or Hong Kong Traditional Chinese locale; otherwise `false`.
// 简体中文、台湾繁体中文或香港繁体中文语言返回 `true`，其他语言返回 `false`。
export function isChineseLocale(locale: LocaleCode): boolean {
  return locale === "zh-CN" || locale === "zh-TW" || locale === "zh-HK"
}

// pickBilingualText selects host-provided bilingual text: Chinese locales read the Chinese side, all others English.
// pickBilingualText 选择宿主提供的双语文本：中文语言读取中文部分，其他语言读取英文部分。
//
// # Parameters
// 参数
// `locale` is the active UI language; `zh` and `en` are the two authored variants.
// `locale` 是当前界面语言；`zh` 与 `en` 是已创作的两个版本。
export function pickBilingualText(locale: LocaleCode, zh: string, en: string): string {
  return isChineseLocale(locale) ? zh : en
}
