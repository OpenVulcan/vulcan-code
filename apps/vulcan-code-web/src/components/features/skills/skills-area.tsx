import { useI18n } from "@/i18n"
import type { WorkbenchSnapshot } from "@/types"
import { DynamicPluginMountPanel } from "@/components/plugins/dynamic-plugin-mount-panel"
import { ContextEngineArea } from "@/components/features/context-engine/context-engine-area"
import { KnowledgeBaseArea } from "@/components/features/knowledge/knowledge-base-area"
import { McpSettingsArea } from "@/components/features/settings/mcp-settings-area"

import { LuaSkillsPanel, LuaSkillInstallDialog } from "./lua-skills-panel"
import { LuaSkillsSettingsPanel } from "./lua-skills-settings-panel"
import { VulcanPluginsPanel } from "./vulcan-plugins-panel"
import { PluginPrewarmControls } from "./plugin-prewarm-controls"
import { PluginPrewarmPolicyControls } from "./plugin-prewarm-policy-controls"
import {
  findEnabledPluginDetailMount,
  isPluginDetailSubArea,
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
  // Navigates to the application-wide runtime diagnostics page when the host provides one.
  // 宿主提供时导航至应用全局运行时诊断页面。
  onOpenDiagnostics?: () => void
  // onSelectPluginResourceOwner navigates the original resource owner through workspace service selection.
  // onSelectPluginResourceOwner 通过工作台服务选择导航至原资源所有者。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>
}) {
  // t provides a visible unavailable state when the selected plugin loses its enabled mount.
  // t 在所选插件失去已启用挂载点时提供可见的不可用状态。
  const { t } = useI18n()
  // pluginSelection is the exact enabled dynamic page selected by the host navigation.
  // pluginSelection 是宿主导航选中的精确已启用动态页面。
  const pluginSelection = findEnabledPluginDetailMount(props.workbench.plugins, props.activeTab)
  return (
    <section className="skill-management-surface">
      {props.activeTab === "catalog" ? (
        <LuaSkillsPanel
          actions={props.skillActions}
          operationProgress={props.workbench.luaSkills.operationProgress ?? null}
          skills={props.workbench.luaSkills.skills}
        />
      ) : props.activeTab === "plugins" ? (
        <>
          <VulcanPluginsPanel
            actions={props.skillActions}
            plugins={props.workbench.plugins}
            conversations={props.workbench.conversations}
            onSelectPluginResourceOwner={props.onSelectPluginResourceOwner}
          />
          <PluginPrewarmControls conversations={props.workbench.conversations} selectedConversationId={props.workbench.selectedConversationId} />
          <PluginPrewarmPolicyControls />
        </>
      ) : props.activeTab === "settings" ? (
        <LuaSkillsSettingsPanel
          actions={props.skillActions}
          catalog={props.workbench.luaSkills}
        />
      ) : props.activeTab === "mcp-servers" ? (
        <McpSettingsArea actions={props.skillActions} servers={props.workbench.mcpServers ?? []} onOpenDiagnostics={props.onOpenDiagnostics} />
      ) : props.activeTab === "context-engine" ? (
        <ContextEngineArea workbenchRevision={props.workbench.revision} />
      ) : props.activeTab === "knowledge" ? (
        <KnowledgeBaseArea />
      ) : null}
      {pluginSelection ? (
        <div key={pluginSelection.subAreaId}>
          <DynamicPluginMountPanel mount={pluginSelection.mount} plugin={pluginSelection.plugin} />
        </div>
      ) : null}
      {!pluginSelection && isPluginDetailSubArea(props.activeTab) ? <div className="skill-empty-state" role="status">
        {t("skills.thisPluginPageIs")}
      </div> : null}
    </section>
  )
}
// Directory-only rules follow this lazy owner; shared workbench and responsive rules remain global.
// 目录专有规则随此懒加载归属载入；共用工作台与响应式规则保持全局。
import "@/styles/skill-directory.css";
