import type { TranslationKey } from "@/i18n-messages"
import type { PublicAiServiceEntry } from "@/types"

// TranslationResolver resolves one registered WebUI translation key.
// TranslationResolver 解析一个已注册的 WebUI 翻译键。
export type TranslationResolver = (key: TranslationKey) => string

// ServiceEntryPresentation separates a product title from its composable classification tags.
// ServiceEntryPresentation 将产品标题与可组合的分类标签分离。
export interface ServiceEntryPresentation {
  // Stable localized product title.
  // 稳定的本地化产品标题。
  title: string
  // Ordered localized classification tags.
  // 有序的本地化分类标签。
  tags: string[]
}

// providerDescriptionKeys maps every built-in provider to its exact localized introduction.
// providerDescriptionKeys 将每个内置供应商映射到其精确的本地化介绍。
const providerDescriptionKeys: Readonly<Record<string, TranslationKey>> = {
  minimax: "settings.providerDescription.minimax",
  kimi: "settings.providerDescription.kimi",
  deepseek: "settings.providerDescription.deepseek",
  alibaba: "settings.providerDescription.alibaba",
  openrouter: "settings.providerDescription.openrouter",
  apikeyfun: "settings.providerDescription.apikeyfun",
  claudeapi: "settings.providerDescription.claudeapi",
  qiniu: "settings.providerDescription.qiniu",
  fennoai: "settings.providerDescription.fennoai",
  code0: "settings.providerDescription.code0",
  tavily: "settings.providerDescription.tavily",
  openai: "settings.providerDescription.openai",
  anthropic: "settings.providerDescription.anthropic",
  google: "settings.providerDescription.google",
}

// alibabaServiceEntryKeys defines the exact product and category composition for Alibaba entries.
// alibabaServiceEntryKeys 定义阿里云入口的精确产品与分类组合。
const alibabaServiceEntryKeys: Readonly<Record<string, {
  title: TranslationKey
  tags: TranslationKey[]
}>> = {
  "coding-plan-cn": {
    title: "settings.serviceEntry.title.codingPlan",
    tags: ["settings.serviceEntry.tag.china"],
  },
  "coding-plan-global": {
    title: "settings.serviceEntry.title.codingPlan",
    tags: ["settings.serviceEntry.tag.international"],
  },
  "token-plan-personal-cn": {
    title: "settings.serviceEntry.title.tokenPlan",
    tags: ["settings.serviceEntry.tag.personal", "settings.serviceEntry.tag.china"],
  },
  "token-plan-personal-global": {
    title: "settings.serviceEntry.title.tokenPlan",
    tags: ["settings.serviceEntry.tag.personal", "settings.serviceEntry.tag.international"],
  },
  "token-plan-team-cn": {
    title: "settings.serviceEntry.title.tokenPlan",
    tags: ["settings.serviceEntry.tag.team", "settings.serviceEntry.tag.china"],
  },
  "token-plan-team-global": {
    title: "settings.serviceEntry.title.tokenPlan",
    tags: ["settings.serviceEntry.tag.team", "settings.serviceEntry.tag.international"],
  },
  "model-studio-cn": {
    title: "settings.serviceEntry.title.modelStudio",
    tags: ["settings.serviceEntry.tag.china"],
  },
  "model-studio-singapore": {
    title: "settings.serviceEntry.title.modelStudio",
    tags: ["settings.serviceEntry.tag.international"],
  },
}

// localizedProviderDescription returns the localized introduction for one built-in provider.
// localizedProviderDescription 返回一个内置供应商的本地化介绍。
//
// Parameters
// 参数
// `providerId` is the exact provider registry identifier and `t` resolves translations.
// `providerId` 是精确的供应商注册标识，`t` 用于解析翻译。
//
// Returns
// 返回值
// The provider-specific introduction or the explicit generic introduction.
// 返回供应商专属介绍，未知标识则返回明确的通用介绍。
export function localizedProviderDescription(
  providerId: string,
  t: TranslationResolver,
): string {
  return t(providerDescriptionKeys[providerId] ?? "settings.providerDescription.default")
}

// localizedServiceEntryPresentation returns a compact title and ordered tags for one entry.
// localizedServiceEntryPresentation 返回一个入口的紧凑标题与有序标签。
//
// Parameters
// 参数
// `providerId` and `entry` identify the exact registered entry; `t` resolves translations.
// `providerId` 与 `entry` 标识精确注册入口，`t` 用于解析翻译。
//
// Returns
// 返回值
// A structured presentation; providers without a registered split retain their exact display name.
// 返回结构化展示；未注册拆分规则的供应商保留其精确显示名称。
export function localizedServiceEntryPresentation(
  providerId: string,
  entry: PublicAiServiceEntry,
  t: TranslationResolver,
): ServiceEntryPresentation {
  if (providerId !== "alibaba") {
    return { title: entry.display_name, tags: [] }
  }
  const presentation = alibabaServiceEntryKeys[entry.id]
  if (!presentation) {
    return { title: entry.display_name, tags: [] }
  }
  return {
    title: t(presentation.title),
    tags: presentation.tags.map(t),
  }
}
