import { useI18n } from "@/i18n"
import type { WorkbenchSnapshot } from "@/types"
import { DynamicPluginMountPanel } from "@/components/plugins/dynamic-plugin-mount-panel"

import { LuaSkillsPanel, LuaSkillInstallDialog } from "./lua-skills-panel"
import { LuaSkillsSettingsPanel } from "./lua-skills-settings-panel"
import { VulcanPluginsPanel } from "./vulcan-plugins-panel"
import {
  findEnabledPluginDetailMount,
  type SkillActionProps,
  type SkillSubAreaId,
} from "./skill-area-types"

export { LuaSkillInstallDialog }
export type { SkillActionProps, SkillHeaderBusyActionId, SkillHeaderSummary, SkillSubAreaId } from "./skill-area-types"
export { activeSkillHeaderSummary } from "./skill-area-header"

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
  // pluginSelection is the exact enabled dynamic page selected by the host navigation.
  // pluginSelection 是宿主导航选中的精确已启用动态页面。
  const pluginSelection = findEnabledPluginDetailMount(props.workbench.plugins, props.activeTab)
  return (
    <section className="skill-management-surface">
      {props.activeTab === "catalog" ? (
        <LuaSkillsPanel
          actions={props.skillActions}
          skills={props.workbench.luaSkills.skills}
        />
      ) : props.activeTab === "plugins" ? (
        <VulcanPluginsPanel
          actions={props.skillActions}
          plugins={props.workbench.plugins}
        />
      ) : props.activeTab === "settings" ? (
        <LuaSkillsSettingsPanel
          actions={props.skillActions}
          catalog={props.workbench.luaSkills}
        />
      ) : null}
      {pluginSelection ? (
        <div key={pluginSelection.subAreaId}>
          <DynamicPluginMountPanel mount={pluginSelection.mount} plugin={pluginSelection.plugin} />
        </div>
      ) : null}
    </section>
  )
}
