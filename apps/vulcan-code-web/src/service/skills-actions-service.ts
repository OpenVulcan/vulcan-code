import type {
  AgentSkillsCatalogUpdate,
  AgentSkillCategorySaveRequest,
  AgentSkillSaveRequest,
  LuaSkillsCatalogUpdate,
  SubagentCatalogUpdate,
  SubagentDefinitionSaveRequest,
  WorkbenchSnapshot,
} from "@/types";
import {
  updateBrowserFallbackWorkbench,
} from "@/service/browser-fallback-workbench";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// AgentSkillProjectState describes one project-level prompt skill override state.
// AgentSkillProjectState 描述单个项目级提示词技能覆盖状态。
export type AgentSkillProjectState = "inherit" | "enabled" | "disabled";

// luaSkillRegistrationLabel returns the preview label for one LuaSkill tool registration state.
// luaSkillRegistrationLabel 返回单个 LuaSkill 工具注册状态的预览标签。
function luaSkillRegistrationLabel(registrationState: string): string {
  if (registrationState === "explicit") {
    return "Explicit";
  }
  if (registrationState === "implicit") {
    return "Implicit";
  }
  return "Disabled";
}

// browserPreviewAgentDefinitionPath builds the user definition path shown by browser fallback data.
// browserPreviewAgentDefinitionPath 构建浏览器预览数据展示的用户定义路径。
function browserPreviewAgentDefinitionPath(userRoot: string, agentId: string, extension: "md" | "yaml"): string {
  return `${userRoot}/definitions/${agentId}.${extension}`;
}

// luaSkillsCatalogUpdateFromSnapshot projects one browser-preview workbench into the LuaSkills domain contract.
// luaSkillsCatalogUpdateFromSnapshot 将单个浏览器预览工作台投影为 LuaSkills 领域契约。
//
// Parameters: snapshot is the committed browser-preview workbench snapshot.
// 参数：snapshot 是已提交的浏览器预览工作台快照。
// Returns: the revisioned LuaSkills catalog and its derived plugin rows.
// 返回值：带版本的 LuaSkills 目录及其派生插件行。
function luaSkillsCatalogUpdateFromSnapshot(snapshot: WorkbenchSnapshot): LuaSkillsCatalogUpdate {
  return {
    revision: snapshot.revision ?? 0,
    luaSkills: snapshot.luaSkills,
    plugins: snapshot.plugins,
  };
}

// agentSkillsCatalogUpdateFromSnapshot projects one browser-preview workbench into the Agent Skills domain contract.
// agentSkillsCatalogUpdateFromSnapshot 将单个浏览器预览工作台投影为 Agent Skills 领域契约。
//
// Parameters: snapshot is the committed browser-preview workbench snapshot.
// 参数：snapshot 是已提交的浏览器预览工作台快照。
// Returns: the revisioned complete Agent Skills catalog.
// 返回值：带版本的完整 Agent Skills 目录。
function agentSkillsCatalogUpdateFromSnapshot(snapshot: WorkbenchSnapshot): AgentSkillsCatalogUpdate {
  return {
    revision: snapshot.revision ?? 0,
    agentSkills: snapshot.agentSkills,
  };
}

// subagentCatalogUpdateFromSnapshot projects one browser-preview workbench into the agent-catalog domain contract.
// subagentCatalogUpdateFromSnapshot 将单个浏览器预览工作台投影为智能体目录领域契约。
//
// Parameters: snapshot is the committed browser-preview workbench snapshot.
// 参数：snapshot 是已提交的浏览器预览工作台快照。
// Returns: the revisioned complete agent catalog.
// 返回值：带版本的完整智能体目录。
function subagentCatalogUpdateFromSnapshot(snapshot: WorkbenchSnapshot): SubagentCatalogUpdate {
  return {
    revision: snapshot.revision ?? 0,
    subagentCatalog: snapshot.subagentCatalog,
  };
}

// refreshLuaSkillsCatalog refreshes the Lua Skills catalog through the local service.
// refreshLuaSkillsCatalog 通过本地服务刷新 Lua Skills 目录。
export async function refreshLuaSkillsCatalog(): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_refresh_luaskills_catalog");
}

// installLuaSkill installs one Lua Skill package through the local service.
// installLuaSkill 通过本地服务安装单个 Lua Skill 包。
export async function installLuaSkill(skillId: string, source: string, sourceType: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    const normalizedSkillId = skillId.trim() || source.split("/").filter(Boolean).pop() || "preview-skill";
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      if (snapshot.luaSkills.skills.some((skill) => skill.skillId === normalizedSkillId)) {
        return;
      }
      snapshot.luaSkills.skills.push({
        skillId: normalizedSkillId,
        displayName: normalizedSkillId,
        version: "0.1.0",
        rootName: "USER",
        skillDir: `C:/Users/20000/.vulcan/code/luaskills-user/skills/${normalizedSkillId}`,
        enabled: true,
        managed: true,
        sourceType,
        sourceLocator: source,
        entryCount: 0,
        systemOwned: false,
        tools: [],
      });
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_install_luaskill", { skillId: skillId.trim() || null, source, sourceType });
}

// updateLuaSkill updates one managed Lua Skill package.
// updateLuaSkill 更新单个受管 Lua Skill 包。
export async function updateLuaSkill(skillId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.version = skill.version || "0.1.0";
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_update_luaskill", { skillId });
}

// setLuaSkillEnabled toggles one Lua Skill package lifecycle state.
// setLuaSkillEnabled 切换单个 Lua Skill 包生命周期状态。
export async function setLuaSkillEnabled(skillId: string, enabled: boolean): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.enabled = enabled;
        skill.disabledReason = enabled ? null : "disabled from browser preview";
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_set_luaskill_enabled", { skillId, enabled });
}

// setLuaSkillsSystemPluginEnabled toggles one host-owned LuaSkills System Plugin.
// setLuaSkillsSystemPluginEnabled 切换单个宿主自有 LuaSkills System Plugin。
export async function setLuaSkillsSystemPluginEnabled(pluginId: string, enabled: boolean): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      // plugin resolves only a host-owned System Plugin row in browser preview mode.
      // plugin 在浏览器预览模式中仅解析宿主自有 System Plugin 行。
      const plugin = snapshot.plugins.find((item) => item.id === pluginId && item.managementKind === "systemPlugin");
      if (plugin) {
        plugin.enabled = enabled;
        plugin.status = enabled ? "已启用" : "已停用";
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_set_luaskills_system_plugin_enabled", { pluginId, enabled });
}

// uninstallLuaSkill removes one ordinary Lua Skill package.
// uninstallLuaSkill 移除单个普通 Lua Skill 包。
export async function uninstallLuaSkill(skillId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.luaSkills.skills = snapshot.luaSkills.skills.filter((skill) => skill.skillId !== skillId || skill.systemOwned);
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_uninstall_luaskill", { skillId });
}

// setLuaSkillToolRegistrationState persists one Lua Skill tool visibility state.
// setLuaSkillToolRegistrationState 持久化单个 Lua Skill 工具可见性状态。
export async function setLuaSkillToolRegistrationState(skillId: string, localName: string, registrationState: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return luaSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      const tool = skill?.tools.find((item) => item.localName === localName);
      if (tool) {
        tool.registrationState = registrationState as "explicit" | "implicit" | "disabled";
        tool.registrationLabel = luaSkillRegistrationLabel(registrationState);
      }
    }));
  }
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_set_luaskill_tool_registration_state", { skillId, localName, registrationState });
}

// refreshAgentSkillsCatalog refreshes prompt-based Agent Skills through the local service.
// refreshAgentSkillsCatalog 通过本地服务刷新基于提示词的 Agent Skills。
export async function refreshAgentSkillsCatalog(): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return agentSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_refresh_agent_skills_catalog");
}

// openAgentSkillsUserDirectory opens the configured user package root.
// openAgentSkillsUserDirectory 打开配置后的用户技能包根目录。
export async function openAgentSkillsUserDirectory(): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return agentSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.agentSkills.notice = snapshot.uiLocale?.startsWith("zh") !== false
        ? `浏览器预览用户技能目录：${snapshot.agentSkills.userRoot}`
        : `Browser preview user skill directory: ${snapshot.agentSkills.userRoot}`
    }))
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_open_agent_skills_user_directory")
}

// saveAgentSkill saves one complete strict user Agent Skill package.
// saveAgentSkill 保存一个完整严格用户 Agent Skill 技能包。
export async function saveAgentSkill(skill: AgentSkillSaveRequest): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill package editing requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_save_agent_skill", { skill })
}

// deleteAgentSkill deletes one exact user-owned Agent Skill package.
// deleteAgentSkill 删除一个精确用户 Agent Skill 技能包。
export async function deleteAgentSkill(name: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill package deletion requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_delete_agent_skill", { name })
}

// restoreAgentSkillDefault removes one user override and restores its system source.
// restoreAgentSkillDefault 移除一个用户覆盖并恢复其系统来源。
export async function restoreAgentSkillDefault(name: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill restoration requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_restore_agent_skill_default", { name })
}

// saveAgentSkillCategory saves one complete user-defined logical category.
// saveAgentSkillCategory 保存一个完整用户定义逻辑分类。
export async function saveAgentSkillCategory(category: AgentSkillCategorySaveRequest): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill category editing requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_save_agent_skill_category", { category })
}

// deleteAgentSkillCategory deletes one unused user-defined logical category.
// deleteAgentSkillCategory 删除一个未使用的用户定义逻辑分类。
export async function deleteAgentSkillCategory(categoryId: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill category deletion requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_delete_agent_skill_category", { categoryId })
}

// setAgentSkillCategory assigns one effective skill to one exact logical category.
// setAgentSkillCategory 将一个有效技能归入一个精确逻辑分类。
export async function setAgentSkillCategory(name: string, categoryId: string): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    throw new Error("Agent Skill category assignment requires the desktop service runtime.")
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_set_agent_skill_category", { name, categoryId })
}

// refreshSubagentCatalog refreshes subagent and expert definitions through the local service.
// refreshSubagentCatalog 通过本地服务刷新子代理与专家定义。
export async function refreshSubagentCatalog(): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench(() => undefined));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_refresh_subagent_catalog");
}

// openSubagentUserDirectory opens the user-owned agent definition directory.
// openSubagentUserDirectory 打开用户拥有的智能体定义目录。
export async function openSubagentUserDirectory(): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.subagentCatalog.notice = `Browser preview user agent directory: ${snapshot.subagentCatalog.userRoot}`;
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_open_subagent_user_directory");
}

// saveSubagentDefinition persists one editable agent definition.
// saveSubagentDefinition 持久化单个可编辑智能体定义。
export async function saveSubagentDefinition(definition: SubagentDefinitionSaveRequest): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const targetAgentId = browserPreviewTargetAgentId(definition);
      const originalAgentId = definition.originalAgentId?.trim() || definition.agentId;
      const existingIndex = snapshot.subagentCatalog.definitions.findIndex((item) => item.agentId === originalAgentId);
      const existingDefinition = existingIndex >= 0 ? snapshot.subagentCatalog.definitions[existingIndex] : undefined;
      if (existingDefinition?.scopeKey === "system" || existingDefinition?.overrideOf) {
        throw new Error("系统内置智能体不允许编辑；只能启用或关闭。");
      }
      if (existingIndex < 0 && snapshot.subagentCatalog.definitions.some((item) => item.agentId === targetAgentId)) {
        throw new Error(`目标智能体 ${targetAgentId} 已存在。`);
      }
      const locales = definition.locales ?? {};
      const nextDefinition = {
        agentId: targetAgentId,
        name: preferredBrowserPreviewLocaleField(locales, "name") ?? (definition.name.trim() || targetAgentId),
        description: preferredBrowserPreviewLocaleField(locales, "description") ?? definition.description ?? null,
        locales,
        emoji: browserPreviewDefaultedTextPatch(definition.emoji, existingDefinition?.emoji, "🤖"),
        color: browserPreviewDefaultedTextPatch(definition.color, existingDefinition?.color, "gray"),
        vibe: browserPreviewOptionalTextPatch(definition.vibe, existingDefinition?.vibe ?? null),
        sourceKey: `user:${targetAgentId}`,
        categoryId: targetAgentId.includes("/") ? targetAgentId.split("/").slice(0, -1).join("/") : null,
        packageId: definition.packageId?.trim() || null,
        scope: "User",
        scopeKey: "user",
        enabled: definition.enabled,
        definitionFile: browserPreviewAgentDefinitionPath(snapshot.subagentCatalog.userRoot, targetAgentId, "md"),
        sidecarFile: browserPreviewAgentDefinitionPath(snapshot.subagentCatalog.userRoot, targetAgentId, "yaml"),
        remoteUrl: null,
        overrideOf: null,
        canRestoreDefault: false,
        sourceHash: null,
        version: null,
        updatedAt: Date.now().toString(),
        toolPolicyMode: definition.toolPolicyMode ?? "all",
        toolPolicyTools: definition.toolPolicyTools ?? [],
        allowedToolCount: definition.toolPolicyMode === "allow_only" ? (definition.toolPolicyTools?.length ?? 0) : definition.allowedTools.length,
        disabledToolCount: definition.toolPolicyMode === "deny_only" ? (definition.toolPolicyTools?.length ?? 0) : definition.disabledTools.length,
        enabledPromptSkillCount: definition.promptSkillPolicyMode === "allow_only" ? (definition.promptSkillPolicySkills?.length ?? 0) : definition.enabledPromptSkills.length,
        disabledPromptSkillCount: definition.promptSkillPolicyMode === "deny_only" ? (definition.promptSkillPolicySkills?.length ?? 0) : definition.disabledPromptSkills.length,
        promptSkillPolicyMode: definition.promptSkillPolicyMode ?? "all",
        promptSkillPolicySkills: definition.promptSkillPolicySkills ?? [],
        providerModel: definition.providerModel ?? {},
        systemPrompt: definition.systemPrompt ?? null,
        systemPromptAppend: definition.systemPromptAppend ?? null,
        allowedTools: definition.allowedTools,
        disabledTools: definition.disabledTools,
        allowedAiCapabilities: [],
        disabledAiCapabilities: [],
        luaskillsOverrides: definition.luaskillsOverrides ?? null,
        mcpServerOverrides: definition.mcpServerOverrides ?? null,
        enabledPromptSkills: definition.enabledPromptSkills,
        disabledPromptSkills: definition.disabledPromptSkills,
        capabilityDiagnostics: ["Browser preview saved this definition in the user agent layer."],
      };
      if (existingIndex >= 0) {
        snapshot.subagentCatalog.definitions[existingIndex] = nextDefinition;
      } else {
        snapshot.subagentCatalog.definitions.push(nextDefinition);
      }
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_save_subagent_definition", { definition });
}

// browserPreviewTargetAgentId applies package assignment in browser fallback mode.
// browserPreviewTargetAgentId 在浏览器预览模式下应用包分配。
function browserPreviewTargetAgentId(definition: SubagentDefinitionSaveRequest): string {
  const agentId = definition.agentId.trim();
  const packageId = definition.packageId?.trim();
  if (!packageId) {
    return agentId;
  }
  const prefix = `${packageId}/`;
  return agentId.startsWith(prefix) ? agentId : `${packageId}/${agentId}`;
}

// browserPreviewDefaultedTextPatch applies the service tri-state text patch contract to a defaulted field.
// browserPreviewDefaultedTextPatch 将服务端三态文本补丁契约应用到带默认值字段。
function browserPreviewDefaultedTextPatch(value: string | null | undefined, previousValue: string | null | undefined, defaultValue: string): string {
  if (value === undefined) {
    return previousValue?.trim() || defaultValue;
  }
  if (value === null) {
    return defaultValue;
  }
  return value.trim() || defaultValue;
}

// browserPreviewOptionalTextPatch applies the service tri-state text patch contract to an optional field.
// browserPreviewOptionalTextPatch 将服务端三态文本补丁契约应用到可选字段。
function browserPreviewOptionalTextPatch(value: string | null | undefined, previousValue: string | null | undefined): string | null {
  if (value === undefined) {
    return previousValue?.trim() || null;
  }
  if (value === null) {
    return null;
  }
  return value.trim() || null;
}

// preferredBrowserPreviewLocaleField chooses English metadata first and Chinese as fallback.
// preferredBrowserPreviewLocaleField 选择英文优先、中文兜底的元数据字段。
function preferredBrowserPreviewLocaleField(
  locales: Record<string, { name?: string | null; description?: string | null }>,
  field: "name" | "description",
): string | null {
  return locales["en-US"]?.[field]?.trim()
    || locales.en?.[field]?.trim()
    || locales["zh-CN"]?.[field]?.trim()
    || locales.zh?.[field]?.trim()
    || null;
}

// setSubagentDefinitionEnabled persists one subagent definition enabled state.
// setSubagentDefinitionEnabled 持久化单个子代理定义启停状态。
export async function setSubagentDefinitionEnabled(agentId: string, enabled: boolean): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const definition = snapshot.subagentCatalog.definitions.find((item) => item.agentId === agentId);
      if (definition) {
        definition.enabled = enabled;
      }
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_set_subagent_definition_enabled", { agentId, enabled });
}

// deleteSubagentDefinition removes one user-owned subagent definition.
// deleteSubagentDefinition 移除单个用户拥有的子代理定义。
export async function deleteSubagentDefinition(agentId: string): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const index = snapshot.subagentCatalog.definitions.findIndex((item) => item.agentId === agentId);
      const definition = index >= 0 ? snapshot.subagentCatalog.definitions[index] : undefined;
      if (!definition || (definition.scopeKey !== "user" && definition.scopeKey !== "package") || definition.overrideOf) {
        throw new Error("只有用户自定义智能体允许删除。");
      }
      snapshot.subagentCatalog.definitions.splice(index, 1);
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_delete_subagent_definition", { agentId });
}

// restoreSubagentDefinitionDefault removes one user override for a system-backed definition.
// restoreSubagentDefinitionDefault 移除单个系统集成定义的用户覆盖。
export async function restoreSubagentDefinitionDefault(agentId: string): Promise<SubagentCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return subagentCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
      const definition = snapshot.subagentCatalog.definitions.find((item) => item.agentId === agentId);
      if (definition?.canRestoreDefault || definition?.overrideOf) {
        definition.scope = "System";
        definition.scopeKey = "system";
        definition.overrideOf = null;
        definition.canRestoreDefault = false;
      }
    }));
  }
  return serviceClient.request<SubagentCatalogUpdate>("service_restore_subagent_definition_default", { agentId });
}

// setAgentSkillEnabled persists one Agent Skill state.
// setAgentSkillEnabled 持久化单个 Agent Skill 状态。
export async function setAgentSkillEnabled(name: string, enabled: boolean): Promise<AgentSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) {
    return agentSkillsCatalogUpdateFromSnapshot(updateBrowserFallbackWorkbench((snapshot) => {
    const skill = snapshot.agentSkills.records.find((item) => item.name === name && item.effective);
      if (skill) {
        skill.enabled = enabled;
      }
    }));
  }
  return serviceClient.request<AgentSkillsCatalogUpdate>("service_set_agent_skill_enabled", { name, enabled });
}

// loadProjectAgentSkillOverrides loads project-local Agent Skill overrides.
// loadProjectAgentSkillOverrides 加载项目本地 Agent Skill 覆盖。
export async function loadProjectAgentSkillOverrides(projectId: string): Promise<Record<string, string>> {
  if (!hasServiceRuntime()) {
    return {};
  }
  return serviceClient.request<Record<string, string>>("service_load_project_agent_skill_overrides", { projectId });
}

// setAgentSkillProjectState persists one project-level Agent Skill tri-state override.
// setAgentSkillProjectState 持久化单个项目级 Agent Skill 三态覆盖。
export async function setAgentSkillProjectState(projectId: string, name: string, stateKey: AgentSkillProjectState): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  return serviceClient.request<void>("service_set_agent_skill_project_state", { projectId, name, stateKey });
}
