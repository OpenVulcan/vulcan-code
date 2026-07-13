import { formatSkillText } from "@/components/features/featureText"
import { useI18n } from "@/i18n"
import type { WorkbenchSnapshot } from "@/types"

import { LuaSkillsPanel, LuaSkillInstallDialog } from "./lua-skills-panel"
import { LuaSkillsSettingsPanel } from "./lua-skills-settings-panel"
import { SystemPluginsPanel } from "./system-plugins-panel"
import type { SkillActionProps, SkillHeaderSummary, SkillSubAreaId } from "./skill-area-types"

export { LuaSkillInstallDialog }
export type { SkillActionProps, SkillHeaderBusyActionId, SkillHeaderSummary, SkillSubAreaId } from "./skill-area-types"

// activeSkillHeaderSummary returns the title and metrics for the current skill sub page.
// activeSkillHeaderSummary 返回当前技能子页面的标题与统计。
export function activeSkillHeaderSummary(activeTab: SkillSubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): SkillHeaderSummary {
  if (activeTab === "settings") {
    return {
      title: t("skills.navigation.luaSettings"),
      detail: workbench.luaSkills.notice,
    }
  }
  if (activeTab === "plugins") {
    // systemPlugins excludes ordinary Lua Skill projection rows from the plugin menu page.
    // systemPlugins 从插件菜单页面排除普通 Lua Skill 投影行。
    const systemPlugins = workbench.plugins.filter((plugin) => plugin.managementKind === "systemPlugin")
    return {
      title: t("skills.navigation.plugins"),
      detail: formatSkillText(t("skills.header.pluginDetail"), {
        total: systemPlugins.length,
        enabled: systemPlugins.filter((plugin) => plugin.enabled && plugin.available).length,
      }),
    }
  }
  const visibleToolCount = workbench.luaSkills.skills.reduce((count, skill) => count + skill.entryCount, 0)
  const implicitToolCount = workbench.luaSkills.skills.reduce((count, skill) => (
    count + skill.tools.filter((tool) => tool.registrationState === "implicit").length
  ), 0)
  return {
    title: t("skills.navigation.list"),
    detail: formatSkillText(t("skills.header.luaDetail"), {
      total: workbench.luaSkills.skills.length,
      visible: visibleToolCount,
      implicit: implicitToolCount,
    }),
  }
}

// SkillsArea renders the selected skill management page.
// SkillsArea 渲染当前选中的技能管理页面。
export function SkillsArea(props: {
  // Selected skill tab.
  // 当前选中的技能页签。
  activeTab: SkillSubAreaId
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps
  // Workbench snapshot containing the LuaSkills catalog and runtime settings.
  // 包含 LuaSkills 目录与运行时设置的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  return (
    <section className="skill-management-surface">
      {props.activeTab === "catalog" ? (
        <LuaSkillsPanel
          actions={props.skillActions}
          skills={props.workbench.luaSkills.skills}
        />
      ) : props.activeTab === "plugins" ? (
        <SystemPluginsPanel
          actions={props.skillActions}
          plugins={props.workbench.plugins}
        />
      ) : props.activeTab === "settings" ? (
        <LuaSkillsSettingsPanel
          actions={props.skillActions}
          catalog={props.workbench.luaSkills}
        />
      ) : null}
    </section>
  )
}
