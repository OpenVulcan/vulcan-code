import type { AgentSkillCategorySaveRequest, AgentSkillSaveRequest, AgentSkillsCatalogSummary } from "@/types"

// agentSkillsCatalogSummary builds the plain-language catalog sentence shown by Agent Skills surfaces.
// It leads with how many skills are usable and mentions user, plugin and error counts only when they are non-zero.
// agentSkillsCatalogSummary 构建 Agent Skills 各界面显示的通俗目录摘要。
// 先说明可用技能数，仅在非零时提及你添加的、插件提供的和读取失败的数量。
export function agentSkillsCatalogSummary(
  catalog: AgentSkillsCatalogSummary,
  locale: string,
): string {
  const zh = locale.startsWith("zh")
  const userCount = catalog.records.filter((record) => record.scopeKey === "user").length
  const pluginCount = catalog.records.filter((record) => record.scopeKey === "plugin").length
  const effectiveCount = catalog.records.filter((record) => record.effective).length
  const issueCount = catalog.errors.length
  const parts: string[] = []
  if (userCount > 0) parts.push(zh ? `${userCount} 个是你添加的` : `${userCount} added by you`)
  if (pluginCount > 0) parts.push(zh ? `${pluginCount} 个来自插件` : `${pluginCount} from plugins`)
  let sentence = zh
    ? `${effectiveCount} 个技能可用${parts.length ? `，其中 ${parts.join("、")}` : ""}`
    : `${effectiveCount} ${effectiveCount === 1 ? "skill" : "skills"} available${parts.length ? ` (${parts.join(", ")})` : ""}`
  if (issueCount > 0) {
    sentence += zh ? `；${issueCount} 个技能目录读取失败` : `; ${issueCount} ${issueCount === 1 ? "folder" : "folders"} could not be read`
  }
  return zh ? `${sentence}。` : `${sentence}.`
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
