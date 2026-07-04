import type {
  AgentSkillsCatalogSummary,
  SubagentDefinitionEnabledUpdate,
  SubagentDefinitionSaveRequest,
  WorkbenchSnapshot,
} from "@/types";
import {
  browserFallbackSnapshot,
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

// refreshLuaSkillsCatalog refreshes the Lua Skills catalog through the local service.
// refreshLuaSkillsCatalog 通过本地服务刷新 Lua Skills 目录。
export async function refreshLuaSkillsCatalog(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("service_refresh_luaskills_catalog");
}

// installLuaSkill installs one Lua Skill package through the local service.
// installLuaSkill 通过本地服务安装单个 Lua Skill 包。
export async function installLuaSkill(skillId: string, source: string, sourceType: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    const normalizedSkillId = skillId.trim() || source.split("/").filter(Boolean).pop() || "preview-skill";
    return updateBrowserFallbackWorkbench((snapshot) => {
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
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_install_luaskill", { skillId: skillId.trim() || null, source, sourceType });
}

// updateLuaSkill updates one managed Lua Skill package.
// updateLuaSkill 更新单个受管 Lua Skill 包。
export async function updateLuaSkill(skillId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.version = skill.version || "0.1.0";
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_update_luaskill", { skillId });
}

// setLuaSkillEnabled toggles one Lua Skill package lifecycle state.
// setLuaSkillEnabled 切换单个 Lua Skill 包生命周期状态。
export async function setLuaSkillEnabled(skillId: string, enabled: boolean): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      if (skill) {
        skill.enabled = enabled;
        skill.disabledReason = enabled ? null : "disabled from browser preview";
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_luaskill_enabled", { skillId, enabled });
}

// uninstallLuaSkill removes one ordinary Lua Skill package.
// uninstallLuaSkill 移除单个普通 Lua Skill 包。
export async function uninstallLuaSkill(skillId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.luaSkills.skills = snapshot.luaSkills.skills.filter((skill) => skill.skillId !== skillId || skill.systemOwned);
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_uninstall_luaskill", { skillId });
}

// setLuaSkillToolRegistrationState persists one Lua Skill tool visibility state.
// setLuaSkillToolRegistrationState 持久化单个 Lua Skill 工具可见性状态。
export async function setLuaSkillToolRegistrationState(skillId: string, localName: string, registrationState: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.luaSkills.skills.find((item) => item.skillId === skillId);
      const tool = skill?.tools.find((item) => item.localName === localName);
      if (tool) {
        tool.registrationState = registrationState as "explicit" | "implicit" | "disabled";
        tool.registrationLabel = luaSkillRegistrationLabel(registrationState);
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_luaskill_tool_registration_state", { skillId, localName, registrationState });
}

// refreshAgentSkillsCatalog refreshes prompt-based Agent Skills through the local service.
// refreshAgentSkillsCatalog 通过本地服务刷新基于提示词的 Agent Skills。
export async function refreshAgentSkillsCatalog(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("service_refresh_agent_skills_catalog");
}

// refreshSubagentCatalog refreshes subagent and expert definitions through the local service.
// refreshSubagentCatalog 通过本地服务刷新子代理与专家定义。
export async function refreshSubagentCatalog(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return browserFallbackSnapshot();
  }
  return serviceClient.request<WorkbenchSnapshot>("service_refresh_subagent_catalog");
}

// openSubagentUserDirectory opens the user-owned agent definition directory.
// openSubagentUserDirectory 打开用户拥有的智能体定义目录。
export async function openSubagentUserDirectory(): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.subagentCatalog.notice = `Browser preview user agent directory: ${snapshot.subagentCatalog.userRoot}`;
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_open_subagent_user_directory");
}

// saveSubagentDefinition persists one editable agent definition.
// saveSubagentDefinition 持久化单个可编辑智能体定义。
export async function saveSubagentDefinition(definition: SubagentDefinitionSaveRequest): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
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
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_subagent_definition", { definition });
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
export async function setSubagentDefinitionEnabled(agentId: string, enabled: boolean): Promise<SubagentDefinitionEnabledUpdate> {
  if (!hasServiceRuntime()) {
    const snapshot = await updateBrowserFallbackWorkbench((snapshot) => {
      const definition = snapshot.subagentCatalog.definitions.find((item) => item.agentId === agentId);
      if (definition) {
        definition.enabled = enabled;
      }
    });
    return {
      agentId,
      enabled,
      notice: snapshot.subagentCatalog.notice,
    };
  }
  return serviceClient.request<SubagentDefinitionEnabledUpdate>("service_set_subagent_definition_enabled", { agentId, enabled });
}

// deleteSubagentDefinition removes one user-owned subagent definition.
// deleteSubagentDefinition 移除单个用户拥有的子代理定义。
export async function deleteSubagentDefinition(agentId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const index = snapshot.subagentCatalog.definitions.findIndex((item) => item.agentId === agentId);
      const definition = index >= 0 ? snapshot.subagentCatalog.definitions[index] : undefined;
      if (!definition || (definition.scopeKey !== "user" && definition.scopeKey !== "package") || definition.overrideOf) {
        throw new Error("只有用户自定义智能体允许删除。");
      }
      snapshot.subagentCatalog.definitions.splice(index, 1);
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_delete_subagent_definition", { agentId });
}

// restoreSubagentDefinitionDefault removes one user override for a system-backed definition.
// restoreSubagentDefinitionDefault 移除单个系统集成定义的用户覆盖。
export async function restoreSubagentDefinitionDefault(agentId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      const definition = snapshot.subagentCatalog.definitions.find((item) => item.agentId === agentId);
      if (definition?.canRestoreDefault || definition?.overrideOf) {
        definition.scope = "System";
        definition.scopeKey = "system";
        definition.overrideOf = null;
        definition.canRestoreDefault = false;
      }
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_restore_subagent_definition_default", { agentId });
}

// setAgentSkillsEnabled persists the global Agent Skills switch.
// setAgentSkillsEnabled 持久化 Agent Skills 全局开关。
export async function setAgentSkillsEnabled(enabled: boolean): Promise<AgentSkillsCatalogSummary> {
  if (!hasServiceRuntime()) {
    const snapshot = updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.agentSkills.globalEnabled = enabled;
      snapshot.agentSkills.skills = snapshot.agentSkills.skills.map((skill) => ({ ...skill, enabled }));
    });
    return snapshot.agentSkills;
  }
  return serviceClient.request<AgentSkillsCatalogSummary>("service_set_agent_skills_enabled", { enabled });
}

// setAgentSkillEnabled persists one global Agent Skill state.
// setAgentSkillEnabled 持久化单个全局 Agent Skill 状态。
export async function setAgentSkillEnabled(name: string, enabled: boolean): Promise<AgentSkillsCatalogSummary> {
  if (!hasServiceRuntime()) {
    const snapshot = updateBrowserFallbackWorkbench((snapshot) => {
      const skill = snapshot.agentSkills.skills.find((item) => item.name === name);
      if (skill) {
        skill.enabled = enabled && snapshot.agentSkills.globalEnabled;
      }
    });
    return snapshot.agentSkills;
  }
  return serviceClient.request<AgentSkillsCatalogSummary>("service_set_agent_skill_enabled", { name, enabled });
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
