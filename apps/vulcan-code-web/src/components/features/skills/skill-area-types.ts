import type { FeatureHeaderSummary } from "@/components/features/feature-area-types"
import type { LuaSkillsManagedRuntimeTarget, LuaSkillsRuntimeSettings } from "@/types"

// SkillActionProps groups service-backed actions used by the skill page.
// SkillActionProps 分组技能页面使用的服务端动作。
export interface SkillActionProps {
  // Refreshes the Lua Skills catalog.
  // 刷新 Lua Skills 目录。
  onRefreshLuaSkills: () => Promise<void>
  // Retries managed Python and Node bootstrap.
  // 重试受管 Python 与 Node 初始化。
  onRetryLuaSkillsManagedRuntimes: (target: LuaSkillsManagedRuntimeTarget) => Promise<void>
  // Saves the Vulcan-owned LuaSkills runtime startup settings.
  // 保存 Vulcan 自有 LuaSkills 运行时启动设置。
  onSaveLuaSkillsRuntimeSettings: (settings: LuaSkillsRuntimeSettings) => Promise<void>
  // Installs one Lua Skill package.
  // 安装单个 Lua Skill 包。
  onInstallLuaSkill: (skillId: string, source: string, sourceType: string) => Promise<void>
  // Updates one managed Lua Skill package.
  // 更新单个受管 Lua Skill 包。
  onUpdateLuaSkill: (skillId: string) => Promise<void>
  // Toggles one Lua Skill package lifecycle state.
  // 切换单个 Lua Skill 包生命周期状态。
  onSetLuaSkillEnabled: (skillId: string, enabled: boolean) => Promise<void>
  // Persists one host-owned LuaSkills System Plugin enabled state.
  // 持久化单个宿主自有 LuaSkills System Plugin 的启用状态。
  onSetSystemPluginEnabled: (pluginId: string, enabled: boolean) => Promise<void>
  // Uninstalls one ordinary Lua Skill package.
  // 卸载单个普通 Lua Skill 包。
  onUninstallLuaSkill: (skillId: string) => Promise<void>
  // Persists one Lua Skill tool registration state.
  // 持久化单个 Lua Skill 工具注册状态。
  onSetLuaSkillToolRegistrationState: (skillId: string, localName: string, registrationState: string) => Promise<void>
}

// SkillSubAreaId identifies the selected skill management category.
// SkillSubAreaId 标识当前选中的技能管理分类。
export type SkillSubAreaId = "catalog" | "plugins" | "settings"

// SkillHeaderBusyActionId identifies one title-bar skill action currently showing feedback.
// SkillHeaderBusyActionId 标识当前正在标题栏展示反馈的技能动作。
export type SkillHeaderBusyActionId = "lua-refresh"

// SkillHeaderSummary aliases the shared feature header shape for skill pages.
// SkillHeaderSummary 为技能页面复用共享功能标题结构。
export type SkillHeaderSummary = FeatureHeaderSummary
