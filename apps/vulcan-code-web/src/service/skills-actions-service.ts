import type {
  AgentSkillsCatalogSummary,
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
    return "显式启用";
  }
  if (registrationState === "implicit") {
    return "隐式启用";
  }
  return "禁用";
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
