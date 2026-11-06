import { readFileSync } from 'node:fs';

// User-facing text for the desktop shell, shown before or outside the WebUI in the WebUI's languages.
// 桌面壳面向用户的文本，在 WebUI 之前或之外以 WebUI 所支持的语言展示。
//
// Languages and text come from the shared catalogs in `crates/vulcan-i18n/catalogs` (`locales.json`
// and the `desktop` surface). The build copies them beside `gui/` as `i18n/`, in `dist/` and in the
// packaged application alike; a missing or incomplete copy fails at startup instead of rendering keys.
// 语言与文本来自 `crates/vulcan-i18n/catalogs` 中的共享目录（`locales.json` 与 `desktop` 分面）。构建会把它们
// 作为 `i18n/` 复制到 `gui/` 旁边，`dist/` 与打包应用一致；副本缺失或不完整时在启动时失败，而不是渲染键名。

/** One language code of the shared registry, for example `zh-TW`.
 * 共享注册表中的一种语言代码，例如 `zh-TW`。
 */
export type DesktopLocale = string;

/** Every piece of desktop-shell text; `{name}` placeholders are filled by `formatDesktopText`. */
export interface DesktopText {
  // Native close action of the renderer recovery dialog.
  // 渲染进程恢复对话框的原生关闭动作。
  closeWindow: string;
  // Launcher stage: probing for a running service.
  // 启动器阶段：探测正在运行的服务。
  checkingInstance: string;
  // Launcher stage: reusing a running service, `{pid}`.
  // 启动器阶段：复用正在运行的服务，`{pid}`。
  reusingInstance: string;
  // Launcher stage: starting the background service.
  // 启动器阶段：启动后台服务。
  startingService: string;
  // Launcher stage: waiting for business readiness.
  // 启动器阶段：等待业务就绪。
  awaitingReady: string;
  // Launcher stage: service ready, `{pid}`.
  // 启动器阶段：服务就绪，`{pid}`。
  serviceReady: string;
  // Launcher failure: the service process could not be spawned, `{error}`.
  // 启动器失败：无法创建服务进程，`{error}`。
  launchFailed: string;
  // Launcher failure: the process exited before readiness, `{code}`.
  // 启动器失败：进程在就绪前退出，`{code}`。
  exitedBeforeReady: string;
  // Launcher failure: a signal terminated the process before readiness, `{signal}`.
  // 启动器失败：进程在就绪前被信号终止，`{signal}`。
  signaledBeforeReady: string;
  // Launcher failure: the service reported an unusable status, `{status}`.
  // 启动器失败：服务报告了不可用状态，`{status}`。
  serviceUnavailable: string;
  // Launcher failure: no ready discovery document in time, `{seconds}` and `{path}`.
  // 启动器失败：未在时限内发布就绪发现文档，`{seconds}` 与 `{path}`。
  readyTimeout: string;
  // Launcher failure: another process owns the data directory but never answered, `{seconds}`.
  // 启动器失败：另一个进程占用数据目录却始终未应答，`{seconds}`。
  ownerUnresponsive: string;
  // Workbench failure: the WebUI could not be loaded, `{url}` and `{error}`.
  // 工作台失败：无法加载 WebUI，`{url}` 与 `{error}`。
  workbenchLoadFailed: string;
  // Native renderer recovery dialog title.
  // 原生渲染进程恢复对话框标题。
  rendererFailureTitle: string;
  // Native renderer recovery explanation, available even when the page cannot render.
  // 原生渲染进程恢复说明，即使页面无法渲染也可显示。
  rendererFailureMessage: string;
  // Recovery diagnostics and behavior, with {reason}, {code} and {path}.
  // 恢复诊断与行为说明，包含 {reason}、{code} 和 {path}。
  rendererFailureDetail: string;
  // Native action that reloads the workbench without restarting Core.
  // 重新加载工作台而不重启 Core 的原生动作。
  reloadWorkbench: string;
  // Built-in browser close confirmation title.
  // 内置浏览器关闭确认标题。
  closeBrowserTitle: string;
  // Built-in browser close confirmation message, `{count}`.
  // 内置浏览器关闭确认消息，`{count}`。
  closeBrowserMessage: string;
  // Built-in browser close confirmation detail.
  // 内置浏览器关闭确认详情。
  closeBrowserDetail: string;
  // Cancel button.
  // 取消按钮。
  cancel: string;
  // Confirm button closing every page.
  // 关闭全部页面的确认按钮。
  closeAll: string;
}

/** Compile-time list of every field, checked against the default catalog at load.
 * 编译期完整字段清单，加载时与默认目录核对。
 */
const DESKTOP_TEXT_FIELDS: Readonly<Record<keyof DesktopText, true>> = {
  closeWindow: true, checkingInstance: true,
  reusingInstance: true, startingService: true, awaitingReady: true, serviceReady: true, launchFailed: true,
  exitedBeforeReady: true, signaledBeforeReady: true, serviceUnavailable: true, readyTimeout: true, ownerUnresponsive: true,
  workbenchLoadFailed: true,
  rendererFailureTitle: true, rendererFailureMessage: true, rendererFailureDetail: true, reloadWorkbench: true,
  closeBrowserTitle: true, closeBrowserMessage: true, closeBrowserDetail: true, cancel: true, closeAll: true,
};

/** Root of the catalog copy placed beside `gui/` by the build.
 * 构建放在 `gui/` 旁边的目录副本根路径。
 */
const CATALOG_ROOT = new URL('../i18n/', import.meta.url);

/** Reads one JSON file of the catalog copy. */
function readCatalogFile<T>(relative: string): T {
  return JSON.parse(readFileSync(new URL(relative, CATALOG_ROOT), 'utf8')) as T;
}

/** Shared language registry. */
const REGISTRY = readCatalogFile<{ default: string; locales: ReadonlyArray<{ code: string }> }>('locales.json');

/** Supported language codes in display order. */
export const DESKTOP_LOCALES: readonly DesktopLocale[] = REGISTRY.locales.map((entry) => entry.code);

/** Default language when no device language is supported; it completes every other language. */
export const DEFAULT_DESKTOP_LOCALE: DesktopLocale = REGISTRY.default;

/** Default text, verified to define every field. */
const DEFAULT_TEXT: DesktopText = (() => {
  const catalog = readCatalogFile<Partial<DesktopText>>(`desktop/${DEFAULT_DESKTOP_LOCALE}.json`);
  const missing = Object.keys(DESKTOP_TEXT_FIELDS).filter((field) => typeof catalog[field as keyof DesktopText] !== 'string');
  if (missing.length > 0) throw new Error(`desktop/${DEFAULT_DESKTOP_LOCALE}.json is missing ${missing.join(', ')}`);
  return catalog as DesktopText;
})();

/** Loaded languages, read on first use. */
const loadedText = new Map<DesktopLocale, DesktopText>([[DEFAULT_DESKTOP_LOCALE, DEFAULT_TEXT]]);

/** Returns the desktop-shell text of one supported language, completing missing keys from the default.
 * 返回一种受支持语言的桌面壳文本，并用默认语言补全缺失的键。
 */
export function desktopText(locale: DesktopLocale): DesktopText {
  const code = normalizeDesktopLocale(locale) ?? DEFAULT_DESKTOP_LOCALE;
  let text = loadedText.get(code);
  if (!text) {
    text = { ...DEFAULT_TEXT, ...readCatalogFile<Partial<DesktopText>>(`desktop/${code}.json`) };
    loadedText.set(code, text);
  }
  return text;
}

/** Regions whose Chinese defaults to Traditional script when no script subtag is given. */
const TRADITIONAL_CHINESE_REGIONS: ReadonlySet<string> = new Set(['tw', 'hk', 'mo']);

/** Returns the exact supported locale for a wire value such as `zh-TW`, or `null`. */
export function normalizeDesktopLocale(value: string | null | undefined): DesktopLocale | null {
  return value && DESKTOP_LOCALES.includes(value) ? value : null;
}

/** Maps one device language tag by explicit Chinese script and region, or by a supported language.
 * 根据中文的显式书写系统与地区，或其他受支持语言，映射设备语言标签。
 * @param tag BCP 47 device language tag; underscores are accepted as subtag separators.
 * @param tag BCP 47 设备语言标签；允许下划线作为子标签分隔符。
 * @returns The closest supported desktop locale, or `null` for an unsupported language.
 * @returns 最接近的受支持桌面语言；不受支持的语言返回 `null`。
 */
export function matchDesktopLocaleTag(tag: string): DesktopLocale | null {
  const subtags = tag.trim().replace(/_/g, '-').toLowerCase().split('-').filter(Boolean);
  const language = subtags[0];
  if (!language) return null;
  if (language === 'zh') {
    // Hong Kong selects its own Traditional catalog; other Traditional regions retain Taiwan's catalog.
    // 香港选择专属繁体目录；其他繁体地区继续使用台湾目录。
    const traditionalLocale = subtags.includes('hk') ? 'zh-HK' : 'zh-TW';
    if (subtags.includes('hant')) return normalizeDesktopLocale(traditionalLocale);
    if (subtags.includes('hans')) return normalizeDesktopLocale('zh-CN');
    return normalizeDesktopLocale(subtags.some((subtag) => TRADITIONAL_CHINESE_REGIONS.has(subtag)) ? traditionalLocale : 'zh-CN');
  }
  return DESKTOP_LOCALES.find((locale) => locale.toLowerCase() === subtags.join('-'))
    ?? DESKTOP_LOCALES.find((locale) => locale.slice(0, locale.indexOf('-')).toLowerCase() === language)
    ?? null;
}

/** Picks the first supported device language, falling back to English. */
export function detectDesktopLocale(preferredTags: readonly string[]): DesktopLocale {
  for (const tag of preferredTags) {
    const match = matchDesktopLocaleTag(tag);
    if (match) return match;
  }
  return DEFAULT_DESKTOP_LOCALE;
}

/** Fills every `{name}` placeholder; placeholders without a value stay visible. */
export function formatDesktopText(template: string, values: Readonly<Record<string, string | number>>): string {
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) => Object.hasOwn(values, name) ? String(values[name]) : placeholder);
}
