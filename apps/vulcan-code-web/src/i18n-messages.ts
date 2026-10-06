import type enUSCatalog from "@catalogs/webui/en-US.json"

// LocaleCode describes supported WebUI languages; `catalogs/locales.json` in the vulcan-i18n crate owns the list.
// LocaleCode 描述 WebUI 支持的语言；列表由 vulcan-i18n crate 的 `catalogs/locales.json` 持有。
export type { LocaleCode } from "@/i18n-catalogs"

// TranslationKey is every key of the default (English) WebUI catalog shared with the TUI.
// TranslationKey 是与 TUI 共用的默认（英文）WebUI 目录中的全部键。
export type TranslationKey = keyof typeof enUSCatalog
