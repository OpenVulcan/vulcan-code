import type { AgentSkillCategorySaveRequest, AgentSkillSaveRequest } from "@/types"

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
