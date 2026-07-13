import type { FeatureHeaderSummary } from "@/components/features/feature-area-types"
import type { LuaSkillsManagedRuntimeTarget, LuaSkillsRuntimeSettings, PluginSummary, SystemPluginPermissionPreview } from "@/types"

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
  // Inspects one local user plugin before authorization.
  // 在授权前检查单个本地用户插件。
  onInspectLocalPlugin: (sourcePath: string) => Promise<SystemPluginPermissionPreview>
  // Installs one local user plugin with exact capability and dependency consent.
  // 使用精确能力与依赖许可安装单个本地用户插件。
  onInstallLocalPlugin: (sourcePath: string, approvedCapabilities: string[], allowRegistryDependencies: boolean) => Promise<void>
  // Uninstalls one disabled user plugin.
  // 卸载单个已停用用户插件。
  onUninstallUserPlugin: (pluginId: string) => Promise<void>
  // Uninstalls one ordinary Lua Skill package.
  // 卸载单个普通 Lua Skill 包。
  onUninstallLuaSkill: (skillId: string) => Promise<void>
  // Persists one Lua Skill tool registration state.
  // 持久化单个 Lua Skill 工具注册状态。
  onSetLuaSkillToolRegistrationState: (skillId: string, localName: string, registrationState: string) => Promise<void>
}

// SkillSubAreaId identifies the selected skill management category.
// SkillSubAreaId 标识当前选中的技能管理分类。
export type SkillSubAreaId = "catalog" | "plugins" | "rtk" | "settings"

// hasEnabledPluginDetailMount reports whether one exact enabled plugin contributes the requested integrated mount point.
// hasEnabledPluginDetailMount 判断一个精确的已启用插件是否贡献了请求的集成挂载点。
//
// Parameters: plugins is the committed plugin projection and pluginId is the exact manifest identifier.
// 参数：plugins 是已提交插件投影，pluginId 是精确清单标识。
// Returns: true only when the plugin is available, enabled, runtime-ready, and declares the detail mount.
// 返回值：仅当插件可用、已启用、运行时就绪且声明详情挂载点时返回 true。
export function hasEnabledPluginDetailMount(plugins: PluginSummary[], pluginId: string): boolean {
  return plugins.some((plugin) => (
    plugin.id === pluginId
    && plugin.available
    && plugin.enabled
    && plugin.runtimeState === "enabled"
    && plugin.extension?.uiMounts.some((mount) => mount.point === "skillsPluginDetail") === true
  ))
}

// SkillHeaderBusyActionId identifies one title-bar skill action currently showing feedback.
// SkillHeaderBusyActionId 标识当前正在标题栏展示反馈的技能动作。
export type SkillHeaderBusyActionId = "lua-refresh"

// SkillHeaderSummary aliases the shared feature header shape for skill pages.
// SkillHeaderSummary 为技能页面复用共享功能标题结构。
export type SkillHeaderSummary = FeatureHeaderSummary
