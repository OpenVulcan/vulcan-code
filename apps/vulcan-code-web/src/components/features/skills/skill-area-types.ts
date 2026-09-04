import type { FeatureHeaderSummary } from "@/components/features/feature-area-types"
import type { LuaSkillConfigDeleteRequest, LuaSkillConfigDeleteResult, LuaSkillConfigDescriptor, LuaSkillConfigEventBatch, LuaSkillConfigSetRequest, LuaSkillConfigSetResult, LuaSkillConfigValidation, LuaSkillsManagedRuntimeTarget, LuaSkillsRuntimeSettings, PluginSummary, VulcanPluginInitialSecret, VulcanPluginPermissionPreview } from "@/types"

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
  // Persists one host-owned Vulcan Plugin enabled state through the unified plugin action.
  // 通过统一插件操作持久化单个宿主持有 Vulcan 插件的启用状态。
  onSetVulcanPluginEnabled: (pluginId: string, enabled: boolean) => Promise<void>
  // Inspects one local user plugin before authorization.
  // 在授权前检查单个本地用户插件。
  onInspectLocalPlugin: (sourcePath: string) => Promise<VulcanPluginPermissionPreview>
  // Installs one local user plugin with exact capability and dependency consent.
  // 使用精确能力与依赖许可安装单个本地用户插件。
  onInstallLocalPlugin: (sourcePath: string, contentFingerprint: string, approvedPermissions: string[], allowRegistryDependencies: boolean, initialSecrets: VulcanPluginInitialSecret[]) => Promise<void>
  // Updates one disabled local user plugin through the same exact authorization surface.
  // 通过同一精确授权界面更新单个已停用本地用户插件。
  onUpdateLocalPlugin: (sourcePath: string, contentFingerprint: string, approvedPermissions: string[], allowRegistryDependencies: boolean, initialSecrets: VulcanPluginInitialSecret[]) => Promise<void>
  // Uninstalls one disabled user plugin.
  // 卸载单个已停用用户插件。
  onUninstallUserPlugin: (pluginId: string) => Promise<void>
  // Stores one exact plugin MCP secret without returning its value.
  // 存储单个精确插件 MCP Secret，且不返回其值。
  onSetPluginMcpSecret: (pluginId: string, serverId: string, secretKey: string, value: string) => Promise<void>
  // Deletes one exact plugin MCP secret.
  // 删除单个精确插件 MCP Secret。
  onDeletePluginMcpSecret: (pluginId: string, serverId: string, secretKey: string) => Promise<void>
  // Uninstalls one ordinary Lua Skill package.
  // 卸载单个普通 Lua Skill 包。
  onUninstallLuaSkill: (skillId: string) => Promise<void>
  // Persists one Lua Skill tool registration state.
  // 持久化单个 Lua Skill 工具注册状态。
  onSetLuaSkillToolRegistrationState: (skillId: string, localName: string, registrationState: string) => Promise<void>
  // Loads one host-safe package configuration descriptor.
  // 加载单个宿主安全技能包配置描述。
  onDescribeLuaSkillConfig: (skillId: string) => Promise<LuaSkillConfigDescriptor | null>
  // Validates one package configuration without changing it.
  // 校验单个技能包配置且不修改它。
  onValidateLuaSkillConfig: (skillId: string) => Promise<LuaSkillConfigValidation>
  // Atomically persists one typed package configuration batch.
  // 原子持久化一组类型化技能包配置。
  onSetLuaSkillConfig: (request: LuaSkillConfigSetRequest) => Promise<LuaSkillConfigSetResult>
  // Deletes one explicit or orphaned package configuration value.
  // 删除单个显式或遗留技能包配置值。
  onDeleteLuaSkillConfig: (request: LuaSkillConfigDeleteRequest) => Promise<LuaSkillConfigDeleteResult>
  // Polls package configuration events after one optional cursor.
  // 轮询一个可选游标之后的技能包配置事件。
  onPollLuaSkillConfigEvents: (afterSequence?: string | null) => Promise<LuaSkillConfigEventBatch>
}

// PluginSkillSubAreaId identifies one exact plugin-owned detail page.
// PluginSkillSubAreaId 标识一个精确的插件自有详情页。
export type PluginSkillSubAreaId = `plugin:${string}:${string}`

// SkillSubAreaId identifies the selected skill management category.
// SkillSubAreaId 标识当前选中的技能管理分类。
export type SkillSubAreaId = "catalog" | "plugins" | "settings" | "context-engine" | PluginSkillSubAreaId

// PluginDetailMountSelection binds one enabled plugin to one exact detail mount and navigation id.
// PluginDetailMountSelection 将一个已启用插件绑定到一个精确详情挂载点及导航标识。
export interface PluginDetailMountSelection {
  // Exact enabled plugin projection.
  // 精确的已启用插件投影。
  plugin: PluginSummary
  // Exact plugin-owned detail mount.
  // 精确的插件自有详情挂载点。
  mount: NonNullable<PluginSummary["extension"]>["uiMounts"][number]
  // Stable skill subarea id generated from manifest identifiers.
  // 由清单标识生成的稳定技能子区域标识。
  subAreaId: PluginSkillSubAreaId
}

// pluginDetailSubAreaId builds the stable navigation id for one exact plugin mount.
// pluginDetailSubAreaId 为一个精确插件挂载点构建稳定导航标识。
//
// Parameters: pluginId and mountId are validated lowercase manifest identifiers.
// 参数：pluginId 与 mountId 是已经校验的小写清单标识。
// Returns: a collision-free id because validated manifest identifiers cannot contain colons.
// 返回值：由于已校验清单标识不能包含冒号，因此返回无冲突标识。
export function pluginDetailSubAreaId(pluginId: string, mountId: string): PluginSkillSubAreaId {
  return `plugin:${pluginId}:${mountId}`
}

// enabledPluginDetailMounts returns all runtime-ready plugin detail mounts in catalog order.
// enabledPluginDetailMounts 按目录顺序返回全部运行时就绪的插件详情挂载点。
//
// Parameters: plugins is the committed plugin projection.
// 参数：plugins 是已提交的插件投影。
// Returns: exact plugin, mount, and subarea tuples for enabled detail mounts only.
// 返回值：仅包含已启用详情挂载点的精确插件、挂载点与子区域元组。
export function enabledPluginDetailMounts(plugins: PluginSummary[]): PluginDetailMountSelection[] {
  return plugins.flatMap((plugin) => {
    if (
      !plugin.available
      || !plugin.enabled
      || plugin.runtimeState !== "enabled"
      || !plugin.extension
    ) {
      return []
    }
    return plugin.extension.uiMounts
      .filter((mount) => mount.point === "skillsPluginDetail")
      .map((mount) => ({
        mount,
        plugin,
        subAreaId: pluginDetailSubAreaId(plugin.id, mount.id),
      }))
  })
}

// findEnabledPluginDetailMount resolves one selected dynamic subarea against the committed catalog.
// findEnabledPluginDetailMount 根据已提交目录解析一个选中的动态子区域。
//
// Parameters: plugins is the committed projection and subAreaId is the selected page id.
// 参数：plugins 是已提交投影，subAreaId 是选中的页面标识。
// Returns: the one exact enabled selection, or null when the page is no longer available.
// 返回值：返回唯一精确的已启用选择项；页面不再可用时返回 null。
export function findEnabledPluginDetailMount(
  plugins: PluginSummary[],
  subAreaId: SkillSubAreaId,
): PluginDetailMountSelection | null {
  return enabledPluginDetailMounts(plugins).find((selection) => selection.subAreaId === subAreaId) ?? null
}

// isPluginDetailSubArea reports whether one skill subarea is manifest-generated.
// isPluginDetailSubArea 判断一个技能子区域是否由插件清单生成。
//
// Parameters: subAreaId is the selected skill page id.
// 参数：subAreaId 是选中的技能页面标识。
// Returns: true only for the reserved plugin detail prefix.
// 返回值：仅对保留的插件详情前缀返回 true。
export function isPluginDetailSubArea(subAreaId: SkillSubAreaId): subAreaId is PluginSkillSubAreaId {
  return subAreaId.startsWith("plugin:")
}

// localizedPluginMountLabel resolves the exact locale, same-language locale, then default label.
// localizedPluginMountLabel 依次解析精确语言、同语言语言包与默认名称。
//
// Parameters: selection owns the label maps and locale is the active WebUI language.
// 参数：selection 持有名称映射，locale 是当前 WebUI 语言。
// Returns: the effective host navigation label.
// 返回值：返回生效的宿主导航名称。
export function localizedPluginMountLabel(selection: PluginDetailMountSelection, locale: string): string {
  // exactLabel is the mount label for the complete locale identifier.
  // exactLabel 是完整语言标识对应的挂载点名称。
  const exactLabel = selection.mount.labelByLocale[locale]
  if (exactLabel) {
    return exactLabel
  }
  // languagePrefix matches locales such as en-US to the plugin's en locale file.
  // languagePrefix 将 en-US 等语言匹配到插件的 en 语言文件。
  const languagePrefix = `${locale.split("-")[0]}-`
  // languageLabel is the first deterministic same-language localized label.
  // languageLabel 是第一个确定性的同语言本地化名称。
  const languageLabelEntry = Object.entries(selection.mount.labelByLocale)
    .sort(([left], [right]) => left.localeCompare(right))
    .find(([candidate]) => candidate === locale.split("-")[0] || candidate.startsWith(languagePrefix))
  return languageLabelEntry ? languageLabelEntry[1] : selection.mount.label
}

// localizedPluginCapability resolves plugin capability text for the active WebUI language.
// localizedPluginCapability 为当前 WebUI 语言解析插件能力说明。
//
// Parameters: plugin is the exact projection and locale is the active WebUI language.
// 参数：plugin 是精确插件投影，locale 是当前 WebUI 语言。
// Returns: exact locale, same-language locale, or the projected default capability.
// 返回值：返回精确语言、同语言语言包或投影的默认能力说明。
export function localizedPluginCapability(plugin: PluginSummary, locale: string): string {
  // exactCapability is the text for the complete locale identifier.
  // exactCapability 是完整语言标识对应的能力说明。
  const exactCapability = plugin.capabilityByLocale[locale]
  if (exactCapability) {
    return exactCapability
  }
  // languagePrefix matches regional locales to the corresponding plugin language.
  // languagePrefix 将区域语言匹配到对应的插件语言。
  const languagePrefix = `${locale.split("-")[0]}-`
  // languageCapabilityEntry is the deterministic same-language capability entry.
  // languageCapabilityEntry 是确定性的同语言能力说明条目。
  const languageCapabilityEntry = Object.entries(plugin.capabilityByLocale)
    .sort(([left], [right]) => left.localeCompare(right))
    .find(([candidate]) => candidate === locale.split("-")[0] || candidate.startsWith(languagePrefix))
  return languageCapabilityEntry ? languageCapabilityEntry[1] : plugin.capability
}

// hasEnabledPluginDetailMount reports whether one exact enabled plugin contributes the requested integrated mount point.
// hasEnabledPluginDetailMount 判断一个精确的已启用插件是否贡献了请求的集成挂载点。
//
// Parameters: plugins is the committed plugin projection and pluginId is the exact manifest identifier.
// 参数：plugins 是已提交插件投影，pluginId 是精确清单标识。
// Returns: true only when the plugin is available, enabled, runtime-ready, and declares the detail mount.
// 返回值：仅当插件可用、已启用、运行时就绪且声明详情挂载点时返回 true。
export function hasEnabledPluginDetailMount(plugins: PluginSummary[], pluginId: string): boolean {
  return enabledPluginDetailMounts(plugins).some((selection) => selection.plugin.id === pluginId)
}

// SkillHeaderBusyActionId identifies one title-bar skill action currently showing feedback.
// SkillHeaderBusyActionId 标识当前正在标题栏展示反馈的技能动作。
export type SkillHeaderBusyActionId = "lua-refresh"

// SkillHeaderSummary aliases the shared feature header shape for skill pages.
// SkillHeaderSummary 为技能页面复用共享功能标题结构。
export type SkillHeaderSummary = FeatureHeaderSummary
