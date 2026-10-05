import type { PluginSummary } from "@/types";

// localizedPluginText resolves plugin manifest text for the active WebUI locale.
// localizedPluginText 为当前 WebUI 语言解析插件清单文案。
//
// Parameters: values contains locale-keyed manifest text, locale is the active WebUI locale, and fallback is the service-projected default text.
// 参数：values 包含按语言标识组织的清单文案，locale 是当前 WebUI 语言，fallback 是服务投影的默认文案。
// Returns: the exact-locale value, a same-language value, or the projected fallback in that order.
// 返回值：依次返回完整语言匹配值、同语言匹配值或投影回退文案。
export function localizedPluginText(
  values: Record<string, string>,
  locale: string,
  fallback: string,
): string {
  // exactValue preserves region-specific plugin translations when the package supplies them.
  // exactValue 在插件包提供地区专用翻译时保留该翻译。
  const exactValue = values[locale]?.trim();
  if (exactValue) {
    return exactValue;
  }
  // language identifies the configured base language for locale-compatible lookup.
  // language 标识用于兼容语言查找的当前基础语言。
  const language = locale.split("-")[0];
  // languageValue follows the same exact-then-language convention as other localized catalogs.
  // languageValue 遵循其他本地化目录使用的完整匹配后同语言匹配约定。
  const languageValue = Object.entries(values).find(([candidate, value]) => (
    (candidate === language || candidate.startsWith(`${language}-`)) && value.trim().length > 0
  ))?.[1]?.trim();
  return languageValue || fallback;
}

// pluginDisplayName resolves one plugin identifier to its localized catalog name, or the identifier itself.
// pluginDisplayName 将单个插件标识解析为本地化目录名称，找不到时返回标识本身。
// Parameters: plugins is the projected catalog, locale is the active WebUI locale, pluginId is the exact identifier.
// 参数：plugins 是投影目录，locale 是当前 WebUI 语言，pluginId 是精确标识。
// Returns: the display name; an uninstalled or unknown plugin keeps its raw identifier.
// 返回值：显示名称；已卸载或未知插件保留原始标识。
export function pluginDisplayName(plugins: ReadonlyArray<Pick<PluginSummary, "id" | "name" | "nameByLocale">>, locale: string, pluginId: string): string {
  const owner = plugins.find((plugin) => plugin.id === pluginId);
  return owner ? localizedPluginText(owner.nameByLocale, locale, owner.name) : pluginId;
}
