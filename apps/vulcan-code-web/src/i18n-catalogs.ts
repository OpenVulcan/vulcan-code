import type { TranslationKey } from "@/i18n-messages"

// LocaleCatalog maps every stable text key to its text in one language.
// LocaleCatalog 将每个稳定文本键映射为一种语言中的文本。
export type LocaleCatalog = Readonly<Record<TranslationKey, string>>

// catalogLoaders fetches each language from its own chunk, so a session downloads only the languages it shows.
// catalogLoaders 从各自分块获取每种语言，使一次会话只下载其展示的语言。
//
// The bundler needs literal paths, so the keys repeat the codes of `catalogs/locales.json`; a test keeps
// them aligned. `satisfies` makes every catalog prove at compile time that it covers every key.
// 打包器需要字面路径，因此键重复 `catalogs/locales.json` 中的代码；一个测试保证两者一致。`satisfies`
// 使每个目录在编译期证明其覆盖全部键。
//
// This module imports catalog types only; the boot gate can start a download without bundling any catalog.
// 本模块只导入目录类型；启动门可以发起下载而不打包任何目录。
export const catalogLoaders = {
  "en-US": () => import("@catalogs/webui/en-US.json").then((module) => module.default),
  "zh-CN": () => import("@catalogs/webui/zh-CN.json").then((module) => module.default),
  "zh-TW": () => import("@catalogs/webui/zh-TW.json").then((module) => module.default),
  "zh-HK": () => import("@catalogs/webui/zh-HK.json").then((module) => module.default),
  "ja-JP": () => import("@catalogs/webui/ja-JP.json").then((module) => module.default),
  "ko-KR": () => import("@catalogs/webui/ko-KR.json").then((module) => module.default),
  "es-ES": () => import("@catalogs/webui/es-ES.json").then((module) => module.default),
  "fr-FR": () => import("@catalogs/webui/fr-FR.json").then((module) => module.default),
  "de-DE": () => import("@catalogs/webui/de-DE.json").then((module) => module.default),
  "pt-BR": () => import("@catalogs/webui/pt-BR.json").then((module) => module.default),
  "ru-RU": () => import("@catalogs/webui/ru-RU.json").then((module) => module.default),
} satisfies Record<string, () => Promise<LocaleCatalog>>

// LocaleCode describes one supported language.
// LocaleCode 描述一种受支持的语言。
export type LocaleCode = keyof typeof catalogLoaders

// CatalogState is the document-local catalog store shared by old and refreshed module consumers.
// CatalogState 是文档内的目录仓库，由旧模块与热更新后的消费者共享。
interface CatalogState {
  // loaded retains the last successfully loaded catalog while replacement data downloads.
  // loaded 在替换数据下载期间保留上一次成功加载的目录。
  loaded: Partial<Record<LocaleCode, LocaleCatalog>>
  // pending owns the latest request for each locale, rejecting stale writes after hot replacement.
  // pending 持有每种语言的最新请求，阻止热替换后的旧请求回写。
  pending: Partial<Record<LocaleCode, Promise<LocaleCatalog>>>
  // revision changes only when a new catalog object becomes available to readers.
  // revision 仅在新目录对象可供读取时改变。
  revision: number
  // listeners notify React consumers independently of whether their locale code changed.
  // listeners 独立于语言代码是否变化，通知 React 消费者。
  listeners: Set<() => void>
}

// Vite retains hot.data across replacement; production owns a fresh store for each document.
// Vite 跨模块替换保留 hot.data；生产模式为每个文档创建新仓库。
const hotState: { catalogState?: CatalogState } | undefined = import.meta.hot?.data
// restoredCatalogState identifies a replacement that must refresh already requested language chunks.
// restoredCatalogState 标识需要刷新已请求语言分块的模块替换。
const restoredCatalogState = hotState?.catalogState
// catalogState remains authoritative when a previous module's pending request completes.
// 前一模块的待处理请求完成时，catalogState 仍是权威仓库。
const catalogState: CatalogState = restoredCatalogState ?? {
  loaded: {}, pending: {}, revision: 0, listeners: new Set(),
}
if (hotState) hotState.catalogState = catalogState

// registerLocaleCatalog publishes a bundled or downloaded catalog and notifies its consumers.
// registerLocaleCatalog 发布静态打包或下载的目录，并通知其消费者。
// Parameters: locale identifies the language; catalog is its successfully loaded text. Returns nothing.
// 参数：locale 标识语言；catalog 是成功加载的文案。无返回值。
function registerLocaleCatalog(locale: LocaleCode, catalog: LocaleCatalog): void {
  if (catalogState.loaded[locale] === catalog) return
  catalogState.loaded[locale] = catalog
  catalogState.revision += 1
  for (const listener of catalogState.listeners) listener()
}

// loadedLocaleCatalog returns a downloaded catalog, or `undefined` while it is not available yet.
// loadedLocaleCatalog 返回已下载的目录；尚不可用时返回 `undefined`。
// Parameter: locale selects the language to read synchronously without starting a download.
// 参数：locale 选择要同步读取的语言，不触发下载。
export function loadedLocaleCatalog(locale: LocaleCode): LocaleCatalog | undefined {
  return catalogState.loaded[locale]
}

// subscribeLocaleCatalogs observes successful catalog publication, including same-language reloads.
// subscribeLocaleCatalogs 监听目录成功发布，包括相同语言的重新加载。
// Parameter: listener requests a consumer update. Returns the exact unsubscribe callback.
// 参数：listener 请求消费者更新。返回对应的取消订阅回调。
export function subscribeLocaleCatalogs(listener: () => void): () => void {
  catalogState.listeners.add(listener)
  return () => { catalogState.listeners.delete(listener) }
}

// localeCatalogRevision returns the stable snapshot consumed by useSyncExternalStore.
// localeCatalogRevision 返回 useSyncExternalStore 使用的稳定快照。
// Takes no arguments; returns a number that changes only when catalog content is published.
// 无参数；返回仅在目录内容发布时改变的数字。
export function localeCatalogRevision(): number {
  return catalogState.revision
}

// loadLocaleCatalog replaces one pending request while preserving the last usable catalog.
// loadLocaleCatalog 替换一个待处理请求，同时保留上一次可用目录。
// Parameter: locale selects the declared loader. Returns its result or the original load rejection.
// 参数：locale 选择已声明的加载器。返回加载结果或原始加载拒绝。
function loadLocaleCatalog(locale: LocaleCode): Promise<LocaleCatalog> {
  // request identity prevents an older download from overwriting or deleting a newer request.
  // request 标识阻止旧下载覆盖或删除新请求。
  const request: Promise<LocaleCatalog> = catalogLoaders[locale]().then((catalog) => {
    // Use the shared usable catalog or current download; never return discarded replacement data.
    // 使用共享的可用目录或当前下载结果；绝不返回已被替代的数据。
    if (catalogState.pending[locale] !== request) return ensureLocaleCatalog(locale)
    registerLocaleCatalog(locale, catalog)
    return catalog
  }).finally(() => {
    if (catalogState.pending[locale] === request) delete catalogState.pending[locale]
  })
  catalogState.pending[locale] = request
  return request
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
  // loaded is kept during HMR, so ordinary readers never switch to English while it refreshes.
  // loaded 在 HMR 期间保留，普通读取方不会在刷新目录时切换为英文。
  const loaded = catalogState.loaded[locale]
  if (loaded) {
    return Promise.resolve(loaded)
  }
  return catalogState.pending[locale] ?? loadLocaleCatalog(locale)
}

if (restoredCatalogState) {
  // Refresh only languages already requested, using the replacement module's updated chunk URLs.
  // 仅刷新已请求的语言，使用替换模块更新后的分块地址。
  const requestedLocales = new Set([
    ...Object.keys(catalogState.loaded), ...Object.keys(catalogState.pending),
  ] as LocaleCode[])
  for (const locale of requestedLocales) {
    void loadLocaleCatalog(locale).catch((error: unknown) => {
      // Failed HMR downloads keep the previous catalog and expose the real error in development.
      // HMR 下载失败时保留原目录，并在开发环境中暴露真实错误。
      console.error(`Locale catalog refresh failed: ${locale}`, error)
    })
  }
}
