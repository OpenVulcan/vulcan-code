import type { LocaleCode } from "@/i18n-locales"
import type { TranslationKey } from "@/i18n-messages"

// LocaleCatalog maps every stable text key to its text in one language.
// LocaleCatalog 将每个稳定文本键映射为一种语言中的文本。
export type LocaleCatalog = Readonly<Record<TranslationKey, string>>

// catalogLoaders fetches each language from its own chunk, so a session downloads only the languages it shows.
// catalogLoaders 从各自分块获取每种语言，使一次会话只下载其展示的语言。
//
// This module imports catalog types only; the boot gate can start a download without bundling any catalog.
// 本模块只导入目录类型；启动门可以发起下载而不打包任何目录。
const catalogLoaders: Record<LocaleCode, () => Promise<LocaleCatalog>> = {
  "en-US": () => import("@/i18n-messages.en-US").then((module) => module.enUSTranslations),
  "zh-CN": () => import("@/i18n-messages.zh-CN").then((module) => module.zhCNTranslations),
  "zh-TW": () => import("@/i18n-messages.zh-TW").then((module) => module.zhTWTranslations),
  "ja-JP": () => import("@/i18n-messages.ja-JP").then((module) => module.jaJPTranslations),
  "ko-KR": () => import("@/i18n-messages.ko-KR").then((module) => module.koKRTranslations),
  "es-ES": () => import("@/i18n-messages.es-ES").then((module) => module.esESTranslations),
  "fr-FR": () => import("@/i18n-messages.fr-FR").then((module) => module.frFRTranslations),
  "de-DE": () => import("@/i18n-messages.de-DE").then((module) => module.deDETranslations),
  "pt-BR": () => import("@/i18n-messages.pt-BR").then((module) => module.ptBRTranslations),
  "ru-RU": () => import("@/i18n-messages.ru-RU").then((module) => module.ruRUTranslations),
}

// loadedCatalogs holds every catalog that finished downloading; readers fall back to English for the rest.
// loadedCatalogs 保存所有已完成下载的目录；其余语言由读取方回退到英文。
const loadedCatalogs: Partial<Record<LocaleCode, LocaleCatalog>> = {}

// pendingCatalogs deduplicates concurrent requests for the same language.
// pendingCatalogs 对同一语言的并发请求去重。
const pendingCatalogs: Partial<Record<LocaleCode, Promise<LocaleCatalog>>> = {}

// registerLocaleCatalog records a catalog that was bundled statically, such as the English fallback.
// registerLocaleCatalog 记录一个静态打包的目录，例如英文回退目录。
export function registerLocaleCatalog(locale: LocaleCode, catalog: LocaleCatalog): void {
  loadedCatalogs[locale] = catalog
}

// loadedLocaleCatalog returns a downloaded catalog, or `undefined` while it is not available yet.
// loadedLocaleCatalog 返回已下载的目录；尚不可用时返回 `undefined`。
export function loadedLocaleCatalog(locale: LocaleCode): LocaleCatalog | undefined {
  return loadedCatalogs[locale]
}

// ensureLocaleCatalog downloads one language once and resolves when it can be read synchronously.
// ensureLocaleCatalog 对一种语言只下载一次，并在可同步读取时完成。
//
// # Parameters
// 参数
// `locale` is the language about to be shown.
// `locale` 是即将展示的语言。
//
// # Returns
// 返回值
// The catalog; a failed download rejects and is retried by the next call.
// 返回目录；下载失败时拒绝，下一次调用会重试。
export function ensureLocaleCatalog(locale: LocaleCode): Promise<LocaleCatalog> {
  const loaded = loadedCatalogs[locale]
  if (loaded) {
    return Promise.resolve(loaded)
  }
  const pending = pendingCatalogs[locale] ?? catalogLoaders[locale]().then((catalog) => {
    loadedCatalogs[locale] = catalog
    return catalog
  }).finally(() => {
    delete pendingCatalogs[locale]
  })
  pendingCatalogs[locale] = pending
  return pending
}
