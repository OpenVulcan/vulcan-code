import { formatSkillText } from "@/components/features/featureText"
import { useI18n } from "@/i18n"
import type { WorkbenchSnapshot } from "@/types"

import { AgentSkillsPanel } from "./agent-skills-panel"
import { LuaSkillsPanel, LuaSkillInstallDialog } from "./lua-skills-panel"
import type { SkillActionProps, SkillHeaderSummary, SkillSubAreaId } from "./skill-area-types"

export { LuaSkillInstallDialog }
export type { SkillActionProps, SkillHeaderBusyActionId, SkillHeaderSummary, SkillSubAreaId } from "./skill-area-types"

// activeSkillHeaderSummary returns the title and metrics for the current skill sub page.
// activeSkillHeaderSummary 返回当前技能子页面的标题与统计。
export function activeSkillHeaderSummary(activeTab: SkillSubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): SkillHeaderSummary {
  if (activeTab === "agent") {
    const enabledCount = workbench.agentSkills.skills.filter((skill) => skill.enabled).length
    return {
      title: "Agent Skills",
      detail: formatSkillText(t("skills.header.agentDetail"), {
        total: workbench.agentSkills.skills.length,
        enabled: enabledCount,
        warnings: workbench.agentSkills.errors.length,
      }),
    }
  }
  const visibleToolCount = workbench.luaSkills.skills.reduce((count, skill) => count + skill.entryCount, 0)
  const implicitToolCount = workbench.luaSkills.skills.reduce((count, skill) => (
    count + skill.tools.filter((tool) => tool.registrationState === "implicit").length
  ), 0)
  return {
    title: "Lua Skills",
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
  // Workbench snapshot containing Lua Skills and Agent Skills catalogs.
  // 包含 Lua Skills 与 Agent Skills 目录的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  return (
    <section className="skill-management-surface">
      {props.activeTab === "lua" ? (
        <LuaSkillsPanel
          actions={props.skillActions}
          skills={props.workbench.luaSkills.skills}
        />
      ) : (
        <AgentSkillsPanel
          actions={props.skillActions}
          errors={props.workbench.agentSkills.errors}
          skills={props.workbench.agentSkills.skills}
        />
      )}
    </section>
  )
}
