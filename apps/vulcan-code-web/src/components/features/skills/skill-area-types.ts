import type { useI18n } from "@/i18n"
import type { FeatureHeaderSummary } from "@/components/features/feature-area-types"

// SkillActionProps groups service-backed actions used by the skill page.
// SkillActionProps 分组技能页面使用的服务端动作。
export interface SkillActionProps {
  // Refreshes the Lua Skills catalog.
  // 刷新 Lua Skills 目录。
  onRefreshLuaSkills: () => Promise<void>
  // Installs one Lua Skill package.
  // 安装单个 Lua Skill 包。
  onInstallLuaSkill: (skillId: string, source: string, sourceType: string) => Promise<void>
  // Updates one managed Lua Skill package.
  // 更新单个受管 Lua Skill 包。
  onUpdateLuaSkill: (skillId: string) => Promise<void>
  // Toggles one Lua Skill package lifecycle state.
  // 切换单个 Lua Skill 包生命周期状态。
  onSetLuaSkillEnabled: (skillId: string, enabled: boolean) => Promise<void>
  // Uninstalls one ordinary Lua Skill package.
  // 卸载单个普通 Lua Skill 包。
  onUninstallLuaSkill: (skillId: string) => Promise<void>
  // Persists one Lua Skill tool registration state.
  // 持久化单个 Lua Skill 工具注册状态。
  onSetLuaSkillToolRegistrationState: (skillId: string, localName: string, registrationState: string) => Promise<void>
  // Refreshes prompt-based Agent Skills.
  // 刷新基于提示词的 Agent Skills。
  onRefreshAgentSkills: () => Promise<void>
  // Toggles the global Agent Skills switch.
  // 切换 Agent Skills 全局开关。
  onSetAgentSkillsEnabled: (enabled: boolean) => Promise<void>
  // Toggles one Agent Skill global state.
  // 切换单个 Agent Skill 全局状态。
  onSetAgentSkillEnabled: (name: string, enabled: boolean) => Promise<void>
}

// SkillSubAreaId identifies the selected skill management category.
// SkillSubAreaId 标识当前选中的技能管理分类。
export type SkillSubAreaId = "lua" | "agent"

// SkillHeaderBusyActionId identifies one title-bar skill action currently showing feedback.
// SkillHeaderBusyActionId 标识当前正在标题栏展示反馈的技能动作。
export type SkillHeaderBusyActionId = "lua-refresh" | "agent-refresh"

// SkillHeaderSummary aliases the shared feature header shape for skill pages.
// SkillHeaderSummary 为技能页面复用共享功能标题结构。
export type SkillHeaderSummary = FeatureHeaderSummary

// localizedAgentSkillScope maps backend scope labels into the active UI language.
// localizedAgentSkillScope 将后端作用域标签映射到当前界面语言。
export function localizedAgentSkillScope(scope: string, t: ReturnType<typeof useI18n>["t"]): string {
  const normalized = scope.trim().toLowerCase()
  if (normalized === "系统" || normalized === "system") {
    return t("skills.label.system")
  }
  if (normalized === "用户" || normalized === "user") {
    return t("skills.label.user")
  }
  return scope
}
