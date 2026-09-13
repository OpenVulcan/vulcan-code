import * as React from "react"
import { UiTextProvider } from "@/components/ui/ui-text"
import {
  translations,
  type LocaleCode,
  type TranslationKey,
} from "@/i18n-messages"

export type { LocaleCode, TranslationKey } from "@/i18n-messages"

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
  t: (key: TranslationKey) => string
}

// I18nContext stores the WebUI localization state.
// I18nContext 保存 WebUI 本地化状态。
const I18nContext = React.createContext<I18nContextValue | null>(null)

// initialLocale returns the persisted language or a browser-derived default.
// initialLocale 返回持久化语言或从浏览器推导出的默认语言。
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
  const t = React.useCallback((key: TranslationKey) => translations[locale][key] ?? translations["zh-CN"][key], [locale])
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
