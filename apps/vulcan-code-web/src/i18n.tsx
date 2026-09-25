import * as React from "react"
import { UiTextProvider } from "@/components/ui/ui-text"
import {
  type LocaleCode,
  type TranslationKey,
} from "@/i18n-messages"
import { enUSTranslations } from "@/i18n-messages.en-US"
import { zhCNTranslations } from "@/i18n-messages.zh-CN"

export type { LocaleCode, TranslationKey } from "@/i18n-messages"

// Translate resolves one stable text key in the active language; helpers outside React receive it as a parameter.
// Translate 以当前语言解析一个稳定文本键；React 之外的辅助函数以参数形式接收它。
export type Translate = (key: TranslationKey) => string

// localeMessages resolves each supported language from its own module.
// localeMessages 从各自模块解析每种受支持语言。
//
// Dedicated chunks change and cache independently, but both static imports are fetched when the
// workbench loads; the build budget measures that complete eager graph.
// 独立分块可分别变化和缓存，但工作台加载时两份静态导入都会被获取；构建预算测量完整预加载图。
const localeMessages: Record<LocaleCode, Record<TranslationKey, string>> = {
  "zh-CN": zhCNTranslations,
  "en-US": enUSTranslations,
}

// I18nContextValue exposes the active language and translation helpers.
// I18nContextValue 暴露当前语言与翻译辅助函数。
interface I18nContextValue {
  // Current language code.
  // 当前语言代码。
  locale: LocaleCode
  // Updates the current language code.
  // 更新当前语言代码。
  setLocale: (locale: LocaleCode) => void
  // Translates one stable text key.
  // 翻译单个稳定文本键。
  t: Translate
}

// I18nContext stores the WebUI localization state.
// I18nContext 保存 WebUI 本地化状态。
const I18nContext = React.createContext<I18nContextValue | null>(null)

// normalizeLocaleCode validates one persisted or service-provided locale code.
// normalizeLocaleCode 校验单个持久化或服务提供的语言代码。
export function normalizeLocaleCode(value: string | null | undefined): LocaleCode | null {
  if (value === "zh-CN" || value === "en-US") {
    return value
  }
  return null
}

// initialLocale returns the service-provided language or a browser-derived default.
// initialLocale 返回服务提供的语言或从浏览器推导出的默认语言。
function initialLocale(serviceLocale: string | null | undefined): LocaleCode {
  return normalizeLocaleCode(serviceLocale)
    ?? (navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US")
}

// readLocaleMessage reads one localized app-shell label outside the i18n provider.
// readLocaleMessage 在 i18n provider 外读取单个本地化应用外壳文案。
//
// # Parameters
// 参数
// `locale` selects the language and `key` is the stable text key.
// `locale` 选择语言，`key` 是稳定文本键。
//
// # Returns
// 返回值
// The localized label, or its simplified Chinese fallback when the language lacks one.
// 返回本地化文案；该语言缺失时返回其中简体中文回退。
export function readLocaleMessage(locale: LocaleCode, key: TranslationKey): string {
  return localeMessages[locale][key] ?? localeMessages["zh-CN"][key]
}

// activeLocale mirrors the mounted provider's language for modules that run outside React.
// activeLocale 为 React 之外运行的模块镜像当前已挂载 provider 的语言。
let activeLocale: LocaleCode | null = null

// translateActive reads one message in the mounted provider's language for service adapters and stores.
// translateActive 按已挂载 provider 的语言为服务适配器与状态仓库读取单条文案。
//
// # Parameters
// 参数
// `key` is the stable text key.
// `key` 是稳定文本键。
//
// # Returns
// 返回值
// The localized message; before any provider mounts, the browser-derived default language is used.
// 返回本地化文案；任何 provider 挂载前使用从浏览器推导的默认语言。
export function translateActive(key: TranslationKey): string {
  return readLocaleMessage(activeLocale ?? initialLocale(null), key)
}

// formatMessage fills every {name} placeholder of one localized template.
// formatMessage 填充单个本地化模板中的每个 {name} 占位符。
//
// # Parameters
// 参数
// `template` is a translated message and `values` maps placeholder names to their text.
// `template` 是已翻译文案，`values` 将占位符名称映射到对应文本。
//
// # Returns
// 返回值
// The filled message; placeholders without a value stay visible so a missing binding is noticed.
// 返回填充后的文案；没有对应值的占位符保持原样，便于发现遗漏的绑定。
export function formatMessage(template: string, values: Readonly<Record<string, string | number>>): string {
  return template.replace(/\{(\w+)\}/gu, (placeholder, name: string) => Object.hasOwn(values, name) ? String(values[name]) : placeholder)
}

// I18nProvider owns the WebUI language state.
// I18nProvider 管理 WebUI 语言状态。
function I18nProvider(props: {
  // Initial locale resolved before the provider mounts.
  // Provider 挂载前解析出的初始语言。
  initialLocale?: string | null
  // Child tree that consumes localized shell labels.
  // 消费本地化外壳标签的子树。
  children: React.ReactNode
}) {
  // locale stores the active WebUI language.
  // locale 保存当前 WebUI 语言。
  const [locale, setLocaleState] = React.useState<LocaleCode>(() => initialLocale(props.initialLocale))
  React.useEffect(() => {
    const nextLocale = normalizeLocaleCode(props.initialLocale)
    if (nextLocale) {
      setLocaleState(nextLocale)
    }
  }, [props.initialLocale])
  // setLocale applies one selected language.
  // setLocale 应用选中的语言。
  const setLocale = React.useCallback((nextLocale: LocaleCode) => {
    setLocaleState(nextLocale)
  }, [])
  // t translates one stable key using the current language.
  // t 使用当前语言翻译单个稳定键。
  const t = React.useCallback((key: TranslationKey) => localeMessages[locale][key] ?? localeMessages["zh-CN"][key], [locale])
  // activeLocale is published during render so non-React callers see the language the tree renders with.
  // activeLocale 在渲染期间发布，使非 React 调用方看到子树正在使用的语言。
  activeLocale = locale
  // value memoizes the provider payload to avoid unnecessary shell rerenders.
  // value 缓存 provider 载荷，避免外壳不必要重渲染。
  const value = React.useMemo(() => ({ locale, setLocale, t }), [locale, setLocale, t])
  // uiText bridges the existing language authority to primitives without requiring them to import the full catalog.
  // uiText 将既有语言权威传递给基础组件，不要求基础组件导入完整文案目录。
  const uiText = React.useMemo(() => ({ close: t("ui.close"), loading: t("ui.loading"), notifications: t("settings.notifications") }), [t])

  return <I18nContext.Provider value={value}><UiTextProvider value={uiText}>{props.children}</UiTextProvider></I18nContext.Provider>
}

// useI18n returns the current WebUI localization helpers.
// useI18n 返回当前 WebUI 本地化辅助函数。
function useI18n(): I18nContextValue {
  const context = React.useContext(I18nContext)
  if (!context) {
    throw new Error("useI18n must be used within I18nProvider")
  }
  return context
}

export { I18nProvider, useI18n }
