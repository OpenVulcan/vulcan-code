import type { AgentSkillCategorySaveRequest, AgentSkillCategorySummary, AgentSkillSaveRequest, AgentSkillsCatalogSummary } from "@/types"
import { formatMessage, type Translate, type TranslationKey } from "@/i18n"

// BUILTIN_CATEGORY_NAME_KEYS names each built-in category through the interface catalog; the service ships only
// English names for them, so the catalog is the one place their translations live.
// BUILTIN_CATEGORY_NAME_KEYS 通过界面目录命名每个内置分类；服务只提供其英文名称，因此目录是其译文的唯一所在。
const BUILTIN_CATEGORY_NAME_KEYS: Readonly<Record<string, TranslationKey>> = {
  general: "agentSkills.builtinCategory.general",
  engineering: "agentSkills.builtinCategory.engineering",
  quality: "agentSkills.builtinCategory.quality",
  operations: "agentSkills.builtinCategory.operations",
  data: "agentSkills.builtinCategory.data",
  documents: "agentSkills.builtinCategory.documents",
  design: "agentSkills.builtinCategory.design",
  research: "agentSkills.builtinCategory.research",
  security: "agentSkills.builtinCategory.security",
  integrations: "agentSkills.builtinCategory.integrations",
  plugins: "agentSkills.builtinCategory.plugins",
  uncategorized: "agentSkills.builtinCategory.uncategorized",
}

// localizedAgentSkillCategoryName names a category in the interface language: built-in categories come from the
// catalog, user categories from their own exact-then-language-prefix locale entries, then their stored name.
// localizedAgentSkillCategoryName 以界面语言命名分类：内置分类取自目录，用户分类按精确语言再语言前缀取其自身
// 语言条目，最后回落到其存储名称。
export function localizedAgentSkillCategoryName(category: AgentSkillCategorySummary, locale: string, t: Translate): string {
  const builtinKey = category.builtin ? BUILTIN_CATEGORY_NAME_KEYS[category.categoryId] : undefined
  if (builtinKey) return t(builtinKey)
  return category.locales[locale]?.name
    ?? Object.entries(category.locales).find(([candidate]) => candidate.split("-")[0] === locale.split("-")[0])?.[1].name
    ?? category.name
}

// agentSkillsCatalogSummary builds the plain-language catalog sentence shown by Agent Skills surfaces.
// It leads with how many skills are usable and mentions user, plugin and error counts only when they are non-zero.
// agentSkillsCatalogSummary 构建 Agent Skills 各界面显示的通俗目录摘要。
// 先说明可用技能数，仅在非零时提及你添加的、插件提供的和读取失败的数量。
export function agentSkillsCatalogSummary(
  catalog: AgentSkillsCatalogSummary,
  t: Translate,
): string {
  const userCount = catalog.records.filter((record) => record.scopeKey === "user").length
  const pluginCount = catalog.records.filter((record) => record.scopeKey === "plugin").length
  const effectiveCount = catalog.records.filter((record) => record.effective).length
  const issueCount = catalog.errors.length
  const parts: string[] = []
  if (userCount > 0) parts.push(formatMessage(t("agentSkills.addedByYou"), { userCount }))
  if (pluginCount > 0) parts.push(formatMessage(t("agentSkills.fromPlugins"), { pluginCount }))
  let sentence = formatMessage(t(effectiveCount === 1 ? "agentSkills.summary.availableOne" : "agentSkills.summary.available"), { count: effectiveCount })
  if (parts.length) sentence += formatMessage(t("agentSkills.summary.parts"), { parts: parts.join(t("agentSkills.summary.partSeparator")) })
  if (issueCount > 0) {
    sentence += formatMessage(t(issueCount === 1 ? "agentSkills.summary.unreadableOne" : "agentSkills.summary.unreadable"), { count: issueCount })
  }
  return formatMessage(t("agentSkills.summary.sentence"), { sentence })
}

// AgentSkillActionProps defines the independent Agent Skills management boundary.
// AgentSkillActionProps 定义独立 Agent Skills 管理边界。
export interface AgentSkillActionProps {
  // Refreshes the complete source-aware catalog.
  // 刷新完整来源感知目录。
  onRefreshAgentSkills: () => Promise<void>
  // Opens the configured user package directory.
  // 打开配置后的用户技能包目录。
  onOpenAgentSkillsUserDirectory: () => Promise<void>
  // Toggles one effective Agent Skill.
  // 切换一个有效 Agent Skill。
  onSetAgentSkillEnabled: (name: string, enabled: boolean) => Promise<void>
  // Saves one complete user package.
  // 保存一个完整用户技能包。
  onSaveAgentSkill: (skill: AgentSkillSaveRequest) => Promise<void>
  // Deletes one exact user package.
  // 删除一个精确用户技能包。
  onDeleteAgentSkill: (name: string) => Promise<void>
  // Restores one system package by removing its user override.
  // 通过移除用户覆盖恢复一个系统技能包。
  onRestoreAgentSkillDefault: (name: string) => Promise<void>
  // Saves one user-defined category.
  // 保存一个用户定义分类。
  onSaveAgentSkillCategory: (category: AgentSkillCategorySaveRequest) => Promise<void>
  // Deletes one user-defined category and moves its skills to unassigned.
  // 删除一个用户定义分类并将其技能移至未分配。
  onDeleteAgentSkillCategory: (categoryId: string) => Promise<void>
  // Assigns one effective skill to one category.
  // 将一个有效技能归入一个分类。
  onSetAgentSkillCategory: (name: string, categoryId: string) => Promise<void>
}
