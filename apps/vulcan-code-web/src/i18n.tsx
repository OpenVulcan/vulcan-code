import * as React from "react"
import { UiTextProvider } from "@/components/ui/ui-text"
import { ensureLocaleCatalog, loadedLocaleCatalog, registerLocaleCatalog } from "@/i18n-catalogs"
import { DEFAULT_LOCALE, normalizeLocaleCode, resolveAppLocale, type LocaleCode } from "@/i18n-locales"
import type { TranslationKey } from "@/i18n-messages"
import enUSTranslations from "@catalogs/webui/en-US.json"

export type { LocaleCode, TranslationKey } from "@/i18n-messages"
export { normalizeLocaleCode, pickBilingualText, resolveAppLocale, SUPPORTED_LOCALES } from "@/i18n-locales"

// Translate resolves one stable text key in the active language; helpers outside React receive it as a parameter.
// Translate 以当前语言解析一个稳定文本键；React 之外的辅助函数以参数形式接收它。
export type Translate = (key: TranslationKey) => string

// The English catalog is the only statically bundled language: it fills every key another catalog lacks
// and renders while a different language downloads. Every other language loads on demand.
// 英文目录是唯一静态打包的语言：它补全其他目录缺少的每个键，并在其他语言下载期间渲染。其余语言按需加载。
registerLocaleCatalog(DEFAULT_LOCALE, enUSTranslations)

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

// I18nHotState retains this module's context identity and active locale across Vite replacement.
// I18nHotState 在 Vite 替换模块时保留本模块的上下文标识和当前语言。
interface I18nHotState {
  // context is initialized once per document, so retained providers and refreshed hooks share it.
  // context 在每个文档中只初始化一次，使保留的 provider 与刷新的 hook 共享同一对象。
  context?: React.Context<I18nContextValue | null>
  // activeLocale preserves non-React translation until the refreshed provider renders.
  // activeLocale 在刷新的 provider 渲染前保留 React 外部的翻译语言。
  activeLocale?: LocaleCode | null
}

// Vite owns hot.data across module versions; production creates an ordinary document-local context.
// Vite 跨模块版本持有 hot.data；生产构建创建普通的文档内上下文。
const hotState: I18nHotState | undefined = import.meta.hot?.data
// Recreating the context during HMR disconnects mounted providers from refreshed consumers.
// 在 HMR 期间重建上下文会使已挂载的 provider 与刷新的消费者断开连接。
const I18nContext = hotState?.context ?? React.createContext<I18nContextValue | null>(null)
if (hotState) hotState.context = I18nContext

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
// The localized label; English is used while the language is still downloading or lacks the key.
// 返回本地化文案；该语言仍在下载或缺少该键时使用英文。
export function readLocaleMessage(locale: LocaleCode, key: TranslationKey): string {
  return loadedLocaleCatalog(locale)?.[key] ?? enUSTranslations[key]
}

// activeLocale mirrors the mounted provider's language for modules that run outside React.
// activeLocale 为 React 之外运行的模块镜像当前已挂载 provider 的语言。
let activeLocale: LocaleCode | null = hotState?.activeLocale ?? null
if (import.meta.hot) {
  // Save the current value before replacement; never write a user preference during hot reload.
  // 在模块替换前保存当前值；热更新期间绝不写入用户偏好。
  import.meta.hot.dispose((data: I18nHotState) => { data.activeLocale = activeLocale })
}

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
  return readLocaleMessage(activeLocale ?? resolveAppLocale(null), key)
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
  const [locale, setLocaleState] = React.useState<LocaleCode>(() => resolveAppLocale(props.initialLocale))
  // catalogRevision re-renders the tree once the active language finishes downloading.
  // catalogRevision 在当前语言下载完成后重新渲染子树。
  const [catalogRevision, setCatalogRevision] = React.useState(0)
  // requestedLocaleRef lets only the newest selection apply when downloads finish out of order.
  // requestedLocaleRef 使下载乱序完成时只有最新选择生效。
  const requestedLocaleRef = React.useRef(locale)
  // setLocale switches after the catalog is available, so the tree never flashes the fallback language.
  // setLocale 在目录可用后才切换，使子树不会闪现回退语言。
  const setLocale = React.useCallback((nextLocale: LocaleCode) => {
    requestedLocaleRef.current = nextLocale
    if (loadedLocaleCatalog(nextLocale)) {
      setLocaleState(nextLocale)
      return
    }
    void ensureLocaleCatalog(nextLocale).then(
      () => { if (requestedLocaleRef.current === nextLocale) setLocaleState(nextLocale) },
      () => { if (requestedLocaleRef.current === nextLocale) setLocaleState(nextLocale) },
    )
  }, [])
  React.useEffect(() => {
    const nextLocale = normalizeLocaleCode(props.initialLocale)
    if (nextLocale) {
      setLocale(nextLocale)
    }
  }, [props.initialLocale, setLocale])
  React.useEffect(() => {
    // The initial language may not be preloaded (for example after a failed download); render English until it arrives.
    // 初始语言可能未预加载（例如下载失败后）；在其到达前以英文渲染。
    if (loadedLocaleCatalog(locale)) {
      return
    }
    let cancelled = false
    void ensureLocaleCatalog(locale).then(() => { if (!cancelled) setCatalogRevision((revision) => revision + 1) }, () => undefined)
    return () => { cancelled = true }
  }, [locale])
  React.useEffect(() => {
    document.documentElement.lang = locale
  }, [locale])
  // t translates one stable key using the current language.
  // t 使用当前语言翻译单个稳定键。
  // catalogRevision is a dependency only so t is recreated once a download completes.
  // catalogRevision 仅作为依赖，使下载完成后重新创建 t。
  const t = React.useCallback((key: TranslationKey) => readLocaleMessage(locale, key), [locale, catalogRevision])
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
