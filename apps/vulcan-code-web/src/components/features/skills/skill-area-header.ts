import { formatSkillText } from "@/components/features/featureText"
import type { useI18n } from "@/i18n"
import type { WorkbenchSnapshot } from "@/types"
import {
  findEnabledPluginDetailMount,
  localizedPluginCapability,
  localizedPluginMountLabel,
  type SkillHeaderSummary,
  type SkillSubAreaId,
} from "./skill-area-types"

// activeSkillHeaderSummary returns the selected skill-page summary without loading its editors.
// activeSkillHeaderSummary 在不加载技能编辑器的情况下返回当前技能页摘要。
export function activeSkillHeaderSummary(
  activeTab: SkillSubAreaId,
  workbench: WorkbenchSnapshot,
  locale: string,
  t: ReturnType<typeof useI18n>["t"],
): SkillHeaderSummary {
  if (activeTab === "settings") {
    // The header carries the page description, led by the pending-restart state when saved settings await a restart.
    // 头部承载页面说明；已保存设置等待重启时，以待重启状态开头。
    const description = t("skills.luaSettings.description")
    return {
      title: t("skills.navigation.luaSettings"),
      detail: workbench.luaSkills.restartRequired ? `${t("skills.luaSettings.restartRequired")} · ${description}` : description,
    }
  }
  if (activeTab === "mcp-servers") {
    return { title: t("settings.mcpServers"), detail: t("settings.mcpServers.description") }
  }
  if (activeTab === "knowledge") {
    return { title: t("area.knowledge"), detail: t("area.knowledge.description") }
  }
  if (activeTab === "plugin-maintenance") {
    return { title: t("skills.navigation.pluginMaintenance"), detail: t("pluginMaintenance.description") }
  }
  if (activeTab === "plugins") {
    // vulcanPlugins excludes ordinary Lua Skill projection rows from the plugin menu page.
    // vulcanPlugins 从插件菜单页面排除普通 Lua Skill 投影行。
    const vulcanPlugins = workbench.plugins.filter((plugin) => plugin.managementKind === "vulcanPlugin")
    return {
      title: t("skills.navigation.plugins"),
      detail: formatSkillText(t("skills.header.pluginDetail"), {
        total: vulcanPlugins.length,
        enabled: vulcanPlugins.filter((plugin) => plugin.enabled && plugin.available).length,
      }),
    }
  }
  // pluginSelection resolves a manifest-owned detail page without plugin-specific branches.
  // pluginSelection 解析清单拥有的详情页，不包含插件专用分支。
  const pluginSelection = findEnabledPluginDetailMount(workbench.plugins, activeTab)
  if (pluginSelection) {
    return {
      title: localizedPluginMountLabel(pluginSelection, locale),
      detail: localizedPluginCapability(pluginSelection.plugin, locale),
    }
  }
  // visibleToolCount is the exact declared entry count across the LuaSkills catalog.
  // visibleToolCount 是 LuaSkills 目录中精确的已声明入口总数。
  const visibleToolCount = workbench.luaSkills.skills.reduce((count, skill) => count + skill.entryCount, 0)
  // implicitToolCount is the exact implicit registration count.
  // implicitToolCount 是精确的隐式注册数量。
  const implicitToolCount = workbench.luaSkills.skills.reduce((count, skill) => count + skill.tools.filter((tool) => tool.registrationState === "implicit").length, 0)
  return {
    title: t("skills.navigation.list"),
    detail: formatSkillText(t("skills.header.luaDetail"), {
      total: workbench.luaSkills.skills.length,
      visible: visibleToolCount,
      implicit: implicitToolCount,
    }),
  }
}
